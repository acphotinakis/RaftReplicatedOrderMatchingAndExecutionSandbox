//! Shared Alpaca enumerations.
use serde::{Deserialize, Serialize};

/// Exchange tape identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tape {
    /// New York Stock Exchange.
    A,
    /// Regional exchanges.
    B,
    /// NASDAQ.
    C,
    /// Overnight.
    N,
    /// Over-the-counter.
    O,
}

/// Chronological sorting option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// Oldest first.
    #[default]
    Asc,
    /// Newest first.
    Desc,
}

/// Historical market-data feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StockFeed {
    /// Consolidated feed.
    #[default]
    Sip,
    /// IEX feed.
    Iex,
    /// Over-the-counter feed.
    Otc,
    /// Blue Ocean feed.
    Boats,
}
