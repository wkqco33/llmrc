//! A bounded, cancellation-aware tool-using agent loop.

use async_trait::async_trait;
use futures::future::join_all;
use llmrc_core::{ChatProvider, ChatRequest, Message, ToolCall, ToolDefinition};
use llmrc_runtime::{HeuristicTokenCounter, RetryPolicy, TokenAccounting, TokenCounter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fmt, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool")]
    Unknown,
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("tool failed")]
    Failed,
    #[error("tool failed: {0}")]
    FailedWithMessage(String),
    #[error("tool timed out")]
    Timeout,
    #[error("tool was cancelled")]
    Cancelled,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, arguments: Value) -> Result<String, ToolError>;

    /// Executes a tool while observing a caller-provided cancellation token.
    ///
    /// Existing tools only need to implement [`Tool::execute`]. Tools which
    /// perform cancellable I/O can override this method and propagate the
    /// token to their transport.
    async fn execute_with_cancellation(
        &self,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> Result<String, ToolError> {
        tokio::select! {
            _ = cancellation.cancelled() => Err(ToolError::Cancelled),
            result = self.execute(arguments) => result,
        }
    }
}

/// Name used by integrations for a tool that can be executed by an agent.
pub use Tool as ExecutableTool;

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Arc<HashMap<String, Arc<dyn Tool>>>,
}

impl fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: impl Tool + 'static) -> Result<(), AgentError> {
        let definition = tool.definition();
        validate_tool_name(&definition.name)?;
        let mut tools = (*self.tools).clone();
        if tools.contains_key(&definition.name) {
            return Err(AgentError::Configuration(format!(
                "duplicate tool `{}`",
                definition.name
            )));
        }
        tools.insert(definition.name, Arc::new(tool));
        self.tools = Arc::new(tools);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub history: Vec<Message>,
}

impl Session {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            history: Vec::new(),
        }
    }
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn load(&self, id: &str) -> Result<Option<Session>, AgentError>;
    async fn save(&self, session: Session) -> Result<(), AgentError>;
}

#[derive(Clone, Default)]
pub struct InMemorySessionStore {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl fmt::Debug for InMemorySessionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InMemorySessionStore(..)")
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn load(&self, id: &str) -> Result<Option<Session>, AgentError> {
        Ok(self.sessions.lock().await.get(id).cloned())
    }

    async fn save(&self, session: Session) -> Result<(), AgentError> {
        self.sessions
            .lock()
            .await
            .insert(session.id.clone(), session);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub model: String,
    pub max_turns: u32,
    pub max_tool_calls: u32,
    pub deadline: Option<Duration>,
    pub tool_timeout: Option<Duration>,
    pub max_concurrency: usize,
    pub max_history_messages: usize,
    pub retry_policy: RetryPolicy,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            max_turns: 16,
            max_tool_calls: 64,
            deadline: None,
            tool_timeout: Some(Duration::from_secs(30)),
            max_concurrency: 4,
            max_history_messages: 100,
            retry_policy: RetryPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    TurnStarted {
        turn: u32,
    },
    ProviderCompleted {
        turn: u32,
    },
    ToolStarted {
        turn: u32,
        call_id: String,
        name: String,
    },
    ToolCompleted {
        turn: u32,
        call_id: String,
        name: String,
        is_error: bool,
    },
    Completed {
        turn: u32,
    },
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct AgentResult {
    pub message: Message,
    pub history: Vec<Message>,
    pub events: Vec<AgentEvent>,
    pub usage: TokenAccounting,
    pub turns: u32,
    pub tool_calls: u32,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent configuration error: {0}")]
    Configuration(String),
    #[error("provider call failed: {0}")]
    Provider(#[source] llmrc_core::LlmError),
    #[error("unknown tool `{0}`")]
    UnknownTool(String),
    #[error("invalid arguments for tool `{tool}`: {reason}")]
    InvalidToolArguments { tool: String, reason: String },
    #[error("maximum turns exceeded")]
    MaxTurns,
    #[error("maximum tool calls exceeded")]
    MaxToolCalls,
    #[error("agent deadline exceeded")]
    Deadline,
    #[error("agent was cancelled")]
    Cancelled,
    #[error("session storage failed: {0}")]
    Storage(String),
}

pub struct Agent {
    provider: Arc<dyn ChatProvider>,
    registry: ToolRegistry,
    store: Arc<dyn SessionStore>,
    counter: Arc<dyn TokenCounter>,
    config: AgentConfig,
}

impl fmt::Debug for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Agent")
            .field("provider", &self.provider.kind())
            .field("registry", &self.registry)
            .field("config", &self.config)
            .finish()
    }
}

impl Agent {
    pub fn new(provider: impl ChatProvider + 'static) -> Self {
        Self::from_shared(Arc::new(provider))
    }

