use std::fmt;

use thiserror::Error;

/// A coarse category that callers can use for retry and telemetry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Configuration,
    Authentication,
    RateLimited,
    Server,
    Transport,
    Decode,
    Cancelled,
    Timeout,
    Unsupported,
    InvalidRequest,
    Provider,
}

/// Errors exposed by provider implementations.
///
/// # Examples
///
/// ```
/// use llmrc_core::{ErrorKind, LlmError};
///
/// let err = LlmError::Timeout;
/// assert_eq!(err.kind(), ErrorKind::Timeout);
/// assert!(err.is_retryable());
///
/// let auth_err = LlmError::Authentication;
/// assert_eq!(auth_err.kind(), ErrorKind::Authentication);
/// assert!(!auth_err.is_retryable());
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LlmError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("authentication failed")]
    Authentication,
    #[error("rate limited")]
    RateLimited {
        retry_after: Option<std::time::Duration>,
    },
    #[error("provider server error: {0}")]
    Server(String),
    #[error("transport error: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("response decode error: {0}")]
    Decode(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("request was cancelled")]
    Cancelled,
    #[error("request timed out")]
    Timeout,
    #[error("provider feature is unsupported: {0}")]
    Unsupported(String),
    #[error("provider error: {0}")]
    Provider(String),
}

impl LlmError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::InvalidRequest(_) => ErrorKind::InvalidRequest,
            Self::Configuration(_) => ErrorKind::Configuration,
            Self::Authentication => ErrorKind::Authentication,
            Self::RateLimited { .. } => ErrorKind::RateLimited,
            Self::Server(_) => ErrorKind::Server,
            Self::Transport(_) => ErrorKind::Transport,
            Self::Decode(_) => ErrorKind::Decode,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::Timeout => ErrorKind::Timeout,
            Self::Unsupported(_) => ErrorKind::Unsupported,
            Self::Provider(_) => ErrorKind::Provider,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::RateLimited | ErrorKind::Server | ErrorKind::Transport | ErrorKind::Timeout
        )
    }
}

/// A secret string that deliberately does not reveal its contents in `Debug`.
///
/// # Examples
///
/// ```
/// use llmrc_core::Secret;
///
/// let secret = Secret::new("sk-secret-key-12345");
/// assert_eq!(format!("{secret:?}"), "Secret(REDACTED)");
/// assert_eq!(secret.expose(), "sk-secret-key-12345");
/// ```
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(REDACTED)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_is_redacted_and_retry_classification_is_stable() {
        let secret = Secret::new("do-not-print");
        assert!(!format!("{secret:?}").contains("do-not-print"));
        assert!(LlmError::Timeout.is_retryable());
        assert!(!LlmError::Authentication.is_retryable());
        assert_eq!(LlmError::Cancelled.kind(), ErrorKind::Cancelled);
    }
}
