use serde::{Deserialize, Serialize};
use std::collections::HashMap;

///
/// COMMON
///

///
/// HISTORICAL BARS CONFIG MODELS
///

/// Individual historical OHLCV bar (candlestick) data point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockBar {
    /// Open price
    pub o: f64,
    /// High price
    pub h: f64,
    /// Low price
    pub l: f64,
    /// Close price
    pub c: f64,
    /// Volume
    pub v: u64,
    /// Number of trades
    pub n: u32,
    /// Volume weighted average price
    pub vw: f64,
    /// Timestamp in RFC3339 format
    pub t: String,
}

/// Query parameters for GET `/v2/stocks/bars`
#[derive(Debug, Clone, Serialize, Default)]
pub struct StockBarsQuery {
    pub symbols: String,
    pub timeframe: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asof: Option<String>,
    pub feed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    pub sort: String,
}

/// Root response payload returned by `/v2/stocks/bars`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockBarsResponse {
    /// Map of ticker symbols to their list of historical bars.
    pub bars: HashMap<String, Vec<StockBar>>,

    /// Pagination token to retrieve the next page of results.
    pub next_page_token: Option<String>,
}

///
/// HISTORICAL QUOTES CONFIG MODELS
///

/// Exchange Tape identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tape {
    /// New York Stock Exchange
    A,
    /// NYSE Arca, Bats, IEX, and regional exchanges
    B,
    /// NASDAQ
    C,
    /// Overnight
    N,
    /// Over-the-counter
    O,
}

/// Best Bid and Offer (BBO) quote information for a security.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockQuote {
    /// Timestamp in RFC-3339 format with nanosecond precision.
    #[serde(rename = "t")]
    pub timestamp: String,

    /// Bid exchange code.
    #[serde(rename = "bx")]
    pub bid_exchange: String,

    /// Bid price. 0 means no active bid.
    #[serde(rename = "bp")]
    pub bid_price: f64,

    /// Bid size in shares.
    #[serde(rename = "bs")]
    pub bid_size: u32,

    /// Ask exchange code.
    #[serde(rename = "ax")]
    pub ask_exchange: String,

    /// Ask price. 0 means no active ask.
    #[serde(rename = "ap")]
    pub ask_price: f64,

    /// Ask size in shares.
    #[serde(rename = "as")] // Fixes the 'as_' keyword conflict natively!
    pub ask_size: u32,

    /// Quote condition flags.
    #[serde(rename = "c")]
    pub conditions: Vec<String>,

    /// Exchange tape.
    #[serde(rename = "z")]
    pub tape: Tape,
}

/// Root response payload returned by `/v2/stocks/quotes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockQuotesResponse {
    /// Currency of quote prices (defaults to USD).
    pub currency: Option<String>,

    /// Pagination token to retrieve the next page of results.
    pub next_page_token: Option<String>,

    /// Map of ticker symbols to their list of quotes.
    pub quotes: HashMap<String, Vec<StockQuote>>,
}

/// Query parameters for GET `/v2/stocks/quotes`
#[derive(Debug, Clone, Serialize, Default)]
pub struct StockQuotesQuery {
    pub symbols: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asof: Option<String>,
    pub feed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    pub sort: String,
}

///
/// COMMON
///

/// Chronological sorting option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

/// Source feed for historical market data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StockFeed {
    #[default]
    Sip,
    Iex,
    Otc,
    Boats,
}
