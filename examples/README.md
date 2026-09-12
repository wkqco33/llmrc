# llmrc Examples

This directory contains executable examples demonstrating the core features and common patterns of the `llmrc` runtime.

Each example is self-contained and runnable without external network services or API keys.

---

## Example Index

| Example | Description | Command |
| :--- | :--- | :--- |
| [`01_basic_agent.rs`](01_basic_agent.rs) | Configuring and running a bounded Agent with token accounting | `cargo run --example 01_basic_agent` |
| [`02_custom_tools.rs`](02_custom_tools.rs) | Defining custom tools, registering schemas, and executing tool loops | `cargo run --example 02_custom_tools` |
| [`03_bot_conversation.rs`](03_bot_conversation.rs) | Bot session serialization, per-user locks, and UTF-8 safe message splitting | `cargo run --example 03_bot_conversation --features bots` |
| [`04_streaming_and_retry.rs`](04_streaming_and_retry.rs) | Retry policies with jitter, budgets, and safe stream retry semantics | `cargo run --example 04_streaming_and_retry` |

---

## Running with Real LLM Providers

To run `llmrc` against live providers in your own application:

### OpenAI
```bash
export OPENAI_API_KEY="sk-..."
cargo run --example <name> --features openai
```

```rust,ignore
use llmrc::openai::OpenAiProvider;

let provider = OpenAiProvider::openai(std::env::var("OPENAI_API_KEY")?);
```

### Local Ollama
```bash
# Ensure ollama is running locally: ollama serve
cargo run --example <name> --features ollama
```

```rust,ignore
use llmrc::ollama::OllamaProvider;

let provider = OllamaProvider::localhost();
```
