# llmrc-ollama

A local or remote Ollama provider for the
[`llmrc`](https://github.com/wkqco33/llmrc) LLM runtime, backed by
[`ollama-rs`](https://crates.io/crates/ollama-rs).

Supports chat, streaming, embeddings, tool calls, and base64 images. URL images
are rejected with `LlmError::Unsupported` because Ollama does not fetch remote
images.

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade with the `ollama` feature instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-ollama = "0.1"
```

## Usage

```rust,no_run
# use llmrc_ollama::OllamaProvider;
let provider = OllamaProvider::localhost();               // http://127.0.0.1:11434
let provider = OllamaProvider::new("http://gpu-box:11434")?;
# Ok::<(), llmrc_core::LlmError>(())
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-ollama)

## License

MIT
