//! Volume Profile (Volume by Price) and POC visualization for historical trades.

use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};

use arrow::array::{Float64Array, TimestampNanosecondArray, UInt64Array};
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use plotters::prelude::*;

use crate::market::error::MarketError;

/// Individual price bin inside the Volume Profile histogram.
#[derive(Debug, Clone)]
pub struct PriceBin {
    /// Lower price bound of the bin.
    pub price_low: f64,
    /// Upper price bound of the bin.
    pub price_high: f64,
    /// Midpoint price of the bin.
    pub price_mid: f64,
    /// Total cumulative volume executed within this price bin.
    pub volume: u64,
    /// Whether this bin belongs to the 70% Value Area.
    pub is_value_area: bool,
    /// Whether this bin is the Point of Control (POC).
    pub is_poc: bool,
}

/// Generates Volume Profile and Point of Control visualizations from trade Parquet.
pub struct PlotTrades {
    /// Stock symbol (e.g. SPY).
    pub symbol: String,
    /// Path to trades Parquet file.
    pub input_file: PathBuf,
    /// Optional output file path.
    pub output_file: Option<PathBuf>,
    /// Optional start time filter string.
    pub start_time: Option<String>,
    /// Optional end time filter string.
    pub end_time: Option<String>,
    /// Maximum trade samples to draw on the intraday price panel.
    pub max_points: usize,
}

