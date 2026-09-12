# llmrc-core

Provider-independent domain types and async contracts for the [`llmrc`](https://crates.io/crates/llmrc) modular LLM runtime.

## Overview

`llmrc-core` defines the foundational data types and trait contracts for LLM interactions without depending on any specific provider SDK:

- **Domain Types**: `Message`, `ChatRequest`, `ChatResponse`, `ContentPart`, `ToolDefinition`, `ToolCall`, `TokenUsage`
- **Contracts**: `ChatProvider`, `EmbeddingProvider`
- **Error Model**: `LlmError`, `ErrorKind`
- **Secret Safety**: `Secret` (automatic redaction in `Debug`, no `Display` implementation)

## Usage

Most applications should depend on the top-level [`llmrc`](https://crates.io/crates/llmrc) facade crate:

```toml
[dependencies]
llmrc = { version = "0.1", features = ["openai"] }
```

If you are implementing a custom provider adapter, depend on `llmrc-core` directly:

```toml
[dependencies]
llmrc-core = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-core).
