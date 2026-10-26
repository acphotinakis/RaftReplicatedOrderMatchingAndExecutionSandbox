//! Historical bars endpoint payloads.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Raw historical OHLCV bar; prices retain the provider representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockBar {
    /// Open price.
    pub o: f64,
    /// High price.
    pub h: f64,
    /// Low price.
    pub l: f64,
    /// Close price.
    pub c: f64,
    /// Volume.
    pub v: u64,
    /// Trade count.
    pub n: u32,
    /// Volume-weighted average price.
    pub vw: f64,
    /// RFC3339 timestamp.
    pub t: String,
}

/// Query parameters for `/v2/stocks/bars`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct StockBarsQuery {
    /// Comma-separated symbols.
    pub symbols: String,
    /// Aggregation interval.
    pub timeframe: String,
    /// Inclusive start timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// End timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    /// Maximum records in a page.
    pub limit: u32,
    /// Corporate-action adjustment selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<String>,
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

/// One page of bars grouped by symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockBarsResponse {
    /// Records by symbol.
    pub bars: HashMap<String, Vec<StockBar>>,
    /// Continuation token, absent at the end.
    pub next_page_token: Option<String>,
}
