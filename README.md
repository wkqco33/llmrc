# llmrc

`llmrc` is a modular Rust LLM runtime for predictable provider switching,
bounded tool execution, MCP integration, and chat-platform adapters.

## Status

The initial implementation is in progress and currently includes:

- Common typed chat, multimodal content, tool, embedding, usage, and stream
  event models in `llmrc-core`
- OpenAI and Azure adapters backed by `async-openai`
- Ollama adapter backed by `ollama-rs`
- Retry policies and explicit exact/provider-reported/estimated token counts
  in `llmrc-runtime`
- A bounded agent loop with ordered parallel tool execution, cancellation,
  timeouts, and session storage in `llmrc-agent`
- MCP tool bridging with custom test transports and optional official `rmcp`
  3.x stdio and Streamable HTTP adapters
- Platform-neutral bot sessions plus feature-gated Discord, Telegram, and
  Slack event mappers

The public API may change before the first stable release.

## Workspace crates

| Crate | Purpose |
| --- | --- |
| `llmrc` | Facade and feature-gated re-exports |
| `llmrc-core` | Provider-independent domain types and traits |
| `llmrc-runtime` | Retry and token-budget policies |
| `llmrc-agent` | Bounded agent execution and tools |
| `llmrc-openai` | OpenAI and Azure OpenAI |
| `llmrc-ollama` | Ollama |
| `llmrc-mcp` | MCP transports and tool bridge |
| `llmrc-bots-core` | Shared conversation/session handling |
| `llmrc-bots-{discord,telegram,slack}` | Platform-specific mappings |

## Feature selection

```toml
[dependencies]
llmrc = { version = "0.1", features = ["openai", "agent", "mcp"] }
```

Bot platform dependencies are opt-in:

```toml
llmrc = {
    version = "0.1",
    features = ["openai", "agent", "discord", "telegram", "slack"],
}
```

The `mcp` feature enables the transport-agnostic bridge. Enable the
`rmcp` feature on `llmrc-mcp` when using the official `rmcp` transport
constructors directly.

## Quickstart

```rust,no_run
use llmrc::prelude::*;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Construct a provider (e.g. OpenAI or Ollama)
    #[cfg(feature = "openai")]
    let provider = llmrc::openai::OpenAiProvider::openai("YOUR_API_KEY");

    #[cfg(feature = "ollama")]
    let provider = llmrc::ollama::OllamaProvider::localhost();

    // Create a bounded, cancellation-aware agent
    let agent = Agent::new(provider).with_config(AgentConfig {
        model: "gpt-4o".into(),
        max_turns: 8,
        ..Default::default()
    });

    let cancellation = CancellationToken::new();
    let result = agent.run("Hello, world!", cancellation).await?;
    println!("Response: {}", result.message.text());

    Ok(())
}
```

## Quality checks

```bash
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

No real provider or bot credentials are required by the test suite; network
smoke tests should remain opt-in.

## Examples

Self-contained, executable examples are located in [`examples/`](examples/README.md):

- `cargo run --example 01_basic_agent`: Configuring an agent and inspecting token accounting.
- `cargo run --example 02_custom_tools`: Registering custom tools and handling tool call reasoning loops.
- `cargo run --example 03_bot_conversation --features bots`: Multi-platform bot session locks and message splitting.
- `cargo run --example 04_streaming_and_retry`: Exponential backoff with jitter and stream retry semantics.

See [examples/README.md](examples/README.md) for details.

## Guidelines & Documentation

- [AGENTS.md](AGENTS.md): Development guide and invariants for automated coding agents
- [CONTRIBUTING.md](CONTRIBUTING.md): Contribution workflow and quality requirements
- [SECURITY.md](SECURITY.md): Vulnerability reporting policy and security practices
- [LICENSE](LICENSE): MIT License
