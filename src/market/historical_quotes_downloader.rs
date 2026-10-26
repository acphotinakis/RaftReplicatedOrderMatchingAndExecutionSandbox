use crate::market::partition_parquet::partition_csv_to_parquet;
use crate::market::rate_limiter::RateLimiter;
use crate::utils::{end_of_day_rfc3339, iter_daily_windows, rfc3339};
use arrow::array::{
    ArrayRef, Float64Builder, StringBuilder, TimestampNanosecondBuilder, UInt32Builder,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveDateTime, TimeZone, Utc};
use clap::Parser;
use log::{debug, info, warn};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const BASE_URL_TEMPLATE: &str = "https://data.alpaca.markets/v2/stocks/{symbol}/quotes";
const MAX_LIMIT: usize = 10_000;
const DEFAULT_SYMBOLS: &[&str] = &[
    "AAPL", "MSFT", "NVDA", "AMZN", "GOOGL", "META", "TSLA", "AMD", "SPY", "QQQ",
];

const CSV_FIELDS: &[&str] = &[
    "timestamp",
    "symbol",
    "ask_price",
    "ask_size",
    "ask_exchange",
    "bid_price",
    "bid_size",
    "bid_exchange",
    "conditions",
    "tape",
];



// --- Data Models & Schemas ---

#[derive(Debug, Serialize, Deserialize)]
struct CompletedDaysState {
    symbol: String,
    feed: String,
    completed_days: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SingleSymbolQuotesResponse {
    #[serde(default)]
    quotes: Vec<serde_json::Value>,
    next_page_token: Option<String>,
}

pub fn quote_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
            false,
        ),
        Field::new("symbol", DataType::Utf8, false),
        Field::new("ask_price", DataType::Float64, false),
        Field::new("ask_size", DataType::UInt32, false),
        Field::new("ask_exchange", DataType::Utf8, true),
        Field::new("bid_price", DataType::Float64, false),
        Field::new("bid_size", DataType::UInt32, false),
        Field::new("bid_exchange", DataType::Utf8, true),
        Field::new("conditions", DataType::Utf8, true),
        Field::new("tape", DataType::Utf8, true),
    ]))
}

pub fn partition_quotes_csv_to_parquet(
    part_csv_path: &Path,
    parquet_output_dir: &Path,
    symbol: &str,
    day: NaiveDate,
) -> Result<PathBuf, Box<dyn Error>> {
    let schema = quote_schema();

    let mut timestamp_builder = TimestampNanosecondBuilder::new().with_timezone("UTC");
    let mut symbol_builder = StringBuilder::new();
    let mut ask_price_builder = Float64Builder::new();
    let mut ask_size_builder = UInt32Builder::new();
    let mut ask_exchange_builder = StringBuilder::new();
    let mut bid_price_builder = Float64Builder::new();
    let mut bid_size_builder = UInt32Builder::new();
    let mut bid_exchange_builder = StringBuilder::new();
    let mut conditions_builder = StringBuilder::new();
    let mut tape_builder = StringBuilder::new();

    let file = File::open(part_csv_path)?;
    let mut reader = csv::Reader::from_reader(file);

    for result in reader.records() {
        let record = result?;

        // 0: timestamp
        if let Some(ts_str) = record.get(0) {
            let dt = DateTime::parse_from_rfc3339(ts_str)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| day.and_hms_opt(0, 0, 0).unwrap().and_utc());
            timestamp_builder.append_value(dt.timestamp_nanos_opt().unwrap_or(0));
        } else {
            timestamp_builder.append_null();
        }

        // 1: symbol
        symbol_builder.append_value(record.get(1).unwrap_or(symbol));

        // 2: ask_price
        ask_price_builder.append_value(record.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0));

        // 3: ask_size
        ask_size_builder.append_value(record.get(3).and_then(|v| v.parse().ok()).unwrap_or(0));

        // 4: ask_exchange
        match record.get(4) {
            Some(ex) if !ex.is_empty() => ask_exchange_builder.append_value(ex),
            _ => ask_exchange_builder.append_null(),
        }

        // 5: bid_price
        bid_price_builder.append_value(record.get(5).and_then(|v| v.parse().ok()).unwrap_or(0.0));

        // 6: bid_size
        bid_size_builder.append_value(record.get(6).and_then(|v| v.parse().ok()).unwrap_or(0));

        // 7: bid_exchange
        match record.get(7) {
            Some(ex) if !ex.is_empty() => bid_exchange_builder.append_value(ex),
            _ => bid_exchange_builder.append_null(),
        }

        // 8: conditions
        match record.get(8) {
            Some(cond) if !cond.is_empty() => conditions_builder.append_value(cond),
            _ => conditions_builder.append_null(),
        }

        // 9: tape
        match record.get(9) {
            Some(tp) if !tp.is_empty() => tape_builder.append_value(tp),
            _ => tape_builder.append_null(),
        }
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(timestamp_builder.finish()),
        Arc::new(symbol_builder.finish()),
        Arc::new(ask_price_builder.finish()),
        Arc::new(ask_size_builder.finish()),
        Arc::new(ask_exchange_builder.finish()),
        Arc::new(bid_price_builder.finish()),
        Arc::new(bid_size_builder.finish()),
        Arc::new(bid_exchange_builder.finish()),
        Arc::new(conditions_builder.finish()),
        Arc::new(tape_builder.finish()),
    ];

    let batch = RecordBatch::try_new(schema.clone(), columns)?;

    let partition_dir = parquet_output_dir
        .join(format!("symbol={}", symbol))
        .join(format!("date={}", day.format("%Y-%m-%d")));
    fs::create_dir_all(&partition_dir)?;

    let target_file = partition_dir.join("quotes.parquet");
    let tmp_file = partition_dir.join("quotes.parquet.tmp");

    let file = File::create(&tmp_file)?;
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();

    let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    fs::rename(tmp_file, &target_file)?;
    Ok(target_file)
}

