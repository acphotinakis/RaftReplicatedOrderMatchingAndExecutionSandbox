use clap::Parser;

/// CLI tool to fetch historical stock bars from Alpaca Data API v2.
#[derive(Parser, Debug)]
#[command(name = "alpaca-bars", author, version, about)]
pub struct CliArgs {
    /// Comma-separated list of stock symbols (e.g., SPY,AAPL)
    #[arg(short, long, default_value = "SPY")]
    pub symbols: String,

    /// Timeframe for the bars (e.g., 1Min, 5Min, 1Hour, 1Day)
    #[arg(short, long, default_value = "1Min")]
    pub timeframe: String,

    /// Filter bars on or after this timestamp (ISO-8601 / RFC-3339)
    #[arg(long, default_value = "2024-01-03T09:30:00-04:00")]
    pub start: String,

    /// Filter bars on or before this timestamp (ISO-8601 / RFC-3339)
    #[arg(long, default_value = "2024-01-04T09:30:00-04:00")]
    pub end: String,

    /// Maximum number of bars to return (1-10000)
    #[arg(short, long, default_value_t = 10)]
    pub limit: u32,

    /// Corporate action adjustment (raw, split, dividend, all)
    #[arg(short, long, default_value = "raw")]
    pub adjustment: String,

    /// Data feed source (sip, iex, otc)
    #[arg(short, long, default_value = "sip")]
    pub feed: String,

    /// Chronological sort order (asc, desc)
    #[arg(long, default_value = "asc")]
    pub sort: String,

    /// Pagination page token (optional)
    #[arg(long)]
    pub page_token: Option<String>,
}
