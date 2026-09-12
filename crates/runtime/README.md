# llmrc-runtime

Retry and token-accounting primitives for the [`llmrc`](https://crates.io/crates/llmrc) modular LLM runtime.

## Overview

`llmrc-runtime` provides foundational execution primitives:

- **Bounded Retry Policies**: `RetryPolicy` with configurable max retries, exponential backoff, jitter (`Full`, `Equal`, `None`), and retry budgets.
- **Safe Stream Retries**: Distinguishes transient stream initialization errors from in-flight stream failures to guarantee that visible chunks are never duplicated.
- **Token Accounting**: `TokenAccounting` tracking prompt, completion, and total tokens across multiple steps, plus `TokenCounter` traits (`HeuristicTokenCounter`, `ProviderReportedTokenCounter`).

## Usage

Most applications should use the top-level [`llmrc`](https://crates.io/crates/llmrc) facade:

```toml
[dependencies]
llmrc = { version = "0.1" }
```

Direct dependency:

```toml
[dependencies]
llmrc-runtime = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-runtime).