// --- Historical Quote Downloader ---

pub struct HistoricalQuoteDownloader {
    output_dir: PathBuf,
    parquet_output_dir: Option<PathBuf>,
    feed: String,
    currency: String,
    limit: usize,
    timeout: Duration,
    max_retries: u32,
    asof: Option<String>,
    tmp_dir: PathBuf,
    state_dir: PathBuf,
    client: Client,
    rate_limiter: RateLimiter,
}

impl HistoricalQuoteDownloader {
    pub fn new(
        api_key: &str,
        secret_key: &str,
        output_dir: PathBuf,
        parquet_output_dir: Option<PathBuf>,
        feed: String,
        currency: String,
        limit: usize,
        timeout: Duration,
        max_retries: u32,
        requests_per_minute: u32,
        asof: Option<String>,
    ) -> Result<Self, Box<dyn Error>> {
        fs::create_dir_all(&output_dir)?;
        if let Some(ref p_dir) = parquet_output_dir {
            fs::create_dir_all(p_dir)?;
        }

        let tmp_dir = output_dir.join(".historical_quotes_tmp");
        fs::create_dir_all(&tmp_dir)?;

        let state_dir = output_dir.join(".historical_quotes_state");
        fs::create_dir_all(&state_dir)?;

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert("APCA-API-KEY-ID", HeaderValue::from_str(api_key)?);
        headers.insert("APCA-API-SECRET-KEY", HeaderValue::from_str(secret_key)?);

        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()?;

        let clamped_limit = limit.clamp(1, MAX_LIMIT);
        let rate_limiter = RateLimiter::new(requests_per_minute);

        Ok(Self {
            output_dir,
            parquet_output_dir,
            feed,
            currency,
            limit: clamped_limit,
            timeout,
            max_retries,
            asof,
            tmp_dir,
            state_dir,
            client,
            rate_limiter,
        })
    }

