//! Small, provider-neutral runtime building blocks.
//!
//! Retry delays are bounded and cancellation-aware. Stream retries are only
//! attempted before a user-visible item is observed; callers provide the
//! visibility predicate because usage and metadata are not necessarily visible
//! to an end user.

mod retry;
mod token;

pub use retry::{
    Jitter, RetryBudget, RetryClassifier, RetryPolicy, RetryPolicyBuilder, RetryStats,
};
pub use token::{
    ExactTokenCounter, HeuristicTokenCounter, ProviderReportedTokenCounter, TokenAccounting,
    TokenCountResult, TokenCounter,
};
