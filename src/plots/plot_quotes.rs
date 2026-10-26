//! NBBO quote spread and depth imbalance visualization.

use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};

use arrow::array::{Float64Array, TimestampNanosecondArray, UInt32Array};
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use plotters::prelude::*;

use crate::market::error::MarketError;

/// Individual quote sample used for charting.
#[derive(Debug, Clone)]
pub struct QuotePoint {
    /// UTC timestamp in nanoseconds since UNIX epoch.
    pub timestamp_ns: i64,
    /// Human-readable time string (HH:MM:SS).
    pub timestamp_str: String,
    /// National Best Bid price.
    pub bid_price: f64,
    /// Bid depth/size.
    pub bid_size: u32,
    /// National Best Offer (Ask) price.
    pub ask_price: f64,
    /// Ask depth/size.
    pub ask_size: u32,
    /// Midpoint price: `(bid + ask) / 2`.
    pub mid_price: f64,
    /// Spread in basis points: `((ask - bid) / mid) * 10,000`.
    pub spread_bps: f64,
    /// Order Flow Imbalance: `(bid_size - ask_size) / (bid_size + ask_size)`.
    pub imbalance: f64,
}

/// Generates NBBO spread and order flow depth imbalance visualizations from quotes Parquet.
pub struct PlotQuotes {
    /// Stock symbol (e.g. SPY).
    pub symbol: String,
    /// Path to quotes Parquet file.
    pub input_file: PathBuf,
    /// Optional output file path.
    pub output_file: Option<PathBuf>,
    /// Optional start time filter string (e.g. "09:30:00" or ISO-8601).
    pub start_time: Option<String>,
    /// Optional end time filter string (e.g. "09:35:00" or ISO-8601).
    pub end_time: Option<String>,
    /// Maximum number of sampled points to render.
    pub max_points: usize,
}

impl PlotQuotes {
    /// Creates a new `PlotQuotes` configuration instance.
    pub fn new(
        symbol: String,
        input_file: PathBuf,
        output_file: Option<PathBuf>,
        start_time: Option<String>,
        end_time: Option<String>,
        max_points: usize,
    ) -> Result<Self, Box<dyn Error>> {
        if symbol.is_empty() {
            return Err(MarketError::InvalidConfig("symbol must not be empty".into()).into());
        }
        Ok(Self {
            symbol,
            input_file,
            output_file,
            start_time,
            end_time,
            max_points: if max_points == 0 { 5000 } else { max_points },
        })
    }

    /// Reads quotes Parquet, calculates spread and OFI, and renders a 3-panel visualization.
    pub fn plot_quotes(&self) -> Result<(), Box<dyn Error>> {
        let file_path = if self.input_file.is_dir() {
            self.input_file
                .join(format!("{}_quotes.parquet", self.symbol))
        } else {
            self.input_file.clone()
        };

        if !file_path.exists() {
            return Err(MarketError::InvalidData(format!(
                "quotes parquet not found at {}",
                file_path.display()
            ))
            .into());
        }

        println!("Reading quotes Parquet from {}", file_path.display());
        let file = File::open(&file_path)?;
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)?.build()?;

        let mut raw_quotes: Vec<QuotePoint> = Vec::new();

