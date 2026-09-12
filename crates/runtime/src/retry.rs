use futures::{Stream, StreamExt};
use llmrc_core::LlmError;
use std::{
    fmt,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

/// Amount of randomisation applied to exponential backoff.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Jitter {
    None,
    /// Random delay in `[0, exponential_delay]`.
    Full,
    /// Random delay in `[exponential_delay / 2, exponential_delay]`.
    #[default]
    Equal,
}

/// A shared retry allowance. Cloning a budget shares the allowance.
///
/// # Examples
///
/// ```
/// use llmrc_runtime::RetryBudget;
///
/// let budget = RetryBudget::new(5);
/// assert_eq!(budget.remaining(), 5);
/// ```
#[derive(Clone)]
pub struct RetryBudget(Arc<AtomicU32>);

impl RetryBudget {
    pub fn new(retries: u32) -> Self {
        Self(Arc::new(AtomicU32::new(retries)))
    }

    pub fn remaining(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    fn consume(&self) -> bool {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_sub(1)
            })
            .is_ok()
    }
}

impl fmt::Debug for RetryBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetryBudget")
            .field("remaining", &self.remaining())
            .finish()
    }
}

/// A hook for classifying provider errors without exposing provider-specific
/// error types. The default is [`LlmError::is_retryable`].
pub type RetryClassifier = Arc<dyn Fn(&LlmError) -> bool + Send + Sync>;

/// Bounded exponential backoff policy with jitter and budget.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use llmrc_runtime::{Jitter, RetryPolicy};
///
/// let policy = RetryPolicy::builder()
///     .max_retries(3)
///     .base_delay(Duration::from_millis(50))
///     .jitter(Jitter::Equal)
///     .build();
/// assert_eq!(policy.max_retries, 3);
/// assert_eq!(policy.base_delay, Duration::from_millis(50));
/// ```
#[derive(Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub jitter: Jitter,
    pub budget: Option<RetryBudget>,
    classifier: RetryClassifier,
}

impl fmt::Debug for RetryPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetryPolicy")
            .field("max_retries", &self.max_retries)
            .field("base_delay", &self.base_delay)
            .field("max_delay", &self.max_delay)
            .field("jitter", &self.jitter)
            .field("budget", &self.budget)
            .finish()
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::builder().build()
    }
}

pub struct RetryPolicyBuilder {
    policy: RetryPolicy,
}

impl Default for RetryPolicyBuilder {
    fn default() -> Self {
        Self {
            policy: RetryPolicy {
                max_retries: 2,
                base_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(30),
                jitter: Jitter::default(),
                budget: None,
                classifier: Arc::new(LlmError::is_retryable),
            },
        }
    }
}

impl RetryPolicyBuilder {
    pub fn max_retries(mut self, value: u32) -> Self {
        self.policy.max_retries = value;
        self
    }
    pub fn base_delay(mut self, value: Duration) -> Self {
        self.policy.base_delay = value;
        self
    }
    pub fn max_delay(mut self, value: Duration) -> Self {
        self.policy.max_delay = value;
        self
    }
    pub fn jitter(mut self, value: Jitter) -> Self {
        self.policy.jitter = value;
        self
    }
    pub fn budget(mut self, value: RetryBudget) -> Self {
        self.policy.budget = Some(value);
        self
    }
    pub fn classifier<F>(mut self, value: F) -> Self
    where
        F: Fn(&LlmError) -> bool + Send + Sync + 'static,
    {
        self.policy.classifier = Arc::new(value);
        self
    }
    pub fn build(self) -> RetryPolicy {
        self.policy
    }
}

impl RetryPolicy {
    pub fn new(max_retries: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self::builder()
            .max_retries(max_retries)
            .base_delay(base_delay)
            .max_delay(max_delay)
            .build()
    }

    pub fn builder() -> RetryPolicyBuilder {
        RetryPolicyBuilder::default()
    }

    pub fn should_retry(&self, error: &LlmError) -> bool {
        (self.classifier)(error)
    }

    /// Computes the delay before `retry_number` (one-based), honoring
    /// `Retry-After` on rate-limit errors and the configured cap.
    pub fn delay_for(&self, retry_number: u32, error: &LlmError) -> Duration {
        let exponential = self
            .base_delay
            .checked_mul(2u32.saturating_pow(retry_number.saturating_sub(1)))
            .unwrap_or(self.max_delay)
            .min(self.max_delay);
        let delay = match self.jitter {
            Jitter::None => exponential,
            Jitter::Full => random_duration(exponential),
            Jitter::Equal => {
                let half = exponential / 2;
                half + random_duration(exponential.saturating_sub(half))
            }
        };
        match error {
            LlmError::RateLimited {
                retry_after: Some(retry_after),
            } => (*retry_after).min(self.max_delay),
            _ => delay.min(self.max_delay),
        }
    }

