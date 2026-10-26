//! Historical trades endpoint payloads.
use super::common::Tape;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Raw trade record with required price, quantity, and timestamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockTrade {
    /// RFC3339 timestamp.
    #[serde(rename = "t")]
    pub timestamp: String,
    /// Trade price.
    #[serde(rename = "p")]
    pub price: f64,
    /// Trade size in shares.
    #[serde(rename = "s")]
    pub size: u64,
    /// Exchange identifier.
    #[serde(rename = "x")]
    pub exchange: String,
    /// Trade identifier.
    #[serde(rename = "i")]
    pub trade_id: u64,
    /// Condition flags.
    #[serde(rename = "c")]
    pub conditions: Vec<String>,
    /// Provider update code, when supplied.
    #[serde(rename = "u")]
    pub update: Option<String>,
    /// Exchange tape.
    #[serde(rename = "z")]
    pub tape: Tape,
}

/// One page of trades grouped by symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlpacaTradesResponse {
    /// Returned records; required so malformed responses cannot complete a window.
    pub trades: HashMap<String, Vec<StockTrade>>,
    /// Continuation token.
    pub next_page_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_price_is_rejected() {
        let payload = r#"{"t":"2024-01-01T00:00:00Z","s":10,"x":"V","i":7,"c":[],"z":"C"}"#;
        let result = serde_json::from_str::<StockTrade>(payload);
        assert!(result.is_err());
    }

    #[test]
    fn trade_page_preserves_token_and_records() {
        let payload = r#"{"trades":{"SPY":[{"t":"2024-01-01T00:00:00Z","p":100.25,"s":10,"x":"V","i":7,"c":[],"z":"C"}]},"next_page_token":"page2"}"#;
        let page: AlpacaTradesResponse = serde_json::from_str(payload).unwrap();
        assert_eq!(page.next_page_token.as_deref(), Some("page2"));
        assert_eq!(page.trades["SPY"][0].trade_id, 7);
    }
}