impl PlotTrades {
    /// Creates a new `PlotTrades` instance.
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
            max_points: if max_points == 0 { 4000 } else { max_points },
        })
    }

    /// Reads trades Parquet, computes Volume Profile and POC, and renders the dual-panel chart.
    pub fn plot_trades(&self) -> Result<(), Box<dyn Error>> {
        let file_path = if self.input_file.is_dir() {
            self.input_file
                .join(format!("{}_trades.parquet", self.symbol))
        } else {
            self.input_file.clone()
        };

        if !file_path.exists() {
            return Err(MarketError::InvalidData(format!(
                "trades parquet not found at {}",
                file_path.display()
            ))
            .into());
        }

        println!("Reading trades Parquet from {}", file_path.display());
        let file = File::open(&file_path)?;
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)?.build()?;

        let mut raw_trades: Vec<(i64, String, f64, u64)> = Vec::new();
        let mut min_price = f64::INFINITY;
        let mut max_price = f64::NEG_INFINITY;
        let mut total_volume: u64 = 0;
        let mut total_dollar_vol: f64 = 0.0;

        for batch_res in reader {
            let batch = batch_res?;
            let schema = batch.schema();

            let ts_idx = schema.index_of("timestamp")?;
            let p_idx = schema.index_of("price")?;
            let s_idx = schema.index_of("size")?;

            let ts_arr = batch
                .column(ts_idx)
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .ok_or_else(|| MarketError::InvalidData("invalid timestamp column".into()))?;
            let p_arr = batch
                .column(p_idx)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid price column".into()))?;
            let s_arr = batch
                .column(s_idx)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .ok_or_else(|| MarketError::InvalidData("invalid size column".into()))?;

            for i in 0..batch.num_rows() {
                let p = p_arr.value(i);
                let s = s_arr.value(i);
                if p <= 0.0 || s == 0 {
                    continue;
                }

                let nanos = ts_arr.value(i);
                let dt = DateTime::<Utc>::from_timestamp_nanos(nanos);
                let time_str = dt.format("%H:%M:%S").to_string();

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

                min_price = min_price.min(p);
                max_price = max_price.max(p);
                total_volume += s;
                total_dollar_vol += p * (s as f64);

                raw_trades.push((nanos, time_str, p, s));
            }
        }

        if raw_trades.is_empty() {
            println!("No matching trades found in {}", file_path.display());
            return Ok(());
        }

        let overall_vwap = total_dollar_vol / (total_volume as f64);
        println!(
            "Loaded {} trades for {} (Min: ${:.2}, Max: ${:.2}, Total Vol: {}, VWAP: ${:.2})",
            raw_trades.len(),
            self.symbol,
            min_price,
            max_price,
            total_volume,
            overall_vwap
        );

        // 1. Construct Volume Profile bins (100 bins across price range)
        let num_bins = 100usize;
        let price_span = (max_price - min_price).max(0.10);
        let bin_width = price_span / (num_bins as f64);

        let mut bin_vols = vec![0u64; num_bins];
        for &(_, _, p, s) in &raw_trades {
            let idx = (((p - min_price) / bin_width).floor() as usize).min(num_bins - 1);
            bin_vols[idx] += s;
        }

        // 2. Identify Point of Control (POC)
        let mut poc_idx = 0usize;
        let mut max_bin_vol = 0u64;
        for (i, &vol) in bin_vols.iter().enumerate() {
            if vol > max_bin_vol {
                max_bin_vol = vol;
                poc_idx = i;
            }
        }
        let poc_price = min_price + (poc_idx as f64 + 0.5) * bin_width;

        // 3. Compute 70% Value Area (VAH / VAL) around POC
        let target_va_vol = (total_volume as f64 * 0.70) as u64;
        let mut va_vol = bin_vols[poc_idx];
        let mut va_low_idx = poc_idx;
        let mut va_high_idx = poc_idx;

        while va_vol < target_va_vol && (va_low_idx > 0 || va_high_idx < num_bins - 1) {
            let next_low_vol = if va_low_idx > 0 {
                bin_vols[va_low_idx - 1]
            } else {
                0
            };
            let next_high_vol = if va_high_idx < num_bins - 1 {
                bin_vols[va_high_idx + 1]
            } else {
                0
            };

            if next_low_vol >= next_high_vol && va_low_idx > 0 {
                va_low_idx -= 1;
                va_vol += next_low_vol;
            } else if va_high_idx < num_bins - 1 {
                va_high_idx += 1;
                va_vol += next_high_vol;
            } else if va_low_idx > 0 {
                va_low_idx -= 1;
                va_vol += next_low_vol;
            } else {
                break;
            }
        }

        let val_price = min_price + (va_low_idx as f64) * bin_width;
        let vah_price = min_price + (va_high_idx as f64 + 1.0) * bin_width;

        println!(
            "Volume Profile: POC=${:.2}, VAH=${:.2}, VAL=${:.2}",
            poc_price, vah_price, val_price
        );

        let bins: Vec<PriceBin> = bin_vols
            .iter()
            .enumerate()
            .map(|(i, &vol)| {
                let p_low = min_price + (i as f64) * bin_width;
                let p_high = p_low + bin_width;
                PriceBin {
                    price_low: p_low,
                    price_high: p_high,
                    price_mid: (p_low + p_high) / 2.0,
                    volume: vol,
                    is_value_area: i >= va_low_idx && i <= va_high_idx,
                    is_poc: i == poc_idx,
                }
            })
            .collect();

        // 4. Downsample trades for intraday price line
        let sampled_trades: Vec<(String, f64, f64)> = {
            let stride = (raw_trades.len() as f64 / self.max_points as f64).ceil() as usize;
            let mut sampled = Vec::new();
            let mut cum_vol = 0u64;
            let mut cum_dlr = 0.0f64;

            for (i, (_, t_str, p, s)) in raw_trades.iter().enumerate() {
                cum_vol += s;
                cum_dlr += p * (*s as f64);
                if i % stride == 0 || i == raw_trades.len() - 1 {
                    let running_vwap = if cum_vol > 0 {
                        cum_dlr / cum_vol as f64
                    } else {
                        *p
                    };
                    sampled.push((t_str.clone(), *p, running_vwap));
                }
            }
            sampled
        };

        let output_path = self.output_file.clone().unwrap_or_else(|| {
            let parent = file_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!("{}_volume_profile.png", self.symbol))
        });

        let root = BitMapBackend::new(&output_path, (1400, 800)).into_drawing_area();
        root.fill(&WHITE)?;

        // Horizontal split: Left 70% Price Evolution, Right 30% Volume Profile
        let (left_area, right_area) = root.split_horizontally(960);

        let price_margin = (price_span * 0.05).max(0.05);
        let shared_price_range = (min_price - price_margin)..(max_price + price_margin);
        let time_range = 0usize..sampled_trades.len();

        // 1. Left Panel: Price time series with VWAP and POC / VA levels
        let mut price_chart = ChartBuilder::on(&left_area)
            .caption(
                format!(
                    "{} - Intraday Price & VWAP (POC: ${:.2}, VAH: ${:.2}, VAL: ${:.2})",
                    self.symbol, poc_price, vah_price, val_price
                ),
                ("sans-serif", 20).into_font(),
            )
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(65)
            .build_cartesian_2d(time_range.clone(), shared_price_range.clone())?;

        price_chart
            .configure_mesh()
            .y_desc("Price ($)")
            .x_labels(10)
            .x_label_formatter(&|idx| {
                sampled_trades
                    .get(*idx)
                    .map(|(t, _, _)| t.clone())
                    .unwrap_or_default()
            })
            .draw()?;

        // Trade Price Series (Navy)
        price_chart
            .draw_series(LineSeries::new(
                sampled_trades
                    .iter()
                    .enumerate()
                    .map(|(i, (_, p, _))| (i, *p)),
                &RGBColor(0, 51, 102),
            ))?
            .label("Trade Price")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(0, 51, 102)));

        // VWAP Series (Amber)
        price_chart
            .draw_series(LineSeries::new(
                sampled_trades
                    .iter()
                    .enumerate()
                    .map(|(i, (_, _, v))| (i, *v)),
                &RGBColor(255, 140, 0),
            ))?
            .label("VWAP")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(255, 140, 0)));

        // POC horizontal line (Red)
        price_chart
            .draw_series(LineSeries::new(
                vec![
                    (0, poc_price),
                    (sampled_trades.len().saturating_sub(1), poc_price),
                ],
                &RGBColor(220, 20, 60),
            ))?
            .label("POC")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(220, 20, 60)));

        // VAH horizontal line (Steel Blue)
        price_chart
            .draw_series(LineSeries::new(
                vec![
                    (0, vah_price),
                    (sampled_trades.len().saturating_sub(1), vah_price),
                ],
                &RGBColor(70, 130, 180),
            ))?
            .label("VAH / VAL (70%)")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RGBColor(70, 130, 180)));

        // VAL horizontal line (Steel Blue)
        price_chart.draw_series(LineSeries::new(
            vec![
                (0, val_price),
                (sampled_trades.len().saturating_sub(1), val_price),
            ],
            &RGBColor(70, 130, 180),
        ))?;

        price_chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.85))
            .border_style(&BLACK)
            .draw()?;

        // 2. Right Panel: Horizontal Volume Profile
        let max_vol_axis = (max_bin_vol as f64 * 1.15) as u64;
        let mut vp_chart = ChartBuilder::on(&right_area)
            .caption("Volume Profile", ("sans-serif", 20).into_font())
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(0u64..max_vol_axis, shared_price_range)?;

        vp_chart
            .configure_mesh()
            .x_desc("Volume")
            .x_labels(4)
            .draw()?;

        // Draw horizontal bars for each price bin
        for bin in &bins {
            let color = if bin.is_poc {
                RGBColor(220, 20, 60).filled() // Red for POC
            } else if bin.is_value_area {
                RGBColor(65, 105, 225).filled() // Royal blue for Value Area
            } else {
                RGBColor(176, 196, 222).filled() // Light steel blue for outside VA
            };

            vp_chart.draw_series(std::iter::once(Rectangle::new(
                [(0, bin.price_low), (bin.volume, bin.price_high)],
                color,
            )))?;
        }

        // Horizontal POC line indicator on profile
        vp_chart.draw_series(LineSeries::new(
            vec![(0, poc_price), (max_vol_axis, poc_price)],
            &RGBColor(220, 20, 60),
        ))?;

        root.present()?;
        println!(
            "Saved volume profile visualization to {}",
            output_path.display()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_symbol_is_rejected() {
        let res = PlotTrades::new(
            "".into(),
            PathBuf::from("trades.parquet"),
            None,
            None,
            None,
            1000,
        );
        assert!(res.is_err());
    }

    #[test]
    fn non_existent_file_returns_error() {
        let plotter = PlotTrades::new(
            "SPY".into(),
            PathBuf::from("does_not_exist.parquet"),
            None,
            None,
            None,
            1000,
        )
        .unwrap();
        assert!(plotter.plot_trades().is_err());
    }
}
