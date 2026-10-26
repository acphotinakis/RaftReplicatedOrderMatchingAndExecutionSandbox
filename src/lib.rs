#![warn(missing_docs)]

pub mod args;
pub mod market;

pub use market::alpaca::{fetch_stock_bars, fetch_stock_quotes};
pub use market::model::{
    SortOrder, StockBarsQuery, StockBarsResponse, StockFeed, StockQuote, StockQuotesQuery,
    StockQuotesResponse,
};
