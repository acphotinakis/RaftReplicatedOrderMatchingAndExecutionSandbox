//! UTC daily windows preserving the exact requested endpoints.

use crate::market::error::MarketError;
use chrono::{DateTime, Days, NaiveDate, SecondsFormat, Utc};

/// One inclusive provider query window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadWindow {
    /// Inclusive UTC start in RFC3339 form.
    pub start: String,
    /// Inclusive UTC end in RFC3339 form.
    pub end: String,
    /// UTC partition date.
    pub day: NaiveDate,
}

/// Split a nonempty requested interval into nonoverlapping UTC days.
pub fn daily_windows(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<DownloadWindow>, MarketError> {
    if start >= end {
        return Err(MarketError::InvalidConfig("start must precede end".into()));
    }
    let mut day = start.date_naive();
    let mut windows = Vec::new();
    loop {
        windows.push(DownloadWindow {
            start: if day == start.date_naive() {
                start.to_rfc3339_opts(SecondsFormat::AutoSi, true)
            } else {
                format!("{day}T00:00:00Z")
            },
            end: if day == end.date_naive() {
                end.to_rfc3339_opts(SecondsFormat::AutoSi, true)
            } else {
                format!("{day}T23:59:59.999999999Z")
            },
            day,
        });
        if day == end.date_naive() {
            break;
        }
        day = day
            .checked_add_days(Days::new(1))
            .ok_or_else(|| MarketError::InvalidConfig("date range overflow".into()))?;
    }
    Ok(windows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_days_preserve_bounds_without_overlap() {
        let start = "2024-01-01T12:30:00Z".parse().unwrap();
        let end = "2024-01-02T10:00:00Z".parse().unwrap();
        let windows = daily_windows(start, end).unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].start, "2024-01-01T12:30:00Z");
        assert_eq!(windows[0].end, "2024-01-01T23:59:59.999999999Z");
        assert_eq!(windows[1].start, "2024-01-02T00:00:00Z");
        assert_eq!(windows[1].end, "2024-01-02T10:00:00Z");
    }

    #[test]
    fn empty_range_is_rejected() {
        let instant = "2024-01-01T00:00:00Z".parse().unwrap();
        assert!(daily_windows(instant, instant).is_err());
    }
}
