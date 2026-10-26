//! Dataset partition naming.

use chrono::NaiveDate;
use std::error::Error;
use std::path::{Component, Path, PathBuf};

/// Builds a symbol/date partition path without creating it.
///
/// # Errors
/// Returns an error if the symbol or filename is not a single path component.
pub fn partition_file(
    root: &Path,
    symbol: &str,
    day: NaiveDate,
    filename: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    for value in [symbol, filename] {
        let mut components = Path::new(value).components();
        if value.is_empty()
            || !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "symbol and filename must each be a single path component",
            )
            .into());
        }
    }
    Ok(root
        .join(format!("symbol={symbol}"))
        .join(format!("date={}", day.format("%Y-%m-%d")))
        .join(filename))
}
