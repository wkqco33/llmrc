//! Bot Multi-Session & Message Splitting Example
//!
//! Demonstrates platform-neutral bot handling with `BotHandler`:
//! - Serialized turns per conversation key (preventing race conditions)
//! - Concurrent handling across different users/channels
//! - Automatic UTF-8 safe message splitting for Discord/Slack character limits
//!
//! Run with:
//! ```bash
//! cargo run --example 03_bot_conversation --features bots
//! ```

use async_trait::async_trait;
use llmrc::bots::{BotEvent, BotHandler, ConversationKey, split_message};
use llmrc::prelude::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

struct EchoProvider;

#[async_trait]
impl ChatProvider for EchoProvider {
    fn kind(&self) -> llmrc::core::ProviderKind {
        llmrc::core::ProviderKind::Ollama
    }
    fn capabilities(&self) -> llmrc::core::ProviderCapabilities {
        llmrc::core::ProviderCapabilities {
            streaming: false,
            embeddings: false,
            tool_calls: false,
            multimodal: false,
        }
    }
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let last = request
            .messages
            .last()
            .map(Message::text)
            .unwrap_or_default();
        let reply = format!("Bot reply to: '{last}'");
        Ok(ChatResponse {
            id: None,
            model: request.model,
            message: Message::assistant(reply),
            finish_reason: Some(FinishReason::Stop),
            usage: None,
        })
    }
    async fn chat_stream(&self, _: ChatRequest) -> Result<llmrc::core::BoxChatStream, LlmError> {
        Err(LlmError::Unsupported("not supported".into()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== llmrc: Bot Conversation Handler Example ===\n");

    // 1. Initialize an Agent and wrap it into a BotHandler
    let agent = Arc::new(Agent::new(EchoProvider));
    let handler = Arc::new(BotHandler::new(agent).with_max_response_bytes(100));

    // 2. Dispatch events from two different platforms concurrently
    let user_a_key = ConversationKey::new("discord", "channel-general");
    let user_b_key = ConversationKey::new("slack", "C01234567");

    let event_a = BotEvent::new(user_a_key.clone(), "Hello from Discord!");
    let event_b = BotEvent::new(user_b_key.clone(), "Hello from Slack!");

    let cancellation = CancellationToken::new();

    let (res_a, res_b) = tokio::join!(
        handler.handle(event_a, cancellation.clone()),
        handler.handle(event_b, cancellation.clone())
    );

    println!("Discord response: {:?}", res_a?.first().map(|r| &r.text));
    println!("Slack response: {:?}", res_b?.first().map(|r| &r.text));

    // 3. Demonstrate UTF-8 safe message splitting for platform limits
    println!("\n--- UTF-8 Safe Message Splitting Demo ---");
    let long_text = "Rust 🦀 is a multi-paradigm, general-purpose programming language designed for performance and safety, especially safe concurrency. It enforces memory safety without garbage collection.";
    let chunks = split_message(long_text, 60)?;

    println!("Original length: {} bytes", long_text.len());
    println!("Split into {} chunks (max 60 bytes each):", chunks.len());
    for (i, chunk) in chunks.iter().enumerate() {
        println!("  Chunk {}: \"{}\" ({} bytes)", i + 1, chunk, chunk.len());
    }

    Ok(())
}
