//! OHLCV Arrow schema and validated conversion.

use crate::market::alpaca::models::StockBar;
use arrow::array::{ArrayRef, Float64Array, TimestampNanosecondArray, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use std::error::Error;
use std::sync::Arc;

/// Returns the Arrow schema for historical OHLCV bars.
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

/// Converts bars without replacing invalid prices or timestamps with defaults.
///
/// # Errors
/// Returns an error for invalid timestamps, nonfinite/negative prices, or Arrow construction.
pub fn bars_to_record_batch(bars: &[StockBar]) -> Result<RecordBatch, Box<dyn Error>> {
    let timestamps = bars
        .iter()
        .map(|bar| super::timestamp(&bar.t))
        .collect::<Result<Vec<_>, _>>()?;
    for bar in bars {
        for (field, value) in [
            ("open", bar.o),
            ("high", bar.h),
            ("low", bar.l),
            ("close", bar.c),
            ("vwap", bar.vw),
        ] {
            super::price(value, field)?;
        }
    }
    let columns: Vec<ArrayRef> = vec![
        Arc::new(TimestampNanosecondArray::from(timestamps).with_timezone("UTC")),
        Arc::new(Float64Array::from_iter_values(bars.iter().map(|bar| bar.o))),
        Arc::new(Float64Array::from_iter_values(bars.iter().map(|bar| bar.h))),
        Arc::new(Float64Array::from_iter_values(bars.iter().map(|bar| bar.l))),
        Arc::new(Float64Array::from_iter_values(bars.iter().map(|bar| bar.c))),
        Arc::new(UInt64Array::from_iter_values(bars.iter().map(|bar| bar.v))),
        Arc::new(UInt32Array::from_iter_values(bars.iter().map(|bar| bar.n))),
        Arc::new(Float64Array::from_iter_values(
            bars.iter().map(|bar| bar.vw),
        )),
    ];
    Ok(RecordBatch::try_new(stock_bar_schema(), columns)?)
}
