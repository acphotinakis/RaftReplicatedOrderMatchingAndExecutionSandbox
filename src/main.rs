use chrono::Utc;
use clap::Parser;
use dotenvy::dotenv;
use raft_replicated_order_matching_and_execution_sandbox::args::{CliArgs, MarketCommand};
use raft_replicated_order_matching_and_execution_sandbox::market::alpaca::fetch_stock_bars;
use raft_replicated_order_matching_and_execution_sandbox::market::historical_trade_downloader::HistoricalTradeDownloader;
use raft_replicated_order_matching_and_execution_sandbox::market::model::{
    StockBarsQuery, StockBarsResponse,
};
use raft_replicated_order_matching_and_execution_sandbox::utils::{
    five_years_ago, parse_iso_datetime,
};
use raft_replicated_order_matching_and_execution_sandbox::{
    StockQuotesQuery, StockQuotesResponse, fetch_stock_quotes,
};
use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;

pub async fn call_bars(query: StockBarsQuery) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();

    println!("Fetching bars for: {}", query.symbols);
    let response: StockBarsResponse = fetch_stock_bars(&client, &query).await?;
    println!("{:#?}", response);

    Ok(())
}

pub async fn call_quotes(query: StockQuotesQuery) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();

    println!("Fetching quotes for: {}", query.symbols);
    let response: StockQuotesResponse = fetch_stock_quotes(&client, &query).await?;
    println!("{:#?}", response);

    Ok(())
}

// Handler function for downloading trades
pub fn call_download_trades(
    symbols: String,
    start_str: Option<String>,
    end_str: Option<String>,
    feed: String,
    output_dir: PathBuf,
    parquet_output_dir: Option<PathBuf>,
    limit: usize,
    requests_per_minute: u32,
) -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let api_key = std::env::var("ALPACA_API_KEY")
        .or_else(|_| std::env::var("APCA_API_KEY_ID"))
        .unwrap_or_default();
    let secret_key = std::env::var("ALPACA_SECRET_KEY")
        .or_else(|_| std::env::var("APCA_API_SECRET_KEY"))
        .unwrap_or_default();

    if api_key.trim().is_empty() || secret_key.trim().is_empty() {
        return Err("Missing API credentials. Set APCA_API_KEY_ID and APCA_API_SECRET_KEY in your environment or .env file.".into());
    }

    let symbol_list: Vec<String> = symbols
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();

    if symbol_list.is_empty() {
        return Err("At least one symbol is required.".into());
    }

    if parquet_output_dir.is_some() && symbol_list.len() != 1 {
        return Err("--parquet-output-dir requires exactly one symbol per invocation.".into());
    }

    let now = Utc::now();
    let start = match start_str {
        Some(s) => parse_iso_datetime(&s)?,
        None => five_years_ago(now),
    };
    let end = match end_str {
        Some(s) => parse_iso_datetime(&s)?,
        None => now,
    };

    if start >= end {
        return Err("--start must be before --end.".into());
    }

    let downloader = HistoricalTradeDownloader::new(
        &api_key,
        &secret_key,
        output_dir,
        parquet_output_dir,
        feed,
        limit,
        Duration::from_secs(60),
        8,
        requests_per_minute,
    )?;

    let mut grand_total = 0;
    for symbol in &symbol_list {
        println!("Starting trade download for {symbol}...");
        let count = downloader.download_symbol(symbol, start, end)?;
        grand_total += count;
        println!("Finished {symbol}: {count} trades written.");
    }

    println!("Total trades downloaded: {grand_total}");
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse();

    match args.command {
        MarketCommand::Bars { .. } | MarketCommand::Quotes { .. } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                // run async commands here
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
            // Runs synchronously without any enclosing async context
            call_download_trades(
                symbols,
                start,
                end,
                feed,
                output_dir,
                parquet_output_dir,
                limit,
                requests_per_minute,
            )?;
        }
        MarketCommand::List => todo!(),
    }

    Ok(())
}
