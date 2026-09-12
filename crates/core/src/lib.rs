//! Provider-independent types and contracts.

mod error;
mod provider;
mod types;

pub use error::{ErrorKind, LlmError, Secret};
pub type Result<T> = std::result::Result<T, LlmError>;
pub use provider::{
    BoxChatStream, ChatProvider, EmbeddingProvider, ProviderCapabilities, ProviderKind,
};
pub use types::{
    AssistantMessage, ChatRequest, ChatResponse, ContentPart, Embedding, EmbeddingRequest,
    FinishReason, ImageSource, Message, MessageRole, StreamEvent, TokenCount, TokenUsage, ToolCall,
    ToolCallDelta, ToolDefinition, ToolResult,
};
