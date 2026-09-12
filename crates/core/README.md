# llmrc-core

Provider-independent domain types and async contracts for the
[`llmrc`](https://github.com/wkqco33/llmrc) LLM runtime.

This crate defines `Message`, `ChatRequest`, `ChatResponse`, `StreamEvent`,
`ToolDefinition`, `LlmError`, `Secret`, and the `ChatProvider` /
`EmbeddingProvider` traits. It has no provider SDK dependency.

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade with a feature flag instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-core = "0.1"
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-core)

## License

MIT
