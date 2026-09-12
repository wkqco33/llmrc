//! Ollama adapter backed by `ollama-rs`.

use async_trait::async_trait;
use futures::StreamExt;
use llmrc_core::{
    ChatProvider, ChatRequest, ChatResponse, ContentPart, Embedding, EmbeddingProvider,
    EmbeddingRequest, FinishReason, LlmError, Message, MessageRole, ProviderCapabilities,
    ProviderKind, StreamEvent, TokenCount, TokenUsage, ToolDefinition,
};
use ollama_rs::{
    Ollama,
    generation::{
        chat::{ChatMessage, MessageRole as OllamaRole, request::ChatMessageRequest},
        embeddings::request::{EmbeddingsInput, GenerateEmbeddingsRequest},
        images::Image,
        tools::{ToolFunctionInfo, ToolInfo, ToolType},
    },
};

/// A provider for a local or remote Ollama server.
#[derive(Clone)]
pub struct OllamaProvider {
    client: Ollama,
}

impl OllamaProvider {
    /// Construct a provider from an Ollama base URL, such as
    /// `http://127.0.0.1:11434`.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, LlmError> {
        let client = Ollama::try_new(base_url.as_ref())
            .map_err(|error| LlmError::Configuration(error.to_string()))?;
        Ok(Self { client })
    }

    pub fn localhost() -> Self {
        Self {
            client: Ollama::default(),
        }
    }

    fn request(request: &ChatRequest) -> Result<ChatMessageRequest, LlmError> {
        let messages = request
            .messages
            .iter()
            .map(to_ollama_message)
            .collect::<Result<Vec<_>, _>>()?;
        let tools = request
            .tools
            .iter()
            .map(to_ollama_tool)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ChatMessageRequest::new(request.model.clone(), messages).tools(tools))
    }
}

#[async_trait]
impl ChatProvider for OllamaProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Ollama
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            embeddings: true,
            tool_calls: true,
            multimodal: true,
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let response = self
            .client
            .send_chat_messages(Self::request(&request)?)
            .await
            .map_err(map_error)?;
        let message = response_message(response.message)?;
        let usage = response.final_data.map(|usage| TokenUsage {
            prompt_tokens: usage.prompt_eval_count,
            completion_tokens: usage.eval_count,
            total_tokens: usage.prompt_eval_count + usage.eval_count,
            count_type: TokenCount::ProviderReported,
        });
        Ok(ChatResponse {
            id: None,
            model: response.model,
            message,
            finish_reason: Some(FinishReason::Stop),
            usage,
        })
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<llmrc_core::BoxChatStream, LlmError> {
        let model = request.model.clone();
        let stream = self
            .client
            .send_chat_messages_stream(Self::request(&request)?)
            .await
            .map_err(map_error)?;
        let events = stream.map(move |item| match item {
            Ok(response) => {
                let mut events = Vec::new();
                if !response.message.content.is_empty() {
                    events.push(Ok(StreamEvent::TextDelta {
                        text: response.message.content,
                    }));
                }
                for call in response.message.tool_calls {
                    events.push(Ok(StreamEvent::ToolCallDelta(llmrc_core::ToolCallDelta {
                        index: 0,
                        id: None,
                        name: Some(call.function.name),
                        arguments: Some(call.function.arguments.to_string()),
                    })));
                }
                if response.done {
                    if let Some(usage) = response.final_data {
                        events.push(Ok(StreamEvent::Usage(TokenUsage {
                            prompt_tokens: usage.prompt_eval_count,
                            completion_tokens: usage.eval_count,
                            total_tokens: usage.prompt_eval_count + usage.eval_count,
                            count_type: TokenCount::ProviderReported,
                        })));
                    }
                    events.push(Ok(StreamEvent::Completed {
                        finish_reason: Some(FinishReason::Stop),
                    }));
                }
                futures::stream::iter(events)
            }
            Err(_) => futures::stream::iter(vec![Err(LlmError::Transport(
                "Ollama stream read failed".into(),
            ))]),
        });
        Ok(Box::pin(
            futures::stream::once(async move { Ok(StreamEvent::Started { id: None, model }) })
                .chain(events.flatten()),
        ))
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Ollama
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<Vec<Embedding>, LlmError> {
        let response = self
            .client
            .generate_embeddings(GenerateEmbeddingsRequest::new(
                request.model,
                EmbeddingsInput::Multiple(request.input),
            ))
            .await
            .map_err(map_error)?;
        Ok(response
            .embeddings
            .into_iter()
            .enumerate()
            .map(|(index, vector)| Embedding { index, vector })
            .collect())
    }
}

