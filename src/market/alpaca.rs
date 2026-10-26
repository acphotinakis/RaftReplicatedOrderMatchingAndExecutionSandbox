use crate::market::model::{
    StockBarsQuery, StockBarsResponse, StockQuotesQuery, StockQuotesResponse,
};
use clap::builder::styling::Reset;
use dotenvy::dotenv;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use std::env;
use std::error::Error;

pub async fn fetch_stock_bars(
    client: &reqwest::Client,
    base_url: &str,
    query: &StockBarsQuery,
) -> Result<StockBarsResponse, Box<dyn Error>> {
    // Load environment variables from the .env file
    // .ok() ignores the error if the file doesn't exist (useful for production)
    dotenv().ok();

    let alpaca_api_key = env::var("ALPACA_API_KEY")
        .expect("ALPACA_API_KEY must be set in the environment or .env file");

    let alpaca_secret_key = env::var("ALPACA_SECRET_KEY")
        .expect("ALPACA_SECRET_KEY must be set in the environment or .env file");

    println!("ALPACA_SECRET_KEY and ALPACA_API_KEY keys loaded successfully!");

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("APCA-API-KEY-ID", HeaderValue::from_str(&alpaca_api_key)?);
    headers.insert(
        "APCA-API-SECRET-KEY",
        HeaderValue::from_str(&alpaca_secret_key)?,
    );

    let response = client
        .get(base_url)
        .headers(headers)
        .query(query)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        return Err(format!("Request failed with status {status}: {error_body}").into());
    }

    // 1. Consume the response stream into a raw String
    let raw_body = response.text().await?;

    // 2. Parse into a generic JSON Value and pretty-print it
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&raw_body) {
        if let Ok(pretty_json) = serde_json::to_string_pretty(&json_value) {
            println!("--- Pretty JSON Response --- \n{}", pretty_json);
        }
    } else {
        // Fallback if the body isn't actually valid JSON (e.g., plain text errors)
        println!("--- Raw Non-JSON Response --- \n{}", raw_body);
    }

    // 3. Deserialize into your typed struct from the string
    let data = serde_json::from_str::<StockBarsResponse>(&raw_body)?;

    println!("Next Page Token: {:?}", &data.next_page_token);
    for (symbol, bars) in &data.bars {
        println!("Symbol: {symbol} (Returned {} bars)", bars.len());
        for quote in bars.iter().take(2) {
            println!(
                "  [{}] Open: ${:.2} High: ${:.2} Low: ${:.2} Close: ${:.2} Volume: {:.2} VWAP: ${:.2}",
                quote.t, quote.o, quote.h, quote.l, quote.c, quote.v, quote.vw
            );
        }
    }

    Ok(data)
}

pub async fn fetch_stock_quotes(
    client: &reqwest::Client,
    base_url: &str,
    query: &StockQuotesQuery,
) -> Result<StockQuotesResponse, Box<dyn Error>> {
    // Load environment variables from the .env file
    // .ok() ignores the error if the file doesn't exist (useful for production)
    dotenv().ok();

    let alpaca_api_key = env::var("ALPACA_API_KEY")
        .expect("ALPACA_API_KEY must be set in the environment or .env file");

    let alpaca_secret_key = env::var("ALPACA_SECRET_KEY")
        .expect("ALPACA_SECRET_KEY must be set in the environment or .env file");

    println!("ALPACA_SECRET_KEY and ALPACA_API_KEY keys loaded successfully!");

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("APCA-API-KEY-ID", HeaderValue::from_str(&alpaca_api_key)?);
    headers.insert(
        "APCA-API-SECRET-KEY",
        HeaderValue::from_str(&alpaca_secret_key)?,
    );

    let response = client
        .get(base_url)
        .headers(headers)
        .query(query)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        return Err(format!("Request failed with status {status}: {error_body}").into());
    }

    // 1. Consume the response stream into a raw String
    let raw_body = response.text().await?;

    // 2. Parse into a generic JSON Value and pretty-print it
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&raw_body) {
        if let Ok(pretty_json) = serde_json::to_string_pretty(&json_value) {
            println!("--- Pretty JSON Response --- \n{}", pretty_json);
        }
    } else {
        // Fallback if the body isn't actually valid JSON (e.g., plain text errors)
        println!("--- Raw Non-JSON Response --- \n{}", raw_body);
    }

    // 3. Deserialize into your typed struct from the string
    let data = serde_json::from_str::<StockQuotesResponse>(&raw_body)?;

    println!("Next Page Token: {:?}", &data.next_page_token);
    for (symbol, bars) in &data.quotes {
        println!("Symbol: {symbol} (Returned {} bars)", bars.len());
        for quote in bars.iter().take(2) {
            println!(
                "  [{}] Bid: ${:.2} (size: {}) @ {} | Ask: ${:.2} (size: {}) @ {}",
                quote.t, quote.bp, quote.bs, quote.bx, quote.ap, quote.as_, quote.ax
            );
        }
    }

    Ok(data)
}
