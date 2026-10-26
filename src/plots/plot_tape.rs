//! Tape-walk execution overlay fusing historical quotes and trades.

use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};

use arrow::array::{Float64Array, TimestampNanosecondArray, UInt64Array};
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use plotters::prelude::*;

use crate::market::error::MarketError;

/// Classification of a trade execution against prevailing NBBO quotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionSide {
    /// Trade executed at or above the prevailing Ask price (buyer aggressor).
    AggressorBuy,
    /// Trade executed at or below the prevailing Bid price (seller aggressor).
    AggressorSell,
    /// Trade executed inside the spread / midpoint (price improvement / dark pool).
    InsideSpread,
}

/// Compact representation of a quote tick.
#[derive(Debug, Clone)]
struct QuoteTick {
    timestamp_ns: i64,
    bid_price: f64,
    ask_price: f64,
}

/// Compact representation of a trade tick.
#[derive(Debug, Clone)]
struct TradeTick {
    timestamp_ns: i64,
    time_str: String,
    price: f64,
    size: u64,
    side: ExecutionSide,
}

/// Generates a high-resolution tape-walk execution overlay chart.
pub struct PlotTape {
    /// Stock ticker symbol.
    pub symbol: String,
    /// Path to quotes Parquet file.
    pub quotes_file: PathBuf,
    /// Path to trades Parquet file (optional, auto-inferred if omitted).
    pub trades_file: Option<PathBuf>,
    /// Optional output PNG file path.
    pub output_file: Option<PathBuf>,
    /// Optional start time filter string (e.g. "09:30:00" or ISO timestamp).
    pub start_time: Option<String>,
    /// Optional end time filter string (e.g. "09:35:00" or ISO timestamp).
    pub end_time: Option<String>,
    /// Maximum number of trades to display on the tape-walk window.
    pub max_points: usize,
}

