//! Raft sandbox tooling and historical market-data ingestion.
#![warn(missing_docs)]
pub mod args;
pub mod market;
pub mod utils;

pub use market::alpaca::models::{
    SortOrder, StockBar, StockBarsQuery, StockBarsResponse, StockFeed, StockQuote,
    StockQuotesQuery, StockQuotesResponse,
};
pub use market::alpaca::{fetch_stock_bars, fetch_stock_quotes};
