//! Trade CSV conversion and Parquet schema.

use arrow::array::{
    ArrayRef, Float64Builder, StringBuilder, TimestampNanosecondBuilder, UInt64Builder,
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
struct TradeRow {
    timestamp: String,
    symbol: String,
    price: f64,
    size: u64,
    exchange: Option<String>,
    trade_id: Option<u64>,
    conditions: Option<String>,
    update: Option<String>,
    tape: Option<String>,
}

/// Returns the Arrow schema for historical trade CSV records.
pub fn trade_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
            false,
        ),
        Field::new("symbol", DataType::Utf8, false),
        Field::new("price", DataType::Float64, false),
        Field::new("size", DataType::UInt64, false),
        Field::new("exchange", DataType::Utf8, true),
        Field::new("trade_id", DataType::UInt64, true),
        Field::new("conditions", DataType::Utf8, true),
        Field::new("update", DataType::Utf8, true),
        Field::new("tape", DataType::Utf8, true),
    ]))
}

/// Validates a trade CSV and publishes a symbol/date Parquet partition.
///
/// # Errors
/// Returns malformed-field, symbol mismatch, filesystem, CSV, or Parquet errors.
pub fn partition_csv_to_parquet(
    part_csv_path: &Path,
    parquet_output_dir: &Path,
    symbol: &str,
    day: NaiveDate,
) -> Result<PathBuf, Box<dyn Error>> {
    let mut timestamps = TimestampNanosecondBuilder::new().with_timezone("UTC");
    let mut symbols = StringBuilder::new();
    let mut prices = Float64Builder::new();
    let mut sizes = UInt64Builder::new();
    let mut exchanges = StringBuilder::new();
    let mut trade_ids = UInt64Builder::new();
    let mut conditions = StringBuilder::new();
    let mut updates = StringBuilder::new();
    let mut tapes = StringBuilder::new();
    let mut reader = csv::Reader::from_path(part_csv_path)?;
    for record in reader.deserialize::<TradeRow>() {
        let row = record?;
        if row.symbol != symbol {
            return Err(super::invalid("symbol", "does not match partition").into());
        }
        timestamps.append_value(super::timestamp(&row.timestamp)?);
        symbols.append_value(row.symbol);
        prices.append_value(super::price(row.price, "price")?);
        sizes.append_value(row.size);
        exchanges.append_option(row.exchange);
        trade_ids.append_option(row.trade_id);
        conditions.append_option(row.conditions);
        updates.append_option(row.update);
        tapes.append_option(row.tape);
    }
    let columns: Vec<ArrayRef> = vec![
        Arc::new(timestamps.finish()),
        Arc::new(symbols.finish()),
        Arc::new(prices.finish()),
        Arc::new(sizes.finish()),
        Arc::new(exchanges.finish()),
        Arc::new(trade_ids.finish()),
        Arc::new(conditions.finish()),
        Arc::new(updates.finish()),
        Arc::new(tapes.finish()),
    ];
    let batch = RecordBatch::try_new(trade_schema(), columns)?;
    let target = crate::market::storage::paths::partition_file(
        parquet_output_dir,
        symbol,
        day,
        "trades.parquet",
    )?;
    super::writer::write_batch(&batch, &target, Compression::SNAPPY)?;
    Ok(target)
}
