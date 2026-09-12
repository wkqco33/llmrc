#!/usr/bin/env bash
#
# Publishes every workspace crate in this repository to crates.io.
#
# The publish order is fixed, because `cargo publish` resolves the path
# dependencies of a packaged crate against the crates.io index: a crate can only
# be published once everything it depends on is already there.
#
# The script is idempotent and rate-limit aware:
#   * crates whose version is already in the sparse index are skipped, so a
#     partially completed release is resumed by re-running it;
#   * crates.io publish rate limits (a burst of 5 new crates, then 1 every 10
#     minutes) are honored by waiting for the time crates.io reports.
#
# Usage:
#   DRY_RUN=1 scripts/publish-crates.sh   # validate packaging, upload nothing
#   scripts/publish-crates.sh             # publish (needs CARGO_REGISTRY_TOKEN)
#
# Environment:
#   CARGO_REGISTRY_TOKEN  crates.io API token (required unless DRY_RUN=1)
#   DRY_RUN               set to 1 to package/verify without uploading
#   MAX_RATE_LIMIT_WAITS  max rate-limit waits per crate (default 24)
#   INDEX_POLL_SECONDS    seconds between sparse-index polls (default 10)
#   INDEX_MAX_POLLS       max polls before giving up (default 60)
#
# Note: this script runs under `set -o pipefail` and therefore avoids pipelines
# that can terminate a writer early (for example `grep -q` on a `curl` body).

set -euo pipefail

# Dependencies before dependents.
CRATES=(
  llmrc-core
  llmrc-runtime
  llmrc-agent
  llmrc-openai
  llmrc-ollama
  llmrc-mcp
  llmrc-bots-core
  llmrc-bots-discord
  llmrc-bots-telegram
  llmrc-bots-slack
  llmrc
)

DRY_RUN="${DRY_RUN:-0}"
MAX_RATE_LIMIT_WAITS="${MAX_RATE_LIMIT_WAITS:-24}"
INDEX_POLL_SECONDS="${INDEX_POLL_SECONDS:-10}"
INDEX_MAX_POLLS="${INDEX_MAX_POLLS:-60}"
FALLBACK_RATE_LIMIT_WAIT="${FALLBACK_RATE_LIMIT_WAIT:-600}"
USER_AGENT="llmrc-release (+https://github.com/wkqco33/llmrc)"

cd "$(dirname "${BASH_SOURCE[0]}")/.."

log() { printf '%s\n' "$*"; }
fail() {
  printf '::error::%s\n' "$*" >&2
  exit 1
}

command -v jq >/dev/null 2>&1 || fail "jq is required"
command -v curl >/dev/null 2>&1 || fail "curl is required"

META="$(mktemp)"
trap 'rm -f "$META"' EXIT
cargo metadata --no-deps --format-version 1 --locked >"$META"

WORKSPACE_VERSION="$(jq -r '.packages[] | select(.name == "llmrc") | .version' "$META")"
[ -n "$WORKSPACE_VERSION" ] || fail "could not read the llmrc version from cargo metadata"

# Refuse to release a workspace whose crates disagree on the version, because
# dependents resolve each other by exact version on crates.io.
if ! jq -e --arg v "$WORKSPACE_VERSION" 'all(.packages[]; .version == $v)' "$META" >/dev/null; then
  jq -r --arg v "$WORKSPACE_VERSION" \
    '.packages[] | select(.version != $v) | "  \(.name) is \(.version), expected \($v)"' "$META" >&2
  fail "all workspace crates must share version $WORKSPACE_VERSION"
fi

# Sparse index path, per the cargo registry index specification.
index_path() {
  local name="$1"
  case "${#name}" in
  1) printf '1/%s' "$name" ;;
  2) printf '2/%s' "$name" ;;
  3) printf '3/%s/%s' "${name:0:1}" "$name" ;;
  *) printf '%s/%s/%s' "${name:0:2}" "${name:2:2}" "$name" ;;
  esac
}

# True when `name` has a `version` entry in the sparse index. The body is
# captured before matching so no writer can be killed by an early-exiting reader.
in_index() {
  local name="$1" want="$2" body
  body="$(curl -fsS --max-time 30 -A "$USER_AGENT" \
    "https://index.crates.io/$(index_path "$name")" 2>/dev/null)" || return 1
  [[ "$body" == *"\"vers\":\"${want}\""* ]]
}

