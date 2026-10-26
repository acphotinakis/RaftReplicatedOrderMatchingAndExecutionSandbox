//! Quote CSV conversion and Parquet schema.

use arrow::array::{
    ArrayRef, Float64Builder, StringBuilder, TimestampNanosecondBuilder, UInt32Builder,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::NaiveDate;
use parquet::basic::Compression;
use serde::Deserialize;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Deserialize)]
struct QuoteRow {
    timestamp: String,
    symbol: String,
    ask_price: f64,
    ask_size: u32,
    ask_exchange: Option<String>,
    bid_price: f64,
    bid_size: u32,
    bid_exchange: Option<String>,
    conditions: Option<String>,
    tape: Option<String>,
}

/// Returns the Arrow schema for historical quote CSV records.
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

/// Validates a quote CSV and publishes a symbol/date Parquet partition.
///
/// # Errors
/// Returns malformed-field, symbol mismatch, filesystem, CSV, or Parquet errors.
pub fn partition_quotes_csv_to_parquet(
    part_csv_path: &Path,
    parquet_output_dir: &Path,
    symbol: &str,
    day: NaiveDate,
) -> Result<PathBuf, Box<dyn Error>> {
    let mut timestamps = TimestampNanosecondBuilder::new().with_timezone("UTC");
    let mut symbols = StringBuilder::new();
    let mut ask_prices = Float64Builder::new();
    let mut ask_sizes = UInt32Builder::new();
    let mut ask_exchanges = StringBuilder::new();
    let mut bid_prices = Float64Builder::new();
    let mut bid_sizes = UInt32Builder::new();
    let mut bid_exchanges = StringBuilder::new();
    let mut conditions = StringBuilder::new();
    let mut tapes = StringBuilder::new();
    let mut reader = csv::Reader::from_path(part_csv_path)?;
    for record in reader.deserialize::<QuoteRow>() {
        let row = record?;
        if row.symbol != symbol {
            return Err(super::invalid("symbol", "does not match partition").into());
        }
        timestamps.append_value(super::timestamp(&row.timestamp)?);
        symbols.append_value(row.symbol);
        ask_prices.append_value(super::price(row.ask_price, "ask_price")?);
        ask_sizes.append_value(row.ask_size);
        ask_exchanges.append_option(row.ask_exchange);
        bid_prices.append_value(super::price(row.bid_price, "bid_price")?);
        bid_sizes.append_value(row.bid_size);
        bid_exchanges.append_option(row.bid_exchange);
        conditions.append_option(row.conditions);
        tapes.append_option(row.tape);
    }
    let columns: Vec<ArrayRef> = vec![
        Arc::new(timestamps.finish()),
        Arc::new(symbols.finish()),
        Arc::new(ask_prices.finish()),
        Arc::new(ask_sizes.finish()),
        Arc::new(ask_exchanges.finish()),
        Arc::new(bid_prices.finish()),
        Arc::new(bid_sizes.finish()),
        Arc::new(bid_exchanges.finish()),
        Arc::new(conditions.finish()),
        Arc::new(tapes.finish()),
    ];
    let batch = RecordBatch::try_new(quote_schema(), columns)?;
    let target = crate::market::storage::paths::partition_file(
        parquet_output_dir,
        symbol,
        day,
        "quotes.parquet",
    )?;
    super::writer::write_batch(&batch, &target, Compression::SNAPPY)?;
    Ok(target)
}
