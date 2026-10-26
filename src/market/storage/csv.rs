//! CSV layouts and serialization for downloaded records.

use crate::market::alpaca::models::{StockQuote, StockTrade, Tape};
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Columns emitted for historical quotes.
pub const QUOTE_FIELDS: &[&str] = &[
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
/// Columns emitted for historical trades.
pub const TRADE_FIELDS: &[&str] = &[
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

fn tape(value: Tape) -> &'static str {
    match value {
        Tape::A => "A",
        Tape::B => "B",
        Tape::C => "C",
        Tape::N => "N",
        Tape::O => "O",
    }
}

/// Converts an Alpaca quote to the quote CSV layout.
///
/// # Errors
/// Rejects malformed timestamps and nonfinite or negative prices.
pub fn quote_to_row(symbol: &str, quote: &StockQuote) -> Result<Vec<String>, Box<dyn Error>> {
    super::parquet::timestamp(&quote.timestamp)?;
    super::parquet::price(quote.ask_price, "ask_price")?;
    super::parquet::price(quote.bid_price, "bid_price")?;
    Ok(vec![
        quote.timestamp.clone(),
        symbol.to_owned(),
        quote.ask_price.to_string(),
        quote.ask_size.to_string(),
        quote.ask_exchange.clone(),
        quote.bid_price.to_string(),
        quote.bid_size.to_string(),
        quote.bid_exchange.clone(),
        quote.conditions.join("|"),
        tape(quote.tape).to_owned(),
    ])
}

/// Converts an Alpaca trade to the trade CSV layout.
///
/// # Errors
/// Rejects malformed timestamps and nonfinite or negative prices.
pub fn trade_to_row(symbol: &str, trade: &StockTrade) -> Result<Vec<String>, Box<dyn Error>> {
    super::parquet::timestamp(&trade.timestamp)?;
    super::parquet::price(trade.price, "price")?;
    Ok(vec![
        trade.timestamp.clone(),
        symbol.to_owned(),
        trade.price.to_string(),
        trade.size.to_string(),
        trade.exchange.clone(),
        trade.trade_id.to_string(),
        trade.conditions.join("|"),
        trade.update.clone().unwrap_or_default(),
        tape(trade.tape).to_owned(),
    ])
}

/// Appends CSV records with one header, preserving quoted multiline fields.
///
/// # Errors
/// Returns read, write, or incompatible-header errors. This operation is not a
/// checkpoint transaction; resumable downloads should use [`rebuild_from_parts`].
pub fn append_part(part_path: &Path, main_path: &Path) -> Result<(), Box<dyn Error>> {
    let mut source = ::csv::Reader::from_path(part_path)?;
    let headers = source.headers()?.clone();
    let has_output = match fs::metadata(main_path) {
        Ok(metadata) => metadata.len() > 0,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if has_output {
        let mut existing = ::csv::Reader::from_reader(File::open(main_path)?);
        if existing.headers()? != &headers {
            return Err(
                std::io::Error::new(std::io::ErrorKind::InvalidData, "CSV headers differ").into(),
            );
        }
    }
    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(main_path)?;
    let mut writer = ::csv::Writer::from_writer(output);
    if !has_output {
        writer.write_record(&headers)?;
    }
    for record in source.records() {
        writer.write_record(&record?)?;
    }
    writer.flush()?;
    Ok(())
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Rebuilds a combined CSV from ordered immutable parts and atomically publishes it.
///
/// # Errors
/// Returns filesystem, CSV, or header mismatch errors; preserves the old target
/// when any input is invalid. An empty part list is rejected.
pub fn rebuild_from_parts(parts: &[PathBuf], target: &Path) -> Result<(), Box<dyn Error>> {
    if parts.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "at least one CSV part is required",
        )
        .into());
    }
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".csv-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut writer = ::csv::Writer::from_writer(file);
        let mut expected = None;
        for part in parts {
            let mut reader = ::csv::Reader::from_path(part)?;
            let headers = reader.headers()?.clone();
            match &expected {
                None => {
                    writer.write_record(&headers)?;
                    expected = Some(headers);
                }
                Some(previous) if previous != &headers => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "CSV headers differ",
                    )
                    .into());
                }
                Some(_) => {}
            }
            for record in reader.records() {
                writer.write_record(&record?)?;
            }
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        fs::rename(&temporary, target)?;
        Ok(())
    })();
    if result.is_err()
        && let Err(error) = fs::remove_file(&temporary)
    {
        log::warn!(
            "Unable to remove temporary CSV {}: {error}",
            temporary.display()
        );
    }
    result
}
