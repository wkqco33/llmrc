//! Platform-neutral bot contracts and a serialized conversation handler.

use async_trait::async_trait;
use llmrc_agent::{Agent, AgentError};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt, sync::Arc};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(String);

impl ConversationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ConversationId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl From<&str> for ConversationId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}
impl fmt::Display for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationKey {
    pub platform: String,
    pub id: ConversationId,
}

impl Default for ConversationKey {
    fn default() -> Self {
        Self::new("", "")
    }
}

impl ConversationKey {
    pub fn new(platform: impl Into<String>, id: impl Into<ConversationId>) -> Self {
        Self {
            platform: platform.into(),
            id: id.into(),
        }
    }
    pub fn storage_key(&self) -> String {
        format!("{}:{}", self.platform, self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotEvent {
    pub key: ConversationKey,
    pub message_id: Option<String>,
    pub author: Option<String>,
    pub text: String,
}

impl BotEvent {
    pub fn new(key: ConversationKey, text: impl Into<String>) -> Self {
        Self {
            key,
            message_id: None,
            author: None,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotResponse {
    pub key: ConversationKey,
    pub text: String,
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BotSession {
    pub key: ConversationKey,
    pub messages_seen: u64,
}

#[async_trait]
pub trait BotSessionStore: Send + Sync {
    async fn load(&self, key: &ConversationKey) -> Result<Option<BotSession>, BotError>;
    async fn save(&self, session: BotSession) -> Result<(), BotError>;
}

#[derive(Clone, Default)]
pub struct InMemoryBotSessionStore {
    sessions: Arc<RwLock<HashMap<ConversationKey, BotSession>>>,
}

impl fmt::Debug for InMemoryBotSessionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InMemoryBotSessionStore(..)")
    }
}

#[async_trait]
impl BotSessionStore for InMemoryBotSessionStore {
    async fn load(&self, key: &ConversationKey) -> Result<Option<BotSession>, BotError> {
        Ok(self.sessions.read().await.get(key).cloned())
    }
    async fn save(&self, session: BotSession) -> Result<(), BotError> {
        self.sessions
            .write()
            .await
            .insert(session.key.clone(), session);
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum BotError {
    #[error("bot is shutting down")]
    Shutdown,
    #[error("bot operation was cancelled")]
    Cancelled,
    #[error("agent failed: {0}")]
    Agent(#[from] AgentError),
    #[error("session storage failed: {0}")]
    Storage(String),
    #[error("message split limit must be greater than zero")]
    InvalidSplitLimit,
}

/// Splits on UTF-8 character boundaries and prefers a newline or whitespace
/// before the hard limit. Every returned part is at most `max_bytes` bytes.
pub fn split_message(text: &str, max_bytes: usize) -> Result<Vec<String>, BotError> {
    if max_bytes == 0 {
        return Err(BotError::InvalidSplitLimit);
    }
    if text.is_empty() {
        return Ok(vec![String::new()]);
    }
    let mut parts = Vec::new();
    let mut remaining = text;
    while remaining.len() > max_bytes {
        let mut end = max_bytes;
        while !remaining.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 {
            end = remaining
                .char_indices()
                .nth(1)
                .map(|(index, _)| index)
                .unwrap_or(remaining.len());
        }
        let candidate = &remaining[..end];
        if let Some(index) = candidate
            .rfind('\n')
            .or_else(|| candidate.rfind(char::is_whitespace))
            && index > 0
        {
            end = index + 1;
        }
        parts.push(remaining[..end].to_owned());
        remaining = &remaining[end..];
    }
    parts.push(remaining.to_owned());
    Ok(parts)
}

#[derive(Clone, Default)]
struct ConversationLocks {
    locks: Arc<Mutex<HashMap<ConversationKey, Arc<Mutex<()>>>>>,
}

impl ConversationLocks {
    async fn lock(&self, key: &ConversationKey) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

/// Invokes one llmrc agent per conversation, serializing turns for each key
/// while allowing unrelated conversations to run concurrently.
pub struct BotHandler {
    agent: Arc<Agent>,
    sessions: Arc<dyn BotSessionStore>,
    locks: ConversationLocks,
    shutdown: CancellationToken,
    max_response_bytes: usize,
}

impl fmt::Debug for BotHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BotHandler")
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

impl BotHandler {
    pub fn new(agent: Arc<Agent>) -> Self {
        Self {
            agent,
            sessions: Arc::new(InMemoryBotSessionStore::default()),
            locks: ConversationLocks::default(),
            shutdown: CancellationToken::new(),
            max_response_bytes: 2_000,
        }
    }

    pub fn with_session_store(mut self, store: impl BotSessionStore + 'static) -> Self {
        self.sessions = Arc::new(store);
        self
    }

    pub fn with_max_response_bytes(mut self, max: usize) -> Self {
        self.max_response_bytes = max;
        self
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
    pub fn cancellation_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    pub async fn handle(
        &self,
        event: BotEvent,
        cancellation: CancellationToken,
    ) -> Result<Vec<BotResponse>, BotError> {
        if self.shutdown.is_cancelled() {
            return Err(BotError::Shutdown);
        }
        let lock = self.locks.lock(&event.key).await;
        let _guard = lock.lock().await;
        let session = self
            .sessions
            .load(&event.key)
            .await?
            .unwrap_or_else(|| BotSession {
                key: event.key.clone(),
                messages_seen: 0,
            });
        let child = self.shutdown.child_token();
        let agent = self.agent.clone();
        let key = event.key.clone();
        let storage_key = key.storage_key();
        let input = event.text;
        let result = tokio::select! {
            _ = cancellation.cancelled() => Err(BotError::Cancelled),
            _ = self.shutdown.cancelled() => Err(BotError::Shutdown),
            result = agent.run_session(&storage_key, input, child.clone()) => result.map_err(BotError::Agent),
        }?;
        self.sessions
            .save(BotSession {
                messages_seen: session.messages_seen + 1,
                ..session
            })
            .await?;
        let chunks = split_message(&result.message.text(), self.max_response_bytes)?;
        Ok(chunks
            .into_iter()
            .map(|text| BotResponse {
                key: key.clone(),
                text,
                reply_to: event.message_id.clone(),
            })
            .collect())
    }
}

/// A minimal adapter contract: platform crates only need to map native events
/// into [`BotEvent`] and response text back into their native messages.
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    type Event: Send;
    type Error: std::error::Error + Send + Sync + 'static;
    fn to_event(&self, event: Self::Event) -> Result<BotEvent, Self::Error>;
    async fn send(&self, response: BotResponse) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use llmrc_core::{
        ChatProvider, ChatRequest, ChatResponse, FinishReason, Message, ProviderCapabilities,
        ProviderKind,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Provider(AtomicUsize);
    #[async_trait]
    impl ChatProvider for Provider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Ollama
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                streaming: false,
                embeddings: false,
                tool_calls: false,
                multimodal: false,
            }
        }
        async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, llmrc_core::LlmError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(ChatResponse {
                id: None,
                model: request.model,
                message: Message::assistant(
                    request
                        .messages
                        .last()
                        .map(Message::text)
                        .unwrap_or_default(),
                ),
                finish_reason: Some(FinishReason::Stop),
                usage: None,
            })
        }
        async fn chat_stream(
            &self,
            _: ChatRequest,
        ) -> Result<llmrc_core::BoxChatStream, llmrc_core::LlmError> {
            Err(llmrc_core::LlmError::Unsupported("test".into()))
        }
    }

    #[test]
    fn splitting_is_utf8_safe_and_bounded() {
        let parts = split_message("hello 🌍\nworld", 8).unwrap();
        assert!(parts.iter().all(|part| part.len() <= 8));
        assert_eq!(parts.join(""), "hello 🌍\nworld");
    }

    #[tokio::test]
    async fn conversations_are_isolated() {
        let config = llmrc_agent::AgentConfig {
            model: "test".into(),
            ..Default::default()
        };
        let handler = BotHandler::new(Arc::new(
            Agent::new(Provider(AtomicUsize::new(0))).with_config(config),
        ));
        let a = handler
            .handle(
                BotEvent::new(ConversationKey::new("x", "a"), "one"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let b = handler
            .handle(
                BotEvent::new(ConversationKey::new("x", "b"), "two"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(a[0].text, "one");
        assert_eq!(b[0].text, "two");
    }

    #[tokio::test]
    async fn same_conversation_updates_are_serialized() {
        let config = llmrc_agent::AgentConfig {
            model: "test".into(),
            ..Default::default()
        };
        let handler = Arc::new(BotHandler::new(Arc::new(
            Agent::new(Provider(AtomicUsize::new(0))).with_config(config),
        )));
        let key = ConversationKey::new("test", "same");
        let first = {
            let handler = handler.clone();
            let key = key.clone();
            tokio::spawn(async move {
                handler
                    .handle(BotEvent::new(key, "first"), CancellationToken::new())
                    .await
            })
        };
        let second = {
            let handler = handler.clone();
            tokio::spawn(async move {
                handler
                    .handle(BotEvent::new(key, "second"), CancellationToken::new())
                    .await
            })
        };
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        let session = handler
            .sessions
            .load(&ConversationKey::new("test", "same"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.messages_seen, 2);
    }

    #[tokio::test]
    async fn cancellation_and_shutdown_are_explicit() {
        let config = llmrc_agent::AgentConfig {
            model: "test".into(),
            ..Default::default()
        };
        let handler = BotHandler::new(Arc::new(
            Agent::new(Provider(AtomicUsize::new(0))).with_config(config),
        ));
        handler.shutdown();
        assert!(matches!(
            handler
                .handle(
                    BotEvent::new(ConversationKey::new("x", "a"), "x"),
                    CancellationToken::new()
                )
                .await,
            Err(BotError::Shutdown)
        ));
    }
}
