//! `llmrc` is the user-facing facade for the modular LLM runtime.
//!
//! Provider implementations are feature-gated so applications only compile
//! the SDKs they use.
//!
//! # Examples
//!
//! ```
//! use llmrc::prelude::*;
//!
//! let user_msg = Message::user("Hello from llmrc");
//! assert_eq!(user_msg.role, MessageRole::User);
//! assert_eq!(user_msg.text(), "Hello from llmrc");
//!
//! let config = AgentConfig {
//!     model: "gpt-4o-mini".into(),
//!     max_turns: 4,
//!     ..Default::default()
//! };
//! assert_eq!(config.max_turns, 4);
//! ```

pub use llmrc_agent as agent;
pub use llmrc_agent::{
    Agent, AgentConfig, AgentError, AgentEvent, AgentResult, ExecutableTool, InMemorySessionStore,
    Session, SessionStore, Tool, ToolError, ToolRegistry,
};
pub use llmrc_core as core;
pub use llmrc_core::{
    AssistantMessage, ChatProvider, ChatRequest, ChatResponse, ContentPart, Embedding,
    EmbeddingProvider, EmbeddingRequest, FinishReason, LlmError, Message, MessageRole,
    ProviderCapabilities, ProviderKind, Secret, StreamEvent, TokenCount, TokenUsage, ToolCall,
    ToolDefinition, ToolResult,
};
pub use llmrc_runtime as runtime;
pub use llmrc_runtime::{
    ExactTokenCounter, HeuristicTokenCounter, Jitter, ProviderReportedTokenCounter, RetryBudget,
    RetryPolicy, TokenAccounting, TokenCountResult, TokenCounter,
};

#[cfg(feature = "bots")]
pub use llmrc_bots_core as bots;
#[cfg(feature = "discord")]
pub use llmrc_bots_discord as discord;
#[cfg(feature = "slack")]
pub use llmrc_bots_slack as slack;
#[cfg(feature = "telegram")]
pub use llmrc_bots_telegram as telegram;
#[cfg(feature = "mcp")]
pub use llmrc_mcp as mcp;

/// The most commonly used types for applications.
///
/// # Examples
///
/// ```
/// use llmrc::prelude::*;
///
/// let message = Message::assistant("Ready to help.");
/// assert_eq!(message.role, MessageRole::Assistant);
/// ```
pub mod prelude {
    pub use crate::{
        Agent, AgentConfig, AgentError, AgentEvent, ChatProvider, ChatRequest, ChatResponse,
        ContentPart, EmbeddingProvider, EmbeddingRequest, FinishReason, HeuristicTokenCounter,
        LlmError, Message, MessageRole, RetryPolicy, StreamEvent, TokenAccounting, TokenCounter,
        Tool, ToolCall, ToolDefinition, ToolError, ToolRegistry, ToolResult,
    };
}

#[cfg(feature = "ollama")]
pub use llmrc_ollama as ollama;
#[cfg(any(feature = "azure", feature = "openai"))]
pub use llmrc_openai as openai;

#[cfg(test)]
mod tests {
    use super::prelude::*;
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    struct DummyProvider;

    #[async_trait]
    impl ChatProvider for DummyProvider {
        fn kind(&self) -> crate::core::ProviderKind {
            crate::core::ProviderKind::Ollama
        }
        fn capabilities(&self) -> crate::core::ProviderCapabilities {
            crate::core::ProviderCapabilities {
                streaming: false,
                embeddings: false,
                tool_calls: false,
                multimodal: false,
            }
        }
        async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
            Ok(ChatResponse {
                id: Some("mock-id".into()),
                model: request.model,
                message: Message::assistant("mock reply"),
                finish_reason: Some(FinishReason::Stop),
                usage: None,
            })
        }
        async fn chat_stream(
            &self,
            _: ChatRequest,
        ) -> Result<crate::core::BoxChatStream, LlmError> {
            Err(LlmError::Unsupported("not implemented".into()))
        }
    }

    #[tokio::test]
    async fn facade_prelude_agent_workflow() {
        let message = Message::user("test prompt");
        assert_eq!(message.role, MessageRole::User);

        let agent = Agent::new(DummyProvider).with_config(AgentConfig {
            model: "test-model".into(),
            ..Default::default()
        });

        let result = agent
            .run("hello facade", CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(result.message.text(), "mock reply");
        assert_eq!(result.turns, 1);
    }
}
