//! Resumable historical trade downloads.

use super::{
    checkpoint::{Checkpoint, DatasetIdentity},
    config::TradeDownloadConfig,
    windows::{DownloadWindow, daily_windows},
};
use crate::market::{
    alpaca::{AlpacaClient, models::AlpacaTradesResponse},
    error::MarketError,
    storage::{
        csv::{TRADE_FIELDS, trade_to_row},
        partition_csv_to_parquet,
    },
};
use chrono::{DateTime, Utc};
use std::{collections::BTreeSet, error::Error, fs::File};

/// Coordinates trade pages, storage publication, and exact-query resume state.
pub struct HistoricalTradeDownloader {
    client: AlpacaClient,
    config: TradeDownloadConfig,
}

impl HistoricalTradeDownloader {
    /// Construct a downloader using an already authenticated HTTP client.
    pub fn new(client: AlpacaClient, config: TradeDownloadConfig) -> Result<Self, Box<dyn Error>> {
        config.validate()?;
        Ok(Self { client, config })
    }

    /// Download an exact interval, resuming only matching query parameters.
    pub fn download_symbol(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<usize, Box<dyn Error>> {
        let windows = daily_windows(start, end)?;
        let identity = DatasetIdentity {
            kind: "trades".into(),
            symbol: symbol.into(),
            feed: self.config.feed.clone(),
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            currency: None,
            asof: None,
            parquet_output_dir: self.config.parquet_output_dir.clone(),
        };
        let mut checkpoint = Checkpoint::open(&self.config, identity)?;
        let mut total = 0usize;
        for window in windows {
            if checkpoint.is_complete(&window) {
                continue;
            }
            let count = self.download_window(symbol, &window, &checkpoint)?;
            if let Some(root) = &self.config.parquet_output_dir {
                partition_csv_to_parquet(
                    &checkpoint.temporary_part(&window),
                    root,
                    symbol,
                    window.day,
                )?;
                checkpoint.complete_parquet(&window)?;
            } else {
                checkpoint.complete_csv(&window)?;
            }
            total = total
                .checked_add(count)
                .ok_or_else(|| MarketError::InvalidData("record count overflow".into()))?;
            log::info!("{symbol} | {} | wrote {count} trades", window.day);
        }
        Ok(total)
    }

    fn download_window(
        &self,
        symbol: &str,
        window: &DownloadWindow,
        checkpoint: &Checkpoint,
    ) -> Result<usize, Box<dyn Error>> {
        let file = File::create(checkpoint.temporary_part(window))?;
        let mut writer = csv::Writer::from_writer(file);
        writer.write_record(TRADE_FIELDS)?;
        let mut token: Option<String> = None;
        let mut seen_tokens = BTreeSet::new();
        let mut count = 0usize;
        loop {
            let mut params = vec![
                ("symbols", symbol.into()),
                ("start", window.start.clone()),
                ("end", window.end.clone()),
                ("limit", self.config.limit.to_string()),
                ("feed", self.config.feed.clone()),
                ("sort", "asc".into()),
            ];
            if let Some(token) = &token {
                params.push(("page_token", token.clone()));
            }
            let page: AlpacaTradesResponse = self.client.get_json("/v2/stocks/trades", &params)?;
            if let Some(trades) = page.trades.get(symbol) {
                for trade in trades {
                    writer.write_record(trade_to_row(symbol, trade)?)?;
                }
                count = count
                    .checked_add(trades.len())
                    .ok_or_else(|| MarketError::InvalidData("record count overflow".into()))?;
            }
            token = page.next_page_token;
            match &token {
                None => break,
                Some(value) if !seen_tokens.insert(value.clone()) => {
                    return Err(MarketError::InvalidData("repeated trade page token".into()).into());
                }
                Some(_) => {}
            }
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;
        Ok(count)
    }
}
