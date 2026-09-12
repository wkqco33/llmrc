# llmrc

[![CI](https://github.com/wkqco33/llmrc/actions/workflows/ci.yml/badge.svg)](https://github.com/wkqco33/llmrc/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/llmrc.svg)](https://crates.io/crates/llmrc)
[![docs.rs](https://docs.rs/llmrc/badge.svg)](https://docs.rs/llmrc)

예측 가능하고 모듈화된 Rust용 LLM 런타임 파사드(facade).

`llmrc`는 채팅, 임베딩, 도구 호출, 재시도, 토큰 집계, MCP 도구, 채팅 플랫폼 봇을
하나의 타입 안전한 API로 제공합니다. 동시에 모든 provider SDK는 선택적 기능
플래그(feature flag) 뒤에 숨겨져 있습니다. 이 런타임은 **무한정 실행되는 것이
없도록** 설계되었습니다. 모든 호출에는 턴 제한, 도구 제한, 동시성 상한, 타임아웃,
그리고 취소 토큰이 있습니다.

- **Provider 독립적** — 애플리케이션 코드는 `async-openai`나 `ollama-rs`가 아니라
  `ChatProvider`와 대화합니다.
- **취소 우선** — 모든 비동기 진입점이
  `tokio_util::sync::CancellationToken`을 받아 처리 중인 작업을 즉시 중단합니다.
- **설계부터 유한함** — 턴, 도구, 동시성, 데드라인, 히스토리 제한이 부가 기능이
  아니라 명시적 설정입니다.
- **비밀 정보 안전** — 자격 증명은 `llmrc_core::Secret`에 보관되며, `Debug`에서
  스스로를 마스킹하고 `Display`를 절대 구현하지 않습니다.
- **기능 게이트** — SDK와 플랫폼 어댑터는 요청했을 때만 컴파일됩니다.

> 영문 문서: [README.md](./README.md) · 상세 가이드: [GUIDE.ko.md](GUIDE.ko.md)

---

## 상태

`0.1.0`, 안정화 이전(pre-stable) 버전입니다. 공개 API는 첫 안정 릴리스 전에 변경될
수 있습니다. 워크스페이스 전체가 오프라인으로 빌드·테스트됩니다. **29개의 단위
테스트가 통과하고, `clippy -D warnings`가 깨끗하며, `rustfmt`는 차이를 보고하지
않습니다.** 테스트 스위트에는 provider나 봇 자격 증명이 필요하지 않습니다.

| 게이트 | 명령 | 현재 상태 |
| --- | --- | --- |
| 포맷 | `cargo fmt --all -- --check` | 통과 |
| 검사 | `cargo check --workspace --all-features` | 통과 |
| 테스트 | `cargo test --workspace --all-features` | 29개 통과 |
| 린트 | `cargo clippy --workspace --all-features --all-targets -- -D warnings` | 통과 |

---

## 아키텍처

```text
llmrc (workspace root facade)
├── crates/core               # 도메인 타입 + 비동기 계약 (provider SDK 없음)
├── crates/runtime            # RetryPolicy, 지터, 예산, 토큰 집계
├── crates/agent              # 유한한 도구 사용 에이전트 루프, 세션, Tool 트레이트
├── crates/mcp                # MCP 브리지 + stdio / Streamable HTTP 전송
├── crates/bots-core          # 플랫폼 중립 BotHandler, 세션별 잠금
├── crates/bots-discord       # Discord 이벤트 매핑
├── crates/bots-telegram      # Telegram 이벤트 매핑
├── crates/bots-slack         # Slack 이벤트 매핑
├── crates/provider-openai    # async-openai 어댑터 (OpenAI + Azure)
└── crates/provider-ollama    # ollama-rs 어댑터
```

의존성 방향은 엄격하게 단방향입니다.

```text
provider-* ─┐
bots-* ─────┼─► agent ──► runtime ──► core
mcp ────────┘                │
                             └─► core
```

`core`는 provider SDK에 의존하지 않습니다. `runtime`은 `core`와
`futures`/`tokio`/`tokio-util`에만 의존합니다. `agent`는 `core + runtime` 위에
쌓입니다. provider, MCP, 봇 크레이트는 선택 사항이며 루트 `llmrc` 파사드에서 기능
게이트됩니다.

---

## 크레이트 레퍼런스

| 크레이트 | 역할 | 주요 공개 API |
| --- | --- | --- |
| `llmrc` | 파사드, 기능 게이트 재수출, `prelude` | `Agent`, `Message`, `RetryPolicy`, `OpenAiProvider`, `OllamaProvider`, `mcp::*`, `bots::*` |
| `llmrc-core` | Provider 독립 도메인 타입과 트레이트 | `Message`, `MessageRole`, `ContentPart`, `ImageSource`, `ChatRequest`, `ChatResponse`, `StreamEvent`, `ToolDefinition`, `ToolCall`, `TokenUsage`, `LlmError`, `ErrorKind`, `Secret`, `ChatProvider`, `EmbeddingProvider` |
| `llmrc-runtime` | 재시도 정책과 토큰 집계 | `RetryPolicy`, `RetryPolicyBuilder`, `RetryBudget`, `Jitter`, `RetryClassifier`; `TokenCounter`, `HeuristicTokenCounter`, `ExactTokenCounter`, `ProviderReportedTokenCounter`, `TokenAccounting` |
| `llmrc-agent` | 유한한 에이전트 루프, 도구, 세션 | `Agent`, `AgentConfig`, `AgentEvent`, `AgentResult`, `AgentError`, `Tool`, `ToolRegistry`, `ToolError`, `Session`, `SessionStore`, `InMemorySessionStore` |
| `llmrc-openai` | `async-openai` 기반 OpenAI / Azure OpenAI | `OpenAiProvider::openai`, `::openai_with_base_url`, `::azure`; `EmbeddingProvider` |
| `llmrc-ollama` | `ollama-rs` 기반 로컬/원격 Ollama | `OllamaProvider::localhost`, `::new(base_url)`; `EmbeddingProvider` |
| `llmrc-mcp` | MCP JSON-RPC 브리지와 전송 | `McpBridge`, `McpPool`, `McpTransport`, `McpExecutableTool`, `StdioTransport`, `StreamableHttpTransport`, `rmcp_adapter` |
| `llmrc-bots-core` | 대화 처리와 메시지 분할 | `BotHandler`, `BotEvent`, `BotResponse`, `ConversationKey`, `ConversationId`, `BotSessionStore`, `split_message`, `PlatformAdapter`, `BotError` |
| `llmrc-bots-{discord,telegram,slack}` | 네이티브 이벤트 ↔ `BotEvent` 매핑 | `DiscordAdapter`, `TelegramAdapter`, `SlackAdapter` |

---

## 기능 선택

```toml
[dependencies]
# provider + MCP 브리지만:
llmrc = { version = "0.1", features = ["openai", "mcp"] }
```

| 기능 | 활성화 대상 | 비고 |
| --- | --- | --- |
| *(없음)* | `core`, `runtime`, `agent` | 항상 컴파일됨. 에이전트 루프는 선택 사항이 아님 |
| `openai` | `llmrc-openai` | OpenAI 채팅, 스트리밍, 임베딩, 도구, 멀티모달 |
| `azure` | `llmrc-openai` | Azure OpenAI 배포 설정 |
| `ollama` | `llmrc-ollama` | Ollama 채팅, 스트리밍, 임베딩, 도구, base64 이미지 |
| `mcp` | `llmrc-mcp` | 전송 독립 MCP 브리지와 내장 전송 |
| `bots` | `llmrc-bots-core` | 플랫폼 중립 `BotHandler` |
| `discord` / `telegram` / `slack` | `bots` + 플랫폼 매퍼 | `bots`를 함께 활성화 |
| `serenity` / `teloxide` / `slack-morphism` | 해당 플랫폼 기능의 별칭 | 이름 대칭용. 추가 의존성은 없음 |

봇 플랫폼은 옵트인입니다.

```toml
llmrc = {
    version = "0.1",
    features = ["openai", "discord", "telegram", "slack"],
}
```

공식 `rmcp` 클라이언트 어댑터를 쓰려면 `llmrc-mcp`를 직접 의존성으로 추가하고
`rmcp` 기능을 켜세요. 파사드는 이 기능을 재수출하지 않습니다.

```toml
llmrc-mcp = { version = "0.1", features = ["rmcp"] }
```

---

## 빠른 시작

```rust,no_run
use llmrc::prelude::*;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. provider를 하나 고릅니다.
    let provider = llmrc::ollama::OllamaProvider::localhost();
    // let provider = llmrc::openai::OpenAiProvider::openai(std::env::var("OPENAI_API_KEY")?);

    // 2. 유한하고 취소 가능한 에이전트를 만듭니다.
    let agent = Agent::new(provider).with_config(AgentConfig {
        model: "llama3.2".into(),
        max_turns: 8,
        max_tool_calls: 16,
        max_concurrency: 4,
        ..Default::default()
    });

    // 3. 취소 토큰과 함께 실행합니다.
    let cancellation = CancellationToken::new();
    let result = agent.run("Rust 소유권을 한 문장으로 설명해줘.", cancellation).await?;

    println!("{}", result.message.text());
    println!("turns={} tools={} tokens={}", result.turns, result.tool_calls, result.usage.total());
    Ok(())
}
```

`prelude`는 대부분의 애플리케이션이 필요로 하는 타입을 재수출합니다: `Agent`,
`AgentConfig`, `ChatProvider`, `ChatRequest`, `Message`, `RetryPolicy`, `Tool`,
`ToolRegistry`, `TokenCounter`, `LlmError` 등.

---

## 핵심 개념

### Provider

`ChatProvider`는 애플리케이션과 모든 LLM 사이의 유일한 경계입니다.

```rust
fn kind(&self) -> ProviderKind;
fn capabilities(&self) -> ProviderCapabilities;
async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;
async fn chat_stream(&self, request: ChatRequest) -> Result<BoxChatStream, LlmError>;
```

새 백엔드를 위해 이 트레이트만 구현하면 에이전트, 재시도, 봇 스택 전체가 그대로
동작합니다. 스트리밍은 별도 메서드입니다. 현재 에이전트 루프는 `chat`을 사용하며,
증분 출력이 필요하면 `RetryPolicy::collect_stream`이나 provider 스트림을 직접
사용하세요.

### Agent

`Agent`는 유한한 추론 → 도구 → 관찰 루프를 실행합니다.

1. 누적된 히스토리와 등록된 도구 정의를 provider에 보냅니다.
2. 어시스턴트가 도구 호출을 요청하면 인자를 검증하고, 동시성 상한 내에서
   실행한 뒤 `Tool` 메시지를 추가합니다.
3. 어시스턴트가 멈추거나 제한에 도달할 때까지 반복합니다.

`AgentConfig`의 기본값은 보수적이며 모두 재정의할 수 있습니다.

| 필드 | 기본값 | 목적 |
| --- | --- | --- |
| `model` | `""` | **반드시 설정**. 모든 요청에 전송됨 |
| `max_turns` | `16` | 실행당 provider 왕복 횟수 |
| `max_tool_calls` | `64` | 실행당 도구 수. 배치 실행 전에 검사 |
| `deadline` | `None` | 실행 전체의 실시간 상한 |
| `tool_timeout` | `30s` | 도구 하나의 실행 상한 |
| `max_concurrency` | `4` | 병렬 도구 호출 세마포어 폭 |
| `max_history_messages` | `100` | 트리밍 전 히스토리 길이 |
| `retry_policy` | 재시도 2회 / 기본 100ms / 상한 30s | provider 호출에만 적용 |

세션은 문자열 키로 관리되며 `SessionStore`(기본값
`InMemorySessionStore`)에 저장됩니다. `Agent::run`은 세션 id `"default"`를,
`run_session`은 명시적 id를 사용합니다. 세션은 성공한 실행 이후에만 저장됩니다.

### 도구(Tool)

`Tool`을 구현하고 등록합니다.

```rust
struct AddTool;

#[async_trait::async_trait]
impl Tool for AddTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "add".into(),
            description: Some("두 수를 더한다".into()),
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

레지스트리는 중복 이름을 거부하고, 도구 이름을 1~64자의 ASCII 영숫자/`_`/`-`로
강제합니다(따라서 항상 유효한 provider 스키마 식별자가 됩니다). 인자는 실행 전에
실용적인 JSON Schema 부분집합 — `type`, `required`, `properties`,
`additionalProperties: false` — 으로 검증됩니다. 도구가 자체적인 취소 가능 I/O를
수행한다면 `Tool::execute_with_cancellation`을 재정의하세요.

### 재시도와 취소

`RetryPolicy`는 `LlmError::is_retryable()`이 일시적이라고 분류한 오류
(`RateLimited`, `Server`, `Transport`, `Timeout`)만 재시도합니다.

- 지수 백오프 `base_delay * 2^(n-1)`, `max_delay`로 상한.
- `Jitter::{None, Full, Equal}` (기본 `Equal`).
- 선택적 공유 `RetryBudget`으로 호출 무리가 무한 재시도하지 못하게 함.
- `LlmError::RateLimited`의 `Retry-After`가 계산된 지연을 덮어씀.
- provider별 정책을 위한 교체 가능한 `RetryClassifier`.

`collect_stream`은 사용자에게 **보이는 항목이 관찰되기 전까지만** 스트림을
재시도합니다. 가시성 판정 함수가 true를 반환한 뒤에는 오류를 그대로 노출하고
스트림을 절대 재시작하지 않습니다 — 중복 출력을 방지합니다.

### 토큰 집계

`TokenCounter`는 교체 가능하며, 모든 카운트는 출처를 함께 가집니다.
`TokenCount::{Exact, ProviderReported, Estimated}`. 에이전트는 provider가 보고한
usage를 우선 사용하고, 없으면 설정된 counter로 추정합니다.
`HeuristicTokenCounter`는 `chars / 4`(비어 있지 않은 텍스트는 최소 1)를 사용하며
청구 기준이 아닌 추정치임이 문서화되어 있습니다. `TokenAccounting`은
prompt/completion 합계를 누적하고 출처를 보수적으로 병합합니다(추정치가 섞이면
병합 결과도 추정치가 됩니다).

### MCP

`McpBridge`는 어떤 `McpTransport`든 감싸서 도구 목록을 갱신하고, 도구 이름을
`server__tool`로 네임스페이스한 뒤 `ToolRegistry`에 등록합니다. 그러면 에이전트가
원격 MCP 도구를 로컬 도구처럼 호출할 수 있습니다. 내장 전송 두 가지:

- `StdioTransport::spawn(program, args)` — 자식 프로세스와 줄바꿈 구분
  JSON-RPC, 요청 직렬화, drop 시 자식 프로세스 종료.
- `StreamableHttpTransport::new(endpoint)` — 선택적 헤더를 지원하는 HTTP(S)
  POST 기반 JSON-RPC.

공식 `rmcp` 3.x 클라이언트 어댑터를 쓰려면 `llmrc-mcp`의 `rmcp` 기능을
활성화하세요 (`rmcp_adapter::StdioTransport`,
`rmcp_adapter::StreamableHttpTransport`).

### 봇(Bots)

`BotHandler`는 `Agent`를 다중 대화 서비스로 바꿉니다. **`ConversationKey`별로**
턴을 잠그므로(플랫폼 + 대화 id), 붐비는 채널 하나가 자기 세션을 망가뜨리지
않으면서 서로 다른 채널은 병렬로 실행됩니다. 또한 `split_message`로 긴 응답을
분할합니다. 이 함수는 UTF-8 경계에서 자르고 개행/공백 지점을 우선하며, 각 조각이
설정된 바이트 상한 이하임을 보장합니다. `DiscordAdapter`, `TelegramAdapter`,
`SlackAdapter`가 네이티브 이벤트를 `BotEvent`로, 그 반대로 매핑합니다.

---

## 예제

자격 증명이 필요 없는 오프라인 실행 가능 예제 — 자세한 내용은
[`examples/README.md`](examples/README.md) 참고:

```bash
cargo run --example 01_basic_agent            # 에이전트 설정 + 토큰 집계
cargo run --example 02_custom_tools           # 도구 스키마 + 추론 루프
cargo run --example 03_bot_conversation --features bots   # 세션 + 메시지 분할
cargo run --example 04_streaming_and_retry    # 백오프, 예산, 안전한 스트리밍
```

---

## 문서

- [`GUIDE.ko.md`](GUIDE.ko.md) — 상세 사용자 가이드(한국어): 설치, provider
  설정, 에이전트 구성, 도구, 재시도, 스트리밍, MCP, 봇, 오류 처리, 문제 해결.
- [`GUIDE.md`](GUIDE.md) — 동일한 가이드의 영문판.
- [`README.md`](./README.md) — 영문 README.
- [`RELEASING.ko.md`](RELEASING.ko.md) — 태그 기반 릴리스 절차: crates.io 설정,
  배포 순서, 속도 제한, 중단된 릴리스 이어하기.
- [`examples/README.md`](examples/README.md) — 예제 색인과 실제 provider 설정.
- [`AGENTS.md`](AGENTS.md) — 자동화 코딩 에이전트를 위한 불변 규칙과 TDD 프로토콜.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — 기여 워크플로와 품질 게이트.
- [`SECURITY.md`](SECURITY.md) — 취약점 신고와 보안 관행.
- [`LICENSE`](LICENSE) — MIT.

---

## 알려진 한계

의도된 현재 경계이며 버그가 아닙니다.

- `Agent` 루프는 비스트리밍입니다. `ChatProvider::chat`을 호출합니다.
  스트리밍은 provider에서 직접, 또는 `RetryPolicy::collect_stream`으로 사용할 수
  있습니다.
- 히스토리 트리밍은 가장 오래된 비시스템 메시지를 먼저 버립니다. 따라서
  `max_history_messages`를 너무 작게 잡으면 어시스턴트의 `tool_calls` 메시지와
  그 `Tool` 결과가 분리될 수 있습니다. 한 번의 도구 라운드보다 넉넉하게
  유지하거나 턴 경계에서만 트리밍하세요.
- provider SDK가 아직 `Retry-After` 헤더를 노출하지 않아
  `LlmError::RateLimited`는 `retry_after: None`으로 생성됩니다. 현재는 커스텀
  provider만 지연 덮어쓰기의 이점을 얻습니다.
- 봇 플랫폼 기능은 얇은 매퍼입니다. 게이트웨이 연결을 열지 않으며, 애플리케이션이
  `serenity`, `teloxide`, `slack-morphism` 전송을 직접 구동합니다.
- 도구 인자 검증은 JSON Schema 부분집합이며 완전한 검증기가 아닙니다.
- `RetryStats`는 회계용으로 공개되어 있지만 아직 런타임이 채우지 않습니다.
