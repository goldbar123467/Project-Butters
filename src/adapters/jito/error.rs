//! Jito Error Types
//!
//! Error handling for Jito bundle operations.

use thiserror::Error;

/// Errors that can occur during Jito bundle operations
#[derive(Error, Debug, Clone)]
pub enum JitoError {
    /// HTTP client error
    #[error("HTTP error: {0}")]
    HttpError(String),

    /// Block Engine API error
    #[error("Block Engine error: {message} (code: {code})")]
    ApiError {
        code: i32,
        message: String,
    },

    /// Bundle rejected by block engine
    #[error("Bundle rejected: {0}")]
    BundleRejected(String),

    /// Bundle simulation failed
    #[error("Bundle simulation failed: {0}")]
    SimulationFailed(String),

    /// Bundle dropped (not included in block)
    #[error("Bundle dropped: not included in block")]
    BundleDropped,

    /// Invalid bundle (empty, too large, etc.)
    #[error("Invalid bundle: {0}")]
    InvalidBundle(String),

    /// Invalid transaction format
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimited,

    /// Request timeout
    #[error("Request timed out")]
    Timeout,

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Network/connection error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Bundle status check failed
    #[error("Status check failed: {0}")]
    StatusCheckFailed(String),

    /// Maximum retries exceeded
    #[error("Max retries exceeded after {attempts} attempts: {last_error}")]
    MaxRetriesExceeded {
        attempts: u32,
        last_error: String,
    },
}

impl JitoError {
    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            JitoError::HttpError(_)
                | JitoError::Timeout
                | JitoError::NetworkError(_)
                | JitoError::RateLimited
        )
    }

    /// Check if error indicates bundle was invalid
    pub fn is_bundle_error(&self) -> bool {
        matches!(
            self,
            JitoError::BundleRejected(_)
                | JitoError::SimulationFailed(_)
                | JitoError::InvalidBundle(_)
                | JitoError::InvalidTransaction(_)
        )
    }
}

impl From<reqwest::Error> for JitoError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            JitoError::Timeout
        } else if err.is_connect() {
            JitoError::NetworkError(err.to_string())
        } else {
            JitoError::HttpError(err.to_string())
        }
    }
}

impl From<serde_json::Error> for JitoError {
    fn from(err: serde_json::Error) -> Self {
        JitoError::SerializationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(JitoError::Timeout.is_retryable());
        assert!(JitoError::RateLimited.is_retryable());
        assert!(JitoError::NetworkError("test".into()).is_retryable());

        assert!(!JitoError::BundleRejected("test".into()).is_retryable());
        assert!(!JitoError::InvalidBundle("test".into()).is_retryable());
    }

    #[test]
    fn test_bundle_errors() {
        assert!(JitoError::BundleRejected("test".into()).is_bundle_error());
        assert!(JitoError::SimulationFailed("test".into()).is_bundle_error());
        assert!(JitoError::InvalidBundle("test".into()).is_bundle_error());

        assert!(!JitoError::Timeout.is_bundle_error());
        assert!(!JitoError::RateLimited.is_bundle_error());
    }

    #[test]
    fn test_error_display() {
        let err = JitoError::ApiError {
            code: -32000,
            message: "Bundle simulation failed".to_string(),
        };
        assert!(err.to_string().contains("-32000"));
        assert!(err.to_string().contains("Bundle simulation failed"));
    }
}
