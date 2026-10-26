//! Authenticated HTTP clients with shared retry policy.
use super::config::{AlpacaConfig, Credentials};
use super::models::{StockBarsQuery, StockBarsResponse, StockQuotesQuery, StockQuotesResponse};
use super::rate_limit::{RateLimitHeaders, RateLimiter};
use super::retry::{retry_delay, retryable_error, retryable_status};
use crate::market::error::MarketError;
use serde::{Serialize, de::DeserializeOwned};
use std::time::{Duration, Instant};

fn endpoint_url(base_url: &str, endpoint: &str) -> Result<String, MarketError> {
    if !endpoint.starts_with("/v2/") || endpoint.contains(['?', '#']) {
        return Err(MarketError::InvalidConfig(
            "Endpoint must be a /v2/ path without query or fragment".into(),
        ));
    }
    Ok(format!("{}{endpoint}", base_url.trim_end_matches('/')))
}

/// Blocking Alpaca transport for offline historical downloads.
pub struct AlpacaClient {
    client: reqwest::blocking::Client,
    config: AlpacaConfig,
    rate_limiter: RateLimiter,
}

impl AlpacaClient {
    /// Builds a client without reading environment variables.
    ///
    /// # Errors
    /// Returns configuration or HTTP client initialization errors.
    pub fn new(credentials: Credentials, config: AlpacaConfig) -> Result<Self, MarketError> {
        config.validate()?;
        let client = reqwest::blocking::Client::builder()
            .default_headers(credentials.headers()?)
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let rate_limiter = RateLimiter::new(config.requests_per_minute)?;
        Ok(Self {
            client,
            config,
            rate_limiter,
        })
    }

    /// Fetches one typed page, retrying transient failures only.
    ///
    /// # Errors
    /// Returns request, HTTP status, or JSON decoding errors. Invalid payloads
    /// and permanent client errors are never retried.
    pub fn get_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &impl Serialize,
    ) -> Result<T, MarketError> {
        let url = endpoint_url(&self.config.base_url, endpoint)?;
        for attempt in 1..=self.config.max_attempts {
            self.rate_limiter.wait()?;
            match self.client.get(&url).query(query).send() {
                Ok(response) => {
                    let status = response.status();
                    let delay = retry_delay(response.headers(), attempt);
                    log::debug!(
                        "Alpaca rate limit: {:?}",
                        RateLimitHeaders::from_headers(response.headers())
                    );
                    if status.is_success() {
                        return Ok(serde_json::from_slice(&response.bytes()?)?);
                    }
                    if retryable_status(status) && attempt < self.config.max_attempts {
                        drop(response);
                        std::thread::sleep(delay);
                        continue;
                    }
                    return Err(MarketError::HttpStatus {
                        status: status.as_u16(),
                        body: response.text()?,
                    });
                }
                Err(error) => {
                    if retryable_error(&error) && attempt < self.config.max_attempts {
                        std::thread::sleep(retry_delay(
                            &reqwest::header::HeaderMap::new(),
                            attempt,
                        ));
                        continue;
                    }
                    return Err(error.into());
                }
            }
        }
        Err(MarketError::RetriesExhausted)
    }
}

/// Asynchronous Alpaca transport for callers already using Tokio.
pub struct AsyncAlpacaClient {
    client: reqwest::Client,
    config: AlpacaConfig,
    interval: Duration,
    last_request: tokio::sync::Mutex<Option<Instant>>,
}

impl AsyncAlpacaClient {
    /// Builds an asynchronous client with injected credentials and policy.
    ///
    /// # Errors
    /// Returns configuration or HTTP client initialization errors.
    pub fn new(credentials: Credentials, config: AlpacaConfig) -> Result<Self, MarketError> {
        config.validate()?;
        let client = reqwest::Client::builder()
            .default_headers(credentials.headers()?)
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let interval = Duration::from_secs_f64(60.0 / f64::from(config.requests_per_minute));
        Ok(Self {
            client,
            config,
            interval,
            last_request: tokio::sync::Mutex::new(None),
        })
    }

