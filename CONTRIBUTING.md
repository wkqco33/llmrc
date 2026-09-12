# Contributing to llmrc

Thank you for your interest in contributing to `llmrc`!

`llmrc` is a predictable, modular LLM runtime facade for Rust, emphasizing safe concurrency, bounded tool execution, cancellation propagation, and pluggable provider adapters.

---

## Development Prerequisites

- **Rust**: Version 1.88 or later (Rust 2024 edition)
- **Cargo**: Standard Cargo toolchain with `clippy` and `rustfmt` components:
  ```bash
  rustup component add clippy rustfmt
  ```

---

## Development Workflow

### 1. Test-Driven Development (TDD)

All new features, bug fixes, and refactoring should follow TDD principles:
1. Write focused unit tests that assert desired behavior or expose the bug.
2. Verify tests fail as expected.
3. Implement the minimal necessary logic to make tests pass.
4. Refactor while keeping all tests passing.

### 2. Required Quality Checks

Before submitting any changes or opening a pull request, ensure all checks pass with zero errors and zero warnings:

```bash
# 1. Format verification
cargo fmt --all -- --check

# 2. Workspace compilation
cargo check --workspace --all-features

# 3. Comprehensive unit & integration tests
cargo test --workspace --all-features

# 4. Strict linting (warnings are treated as errors)
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

---

## Architectural & Coding Standards

- **Cancellation First**: Every long-running, streaming, or network operation must accept and observe a `tokio_util::sync::CancellationToken`.
- **Secret Redaction**: Sensitive data (API keys, bot tokens, passwords) must wrap in `llmrc_core::Secret` to prevent accidental leakage in `Debug` logs. Never log prompt or message contents directly.
- **Pluggable & Feature-Gated**: External provider SDKs (OpenAI, Ollama, Slack, Discord, Telegram, MCP) must remain strictly feature-gated to avoid bloat in consuming applications.
- **Typed Errors**: Propagate errors through `llmrc_core::LlmError` or crate-specific error enums (`AgentError`, `McpError`, `BotError`) deriving `thiserror::Error`.
- **Minimal & Clean Comments**: Use standard Rust documentation comments (`///` and `//!`). Avoid monologue, redundant restatements of code, or verbose commentary.

---

## Pull Request Guidelines

1. Create a descriptive feature branch (e.g., `feat/anthropic-provider` or `fix/history-trim-bounds`).
2. Keep commits atomic, well-described, and adhering to conventional commits (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`).
3. Ensure no secrets, tokens, or untracked test dumps are included in the git index.
4. Verify all automated quality checks pass locally.
