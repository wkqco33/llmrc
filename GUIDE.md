# llmrc User Guide

A practical, end-to-end guide to building applications with `llmrc`. It covers
installation, provider setup, the agent loop, tools, retries, streaming, token
accounting, MCP integration, chat bots, error handling, and troubleshooting.

If you want the short version, read the [README](./README.md). This document is
the long version.

> 한국어판: [GUIDE.ko.md](GUIDE.ko.md)

---

## Table of contents

1. [Installation and feature selection](#1-installation-and-feature-selection)
2. [The mental model](#2-the-mental-model)
3. [Domain types and messages](#3-domain-types-and-messages)
4. [Providers](#4-providers)
5. [The agent loop](#5-the-agent-loop)
6. [Tools](#6-tools)
7. [Sessions and history](#7-sessions-and-history)
8. [Retries, jitter, and budgets](#8-retries-jitter-and-budgets)
9. [Cancellation and timeouts](#9-cancellation-and-timeouts)
10. [Streaming](#10-streaming)
11. [Token accounting](#11-token-accounting)
12. [MCP integration](#12-mcp-integration)
13. [Chat bots](#13-chat-bots)
14. [Error handling](#14-error-handling)
15. [Security](#15-security)
16. [Testing your integration](#16-testing-your-integration)
17. [Troubleshooting and FAQ](#17-troubleshooting-and-faq)
18. [API cheat sheet](#18-api-cheat-sheet)

---

## 1. Installation and feature selection

Add the facade and pick only the backends you need. `core`, `runtime`, and
`agent` are always compiled; providers, MCP, and bots are opt-in.

```toml
[dependencies]
llmrc = { version = "0.1", features = ["openai", "mcp"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tokio-util = "0.7"
async-trait = "0.1"   # only if you implement Tool / ChatProvider yourself
serde_json = "1"       # only if you implement Tool
futures = "0.3"        # only if you consume streams
```

### Feature matrix

| Feature | Adds | Typical use |
| --- | --- | --- |
| *(none)* | `llmrc-core`, `llmrc-runtime`, `llmrc-agent` | Custom providers, retry only, agent with your own backend |
| `openai` | OpenAI chat/stream/embeddings/tools/multimodal | Hosted OpenAI |
| `azure` | Azure OpenAI (same crate, different client config) | Azure deployments |
| `ollama` | Ollama chat/stream/embeddings/tools/base64 images | Local models |
| `mcp` | MCP bridge + custom stdio/HTTP transports | Remote tool servers |
| `bots` | `BotHandler`, sessions, `split_message` | Any chat platform |
| `discord` | `bots` + `DiscordAdapter` | Discord |
| `telegram` | `bots` + `TelegramAdapter` | Telegram |
| `slack` | `bots` + `SlackAdapter` | Slack |
| `serenity` | Alias for `discord` | Naming symmetry |
| `teloxide` | Alias for `telegram` | Naming symmetry |
| `slack-morphism` | Alias for `slack` | Naming symmetry |

> **Note:** There is no `agent` feature. The agent is a mandatory dependency of
> the facade. `features = ["openai", "agent"]` will fail to resolve.

### Official `rmcp` transports

The `mcp` feature ships transport-agnostic bridges plus JSON-RPC stdio and
Streamable HTTP transports written in this workspace. To use the official
`rmcp` 3.x client instead, depend on the crate directly:

```toml
llmrc-mcp = { version = "0.1", features = ["rmcp"] }
```

and import `llmrc_mcp::rmcp_adapter::{StdioTransport, StreamableHttpTransport}`.

### MSRV

Rust 1.88+, edition 2024.

---

## 2. The mental model

```text
your app
   │
   ├─ constructs a ChatProvider ──────────────┐
   │                                          │
   ├─ registers Tools into a ToolRegistry ─┐  │
   │                                       │  │
   └─ builds an Agent ◄────────────────────┴──┘
         │  uses RetryPolicy for provider calls
         │  uses TokenCounter for accounting
         └─ run(prompt, CancellationToken) -> AgentResult
                                                │
                                    Message + history + events + usage
```

Everything is a value plus a token:

- Values in, values out — `ChatRequest`/`ChatResponse`, `Message`,
  `ToolDefinition`.
- A `tokio_util::sync::CancellationToken` on every long-running call.
- Limits on every loop: turns, tools, concurrency, deadline, history.

You can stop at any layer. Use `ChatProvider` alone for raw chat, add
`RetryPolicy` for resilience, add `ToolRegistry` for tool calling, or wrap the
whole thing in a `BotHandler` for multi-user serving.

---

## 3. Domain types and messages

All shared types live in `llmrc_core` and are re-exported from `llmrc`.

### Message construction

```rust
use llmrc::prelude::*;

let system = Message::system("You are a terse assistant.");
let user   = Message::user("What is 2 + 2?");
let assistant = Message::assistant("4");
let tool   = Message::tool("call_1", "4");           // requires tool_call_id
let custom = Message::new(MessageRole::User, "hi");  // explicit role
```

`Message` fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `role` | `MessageRole` | `System`, `User`, `Assistant`, `Tool` |
| `content` | `Vec<ContentPart>` | Text and/or image parts |
| `name` | `Option<String>` | Optional participant name |
| `tool_calls` | `Vec<ToolCall>` | Assistant-requested calls |
| `tool_call_id` | `Option<String>` | Required for `Tool` messages |

`Message::text()` concatenates all text parts and ignores images.

### Multimodal content

```rust
use llmrc::core::{ContentPart, ImageSource};

let message = Message {
    role: MessageRole::User,
    content: vec![
        ContentPart::text("What is in this image?"),
        ContentPart::Image {
            source: ImageSource::Base64 {
                media_type: "image/png".into(),
                data: "aGVsbG8=".into(),
            },
        },
        // or: ImageSource::Url { url: "https://…".into(), detail: None }
    ],
    name: None,
    tool_calls: vec![],
    tool_call_id: None,
};
```

Provider support differs:

- **OpenAI / Azure** — both `Url` and `Base64` (base64 is emitted as a
  `data:` URI).
- **Ollama** — `Base64` only. A `Url` image returns
  `LlmError::Unsupported`, because Ollama does not fetch remote images. Download
  the bytes yourself and pass base64.

### Chat request and response

```rust
let request = ChatRequest {
    model: "gpt-4o-mini".into(),
    messages: vec![Message::user("hello")],
    temperature: Some(0.2),
    max_tokens: Some(512),
    tools: vec![], // populated by the agent from the registry
};

let response: ChatResponse = provider.chat(request).await?;
println!("{}", response.message.text());
println!("finish: {:?}", response.finish_reason);
println!("usage:  {:?}", response.usage);
```

`FinishReason` is `Stop`, `Length`, `ToolCalls`, `ContentFilter`, or `Other`.
Note that OpenAI's legacy `FunctionCall` is normalized to `ToolCalls`.

### Stream events

`StreamEvent` is the streaming vocabulary:

| Variant | Meaning |
| --- | --- |
| `Started { id, model }` | Stream opened (emitted by both providers first) |
| `TextDelta { text }` | Incremental assistant text |
| `ToolCallDelta(ToolCallDelta)` | Incremental tool-call fragments |
| `Usage(TokenUsage)` | Provider-reported usage |
| `Completed { finish_reason }` | Terminal event |

Accumulate `ToolCallDelta` fragments by `index`; OpenAI sends the id and name
once and then argument fragments.

---

## 4. Providers

### OpenAI

```rust
use llmrc::openai::OpenAiProvider;

// Standard OpenAI
let provider = OpenAiProvider::openai(std::env::var("OPENAI_API_KEY")?);

// Compatible gateway / proxy
let provider = OpenAiProvider::openai_with_base_url(
    std::env::var("OPENAI_API_KEY")?,
    "https://my-gateway.example.com/v1",
);
```

### Azure OpenAI

```rust
use llmrc::openai::OpenAiProvider;

let provider = OpenAiProvider::azure(
    std::env::var("AZURE_OPENAI_API_KEY")?,
    "https://my-resource.openai.azure.com", // endpoint
    "my-gpt4o-deployment",                  // deployment id
    "2024-10-21",                           // api version
);
```

`ProviderKind` reports `AzureOpenAi`, so you can branch on
`provider.kind()` if needed.

### Ollama

```rust
use llmrc::ollama::OllamaProvider;

let provider = OllamaProvider::localhost();               // http://127.0.0.1:11434
let provider = OllamaProvider::new("http://gpu-box:11434")?; // remote
```

`OllamaProvider::new` returns `Result<_, LlmError>` and maps a bad URL to
`LlmError::Configuration`.

### Capabilities

Check before you rely on a feature:

```rust
let caps = provider.capabilities();
if !caps.streaming { /* fall back to chat() */ }
```

`ProviderCapabilities { streaming, embeddings, tool_calls, multimodal }`.

### Writing your own provider

Implement `ChatProvider` (and optionally `EmbeddingProvider`) and the whole
stack works with it:

```rust
use async_trait::async_trait;
use llmrc::core::{BoxChatStream, ChatProvider, ChatRequest, ChatResponse, LlmError};
use llmrc::core::{ProviderCapabilities, ProviderKind};

struct MyProvider { /* client, Secret<...> */ }

#[async_trait]
impl ChatProvider for MyProvider {
    fn kind(&self) -> ProviderKind { ProviderKind::Ollama }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            embeddings: false,
            tool_calls: true,
            multimodal: false,
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        // map request -> SDK call -> ChatResponse; map SDK errors -> LlmError
        todo!()
    }

    async fn chat_stream(&self, request: ChatRequest) -> Result<BoxChatStream, LlmError> {
        Err(LlmError::Unsupported("streaming not implemented".into()))
    }
}
```

Error mapping is the important part: return `LlmError::RateLimited`,
`Server`, `Transport`, or `Timeout` for transient failures so the retry policy
can act; return `Authentication`, `InvalidRequest`, or `Configuration` for
permanent ones. Fill `RateLimited { retry_after }` when you know the delay.

---

## 5. The agent loop

### Minimal usage

```rust
use llmrc::prelude::*;
use tokio_util::sync::CancellationToken;

let agent = Agent::new(provider).with_config(AgentConfig {
    model: "gpt-4o-mini".into(),
    ..Default::default()
});

let result = agent.run("Summarize the Rust borrow checker.", CancellationToken::new()).await?;
println!("{}", result.message.text());
```

### What `run` actually does

1. Load the session (`"default"` for `run`, or your id for `run_session`).
2. Append the user message.
3. Trim history to `max_history_messages`.
4. Loop up to `max_turns` times:
   - Cancel check → `AgentError::Cancelled`.
   - Build `ChatRequest` with full history and registry definitions.
   - Call `provider.chat` through `RetryPolicy::execute`.
   - Record usage (provider-reported, else counted).
   - If no tool calls: return `AgentResult`.
   - Else validate `max_tool_calls`, execute tools concurrently under the
     semaphore, append `Tool` messages in call order, trim history, repeat.
5. On success, save the session.
6. If `deadline` is set, the whole loop runs under `tokio::time::timeout`.

### Configuration reference

```rust
let config = AgentConfig {
    model: "gpt-4o".into(),                        // required
    max_turns: 16,                                 // provider round-trips
    max_tool_calls: 64,                            // total tools per run
    deadline: Some(Duration::from_secs(60)),       // whole-run wall clock
    tool_timeout: Some(Duration::from_secs(30)),   // per tool
    max_concurrency: 4,                            // parallel tools
    max_history_messages: 100,                     // history cap
    retry_policy: RetryPolicy::builder()
        .max_retries(3)
        .base_delay(Duration::from_millis(200))
        .build(),
};
```

### Builder methods

| Method | Effect |
| --- | --- |
| `Agent::new(provider)` | Owns a provider by value |
| `Agent::from_shared(Arc<dyn ChatProvider>)` | Share one provider across agents |
| `.with_config(cfg)` | Replace configuration |
| `.with_registry(registry)` | Register tools |
| `.with_store(store)` | Replace the session store |
| `.with_token_counter(counter)` | Replace token accounting |
| `.run(input, token)` | Session `"default"` |
| `.run_session(id, input, token)` | Explicit session id |

### Reading the result

`AgentResult` carries everything you need for observability:

| Field | Type | Meaning |
| --- | --- | --- |
| `message` | `Message` | Final assistant message |
| `history` | `Vec<Message>` | Full conversation after the run |
| `events` | `Vec<AgentEvent>` | Lifecycle trace |
| `usage` | `TokenAccounting` | Prompt/completion totals |
| `turns` | `u32` | Turns consumed |
| `tool_calls` | `u32` | Tools executed |

`AgentEvent` variants: `TurnStarted`, `ProviderCompleted`, `ToolStarted`,
`ToolCompleted { is_error }`, `Completed`, `Cancelled`. Log these to build
per-turn traces without re-deriving them.

### Sharing an agent

`Agent` is not `Clone`, but `Arc<Agent>` is the intended sharing pattern,
especially for `BotHandler`:

```rust
let agent = Arc::new(Agent::new(provider).with_config(config));
```

---

## 6. Tools

### Implementing a tool

```rust
use async_trait::async_trait;
use llmrc::prelude::*;
use serde_json::Value;

struct WeatherTool;

#[async_trait]
impl Tool for WeatherTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_weather".into(),
            description: Some("Look up current weather for a city".into()),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["city"],
                "properties": {
                    "city": { "type": "string", "description": "City name" },
                    "units": { "type": "string" }
                },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<String, ToolError> {
        let city = args.get("city").and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArguments("city is required".into()))?;
        // call your service…
        Ok(format!("18°C and clear in {city}"))
    }
}
```

### Registering

```rust
let mut registry = ToolRegistry::new();
registry.register(WeatherTool)?;                        // Result<(), AgentError>
assert!(registry.get("get_weather").is_some());
let defs = registry.definitions();                       // Vec<ToolDefinition>
let agent = Agent::new(provider).with_registry(registry);
```

Rules enforced at registration:

- Names: 1–64 characters, ASCII alphanumeric, `_`, or `-`.
- Duplicates are rejected with `AgentError::Configuration`.

### Argument validation

Before a tool executes, the agent validates the model's arguments against the
declared schema. Supported keywords:

- `type` — `object`, `array`, `string`, `number`, `integer`, `boolean`,
  `null` (unknown types pass through).
- `required` — must be present as keys.
- `properties` — validated recursively; nested errors are prefixed with the
  field name.
- `additionalProperties: false` — unknown keys are rejected.

A schema of `null` or `{}` disables validation for that tool. This is a
practical subset, not a full JSON Schema implementation: no `oneOf`, `enum`,
`pattern`, or numeric bounds. If you need stricter checks, validate inside
`execute` and return `ToolError::InvalidArguments`.

### Failure semantics

Any `ToolError` becomes a `Tool` message rather than aborting the run:

| `ToolError` | Message sent to the model |
| --- | --- |
| `Unknown` | `unknown tool` |
| `InvalidArguments(reason)` | `invalid arguments: {reason}` |
| `Timeout` | `tool timed out` |
| `Cancelled` | `tool cancelled` |
| `Failed` | `tool failed` |
| `FailedWithMessage(msg)` | `tool failed: {msg}` |

This lets the model recover (for example, retry with fixed arguments) instead
of failing the whole conversation.

### Concurrency, ordering, and timeouts

- All calls in a batch are launched together under a semaphore of
  `max_concurrency`.
- Results are appended **in the model's call order**, regardless of completion
  order — so history stays coherent.
- Each call is wrapped in `tool_timeout` (default 30 s).
- If a batch would push the run past `max_tool_calls`, the agent returns
  `AgentError::MaxToolCalls` **before executing anything**.

### Cancellable tools

Override `execute_with_cancellation` for tools that do their own I/O so the
token reaches your transport:

```rust
async fn execute_with_cancellation(
    &self,
    args: Value,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<String, ToolError> {
    tokio::select! {
        _ = cancellation.cancelled() => Err(ToolError::Cancelled),
        result = self.execute(args) => result,
    }
}
```

The default implementation already does exactly this around `execute`.

---

## 7. Sessions and history

Sessions are `{ id: String, history: Vec<Message> }` and live behind a
`SessionStore`:

```rust
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn load(&self, id: &str) -> Result<Option<Session>, AgentError>;
    async fn save(&self, session: Session) -> Result<(), AgentError>;
}
```

`InMemorySessionStore` is the default and is process-local. Implement your own
for persistence:

```rust
struct RedisStore { /* ... */ }

#[async_trait::async_trait]
impl SessionStore for RedisStore {
    async fn load(&self, id: &str) -> Result<Option<Session>, AgentError> {
        // deserialize from your backend
        todo!()
    }
    async fn save(&self, session: Session) -> Result<(), AgentError> {
        // serialize to your backend
        todo!()
    }
}

let agent = agent.with_store(RedisStore { /* ... */ });
```

Behavior notes:

- `run` uses session id `"default"`; always use `run_session` in multi-user
  code so conversations do not collide.
- Sessions are saved **only when the run succeeds**. A failed or cancelled run
  leaves the stored session untouched.
- `AgentError::Storage` wraps store failures.

### History trimming

When `history.len() > max_history_messages`, the oldest **non-system** messages
are dropped first; system messages are only dropped once non-system messages
are exhausted. `max_history_messages = 0` clears history.

> **Caveat:** trimming is positional, not tool-aware. If a trim boundary lands
> between an assistant `tool_calls` message and its `Tool` results, the results
> can be orphaned. Set `max_history_messages` comfortably larger than one full
> tool round (assistant + all tool messages), or use a store/trim strategy that
> keeps rounds intact.

---

## 8. Retries, jitter, and budgets

```rust
use llmrc::runtime::{Jitter, RetryBudget};
use std::time::Duration;

let budget = RetryBudget::new(5);           // shared across calls

let policy = RetryPolicy::builder()
    .max_retries(3)                          // up to 3 additional attempts
    .base_delay(Duration::from_millis(200))  // first backoff
    .max_delay(Duration::from_secs(20))      // cap
    .jitter(Jitter::Equal)                   // default
    .budget(budget.clone())
    .classifier(|error| error.is_retryable())
    .build();
```

### What gets retried

Default classifier = `LlmError::is_retryable()` = `true` only for
`RateLimited`, `Server`, `Transport`, `Timeout`. Authentication,
InvalidRequest, Configuration, Decode, Unsupported, Provider, and Cancelled
are **not** retried.

### Delay computation

For retry number `n` (1-based):

1. `exponential = min(base_delay * 2^(n-1), max_delay)`.
2. Apply jitter:
   - `None` → `exponential`
   - `Full` → random in `[0, exponential]`
   - `Equal` → random in `[exponential/2, exponential]`
3. If the error is `RateLimited { retry_after: Some(d) }`, use `min(d, max_delay)`
   instead.
4. Cap at `max_delay`.

### Budgets

`RetryBudget` is an `Arc<AtomicU32>` under the hood. Cloning shares the
allowance, so a fleet-wide budget prevents retry storms:

```rust
let budget = RetryBudget::new(10);
let policy_a = RetryPolicy::builder().budget(budget.clone()).build();
let policy_b = RetryPolicy::builder().budget(budget.clone()).build();
// A and B draw from the same pool of 10 retries.
budget.remaining(); // inspect at any time
```

A retry only happens if the budget can be decremented; otherwise the error is
returned.

### Executing operations with retries

```rust
let result = policy.execute(
    || async { provider.chat(request.clone()).await },
    &cancellation,
).await?;
```

The closure returns a fresh future each attempt, so the operation is
re-issuable. Cancellation is checked before, during, and between attempts —
including during the backoff sleep.

`RetryStats` exists in the public API for bookkeeping but is not yet populated
by the runtime; track attempts yourself if you need them.

---

## 9. Cancellation and timeouts

Cancellation is universal in `llmrc`. The `CancellationToken` is observed at
every boundary:

| Layer | How it cancels |
| --- | --- |
| `RetryPolicy::execute` / `collect_stream` | `select!` around the attempt and the backoff sleep |
| `Agent::run` / `run_session` | Checked each turn; mapped to `AgentError::Cancelled` |
| `Tool::execute_with_cancellation` | `select!` around `execute` |
| Agent tool execution | `select!` around the semaphore wait and the tool future |
| `McpTransport::list_tools` / `call_tool` | `select!` around the request |
| `BotHandler::handle` | Combines caller token + shutdown token |

### Common patterns

**User pressed stop:**

```rust
let token = CancellationToken::new();
let child = token.clone();
tokio::spawn(async move {
    // ... later
    child.cancel();
});
let result = agent.run("…", token).await; // Err(AgentError::Cancelled)
```

**Graceful shutdown:**

```rust
let handler = Arc::new(BotHandler::new(agent));
handler.shutdown();                 // future handles return BotError::Shutdown
handler.cancellation_token();       // propagates into agent runs
```

### Timeouts versus deadlines

- `tool_timeout` — per tool call; a timeout becomes a `tool timed out` tool
  message, and the loop continues.
- `deadline` — the entire run; expiry returns `AgentError::Deadline` and the
  session is not saved.

Both are independent: a run can hit many tool timeouts before its deadline.

---

## 10. Streaming

Streaming bypasses the agent loop and uses `ChatProvider::chat_stream`
directly. When you must survive transient failures, wrap it with
`RetryPolicy::collect_stream`.

### Consuming a stream

```rust
use futures::StreamExt;
use llmrc::core::StreamEvent;

let mut stream = provider.chat_stream(request).await?;
while let Some(event) = stream.next().await {
    match event? {
        StreamEvent::Started { model, .. } => println!("[{model}]"),
        StreamEvent::TextDelta { text } => print!("{text}"),
        StreamEvent::ToolCallDelta(delta) => { /* accumulate by delta.index */ }
        StreamEvent::Usage(usage) => println!("\nusage: {} tokens", usage.total_tokens),
        StreamEvent::Completed { finish_reason } => println!("\ndone: {finish_reason:?}"),
    }
}
```

### Safe retry of a stream

```rust
let chunks = policy.collect_stream(
    || async { provider.chat_stream(request.clone()).await },
    &cancellation,
    |event| matches!(event, StreamEvent::TextDelta { .. }), // user-visible predicate
).await?;
```

Semantics that matter:

- Errors **before** any visible item can trigger a fresh attempt.
- Once the predicate returns `true`, the source is never restarted. Any later
  error is surfaced immediately.
- The visibility predicate is yours because metadata (like `Usage`) may be
  internal while text is user-visible. Choose the boundary deliberately to
  avoid duplicated output.

### Accumulating tool calls

OpenAI streams tool calls as fragments: the first delta carries `id` and
`name`, later deltas carry argument JSON pieces, all keyed by `index`. Buffer
per index until `Completed`, then parse the concatenated arguments.

---

## 11. Token accounting

Every count is tagged with provenance via `TokenCount`:

| Variant | Meaning | Trust |
| --- | --- | --- |
| `Exact` | A real tokenizer computed it | Billing-grade |
| `ProviderReported` | The provider's usage field | Billing-grade |
| `Estimated` | A heuristic | Planning only |

### Counters

```rust
use llmrc::runtime::{ExactTokenCounter, HeuristicTokenCounter, ProviderReportedTokenCounter};

// Built-in heuristic: chars / 4 (1 for any non-empty text)
let counter = HeuristicTokenCounter;

// Bring your own tokenizer
let counter = ExactTokenCounter::new(|text| tiktoken_count(text));

// Reuse a provider's reported usage
let counter = ProviderReportedTokenCounter::new(usage);
```

Pass a counter to the agent:

```rust
let agent = Agent::new(provider).with_token_counter(HeuristicTokenCounter);
```

The agent prefers `response.usage` from the provider; only when a provider
omits usage does it estimate prompt and completion tokens with the configured
counter.

### Accumulating

```rust
use llmrc::runtime::TokenAccounting;

let mut accounting = TokenAccounting::default();
accounting.record_prompt(counter.count_text("prompt text"));
accounting.record_completion(counter.count_text("completion text"));
accounting.record_usage(&provider_reported);   // also supported
accounting.total();                            // prompt + completion
```

Provenance merging is conservative: if any contribution is `Estimated`, the
merged count is `Estimated`. Mixing `ProviderReported` and `Exact` yields
`ProviderReported`.

### Privacy rule

`TokenCounter` implementations **must not** log, retain, or embed prompt text
in `Debug` output. `ExactTokenCounter` and `HeuristicTokenCounter` are already
safe. If you write your own, keep the same guarantee.

---

## 12. MCP integration

MCP (Model Context Protocol) servers expose tools over JSON-RPC. `llmrc`
bridges them into the same `ToolRegistry` the agent already uses.

### The pieces

| Type | Role |
| --- | --- |
| `McpTransport` | Trait: `list_tools` + `call_tool` (both cancellation-aware) |
| `McpBridge` | Binds one server to a transport; namespaces and caches tools |
| `McpPool` | Manages many bridges, refreshes all at once |
| `McpExecutableTool` | A `Tool` impl that calls back through the transport |
| `StdioTransport` | Child-process JSON-RPC transport |
| `StreamableHttpTransport` | HTTP JSON-RPC transport |

### Stdio server

```rust
use llmrc::mcp::{McpBridge, StdioTransport};
use std::time::Duration;

let transport = StdioTransport::spawn("npx", &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]).await?;
let bridge = McpBridge::new("filesystem", transport)
    .with_timeout(Some(Duration::from_secs(20)));

bridge.refresh().await?;                  // fetch tools/list; must be called first
let mut registry = llmrc::ToolRegistry::new();
bridge.register_into(&mut registry).await?;
```

### HTTP server

```rust
use llmrc::mcp::{McpBridge, StreamableHttpTransport};

let transport = StreamableHttpTransport::new("https://mcp.example.com/mcp")?
    .with_header("Authorization", "Bearer …");

let bridge = McpBridge::new("remote", transport);
bridge.refresh().await?;
```

### Naming and namespacing

Remote tool `search` on server `docs.server` becomes
`docs_server__search`: the server name is sanitized (non-alphanumeric → `_`),
then joined with the tool name by `__`, then truncated to 64 characters. That
keeps every name legal for provider function schemas and collision-free across
servers.

### Pooling servers

```rust
use llmrc::mcp::McpPool;

let pool = McpPool::new();
pool.add(McpBridge::new("filesystem", fs_transport)).await;
pool.add(McpBridge::new("search", search_transport)).await;

let all_definitions = pool.refresh().await?;     // refresh every bridge
let mut registry = llmrc::ToolRegistry::new();
pool.register_into(&mut registry).await?;
```

`McpPool` also answers to the aliases `McpRegistry` and `McpServerRegistry`.

### Operational notes

- `McpBridge` caches the tool list; call `refresh()` on start and whenever the
  server's tool set changes.
- Every bridge call is wrapped in the bridge `timeout` (default 30 s) and is
  cancellation-aware.
- `StdioTransport` serializes requests with a lock (JSON-RPC stdio is not
  multiplexed) and kills the child process on drop.
- `StreamableHttpTransport` expects a JSON-RPC response body; it does not
  consume an SSE stream.
- `McpError` variants: `Transport`, `Protocol`, `Timeout`, `Cancelled`,
  `UnknownTool`, `Tool`. These map onto `ToolError` before reaching the model,
  so MCP failures appear as ordinary tool failures.
- Official `rmcp` adapters live behind the `rmcp` feature of `llmrc-mcp`.

---

## 13. Chat bots

`BotHandler` turns one `Agent` into a per-conversation service.

### Setup

```rust
use std::sync::Arc;
use llmrc::bots::{BotEvent, BotHandler, ConversationKey};
use tokio_util::sync::CancellationToken;

let agent = Arc::new(Agent::new(provider).with_config(AgentConfig {
    model: "gpt-4o-mini".into(),
    ..Default::default()
}));

let handler = BotHandler::new(agent)
    .with_max_response_bytes(2_000);   // default
```

### Handling an event

```rust
let event = BotEvent::new(
    ConversationKey::new("discord", "channel-123"),
    "hello bot",
);

let responses: Vec<BotResponse> =
    handler.handle(event, CancellationToken::new()).await?;
for response in responses {
    println!("{}", response.text);
}
```

### What `handle` guarantees

1. **Shutdown check** — a cancelled handler returns `BotError::Shutdown`.
2. **Per-conversation lock** — keyed on `ConversationKey { platform, id }`.
   Turns in the same conversation are serialized; different conversations run
   concurrently.
3. **Stable storage key** — `format!("{platform}:{id}")` is used as the agent
   session id, so each conversation keeps its own history.
4. **Bounded output** — the reply is split with `split_message`, each part at
   most `max_response_bytes`, and returned as separate `BotResponse`s with
   `reply_to` echoing the source message id.
5. **Session accounting** — `BotSession.messages_seen` increments per handled
   event.

### Message splitting

```rust
use llmrc::bots::split_message;

let parts = split_message("long reply 🌍 …", 2_000)?; // each part ≤ 2000 bytes
```

Guarantees: UTF-8 safe (never splits mid-character), bounded, prefers a
newline then whitespace break point, and rejects `max_bytes == 0` with
`BotError::InvalidSplitLimit`. An empty string yields a single empty part.

### Platform adapters

Thin mappers convert native identifiers into `BotEvent` and back:

```rust
use llmrc::discord::DiscordAdapter;
use llmrc::telegram::TelegramAdapter;
use llmrc::slack::SlackAdapter;

let event = DiscordAdapter::event("channel-123", Some("alice".into()), "hi");
let text  = DiscordAdapter::response_text(&response);
```

Each adapter produces a `ConversationKey` with its own platform tag
(`"discord"`, `"telegram"`, `"slack"`), which is exactly what keeps sessions
isolated when you serve several platforms from one handler.

### Custom session store

```rust
let handler = BotHandler::new(agent).with_session_store(MyBotStore::new());
```

`BotSessionStore` mirrors `SessionStore`: `load(&ConversationKey)` and
`save(BotSession)`.

### Implementing a platform

Implement `PlatformAdapter` to bind a native library:

```rust
#[async_trait::async_trait]
impl PlatformAdapter for MyAdapter {
    type Event = MyNativeEvent;
    type Error = MyError;

    fn to_event(&self, event: Self::Event) -> Result<BotEvent, Self::Error> { todo!() }
    async fn send(&self, response: BotResponse) -> Result<(), Self::Error> { todo!() }
}
```

> The platform features are mappers, not gateways. `llmrc` does not open a
> Discord/Telegram/Slack connection; your application drives `serenity`,
> `teloxide`, or `slack-morphism` and calls `handler.handle`.

---

## 14. Error handling

`llmrc` uses typed errors everywhere. Match on specificity, not strings.

### `LlmError` (provider layer)

| Variant | `ErrorKind` | Retryable | Typical cause |
| --- | --- | --- | --- |
| `InvalidRequest(String)` | `InvalidRequest` | no | Bad parameters, missing tool_call_id |
| `Configuration(String)` | `Configuration` | no | Bad base URL, missing key |
| `Authentication` | `Authentication` | no | 401/403 |
| `RateLimited { retry_after }` | `RateLimited` | **yes** | 429 |
| `Server(String)` | `Server` | **yes** | 5xx |
| `Transport(..)` | `Transport` | **yes** | Network/socket failure |
| `Decode(..)` | `Decode` | no | Malformed provider payload |
| `Cancelled` | `Cancelled` | no | Token cancelled |
| `Timeout` | `Timeout` | **yes** | Deadline elapsed |
| `Unsupported(String)` | `Unsupported` | no | Feature not available |
| `Provider(String)` | `Provider` | no | Everything else |

`LlmError` is `#[non_exhaustive]`; always keep a wildcard arm.

### `AgentError` (agent layer)

`Configuration`, `Provider(LlmError)`, `UnknownTool`, `InvalidToolArguments`,
`MaxTurns`, `MaxToolCalls`, `Deadline`, `Cancelled`, `Storage`.

### `ToolError` / `McpError` / `BotError`

- `ToolError` describes a single tool call and is converted into model-visible
  text, never propagated out of the agent loop.
- `McpError` is mapped onto `ToolError` at the bridge boundary.
- `BotError` covers `Shutdown`, `Cancelled`, `Agent(AgentError)`, `Storage`,
  `InvalidSplitLimit`.

### Handling pattern

```rust
use llmrc::core::LlmError;

match agent.run(prompt, token).await {
    Ok(result) => { /* use result */ }
    Err(AgentError::Cancelled) => { /* user aborted; keep session as-is */ }
    Err(AgentError::Deadline) => { /* raise your own SLA alarm */ }
    Err(AgentError::MaxTurns) | Err(AgentError::MaxToolCalls) => { /* tighten prompt or raise limits */ }
    Err(AgentError::Provider(LlmError::Authentication)) => { /* refresh credentials */ }
    Err(AgentError::Provider(LlmError::RateLimited { retry_after })) => { /* back off; retry policy may already have */ }
    Err(error) => { /* log and surface */ }
}
```

### Error mapping from SDKs

Adapters normalize SDK errors:

- **OpenAI**: 401/403 → `Authentication`, 429 → `RateLimited`, 5xx →
  `Server`, reqwest → `Transport`, JSON → `Decode`, otherwise `Provider`.
- **Ollama**: JSON → `Decode`, reqwest → `Transport`, internal → `Server`,
  otherwise `Provider`.

Write the same mapping for custom providers so the retry classifier keeps
working.

---

## 15. Security

The workspace treats credentials and prompt content as sensitive by design.

1. **Use `Secret` for credentials.**

   ```rust
   use llmrc::core::Secret;

   let key = Secret::new(std::env::var("OPENAI_API_KEY")?);
   // Debug prints "Secret(REDACTED)"
   // Reach the raw value only at the network boundary:
   let raw = key.expose();
   ```

   `Secret` deliberately has no `Display` and redacts its `Debug` output. The
   built-in providers currently take a plain `String`; wrap it in `Secret` in
   your configuration layer and call `.expose()` when constructing the
   provider, so it never lands in a log.

2. **Never log prompts or completions.** `TokenCounter` implementations must
   count without retaining or printing text. Telemetry should record counts,
   turn ids, and error kinds — not content.

3. **Bound everything.** `max_turns`, `max_tool_calls`, `max_concurrency`,
   `deadline`, `tool_timeout`, and `max_history_messages` exist to stop
   runaway loops and memory growth. Keep them set in production.

4. **Treat tool output as untrusted.** Tool results are fed back to the model.
   Validate arguments (the agent does a schema pass), and sanitize anything
   you echo into a UI.

5. **Treat MCP servers as untrusted.** A stdio MCP server runs arbitrary code
   locally; an HTTP MCP server is a remote dependency. Set explicit timeouts
   and audit which servers you register.

6. **Report vulnerabilities privately** per [SECURITY.md](SECURITY.md).

---

## 16. Testing your integration

The workspace tests are offline and credential-free — follow that pattern.

### A fake provider

```rust
use async_trait::async_trait;
use llmrc::core::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FakeProvider { calls: AtomicUsize }

#[async_trait]
impl ChatProvider for FakeProvider {
    fn kind(&self) -> ProviderKind { ProviderKind::Ollama }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: false, embeddings: false, tool_calls: true, multimodal: false,
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ChatResponse {
            id: None,
            model: request.model,
            message: Message::assistant("ok"),
            finish_reason: Some(FinishReason::Stop),
            usage: None,
        })
    }

    async fn chat_stream(&self, _: ChatRequest) -> Result<BoxChatStream, LlmError> {
        Err(LlmError::Unsupported("not implemented".into()))
    }
}
```

### Test recipes

- **Tool loop** — return a `ToolCalls` response on turn 1 and `Stop` on turn 2,
  then assert on `result.history` and `result.tool_calls` (see
  `examples/02_custom_tools.rs`).
- **Limits** — set `max_turns: 1` or `max_tool_calls: 1` and assert
  `AgentError::MaxTurns` / `MaxToolCalls`.
- **Cancellation** — pass a pre-cancelled token and assert
  `AgentError::Cancelled`.
- **Retry** — fail N times with `LlmError::Timeout`, assert the operation
  succeeds and the budget is drained.
- **Stream safety** — emit a visible item then an error, and assert the
  operation was invoked exactly once.
- **Splitting** — assert every part is `<= max_bytes` and the parts rejoin to
  the original.
- **MCP** — implement `McpTransport` with a fake and assert namespacing,
  caching (`list_tools` called once), and pool isolation.

### Quality gates before you ship

```bash
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

---

## 17. Troubleshooting and FAQ

**`the package 'llmrc' does not contain this feature: agent`**
There is no `agent` feature — the agent is always compiled. Use
`features = ["openai", "mcp"]` or similar.

**`AgentError::Configuration("tool names must be …")`**
Tool names must be 1–64 ASCII alphanumerics, `_`, or `-`. MCP names are
sanitized and namespaced automatically.

**`duplicate tool`**
Two registered tools share a name — including two MCP bridges whose namespaces
collide. Rename a server or tool.

**The model never calls my tool.**
The tool must be in the registry *before* the run, and its
`ToolDefinition.parameters` should be a real object schema with `required`
fields. Also check `provider.capabilities().tool_calls`.

**`LlmError::Unsupported("Ollama accepts base64 images…")`**
Ollama cannot fetch remote image URLs. Download the bytes and send
`ImageSource::Base64`.

**`AgentError::MaxTurns` on a tool-using prompt.**
Each model ↔ tool round trip is a turn. Raise `max_turns` (or `max_tool_calls`)
for multi-step tasks.

**My reply got cut off.**
`BotHandler` splits at `max_response_bytes`. Raise the limit or join the
returned parts when assembling output.

**Replies appear twice after a retry.**
Only retry streams through `RetryPolicy::collect_stream` with a correct
visibility predicate. Never restart a stream after user-visible text has been
emitted.

**Streaming doesn't work through `Agent`.**
Correct — the agent uses `chat`. Use `provider.chat_stream` or
`collect_stream` directly.

**Session history grows unexpectedly.**
Sessions persist per id. Use distinct `run_session` ids per user, or a
persistent `SessionStore` with your own retention policy.

**Why is my token count an estimate?**
Without provider usage, the agent falls back to `HeuristicTokenCounter`
(`chars / 4`). Use `ExactTokenCounter` with a real tokenizer for
billing-grade numbers. Check `TokenAccounting`'s count type to know which you
have.

**Retries never happen.**
Check `LlmError::is_retryable()` for the error, `max_retries > 0`, and a
non-exhausted `RetryBudget`.

**MCP tools return `unknown tool`.**
Call `bridge.refresh()` (or `pool.refresh()`) before registering, and use the
namespaced name (`server__tool`) when addressing tools.

---

## 18. API cheat sheet

```rust
// Provider
OpenAiProvider::openai(key)
OpenAiProvider::openai_with_base_url(key, base)
OpenAiProvider::azure(key, endpoint, deployment, version)
OllamaProvider::localhost()
OllamaProvider::new(base_url)?

// Chat
provider.kind() / .capabilities()
provider.chat(request).await?
provider.chat_stream(request).await?
provider.embed(EmbeddingRequest { model, input }).await?

// Agent
Agent::new(provider)
Agent::from_shared(Arc<dyn ChatProvider>)
.with_config(AgentConfig { ..Default::default() })
.with_registry(ToolRegistry)
.with_store(SessionStore)
.with_token_counter(TokenCounter)
.run(input, cancellation).await?
.run_session(id, input, cancellation).await?

// Tools
#[async_trait] impl Tool { definition(), execute() }
ToolRegistry::new().register(tool)?
registry.get(name) / .definitions()

// Retry
RetryPolicy::builder().max_retries(..).base_delay(..).max_delay(..)
    .jitter(Jitter::Equal).budget(RetryBudget::new(..)).build()
policy.execute(|| async { .. }, &cancellation).await?
policy.collect_stream(|| async { .. }, &cancellation, |item| ..).await?
policy.delay_for(retry_number, &error)

// Tokens
HeuristicTokenCounter / ExactTokenCounter::new(f) / ProviderReportedTokenCounter::new(usage)
counter.count_text(s) / .count_messages(&msgs, &tools) / .count_request(&req)
TokenAccounting::default().record_prompt(..).record_completion(..).record_usage(..)

// Messages
Message::system(..) / ::user(..) / ::assistant(..) / ::tool(id, ..)
ContentPart::text(..) / ContentPart::Image { source }

// MCP
McpBridge::new(server, transport).with_timeout(Some(dur))
bridge.refresh().await? / .definitions().await / .register_into(&mut registry).await?
McpPool::new().add(bridge).await / .remove(name).await / .refresh().await?
StdioTransport::spawn(program, args).await?
StreamableHttpTransport::new(endpoint)?.with_header(name, value)

// Bots
BotHandler::new(Arc<Agent>)
    .with_max_response_bytes(n)
    .with_session_store(store)
handler.handle(event, cancellation).await? -> Vec<BotResponse>
handler.shutdown() / .cancellation_token()
split_message(text, max_bytes)?
DiscordAdapter::event(id, author, text) / .response_text(&resp)
TelegramAdapter::event(..) / SlackAdapter::event(..)

// Errors
LlmError::kind() / .is_retryable()
AgentError / ToolError / McpError / BotError
```
