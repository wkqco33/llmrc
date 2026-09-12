//! OpenAI and Azure OpenAI adapters backed by `async-openai`.

use async_openai::{
    Client,
    config::{AzureConfig, OpenAIConfig},
    types::{
        chat::{
            ChatCompletionMessageToolCalls, ChatCompletionRequestAssistantMessage,
            ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
            ChatCompletionRequestSystemMessage, ChatCompletionRequestToolMessage,
            ChatCompletionRequestUserMessage, ChatCompletionTool, ChatCompletionTools,
            CreateChatCompletionRequest, FinishReason as OpenAiFinishReason, FunctionCall,
            FunctionObject,
        },
        embeddings::{CreateEmbeddingRequest, EmbeddingInput},
    },
};
use async_trait::async_trait;
use futures::{StreamExt, stream};
use llmrc_core::{
    ChatProvider, ChatRequest, ChatResponse, ContentPart, Embedding, EmbeddingProvider,
    EmbeddingRequest, FinishReason, LlmError, Message, MessageRole, ProviderCapabilities,
    ProviderKind, StreamEvent, TokenCount, TokenUsage, ToolCall, ToolCallDelta, ToolDefinition,
};

#[derive(Clone)]
enum Backend {
    OpenAi(Client<OpenAIConfig>),
    Azure(Client<AzureConfig>),
}

/// An `async-openai` backed provider. Use [`OpenAiProvider::openai`] or
/// [`OpenAiProvider::azure`] to construct one without exposing SDK types.
#[derive(Clone)]
pub struct OpenAiProvider {
    backend: Backend,
}

impl OpenAiProvider {
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self {
            backend: Backend::OpenAi(Client::with_config(
                OpenAIConfig::new().with_api_key(api_key),
            )),
        }
    }

    pub fn openai_with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            backend: Backend::OpenAi(Client::with_config(
                OpenAIConfig::new()
                    .with_api_key(api_key)
                    .with_api_base(base_url),
            )),
        }
    }

    pub fn azure(
        api_key: impl Into<String>,
        endpoint: impl Into<String>,
        deployment: impl Into<String>,
        api_version: impl Into<String>,
    ) -> Self {
        Self {
            backend: Backend::Azure(Client::with_config(
                AzureConfig::new()
                    .with_api_key(api_key)
                    .with_api_base(endpoint)
                    .with_deployment_id(deployment)
                    .with_api_version(api_version),
            )),
        }
    }

    fn request(request: &ChatRequest) -> Result<CreateChatCompletionRequest, LlmError> {
        let messages = request
            .messages
            .iter()
            .map(to_openai_message)
            .collect::<Result<Vec<_>, _>>()?;
        let tools = (!request.tools.is_empty())
            .then(|| request.tools.iter().map(to_openai_tool).collect::<Vec<_>>());
        Ok(CreateChatCompletionRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_completion_tokens: request.max_tokens,
            tools,
            ..Default::default()
        })
    }

    async fn chat_inner(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let sdk_request = Self::request(&request)?;
        let result = match &self.backend {
            Backend::OpenAi(client) => client.chat().create(sdk_request).await,
            Backend::Azure(client) => client.chat().create(sdk_request).await,
        }
        .map_err(map_error)?;
        let choice = result
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Decode("provider returned no choices".into()))?;
        let message = response_message(choice.message)?;
        Ok(ChatResponse {
            id: Some(result.id),
            model: result.model,
            message,
            finish_reason: choice.finish_reason.map(finish_reason),
            usage: result.usage.map(openai_usage),
        })
    }

    async fn stream_inner(
        &self,
        request: ChatRequest,
    ) -> Result<llmrc_core::BoxChatStream, LlmError> {
        let mut sdk_request = Self::request(&request)?;
        sdk_request.stream = Some(true);
        sdk_request.stream_options = Some(async_openai::types::chat::ChatCompletionStreamOptions {
            include_usage: Some(true),
            include_obfuscation: None,
        });
        let result = match &self.backend {
            Backend::OpenAi(client) => client.chat().create_stream(sdk_request).await,
            Backend::Azure(client) => client.chat().create_stream(sdk_request).await,
        }
        .map_err(map_error)?;
        let events = result
            .map(|item| match item {
                Ok(chunk) => {
                    let mut output = Vec::new();
                    if let Some(usage) = chunk.usage {
                        output.push(Ok(StreamEvent::Usage(openai_usage(usage))));
                    }
                    for choice in chunk.choices {
                        if let Some(text) = choice.delta.content {
                            output.push(Ok(StreamEvent::TextDelta { text }));
                        }
                        if let Some(calls) = choice.delta.tool_calls {
                            output.extend(calls.into_iter().map(|call| {
                                Ok(StreamEvent::ToolCallDelta(ToolCallDelta {
                                    index: call.index,
                                    id: call.id,
                                    name: call.function.as_ref().and_then(|f| f.name.clone()),
                                    arguments: call.function.and_then(|f| f.arguments),
                                }))
                            }));
                        }
                        if choice.finish_reason.is_some() {
                            output.push(Ok(StreamEvent::Completed {
                                finish_reason: choice.finish_reason.map(finish_reason),
                            }));
                        }
                    }
                    stream::iter(output)
                }
                Err(error) => stream::iter(vec![Err(map_error(error))]),
            })
            .flatten();
        Ok(Box::pin(
            stream::once(async move {
                Ok(StreamEvent::Started {
                    id: None,
                    model: request.model,
                })
            })
            .chain(events),
        ))
    }
}

