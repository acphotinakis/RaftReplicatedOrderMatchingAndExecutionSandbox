use std::error::Error;
use std::sync::Arc;

use crate::market::model::StockBar;
use arrow::array::{ArrayRef, Float64Array, Int64Array, UInt64Array};
use arrow::array::{Float64Builder, TimestampNanosecondBuilder, UInt32Builder, UInt64Builder};
use arrow::datatypes::{DataType, Field, Int64Type, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};

use std::fs::File;
use std::path::Path;

use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;

pub fn stock_bar_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
            false,
        ),
        Field::new("open", DataType::Float64, false),
        Field::new("high", DataType::Float64, false),
        Field::new("low", DataType::Float64, false),
        Field::new("close", DataType::Float64, false),
        Field::new("volume", DataType::UInt64, false),
        Field::new("trade_count", DataType::UInt32, false),
        Field::new("vwap", DataType::Float64, false),
    ]))
}

pub fn bars_to_record_batch(bars: &[StockBar]) -> Result<RecordBatch, Box<dyn Error>> {
    let schema = stock_bar_schema();

    let mut timestamp_builder =
        TimestampNanosecondBuilder::with_capacity(bars.len()).with_timezone("UTC");
    let mut open_builder = Float64Builder::with_capacity(bars.len());
    let mut high_builder = Float64Builder::with_capacity(bars.len());
    let mut low_builder = Float64Builder::with_capacity(bars.len());
    let mut close_builder = Float64Builder::with_capacity(bars.len());
    let mut volume_builder = UInt64Builder::with_capacity(bars.len());
    let mut trade_count_builder = UInt32Builder::with_capacity(bars.len());
    let mut vwap_builder = Float64Builder::with_capacity(bars.len());

    for bar in bars {
        // Parse RFC-3339 timestamp string into nanoseconds since UNIX epoch
        let nanos = DateTime::parse_from_rfc3339(&bar.t)
            .map(|dt| dt.with_timezone(&Utc).timestamp_nanos_opt().unwrap_or(0))
            .unwrap_or(0);

        timestamp_builder.append_value(nanos);
        open_builder.append_value(bar.o);
        high_builder.append_value(bar.h);
        low_builder.append_value(bar.l);
        close_builder.append_value(bar.c);
        volume_builder.append_value(bar.v);
        trade_count_builder.append_value(bar.n);
        vwap_builder.append_value(bar.vw);
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(timestamp_builder.finish()),
        Arc::new(open_builder.finish()),
        Arc::new(high_builder.finish()),
        Arc::new(low_builder.finish()),
        Arc::new(close_builder.finish()),
        Arc::new(volume_builder.finish()),
        Arc::new(trade_count_builder.finish()),
        Arc::new(vwap_builder.finish()),
    ];

    Ok(RecordBatch::try_new(schema, columns)?)
}

pub fn save_parquet(
    batch: &RecordBatch,
    path: impl AsRef<Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;

    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .build();

    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(properties))?;

    writer.write(batch)?;
    writer.close()?;

    Ok(())
}
