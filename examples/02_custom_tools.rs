//! Custom Tool Calling Example
//!
//! Demonstrates defining a custom `Tool` with JSON schema parameters,
//! registering it in a `ToolRegistry`, and having an `Agent` execute it
//! during its reasoning loop.
//!
//! Run with:
//! ```bash
//! cargo run --example 02_custom_tools
//! ```

use async_trait::async_trait;
use llmrc::prelude::*;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio_util::sync::CancellationToken;

/// A simple Calculator tool that sums two numbers.
struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "add".into(),
            description: Some("Add two numbers together".into()),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["a", "b"],
                "properties": {
                    "a": { "type": "number", "description": "The first number" },
                    "b": { "type": "number", "description": "The second number" }
                },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        let a = arguments.get("a").and_then(Value::as_f64).ok_or_else(|| {
            ToolError::InvalidArguments("missing or invalid parameter 'a'".into())
        })?;
        let b = arguments.get("b").and_then(Value::as_f64).ok_or_else(|| {
            ToolError::InvalidArguments("missing or invalid parameter 'b'".into())
        })?;

        let sum = a + b;
        println!("  [CalculatorTool executed] {a} + {b} = {sum}");
        Ok(sum.to_string())
    }
}

/// A simulated provider that makes a tool call on the first turn,
/// and produces a final completion on the second turn.
struct ToolCallingMockProvider {
    step: Arc<AtomicUsize>,
}

#[async_trait]
impl ChatProvider for ToolCallingMockProvider {
    fn kind(&self) -> llmrc::core::ProviderKind {
        llmrc::core::ProviderKind::OpenAi
    }

    fn capabilities(&self) -> llmrc::core::ProviderCapabilities {
        llmrc::core::ProviderCapabilities {
            streaming: false,
            embeddings: false,
            tool_calls: true,
            multimodal: false,
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let step = self.step.fetch_add(1, Ordering::SeqCst);
        if step == 0 {
            // Turn 1: request the 'add' tool
            let mut message = Message::new(MessageRole::Assistant, "");
            message.tool_calls = vec![ToolCall {
                id: "call_add_1".into(),
                name: "add".into(),
                arguments: serde_json::json!({"a": 40, "b": 2}),
            }];

            Ok(ChatResponse {
                id: Some("call-1".into()),
                model: request.model,
                message,
                finish_reason: Some(FinishReason::ToolCalls),
                usage: None,
            })
        } else {
            // Turn 2: inspect tool result from message history and produce final answer
            let tool_output = request
                .messages
                .iter()
                .rfind(|m| m.role == MessageRole::Tool)
                .map(Message::text)
                .unwrap_or_else(|| "unknown".into());

            let answer = format!("The calculation result is {tool_output}.");
            Ok(ChatResponse {
                id: Some("call-2".into()),
                model: request.model,
                message: Message::assistant(answer),
                finish_reason: Some(FinishReason::Stop),
                usage: None,
            })
        }
    }

    async fn chat_stream(&self, _: ChatRequest) -> Result<llmrc::core::BoxChatStream, LlmError> {
        Err(LlmError::Unsupported("not supported".into()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== llmrc: Custom Tool Calling Example ===\n");

    // 1. Build tool registry and register the calculator tool
    let mut registry = ToolRegistry::new();
    registry.register(CalculatorTool)?;
    println!("Registered tool: 'add'");

    // 2. Configure the agent
    let config = AgentConfig {
        model: "gpt-4o".into(),
        max_turns: 5,
        max_tool_calls: 10,
        ..Default::default()
    };

    let provider = ToolCallingMockProvider {
        step: Arc::new(AtomicUsize::new(0)),
    };

    let agent = Agent::new(provider)
        .with_registry(registry)
        .with_config(config);

    // 3. Run the agent
    println!("Running agent with prompt: 'What is 40 + 2?'\n");
    let result = agent
        .run("What is 40 + 2?", CancellationToken::new())
        .await?;

    // 4. View history and output
    println!("\n=== Final Response ===");
    println!("Assistant: {}", result.message.text());
    println!("Total turns: {}", result.turns);
    println!("Tool calls executed: {}", result.tool_calls);

    println!("\n=== Conversation History ===");
    for (i, msg) in result.history.iter().enumerate() {
        match msg.role {
            MessageRole::User => println!("[{i}] User: {}", msg.text()),
            MessageRole::Assistant if !msg.tool_calls.is_empty() => {
                println!("[{i}] Assistant called: {:?}", msg.tool_calls);
            }
            MessageRole::Assistant => println!("[{i}] Assistant: {}", msg.text()),
            MessageRole::Tool => println!("[{i}] Tool result: {}", msg.text()),
            MessageRole::System => println!("[{i}] System: {}", msg.text()),
        }
    }

    Ok(())
}
