//! Dataset serialization and publication, independent of HTTP and download state.

pub mod csv;
pub mod parquet;
pub mod paths;

pub use parquet::bars::{bars_to_record_batch, stock_bar_schema};
pub use parquet::quotes::{partition_quotes_csv_to_parquet, quote_schema};
pub use parquet::trades::{partition_csv_to_parquet, trade_schema};
pub use parquet::writer::save_parquet;

#[cfg(test)]
mod tests;
