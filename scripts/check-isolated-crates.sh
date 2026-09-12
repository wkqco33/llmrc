#!/usr/bin/env bash
#
# Builds every workspace crate on its own, so that feature unification cannot
# hide a crate that uses a feature it never declared.
#
# `cargo test --workspace` unifies features across all members: if any one crate
# enables, say, `tokio/process`, every other crate sees that feature too. A crate
# can therefore compile in CI and still fail its release, because `cargo publish`
# builds the packaged tarball with only the features that crate declares. This
# script reproduces the publish-time view without needing the registry.

set -euo pipefail

crash() {
  printf '::error::%s\n' "$*" >&2
  exit 1
}

cd "$(dirname "${BASH_SOURCE[0]}")/.."

command -v jq >/dev/null 2>&1 || crash "jq is required"

# Dependencies before dependents, matching scripts/publish-crates.sh.
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

# Fail loudly if a crate is added, renamed, or removed without updating the list.
members="$(cargo metadata --no-deps --format-version 1 --locked \
  | jq -r '.packages[].name' | sort | tr '\n' ' ')"
expected="$(printf '%s\n' "${CRATES[@]}" | sort | tr '\n' ' ')"
[ "$members" = "$expected" ] || crash "crate list is stale; workspace has: ${members}"

for crate in "${CRATES[@]}"; do
  printf '\n== %s ==\n' "$crate"
  cargo check -p "$crate" --locked
  cargo check -p "$crate" --all-features --locked
done

printf '\nisolated builds passed for %s crates\n' "${#CRATES[@]}"
