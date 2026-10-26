//! Provider rate-limit headers and blocking request pacing.
use crate::market::error::MarketError;
use reqwest::header::HeaderMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Parsed rate-limit response headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitHeaders {
    /// Request allowance.
    pub limit: Option<u64>,
    /// Remaining requests.
    pub remaining: Option<u64>,
    /// Reset time in Unix seconds.
    pub reset_unix_epoch: Option<u64>,
}

impl RateLimitHeaders {
    /// Parses numeric headers; absent or malformed values remain unknown.
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let parse = |key: &str| headers.get(key).and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok());
        Self { limit: parse("X-RateLimit-Limit"), remaining: parse("X-RateLimit-Remaining"), reset_unix_epoch: parse("X-RateLimit-Reset") }
    }
}

/// Serializes blocking requests with an evenly spaced per-client allowance.
pub struct RateLimiter {
    interval: Duration,
    last_request: Mutex<Option<Instant>>,
}

impl RateLimiter {
    /// Creates a blocking request pacer.
    ///
    /// # Errors
    /// Returns an invalid-configuration error for a zero request allowance.
    pub fn new(requests_per_minute: u32) -> Result<Self, MarketError> {
        if requests_per_minute == 0 {
            return Err(MarketError::InvalidConfig("Request rate must be positive".into()));
        }
        Ok(Self { interval: Duration::from_secs_f64(60.0 / f64::from(requests_per_minute)), last_request: Mutex::new(None) })
    }

    /// Waits until another blocking request may start.
    ///
    /// # Errors
    /// Returns an error if another thread panicked while holding the pacer.
    pub fn wait(&self) -> Result<(), MarketError> {
        let mut last = self.last_request.lock().map_err(|_| MarketError::InvalidData("Rate limiter mutex was poisoned".into()))?;
        if let Some(previous) = *last {
            std::thread::sleep(self.interval.saturating_sub(previous.elapsed()));
        }
        *last = Some(Instant::now());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_headers_without_inventing_missing_values() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-limit", "200".parse().unwrap());
        headers.insert("x-ratelimit-remaining", "invalid".parse().unwrap());
        let parsed = RateLimitHeaders::from_headers(&headers);
        assert_eq!(parsed, RateLimitHeaders { limit: Some(200), remaining: None, reset_unix_epoch: None });
    }

    #[test]
    fn zero_request_rate_is_rejected() {
        assert!(RateLimiter::new(0).is_err());
    }
}