impl PlotTape {
    /// Creates a new `PlotTape` instance.
    pub fn new(
        symbol: String,
        quotes_file: PathBuf,
        trades_file: Option<PathBuf>,
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
            quotes_file,
            trades_file,
            output_file,
            start_time,
            end_time,
            max_points: if max_points == 0 { 2000 } else { max_points },
        })
    }

    /// Reads quotes and trades, performs NBBO matching, and renders the tape-walk overlay.
    pub fn plot_tape(&self) -> Result<(), Box<dyn Error>> {
        let quotes_path = if self.quotes_file.is_dir() {
            self.quotes_file
                .join(format!("{}_quotes.parquet", self.symbol))
        } else {
            self.quotes_file.clone()
        };

        let trades_path = match &self.trades_file {
            Some(p) if p.is_dir() => p.join(format!("{}_trades.parquet", self.symbol)),
            Some(p) => p.clone(),
            None => {
                // Infer trades file from quotes path
                let path_str = quotes_path.to_string_lossy();
                if path_str.contains("quotes") {
                    PathBuf::from(path_str.replace("quotes", "trades"))
                } else {
                    return Err(MarketError::InvalidConfig(
                        "trades_file must be specified if not inferrable from quotes_file".into(),
                    )
                    .into());
                }
            }
        };

        if !quotes_path.exists() {
            return Err(MarketError::InvalidData(format!(
                "quotes parquet not found at {}",
                quotes_path.display()
            ))
            .into());
        }
        if !trades_path.exists() {
            return Err(MarketError::InvalidData(format!(
                "trades parquet not found at {}",
                trades_path.display()
            ))
            .into());
        }

        println!("Reading trades from {}", trades_path.display());
        let trade_file = File::open(&trades_path)?;
        let trade_reader = ParquetRecordBatchReaderBuilder::try_new(trade_file)?.build()?;

        let mut all_trades: Vec<(i64, String, f64, u64)> = Vec::new();

        for batch_res in trade_reader {
            let batch = batch_res?;
            let schema = batch.schema();

            let ts_idx = schema.index_of("timestamp")?;
            let p_idx = schema.index_of("price")?;
            let s_idx = schema.index_of("size")?;

            let ts_arr = batch
                .column(ts_idx)
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .ok_or_else(|| {
                    MarketError::InvalidData("invalid timestamp column in trades".into())
                })?;
            let p_arr = batch
                .column(p_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid price column in trades".into()))?;
            let s_arr = batch
                .column(s_idx)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid size column in trades".into()))?;

            for i in 0..batch.num_rows() {
                let p = p_arr.value(i);
                let s = s_arr.value(i);
                if p <= 0.0 || s == 0 {
                    continue;
                }
                let nanos = ts_arr.value(i);
                let dt = DateTime::<Utc>::from_timestamp_nanos(nanos);
                let time_str = dt.format("%H:%M:%S%.3f").to_string();

                if let Some(ref st) = self.start_time {
                    if &time_str[..8] < st.as_str() && !dt.to_rfc3339().starts_with(st) {
                        continue;
                    }
                }
                if let Some(ref et) = self.end_time {
                    if &time_str[..8] > et.as_str() && !dt.to_rfc3339().starts_with(et) {
                        continue;
                    }
                }

                all_trades.push((nanos, time_str, p, s));
            }
        }

        if all_trades.is_empty() {
            println!("No matching trades found in {}", trades_path.display());
            return Ok(());
        }

        // If no explicit time filter was provided and trades span entire day,
        // prioritize market open (09:30:00) if present, or take first max_points
        let selected_raw_trades: Vec<(i64, String, f64, u64)> = if self.start_time.is_none()
            && self.end_time.is_none()
            && all_trades.len() > self.max_points
        {
            if let Some(open_idx) = all_trades
                .iter()
                .position(|(_, t, _, _)| &t[..8] >= "09:30:00")
            {
                let end_idx = (open_idx + self.max_points).min(all_trades.len());
                all_trades[open_idx..end_idx].to_vec()
            } else {
                all_trades[..self.max_points.min(all_trades.len())].to_vec()
            }
        } else if all_trades.len() > self.max_points {
            let stride = (all_trades.len() as f64 / self.max_points as f64).ceil() as usize;
            all_trades.into_iter().step_by(stride).collect()
        } else {
            all_trades
        };

        let min_trade_ts = selected_raw_trades.first().map(|t| t.0).unwrap_or(0);
        let max_trade_ts = selected_raw_trades.last().map(|t| t.0).unwrap_or(0);

        println!(
            "Selected {} trades for tape-walk window ({} to {})",
            selected_raw_trades.len(),
            selected_raw_trades.first().map(|t| &t.1[..8]).unwrap_or(""),
            selected_raw_trades.last().map(|t| &t.1[..8]).unwrap_or("")
        );

        // Read quotes overlapping this window (plus 1 second buffer before and after)
        let buffer_ns = 1_000_000_000i64; // 1 second
        let window_start_ns = min_trade_ts - buffer_ns;
        let window_end_ns = max_trade_ts + buffer_ns;

        println!("Reading matching quotes from {}", quotes_path.display());
        let quote_file = File::open(&quotes_path)?;
        let quote_reader = ParquetRecordBatchReaderBuilder::try_new(quote_file)?.build()?;

        let mut quotes: Vec<QuoteTick> = Vec::new();

        for batch_res in quote_reader {
            let batch = batch_res?;
            let schema = batch.schema();

            let ts_idx = schema.index_of("timestamp")?;
            let ask_idx = schema.index_of("ask_price")?;
            let bid_idx = schema.index_of("bid_price")?;

            let ts_arr = batch
                .column(ts_idx)
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .ok_or_else(|| {
                    MarketError::InvalidData("invalid timestamp column in quotes".into())
                })?;
            let ask_arr = batch
                .column(ask_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| {
                    MarketError::InvalidData("invalid ask_price column in quotes".into())
                })?;
            let bid_arr = batch
                .column(bid_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| {
                    MarketError::InvalidData("invalid bid_price column in quotes".into())
                })?;

            for i in 0..batch.num_rows() {
                let nanos = ts_arr.value(i);
                if nanos < window_start_ns || nanos > window_end_ns {
                    continue;
                }
                let ask = ask_arr.value(i);
                let bid = bid_arr.value(i);
                if bid > 0.0 && ask >= bid {
                    quotes.push(QuoteTick {
                        timestamp_ns: nanos,
                        bid_price: bid,
                        ask_price: ask,
                    });
                }
            }
        }

        quotes.sort_by_key(|q| q.timestamp_ns);

        if quotes.is_empty() {
            println!("Warning: No quotes overlapped trade window. Using dummy quotes.");
            let min_p = selected_raw_trades
                .iter()
                .map(|t| t.2)
                .fold(f64::INFINITY, f64::min);
            let max_p = selected_raw_trades
                .iter()
                .map(|t| t.2)
                .fold(f64::NEG_INFINITY, f64::max);
            quotes.push(QuoteTick {
                timestamp_ns: window_start_ns,
                bid_price: min_p,
                ask_price: max_p,
            });
        }

        // Match trades against prevailing quotes
        let mut trades: Vec<TradeTick> = Vec::with_capacity(selected_raw_trades.len());
        for (nanos, time_str, p, s) in selected_raw_trades {
            // Binary search latest quote <= nanos
            let quote_idx = match quotes.binary_search_by_key(&nanos, |q| q.timestamp_ns) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };
            let prevailing = &quotes[quote_idx.min(quotes.len() - 1)];

            let side = if p >= prevailing.ask_price - 1e-4 {
                ExecutionSide::AggressorBuy
            } else if p <= prevailing.bid_price + 1e-4 {
                ExecutionSide::AggressorSell
            } else {
                ExecutionSide::InsideSpread
            };

            trades.push(TradeTick {
                timestamp_ns: nanos,
                time_str,
                price: p,
                size: s,
                side,
            });
        }

        let output_path = self.output_file.clone().unwrap_or_else(|| {
            let parent = quotes_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!("{}_tape_walk.png", self.symbol))
        });

        let root = BitMapBackend::new(&output_path, (1280, 850)).into_drawing_area();
        root.fill(&WHITE)?;

        // Top 70% Price & Tape-walk, Bottom 30% Trade Size
        let (upper_area, lower_area) = root.split_vertically(580);

        let min_price = quotes
            .iter()
            .map(|q| q.bid_price)
            .chain(trades.iter().map(|t| t.price))
            .fold(f64::INFINITY, f64::min);
        let max_price = quotes
            .iter()
            .map(|q| q.ask_price)
            .chain(trades.iter().map(|t| t.price))
            .fold(f64::NEG_INFINITY, f64::max);
        let max_size = trades.iter().map(|t| t.size).max().unwrap_or(1);

        let margin = ((max_price - min_price) * 0.05).max(0.01);
        let price_range = (min_price - margin)..(max_price + margin);
        let x_range = 0usize..trades.len();

        // 1. Upper Panel: Step quotes + Trade bubbles
        let mut tape_chart = ChartBuilder::on(&upper_area)
            .caption(
                format!("{} - Tape-Walk Execution Overlay", self.symbol),
                ("sans-serif", 22).into_font(),
            )
            .margin(10)
            .x_label_area_size(35)
            .y_label_area_size(65)
            .build_cartesian_2d(x_range.clone(), price_range)?;

        tape_chart
            .configure_mesh()
            .y_desc("Price ($)")
            .x_labels(10)
            .x_label_formatter(&|idx| {
                trades
                    .get(*idx)
                    .map(|t| t.time_str[..8].to_string())
                    .unwrap_or_default()
            })
            .draw()?;

        // Resample quote step curve across the trade index timeline
        let mut ask_curve: Vec<(usize, f64)> = Vec::with_capacity(trades.len());
        let mut bid_curve: Vec<(usize, f64)> = Vec::with_capacity(trades.len());

        for (i, t) in trades.iter().enumerate() {
            let q_idx = match quotes.binary_search_by_key(&t.timestamp_ns, |q| q.timestamp_ns) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };
            let q = &quotes[q_idx.min(quotes.len() - 1)];
            ask_curve.push((i, q.ask_price));
            bid_curve.push((i, q.bid_price));
        }

        // Ask Quote Step Line (Red)
        tape_chart
            .draw_series(LineSeries::new(ask_curve, &RGBColor(220, 20, 60)))?
            .label("Ask Quote")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(220, 20, 60)));

        // Bid Quote Step Line (Green)
        tape_chart
            .draw_series(LineSeries::new(bid_curve, &RGBColor(34, 139, 34)))?
            .label("Bid Quote")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(34, 139, 34)));

        // Trade Execution Points (Circles sized by trade size)
        tape_chart.draw_series(trades.iter().enumerate().map(|(i, t)| {
            let radius = ((t.size as f64).sqrt() / 15.0).clamp(3.0, 10.0) as i32;
            let color = match t.side {
                ExecutionSide::AggressorBuy => RGBColor(0, 180, 0),
                ExecutionSide::AggressorSell => RGBColor(220, 20, 60),
                ExecutionSide::InsideSpread => RGBColor(255, 165, 0),
            };
            Circle::new((i, t.price), radius, color.filled())
        }))?;

        // Legend elements for trades
        tape_chart
            .draw_series(LineSeries::new(vec![], &RGBColor(0, 180, 0)))?
            .label("Aggressor Buy (>= Ask)")
            .legend(|(x, y)| Circle::new((x + 10, y), 5, RGBColor(0, 180, 0).filled()));

        tape_chart
            .draw_series(LineSeries::new(vec![], &RGBColor(220, 20, 60)))?
            .label("Aggressor Sell (<= Bid)")
            .legend(|(x, y)| Circle::new((x + 10, y), 5, RGBColor(220, 20, 60).filled()));

        tape_chart
            .draw_series(LineSeries::new(vec![], &RGBColor(255, 165, 0)))?
            .label("Inside Spread / Midpoint")
            .legend(|(x, y)| Circle::new((x + 10, y), 5, RGBColor(255, 165, 0).filled()));

        tape_chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.85))
            .border_style(&BLACK)
            .draw()?;

        // 2. Lower Panel: Execution Volume Bars
        let mut vol_chart = ChartBuilder::on(&lower_area)
            .caption(
                format!("{} - Execution Size by Aggressor Side", self.symbol),
                ("sans-serif", 18).into_font(),
            )
            .margin(10)
            .x_label_area_size(35)
            .y_label_area_size(65)
            .build_cartesian_2d(x_range, 0u64..(max_size + (max_size / 10).max(1)))?;

        vol_chart
            .configure_mesh()
            .y_desc("Size")
            .x_labels(10)
            .x_label_formatter(&|idx| {
                trades
                    .get(*idx)
                    .map(|t| t.time_str[..8].to_string())
                    .unwrap_or_default()
            })
            .draw()?;

        vol_chart.draw_series(trades.iter().enumerate().map(|(i, t)| {
            let color = match t.side {
                ExecutionSide::AggressorBuy => RGBColor(0, 180, 0).filled(),
                ExecutionSide::AggressorSell => RGBColor(220, 20, 60).filled(),
                ExecutionSide::InsideSpread => RGBColor(255, 165, 0).filled(),
            };
            Rectangle::new([(i, 0), (i + 1, t.size)], color)
        }))?;

        root.present()?;
        println!("Saved tape-walk visualization to {}", output_path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_symbol_is_rejected() {
        let res = PlotTape::new(
            "".into(),
            PathBuf::from("quotes.parquet"),
            Some(PathBuf::from("trades.parquet")),
            None,
            None,
            None,
            1000,
        );
        assert!(res.is_err());
    }

    #[test]
    fn non_existent_file_returns_error() {
        let plotter = PlotTape::new(
            "SPY".into(),
            PathBuf::from("missing_quotes.parquet"),
            Some(PathBuf::from("missing_trades.parquet")),
            None,
            None,
            None,
            1000,
        )
        .unwrap();
        assert!(plotter.plot_tape().is_err());
    }
}