    pub fn from_shared(provider: Arc<dyn ChatProvider>) -> Self {
        Self {
            provider,
            registry: ToolRegistry::new(),
            store: Arc::new(InMemorySessionStore::default()),
            counter: Arc::new(HeuristicTokenCounter),
            config: AgentConfig::default(),
        }
    }

    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_registry(mut self, registry: ToolRegistry) -> Self {
        self.registry = registry;
        self
    }

    pub fn with_store(mut self, store: impl SessionStore + 'static) -> Self {
        self.store = Arc::new(store);
        self
    }

    pub fn with_token_counter(mut self, counter: impl TokenCounter + 'static) -> Self {
        self.counter = Arc::new(counter);
        self
    }

    pub async fn run(
        &self,
        input: impl Into<String>,
        cancellation: CancellationToken,
    ) -> Result<AgentResult, AgentError> {
        self.run_session("default", input, cancellation).await
    }

    pub async fn run_session(
        &self,
        id: &str,
        input: impl Into<String>,
        cancellation: CancellationToken,
    ) -> Result<AgentResult, AgentError> {
        let mut session = self
            .store
            .load(id)
            .await?
            .unwrap_or_else(|| Session::new(id));
        session.history.push(Message::user(input));
        trim_history(&mut session.history, self.config.max_history_messages);
        let work = self.run_inner(session.clone(), cancellation.clone());
        let result = if let Some(deadline) = self.config.deadline {
            match tokio::time::timeout(deadline, work).await {
                Ok(result) => result,
                Err(_) => Err(AgentError::Deadline),
            }
        } else {
            work.await
        };
        if let Ok(ref completed) = result {
            self.store
                .save(Session {
                    id: id.to_owned(),
                    history: completed.history.clone(),
                })
                .await?;
        }
        result
    }

