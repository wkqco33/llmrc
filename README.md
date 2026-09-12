# llmrc

[![CI](https://github.com/wkqco33/llmrc/actions/workflows/ci.yml/badge.svg)](https://github.com/wkqco33/llmrc/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/llmrc.svg)](https://crates.io/crates/llmrc)
[![docs.rs](https://docs.rs/llmrc/badge.svg)](https://docs.rs/llmrc)

A predictable, modular LLM runtime facade for Rust.

> 한국어 문서: [README.ko.md](README.ko.md) · [GUIDE.ko.md](GUIDE.ko.md)

`llmrc` gives you one typed API for chat, embeddings, tool calling, retries,
token accounting, MCP tools, and chat-platform bots — while keeping every
provider SDK behind an opt-in feature flag. The runtime is built so that
**nothing runs unbounded**: every call has a turn limit, a tool limit, a
concurrency cap, a timeout, and a cancellation token.

- **Provider independent** — application code talks to `ChatProvider`, not to
  `async-openai` or `ollama-rs`.
- **Cancellation first** — every async entry point accepts a
  `tokio_util::sync::CancellationToken` and aborts in-flight work on cancel.
- **Bounded by construction** — turn, tool, concurrency, deadline, and history
  limits are explicit config, not afterthoughts.
- **Secret safe** — credentials live in `llmrc_core::Secret`, which redacts
  itself in `Debug` and never implements `Display`.
- **Feature gated** — SDKs and platform adapters only compile when asked for.

---

## Status

`0.1.0`, pre-stable. The public API may change before the first stable
release. The entire workspace builds and tests offline: **29 unit tests pass,
`clippy -D warnings` is clean, and `rustfmt` reports no diffs.** No provider or
bot credentials are required by the test suite.

| Gate | Command | Current |
| --- | --- | --- |
| Format | `cargo fmt --all -- --check` | clean |
| Check | `cargo check --workspace --all-features` | clean |
| Test | `cargo test --workspace --all-features` | 29 passed |
| Lint | `cargo clippy --workspace --all-features --all-targets -- -D warnings` | clean |

---

## Architecture

```text
llmrc (workspace root facade)
├── crates/core               # Domain types + async contracts (no provider SDKs)
├── crates/runtime            # RetryPolicy, jitter, budgets, token accounting
├── crates/agent              # Bounded tool-using agent loop, sessions, Tool trait
├── crates/mcp                # MCP bridge + stdio / Streamable HTTP transports
├── crates/bots-core          # Platform-neutral BotHandler, per-session locks
├── crates/bots-discord       # Discord event mapping
├── crates/bots-telegram      # Telegram event mapping
├── crates/bots-slack         # Slack event mapping
├── crates/provider-openai    # async-openai adapter (OpenAI + Azure)
└── crates/provider-ollama    # ollama-rs adapter
```

Dependency direction is strictly one-way:

```text
provider-* ─┐
bots-* ─────┼─► agent ──► runtime ──► core
mcp ────────┘                │
                             └─► core
```

`core` has no provider SDK dependency. `runtime` depends only on `core` plus
`futures`/`tokio`/`tokio-util`. `agent` layers on `core + runtime`. Provider,
MCP, and bot crates are optional and feature-gated in the root `llmrc` facade.

---

## Crate reference

| Crate | Responsibility | Public surface highlights |
| --- | --- | --- |
| `llmrc` | Facade, feature-gated re-exports, `prelude` | `Agent`, `Message`, `RetryPolicy`, `OpenAiProvider`, `OllamaProvider`, `mcp::*`, `bots::*` |
| `llmrc-core` | Provider-independent domain types and traits | `Message`, `MessageRole`, `ContentPart`, `ImageSource`, `ChatRequest`, `ChatResponse`, `StreamEvent`, `ToolDefinition`, `ToolCall`, `TokenUsage`, `LlmError`, `ErrorKind`, `Secret`, `ChatProvider`, `EmbeddingProvider` |
| `llmrc-runtime` | Retry policy and token accounting | `RetryPolicy`, `RetryPolicyBuilder`, `RetryBudget`, `Jitter`, `RetryClassifier`; `TokenCounter`, `HeuristicTokenCounter`, `ExactTokenCounter`, `ProviderReportedTokenCounter`, `TokenAccounting` |
| `llmrc-agent` | Bounded agent loop, tools, sessions | `Agent`, `AgentConfig`, `AgentEvent`, `AgentResult`, `AgentError`, `Tool`, `ToolRegistry`, `ToolError`, `Session`, `SessionStore`, `InMemorySessionStore` |
| `llmrc-openai` | OpenAI and Azure OpenAI via `async-openai` | `OpenAiProvider::openai`, `::openai_with_base_url`, `::azure`; `EmbeddingProvider` |
| `llmrc-ollama` | Local/remote Ollama via `ollama-rs` | `OllamaProvider::localhost`, `::new(base_url)`; `EmbeddingProvider` |
| `llmrc-mcp` | MCP JSON-RPC bridge and transports | `McpBridge`, `McpPool`, `McpTransport`, `McpExecutableTool`, `StdioTransport`, `StreamableHttpTransport`, `rmcp_adapter` |
| `llmrc-bots-core` | Conversation handling and splitting | `BotHandler`, `BotEvent`, `BotResponse`, `ConversationKey`, `ConversationId`, `BotSessionStore`, `split_message`, `PlatformAdapter`, `BotError` |
| `llmrc-bots-{discord,telegram,slack}` | Native event ↔ `BotEvent` mapping | `DiscordAdapter`, `TelegramAdapter`, `SlackAdapter` |

---

## Feature selection

```toml
[dependencies]
# Provider + MCP bridge only:
llmrc = { version = "0.1", features = ["openai", "mcp"] }
```

| Feature | Enables | Notes |
| --- | --- | --- |
| *(none)* | `core`, `runtime`, `agent` | Always compiled; the agent loop is not optional |
| `openai` | `llmrc-openai` | OpenAI chat, streaming, embeddings, tools, multimodal |
| `azure` | `llmrc-openai` | Azure OpenAI deployment configuration |
| `ollama` | `llmrc-ollama` | Ollama chat, streaming, embeddings, tools, base64 images |
| `mcp` | `llmrc-mcp` | Transport-agnostic MCP bridge and custom transports |
| `bots` | `llmrc-bots-core` | Platform-neutral `BotHandler` |
| `discord` / `telegram` / `slack` | `bots` + platform mapper | Implies `bots` |
| `serenity` / `teloxide` / `slack-morphism` | Alias of the platform feature | Present for naming symmetry; no extra deps today |

Bot platforms are opt-in:

```toml
llmrc = {
    version = "0.1",
    features = ["openai", "discord", "telegram", "slack"],
}
```

To use the official `rmcp` client adapters, depend on `llmrc-mcp` directly and
enable its `rmcp` feature — the facade does not re-export that feature.

```toml
llmrc-mcp = { version = "0.1", features = ["rmcp"] }
```

---

## Quickstart

```rust,no_run
use llmrc::prelude::*;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Pick a provider. Exactly one is needed.
    let provider = llmrc::ollama::OllamaProvider::localhost();
    // let provider = llmrc::openai::OpenAiProvider::openai(std::env::var("OPENAI_API_KEY")?);

    // 2. Create a bounded, cancellation-aware agent.
    let agent = Agent::new(provider).with_config(AgentConfig {
        model: "llama3.2".into(),
        max_turns: 8,
        max_tool_calls: 16,
        max_concurrency: 4,
        ..Default::default()
    });

    // 3. Run it with a cancellation token.
    let cancellation = CancellationToken::new();
    let result = agent.run("Explain Rust ownership in one sentence.", cancellation).await?;

    println!("{}", result.message.text());
    println!("turns={} tools={} tokens={}", result.turns, result.tool_calls, result.usage.total());
    Ok(())
}
```

The `prelude` re-exports the types most applications need: `Agent`,
`AgentConfig`, `ChatProvider`, `ChatRequest`, `Message`, `RetryPolicy`,
`Tool`, `ToolRegistry`, `TokenCounter`, `LlmError`, and friends.

---

## Core concepts

### Providers

`ChatProvider` is the single boundary between your application and any LLM:

```rust
fn kind(&self) -> ProviderKind;
fn capabilities(&self) -> ProviderCapabilities;
async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;
async fn chat_stream(&self, request: ChatRequest) -> Result<BoxChatStream, LlmError>;
```

Implement it for a new backend and the entire agent, retry, and bot stack works
unchanged. Streaming is a separate method — the agent loop itself currently
uses `chat`; use `RetryPolicy::collect_stream` or the provider stream directly
when you need incremental output.

### Agent

`Agent` runs a bounded reason → tool → observe loop:

1. Send accumulated history plus registered tool definitions to the provider.
2. If the assistant requested tool calls, validate arguments, execute them with
   bounded concurrency, and append `Tool` messages.
3. Repeat until the assistant stops or a limit is hit.

`AgentConfig` defaults are conservative and every one is overridable:

| Field | Default | Purpose |
| --- | --- | --- |
| `model` | `""` | **Must be set**; sent on every request |
| `max_turns` | `16` | Provider round-trips per run |
| `max_tool_calls` | `64` | Tools per run, checked before a batch executes |
| `deadline` | `None` | Wall-clock cap for the whole run |
| `tool_timeout` | `30s` | Per-tool execution cap |
| `max_concurrency` | `4` | Semaphore width for parallel tool calls |
| `max_history_messages` | `100` | History length before trimming |
| `retry_policy` | 2 retries / 100 ms base / 30 s cap | Applies to provider calls only |

Sessions are keyed strings backed by `SessionStore`
(`InMemorySessionStore` by default); `Agent::run` uses the session id
`"default"`, while `run_session` takes an explicit id. A session is only saved
after a successful run.

### Tools

Implement `Tool` and register it:

```rust
struct AddTool;

#[async_trait::async_trait]
impl Tool for AddTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "add".into(),
            description: Some("Add two numbers".into()),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["a", "b"],
                "properties": { "a": {"type": "number"}, "b": {"type": "number"} },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, ToolError> {
        let a = arguments["a"].as_f64().ok_or(ToolError::Failed)?;
        let b = arguments["b"].as_f64().ok_or(ToolError::Failed)?;
        Ok((a + b).to_string())
    }
}

let mut registry = ToolRegistry::new();
registry.register(AddTool)?;
let agent = agent.with_registry(registry);
```

The registry rejects duplicate names and enforces tool names of 1–64 ASCII
alphanumeric/`_`/`-` characters (so they are always valid provider schema
identifiers). Arguments are validated against a practical JSON Schema subset —
`type`, `required`, `properties`, `additionalProperties: false` — before the
tool runs. Override `Tool::execute_with_cancellation` when your tool performs
its own cancellable I/O.

### Retry and cancellation

`RetryPolicy` retries only what `LlmError::is_retryable()` classifies as
transient (`RateLimited`, `Server`, `Transport`, `Timeout`):

- Exponential backoff `base_delay * 2^(n-1)`, capped at `max_delay`.
- `Jitter::{None, Full, Equal}` (default `Equal`).
- Optional shared `RetryBudget` so a fleet of calls cannot retry forever.
- `Retry-After` on `LlmError::RateLimited` overrides the computed delay.
- A pluggable `RetryClassifier` for provider-specific policy.

`collect_stream` retries a stream **only before a user-visible item is
observed**. Once the visibility predicate returns true, an error is surfaced as
is and the stream is never restarted — avoiding duplicated output.

### Token accounting

`TokenCounter` is pluggable, and every count carries its provenance:
`TokenCount::{Exact, ProviderReported, Estimated}`. The agent prefers
provider-reported usage; otherwise it falls back to the configured counter.
`HeuristicTokenCounter` uses `chars / 4` (floor of 1 for non-empty text) and is
documented as an estimate, not a billing source. `TokenAccounting` accumulates
prompt/completion totals and merges provenance conservatively (an estimate
taints the merged result).

### MCP

`McpBridge` wraps any `McpTransport`, refreshes its tool list, namespaces tools
as `server__tool`, and registers them into a `ToolRegistry` so the agent can
call remote MCP tools like local ones. Two transports ship built in:

- `StdioTransport::spawn(program, args)` — newline-delimited JSON-RPC over a
  child process, serialized requests, child killed on drop.
- `StreamableHttpTransport::new(endpoint)` — JSON-RPC over HTTP(S) POST with
  optional headers.

Enable `rmcp` on `llmrc-mcp` for the official `rmcp` 3.x client adapters
(`rmcp_adapter::StdioTransport`, `rmcp_adapter::StreamableHttpTransport`).

### Bots

`BotHandler` turns an `Agent` into a multi-conversation service. It locks turns
**per `ConversationKey`** (platform + conversation id), so one busy channel
cannot corrupt its session while unrelated channels run in parallel. It also
splits long replies with `split_message`, which slices on UTF-8 boundaries and
prefers newline/whitespace break points, guaranteeing each part is at most the
configured byte limit. `DiscordAdapter`, `TelegramAdapter`, and `SlackAdapter`
map native events to `BotEvent` and back.

---

## Examples

Runnable, offline, no credentials needed — see [`examples/README.md`](examples/README.md):

```bash
cargo run --example 01_basic_agent            # agent config + token accounting
cargo run --example 02_custom_tools           # tool schema + reasoning loop
cargo run --example 03_bot_conversation --features bots   # sessions + splitting
cargo run --example 04_streaming_and_retry    # backoff, budget, safe streaming
```

---

## Documentation

- [`GUIDE.md`](GUIDE.md) — the detailed user guide: installation, provider
  setup, agent configuration, tools, retries, streaming, MCP, bots, error
  handling, and troubleshooting.
- [`GUIDE.ko.md`](GUIDE.ko.md) / [`README.ko.md`](README.ko.md) — Korean
  translations of this guide and README.
- [`RELEASING.md`](RELEASING.md) — tag-driven release process: crates.io setup,
  publish order, rate limits, and resuming a partial release.
- [`examples/README.md`](examples/README.md) — example index and live-provider
  setup.
- [`AGENTS.md`](AGENTS.md) — invariants and TDD protocol for automated coding
  agents.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contribution workflow and quality
  gates.
- [`SECURITY.md`](SECURITY.md) — vulnerability reporting and security
  practices.
- [`LICENSE`](LICENSE) — MIT.

---

## Known limitations

These are deliberate current boundaries, not bugs:

- The `Agent` loop is non-streaming: it calls `ChatProvider::chat`. Streaming
  is available directly on providers and through
  `RetryPolicy::collect_stream`.
- History trimming drops the oldest non-system messages first, so aggressive
  `max_history_messages` values can split an assistant `tool_calls` message
  from its `Tool` results. Keep the limit comfortably above a full tool round,
  or trim only at turn boundaries.
- Provider SDKs do not yet surface `Retry-After` headers, so
  `LlmError::RateLimited` is built with `retry_after: None`; only custom
  providers benefit from delay override today.
- The bot platform features are thin mappers. They do not open gateway
  connections; you drive the transport (`serenity`, `teloxide`,
  `slack-morphism`) in your application.
- Tool argument validation is a JSON Schema subset, not a full validator.
- `RetryStats` is exported for bookkeeping but is not yet populated by the
  runtime.
