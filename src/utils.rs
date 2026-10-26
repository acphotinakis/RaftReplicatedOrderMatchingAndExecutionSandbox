use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, ParseError, TimeZone, Utc};

/// Parses an ISO / RFC-3339 datetime string.
///
/// If an explicit timezone/offset is present (e.g. `Z` or `+02:00`), it parses it
/// and normalizes to UTC.
/// If no timezone offset is provided (naive), it assumes UTC (matching `tzinfo=timezone.utc`).
pub fn parse_iso_datetime(value: &str) -> Result<DateTime<Utc>, ParseError> {
    // 1. Attempt parsing with an explicit offset / RFC 3339 format
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }

    // 2. Fall back to parsing naive ISO 8601/RFC 3339 without offset, then assume UTC
    // Handles formats like "2024-01-03T10:30:00" or "2024-01-03 10:30:00"
    let naive = value.parse::<NaiveDateTime>()?;
    Ok(Utc.from_utc_datetime(&naive))
}

/// Formats a datetime as an RFC-3339 UTC string ending in "Z".
pub fn rfc3339<Tz: TimeZone>(value: &DateTime<Tz>) -> String {
    // Convert to UTC and format with 'Z' instead of '+00:00'
    value
        .with_timezone(&Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

/// Returns the end of day timestamp in RFC-3339 format with nanosecond precision.
pub fn end_of_day_rfc3339(day: NaiveDate) -> String {
    format!("{}T23:59:59.999999999Z", day.format("%Y-%m-%d"))
}

/// Subtracts five calendar years, safely handling Feb. 29.
pub fn five_years_ago(value: DateTime<Utc>) -> DateTime<Utc> {
    let target_year = value.year() - 5;
    let month = value.month();
    let day = if month == 2 && value.day() == 29 {
        28
    } else {
        value.day()
    };

    let naive_date = NaiveDate::from_ymd_opt(target_year, month, day).unwrap();
    let naive_time = value.time();
    let naive_dt = NaiveDateTime::new(naive_date, naive_time);

    Utc.from_utc_datetime(&naive_dt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_iso_datetime() {
        // With explicit Z
        let dt1 = parse_iso_datetime("2024-01-03T09:30:00Z").unwrap();
        assert_eq!(rfc3339(&dt1), "2024-01-03T09:30:00Z");

        // With offset
        let dt2 = parse_iso_datetime("2024-01-03T13:30:00+04:00").unwrap();
        assert_eq!(rfc3339(&dt2), "2024-01-03T09:30:00Z");

        // Without timezone (assumed UTC)
        let dt3 = parse_iso_datetime("2024-01-03T09:30:00").unwrap();
        assert_eq!(rfc3339(&dt3), "2024-01-03T09:30:00Z");
    }

    #[test]
    fn test_end_of_day() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        assert_eq!(end_of_day_rfc3339(date), "2024-01-03T23:59:59.999999999Z");
    }
}