    async fn run_inner(
        &self,
        mut session: Session,
        cancellation: CancellationToken,
    ) -> Result<AgentResult, AgentError> {
        let mut events = Vec::new();
        let mut usage = TokenAccounting::default();
        let mut tool_calls = 0;
        let mut turn = 0;
        let limit = self.config.max_concurrency.max(1);
        let semaphore = Arc::new(Semaphore::new(limit));
        loop {
            if cancellation.is_cancelled() {
                events.push(AgentEvent::Cancelled);
                return Err(AgentError::Cancelled);
            }
            if turn >= self.config.max_turns {
                return Err(AgentError::MaxTurns);
            }
            turn += 1;
            events.push(AgentEvent::TurnStarted { turn });
            let request = ChatRequest {
                model: self.config.model.clone(),
                messages: session.history.clone(),
                tools: self.registry.definitions(),
                ..Default::default()
            };
            let estimated_prompt = self.counter.count_request(&request);
            let provider = self.provider.clone();
            let retry = self.config.retry_policy.clone();
            let response = retry
                .execute(
                    || {
                        let provider = provider.clone();
                        let request = request.clone();
                        async move { provider.chat(request).await }
                    },
                    &cancellation,
                )
                .await
                .map_err(|error| match error {
                    llmrc_core::LlmError::Cancelled => AgentError::Cancelled,
                    other => AgentError::Provider(other),
                })?;
            if let Some(provider_usage) = response.usage.as_ref() {
                usage.record_usage(provider_usage);
            } else {
                usage.record_prompt(estimated_prompt);
                usage.record_completion(self.counter.count_text(&response.message.text()));
            }
            events.push(AgentEvent::ProviderCompleted { turn });
            let assistant = response.message.clone();
            session.history.push(assistant.clone());
            if assistant.tool_calls.is_empty() {
                events.push(AgentEvent::Completed { turn });
                return Ok(AgentResult {
                    message: assistant,
                    history: session.history,
                    events,
                    usage,
                    turns: turn,
                    tool_calls,
                });
            }
            let calls = assistant.tool_calls.clone();
            if tool_calls.saturating_add(calls.len() as u32) > self.config.max_tool_calls {
                return Err(AgentError::MaxToolCalls);
            }
            tool_calls += calls.len() as u32;
            for call in &calls {
                events.push(AgentEvent::ToolStarted {
                    turn,
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                });
            }
            let jobs = calls.iter().map(|call| {
                let registry = self.registry.clone();
                let semaphore = semaphore.clone();
                let cancellation = cancellation.clone();
                let timeout = self.config.tool_timeout;
                async move { execute_tool(call, registry, semaphore, timeout, cancellation).await }
            });
            let results = join_all(jobs).await;
            for (call, result) in calls.iter().zip(results) {
                let (content, is_error, event_name) = match result {
                    Ok(content) => (content, false, call.name.clone()),
                    Err(error) => (tool_error_message(&error), true, call.name.clone()),
                };
                events.push(AgentEvent::ToolCompleted {
                    turn,
                    call_id: call.id.clone(),
                    name: event_name,
                    is_error,
                });
                session.history.push(Message::tool(&call.id, content));
            }
            trim_history(&mut session.history, self.config.max_history_messages);
        }
    }
}

async fn execute_tool(
    call: &ToolCall,
    registry: ToolRegistry,
    semaphore: Arc<Semaphore>,
    timeout: Option<Duration>,
    cancellation: CancellationToken,
) -> Result<String, ToolError> {
    let tool = registry.get(&call.name).ok_or(ToolError::Unknown)?;
    validate_arguments(&tool.definition().parameters, &call.arguments)
        .map_err(ToolError::InvalidArguments)?;
    let permit = tokio::select! {
        _ = cancellation.cancelled() => return Err(ToolError::Cancelled),
        permit = semaphore.acquire_owned() => permit.map_err(|_| ToolError::Cancelled)?,
    };
    let tool_cancellation = cancellation.clone();
    let uncapped_cancellation = cancellation.clone();
    let future = async move {
        if let Some(timeout) = timeout {
            tokio::time::timeout(
                timeout,
                tool.execute_with_cancellation(call.arguments.clone(), tool_cancellation),
            )
            .await
            .map_err(|_| ToolError::Timeout)?
        } else {
            tool.execute_with_cancellation(call.arguments.clone(), uncapped_cancellation)
                .await
        }
    };
    let result = tokio::select! {
        _ = cancellation.cancelled() => Err(ToolError::Cancelled),
        result = future => result,
    };
    drop(permit);
    result
}

fn tool_error_message(error: &ToolError) -> String {
    match error {
        ToolError::Unknown => "unknown tool".into(),
        ToolError::InvalidArguments(reason) => format!("invalid arguments: {reason}"),
        ToolError::Timeout => "tool timed out".into(),
        ToolError::Cancelled => "tool cancelled".into(),
        ToolError::Failed => "tool failed".into(),
        ToolError::FailedWithMessage(message) => format!("tool failed: {message}"),
    }
}

fn validate_tool_name(name: &str) -> Result<(), AgentError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(AgentError::Configuration(
            "tool names must be 1-64 ASCII alphanumeric, `_`, or `-`".into(),
        ));
    }
    Ok(())
}

