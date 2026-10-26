//! Explicit transport configuration; environment loading belongs to the caller.
use crate::market::error::MarketError;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use std::time::Duration;

/// Alpaca credentials with a redacted debug representation.
#[derive(Clone)]
pub struct Credentials {
    api_key: String,
    secret_key: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials { [redacted] }")
    }
}

impl Credentials {
    /// Creates credentials from values supplied by the application.
    ///
    /// # Errors
    /// Rejects empty keys and values unsuitable for HTTP headers.
    pub fn new(api_key: &str, secret_key: &str) -> Result<Self, MarketError> {
        if api_key.trim().is_empty() || secret_key.trim().is_empty() {
            return Err(MarketError::InvalidConfig("Alpaca credentials must not be empty".into()));
        }
        let credentials = Self { api_key: api_key.to_owned(), secret_key: secret_key.to_owned() };
        credentials.headers()?;
        Ok(credentials)
    }

    pub(super) fn headers(&self) -> Result<HeaderMap, MarketError> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        for (name, value) in [("APCA-API-KEY-ID", &self.api_key), ("APCA-API-SECRET-KEY", &self.secret_key)] {
            let mut header = HeaderValue::from_str(value).map_err(|_| MarketError::InvalidConfig("Invalid Alpaca credential header".into()))?;
            header.set_sensitive(true);
            headers.insert(name, header);
        }
        Ok(headers)
    }
}

/// HTTP settings shared by blocking and asynchronous adapters.
#[derive(Debug, Clone)]
pub struct AlpacaConfig {
    /// Origin of the market-data API, configurable for local tests.
    pub base_url: String,
    /// Timeout for each request.
    pub timeout: Duration,
    /// Total attempts, including the initial request.
    pub max_attempts: u32,
    /// Maximum evenly paced requests per minute per client.
    pub requests_per_minute: u32,
}

impl Default for AlpacaConfig {
    fn default() -> Self {
        Self {
            base_url: "https://data.alpaca.markets".into(),
            timeout: Duration::from_secs(60),
            max_attempts: 8,
            requests_per_minute: 200,
        }
    }
}

impl AlpacaConfig {
    pub(super) fn validate(&self) -> Result<(), MarketError> {
        if self.max_attempts == 0 || self.requests_per_minute == 0 || self.timeout.is_zero() {
            return Err(MarketError::InvalidConfig("Attempts, request rate, and timeout must be positive".into()));
        }
        let url = reqwest::Url::parse(&self.base_url).map_err(|_| MarketError::InvalidConfig("Invalid Alpaca base URL".into()))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
            || !url.username().is_empty() || url.password().is_some()
            || url.query().is_some() || url.fragment().is_some()
            || url.path() != "/" {
            return Err(MarketError::InvalidConfig("Alpaca base URL must be an HTTP(S) origin".into()));
        }
        Ok(())
    }
}
