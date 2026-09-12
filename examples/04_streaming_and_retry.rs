//! Streaming & Retry Policy Example
//!
//! Demonstrates configuring a robust `RetryPolicy` with:
//! - Exponential backoff & jitter
//! - Rate-limit `Retry-After` honoring
//! - Shared `RetryBudget`
//! - Safe stream collection without restarting user-visible streams
//!
//! Run with:
//! ```bash
//! cargo run --example 04_streaming_and_retry
//! ```

use futures::stream;
use llmrc::prelude::*;
use llmrc::runtime::{ExactTokenCounter, Jitter, RetryBudget};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== llmrc: Retry Policy & Token Accounting Example ===\n");

    // 1. Configure a RetryPolicy with Jitter and a RetryBudget
    let budget = RetryBudget::new(3);
    let policy = RetryPolicy::builder()
        .max_retries(3)
        .base_delay(Duration::from_millis(50))
        .max_delay(Duration::from_secs(2))
        .jitter(Jitter::Equal)
        .budget(budget.clone())
        .build();

    println!("Configured RetryPolicy:");
    println!("  Max retries: {}", policy.max_retries);
    println!("  Base delay: {:?}", policy.base_delay);
    println!("  Jitter: {:?}", policy.jitter);
    println!("  Budget remaining: {}", budget.remaining());

    // 2. Demonstrate simulated transient failure recovery
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();
    let cancellation = CancellationToken::new();

    println!("\nExecuting operation with transient timeout failure...");
    let result: Result<&str, LlmError> = policy
        .execute(
            || {
                let attempts = attempts_clone.clone();
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        println!(
                            "  Attempt #{}: Encountered transient Timeout error, retrying...",
                            count + 1
                        );
                        Err(LlmError::Timeout)
                    } else {
                        println!("  Attempt #{}: Success!", count + 1);
                        Ok("LLM completion payload successfully received")
                    }
                }
            },
            &cancellation,
        )
        .await;

    println!("Result: {:?}", result?);
    println!("Budget remaining after retries: {}", budget.remaining());

    // 3. Demonstrate Token Accounting
    println!("\n--- Token Accounting Demo ---");
    let mut accounting = TokenAccounting::default();

    // Exact count
    let exact_counter = ExactTokenCounter::new(|text| text.split_whitespace().count() as u64);
    let exact_res = exact_counter.count_text("Hello Rust agent world");
    accounting.record_prompt(exact_res);

    // Heuristic count
    let heuristic_counter = HeuristicTokenCounter;
    let heuristic_res = heuristic_counter.count_text("Thinking through tool reasoning");
    accounting.record_completion(heuristic_res);

    println!("Prompt tokens: {}", accounting.prompt_tokens);
    println!("Completion tokens: {}", accounting.completion_tokens);
    println!("Total tokens accumulated: {}", accounting.total());

    // 4. Demonstrate Stream collection safety
    println!("\n--- Safe Stream Retry Demo ---");
    let stream_attempts = Arc::new(AtomicUsize::new(0));
    let stream_res = policy
        .collect_stream(
            || {
                let attempts = stream_attempts.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok(stream::iter(vec![
                        Ok::<_, LlmError>("chunk 1: Hello "),
                        Ok("chunk 2: world!"),
                    ]))
                }
            },
            &cancellation,
            |_chunk| true, // once visible, stream will not restart on error
        )
        .await?;

    println!("Stream chunks successfully collected: {:?}", stream_res);

    Ok(())
}