fn validate_arguments(schema: &Value, args: &Value) -> Result<(), String> {
    if schema.is_null() || schema == &Value::Object(Default::default()) {
        return Ok(());
    }
    if let Some(expected) = schema.get("type").and_then(Value::as_str) {
        let valid = match expected {
            "object" => args.is_object(),
            "array" => args.is_array(),
            "string" => args.is_string(),
            "number" => args.is_number(),
            "integer" => args.as_i64().is_some() || args.as_u64().is_some(),
            "boolean" => args.is_boolean(),
            "null" => args.is_null(),
            _ => true,
        };
        if !valid {
            return Err(format!("expected {expected}"));
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        let object = args
            .as_object()
            .ok_or_else(|| "expected object".to_string())?;
        for field in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(field) {
                return Err(format!("missing required field `{field}`"));
            }
        }
    }
    if let (Some(properties), Some(object)) = (
        schema.get("properties").and_then(Value::as_object),
        args.as_object(),
    ) {
        for (name, value) in object {
            if let Some(property) = properties.get(name) {
                validate_arguments(property, value)
                    .map_err(|reason| format!("{name}: {reason}"))?;
            } else if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
                return Err(format!("unknown field `{name}`"));
            }
        }
    }
    Ok(())
}

fn trim_history(history: &mut Vec<Message>, max: usize) {
    if max == 0 {
        history.clear();
        return;
    }
    let excess = history.len().saturating_sub(max);
    if excess == 0 {
        return;
    }

    let non_system_count = history
        .iter()
        .filter(|m| !matches!(m.role, llmrc_core::MessageRole::System))
        .count();

    let (mut drop_non_system, mut drop_system) = if non_system_count >= excess {
        (excess, 0)
    } else {
        (non_system_count, excess - non_system_count)
    };

    history.retain(|m| {
        if !matches!(m.role, llmrc_core::MessageRole::System) {
            if drop_non_system > 0 {
                drop_non_system -= 1;
                false
            } else {
                true
            }
        } else if drop_system > 0 {
            drop_system -= 1;
            false
        } else {
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use llmrc_core::{ChatResponse, FinishReason, ProviderCapabilities, ProviderKind};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::sleep;

    struct FakeProvider {
        calls: AtomicUsize,
    }
    #[async_trait]
    impl ChatProvider for FakeProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Ollama
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                streaming: false,
                embeddings: false,
                tool_calls: true,
                multimodal: false,
            }
        }
        async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, llmrc_core::LlmError> {
            let count = self.calls.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                Ok(ChatResponse {
                    id: None,
                    model: request.model,
                    finish_reason: Some(FinishReason::ToolCalls),
                    usage: None,
                    message: Message {
                        role: llmrc_core::MessageRole::Assistant,
                        content: vec![],
                        name: None,
                        tool_calls: vec![
                            ToolCall {
                                id: "a".into(),
                                name: "slow".into(),
                                arguments: serde_json::json!({"n": 1}),
                            },
                            ToolCall {
                                id: "b".into(),
                                name: "slow".into(),
                                arguments: serde_json::json!({"n": 2}),
                            },
                        ],
                        tool_call_id: None,
                    },
                })
            } else {
                Ok(ChatResponse {
                    id: None,
                    model: request.model,
                    finish_reason: Some(FinishReason::Stop),
                    usage: None,
                    message: Message::assistant("done"),
                })
            }
        }
        async fn chat_stream(
            &self,
            _: ChatRequest,
        ) -> Result<llmrc_core::BoxChatStream, llmrc_core::LlmError> {
            Err(llmrc_core::LlmError::Unsupported("test".into()))
        }
    }
    struct SlowTool;
    #[async_trait]
    impl Tool for SlowTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "slow".into(),
                description: None,
                parameters: serde_json::json!({"type":"object","required":["n"],"properties":{"n":{"type":"integer"}}}),
            }
        }
        async fn execute(&self, args: Value) -> Result<String, ToolError> {
            sleep(Duration::from_millis(if args["n"] == 1 { 20 } else { 1 })).await;
            Ok(args["n"].to_string())
        }
    }

    #[tokio::test]
    async fn executes_tools_concurrently_and_preserves_call_order() {
        let mut registry = ToolRegistry::new();
        registry.register(SlowTool).unwrap();
        let config = AgentConfig {
            model: "fake".into(),
            max_concurrency: 2,
            ..Default::default()
        };
        let agent = Agent::new(FakeProvider {
            calls: AtomicUsize::new(0),
        })
        .with_registry(registry)
        .with_config(config);
        let result = agent.run("go", CancellationToken::new()).await.unwrap();
        let tools = result
            .history
            .iter()
            .filter(|message| message.role == llmrc_core::MessageRole::Tool)
            .collect::<Vec<_>>();
        assert_eq!(
            tools
                .iter()
                .map(|message| message.text())
                .collect::<Vec<_>>(),
            vec!["1", "2"]
        );
    }

    #[tokio::test]
    async fn cancellation_is_reported() {
        let token = CancellationToken::new();
        token.cancel();
        let agent = Agent::new(FakeProvider {
            calls: AtomicUsize::new(0),
        });
        assert!(matches!(
            agent.run("go", token).await,
            Err(AgentError::Cancelled)
        ));
    }

    #[tokio::test]
    async fn tool_call_bound_is_enforced_before_execution() {
        let mut registry = ToolRegistry::new();
        registry.register(SlowTool).unwrap();
        let config = AgentConfig {
            model: "fake".into(),
            max_tool_calls: 1,
            ..Default::default()
        };
        let agent = Agent::new(FakeProvider {
            calls: AtomicUsize::new(0),
        })
        .with_registry(registry)
        .with_config(config);
        assert!(matches!(
            agent.run("go", CancellationToken::new()).await,
            Err(AgentError::MaxToolCalls)
        ));
    }

    #[tokio::test]
    async fn turn_bound_and_tool_timeout_are_enforced() {
        let mut registry = ToolRegistry::new();
        registry.register(SlowTool).unwrap();
        let config = AgentConfig {
            model: "fake".into(),
            tool_timeout: Some(Duration::ZERO),
            ..Default::default()
        };
        let agent = Agent::new(FakeProvider {
            calls: AtomicUsize::new(0),
        })
        .with_registry(registry)
        .with_config(config);
        let result = agent.run("go", CancellationToken::new()).await.unwrap();
        assert!(
            result
                .history
                .iter()
                .any(|message| message.text() == "tool timed out")
        );

        let mut registry = ToolRegistry::new();
        registry.register(SlowTool).unwrap();
        let bounded = AgentConfig {
            model: "fake".into(),
            max_turns: 1,
            ..Default::default()
        };
        let bounded_agent = Agent::new(FakeProvider {
            calls: AtomicUsize::new(0),
        })
        .with_registry(registry)
        .with_config(bounded);
        assert!(matches!(
            bounded_agent.run("go", CancellationToken::new()).await,
            Err(AgentError::MaxTurns)
        ));
    }

    #[test]
    fn invalid_tool_arguments_are_rejected() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["city"],
            "properties": {"city": {"type": "string"}},
            "additionalProperties": false
        });
        assert!(validate_arguments(&schema, &serde_json::json!({})).is_err());
        assert!(validate_arguments(&schema, &serde_json::json!({"city": 4})).is_err());
        assert!(validate_arguments(&schema, &serde_json::json!({"city": "Paris"})).is_ok());
    }

    #[test]
    fn trim_history_drops_oldest_non_system_messages_first() {
        let mut history = vec![
            Message::system("sys_prompt"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
        ];
        trim_history(&mut history, 3);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].text(), "sys_prompt");
        assert_eq!(history[1].text(), "a1");
        assert_eq!(history[2].text(), "u2");

        // When non-system messages are depleted, drops system messages
        let mut history2 = vec![
            Message::system("sys1"),
            Message::system("sys2"),
            Message::user("u1"),
        ];
        trim_history(&mut history2, 1);
        assert_eq!(history2.len(), 1);
        assert_eq!(history2[0].text(), "sys2");

        // max = 0 clears completely
        let mut history3 = vec![Message::system("sys"), Message::user("u")];
        trim_history(&mut history3, 0);
        assert!(history3.is_empty());
    }
}
