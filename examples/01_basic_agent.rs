//! Basic Agent Example
//!
//! Demonstrates configuring an `Agent`, sending a prompt, and inspecting
//! the execution result including token accounting and lifecycle events.
//!
//! Run with:
//! ```bash
//! cargo run --example 01_basic_agent
//! ```

use async_trait::async_trait;
use llmrc::prelude::*;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// A lightweight mock provider to allow running this example offline.
struct DemoProvider;

#[async_trait]
impl ChatProvider for DemoProvider {
    fn kind(&self) -> llmrc::core::ProviderKind {
        llmrc::core::ProviderKind::Ollama
    }

    fn capabilities(&self) -> llmrc::core::ProviderCapabilities {
        llmrc::core::ProviderCapabilities {
            streaming: true,
            embeddings: false,
            tool_calls: true,
            multimodal: false,
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let user_prompt = request
            .messages
            .last()
            .map(|m| m.text())
            .unwrap_or_default();

        let reply = format!("Hello from llmrc! You said: '{user_prompt}'");

        Ok(ChatResponse {
            id: Some("demo-response-1".into()),
            model: request.model,
            message: Message::assistant(reply),
            finish_reason: Some(FinishReason::Stop),
            usage: None,
        })
    }

    async fn chat_stream(&self, _: ChatRequest) -> Result<llmrc::core::BoxChatStream, LlmError> {
        Err(LlmError::Unsupported(
            "streaming not implemented for demo".into(),
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== llmrc: Basic Agent Example ===\n");

    // 1. Configure the agent with turn limits, tool timeouts, and retries.
    let config = AgentConfig {
        model: "gpt-4o-mini".into(),
        max_turns: 4,
        tool_timeout: Some(Duration::from_secs(10)),
        retry_policy: RetryPolicy::builder()
            .max_retries(2)
            .base_delay(Duration::from_millis(50))
            .build(),
        ..Default::default()
    };

    // 2. Initialize the agent
    let agent = Agent::new(DemoProvider)
        .with_config(config)
        .with_token_counter(HeuristicTokenCounter);

    println!("Agent initialized with model: gpt-4o-mini");
    println!("Sending user prompt: 'Explain Rust ownership in one sentence.'");

    // 3. Run with cancellation token
    let cancellation = CancellationToken::new();
    let result = agent
        .run("Explain Rust ownership in one sentence.", cancellation)
        .await?;

    // 4. Inspect the output
    println!("\n=== Agent Result ===");
    println!("Assistant: {}", result.message.text());
    println!("Turns taken: {}", result.turns);
    println!("Tool calls executed: {}", result.tool_calls);
    println!("Total tokens (estimated): {}", result.usage.total());
    println!("Recorded events: {}", result.events.len());

    Ok(())
}
