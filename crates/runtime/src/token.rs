use llmrc_core::{ChatRequest, Message, TokenCount, TokenUsage, ToolDefinition};
use std::{fmt, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenCountResult {
    pub tokens: u64,
    pub count_type: TokenCount,
}

/// Token counting is deliberately pluggable because providers use different
/// encodings. Implementations must not retain or print secret prompt content.
pub trait TokenCounter: Send + Sync {
    fn count_text(&self, text: &str) -> TokenCountResult;

    fn count_messages(&self, messages: &[Message], tools: &[ToolDefinition]) -> TokenCountResult {
        let text_tokens: u64 = messages
            .iter()
            .map(|message| self.count_text(&message.text()).tokens + 4)
            .sum();
        let tool_tokens = tools
            .iter()
            .map(|tool| {
                self.count_text(&tool.name).tokens
                    + self.count_text(&tool.parameters.to_string()).tokens
            })
            .sum::<u64>();
        TokenCountResult {
            tokens: text_tokens + tool_tokens,
            count_type: self.count_text("").count_type,
        }
    }

    fn count_request(&self, request: &ChatRequest) -> TokenCountResult {
        self.count_messages(&request.messages, &request.tools)
    }
}

/// A documented heuristic for providers without a local tokenizer: UTF-8
/// characters divided by four, with a minimum of one token for non-empty text.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeuristicTokenCounter;

impl TokenCounter for HeuristicTokenCounter {
    fn count_text(&self, text: &str) -> TokenCountResult {
        TokenCountResult {
            tokens: if text.is_empty() {
                0
            } else {
                text.chars().count().div_ceil(4) as u64
            },
            count_type: TokenCount::Estimated,
        }
    }
}

/// An exact counter supplied by an application or tokenizer crate.
#[derive(Clone)]
pub struct ExactTokenCounter {
    counter: Arc<dyn Fn(&str) -> u64 + Send + Sync>,
}

impl ExactTokenCounter {
    pub fn new(counter: impl Fn(&str) -> u64 + Send + Sync + 'static) -> Self {
        Self {
            counter: Arc::new(counter),
        }
    }
}

impl fmt::Debug for ExactTokenCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ExactTokenCounter(..)")
    }
}

impl TokenCounter for ExactTokenCounter {
    fn count_text(&self, text: &str) -> TokenCountResult {
        TokenCountResult {
            tokens: (self.counter)(text),
            count_type: TokenCount::Exact,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderReportedTokenCounter {
    usage: TokenUsage,
}

impl ProviderReportedTokenCounter {
    pub fn new(usage: TokenUsage) -> Self {
        Self { usage }
    }
    pub fn usage(&self) -> TokenUsage {
        self.usage.clone()
    }
}

impl TokenCounter for ProviderReportedTokenCounter {
    fn count_text(&self, _text: &str) -> TokenCountResult {
        TokenCountResult {
            tokens: self.usage.completion_tokens,
            count_type: TokenCount::ProviderReported,
        }
    }

    fn count_messages(&self, _messages: &[Message], _tools: &[ToolDefinition]) -> TokenCountResult {
        TokenCountResult {
            tokens: self.usage.prompt_tokens,
            count_type: TokenCount::ProviderReported,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenAccounting {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub prompt_count: Option<TokenCount>,
    pub completion_count: Option<TokenCount>,
}

impl TokenAccounting {
    pub fn record_prompt(&mut self, result: TokenCountResult) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(result.tokens);
        self.prompt_count = Some(merge_count(self.prompt_count, result.count_type));
    }
    pub fn record_completion(&mut self, result: TokenCountResult) {
        self.completion_tokens = self.completion_tokens.saturating_add(result.tokens);
        self.completion_count = Some(merge_count(self.completion_count, result.count_type));
    }
    pub fn record_usage(&mut self, usage: &TokenUsage) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(usage.prompt_tokens);
        self.completion_tokens = self
            .completion_tokens
            .saturating_add(usage.completion_tokens);
        self.prompt_count = Some(merge_count(self.prompt_count, usage.count_type));
        self.completion_count = Some(merge_count(self.completion_count, usage.count_type));
    }
    pub fn total(&self) -> u64 {
        self.prompt_tokens.saturating_add(self.completion_tokens)
    }
}

fn merge_count(current: Option<TokenCount>, next: TokenCount) -> TokenCount {
    match (current, next) {
        (Some(TokenCount::Estimated), _) | (_, TokenCount::Estimated) => TokenCount::Estimated,
        (Some(TokenCount::ProviderReported), _) | (_, TokenCount::ProviderReported) => {
            TokenCount::ProviderReported
        }
        _ => TokenCount::Exact,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_preserve_semantics_without_debugging_prompt_content() {
        let estimated = HeuristicTokenCounter.count_text("abcdefgh");
        assert_eq!(
            estimated,
            TokenCountResult {
                tokens: 2,
                count_type: TokenCount::Estimated
            }
        );
        let exact = ExactTokenCounter::new(|text| text.split_whitespace().count() as u64);
        assert_eq!(
            exact.count_text("secret value").count_type,
            TokenCount::Exact
        );
        assert!(!format!("{exact:?}").contains("secret"));
    }

    #[test]
    fn accounting_accumulates_provider_usage() {
        let mut accounting = TokenAccounting::default();
        let usage = TokenUsage {
            prompt_tokens: 5,
            completion_tokens: 2,
            total_tokens: 7,
            count_type: TokenCount::ProviderReported,
        };
        accounting.record_usage(&usage);
        accounting.record_usage(&usage);
        assert_eq!(accounting.prompt_tokens, 10);
        assert_eq!(accounting.completion_tokens, 4);
        assert_eq!(accounting.total(), 14);
    }
}