    fn request_page(
        &self,
        symbol: &str,
        params: &[(&str, String)],
    ) -> Result<SingleSymbolQuotesResponse, Box<dyn Error>> {
        let mut delay = Duration::from_secs_f64(1.0);
        let url = BASE_URL_TEMPLATE.replace("{symbol}", symbol);

        for attempt in 1..=self.max_retries {
            self.rate_limiter.wait();

            let request_result = self.client.get(&url).query(params).send();

            match request_result {
                Ok(response) => {
                    let status = response.status();
                    let headers = response.headers().clone();

                    let remaining = headers
                        .get("X-Ratelimit-Remaining")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    let reset = headers
                        .get("X-Ratelimit-Reset")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    let limit = headers
                        .get("X-Ratelimit-Limit")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");

                    debug!(
                        "Rate limit | limit={} remaining={} reset={}",
                        limit, remaining, reset
                    );

                    if status.as_u16() == 429 {
                        let sleep_for = headers
                            .get("Retry-After")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<f64>().ok())
                            .map(Duration::from_secs_f64)
                            .unwrap_or(delay);

                        warn!(
                            "Rate limited (429) | limit={} remaining={} reset={} | sleeping {:.1}s before retry {}/{}.",
                            limit,
                            remaining,
                            reset,
                            sleep_for.as_secs_f64(),
                            attempt,
                            self.max_retries
                        );
                        thread::sleep(sleep_for);
                        delay = (delay * 2).min(Duration::from_secs(60));
                        continue;
                    }

                    if status.is_server_error() {
                        warn!(
                            "Alpaca server error {}. Sleeping {:.1}s before retry {}/{}.",
                            status,
                            delay.as_secs_f64(),
                            attempt,
                            self.max_retries
                        );
                        thread::sleep(delay);
                        delay = (delay * 2).min(Duration::from_secs(60));
                        continue;
                    }

                    if let Err(err) = response.error_for_status_ref() {
                        if attempt == self.max_retries {
                            return Err(Box::new(err));
                        }
                        warn!(
                            "Request failed: {}. Sleeping {:.1}s before retry {}/{}.",
                            err,
                            delay.as_secs_f64(),
                            attempt,
                            self.max_retries
                        );
                        thread::sleep(delay);
                        delay = (delay * 2).min(Duration::from_secs(60));
                        continue;
                    }

                    let parsed = response.json::<SingleSymbolQuotesResponse>()?;
                    return Ok(parsed);
                }
                Err(exc) => {
                    if attempt == self.max_retries {
                        return Err(Box::new(exc));
                    }
                    warn!(
                        "Request failed: {}. Sleeping {:.1}s before retry {}/{}.",
                        exc,
                        delay.as_secs_f64(),
                        attempt,
                        self.max_retries
                    );
                    thread::sleep(delay);
                    delay = (delay * 2).min(Duration::from_secs(60));
                }
            }
        }

