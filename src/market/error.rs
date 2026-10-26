//! Errors at market-data acquisition and persistence boundaries.

use std::{error::Error, fmt};

/// A failure fetching, decoding, or storing market data.
#[derive(Debug)]
pub enum MarketError {
    /// Invalid client or download configuration.
    InvalidConfig(String),
    /// Malformed market data.
    InvalidData(String),
    /// Filesystem failure.
    Io(std::io::Error),
    /// Invalid JSON payload or checkpoint.
    Json(serde_json::Error),
    /// Invalid CSV data.
    Csv(csv::Error),
    /// HTTP transport failure.
    Http(reqwest::Error),
    /// Arrow conversion failure.
    Arrow(arrow::error::ArrowError),
    /// Parquet encoding failure.
    Parquet(parquet::errors::ParquetError),
    /// Unsuccessful server response.
    HttpStatus { status: u16, body: String },
    /// All configured request attempts failed.
    RetriesExhausted,
}

impl fmt::Display for MarketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid configuration: {message}"),
            Self::InvalidData(message) => write!(f, "invalid market data: {message}"),
            Self::Io(error) => write!(f, "filesystem: {error}"),
            Self::Json(error) => write!(f, "JSON: {error}"),
            Self::Csv(error) => write!(f, "CSV: {error}"),
            Self::Http(error) => write!(f, "HTTP: {error}"),
            Self::Arrow(error) => write!(f, "Arrow: {error}"),
            Self::Parquet(error) => write!(f, "Parquet: {error}"),
            Self::HttpStatus { status, body } => write!(f, "HTTP {status}: {body}"),
            Self::RetriesExhausted => f.write_str("request exhausted retries"),
        }
    }
}

impl Error for MarketError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Csv(error) => Some(error),
            Self::Http(error) => Some(error),
            Self::Arrow(error) => Some(error),
            Self::Parquet(error) => Some(error),
            Self::InvalidConfig(_) | Self::InvalidData(_) | Self::HttpStatus { .. }
            | Self::RetriesExhausted => None,
        }
    }
}

macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for MarketError {
            fn from(error: $source) -> Self { Self::$variant(error) }
        }
    };
}
from_error!(std::io::Error, Io);
from_error!(serde_json::Error, Json);
from_error!(csv::Error, Csv);
from_error!(reqwest::Error, Http);
from_error!(arrow::error::ArrowError, Arrow);
from_error!(parquet::errors::ParquetError, Parquet);
