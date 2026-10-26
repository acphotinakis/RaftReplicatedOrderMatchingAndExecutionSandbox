use std::error::Error;
use std::path::PathBuf;

use plotters::prelude::*;

use crate::market::error::MarketError;

pub struct PlotBars {
    pub symbol: String,
    pub input_dir: PathBuf,
    pub data_type: String,
}

impl PlotBars {
    pub fn new(
        symbol: String,
        input_dir: PathBuf,
        data_type: String,
    ) -> Result<Self, Box<dyn Error>> {
        if symbol.is_empty() {
            return Err(MarketError::InvalidConfig(
                "symbol must not be                     
                empty"
                    .into(),
            )
            .into());
        }
        Ok(Self {
            symbol,
            input_dir,
            data_type,
        })
    }

    pub fn plot_bars(&self) -> Result<(), Box<dyn Error>> {
        let root = BitMapBackend::new("chart.png", (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;

        let title = format!("{} Mid-Price Replay", self.symbol);
        let mut chart = ChartBuilder::on(&root)
            .caption(title, ("sans-serif", 30).into_font())
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(0f32..10f32, 145f32..160f32)?;

        chart.configure_mesh().draw()?;

        // Sample series: (time_step, price)
        let data: Vec<(f32, f32)> = vec![
            (0.0, 148.5),
            (1.0, 149.2),
            (2.0, 148.8),
            (3.0, 151.0),
            (4.0, 153.2),
            (5.0, 152.8),
            (6.0, 155.1),
            (7.0, 157.4),
        ];

        chart
            .draw_series(LineSeries::new(data, &RED))?
            .label("Price")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

        chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .draw()?;

        root.present()?;
        println!("Saved chart to chart.png");
        Ok(())
    }
}