        Err("Historical quote request exhausted retries.".into())
    }

    fn quote_to_row(symbol: &str, quote: &serde_json::Value) -> Vec<String> {
        let conditions = match quote.get("c") {
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .map(|item| item.as_str().unwrap_or(&item.to_string()).to_string())
                .collect::<Vec<_>>()
                .join("|"),
            _ => String::new(),
        };

        vec![
            quote
                .get("t")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            symbol.to_string(),
            quote.get("ap").map(|v| v.to_string()).unwrap_or_default(),
            quote.get("as").map(|v| v.to_string()).unwrap_or_default(),
            quote
                .get("ax")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            quote.get("bp").map(|v| v.to_string()).unwrap_or_default(),
            quote.get("bs").map(|v| v.to_string()).unwrap_or_default(),
            quote
                .get("bx")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            conditions,
            quote
                .get("z")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        ]
    }

    fn completed_state_path(&self, symbol: &str) -> PathBuf {
        self.state_dir
            .join(format!("{}_{}_completed_days.json", symbol, self.feed))
    }

    fn load_completed_days(&self, symbol: &str) -> BTreeSet<String> {
        let path = self.completed_state_path(symbol);
        if !path.exists() {
            return BTreeSet::new();
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<CompletedDaysState>(&content) {
                Ok(payload) => payload.completed_days.into_iter().collect(),
                Err(_) => {
                    warn!(
                        "Could not parse state file {}; starting with empty state.",
                        path.display()
                    );
                    BTreeSet::new()
                }
            },
            Err(_) => {
                warn!(
                    "Could not read state file {}; starting with empty state.",
                    path.display()
                );
                BTreeSet::new()
            }
        }
    }

    fn save_completed_days(
        &self,
        symbol: &str,
        days: &BTreeSet<String>,
    ) -> Result<(), Box<dyn Error>> {
        let path = self.completed_state_path(symbol);
        let tmp = path.with_extension("tmp");

        let state = CompletedDaysState {
            symbol: symbol.to_string(),
            feed: self.feed.clone(),
            completed_days: days.iter().cloned().collect(),
        };

        fs::write(&tmp, serde_json::to_string_pretty(&state)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    fn main_csv(&self, symbol: &str) -> PathBuf {
        self.output_dir
            .join(format!("{}_historical_quotes_{}.csv", symbol, self.feed))
    }

    fn part_csv(&self, symbol: &str, day: NaiveDate) -> PathBuf {
        self.tmp_dir.join(format!(
            "{}_{}_{}.part.csv",
            symbol,
            day.format("%Y-%m-%d"),
            self.feed
        ))
    }

    fn append_part_to_main(&self, part_path: &Path, symbol: &str) -> Result<(), Box<dyn Error>> {
        let main_path = self.main_csv(symbol);
        let main_exists = main_path.exists() && fs::metadata(&main_path)?.len() > 0;

        let mut dst = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&main_path)?;

        let src = File::open(part_path)?;
        let reader = BufReader::new(src);

        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            if main_exists && idx == 0 {
                continue;
            }
            writeln!(dst, "{}", line)?;
        }

        Ok(())
    }

    pub fn download_day(
        &self,
        symbol: &str,
        window_start: &str,
        window_end: &str,
        day: NaiveDate,
    ) -> Result<usize, Box<dyn Error>> {
        let part_path = self.part_csv(symbol, day);

        if part_path.exists() {
            fs::remove_file(&part_path)?;
        }

        let mut total = 0;
        let mut page_token: Option<String> = None;

        {
            let file = File::create(&part_path)?;
            let mut writer = csv::Writer::from_writer(file);
            writer.write_record(CSV_FIELDS)?;

            loop {
                let mut params = vec![
                    ("start", window_start.to_string()),
                    ("end", window_end.to_string()),
                    ("limit", self.limit.to_string()),
                    ("feed", self.feed.clone()),
                    ("currency", self.currency.clone()),
                    ("sort", "asc".to_string()),
                ];

                if let Some(ref asof) = self.asof {
                    params.push(("asof", asof.clone()));
                }

                if let Some(ref token) = page_token {
                    params.push(("page_token", token.clone()));
                }

                let payload = self.request_page(symbol, &params)?;

                for quote in &payload.quotes {
                    writer.write_record(&Self::quote_to_row(symbol, quote))?;
                }

                total += payload.quotes.len();
                writer.flush()?;

                page_token = payload.next_page_token;
                if page_token.is_none() {
                    break;
                }
            }
        }

        if let Some(ref p_dir) = self.parquet_output_dir {
            partition_quotes_csv_to_parquet(&part_path, p_dir, symbol, day)?;
        } else {
            self.append_part_to_main(&part_path, symbol)?;
        }

        if part_path.exists() {
            let _ = fs::remove_file(&part_path);
        }

        Ok(total)
    }

    pub fn download_symbol(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<usize, Box<dyn Error>> {
        let mut completed = self.load_completed_days(symbol);
        let mut symbol_total = 0;

        for (window_start, window_end, day) in iter_daily_windows(start, end) {
            let day_key = day.format("%Y-%m-%d").to_string();

            if completed.contains(&day_key) {
                info!("{} | {} already completed; skipping.", symbol, day_key);
                continue;
            }

            info!(
                "{} | downloading {} | {} -> {}",
                symbol, day_key, window_start, window_end
            );

            let count = self.download_day(symbol, &window_start, &window_end, day)?;
            symbol_total += count;

            completed.insert(day_key.clone());
            self.save_completed_days(symbol, &completed)?;

            info!("{} | {} | wrote {} quotes", symbol, day_key, count);
        }

        Ok(symbol_total)
    }
}

// --- CLI Arguments (Clap) ---