        for batch_res in reader {
            let batch = batch_res?;
            let schema = batch.schema();

            let ts_col_idx = schema.index_of("timestamp")?;
            let ask_p_idx = schema.index_of("ask_price")?;
            let ask_s_idx = schema.index_of("ask_size")?;
            let bid_p_idx = schema.index_of("bid_price")?;
            let bid_s_idx = schema.index_of("bid_size")?;

            let ts_arr = batch
                .column(ts_col_idx)
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .ok_or_else(|| MarketError::InvalidData("invalid timestamp column".into()))?;
            let ask_p_arr = batch
                .column(ask_p_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid ask_price column".into()))?;
            let ask_s_arr = batch
                .column(ask_s_idx)
                .as_any()
                .downcast_ref::<UInt32Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid ask_size column".into()))?;
            let bid_p_arr = batch
                .column(bid_p_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid bid_price column".into()))?;
            let bid_s_arr = batch
                .column(bid_s_idx)
                .as_any()
                .downcast_ref::<UInt32Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid bid_size column".into()))?;

            for i in 0..batch.num_rows() {
                let ask = ask_p_arr.value(i);
                let bid = bid_p_arr.value(i);
                let ask_sz = ask_s_arr.value(i);
                let bid_sz = bid_s_arr.value(i);

                // Filter out non-actionable or crossed quotes
                if bid <= 0.0 || ask < bid || (ask_sz == 0 && bid_sz == 0) {
                    continue;
                }

                let nanos = ts_arr.value(i);
                let dt = DateTime::<Utc>::from_timestamp_nanos(nanos);
                let time_str = dt.format("%H:%M:%S").to_string();

                // Apply time filters if requested
                if let Some(ref st) = self.start_time {
                    if &time_str < st && !dt.to_rfc3339().starts_with(st) {
                        continue;
                    }
                }
                if let Some(ref et) = self.end_time {
                    if &time_str > et && !dt.to_rfc3339().starts_with(et) {
                        continue;
                    }
                }

                let mid = (bid + ask) / 2.0;
                let spread_bps = ((ask - bid) / mid) * 10_000.0;
                let total_size = (bid_sz + ask_sz) as f64;
                let imbalance = if total_size > 0.0 {
                    (bid_sz as f64 - ask_sz as f64) / total_size
                } else {
                    0.0
                };

                raw_quotes.push(QuotePoint {
                    timestamp_ns: nanos,
                    timestamp_str: time_str,
                    bid_price: bid,
                    bid_size: bid_sz,
                    ask_price: ask,
                    ask_size: ask_sz,
                    mid_price: mid,
                    spread_bps,
                    imbalance,
                });
            }
        }

        if raw_quotes.is_empty() {
            println!("No matching quotes found in {}", file_path.display());
            return Ok(());
        }

        println!(
            "Loaded {} valid quotes for {}",
            raw_quotes.len(),
            self.symbol
        );

        // Downsample evenly to self.max_points if dataset exceeds threshold
        let quotes: Vec<QuotePoint> = if raw_quotes.len() > self.max_points {
            let stride = (raw_quotes.len() as f64 / self.max_points as f64).ceil() as usize;
            raw_quotes.into_iter().step_by(stride).collect()
        } else {
            raw_quotes
        };

        println!("Rendering {} downsampled quotes to chart", quotes.len());

        let output_path = self.output_file.clone().unwrap_or_else(|| {
            let parent = file_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!("{}_quotes_spread_imbalance.png", self.symbol))
        });

        let root = BitMapBackend::new(&output_path, (1280, 900)).into_drawing_area();
        root.fill(&WHITE)?;

        // Vertical split: Top 48% NBBO, Middle 26% Spread bps, Bottom 26% Imbalance
        let (upper_area, lower_half) = root.split_vertically(430);
        let (mid_area, bottom_area) = lower_half.split_vertically(235);

        let min_bid = quotes
            .iter()
            .map(|q| q.bid_price)
            .fold(f64::INFINITY, f64::min);
        let max_ask = quotes
            .iter()
            .map(|q| q.ask_price)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_spread = quotes.iter().map(|q| q.spread_bps).fold(0.01f64, f64::max);

        let price_margin = ((max_ask - min_bid) * 0.05).max(0.02);
        let price_range = (min_bid - price_margin)..(max_ask + price_margin);
        let x_range = 0usize..quotes.len();

        // 1. Top Panel: NBBO Ask / Bid / Mid
        let mut price_chart = ChartBuilder::on(&upper_area)
            .caption(
                format!("{} - NBBO Quote Dynamics (Ask / Bid / Mid)", self.symbol),
                ("sans-serif", 22).into_font(),
            )
            .margin(10)
            .x_label_area_size(35)
            .y_label_area_size(65)
            .build_cartesian_2d(x_range.clone(), price_range)?;

        price_chart
            .configure_mesh()
            .y_desc("Price ($)")
            .x_labels(10)
            .x_label_formatter(&|idx| {
                quotes
                    .get(*idx)
                    .map(|q| q.timestamp_str.clone())
                    .unwrap_or_default()
            })
            .draw()?;

        // Ask line (Red)
        price_chart
            .draw_series(LineSeries::new(
                quotes.iter().enumerate().map(|(i, q)| (i, q.ask_price)),
                &RGBColor(220, 20, 60),
            ))?
            .label("Ask Price")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(220, 20, 60)));

        // Bid line (Green)
        price_chart
            .draw_series(LineSeries::new(
                quotes.iter().enumerate().map(|(i, q)| (i, q.bid_price)),
                &RGBColor(34, 139, 34),
            ))?
            .label("Bid Price")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(34, 139, 34)));

        // Mid line (Steel Blue)
        price_chart
            .draw_series(LineSeries::new(
                quotes.iter().enumerate().map(|(i, q)| (i, q.mid_price)),
                &RGBColor(70, 130, 180),
            ))?
            .label("Midpoint")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(70, 130, 180)));

        price_chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.85))
            .border_style(&BLACK)
            .draw()?;

        // 2. Middle Panel: Bid-Ask Spread in Basis Points
        let mut spread_chart = ChartBuilder::on(&mid_area)
            .caption(
                format!("{} - Bid-Ask Spread (Basis Points)", self.symbol),
                ("sans-serif", 18).into_font(),
            )
            .margin(10)
            .x_label_area_size(35)
            .y_label_area_size(65)
            .build_cartesian_2d(x_range.clone(), 0.0..(max_spread * 1.15))?;

        spread_chart
            .configure_mesh()
            .y_desc("Spread (bps)")
            .x_labels(10)
            .x_label_formatter(&|idx| {
                quotes
                    .get(*idx)
                    .map(|q| q.timestamp_str.clone())
                    .unwrap_or_default()
            })
            .draw()?;

        spread_chart
            .draw_series(LineSeries::new(
                quotes.iter().enumerate().map(|(i, q)| (i, q.spread_bps)),
                &RGBColor(255, 140, 0),
            ))?
            .label("Spread (bps)")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(255, 140, 0)));

        spread_chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.85))
            .border_style(&BLACK)
            .draw()?;

        // 3. Bottom Panel: Order Flow Imbalance [-1.0, 1.0]
        let mut ofi_chart = ChartBuilder::on(&bottom_area)
            .caption(
                format!("{} - Top-of-Book Depth Imbalance (OFI)", self.symbol),
                ("sans-serif", 18).into_font(),
            )
            .margin(10)
            .x_label_area_size(35)
            .y_label_area_size(65)
            .build_cartesian_2d(x_range, -1.05f64..1.05f64)?;

        ofi_chart
            .configure_mesh()
            .y_desc("Imbalance")
            .x_labels(10)
            .x_label_formatter(&|idx| {
                quotes
                    .get(*idx)
                    .map(|q| q.timestamp_str.clone())
                    .unwrap_or_default()
            })
            .draw()?;

        // Zero reference line
        ofi_chart.draw_series(LineSeries::new(
            vec![(0, 0.0), (quotes.len().saturating_sub(1), 0.0)],
            &BLACK.mix(0.4),
        ))?;

        // Imbalance line / markers
        ofi_chart.draw_series(quotes.iter().enumerate().map(|(i, q)| {
            let color = if q.imbalance >= 0.0 {
                RGBColor(34, 139, 34).filled()
            } else {
                RGBColor(220, 20, 60).filled()
            };
            Rectangle::new([(i, 0.0), (i + 1, q.imbalance)], color)
        }))?;

        root.present()?;
        println!("Saved quotes visualization to {}", output_path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_symbol_is_rejected() {
        let res = PlotQuotes::new(
            "".into(),
            PathBuf::from("dummy.parquet"),
            None,
            None,
            None,
            1000,
        );
        assert!(res.is_err());
    }

    #[test]
    fn non_existent_file_returns_error() {
        let plotter = PlotQuotes::new(
            "SPY".into(),
            PathBuf::from("does_not_exist.parquet"),
            None,
            None,
            None,
            1000,
        )
        .unwrap();
        assert!(plotter.plot_quotes().is_err());
    }
}
