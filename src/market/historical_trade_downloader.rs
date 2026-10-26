use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveDateTime, TimeZone, Utc};
use clap::Parser;
use log::{debug, info, warn};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use crate::market::partition_parquet::partition_csv_to_parquet;
use crate::market::rate_limiter::RateLimiter;
use crate::utils::{end_of_day_rfc3339, rfc3339};

const BASE_URL: &str = "https://data.alpaca.markets/v2/stocks/trades";
const MAX_LIMIT: usize = 10_000;
const DEFAULT_SYMBOLS: &[&str] = &["SPY"];

const CSV_FIELDS: &[&str] = &[
    "timestamp",
    "symbol",
    "price",
    "size",
    "exchange",
    "trade_id",
    "conditions",
    "update",
    "tape",
];

pub fn iter_daily_windows(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<(String, String, NaiveDate)> {
    let mut windows = Vec::new();
    let mut current_day = start.date_naive();
    let end_day = end.date_naive();

    while current_day <= end_day {
        let window_start = if current_day == start.date_naive() {
            rfc3339(&start)
        } else {
            format!("{}T00:00:00Z", current_day.format("%Y-%m-%d"))
        };

        let window_end = if current_day == end.date_naive() {
            rfc3339(&end)
        } else {
            end_of_day_rfc3339(current_day)
        };

        windows.push((window_start, window_end, current_day));
        current_day = current_day.checked_add_days(Days::new(1)).unwrap();
    }

    windows
}

// --- Data Models ---

#[derive(Debug, Serialize, Deserialize)]
struct CompletedDaysState {
    symbol: String,
    feed: String,
    completed_days: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AlpacaTradesResponse {
    #[serde(default)]
    trades: HashMap<String, Vec<serde_json::Value>>,
    next_page_token: Option<String>,
}

pub struct HistoricalTradeDownloader {
    output_dir: PathBuf,
    parquet_output_dir: Option<PathBuf>,
    feed: String,
    limit: usize,
    timeout: Duration,
    max_retries: u32,
    tmp_dir: PathBuf,
    state_dir: PathBuf,
    client: Client,
    rate_limiter: RateLimiter,
}

impl HistoricalTradeDownloader {
    pub fn new(
        api_key: &str,
        secret_key: &str,
        output_dir: PathBuf,
        parquet_output_dir: Option<PathBuf>,
        feed: String,
        limit: usize,
        timeout: Duration,
        max_retries: u32,
        requests_per_minute: u32,
    ) -> Result<Self, Box<dyn Error>> {
        fs::create_dir_all(&output_dir)?;
        if let Some(ref p_dir) = parquet_output_dir {
            fs::create_dir_all(p_dir)?;
        }

        let tmp_dir = output_dir.join(".historical_tmp");
        fs::create_dir_all(&tmp_dir)?;

        let state_dir = output_dir.join(".historical_state");
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
            limit: clamped_limit,
            timeout,
            max_retries,
            tmp_dir,
            state_dir,
            client,
            rate_limiter,
        })
    }

    fn request_page(
        &self,
        params: &[(&str, String)],
    ) -> Result<AlpacaTradesResponse, Box<dyn Error>> {
        let mut delay = Duration::from_secs_f64(1.0);

        for attempt in 1..=self.max_retries {
            self.rate_limiter.wait();

            let request_result = self.client.get(BASE_URL).query(params).send();

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
                            "Rate limited (429). Sleeping {:.1}s before retry {}/{}.",
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

                    let parsed = response.json::<AlpacaTradesResponse>()?;
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

        Err("Historical trade request exhausted retries.".into())
    }

    fn trade_to_row(symbol: &str, trade: &serde_json::Value) -> Vec<String> {
        let conditions = match trade.get("c") {
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .map(|item| item.as_str().unwrap_or(&item.to_string()).to_string())
                .collect::<Vec<_>>()
                .join("|"),
            _ => String::new(),
        };

        vec![
            trade
                .get("t")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            symbol.to_string(),
            trade.get("p").map(|v| v.to_string()).unwrap_or_default(),
            trade.get("s").map(|v| v.to_string()).unwrap_or_default(),
            trade
                .get("x")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            trade.get("i").map(|v| v.to_string()).unwrap_or_default(),
            conditions,
            trade.get("u").map(|v| v.to_string()).unwrap_or_default(),
            trade
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
            .join(format!("{}_historical_trades_{}.csv", symbol, self.feed))
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
                // Skip part file's header when appending
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
                    ("symbols", symbol.to_string()),
                    ("start", window_start.to_string()),
                    ("end", window_end.to_string()),
                    ("limit", self.limit.to_string()),
                    ("feed", self.feed.clone()),
                    ("sort", "asc".to_string()),
                ];

                if let Some(ref token) = page_token {
                    params.push(("page_token", token.clone()));
                }

                let payload = self.request_page(&params)?;
                let trades = payload.trades.get(symbol).cloned().unwrap_or_default();

                for trade in &trades {
                    writer.write_record(&Self::trade_to_row(symbol, trade))?;
                }

                total += trades.len();
                writer.flush()?;

                page_token = payload.next_page_token;
                if page_token.is_none() {
                    break;
                }
            }
        }

        if let Some(ref p_dir) = self.parquet_output_dir {
            partition_csv_to_parquet(&part_path, p_dir, symbol, day)?;
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
                println!("{} {} already completed; skipping.", symbol, day_key);
                continue;
            }

            println!(
                "{} | downloading {} | {} -> {}",
                symbol, day_key, window_start, window_end
            );

            let count = self.download_day(symbol, &window_start, &window_end, day)?;
            symbol_total += count;

            completed.insert(day_key.clone());
            self.save_completed_days(symbol, &completed)?;

            println!("{} | {} | wrote {} trades", symbol, day_key, count);
        }

        Ok(symbol_total)
    }
}
