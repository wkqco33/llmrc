use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

use crate::{ChatRequest, ChatResponse, Embedding, EmbeddingRequest, LlmError, StreamEvent};

pub type BoxChatStream = BoxStream<'static, Result<StreamEvent, LlmError>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    OpenAi,
    AzureOpenAi,
    Ollama,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub embeddings: bool,
    pub tool_calls: bool,
    pub multimodal: bool,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> ProviderCapabilities;
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;
    async fn chat_stream(&self, request: ChatRequest) -> Result<BoxChatStream, LlmError>;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    async fn embed(&self, request: EmbeddingRequest) -> Result<Vec<Embedding>, LlmError>;
}