    pub async fn execute<F, Fut, T>(
        &self,
        mut operation: F,
        cancellation: &CancellationToken,
    ) -> Result<T, LlmError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, LlmError>>,
    {
        let mut retries = 0;
        loop {
            if cancellation.is_cancelled() {
                return Err(LlmError::Cancelled);
            }
            let result = tokio::select! {
                _ = cancellation.cancelled() => return Err(LlmError::Cancelled),
                result = operation() => result,
            };
            match result {
                Ok(value) => return Ok(value),
                Err(error)
                    if retries < self.max_retries
                        && self.should_retry(&error)
                        && self.budget.as_ref().is_none_or(RetryBudget::consume) =>
                {
                    retries += 1;
                    let delay = self.delay_for(retries, &error);
                    tokio::select! {
                        _ = cancellation.cancelled() => return Err(LlmError::Cancelled),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Collects a stream with safe retry semantics. Once the visibility hook
    /// returns true for an item, a later stream error is returned directly and
    /// the source is never restarted.
    pub async fn collect_stream<F, Fut, S, T>(
        &self,
        mut operation: F,
        cancellation: &CancellationToken,
        mut user_visible: impl FnMut(&T) -> bool,
    ) -> Result<Vec<T>, LlmError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<S, LlmError>>,
        S: Stream<Item = Result<T, LlmError>>,
    {
        let mut retries = 0;
        loop {
            if cancellation.is_cancelled() {
                return Err(LlmError::Cancelled);
            }
            let stream_result = tokio::select! {
                _ = cancellation.cancelled() => return Err(LlmError::Cancelled),
                result = operation() => result,
            };
            let stream = match stream_result {
                Ok(stream) => stream,
                Err(error)
                    if retries < self.max_retries
                        && self.should_retry(&error)
                        && self.budget.as_ref().is_none_or(RetryBudget::consume) =>
                {
                    retries += 1;
                    self.sleep_or_cancel(self.delay_for(retries, &error), cancellation)
                        .await?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let mut stream = Box::pin(stream);
            let mut output = Vec::new();
            while let Some(item) = stream.next().await {
                match item {
                    Ok(item) => {
                        let visible = user_visible(&item);
                        output.push(item);
                        if visible {
                            // A visible item makes restarting unsafe. Continue
                            // draining only to surface the original stream error.
                            while let Some(next) = tokio::select! {
                                _ = cancellation.cancelled() => return Err(LlmError::Cancelled),
                                next = stream.next() => next,
                            } {
                                match next {
                                    Ok(item) => output.push(item),
                                    Err(error) => return Err(error),
                                }
                            }
                            return Ok(output);
                        }
                    }
                    Err(error)
                        if retries < self.max_retries
                            && self.should_retry(&error)
                            && self.budget.as_ref().is_none_or(RetryBudget::consume) =>
                    {
                        retries += 1;
                        self.sleep_or_cancel(self.delay_for(retries, &error), cancellation)
                            .await?;
                        continue;
                    }
                    Err(error) => return Err(error),
                }
            }
            return Ok(output);
        }
    }

    async fn sleep_or_cancel(
        &self,
        delay: Duration,
        cancellation: &CancellationToken,
    ) -> Result<(), LlmError> {
        tokio::select! {
            _ = cancellation.cancelled() => Err(LlmError::Cancelled),
            _ = tokio::time::sleep(delay) => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetryStats {
    pub attempts: u32,
    pub retries: u32,
}

fn random_duration(max: Duration) -> Duration {
    if max.is_zero() {
        return Duration::ZERO;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as u64);
    let upper = max.as_nanos().min(u64::MAX as u128) as u64;
    Duration::from_nanos(if upper == u64::MAX {
        nanos
    } else {
        nanos % (upper + 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn retries_transient_errors_and_honors_budget() {
        let calls = Arc::new(AtomicUsize::new(0));
        let policy = RetryPolicy::builder()
            .max_retries(4)
            .base_delay(Duration::ZERO)
            .jitter(Jitter::None)
            .budget(RetryBudget::new(1))
            .build();
        let token = CancellationToken::new();
        let result = policy
            .execute(
                || {
                    let calls = calls.clone();
                    async move {
                        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                            Err(LlmError::Timeout)
                        } else {
                            Ok(7)
                        }
                    }
                },
                &token,
            )
            .await;
        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(policy.budget.as_ref().unwrap().remaining(), 0);
    }

    #[tokio::test]
    async fn visible_stream_is_not_restarted() {
        let calls = Arc::new(AtomicUsize::new(0));
        let policy = RetryPolicy::builder()
            .max_retries(3)
            .base_delay(Duration::ZERO)
            .jitter(Jitter::None)
            .build();
        let result = policy
            .collect_stream(
                || {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(stream::iter(vec![
                            Ok::<_, LlmError>("visible"),
                            Err(LlmError::Transport("disconnect".into())),
                        ]))
                    }
                },
                &CancellationToken::new(),
                |_| true,
            )
            .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancellation_interrupts_an_in_flight_attempt() {
        let cancellation = CancellationToken::new();
        let child = cancellation.clone();
        let policy = RetryPolicy::builder().max_retries(0).build();
        let task = tokio::spawn(async move {
            policy
                .execute(
                    || async {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        Ok::<_, LlmError>(())
                    },
                    &child,
                )
                .await
        });
        tokio::time::sleep(Duration::from_millis(1)).await;
        cancellation.cancel();
        assert!(matches!(task.await.unwrap(), Err(LlmError::Cancelled)));
    }

    #[test]
    fn retry_after_is_used_for_rate_limits() {
        let policy = RetryPolicy::builder()
            .max_delay(Duration::from_secs(10))
            .jitter(Jitter::None)
            .build();
        let error = LlmError::RateLimited {
            retry_after: Some(Duration::from_secs(3)),
        };
        assert_eq!(policy.delay_for(1, &error), Duration::from_secs(3));
    }
}
