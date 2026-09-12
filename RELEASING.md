# Releasing llmrc

Releases are tag-driven. `[workspace.package].version` is the single source of
truth — all 11 crates inherit it — and `.github/workflows/publish.yml` publishes
them to crates.io when a matching tag is pushed.

> 한국어판: [RELEASING.ko.md](RELEASING.ko.md)

---

## One-time setup

1. **crates.io token.** Create an API token at <https://crates.io/settings/tokens>
   with the `publish-new` and `publish-update` scopes, then store it as the
   repository secret `CARGO_REGISTRY_TOKEN`:

   ```bash
   tr -d '\r\n' < crates_token.txt \
     | gh secret set CARGO_REGISTRY_TOKEN --repo wkqco33/llmrc
   ```

   This is already configured for `wkqco33/llmrc`. Never commit the token; the
   working tree ignores `crates_token.txt` and `*_token.txt`.

2. **Optional release gate.** Create a `crates-io` environment in the repository
   settings and add required reviewers. The publish job targets that
   environment, so approvals will be needed before any upload.

---

## Publish order

A packaged crate resolves its path dependencies against the crates.io index, so
a crate can only be published after everything it depends on is already there.
`scripts/publish-crates.sh` enforces this order:

| # | Crate | Depends on |
| --- | --- | --- |
| 1 | `llmrc-core` | — |
| 2 | `llmrc-runtime` | core |
| 3 | `llmrc-agent` | core, runtime |
| 4 | `llmrc-openai` | core |
| 5 | `llmrc-ollama` | core |
| 6 | `llmrc-mcp` | core, agent |
| 7 | `llmrc-bots-core` | agent |
| 8 | `llmrc-bots-discord` | bots-core |
| 9 | `llmrc-bots-telegram` | bots-core |
| 10 | `llmrc-bots-slack` | bots-core |
| 11 | `llmrc` (facade) | all of the above |

After each publish the script polls the sparse index until the new version is
visible, so dependents never race the index.

---

## crates.io rate limits

From the official [publish rate limits](https://crates.io/docs/rate-limits):

| Publish type | Burst | Sustained rate |
| --- | --- | --- |
| Brand new crates | 5 at once | 1 crate every 10 minutes |
| New versions of existing crates | 30 at once | 1 crate per minute |

This makes the **first** release the slow one: 11 brand new crates means a burst
of 5 followed by 6 more at roughly 10 minutes apart, so expect **about an hour**.
The script reads the retry time crates.io reports and waits automatically; the
job allows 180 minutes.

Every later release publishes new *versions* of crates that already exist, which
falls under the 30-version burst, so the whole workspace goes out in one pass.

---

## Release procedure

```bash
# 1. Bump the single version field.
$EDITOR Cargo.toml        # [workspace.package] version = "0.2.0"

# 2. Refresh the lockfile if dependencies changed.
cargo check --workspace --all-features

# 3. Run the quality gates.
cargo fmt --all -- --check
cargo check --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings

# 4. Commit, then tag with the matching version.
git commit -am "release: 0.2.0"
git tag -a v0.2.0 -m "v0.2.0"
git push origin master
git push origin v0.2.0
```

Pushing the tag starts the `Release` workflow:

1. `verify` — rustfmt, clippy, the full test suite, and
   `scripts/check-isolated-crates.sh`.
2. `version` — fails fast when the tag does not match `[workspace.package].version`.
3. `publish` — runs `scripts/publish-crates.sh` with `CARGO_REGISTRY_TOKEN`,
   then creates a GitHub release with generated notes.

### Why the isolated build check exists

`cargo test --workspace` unifies features across workspace members, so if any one
crate enables a feature, every other crate sees it. A crate can therefore use a
feature it never declared, pass CI, and then fail its release — which is exactly
what happened to `llmrc-mcp` on the first `v0.1.0` attempt, where
`cargo publish` refused the tarball with `unresolved import tokio::process`.

`scripts/check-isolated-crates.sh` builds each crate on its own with only the
features that crate declares, which is the view `cargo publish` uses. It runs in
CI and in the release `verify` job, and fails if the crate list drifts from the
workspace members.

---

## Dry run

Validate packaging without uploading anything:

```bash
DRY_RUN=1 scripts/publish-crates.sh
```

Or run the `Release` workflow manually (Actions → Release → Run workflow) with
`dry_run` left at `true`.

A full `cargo publish --dry-run` only works for crates whose dependencies are
already on crates.io, so dry-run mode verifies the dependency-free crates end to
end and checks the manifest and file list of the rest.

---

## Resuming a partial release

The script is idempotent. It skips every crate whose exact version is already in
the sparse index, so if a release stops halfway — a rate limit, a network blip,
or a cancelled job — re-run it and it continues from the first unpublished
crate.

Re-run the failed job from the Actions UI (same tag, same commit), or locally:

```bash
CARGO_REGISTRY_TOKEN=... scripts/publish-crates.sh
```

---

## Verifying a release

- Crate pages: <https://crates.io/crates/llmrc>, `llmrc-core`, and the rest.
- Documentation: <https://docs.rs/llmrc> once docs.rs finishes building.
- The GitHub release created by the workflow.

---

## If something is wrong after publishing

Published versions cannot be deleted, only yanked:

```bash
cargo yank --version 0.2.0 -p llmrc-core
```

Yanking warns new dependents away without breaking existing lockfiles. For a
defect, yank the affected crates and publish a patch version through the normal
tag flow.

---

## Workflow hardening notes

- Actions are pinned to full-length commit SHAs.
- `actions/checkout` runs with `persist-credentials: false`.
- The release workflow deliberately uses **no build cache**: it is the only
  workflow that can reach the crates.io token, and a poisoned cache is a
  supply-chain vector. CI (unprivileged) still caches.
