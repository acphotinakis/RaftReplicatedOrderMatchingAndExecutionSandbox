//! Historical quotes endpoint payloads.
use super::common::Tape;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Raw best bid and offer quote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockQuote {
    /// RFC3339 timestamp.
    #[serde(rename = "t")]
    pub timestamp: String,
    /// Bid exchange.
    #[serde(rename = "bx")]
    pub bid_exchange: String,
    /// Bid price; zero denotes no active bid.
    #[serde(rename = "bp")]
    pub bid_price: f64,
    /// Bid size in shares.
    #[serde(rename = "bs")]
    pub bid_size: u32,
    /// Ask exchange.
    #[serde(rename = "ax")]
    pub ask_exchange: String,
    /// Ask price; zero denotes no active ask.
    #[serde(rename = "ap")]
    pub ask_price: f64,
    /// Ask size in shares.
    #[serde(rename = "as")]
    pub ask_size: u32,
    /// Condition flags.
    #[serde(rename = "c")]
    pub conditions: Vec<String>,
    /// Exchange tape.
    #[serde(rename = "z")]
    pub tape: Tape,
}

/// One page from the multi-symbol quotes endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockQuotesResponse {
    /// Output currency.
    pub currency: Option<String>,
    /// Continuation token.
    pub next_page_token: Option<String>,
    /// Quotes by symbol.
    pub quotes: HashMap<String, Vec<StockQuote>>,
}

/// One page from the single-symbol quotes endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SingleSymbolQuotesResponse {
    /// Returned quotes; required so malformed responses cannot complete a window.
    pub quotes: Vec<StockQuote>,
    /// Continuation token.
    pub next_page_token: Option<String>,
}

/// Query parameters for `/v2/stocks/quotes`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct StockQuotesQuery {
    /// Comma-separated symbols.
    pub symbols: String,
    /// Start timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// End timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    /// Maximum records per page.
    pub limit: u32,
    /// Symbol mapping date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asof: Option<String>,
    /// Source feed.
    pub feed: String,
    /// Output currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Continuation token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    /// Chronological direction.
    pub sort: String,
}
