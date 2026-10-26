//! CLI configuration and presentation for market-data commands.

use chrono::{DateTime, Utc};
use raft_replicated_order_matching_and_execution_sandbox::{
    args::MarketCommand,
    market::{
        alpaca::models::StockBarsQuery,
        alpaca::{
            AlpacaClient, AsyncAlpacaClient,
            config::{AlpacaConfig, Credentials},
        },
        download::{
            config::{DownloadConfig, QuoteDownloadConfig},
            quotes::HistoricalQuoteDownloader,
            trades::HistoricalTradeDownloader,
        },
    },
    plots::plot_bars::PlotBars,
    utils::{five_years_ago, parse_iso_datetime},
};
use std::{error::Error, time::Duration};

const DEFAULT_SYMBOLS: &str = "AAPL,MSFT,NVDA,AMZN,GOOGL,META,TSLA,AMD,SPY,QQQ";

fn credentials() -> Result<Credentials, Box<dyn Error>> {
    let key = std::env::var("ALPACA_API_KEY").or_else(|_| std::env::var("APCA_API_KEY_ID"))?;
    let secret =
        std::env::var("ALPACA_SECRET_KEY").or_else(|_| std::env::var("APCA_API_SECRET_KEY"))?;
    Ok(Credentials::new(&key, &secret)?)
}

fn symbols(raw: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let symbols: Vec<_> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_uppercase)
        .collect();
    if symbols.is_empty() {
        return Err("at least one symbol is required".into());
    }
    Ok(symbols)
}

fn range(
    start: Option<String>,
    end: Option<String>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), Box<dyn Error>> {
    let now = Utc::now();
    let start = start
        .as_deref()
        .map(parse_iso_datetime)
        .transpose()?
        .unwrap_or_else(|| five_years_ago(now));
    let end = end
        .as_deref()
        .map(parse_iso_datetime)
        .transpose()?
        .unwrap_or(now);
    if start >= end {
        return Err("--start must be before --end".into());
    }
    Ok((start, end))
}

fn client(requests_per_minute: u32) -> Result<AlpacaClient, Box<dyn Error>> {
    Ok(AlpacaClient::new(
        credentials()?,
        AlpacaConfig {
            timeout: Duration::from_secs(60),
            max_attempts: 8,
            requests_per_minute,
            ..AlpacaConfig::default()
        },
    )?)
}

pub fn run(command: MarketCommand) -> Result<(), Box<dyn Error>> {
    match command {
        MarketCommand::Bars {
            symbols,
            timeframe,
            start,
            end,
            limit,
            feed,
            sort,
            parquet_output_dir,
        } => {
            let symbol_list = self::symbols(&symbols)?;
            let query = StockBarsQuery {
                symbols: symbol_list.join(","),
                timeframe,
                start,
                end,
                limit,
                feed,
                sort,
                ..StockBarsQuery::default()
            };
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let client = AsyncAlpacaClient::new(credentials()?, AlpacaConfig::default())?;
                let response =
                    raft_replicated_order_matching_and_execution_sandbox::fetch_stock_bars(
                        &client, &query,
                    )
                    .await?;

                if let Some(parquet_dir) = parquet_output_dir {
                    for (symbol, bars) in &response.bars {
                        if bars.is_empty() {
                            continue;
                        }
                        let batch = raft_replicated_order_matching_and_execution_sandbox::market::storage::bars_to_record_batch(bars)?;
                        let out_path = parquet_dir.join(format!("{symbol}_bars.parquet"));
                        raft_replicated_order_matching_and_execution_sandbox::market::storage::save_parquet(&batch, &out_path)?;
                        println!("Saved {} bars for {symbol} to {}", bars.len(), out_path.display());
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                Ok::<(), Box<dyn Error>>(())
            })?;
        }
        MarketCommand::Trades {
            symbols,
            start,
            end,
            feed,
            output_dir,
            parquet_output_dir,
            limit,
            requests_per_minute,
        } => {
            let symbols = self::symbols(&symbols)?;
            let (start, end) = range(start, end)?;
            let downloader = HistoricalTradeDownloader::new(
                client(requests_per_minute)?,
                DownloadConfig {
                    output_dir,
                    parquet_output_dir,
                    feed,
                    limit,
                },
            )?;
            for symbol in symbols {
                let count = downloader.download_symbol(&symbol, start, end)?;
                println!("{symbol}: {count} trades downloaded");
            }
        }
        MarketCommand::Quotes {
            symbols,
            start,
            end,
            feed,
            currency,
            asof,
            output_dir,
            parquet_output_dir,
            limit,
            requests_per_minute,
        } => {
            let symbols = self::symbols(symbols.as_deref().unwrap_or(DEFAULT_SYMBOLS))?;
            let (start, end) = range(start, end)?;
            let downloader = HistoricalQuoteDownloader::new(
                client(requests_per_minute)?,
                QuoteDownloadConfig {
                    common: DownloadConfig {
                        output_dir,
                        parquet_output_dir,
                        feed,
                        limit,
                    },
                    currency,
                    asof,
                },
            )?;
            for symbol in symbols {
                let count = downloader.download_symbol(&symbol, start, end)?;
                println!("{symbol}: {count} quotes downloaded");
            }
        }
        MarketCommand::Plot {
            symbol,
            input_file,
            data_type,
        } => {
            let plot_bars = PlotBars::new(symbol, input_file, data_type)?;
            plot_bars.plot_bars()?;
        }
        MarketCommand::List => println!("Market data commands: bars, quotes, trades"),
    }
    Ok(())
}
