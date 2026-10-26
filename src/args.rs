// In src/args.rs
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "rromes", about = "Raft Replicated Sandbox Market Data CLI")]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: MarketCommand,
}

#[derive(Debug, Subcommand)]
pub enum MarketCommand {
    List,
    /// Fetch historical aggregate stock bars
    Bars {
        /// Comma-separated ticker symbols (e.g., AAPL,MSFT)
        #[arg(short, long, default_value = "SPY")]
        symbols: String,

        /// Bar timeframe (e.g., 1Min, 5Min, 1Hour, 1Day)
        #[arg(short, long, default_value = "1Min")]
        timeframe: String,

        /// Start timestamp in RFC-3339 format
        #[arg(long, default_value = "2026-01-01")]
        start: Option<String>,

        /// End timestamp in RFC-3339 format
        #[arg(long, default_value = "2026-10-19")]
        end: Option<String>,

        /// Maximum data points to return
        #[arg(short, long, default_value_t = 10)]
        limit: u32,

        /// Data feed (sip, iex, otc)
        #[arg(long, default_value = "sip")]
        feed: String,

        /// Sort direction (asc, desc)
        #[arg(long, default_value = "asc")]
        sort: String,

        /// Optional directory to save output as Parquet
        #[arg(long)]
        parquet_output_dir: Option<PathBuf>,
    },

    // /// Fetch historical top-of-book stock quotes
    // Quotes {
    //     /// Comma-separated ticker symbols (e.g., AAPL,MSFT)
    //     #[arg(short, long, default_value = "AAPL,MSFT")]
    //     symbols: String,

    //     /// Start timestamp in RFC-3339 format
    //     #[arg(long)]
    //     start: Option<String>,

    //     /// End timestamp in RFC-3339 format
    //     #[arg(long)]
    //     end: Option<String>,

    //     /// Maximum data points to return
    //     #[arg(short, long, default_value_t = 10)]
    //     limit: u32,

    //     /// Data feed (sip, iex, otc)
    //     #[arg(long, default_value = "sip")]
    //     feed: String,

    //     /// Sort direction (asc, desc)
    //     #[arg(long, default_value = "asc")]
    //     sort: String,
    // },
    /// Backfill and download historical trade data into CSV or partitioned Parquet
    Trades {
        /// Comma-separated symbols (e.g. SPY,AAPL)
        #[arg(long, default_value = "SPY")]
        symbols: String,

        /// Start timestamp in RFC-3339 format (defaults to 5 years ago if omitted)
        #[arg(long)]
        start: Option<String>,

        /// End timestamp in RFC-3339 format (defaults to current time if omitted)
        #[arg(long)]
        end: Option<String>,

        /// Stock feed (sip, iex, otc, boats)
        #[arg(long, default_value = "sip")]
        feed: String,

        /// Directory for CSV output
        #[arg(long, default_value = "data/raw/trades")]
        output_dir: PathBuf,

        /// Optional directory to partition into Parquet instead of monolithic CSV
        #[arg(long)]
        parquet_output_dir: Option<PathBuf>,

        /// Trades per page (1-10000)
        #[arg(long, default_value_t = 10000)]
        limit: usize,

        /// Requests per minute throttle
        #[arg(long, default_value_t = 150)]
        requests_per_minute: u32,
    },

    Quotes {
        /// Comma-separated symbols. Defaults to ALPACA_SYMBOLS or standard list.
        #[arg(long, env = "ALPACA_SYMBOLS")]
        symbols: Option<String>,

        /// RFC-3339 start timestamp. Default: exactly five calendar years ago.
        #[arg(long)]
        start: Option<String>,

        /// RFC-3339 end timestamp. Default: current UTC time.
        #[arg(long)]
        end: Option<String>,

        /// Historical stock quote feed.
        #[arg(long, default_value = "sip", value_parser = ["sip", "iex", "boats", "otc"])]
        feed: String,

        /// ISO-4217 currency.
        #[arg(long, default_value = "USD")]
        currency: String,

        /// Optional Alpaca as-of date (YYYY-MM-DD) or "-" to disable mapping.
        #[arg(long, env = "ALPACA_ASOF")]
        asof: Option<String>,

        /// Directory for historical quote CSV output.
        #[arg(
            long,
            default_value = "data/raw/quotes",
            env = "HISTORICAL_QUOTES_OUTPUT_DIR"
        )]
        output_dir: PathBuf,

        /// Write atomic date-partitioned Parquet files here instead of monolithic CSV.
        #[arg(long)]
        parquet_output_dir: Option<PathBuf>,

        /// Quotes requested per page (1-10000).
        #[arg(long, default_value_t = 10000)]
        limit: usize,

        /// Proactive API throttle. Default: 150 requests/minute.
        #[arg(long, default_value_t = 150, env = "ALPACA_REQUESTS_PER_MINUTE")]
        requests_per_minute: u32,
    },
}
