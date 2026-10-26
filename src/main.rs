use clap::Parser;
use raft_replicated_order_matching_and_execution_sandbox::args::CliArgs;
use raft_replicated_order_matching_and_execution_sandbox::market::alpaca::fetch_stock_bars;
use raft_replicated_order_matching_and_execution_sandbox::market::model::{
    StockBarsQuery, StockBarsResponse,
};
use raft_replicated_order_matching_and_execution_sandbox::{
    StockQuotesQuery, StockQuotesResponse, fetch_stock_quotes,
};
use std::error::Error;

pub async fn call_bars() -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    let bars_base_url = "https://data.alpaca.markets/v2/stocks/bars";

    let query: StockBarsQuery = StockBarsQuery {
        symbols: "AAPL,MSFT".to_string(),
        timeframe: "1Min".to_string(),
        start: Some("2024-01-03T09:30:00Z".to_string()),
        end: Some("2024-01-03T10:00:00Z".to_string()),
        limit: 10,
        feed: "sip".to_string(),
        sort: "asc".to_string(),
        ..Default::default()
    };

    println!("Fetching quotes...");
    let response: StockBarsResponse = fetch_stock_bars(&client, bars_base_url, &query).await?;

    Ok(())
}

pub async fn call_quotes() -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    let quotes_base_url = "https://data.alpaca.markets/v2/stocks/quotes";

    let query: StockQuotesQuery = StockQuotesQuery {
        symbols: "AAPL,MSFT".to_string(),
        start: Some("2024-01-03T09:30:00Z".to_string()),
        end: Some("2024-01-03T10:00:00Z".to_string()),
        limit: 10,
        feed: "sip".to_string(),
        sort: "asc".to_string(),
        ..Default::default()
    };

    println!("Fetching quotes...");
    let response: StockQuotesResponse =
        fetch_stock_quotes(&client, quotes_base_url, &query).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _args = CliArgs::parse();

    call_bars().await?;

    call_quotes().await?;

    Ok(())
}
