use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A single part of a message, representing text or multimodal input.
///
/// # Examples
///
/// ```
/// use llmrc_core::ContentPart;
///
/// let part = ContentPart::text("Hello world");
/// assert_eq!(part.as_text(), Some("Hello world"));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    Image { source: ImageSource },
}

impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            Self::Image { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Url { url: String, detail: Option<String> },
    Base64 { media_type: String, data: String },
}

/// A conversation message exchanged with an LLM provider.
///
/// # Examples
///
/// ```
/// use llmrc_core::{Message, MessageRole};
///
/// let user_msg = Message::user("How does Rust prevent data races?");
/// assert_eq!(user_msg.role, MessageRole::User);
/// assert_eq!(user_msg.text(), "How does Rust prevent data races?");
///
/// let asst_msg = Message::assistant("Via ownership and Send/Sync traits.");
/// assert_eq!(asst_msg.role, MessageRole::Assistant);
/// assert_eq!(asst_msg.text(), "Via ownership and Send/Sync traits.");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentPart>,
    pub name: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn new(role: MessageRole, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentPart::text(text)],
            name: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::new(MessageRole::User, text)
    }
    pub fn system(text: impl Into<String>) -> Self {
        Self::new(MessageRole::System, text)
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, text)
    }
    pub fn tool(id: impl Into<String>, text: impl Into<String>) -> Self {
        let mut message = Self::new(MessageRole::Tool, text);
        message.tool_call_id = Some(id.into());
        message
    }

    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(ContentPart::as_text)
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: u32,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: Option<String>,
    pub model: String,
    pub message: Message,
    pub finish_reason: Option<FinishReason>,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    pub index: usize,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenCount {
    Exact,
    ProviderReported,
    Estimated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub count_type: TokenCount,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    Started { id: Option<String>, model: String },
    TextDelta { text: String },
    ToolCallDelta(ToolCallDelta),
    Usage(TokenUsage),
    Completed { finish_reason: Option<FinishReason> },
}

/// Kept as a semantic alias for callers that want to name assistant content.
pub type AssistantMessage = Message;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_without_losing_tool_metadata() {
        let message = Message {
            role: MessageRole::Assistant,
            content: vec![ContentPart::text("thinking")],
            name: Some("assistant".into()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "weather".into(),
                arguments: serde_json::json!({"city": "Paris"}),
            }],
            tool_call_id: None,
        };
        let encoded = serde_json::to_string(&message).unwrap();
        assert_eq!(serde_json::from_str::<Message>(&encoded).unwrap(), message);
    }
}
