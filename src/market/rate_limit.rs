use reqwest::header::HeaderMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitHeaders {
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub reset_unix_epoch: Option<u64>,
}

impl RateLimitHeaders {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let parse_header = |key: &str| -> Option<u64> {
            headers
                .get(key)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        };

        Self {
            limit: parse_header("X-RateLimit-Limit"),
            remaining: parse_header("X-RateLimit-Remaining"),
            reset_unix_epoch: parse_header("X-RateLimit-Reset"),
        }
    }
}