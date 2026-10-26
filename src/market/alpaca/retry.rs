//! Shared classification and bounded exponential retry delay.
use reqwest::{StatusCode, header::HeaderMap};
use std::time::Duration;

/// Whether an HTTP response should be retried.
pub fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Whether a transport failure is transient enough to retry.
pub fn retryable_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

/// Delay after a failed attempt, honoring a valid numeric `Retry-After` header.
pub fn retry_delay(headers: &HeaderMap, attempt: u32) -> Duration {
    if let Some(seconds) = headers.get("Retry-After")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok()) {
        return seconds;
    }
    Duration::from_secs(1_u64 << attempt.saturating_sub(1).min(6)).min(Duration::from_secs(60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permanent_client_errors_are_not_retried() {
        for status in [StatusCode::BAD_REQUEST, StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN, StatusCode::NOT_FOUND] {
            assert!(!retryable_status(status));
        }
        assert!(retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(retryable_status(StatusCode::SERVICE_UNAVAILABLE));
    }

    #[test]
    fn invalid_retry_after_uses_backoff() {
        for value in ["-1", "NaN", "inf", "invalid"] {
            let mut headers = HeaderMap::new();
            headers.insert("Retry-After", value.parse().unwrap());
            assert_eq!(retry_delay(&headers, 3), Duration::from_secs(4));
        }
    }

    #[test]
    fn valid_retry_after_overrides_backoff() {
        let mut headers = HeaderMap::new();
        headers.insert("Retry-After", "2.5".parse().unwrap());
        assert_eq!(retry_delay(&headers, 8), Duration::from_millis(2500));
    }
}
