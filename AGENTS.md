# Agent Development Guide (AGENTS.md)

This document provides definitive guidelines for AI coding agents and automated assistants developing or refactoring code within the `llmrc` workspace.

---

## 1. Project Architecture & Workspace Layout

`llmrc` is structured as a cargo workspace with decoupled domain boundaries:

```
llmrc (workspace root facade)
├── crates/core               # Core domain types (Message, ChatRequest, LlmError, Secret, traits)
├── crates/runtime            # RetryPolicy, jitter, token accounting, counters
├── crates/agent              # Bounded Agent loop, Tool trait, SessionStore, concurrency bounds
├── crates/mcp                # MCP bridge, stdio/HTTP transports, tool registry integration
├── crates/bots-core          # Platform-neutral BotHandler, serialized per-session locks, splitting
├── crates/bots-discord       # Discord adapter
├── crates/bots-telegram      # Telegram adapter
├── crates/bots-slack         # Slack adapter
├── crates/provider-openai    # async-openai adapter (OpenAI & Azure)
└── crates/provider-ollama    # ollama-rs adapter
```

### Dependency Rules
- `core` must remain free of provider-specific SDK dependencies.
- `runtime` depends only on `core` and foundational async crates (`tokio`, `futures`).
- `agent` depends on `core` and `runtime`.
- Provider and bot platform crates are optional and feature-gated in the root `llmrc` facade.

---

## 2. Test-Driven Development (TDD) Mandatory Protocol

Agents must strictly follow TDD when modifying or extending this codebase:

1. **Write Unit Tests First**:
   - Every new type, method, or edge case must have corresponding unit tests in a `#[cfg(test)] mod tests` block within the same file or a dedicated test file.
   - Assert failure modes, timeout behaviors, and boundary conditions explicitly.
2. **Verify Failure**:
   - Ensure the new test fails for the expected reason before introducing production changes.
3. **Implement Minimally**:
   - Write the cleanest, most direct code to satisfy the test requirements.
4. **Refactor & Validate**:
   - Run the full workspace suite: `cargo test --workspace --all-features`.
   - Never skip tests or disable features.

---

## 3. Strict Quality & Lint Standards

Every change made by an agent must pass the following four quality gates without exception:

```bash
# 1. Formatting
cargo fmt --all -- --check

# 2. Check
cargo check --workspace --all-features

# 3. All tests
cargo test --workspace --all-features

# 4. Strict clippy (zero warnings tolerated)
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

If `clippy` or `rustfmt` flags any line, you must fix it immediately before concluding your task.

---

## 4. Key Invariants & Design Principles

### A. Cancellation & Timeouts
- **Token Propagation**: Every async API taking part in a request/response or background task must accept `tokio_util::sync::CancellationToken`.
- **Immediate Interruption**: Use `tokio::select!` with cancellation branches to abort in-flight attempts promptly.
- **Do Not Restart Visible Streams**: Once a stream chunk has been observed by a user/caller, retry policies must not restart the stream from scratch.

### B. Secret Safety
- **No Plaintext Credential Logging**: All API keys, tokens, and authorization headers must be stored in `llmrc_core::Secret`.
- **Debug Redaction**: Never remove or bypass `Secret` redaction in `fmt::Debug`.
- **Token Accounting Privacy**: Implementations of `TokenCounter` must count tokens without logging or storing raw prompt/completion strings.

### C. Error Modeling & Recovery
- **Standardized Error Types**: Map provider SDK errors to `llmrc_core::LlmError`.
- **Classification**: Ensure `LlmError::kind()` correctly categorizes errors into `ErrorKind` (RateLimited, Authentication, Server, Timeout, Cancelled, etc.).
- **Retryability**: Mark only transient errors (RateLimited, Server, Transport, Timeout) as retryable (`is_retryable()`).

### D. Concurrency & Performance
- **Bounded Concurrency**: Parallel tasks (e.g., executing multiple tool calls) must be capped using `tokio::sync::Semaphore`.
- **Avoid $O(N^2)$ Loops**: In data structure manipulations (such as history trimming), prefer single-pass filtering (`retain`, `drain`) over element-by-element removals (`Vec::remove(0)`).
- **Session Locking**: In multi-user / multi-platform environments (like `BotHandler`), always lock turns per conversation key to prevent state races while allowing disjoint sessions to run in parallel.

### E. Clean Documentation & Comments
- Maintain concise, standard Rust doc comments (`///` and `//!`).
- **Forbidden**: Agent self-monologues, temporary planning notes in comments, or comments stating obvious code syntax. Preserve existing domain docstrings.
