//! Arrow schemas, validated conversion, and Parquet publication.

pub mod bars;
pub mod quotes;
pub mod trades;
pub mod writer;

use std::io;

pub(super) fn invalid(field: &str, detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid {field}: {detail}"),
    )
}

pub(super) fn timestamp(value: &str) -> Result<i64, io::Error> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|error| invalid("timestamp", error))?
        .timestamp_nanos_opt()
        .ok_or_else(|| invalid("timestamp", "outside nanosecond range"))
}

pub(super) fn price(value: f64, field: &str) -> Result<f64, io::Error> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(invalid(field, "must be finite and nonnegative"))
    }
}
