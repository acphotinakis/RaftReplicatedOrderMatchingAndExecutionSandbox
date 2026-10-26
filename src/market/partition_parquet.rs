use arrow::array::{
    ArrayRef, Float64Builder, StringBuilder, TimestampNanosecondBuilder, UInt64Builder,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, NaiveDate, Utc};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Builds the Arrow Schema corresponding to Alpaca Trade CSV output.
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

/// Reads a single day's trade CSV part file, converts it into an Arrow RecordBatch,
/// and writes it to a date-partitioned Parquet directory:
/// `{parquet_output_dir}/symbol={symbol}/date={YYYY-MM-DD}/trades.parquet`
pub fn partition_csv_to_parquet(
    part_csv_path: &Path,
    parquet_output_dir: &Path,
    symbol: &str,
    day: NaiveDate,
) -> Result<PathBuf, Box<dyn Error>> {
    let schema = trade_schema();

    // Builders for typed Arrow columns
    let mut timestamp_builder = TimestampNanosecondBuilder::new();
    let mut symbol_builder = StringBuilder::new();
    let mut price_builder = Float64Builder::new();
    let mut size_builder = UInt64Builder::new();
    let mut exchange_builder = StringBuilder::new();
    let mut trade_id_builder = UInt64Builder::new();
    let mut conditions_builder = StringBuilder::new();
    let mut update_builder = StringBuilder::new();
    let mut tape_builder = StringBuilder::new();

    let file = File::open(part_csv_path)?;
    let mut reader = csv::Reader::from_reader(file);

    for result in reader.records() {
        let record = result?;

        // 0: timestamp (RFC-3339) -> nanoseconds since UNIX epoch
        if let Some(ts_str) = record.get(0) {
            let dt = DateTime::parse_from_rfc3339(ts_str)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| {
                    day.and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                });
            timestamp_builder.append_value(dt.timestamp_nanos_opt().unwrap_or(0));
        } else {
            timestamp_builder.append_null();
        }

        // 1: symbol
        symbol_builder.append_value(record.get(1).unwrap_or(symbol));

        // 2: price (f64)
        let price: f64 = record.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        price_builder.append_value(price);

        // 3: size (u64)
        let size: u64 = record.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
        size_builder.append_value(size);

        // 4: exchange
        match record.get(4) {
            Some(ex) if !ex.is_empty() => exchange_builder.append_value(ex),
            _ => exchange_builder.append_null(),
        }

        // 5: trade_id (u64)
        match record.get(5).and_then(|v| v.parse::<u64>().ok()) {
            Some(id) => trade_id_builder.append_value(id),
            None => trade_id_builder.append_null(),
        }

        // 6: conditions
        match record.get(6) {
            Some(cond) if !cond.is_empty() => conditions_builder.append_value(cond),
            _ => conditions_builder.append_null(),
        }

        // 7: update
        match record.get(7) {
            Some(upd) if !upd.is_empty() => update_builder.append_value(upd),
            _ => update_builder.append_null(),
        }

        // 8: tape
        match record.get(8) {
            Some(tp) if !tp.is_empty() => tape_builder.append_value(tp),
            _ => tape_builder.append_null(),
        }
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(timestamp_builder.finish()),
        Arc::new(symbol_builder.finish()),
        Arc::new(price_builder.finish()),
        Arc::new(size_builder.finish()),
        Arc::new(exchange_builder.finish()),
        Arc::new(trade_id_builder.finish()),
        Arc::new(conditions_builder.finish()),
        Arc::new(update_builder.finish()),
        Arc::new(tape_builder.finish()),
    ];

    let batch = RecordBatch::try_new(schema.clone(), columns)?;

    // Partition path: {parquet_output_dir}/symbol={symbol}/date=YYYY-MM-DD/
    let partition_dir = parquet_output_dir
        .join(format!("symbol={}", symbol))
        .join(format!("date={}", day.format("%Y-%m-%d")));
    fs::create_dir_all(&partition_dir)?;

    let target_file = partition_dir.join("trades.parquet");
    let tmp_file = partition_dir.join("trades.parquet.tmp");

    let file = File::create(&tmp_file)?;
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();

    let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    // Atomic move to final destination
    fs::rename(tmp_file, &target_file)?;

    Ok(target_file)
}