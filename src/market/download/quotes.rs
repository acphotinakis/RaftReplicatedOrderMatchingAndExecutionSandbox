//! Resumable historical quote downloads.

use super::{checkpoint::{Checkpoint, DatasetIdentity}, config::QuoteDownloadConfig, windows::{DownloadWindow, daily_windows}};
use crate::market::{alpaca::{AlpacaClient, models::SingleSymbolQuotesResponse}, error::MarketError, storage::{csv::{QUOTE_FIELDS, quote_to_row}, partition_quotes_csv_to_parquet}};
use chrono::{DateTime, Utc};
use std::{collections::BTreeSet, error::Error, fs::File};

/// Coordinates quote pages, storage publication, and exact-query resume state.
pub struct HistoricalQuoteDownloader {
    client: AlpacaClient,
    config: QuoteDownloadConfig,
}

impl HistoricalQuoteDownloader {
    /// Construct a downloader using an already authenticated HTTP client.
    pub fn new(client: AlpacaClient, config: QuoteDownloadConfig) -> Result<Self, Box<dyn Error>> {
        config.common.validate()?;
        if config.currency.trim().is_empty() {
            return Err(MarketError::InvalidConfig("currency must not be empty".into()).into());
        }
        Ok(Self { client, config })
    }

    /// Download an exact interval, resuming only matching query parameters.
    pub fn download_symbol(&self, symbol: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<usize, Box<dyn Error>> {
        let windows = daily_windows(start, end)?;
        let identity = DatasetIdentity {
            kind: "quotes".into(), symbol: symbol.into(), feed: self.config.common.feed.clone(),
            start: start.to_rfc3339(), end: end.to_rfc3339(),
            currency: Some(self.config.currency.clone()), asof: self.config.asof.clone(),
            parquet_output_dir: self.config.common.parquet_output_dir.clone(),
        };
        let mut checkpoint = Checkpoint::open(&self.config.common, identity)?;
        let mut total = 0usize;
        for window in windows {
            if checkpoint.is_complete(&window) { continue; }
            let count = self.download_window(symbol, &window, &checkpoint)?;
            if let Some(root) = &self.config.common.parquet_output_dir {
                partition_quotes_csv_to_parquet(&checkpoint.temporary_part(&window), root, symbol, window.day)?;
                checkpoint.complete_parquet(&window)?;
            } else {
                checkpoint.complete_csv(&window)?;
            }
            total = total.checked_add(count).ok_or_else(|| MarketError::InvalidData("record count overflow".into()))?;
            log::info!("{symbol} | {} | wrote {count} quotes", window.day);
        }
        Ok(total)
    }

    fn download_window(&self, symbol: &str, window: &DownloadWindow, checkpoint: &Checkpoint) -> Result<usize, Box<dyn Error>> {
        let file = File::create(checkpoint.temporary_part(window))?;
        let mut writer = csv::Writer::from_writer(file);
        writer.write_record(QUOTE_FIELDS)?;
        let mut token: Option<String> = None;
        let mut seen_tokens = BTreeSet::new();
        let mut count = 0usize;
        loop {
            let mut params = vec![
                ("start", window.start.clone()), ("end", window.end.clone()),
                ("limit", self.config.common.limit.to_string()), ("feed", self.config.common.feed.clone()),
                ("currency", self.config.currency.clone()), ("sort", "asc".into()),
            ];
            if let Some(asof) = &self.config.asof { params.push(("asof", asof.clone())); }
            if let Some(token) = &token { params.push(("page_token", token.clone())); }
            let page: SingleSymbolQuotesResponse = self.client.get_json(&format!("/v2/stocks/{symbol}/quotes"), &params)?;
            for quote in &page.quotes { writer.write_record(quote_to_row(symbol, quote)?)?; }
            count = count.checked_add(page.quotes.len()).ok_or_else(|| MarketError::InvalidData("record count overflow".into()))?;
            token = page.next_page_token;
            match &token {
                None => break,
                Some(value) if !seen_tokens.insert(value.clone()) => return Err(MarketError::InvalidData("repeated quote page token".into()).into()),
                Some(_) => {}
            }
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;
        Ok(count)
    }
}
