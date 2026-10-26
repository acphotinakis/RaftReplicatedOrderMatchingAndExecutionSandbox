//! Named options for historical dataset downloads.

use crate::market::error::MarketError;
use std::path::PathBuf;

/// Output and paging options shared by quote and trade downloads.
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    /// Directory containing CSV datasets and resume state.
    pub output_dir: PathBuf,
    /// Optional root for date-partitioned Parquet output.
    pub parquet_output_dir: Option<PathBuf>,
    /// Alpaca historical feed name.
    pub feed: String,
    /// Requested records per page, from 1 through 10,000.
    pub limit: usize,
}

impl DownloadConfig {
    pub(crate) fn validate(&self) -> Result<(), MarketError> {
        validate_component(&self.feed)?;
        if !(1..=10_000).contains(&self.limit) {
            return Err(MarketError::InvalidConfig("page limit must be 1..=10000".into()));
        }
        Ok(())
    }
}

/// Quote-specific query options.
#[derive(Debug, Clone)]
pub struct QuoteDownloadConfig {
    /// Shared output and paging options.
    pub common: DownloadConfig,
    /// Price currency.
    pub currency: String,
    /// Optional provider symbol-mapping date.
    pub asof: Option<String>,
}

/// Trade downloads use the shared output and paging options.
pub type TradeDownloadConfig = DownloadConfig;

pub(crate) fn validate_component(value: &str) -> Result<(), MarketError> {
    if value.is_empty() || value == "." || value == ".."
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(MarketError::InvalidConfig(format!("invalid dataset name: {value}")));
    }
    Ok(())
}