    async fn wait(&self) {
        let mut last = self.last_request.lock().await;
        if let Some(previous) = *last {
            tokio::time::sleep(self.interval.saturating_sub(previous.elapsed())).await;
        }
        *last = Some(Instant::now());
    }

    /// Fetches one page using asynchronous pacing and retry waits.
    ///
    /// # Errors
    /// Returns request, HTTP status, or JSON decoding errors.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &impl Serialize,
    ) -> Result<T, MarketError> {
        let url = endpoint_url(&self.config.base_url, endpoint)?;
        for attempt in 1..=self.config.max_attempts {
            self.wait().await;
            match self.client.get(&url).query(query).send().await {
                Ok(response) => {
                    let status = response.status();
                    let delay = retry_delay(response.headers(), attempt);
                    log::debug!(
                        "Alpaca rate limit: {:?}",
                        RateLimitHeaders::from_headers(response.headers())
                    );
                    if status.is_success() {
                        return Ok(serde_json::from_slice(&response.bytes().await?)?);
                    }
                    if retryable_status(status) && attempt < self.config.max_attempts {
                        drop(response);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(MarketError::HttpStatus {
                        status: status.as_u16(),
                        body: response.text().await?,
                    });
                }
                Err(error) => {
                    if retryable_error(&error) && attempt < self.config.max_attempts {
                        tokio::time::sleep(retry_delay(
                            &reqwest::header::HeaderMap::new(),
                            attempt,
                        ))
                        .await;
                        continue;
                    }
                    return Err(error.into());
                }
            }
        }
        Err(MarketError::RetriesExhausted)
    }
}

/// Fetches one historical bars page with an already configured client.
///
/// # Errors
/// Returns transport, status, or payload decoding errors.
pub async fn fetch_stock_bars(
    client: &AsyncAlpacaClient,
    query: &StockBarsQuery,
) -> Result<StockBarsResponse, MarketError> {
    client.get_json("/v2/stocks/bars", query).await
}

/// Fetches one historical quotes page with an already configured client.
///
/// # Errors
/// Returns transport, status, or payload decoding errors.
pub async fn fetch_stock_quotes(
    client: &AsyncAlpacaClient,
    query: &StockQuotesQuery,
) -> Result<StockQuotesResponse, MarketError> {
    client.get_json("/v2/stocks/quotes", query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn server(responses: Vec<&'static str>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            responses
                .into_iter()
                .map(|response| {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while !request.ends_with(b"\r\n\r\n") {
                        let length = stream.read(&mut buffer).unwrap();
                        assert!(length > 0);
                        request.extend_from_slice(&buffer[..length]);
                    }
                    stream.write_all(response.as_bytes()).unwrap();
                    String::from_utf8(request).unwrap()
                })
                .collect()
        });
        (url, task)
    }

    #[test]
    fn retries_rate_limit_and_sends_injected_credentials() {
        let (base_url, server) = server(vec![
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        ]);
        let client = AlpacaClient::new(
            Credentials::new("test-key", "test-secret").unwrap(),
            AlpacaConfig {
                base_url,
                requests_per_minute: u32::MAX,
                ..AlpacaConfig::default()
            },
        )
        .unwrap();
        let value: serde_json::Value = client
            .get_json("/v2/stocks/trades", &[("symbols", "SPY")])
            .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(value, serde_json::json!({}));
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("symbols=SPY"));
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("apca-api-key-id: test-key")
        );
    }

    #[test]
    fn unauthorized_response_is_returned_immediately() {
        let (base_url, server) = server(vec![
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 6\r\nConnection: close\r\n\r\ndenied",
        ]);
        let client = AlpacaClient::new(
            Credentials::new("test", "test").unwrap(),
            AlpacaConfig {
                base_url,
                ..AlpacaConfig::default()
            },
        )
        .unwrap();
        let result = client.get_json::<serde_json::Value>("/v2/stocks/bars", &[("symbols", "SPY")]);
        assert!(matches!(
            result,
            Err(MarketError::HttpStatus { status: 401, .. })
        ));
        assert_eq!(server.join().unwrap().len(), 1);
    }
}
