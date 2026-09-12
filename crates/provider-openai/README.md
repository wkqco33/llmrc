# llmrc-openai

OpenAI and Azure OpenAI provider adapter for [`llmrc`](https://crates.io/crates/llmrc).

## Overview

`llmrc-openai` implements the `llmrc_core::ChatProvider` and `llmrc_core::EmbeddingProvider` traits using `async-openai`:

- Standard OpenAI (`OpenAiProvider::openai`)
- Custom base URL compatible endpoints (`OpenAiProvider::openai_with_base_url`)
- Azure OpenAI deployments (`OpenAiProvider::azure`)
- Streaming completions, function/tool calling, and multimodal inputs

## Usage

Via top-level [`llmrc`](https://crates.io/crates/llmrc):

```toml
[dependencies]
llmrc = { version = "0.1", features = ["openai"] }
```

Direct dependency:

```toml
[dependencies]
llmrc-openai = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-openai).
