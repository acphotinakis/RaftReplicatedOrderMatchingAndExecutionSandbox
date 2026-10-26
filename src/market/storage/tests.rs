use super::*;
use crate::market::alpaca::models::StockBar;
use ::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use arrow::array::{Array, Float64Array, StringArray, TimestampNanosecondArray, UInt64Array};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "market-storage-test-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn read_batch(path: &Path) -> arrow::record_batch::RecordBatch {
    ParquetRecordBatchReaderBuilder::try_new(File::open(path).unwrap())
        .unwrap()
        .build()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
}

fn bar() -> StockBar {
    StockBar {
        o: 100.25,
        h: 101.0,
        l: 100.0,
        c: 100.5,
        v: 42,
        n: 3,
        vw: 100.4,
        t: "2024-01-03T09:30:00.123456789Z".into(),
    }
}

#[test]
fn bar_parquet_roundtrip_preserves_values_and_nanoseconds() {
    // Given a bar with subsecond time and a new target path.
    let dir = TestDir::new();
    let target = dir.0.join("bars.parquet");
    let batch = bars_to_record_batch(&[bar()]).unwrap();
    // When it is published as Parquet and read through the Parquet reader.
    save_parquet(&batch, &target).unwrap();
    let actual = read_batch(&target);
    // Then values, timestamp precision, schema, and row count survive.
    assert_eq!(actual, batch);
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[test]
fn bars_reject_invalid_timestamps_and_nonfinite_prices() {
    // Given malformed timestamps and prices which previously became defaults.
    for malformed in ["not-a-time", "9999-01-01T00:00:00Z"] {
        let mut input = bar();
        input.t = malformed.into();
        // When conversion is attempted, then it fails explicitly.
        assert!(bars_to_record_batch(&[input]).is_err());
    }
    let mut input = bar();
    input.c = f64::NAN;
    assert!(bars_to_record_batch(&[input]).is_err());
}

#[test]
fn quote_csv_roundtrip_preserves_nullable_fields() {
    // Given a quote CSV with an optional blank exchange and zero inactive bid.
    let dir = TestDir::new();
    let input = dir.0.join("quotes.csv");
    fs::write(&input, "timestamp,symbol,ask_price,ask_size,ask_exchange,bid_price,bid_size,bid_exchange,conditions,tape\n2024-01-03T09:30:00.123456789Z,SPY,100.25,10,V,0,0,,R,C\n").unwrap();
    // When converted and read back.
    let target = partition_quotes_csv_to_parquet(
        &input,
        &dir.0,
        "SPY",
        chrono::NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
    )
    .unwrap();
    let batch = read_batch(&target);
    // Then the schema and values remain faithful to the CSV.
    assert_eq!(batch.schema(), quote_schema());
    assert_eq!(batch.num_rows(), 1);
    assert!(
        (batch
            .column(2)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0)
            - 100.25)
            .abs()
            < f64::EPSILON
    );
    assert!(batch.column(7).is_null(0));
    assert_eq!(
        batch
            .column(0)
            .as_any()
            .downcast_ref::<TimestampNanosecondArray>()
            .unwrap()
            .value(0),
        1_704_274_200_123_456_789
    );
}

#[test]
fn trade_csv_roundtrip_preserves_identifiers() {
    // Given a trade containing an identifier too large for exact f64 representation.
    let dir = TestDir::new();
    let input = dir.0.join("trades.csv");
    fs::write(&input, "timestamp,symbol,price,size,exchange,trade_id,conditions,update,tape\n2024-01-03T09:30:00Z,SPY,100.25,10,V,9007199254740993,R,,C\n").unwrap();
    // When converted and read back.
    let target = partition_csv_to_parquet(
        &input,
        &dir.0,
        "SPY",
        chrono::NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
    )
    .unwrap();
    let batch = read_batch(&target);
    // Then the integer identifier is exact and the symbol is retained.
    assert_eq!(
        batch
            .column(5)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap()
            .value(0),
        9_007_199_254_740_993
    );
    assert_eq!(
        batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(0),
        "SPY"
    );
}

#[test]
fn malformed_csv_never_replaces_existing_partition() {
    // Given an existing output and malformed required CSV fields.
    let dir = TestDir::new();
    let day = chrono::NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
    let target = paths::partition_file(&dir.0, "SPY", day, "trades.parquet").unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"existing output").unwrap();
    let input = dir.0.join("bad.csv");
    for row in [
        "invalid,SPY,100.25,10,V,7,R,,C",
        "2024-01-03T09:30:00Z,SPY,bad,10,V,7,R,,C",
        "2024-01-03T09:30:00Z,SPY,100.25,-1,V,7,R,,C",
        "2024-01-03T09:30:00Z,SPY,NaN,10,V,7,R,,C",
        "2024-01-03T09:30:00Z,QQQ,100.25,10,V,7,R,,C",
    ] {
        fs::write(
            &input,
            format!(
                "timestamp,symbol,price,size,exchange,trade_id,conditions,update,tape\n{row}\n"
            ),
        )
        .unwrap();
        // When conversion fails, then the existing dataset is unchanged.
        assert!(partition_csv_to_parquet(&input, &dir.0, "SPY", day).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"existing output");
    }
}

#[test]
fn rebuilding_csv_is_repeatable_and_preserves_multiline_fields() {
    // Given two immutable parts with a quoted newline in a field.
    let dir = TestDir::new();
    let first = dir.0.join("first.csv");
    let second = dir.0.join("second.csv");
    let target = dir.0.join("all.csv");
    fs::write(&first, "id,note\n1,\"first\nline\"\n").unwrap();
    fs::write(&second, "id,note\n2,second\n").unwrap();
    let parts = [first, second];
    csv::rebuild_from_parts(&parts, &target).unwrap();
    let before = fs::read(&target).unwrap();
    // When publication is retried from the same parts.
    csv::rebuild_from_parts(&parts, &target).unwrap();
    // Then records are not duplicated and quoted fields remain intact.
    assert_eq!(fs::read(&target).unwrap(), before);
    let rows = ::csv::Reader::from_path(&target)
        .unwrap()
        .records()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(&rows[0][1], "first\nline");
}

#[test]
fn rebuilding_rejects_inconsistent_headers_without_replacing_output() {
    // Given incompatible CSV parts and a previously published file.
    let dir = TestDir::new();
    let first = dir.0.join("first.csv");
    let second = dir.0.join("second.csv");
    let target = dir.0.join("all.csv");
    fs::write(&first, "id,note\n1,first\n").unwrap();
    fs::write(&second, "id,price\n2,10\n").unwrap();
    fs::write(&target, b"previous output").unwrap();
    // When rebuilding, then the error preserves the old output.
    assert!(csv::rebuild_from_parts(&[first, second], &target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"previous output");
}
