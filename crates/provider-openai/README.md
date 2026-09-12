# llmrc-openai

OpenAI and Azure OpenAI providers for the
[`llmrc`](https://github.com/wkqco33/llmrc) LLM runtime, backed by
[`async-openai`](https://crates.io/crates/async-openai).

Supports chat, streaming, embeddings, tool calls, and multimodal content, and
maps SDK errors onto `llmrc-core`'s retryable `LlmError` variants.

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade with the `openai` or `azure` feature instead of using this crate
directly.

## Install

```toml
[dependencies]
llmrc-openai = "0.1"
```

## Usage

```rust,no_run
# use llmrc_openai::OpenAiProvider;
let provider = OpenAiProvider::openai("sk-...");

// Azure OpenAI
let provider = OpenAiProvider::azure(
    "azure-key",
    "https://my-resource.openai.azure.com",
    "my-deployment",
    "2024-10-21",
);
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-openai)

## License

MIT