fn to_ollama_message(message: &Message) -> Result<ChatMessage, LlmError> {
    let role = match message.role {
        MessageRole::System => OllamaRole::System,
        MessageRole::User => OllamaRole::User,
        MessageRole::Assistant => OllamaRole::Assistant,
        MessageRole::Tool => OllamaRole::Tool,
    };
    let mut result = ChatMessage::new(role, message.text());
    let images = message
        .content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Image {
                source: llmrc_core::ImageSource::Base64 { data, .. },
            } => Some(Image::from_base64(data.clone())),
            ContentPart::Image { .. } => None,
            ContentPart::Text { .. } => None,
        })
        .collect::<Vec<_>>();
    if message.content.iter().any(|part| {
        matches!(
            part,
            ContentPart::Image {
                source: llmrc_core::ImageSource::Url { .. }
            }
        )
    }) {
        return Err(LlmError::Unsupported(
            "Ollama accepts base64 images; URL images must be downloaded by the caller".into(),
        ));
    }
    if !images.is_empty() {
        result = result.with_images(images);
    }
    Ok(result)
}

fn to_ollama_tool(tool: &ToolDefinition) -> Result<ToolInfo, LlmError> {
    let parameters = serde_json::from_value::<schemars::Schema>(tool.parameters.clone())
        .map_err(|error| LlmError::InvalidRequest(format!("invalid tool schema: {error}")))?;
    Ok(ToolInfo {
        tool_type: ToolType::Function,
        function: ToolFunctionInfo {
            name: tool.name.clone(),
            description: tool.description.clone().unwrap_or_default(),
            parameters,
        },
    })
}

fn response_message(
    message: ollama_rs::generation::chat::ChatMessage,
) -> Result<Message, LlmError> {
    let role = match message.role {
        OllamaRole::System => MessageRole::System,
        OllamaRole::User => MessageRole::User,
        OllamaRole::Assistant => MessageRole::Assistant,
        OllamaRole::Tool => MessageRole::Tool,
    };
    let mut result = Message::new(role, message.content);
    result.tool_calls = message
        .tool_calls
        .into_iter()
        .enumerate()
        .map(|(index, call)| llmrc_core::ToolCall {
            id: index.to_string(),
            name: call.function.name,
            arguments: call.function.arguments,
        })
        .collect();
    Ok(result)
}

fn map_error(error: ollama_rs::error::OllamaError) -> LlmError {
    match error {
        ollama_rs::error::OllamaError::JsonError(error) => LlmError::Decode(Box::new(error)),
        ollama_rs::error::OllamaError::ReqwestError(error) => LlmError::Transport(Box::new(error)),
        ollama_rs::error::OllamaError::InternalError(error) => {
            LlmError::Server(format!("{error:?}"))
        }
        ollama_rs::error::OllamaError::ToolCallError(error) => {
            LlmError::Provider(error.to_string())
        }
        ollama_rs::error::OllamaError::Other(message) => LlmError::Provider(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_contains_tool_schema_and_base64_image() {
        let request = ChatRequest {
            model: "llama-test".into(),
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![
                    ContentPart::text("What is this?"),
                    ContentPart::Image {
                        source: llmrc_core::ImageSource::Base64 {
                            media_type: "image/png".into(),
                            data: "aGVsbG8=".into(),
                        },
                    },
                ],
                name: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
            }],
            temperature: None,
            max_tokens: None,
            tools: vec![ToolDefinition {
                name: "weather".into(),
                description: Some("Get weather".into()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"city": {"type": "string"}}
                }),
            }],
        };
        let sdk_request = OllamaProvider::request(&request).unwrap();
        let value = serde_json::to_value(sdk_request).unwrap();
        assert_eq!(value["model"], "llama-test");
        assert_eq!(value["messages"][0]["images"][0], "aGVsbG8=");
        assert_eq!(value["tools"][0]["function"]["name"], "weather");
    }
}
