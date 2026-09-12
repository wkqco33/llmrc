# llmrc-agent

Provider-backed tool-using agent runtime for [`llmrc`](https://crates.io/crates/llmrc).

## Overview

`llmrc-agent` implements a bounded agent loop designed for production safety:

- **Bounded Execution**: Explicit turn limits (`max_turns`), tool execution caps (`max_tool_calls`), timeout deadlines, and per-tool timeouts.
- **Concurrency Control**: Parallel tool calls bounded by `tokio::sync::Semaphore` with `max_concurrency`.
- **Tool System**: `Tool` trait, JSON Schema validation, and `ToolRegistry`.
- **Session Management**: `SessionStore` trait and `InMemorySessionStore` with $O(N)$ history trimming preserving system prompts.
- **Cancellation**: Full propagation of `tokio_util::sync::CancellationToken`.

## Usage

Most applications should use the top-level [`llmrc`](https://crates.io/crates/llmrc) facade:

```toml
[dependencies]
llmrc = { version = "0.1" }
```

Direct dependency:

```toml
[dependencies]
llmrc-agent = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-agent).