wait_for_index() {
  local name="$1" want="$2" poll
  for poll in $(seq 1 "$INDEX_MAX_POLLS"); do
    if in_index "$name" "$want"; then
      log "  index: ${name} ${want} is available"
      return 0
    fi
    log "  waiting for ${name} ${want} in the index (${poll}/${INDEX_MAX_POLLS})"
    sleep "$INDEX_POLL_SECONDS"
  done
  fail "timed out waiting for ${name} ${want} to appear in the sparse index"
}

# Seconds to wait after a rate-limit error, taken from the HTTP-date crates.io
# embeds in the error detail. Falls back to a fixed delay when the timestamp
# cannot be parsed (for example under BSD date on macOS).
rate_limit_wait_seconds() {
  local output="$1" stamp target now
  stamp="$(printf '%s' "$output" | sed -n 's/.*Please try again after \(.*\) and see .*/\1/p')"
  stamp="${stamp%%$'\n'*}"
  if [ -n "$stamp" ]; then
    if target="$(date -d "$stamp" +%s 2>/dev/null)"; then
      now="$(date -u +%s)"
      if [ "$target" -gt "$now" ]; then
        printf '%s' "$((target - now + 5))"
        return 0
      fi
    fi
  fi
  printf '%s' "$FALLBACK_RATE_LIMIT_WAIT"
}

is_rate_limited() {
  # Lowercased with `tr` rather than `${var,,}` so the script also runs on the
  # bash 3.2 shipped with macOS.
  local lower
  lower="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
  [[ "$lower" == *"too many new crates"* ||
    "$lower" == *"too many updates to existing crates"* ||
    "$lower" == *"too many requests"* ||
    "$lower" == *"status 429"* ]]
}

publish_crate() {
  local crate="$1" output attempt=1 wait_seconds
  while :; do
    if output="$(cargo publish -p "$crate" --locked 2>&1)"; then
      printf '%s\n' "$output"
      return 0
    fi
    printf '%s\n' "$output"

    # A crate can be in the index even when the command reported failure, for
    # example when an earlier attempt uploaded it before erroring out.
    if in_index "$crate" "$WORKSPACE_VERSION"; then
      log "  ${crate} ${WORKSPACE_VERSION} is already in the index; treating as published"
      return 0
    fi

    if is_rate_limited "$output"; then
      if [ "$attempt" -ge "$MAX_RATE_LIMIT_WAITS" ]; then
        fail "rate limited publishing ${crate} after ${attempt} waits"
      fi
      wait_seconds="$(rate_limit_wait_seconds "$output")"
      log "  rate limited; waiting ${wait_seconds}s before retrying ${crate} (wait ${attempt}/${MAX_RATE_LIMIT_WAITS})"
      sleep "$wait_seconds"
      attempt=$((attempt + 1))
      continue
    fi

    fail "failed to publish ${crate} ${WORKSPACE_VERSION}"
  done
}

workspace_dep_count() {
  jq -r --arg c "$1" '
    (.packages | map(.name)) as $members
    | .packages[]
    | select(.name == $c)
    | [.dependencies[]?.name | select(. as $d | $members | index($d))] | length
  ' "$META"
}

log "workspace version: ${WORKSPACE_VERSION}"
log "mode: $([ "$DRY_RUN" = "1" ] && echo 'dry run (nothing is uploaded)' || echo 'publish')"
log "crates: ${#CRATES[@]}"

if [ "$DRY_RUN" = "1" ]; then
  # Packaging a crate resolves its path dependencies against crates.io, so a
  # full `cargo publish --dry-run` only works for crates whose dependencies are
  # already published. Everything else is validated with `cargo package --list`.
  for crate in "${CRATES[@]}"; do
    cargo package --list -p "$crate" --allow-dirty >/dev/null
    log "  packaged: ${crate} (manifest and file list verified)"
    if [ "$(workspace_dep_count "$crate")" = "0" ]; then
      cargo publish -p "$crate" --dry-run --allow-dirty --locked
      log "  verified ${crate} end to end (no workspace dependencies)"
    fi
  done
  log "dry run complete: ${#CRATES[@]} crates validated, nothing uploaded"
  exit 0
fi

[ -n "${CARGO_REGISTRY_TOKEN:-}" ] || fail "CARGO_REGISTRY_TOKEN is not set"

for crate in "${CRATES[@]}"; do
  if in_index "$crate" "$WORKSPACE_VERSION"; then
    log "skip ${crate} ${WORKSPACE_VERSION}: already published"
    continue
  fi
  log "publishing ${crate} ${WORKSPACE_VERSION}"
  publish_crate "$crate"
  wait_for_index "$crate" "$WORKSPACE_VERSION"
done

log "published all ${#CRATES[@]} crates at ${WORKSPACE_VERSION}"