#[async_trait]
impl ChatProvider for OpenAiProvider {
    fn kind(&self) -> ProviderKind {
        match self.backend {
            Backend::OpenAi(_) => ProviderKind::OpenAi,
            Backend::Azure(_) => ProviderKind::AzureOpenAi,
        }
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
        self.chat_inner(request).await
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<llmrc_core::BoxChatStream, LlmError> {
        self.stream_inner(request).await
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiProvider {
    fn kind(&self) -> ProviderKind {
        ChatProvider::kind(self)
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<Vec<Embedding>, LlmError> {
        let sdk_request = CreateEmbeddingRequest {
            model: request.model,
            input: EmbeddingInput::StringArray(request.input),
            ..Default::default()
        };
        let response = match &self.backend {
            Backend::OpenAi(client) => client.embeddings().create(sdk_request).await,
            Backend::Azure(client) => client.embeddings().create(sdk_request).await,
        }
        .map_err(map_error)?;
        Ok(response
            .data
            .into_iter()
            .map(|item| Embedding {
                index: item.index as usize,
                vector: item.embedding,
            })
            .collect())
    }
}

fn to_openai_tool(tool: &ToolDefinition) -> ChatCompletionTools {
    ChatCompletionTools::Function(ChatCompletionTool {
        function: FunctionObject {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: Some(tool.parameters.clone()),
            strict: None,
        },
    })
}

fn to_openai_message(message: &Message) -> Result<ChatCompletionRequestMessage, LlmError> {
    let text = message.text();
    match message.role {
        MessageRole::System => Ok(ChatCompletionRequestSystemMessage::from(text).into()),
        MessageRole::User => {
            let content = if message.content.len() == 1 && message.content[0].as_text().is_some() {
                text.into()
            } else {
                let parts = message
                    .content
                    .iter()
                    .map(|part| match part {
                        ContentPart::Text { text } => Ok(
                            async_openai::types::chat::ChatCompletionRequestUserMessageContentPart::Text(
                                async_openai::types::chat::ChatCompletionRequestMessageContentPartText {
                                    text: text.clone(),
                                    prompt_cache_breakpoint: None,
                                },
                            ),
                        ),
                        ContentPart::Image { source } => {
                            let url = match source {
                                llmrc_core::ImageSource::Url { url, .. } => url.clone(),
                                llmrc_core::ImageSource::Base64 { media_type, data } => {
                                    format!("data:{media_type};base64,{data}")
                                }
                            };
                            Ok(
                                async_openai::types::chat::ChatCompletionRequestUserMessageContentPart::ImageUrl(
                                    async_openai::types::chat::ChatCompletionRequestMessageContentPartImage {
                                        image_url: async_openai::types::chat::ImageUrl {
                                            url,
                                            detail: None,
                                        },
                                        prompt_cache_breakpoint: None,
                                    },
                                ),
                            )
                        }
                    })
                    .collect::<Result<Vec<_>, LlmError>>()?;
                async_openai::types::chat::ChatCompletionRequestUserMessageContent::Array(parts)
            };
            Ok(ChatCompletionRequestUserMessage {
                content,
                name: message.name.clone(),
            }
            .into())
        }

        MessageRole::Assistant => {
            let tool_calls = (!message.tool_calls.is_empty()).then(|| {
                message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        ChatCompletionMessageToolCalls::Function(
                            async_openai::types::chat::ChatCompletionMessageToolCall {
                                id: call.id.clone(),
                                function: FunctionCall {
                                    name: call.name.clone(),
                                    arguments: call.arguments.to_string(),
                                },
                            },
                        )
                    })
                    .collect()
            });
            Ok(ChatCompletionRequestAssistantMessage {
                content: (!text.is_empty())
                    .then_some(ChatCompletionRequestAssistantMessageContent::Text(text)),
                tool_calls,
                ..Default::default()
            }
            .into())
        }
        MessageRole::Tool => Ok(ChatCompletionRequestToolMessage {
            content: text.into(),
            tool_call_id: message.tool_call_id.clone().ok_or_else(|| {
                LlmError::InvalidRequest("tool messages require tool_call_id".into())
            })?,
        }
        .into()),
    }
}

fn response_message(
    message: async_openai::types::chat::ChatCompletionResponseMessage,
) -> Result<Message, LlmError> {
    let mut result = Message::new(
        match message.role {
            async_openai::types::chat::Role::System => MessageRole::System,
            async_openai::types::chat::Role::User => MessageRole::User,
            async_openai::types::chat::Role::Assistant => MessageRole::Assistant,
            async_openai::types::chat::Role::Tool => MessageRole::Tool,
            async_openai::types::chat::Role::Function => MessageRole::Tool,
        },
        message.content.unwrap_or_default(),
    );
    if let Some(calls) = message.tool_calls {
        result.tool_calls = calls
            .into_iter()
            .filter_map(|call| match call {
                ChatCompletionMessageToolCalls::Function(call) => Some(ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments: serde_json::from_str(&call.function.arguments)
                        .unwrap_or_else(|_| serde_json::Value::String(call.function.arguments)),
                }),
                ChatCompletionMessageToolCalls::Custom(_) => None,
            })
            .collect();
    }
    Ok(result)
}

fn finish_reason(reason: OpenAiFinishReason) -> FinishReason {
    match reason {
        OpenAiFinishReason::Stop => FinishReason::Stop,
        OpenAiFinishReason::Length => FinishReason::Length,
        OpenAiFinishReason::ToolCalls | OpenAiFinishReason::FunctionCall => FinishReason::ToolCalls,
        OpenAiFinishReason::ContentFilter => FinishReason::ContentFilter,
    }
}

fn openai_usage(usage: async_openai::types::chat::CompletionUsage) -> TokenUsage {
    TokenUsage {
        prompt_tokens: usage.prompt_tokens as u64,
        completion_tokens: usage.completion_tokens as u64,
        total_tokens: usage.total_tokens as u64,
        count_type: TokenCount::ProviderReported,
    }
}

fn map_error(error: async_openai::error::OpenAIError) -> LlmError {
    use async_openai::error::OpenAIError;
    match error {
        OpenAIError::InvalidArgument(message) => LlmError::InvalidRequest(message),
        OpenAIError::Reqwest(error) => LlmError::Transport(Box::new(error)),
        OpenAIError::JSONDeserialize(error, _) => LlmError::Decode(Box::new(error)),
        OpenAIError::ApiError(error) => match error.status_code.as_u16() {
            401 | 403 => LlmError::Authentication,
            429 => LlmError::RateLimited { retry_after: None },
            500..=599 => LlmError::Server(error.to_string()),
            _ => LlmError::Provider(error.to_string()),
        },
        OpenAIError::StreamError(error) => LlmError::Transport(Box::new(error)),
        _ => LlmError::Provider(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmrc_core::{ContentPart, ImageSource};

    #[test]
    fn request_serializes_tools_and_multimodal_user_content() {
        let request = ChatRequest {
            model: "gpt-test".into(),
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![
                    ContentPart::text("Describe this"),
                    ContentPart::Image {
                        source: ImageSource::Base64 {
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
                parameters: serde_json::json!({"type":"object"}),
            }],
        };
        let value = serde_json::to_value(OpenAiProvider::request(&request).unwrap()).unwrap();
        assert_eq!(value["model"], "gpt-test");
        assert_eq!(value["tools"][0]["function"]["name"], "weather");
        assert_eq!(
            value["messages"][0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,aGVsbG8="
        );
    }
}
