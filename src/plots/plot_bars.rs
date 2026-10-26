//! Historical bar chart visualization.

use std::error::Error;
use std::fs::File;
use std::path::PathBuf;

use arrow::array::{Float64Array, TimestampNanosecondArray, UInt32Array, UInt64Array};
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use plotters::prelude::*;

use crate::market::alpaca::models::StockBar;
use crate::market::error::MarketError;

/// Generates chart visualizations for historical bars.
pub struct PlotBars {
    /// Stock symbol (e.g. SPY).
    pub symbol: String,
    /// Path to input file or directory containing parquet bar files.
    pub input_dir: PathBuf,
    /// Type of dataset (e.g. "bars").
    pub data_type: String,
}

impl PlotBars {
    /// Creates a new `PlotBars` configuration instance.
    pub fn new(
        symbol: String,
        input_dir: PathBuf,
        data_type: String,
    ) -> Result<Self, Box<dyn Error>> {
        if symbol.is_empty() {
            return Err(MarketError::InvalidConfig("symbol must not be empty".into()).into());
        }
        Ok(Self {
            symbol,
            input_dir,
            data_type,
        })
    }

    /// Reads Parquet bar files into [`StockBar`] instances and plots Close Price and Volume.
    pub fn plot_bars(&self) -> Result<(), Box<dyn Error>> {
        let file_path = if self.input_dir.is_dir() {
            self.input_dir.join(format!("{}_bars.parquet", self.symbol))
        } else {
            self.input_dir.clone()
        };

        if !file_path.exists() {
            return Err(MarketError::InvalidData(format!(
                "input file not found at {}",
                file_path.display()
            ))
            .into());
        }

        // Read records into StockBar structs
        let file = File::open(&file_path)?;
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)?.build()?;

        let mut bars: Vec<StockBar> = Vec::new();
        for batch_res in reader {
            let batch = batch_res?;
            let schema = batch.schema();

            let ts_col_idx = schema.index_of("timestamp")?;
            let open_col_idx = schema.index_of("open")?;
            let high_col_idx = schema.index_of("high")?;
            let low_col_idx = schema.index_of("low")?;
            let close_col_idx = schema.index_of("close")?;
            let volume_col_idx = schema.index_of("volume")?;
            let trade_count_col_idx = schema.index_of("trade_count").ok();
            let vwap_col_idx = schema.index_of("vwap").ok();

            let ts_arr = batch
                .column(ts_col_idx)
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .ok_or_else(|| MarketError::InvalidData("invalid timestamp column".into()))?;
            let open_arr = batch
                .column(open_col_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid open column".into()))?;
            let high_arr = batch
                .column(high_col_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid high column".into()))?;
            let low_arr = batch
                .column(low_col_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid low column".into()))?;
            let close_arr = batch
                .column(close_col_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid close column".into()))?;
            let volume_arr = batch
                .column(volume_col_idx)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid volume column".into()))?;

            let trade_count_arr = trade_count_col_idx
                .and_then(|idx| batch.column(idx).as_any().downcast_ref::<UInt32Array>());
            let vwap_arr = vwap_col_idx
                .and_then(|idx| batch.column(idx).as_any().downcast_ref::<Float64Array>());

            for i in 0..batch.num_rows() {
                let nanos = ts_arr.value(i);
                let dt = DateTime::<Utc>::from_timestamp_nanos(nanos);
                let bar = StockBar {
                    t: dt.to_rfc3339(),
                    o: open_arr.value(i),
                    h: high_arr.value(i),
                    l: low_arr.value(i),
                    c: close_arr.value(i),
                    v: volume_arr.value(i),
                    n: trade_count_arr.map(|a| a.value(i)).unwrap_or(0),
                    vw: vwap_arr.map(|a| a.value(i)).unwrap_or(0.0),
                };
                bars.push(bar);
            }
        }

        if bars.is_empty() {
            println!("No bars found in {}", file_path.display());
            return Ok(());
        }

        println!("Loaded {} bars for {}", bars.len(), self.symbol);

        let parent_dir = file_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        // 2. Place the chart file into that same directory
        let output_filename = parent_dir.join(format!("{}_close_volume.png", self.symbol));
        let root = BitMapBackend::new(&output_filename, (1200, 800)).into_drawing_area();
        root.fill(&WHITE)?;

        // Split canvas into two vertical panels: Upper 70% Price, Lower 30% Volume
        let (upper_area, lower_area) = root.split_vertically(560);

        let min_close = bars.iter().map(|b| b.c).fold(f64::INFINITY, f64::min);
        let max_close = bars.iter().map(|b| b.c).fold(f64::NEG_INFINITY, f64::max);
        let max_vol = bars.iter().map(|b| b.v).max().unwrap_or(1);

        let price_margin = ((max_close - min_close) * 0.05).max(0.5);
        let price_range = (min_close - price_margin)..(max_close + price_margin);
        let x_range = 0usize..bars.len();

        // 1. Plot Close Price in upper panel
        let mut price_chart = ChartBuilder::on(&upper_area)
            .caption(
                format!("{} - Close Price", self.symbol),
                ("sans-serif", 24).into_font(),
            )
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(x_range.clone(), price_range)?;

        price_chart
            .configure_mesh()
            .y_desc("Close Price ($)")
            .x_labels(10)
            .x_label_formatter(&|idx| {
                if let Some(bar) = bars.get(*idx) {
                    if bar.t.len() >= 16 {
                        bar.t[5..16].replace('T', " ")
                    } else {
                        bar.t.clone()
                    }
                } else {
                    String::new()
                }
            })
            .draw()?;

        let close_series = bars.iter().enumerate().map(|(i, b)| (i, b.c));
        price_chart
            .draw_series(LineSeries::new(close_series, &BLUE))?
            .label("Close Price")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

        price_chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .draw()?;

        // 2. Plot Volume in lower panel
        let mut volume_chart = ChartBuilder::on(&lower_area)
            .caption(
                format!("{} - Volume", self.symbol),
                ("sans-serif", 20).into_font(),
            )
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(x_range, 0u64..(max_vol + (max_vol / 10).max(1)))?;

        volume_chart
            .configure_mesh()
            .y_desc("Volume")
            .x_labels(10)
            .x_label_formatter(&|idx| {
                if let Some(bar) = bars.get(*idx) {
                    if bar.t.len() >= 16 {
                        bar.t[5..16].replace('T', " ")
                    } else {
                        bar.t.clone()
                    }
                } else {
                    String::new()
                }
            })
            .draw()?;

        volume_chart.draw_series(bars.iter().enumerate().map(|(i, b)| {
            Rectangle::new([(i, 0), (i + 1, b.v)], RGBColor(70, 130, 180).filled())
        }))?;

        root.present()?;
        println!("Saved chart to {}", output_filename.display());
        Ok(())
    }
}
