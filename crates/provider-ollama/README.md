# llmrc-ollama

Ollama local/remote provider adapter for [`llmrc`](https://crates.io/crates/llmrc).

## Overview

`llmrc-ollama` implements the `llmrc_core::ChatProvider` and `llmrc_core::EmbeddingProvider` traits using `ollama-rs`:

- Local default instance (`OllamaProvider::localhost`)
- Custom remote instances (`OllamaProvider::new(base_url)`)
- Streaming chat responses, JSON schema tool calls, and multimodal image support

## Usage

Via top-level [`llmrc`](https://crates.io/crates/llmrc):

```toml
[dependencies]
llmrc = { version = "0.1", features = ["ollama"] }
```

Direct dependency:

```toml
[dependencies]
llmrc-ollama = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-ollama).
