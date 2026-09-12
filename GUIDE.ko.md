# llmrc 사용자 가이드

`llmrc`로 애플리케이션을 만드는 방법을 처음부터 끝까지 다루는 실전 가이드입니다.
설치, provider 설정, 에이전트 루프, 도구, 재시도, 스트리밍, 토큰 집계, MCP 연동,
채팅 봇, 오류 처리, 문제 해결을 포함합니다.

짧은 버전을 원하면 [README](./README.md)를 읽으세요. 이 문서는 긴 버전입니다.

> 영문판: [GUIDE.md](GUIDE.md)

---

## 목차

1. [설치와 기능 선택](#1-설치와-기능-선택)
2. [사고 모델](#2-사고-모델)
3. [도메인 타입과 메시지](#3-도메인-타입과-메시지)
4. [Provider](#4-provider)
5. [에이전트 루프](#5-에이전트-루프)
6. [도구](#6-도구)
7. [세션과 히스토리](#7-세션과-히스토리)
8. [재시도, 지터, 예산](#8-재시도-지터-예산)
9. [취소와 타임아웃](#9-취소와-타임아웃)
10. [스트리밍](#10-스트리밍)
11. [토큰 집계](#11-토큰-집계)
12. [MCP 연동](#12-mcp-연동)
13. [채팅 봇](#13-채팅-봇)
14. [오류 처리](#14-오류-처리)
15. [보안](#15-보안)
16. [통합 테스트](#16-통합-테스트)
17. [문제 해결과 FAQ](#17-문제-해결과-faq)
18. [API 치트시트](#18-api-치트시트)

---

## 1. 설치와 기능 선택

파사드를 추가하고 필요한 백엔드만 고르세요. `core`, `runtime`, `agent`는 항상
컴파일되며, provider, MCP, 봇은 옵트인입니다.

```toml
[dependencies]
llmrc = { version = "0.1", features = ["openai", "mcp"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tokio-util = "0.7"
async-trait = "0.1"   # Tool / ChatProvider를 직접 구현할 때만
serde_json = "1"       # Tool을 직접 구현할 때만
futures = "0.3"        # 스트림을 소비할 때만
```

### 기능 매트릭스

| 기능 | 추가되는 것 | 대표 용도 |
| --- | --- | --- |
| *(없음)* | `llmrc-core`, `llmrc-runtime`, `llmrc-agent` | 커스텀 provider, 재시도만, 자체 백엔드 에이전트 |
| `openai` | OpenAI 채팅/스트리밍/임베딩/도구/멀티모달 | 호스팅 OpenAI |
| `azure` | Azure OpenAI (같은 크레이트, 다른 클라이언트 설정) | Azure 배포 |
| `ollama` | Ollama 채팅/스트리밍/임베딩/도구/base64 이미지 | 로컬 모델 |
| `mcp` | MCP 브리지 + 내장 stdio/HTTP 전송 | 원격 도구 서버 |
| `bots` | `BotHandler`, 세션, `split_message` | 모든 채팅 플랫폼 |
| `discord` | `bots` + `DiscordAdapter` | Discord |
| `telegram` | `bots` + `TelegramAdapter` | Telegram |
| `slack` | `bots` + `SlackAdapter` | Slack |
| `serenity` | `discord`의 별칭 | 이름 대칭 |
| `teloxide` | `telegram`의 별칭 | 이름 대칭 |
| `slack-morphism` | `slack`의 별칭 | 이름 대칭 |

> **주의:** `agent` 기능은 존재하지 않습니다. 에이전트는 파사드의 필수
> 의존성입니다. `features = ["openai", "agent"]`는 해석에 실패합니다.

### 공식 `rmcp` 전송

`mcp` 기능은 전송 독립 브리지와, 이 워크스페이스에서 직접 작성한 JSON-RPC
stdio/Streamable HTTP 전송을 제공합니다. 대신 공식 `rmcp` 3.x 클라이언트를
쓰려면 크레이트를 직접 의존성으로 추가하세요.

```toml
llmrc-mcp = { version = "0.1", features = ["rmcp"] }
```

그리고 `llmrc_mcp::rmcp_adapter::{StdioTransport, StreamableHttpTransport}`를
임포트합니다.

### MSRV

Rust 1.88+, edition 2024.

---

## 2. 사고 모델

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

모든 것은 값과 토큰의 조합입니다.

- 값이 들어가고 값이 나옵니다 — `ChatRequest`/`ChatResponse`, `Message`,
  `ToolDefinition`.
- 모든 장기 실행 호출에 `tokio_util::sync::CancellationToken`이 있습니다.
- 모든 루프에 제한이 있습니다: 턴, 도구, 동시성, 데드라인, 히스토리.

어느 계층에서든 멈출 수 있습니다. 원시 채팅에는 `ChatProvider`만, 복원력에는
`RetryPolicy`를 추가하고, 도구 호출에는 `ToolRegistry`를 추가하며, 다중 사용자
서빙에는 전체를 `BotHandler`로 감싸면 됩니다.

---

## 3. 도메인 타입과 메시지

공유 타입은 모두 `llmrc_core`에 있으며 `llmrc`에서 재수출됩니다.

### 메시지 생성

```rust
use llmrc::prelude::*;

let system = Message::system("You are a terse assistant.");
let user   = Message::user("What is 2 + 2?");
let assistant = Message::assistant("4");
let tool   = Message::tool("call_1", "4");           // tool_call_id 필요
let custom = Message::new(MessageRole::User, "hi");  // 명시적 role
```

`Message` 필드:

| 필드 | 타입 | 의미 |
| --- | --- | --- |
| `role` | `MessageRole` | `System`, `User`, `Assistant`, `Tool` |
| `content` | `Vec<ContentPart>` | 텍스트 및/또는 이미지 파트 |
| `name` | `Option<String>` | 선택적 참여자 이름 |
| `tool_calls` | `Vec<ToolCall>` | 어시스턴트가 요청한 호출 |
| `tool_call_id` | `Option<String>` | `Tool` 메시지에 필수 |

`Message::text()`는 모든 텍스트 파트를 이어 붙이고 이미지는 무시합니다.

### 멀티모달 콘텐츠

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
        // 또는: ImageSource::Url { url: "https://…".into(), detail: None }
    ],
    name: None,
    tool_calls: vec![],
    tool_call_id: None,
};
```

provider별 지원이 다릅니다.

- **OpenAI / Azure** — `Url`, `Base64` 모두 지원(base64는 `data:` URI로 변환).
- **Ollama** — `Base64`만 지원. `Url` 이미지는 Ollama가 원격 이미지를 가져오지
  않으므로 `LlmError::Unsupported`를 반환합니다. 바이트를 직접 내려받아 base64로
  전달하세요.

### 채팅 요청과 응답

```rust
let request = ChatRequest {
    model: "gpt-4o-mini".into(),
    messages: vec![Message::user("hello")],
    temperature: Some(0.2),
    max_tokens: Some(512),
    tools: vec![], // 에이전트가 레지스트리에서 채워 넣음
};

let response: ChatResponse = provider.chat(request).await?;
println!("{}", response.message.text());
println!("finish: {:?}", response.finish_reason);
println!("usage:  {:?}", response.usage);
```

`FinishReason`은 `Stop`, `Length`, `ToolCalls`, `ContentFilter`, `Other`입니다.
OpenAI의 레거시 `FunctionCall`은 `ToolCalls`로 정규화됩니다.

### 스트림 이벤트

`StreamEvent`가 스트리밍 어휘입니다.

| 변형 | 의미 |
| --- | --- |
| `Started { id, model }` | 스트림 열림(두 provider 모두 가장 먼저 방출) |
| `TextDelta { text }` | 증분 어시스턴트 텍스트 |
| `ToolCallDelta(ToolCallDelta)` | 증분 도구 호출 조각 |
| `Usage(TokenUsage)` | provider가 보고한 usage |
| `Completed { finish_reason }` | 종료 이벤트 |

`ToolCallDelta` 조각은 `index` 기준으로 누적하세요. OpenAI는 id와 name을 한 번만
보내고 이후에는 인자 조각을 보냅니다.

---

## 4. Provider

### OpenAI

```rust
use llmrc::openai::OpenAiProvider;

// 표준 OpenAI
let provider = OpenAiProvider::openai(std::env::var("OPENAI_API_KEY")?);

// 호환 게이트웨이 / 프록시
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

`ProviderKind`는 `AzureOpenAi`를 보고하므로 필요하면 `provider.kind()`로 분기할
수 있습니다.

### Ollama

```rust
use llmrc::ollama::OllamaProvider;

let provider = OllamaProvider::localhost();               // http://127.0.0.1:11434
let provider = OllamaProvider::new("http://gpu-box:11434")?; // 원격
```

`OllamaProvider::new`는 `Result<_, LlmError>`를 반환하며 잘못된 URL을
`LlmError::Configuration`으로 매핑합니다.

### 기능 확인

기능에 의존하기 전에 확인하세요.

```rust
let caps = provider.capabilities();
if !caps.streaming { /* chat()으로 폴백 */ }
```

`ProviderCapabilities { streaming, embeddings, tool_calls, multimodal }`.

### 직접 provider 작성

`ChatProvider`(그리고 선택적으로 `EmbeddingProvider`)를 구현하면 전체 스택이
그대로 동작합니다.

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
        // request -> SDK 호출 -> ChatResponse; SDK 오류 -> LlmError
        todo!()
    }

    async fn chat_stream(&self, request: ChatRequest) -> Result<BoxChatStream, LlmError> {
        Err(LlmError::Unsupported("streaming not implemented".into()))
    }
}
```

중요한 부분은 오류 매핑입니다. 일시적 실패에는 `LlmError::RateLimited`,
`Server`, `Transport`, `Timeout`을 반환해 재시도 정책이 작동하게 하고, 영구적
실패에는 `Authentication`, `InvalidRequest`, `Configuration`을 반환하세요. 지연을
알고 있다면 `RateLimited { retry_after }`를 채우세요.

---

## 5. 에이전트 루프

### 최소 사용법

```rust
use llmrc::prelude::*;
use tokio_util::sync::CancellationToken;

let agent = Agent::new(provider).with_config(AgentConfig {
    model: "gpt-4o-mini".into(),
    ..Default::default()
});

let result = agent.run("Rust borrow checker를 요약해줘.", CancellationToken::new()).await?;
println!("{}", result.message.text());
```

### `run`이 실제로 하는 일

1. 세션을 로드합니다(`run`은 `"default"`, `run_session`은 지정한 id).
2. 사용자 메시지를 추가합니다.
3. 히스토리를 `max_history_messages`로 트리밍합니다.
4. 최대 `max_turns`번 반복합니다.
   - 취소 확인 → `AgentError::Cancelled`.
   - 전체 히스토리와 레지스트리 정의로 `ChatRequest`를 구성합니다.
   - `RetryPolicy::execute`를 통해 `provider.chat`을 호출합니다.
   - usage를 기록합니다(provider 보고 우선, 없으면 카운트).
   - 도구 호출이 없으면 `AgentResult`를 반환합니다.
   - 있으면 `max_tool_calls`를 검증하고, 세마포어 아래에서 도구를 동시
     실행하고, 호출 순서대로 `Tool` 메시지를 추가하고, 히스토리를 트리밍한 뒤
     반복합니다.
5. 성공하면 세션을 저장합니다.
6. `deadline`이 설정되어 있으면 루프 전체가 `tokio::time::timeout` 아래에서
   실행됩니다.

### 설정 레퍼런스

```rust
let config = AgentConfig {
    model: "gpt-4o".into(),                        // 필수
    max_turns: 16,                                 // provider 왕복 횟수
    max_tool_calls: 64,                            // 실행당 총 도구 수
    deadline: Some(Duration::from_secs(60)),       // 실행 전체 실시간 상한
    tool_timeout: Some(Duration::from_secs(30)),   // 도구 하나
    max_concurrency: 4,                            // 병렬 도구
    max_history_messages: 100,                     // 히스토리 상한
    retry_policy: RetryPolicy::builder()
        .max_retries(3)
        .base_delay(Duration::from_millis(200))
        .build(),
};
```

### 빌더 메서드

| 메서드 | 효과 |
| --- | --- |
| `Agent::new(provider)` | provider를 값으로 소유 |
| `Agent::from_shared(Arc<dyn ChatProvider>)` | 여러 에이전트가 provider 공유 |
| `.with_config(cfg)` | 설정 교체 |
| `.with_registry(registry)` | 도구 등록 |
| `.with_store(store)` | 세션 스토어 교체 |
| `.with_token_counter(counter)` | 토큰 집계 교체 |
| `.run(input, token)` | 세션 `"default"` |
| `.run_session(id, input, token)` | 명시적 세션 id |

### 결과 읽기

`AgentResult`는 관측에 필요한 모든 것을 담습니다.

| 필드 | 타입 | 의미 |
| --- | --- | --- |
| `message` | `Message` | 최종 어시스턴트 메시지 |
| `history` | `Vec<Message>` | 실행 후 전체 대화 |
| `events` | `Vec<AgentEvent>` | 수명주기 추적 |
| `usage` | `TokenAccounting` | prompt/completion 합계 |
| `turns` | `u32` | 소비한 턴 |
| `tool_calls` | `u32` | 실행한 도구 |

`AgentEvent` 변형: `TurnStarted`, `ProviderCompleted`, `ToolStarted`,
`ToolCompleted { is_error }`, `Completed`, `Cancelled`. 턴별 추적을 만들려면 이
이벤트를 로깅하세요.

### 에이전트 공유

`Agent`는 `Clone`이 아니지만 `Arc<Agent>`가 의도된 공유 패턴입니다. 특히
`BotHandler`에서 그렇습니다.

```rust
let agent = Arc::new(Agent::new(provider).with_config(config));
```

---

## 6. 도구

### 도구 구현

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
            description: Some("도시의 현재 날씨를 조회한다".into()),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["city"],
                "properties": {
                    "city": { "type": "string", "description": "도시 이름" },
                    "units": { "type": "string" }
                },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<String, ToolError> {
        let city = args.get("city").and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArguments("city is required".into()))?;
        // 서비스 호출…
        Ok(format!("{city}는 18°C, 맑음"))
    }
}
```

### 등록

```rust
let mut registry = ToolRegistry::new();
registry.register(WeatherTool)?;                        // Result<(), AgentError>
assert!(registry.get("get_weather").is_some());
let defs = registry.definitions();                       // Vec<ToolDefinition>
let agent = Agent::new(provider).with_registry(registry);
```

등록 시 강제되는 규칙:

- 이름: 1~64자, ASCII 영숫자, `_`, `-`.
- 중복은 `AgentError::Configuration`으로 거부됩니다.

### 인자 검증

도구 실행 전에 에이전트가 모델의 인자를 선언된 스키마로 검증합니다. 지원하는
키워드:

- `type` — `object`, `array`, `string`, `number`, `integer`, `boolean`,
  `null` (알 수 없는 타입은 통과).
- `required` — 키가 반드시 존재해야 함.
- `properties` — 재귀 검증; 중첩 오류에는 필드 이름이 접두됩니다.
- `additionalProperties: false` — 알 수 없는 키를 거부.

스키마가 `null`이거나 `{}`이면 해당 도구의 검증이 비활성화됩니다. 이는 실용적인
부분집합이며 완전한 JSON Schema 구현이 아닙니다. `oneOf`, `enum`, `pattern`,
숫자 범위는 지원하지 않습니다. 더 엄격한 검증이 필요하면 `execute` 안에서
검증하고 `ToolError::InvalidArguments`를 반환하세요.

### 실패 의미론

어떤 `ToolError`든 실행 전체를 중단시키지 않고 `Tool` 메시지가 됩니다.

| `ToolError` | 모델에 전달되는 메시지 |
| --- | --- |
| `Unknown` | `unknown tool` |
| `InvalidArguments(reason)` | `invalid arguments: {reason}` |
| `Timeout` | `tool timed out` |
| `Cancelled` | `tool cancelled` |
| `Failed` | `tool failed` |
| `FailedWithMessage(msg)` | `tool failed: {msg}` |

따라서 모델이 복구(예: 인자를 고쳐 재시도)할 수 있고, 대화 전체가 실패하지
않습니다.

### 동시성, 순서, 타임아웃

- 한 배치의 모든 호출은 `max_concurrency` 폭의 세마포어 아래에서 함께
  시작됩니다.
- 결과는 완료 순서와 무관하게 **모델의 호출 순서대로** 추가됩니다 — 히스토리가
  일관되게 유지됩니다.
- 각 호출은 `tool_timeout`(기본 30초)으로 감쌉니다.
- 배치가 `max_tool_calls`를 넘기면 에이전트는 **아무것도 실행하기 전에**
  `AgentError::MaxToolCalls`를 반환합니다.

### 취소 가능한 도구

자체 I/O를 하는 도구는 `execute_with_cancellation`을 재정의해 토큰이 전송
계층까지 전달되게 하세요.

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

기본 구현이 이미 `execute`를 이렇게 감싸고 있습니다.

---

## 7. 세션과 히스토리

세션은 `{ id: String, history: Vec<Message> }`이며 `SessionStore` 뒤에 있습니다.

```rust
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn load(&self, id: &str) -> Result<Option<Session>, AgentError>;
    async fn save(&self, session: Session) -> Result<(), AgentError>;
}
```

`InMemorySessionStore`가 기본값이며 프로세스 로컬입니다. 영속화하려면 직접
구현하세요.

```rust
struct RedisStore { /* ... */ }

#[async_trait::async_trait]
impl SessionStore for RedisStore {
    async fn load(&self, id: &str) -> Result<Option<Session>, AgentError> {
        // 백엔드에서 역직렬화
        todo!()
    }
    async fn save(&self, session: Session) -> Result<(), AgentError> {
        // 백엔드로 직렬화
        todo!()
    }
}

let agent = agent.with_store(RedisStore { /* ... */ });
```

동작 참고:

- `run`은 세션 id `"default"`를 사용합니다. 다중 사용자 코드에서는 항상
  `run_session`을 써서 대화가 충돌하지 않게 하세요.
- 세션은 **실행이 성공했을 때만** 저장됩니다. 실패하거나 취소된 실행은 저장된
  세션을 건드리지 않습니다.
- `AgentError::Storage`가 스토어 실패를 감쌉니다.

### 히스토리 트리밍

`history.len() > max_history_messages`가 되면 가장 오래된 **비시스템** 메시지를
먼저 버립니다. 시스템 메시지는 비시스템 메시지가 소진된 뒤에만 버려집니다.
`max_history_messages = 0`은 히스토리를 완전히 비웁니다.

> **주의:** 트리밍은 위치 기반이며 도구를 인식하지 않습니다. 트리밍 경계가
> 어시스턴트 `tool_calls` 메시지와 그 `Tool` 결과 사이에 걸리면 결과가 고아가 될
> 수 있습니다. `max_history_messages`를 한 번의 도구 라운드(어시스턴트 + 모든
> 도구 메시지)보다 넉넉하게 잡거나, 라운드를 온전히 유지하는 트리밍 전략을
> 사용하세요.

---

## 8. 재시도, 지터, 예산

```rust
use llmrc::runtime::{Jitter, RetryBudget};
use std::time::Duration;

let budget = RetryBudget::new(5);           // 호출 간 공유

let policy = RetryPolicy::builder()
    .max_retries(3)                          // 최대 3회 추가 시도
    .base_delay(Duration::from_millis(200))  // 첫 백오프
    .max_delay(Duration::from_secs(20))      // 상한
    .jitter(Jitter::Equal)                   // 기본값
    .budget(budget.clone())
    .classifier(|error| error.is_retryable())
    .build();
```

### 재시도 대상

기본 분류기는 `LlmError::is_retryable()`이며 `RateLimited`, `Server`,
`Transport`, `Timeout`에만 `true`입니다. Authentication, InvalidRequest,
Configuration, Decode, Unsupported, Provider, Cancelled는 **재시도하지
않습니다**.

### 지연 계산

재시도 번호 `n`(1부터 시작)에 대해:

1. `exponential = min(base_delay * 2^(n-1), max_delay)`.
2. 지터 적용:
   - `None` → `exponential`
   - `Full` → `[0, exponential]` 범위의 난수
   - `Equal` → `[exponential/2, exponential]` 범위의 난수
3. 오류가 `RateLimited { retry_after: Some(d) }`이면 대신 `min(d, max_delay)`를
   사용합니다.
4. `max_delay`로 상한을 적용합니다.

### 예산

`RetryBudget`은 내부적으로 `Arc<AtomicU32>`입니다. 복제하면 허용량이 공유되므로
전체 예산으로 재시도 폭주를 막을 수 있습니다.

```rust
let budget = RetryBudget::new(10);
let policy_a = RetryPolicy::builder().budget(budget.clone()).build();
let policy_b = RetryPolicy::builder().budget(budget.clone()).build();
// A와 B가 같은 10회 풀에서 차감합니다.
budget.remaining(); // 언제든 확인
```

예산을 차감할 수 있을 때만 재시도가 일어나며, 그렇지 않으면 오류가 그대로
반환됩니다.

### 재시도로 작업 실행

```rust
let result = policy.execute(
    || async { provider.chat(request.clone()).await },
    &cancellation,
).await?;
```

클로저는 시도마다 새 future를 반환하므로 작업을 재실행할 수 있습니다. 취소는
시도 전, 시도 중, 시도 사이에서 확인되며 백오프 대기 중에도 확인됩니다.

`RetryStats`는 공개 API에 회계용으로 존재하지만 아직 런타임이 채우지 않습니다.
필요하면 직접 시도 횟수를 추적하세요.

---

## 9. 취소와 타임아웃

`llmrc`에서 취소는 보편적입니다. `CancellationToken`은 모든 경계에서
관찰됩니다.

| 계층 | 취소 방식 |
| --- | --- |
| `RetryPolicy::execute` / `collect_stream` | 시도와 백오프 대기 주위의 `select!` |
| `Agent::run` / `run_session` | 매 턴 확인; `AgentError::Cancelled`로 매핑 |
| `Tool::execute_with_cancellation` | `execute` 주위의 `select!` |
| 에이전트 도구 실행 | 세마포어 대기와 도구 future 주위의 `select!` |
| `McpTransport::list_tools` / `call_tool` | 요청 주위의 `select!` |
| `BotHandler::handle` | 호출자 토큰 + 종료 토큰 결합 |

### 일반 패턴

**사용자가 중지 버튼을 누른 경우:**

```rust
let token = CancellationToken::new();
let child = token.clone();
tokio::spawn(async move {
    // ... 나중에
    child.cancel();
});
let result = agent.run("…", token).await; // Err(AgentError::Cancelled)
```

**정상 종료:**

```rust
let handler = Arc::new(BotHandler::new(agent));
handler.shutdown();                 // 이후 handle은 BotError::Shutdown 반환
handler.cancellation_token();       // 에이전트 실행으로 전파
```

### 타임아웃 대 데드라인

- `tool_timeout` — 도구 호출 하나. 타임아웃은 `tool timed out` 도구 메시지가
  되고 루프는 계속됩니다.
- `deadline` — 실행 전체. 만료되면 `AgentError::Deadline`을 반환하고 세션을
  저장하지 않습니다.

둘은 독립적입니다. 데드라인에 도달하기 전에 여러 도구 타임아웃이 발생할 수
있습니다.

---

## 10. 스트리밍

스트리밍은 에이전트 루프를 우회하고 `ChatProvider::chat_stream`을 직접
사용합니다. 일시적 실패를 견뎌야 한다면 `RetryPolicy::collect_stream`으로
감싸세요.

### 스트림 소비

```rust
use futures::StreamExt;
use llmrc::core::StreamEvent;

let mut stream = provider.chat_stream(request).await?;
while let Some(event) = stream.next().await {
    match event? {
        StreamEvent::Started { model, .. } => println!("[{model}]"),
        StreamEvent::TextDelta { text } => print!("{text}"),
        StreamEvent::ToolCallDelta(delta) => { /* delta.index 기준 누적 */ }
        StreamEvent::Usage(usage) => println!("\nusage: {} tokens", usage.total_tokens),
        StreamEvent::Completed { finish_reason } => println!("\ndone: {finish_reason:?}"),
    }
}
```

### 안전한 스트림 재시도

```rust
let chunks = policy.collect_stream(
    || async { provider.chat_stream(request.clone()).await },
    &cancellation,
    |event| matches!(event, StreamEvent::TextDelta { .. }), // 사용자 가시성 판정
).await?;
```

중요한 의미론:

- 보이는 항목이 **없을 때** 발생한 오류는 새 시도를 유발할 수 있습니다.
- 판정 함수가 `true`를 반환한 뒤에는 소스를 절대 재시작하지 않습니다. 이후
  오류는 즉시 표면화됩니다.
- 가시성 판정은 사용자 몫입니다. 메타데이터(`Usage` 등)는 내부용일 수 있고
  텍스트는 사용자에게 보일 수 있습니다. 중복 출력을 피하려면 경계를 신중히
  고르세요.

### 도구 호출 누적

OpenAI는 도구 호출을 조각으로 스트리밍합니다. 첫 delta가 `id`와 `name`을
나르고, 이후 delta가 인자 JSON 조각을 나르며 모두 `index`로 구분됩니다.
`Completed`까지 index별로 버퍼링한 뒤 이어 붙인 인자를 파싱하세요.

---

## 11. 토큰 집계

모든 카운트는 `TokenCount`로 출처가 태깅됩니다.

| 변형 | 의미 | 신뢰도 |
| --- | --- | --- |
| `Exact` | 실제 토크나이저가 계산 | 청구 등급 |
| `ProviderReported` | provider의 usage 필드 | 청구 등급 |
| `Estimated` | 휴리스틱 | 계획용 |

### 카운터

```rust
use llmrc::runtime::{ExactTokenCounter, HeuristicTokenCounter, ProviderReportedTokenCounter};

// 내장 휴리스틱: chars / 4 (비어 있지 않은 텍스트는 1)
let counter = HeuristicTokenCounter;

// 자체 토크나이저 사용
let counter = ExactTokenCounter::new(|text| tiktoken_count(text));

// provider가 보고한 usage 재사용
let counter = ProviderReportedTokenCounter::new(usage);
```

에이전트에 카운터를 전달합니다.

```rust
let agent = Agent::new(provider).with_token_counter(HeuristicTokenCounter);
```

에이전트는 provider의 `response.usage`를 우선 사용하고, provider가 usage를
생략했을 때만 설정된 카운터로 prompt/completion 토큰을 추정합니다.

### 누적

```rust
use llmrc::runtime::TokenAccounting;

let mut accounting = TokenAccounting::default();
accounting.record_prompt(counter.count_text("prompt text"));
accounting.record_completion(counter.count_text("completion text"));
accounting.record_usage(&provider_reported);   // 이것도 지원
accounting.total();                            // prompt + completion
```

출처 병합은 보수적입니다. 어떤 기여라도 `Estimated`면 병합 결과도
`Estimated`입니다. `ProviderReported`와 `Exact`를 섞으면 `ProviderReported`가
됩니다.

### 프라이버시 규칙

`TokenCounter` 구현은 프롬프트 텍스트를 로깅하거나 보관하거나 `Debug` 출력에
포함해서는 **안 됩니다**. `ExactTokenCounter`와 `HeuristicTokenCounter`는 이미
안전합니다. 직접 작성한다면 같은 보장을 유지하세요.

---

## 12. MCP 연동

MCP(Model Context Protocol) 서버는 JSON-RPC로 도구를 노출합니다. `llmrc`는 이를
에이전트가 이미 사용하는 동일한 `ToolRegistry`로 브리지합니다.

### 구성 요소

| 타입 | 역할 |
| --- | --- |
| `McpTransport` | 트레이트: `list_tools` + `call_tool` (둘 다 취소 인식) |
| `McpBridge` | 서버 하나를 전송에 바인딩; 네임스페이스 처리와 캐싱 |
| `McpPool` | 여러 브리지 관리, 일괄 갱신 |
| `McpExecutableTool` | 전송을 통해 호출하는 `Tool` 구현 |
| `StdioTransport` | 자식 프로세스 JSON-RPC 전송 |
| `StreamableHttpTransport` | HTTP JSON-RPC 전송 |

### stdio 서버

```rust
use llmrc::mcp::{McpBridge, StdioTransport};
use std::time::Duration;

let transport = StdioTransport::spawn("npx", &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]).await?;
let bridge = McpBridge::new("filesystem", transport)
    .with_timeout(Some(Duration::from_secs(20)));

bridge.refresh().await?;                  // tools/list 조회; 먼저 호출해야 함
let mut registry = llmrc::ToolRegistry::new();
bridge.register_into(&mut registry).await?;
```

### HTTP 서버

```rust
use llmrc::mcp::{McpBridge, StreamableHttpTransport};

let transport = StreamableHttpTransport::new("https://mcp.example.com/mcp")?
    .with_header("Authorization", "Bearer …");

let bridge = McpBridge::new("remote", transport);
bridge.refresh().await?;
```

### 이름과 네임스페이스

서버 `docs.server`의 원격 도구 `search`는 `docs_server__search`가 됩니다. 서버
이름을 정제(영숫자가 아니면 `_`)하고 도구 이름과 `__`로 연결한 뒤 64자로
자릅니다. 모든 이름이 provider 함수 스키마에 유효하고 서버 간 충돌이 없습니다.

### 서버 풀링

```rust
use llmrc::mcp::McpPool;

let pool = McpPool::new();
pool.add(McpBridge::new("filesystem", fs_transport)).await;
pool.add(McpBridge::new("search", search_transport)).await;

let all_definitions = pool.refresh().await?;     // 모든 브리지 갱신
let mut registry = llmrc::ToolRegistry::new();
pool.register_into(&mut registry).await?;
```

`McpPool`은 별칭 `McpRegistry`, `McpServerRegistry`로도 사용할 수 있습니다.

### 운영 참고

- `McpBridge`는 도구 목록을 캐시합니다. 시작 시 그리고 서버 도구 집합이 바뀔
  때마다 `refresh()`를 호출하세요.
- 모든 브리지 호출은 브리지 `timeout`(기본 30초)으로 감싸지고 취소를 인식합니다.
- `StdioTransport`는 요청을 잠금으로 직렬화하며(JSON-RPC stdio는 다중화되지
  않음) drop 시 자식 프로세스를 종료합니다.
- `StreamableHttpTransport`는 JSON-RPC 응답 본문을 기대하며 SSE 스트림을
  소비하지 않습니다.
- `McpError` 변형: `Transport`, `Protocol`, `Timeout`, `Cancelled`,
  `UnknownTool`, `Tool`. 모델에 도달하기 전에 `ToolError`로 매핑되므로 MCP
  실패는 일반 도구 실패로 나타납니다.
- 공식 `rmcp` 어댑터는 `llmrc-mcp`의 `rmcp` 기능 뒤에 있습니다.

---

## 13. 채팅 봇

`BotHandler`는 `Agent` 하나를 대화별 서비스로 바꿉니다.

### 설정

```rust
use std::sync::Arc;
use llmrc::bots::{BotEvent, BotHandler, ConversationKey};
use tokio_util::sync::CancellationToken;

let agent = Arc::new(Agent::new(provider).with_config(AgentConfig {
    model: "gpt-4o-mini".into(),
    ..Default::default()
}));

let handler = BotHandler::new(agent)
    .with_max_response_bytes(2_000);   // 기본값
```

### 이벤트 처리

```rust
let event = BotEvent::new(
    ConversationKey::new("discord", "channel-123"),
    "안녕 봇",
);

let responses: Vec<BotResponse> =
    handler.handle(event, CancellationToken::new()).await?;
for response in responses {
    println!("{}", response.text);
}
```

### `handle`이 보장하는 것

1. **종료 확인** — 종료된 핸들러는 `BotError::Shutdown`을 반환합니다.
2. **대화별 잠금** — `ConversationKey { platform, id }` 기준. 같은 대화의 턴은
   직렬화되고, 다른 대화는 병렬로 실행됩니다.
3. **안정적인 저장 키** — `format!("{platform}:{id}")`가 에이전트 세션 id로
   사용되므로 각 대화가 자기 히스토리를 유지합니다.
4. **유한한 출력** — 응답은 `split_message`로 분할되고, 각 조각은
   `max_response_bytes` 이하이며, `reply_to`가 원본 메시지 id를 되돌려주는
   별도 `BotResponse`로 반환됩니다.
5. **세션 회계** — 처리된 이벤트마다 `BotSession.messages_seen`이 증가합니다.

### 메시지 분할

```rust
use llmrc::bots::split_message;

let parts = split_message("긴 응답 🌍 …", 2_000)?; // 각 조각 ≤ 2000바이트
```

보장: UTF-8 안전(문자 중간에서 자르지 않음), 상한 준수, 개행 우선 그다음 공백
지점, `max_bytes == 0`이면 `BotError::InvalidSplitLimit` 반환. 빈 문자열은 빈
조각 하나를 만듭니다.

### 플랫폼 어댑터

얇은 매퍼가 네이티브 식별자를 `BotEvent`로, 그 반대로 변환합니다.

```rust
use llmrc::discord::DiscordAdapter;
use llmrc::telegram::TelegramAdapter;
use llmrc::slack::SlackAdapter;

let event = DiscordAdapter::event("channel-123", Some("alice".into()), "hi");
let text  = DiscordAdapter::response_text(&response);
```

각 어댑터는 고유한 플랫폼 태그(`"discord"`, `"telegram"`, `"slack"`)를 가진
`ConversationKey`를 만듭니다. 하나의 핸들러로 여러 플랫폼을 서빙할 때 세션이
격리되는 이유가 바로 이것입니다.

### 커스텀 세션 스토어

```rust
let handler = BotHandler::new(agent).with_session_store(MyBotStore::new());
```

`BotSessionStore`는 `SessionStore`와 같은 모양입니다:
`load(&ConversationKey)`와 `save(BotSession)`.

### 플랫폼 구현

네이티브 라이브러리를 연결하려면 `PlatformAdapter`를 구현하세요.

```rust
#[async_trait::async_trait]
impl PlatformAdapter for MyAdapter {
    type Event = MyNativeEvent;
    type Error = MyError;

    fn to_event(&self, event: Self::Event) -> Result<BotEvent, Self::Error> { todo!() }
    async fn send(&self, response: BotResponse) -> Result<(), Self::Error> { todo!() }
}
```

> 플랫폼 기능은 게이트웨이가 아니라 매퍼입니다. `llmrc`는
> Discord/Telegram/Slack 연결을 열지 않습니다. 애플리케이션이 `serenity`,
> `teloxide`, `slack-morphism`을 구동하고 `handler.handle`을 호출합니다.

---

## 14. 오류 처리

`llmrc`는 모든 곳에서 타입이 있는 오류를 사용합니다. 문자열이 아니라 구체성으로
매칭하세요.

### `LlmError` (provider 계층)

| 변형 | `ErrorKind` | 재시도 | 대표 원인 |
| --- | --- | --- | --- |
| `InvalidRequest(String)` | `InvalidRequest` | 아니오 | 잘못된 파라미터, tool_call_id 누락 |
| `Configuration(String)` | `Configuration` | 아니오 | 잘못된 base URL, 키 누락 |
| `Authentication` | `Authentication` | 아니오 | 401/403 |
| `RateLimited { retry_after }` | `RateLimited` | **예** | 429 |
| `Server(String)` | `Server` | **예** | 5xx |
| `Transport(..)` | `Transport` | **예** | 네트워크/소켓 실패 |
| `Decode(..)` | `Decode` | 아니오 | 잘못된 provider 페이로드 |
| `Cancelled` | `Cancelled` | 아니오 | 토큰 취소 |
| `Timeout` | `Timeout` | **예** | 데드라인 초과 |
| `Unsupported(String)` | `Unsupported` | 아니오 | 사용할 수 없는 기능 |
| `Provider(String)` | `Provider` | 아니오 | 그 외 전부 |

`LlmError`는 `#[non_exhaustive]`이므로 항상 와일드카드 분기를 두세요.

### `AgentError` (에이전트 계층)

`Configuration`, `Provider(LlmError)`, `UnknownTool`, `InvalidToolArguments`,
`MaxTurns`, `MaxToolCalls`, `Deadline`, `Cancelled`, `Storage`.

### `ToolError` / `McpError` / `BotError`

- `ToolError`는 도구 호출 하나를 설명하며 모델이 볼 수 있는 텍스트로 변환되고,
  에이전트 루프 밖으로 전파되지 않습니다.
- `McpError`는 브리지 경계에서 `ToolError`로 매핑됩니다.
- `BotError`는 `Shutdown`, `Cancelled`, `Agent(AgentError)`, `Storage`,
  `InvalidSplitLimit`를 다룹니다.

### 처리 패턴

```rust
use llmrc::core::LlmError;

match agent.run(prompt, token).await {
    Ok(result) => { /* result 사용 */ }
    Err(AgentError::Cancelled) => { /* 사용자가 중단; 세션 그대로 유지 */ }
    Err(AgentError::Deadline) => { /* 자체 SLA 알람 발생 */ }
    Err(AgentError::MaxTurns) | Err(AgentError::MaxToolCalls) => { /* 프롬프트 조정 또는 한도 상향 */ }
    Err(AgentError::Provider(LlmError::Authentication)) => { /* 자격 증명 갱신 */ }
    Err(AgentError::Provider(LlmError::RateLimited { retry_after })) => { /* 백오프; 재시도 정책이 이미 처리했을 수 있음 */ }
    Err(error) => { /* 로깅 후 노출 */ }
}
```

### SDK 오류 매핑

어댑터가 SDK 오류를 정규화합니다.

- **OpenAI**: 401/403 → `Authentication`, 429 → `RateLimited`, 5xx → `Server`,
  reqwest → `Transport`, JSON → `Decode`, 그 외 `Provider`.
- **Ollama**: JSON → `Decode`, reqwest → `Transport`, internal → `Server`,
  그 외 `Provider`.

커스텀 provider도 같은 매핑을 작성해야 재시도 분류기가 계속 동작합니다.

---

## 15. 보안

이 워크스페이스는 자격 증명과 프롬프트 콘텐츠를 설계상 민감 정보로 취급합니다.

1. **자격 증명에는 `Secret`을 사용하세요.**

   ```rust
   use llmrc::core::Secret;

   let key = Secret::new(std::env::var("OPENAI_API_KEY")?);
   // Debug는 "Secret(REDACTED)"를 출력
   // 네트워크 경계에서만 원본 값에 접근:
   let raw = key.expose();
   ```

   `Secret`은 의도적으로 `Display`가 없고 `Debug` 출력을 마스킹합니다. 내장
   provider는 현재 평문 `String`을 받습니다. 설정 계층에서 `Secret`으로 감싸고
   provider를 생성할 때만 `.expose()`를 호출하면 로그에 남지 않습니다.

2. **프롬프트나 완성문을 절대 로깅하지 마세요.** `TokenCounter` 구현은 텍스트를
   보관하거나 출력하지 않고 개수만 세야 합니다. 텔레메트리는 내용이 아니라 개수,
   턴 id, 오류 종류를 기록해야 합니다.

3. **모든 것에 상한을 두세요.** `max_turns`, `max_tool_calls`,
   `max_concurrency`, `deadline`, `tool_timeout`, `max_history_messages`는 폭주
   루프와 메모리 증가를 막기 위해 존재합니다. 운영 환경에서는 항상 설정하세요.

4. **도구 출력을 신뢰하지 마세요.** 도구 결과는 다시 모델에 입력됩니다. 인자를
   검증하고(에이전트가 스키마 검사를 수행) UI에 그대로 출력하는 것은 정제하세요.

5. **MCP 서버를 신뢰하지 마세요.** stdio MCP 서버는 로컬에서 임의 코드를
   실행합니다. HTTP MCP 서버는 원격 의존성입니다. 명시적 타임아웃을 설정하고
   어떤 서버를 등록하는지 감사하세요.

6. **취약점은 비공개로 신고하세요.** [SECURITY.md](SECURITY.md)를 따르세요.

---

## 16. 통합 테스트

워크스페이스 테스트는 오프라인이며 자격 증명이 필요 없습니다. 같은 패턴을
따르세요.

### 가짜 provider

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

### 테스트 레시피

- **도구 루프** — 1턴에 `ToolCalls`, 2턴에 `Stop` 응답을 반환하고
  `result.history`와 `result.tool_calls`를 검증합니다
  (`examples/02_custom_tools.rs` 참고).
- **한도** — `max_turns: 1` 또는 `max_tool_calls: 1`로 설정하고
  `AgentError::MaxTurns` / `MaxToolCalls`를 검증합니다.
- **취소** — 미리 취소된 토큰을 전달하고 `AgentError::Cancelled`를 검증합니다.
- **재시도** — `LlmError::Timeout`으로 N번 실패시키고 작업이 성공하며 예산이
  소진되었는지 검증합니다.
- **스트림 안전** — 보이는 항목 하나를 방출한 뒤 오류를 내고, 작업이 정확히 한
  번만 호출되었는지 검증합니다.
- **분할** — 모든 조각이 `<= max_bytes`이고 조각을 합치면 원본이 되는지
  검증합니다.
- **MCP** — 가짜로 `McpTransport`를 구현하고 네임스페이스, 캐싱(`list_tools`가
  한 번만 호출됨), 풀 격리를 검증합니다.

### 배포 전 품질 게이트

```bash
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

---

## 17. 문제 해결과 FAQ

**`the package 'llmrc' does not contain this feature: agent`**
`agent` 기능은 없습니다 — 에이전트는 항상 컴파일됩니다.
`features = ["openai", "mcp"]` 같은 형태를 사용하세요.

**`AgentError::Configuration("tool names must be …")`**
도구 이름은 1~64자의 ASCII 영숫자, `_`, `-`여야 합니다. MCP 이름은 자동으로
정제되고 네임스페이스됩니다.

**`duplicate tool`**
두 도구가 같은 이름을 공유합니다 — 네임스페이스가 충돌하는 두 MCP 브리지도
포함됩니다. 서버나 도구 이름을 바꾸세요.

**모델이 내 도구를 호출하지 않습니다.**
도구가 실행 *전에* 레지스트리에 있어야 하고,
`ToolDefinition.parameters`가 `required` 필드를 가진 실제 객체 스키마여야
합니다. `provider.capabilities().tool_calls`도 확인하세요.

**`LlmError::Unsupported("Ollama accepts base64 images…")`**
Ollama는 원격 이미지 URL을 가져올 수 없습니다. 바이트를 내려받아
`ImageSource::Base64`로 보내세요.

**도구를 쓰는 프롬프트에서 `AgentError::MaxTurns`가 납니다.**
모델 ↔ 도구 왕복마다 턴이 하나 소비됩니다. 다단계 작업에는 `max_turns`(또는
`max_tool_calls`)를 올리세요.

**응답이 잘렸습니다.**
`BotHandler`가 `max_response_bytes`에서 분할합니다. 상한을 올리거나 반환된
조각을 이어 붙이세요.

**재시도 후 응답이 두 번 나타납니다.**
올바른 가시성 판정과 함께 `RetryPolicy::collect_stream`으로만 스트림을
재시도하세요. 사용자에게 보이는 텍스트가 방출된 뒤에는 절대 스트림을 재시작하지
마세요.

**`Agent`에서는 스트리밍이 안 되나요?**
맞습니다 — 에이전트는 `chat`을 사용합니다. `provider.chat_stream`이나
`collect_stream`을 직접 사용하세요.

**세션 히스토리가 예상보다 커집니다.**
세션은 id별로 유지됩니다. 사용자마다 다른 `run_session` id를 쓰거나, 자체 보존
정책을 가진 영속 `SessionStore`를 사용하세요.

**토큰 수가 왜 추정치인가요?**
provider usage가 없으면 에이전트가 `HeuristicTokenCounter`(`chars / 4`)로
폴백합니다. 청구 등급 숫자가 필요하면 실제 토크나이저와 `ExactTokenCounter`를
사용하세요. 어떤 값을 가졌는지는 `TokenAccounting`의 count 타입으로 확인하세요.

**재시도가 전혀 일어나지 않습니다.**
해당 오류의 `LlmError::is_retryable()`, `max_retries > 0`, 소진되지 않은
`RetryBudget`을 확인하세요.

**MCP 도구가 `unknown tool`을 반환합니다.**
등록 전에 `bridge.refresh()`(또는 `pool.refresh()`)를 호출하고, 도구를 주소
지정할 때 네임스페이스 이름(`server__tool`)을 사용하세요.

---

## 18. API 치트시트

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

// 도구
#[async_trait] impl Tool { definition(), execute() }
ToolRegistry::new().register(tool)?
registry.get(name) / .definitions()

// 재시도
RetryPolicy::builder().max_retries(..).base_delay(..).max_delay(..)
    .jitter(Jitter::Equal).budget(RetryBudget::new(..)).build()
policy.execute(|| async { .. }, &cancellation).await?
policy.collect_stream(|| async { .. }, &cancellation, |item| ..).await?
policy.delay_for(retry_number, &error)

// 토큰
HeuristicTokenCounter / ExactTokenCounter::new(f) / ProviderReportedTokenCounter::new(usage)
counter.count_text(s) / .count_messages(&msgs, &tools) / .count_request(&req)
TokenAccounting::default().record_prompt(..).record_completion(..).record_usage(..)

// 메시지
Message::system(..) / ::user(..) / ::assistant(..) / ::tool(id, ..)
ContentPart::text(..) / ContentPart::Image { source }

// MCP
McpBridge::new(server, transport).with_timeout(Some(dur))
bridge.refresh().await? / .definitions().await / .register_into(&mut registry).await?
McpPool::new().add(bridge).await / .remove(name).await / .refresh().await?
StdioTransport::spawn(program, args).await?
StreamableHttpTransport::new(endpoint)?.with_header(name, value)

// 봇
BotHandler::new(Arc<Agent>)
    .with_max_response_bytes(n)
    .with_session_store(store)
handler.handle(event, cancellation).await? -> Vec<BotResponse>
handler.shutdown() / .cancellation_token()
split_message(text, max_bytes)?
DiscordAdapter::event(id, author, text) / .response_text(&resp)
TelegramAdapter::event(..) / SlackAdapter::event(..)

// 오류
LlmError::kind() / .is_retryable()
AgentError / ToolError / McpError / BotError
```
