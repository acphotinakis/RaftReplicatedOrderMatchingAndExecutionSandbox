//! Market-data acquisition, resumable downloads, and dataset storage.
pub mod alpaca;
pub mod download;
pub mod error;
pub mod storage;

/// Compatibility exports for existing model imports.
pub mod model {
    pub use super::alpaca::models::*;
}
/// Compatibility exports for historical quote downloads.
pub mod historical_quotes_downloader {
    pub use super::download::quotes::HistoricalQuoteDownloader;
    pub use super::storage::parquet::quotes::{quote_schema, partition_quotes_csv_to_parquet};
}
/// Compatibility exports for historical trade downloads.
pub mod historical_trade_downloader {
    pub use super::download::trades::HistoricalTradeDownloader;
}
/// Compatibility exports for trade Parquet conversion.
pub mod partition_parquet {
    pub use super::storage::parquet::trades::{trade_schema, partition_csv_to_parquet};
}
/// Compatibility exports for rate-limit headers.
pub mod rate_limit {
    pub use super::alpaca::rate_limit::RateLimitHeaders;
}
/// Compatibility exports for request pacing.
pub mod rate_limiter {
    pub use super::alpaca::rate_limit::RateLimiter;
}
pub use alpaca::{fetch_stock_bars, fetch_stock_quotes};
pub use alpaca::models::{SortOrder, StockFeed, StockQuote, StockQuotesQuery, StockQuotesResponse};
