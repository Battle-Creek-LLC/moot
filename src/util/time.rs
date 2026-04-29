//! Time helpers — duration parsing for `--since`/`--older-than`, epoch
//! conversion for SQLite, etc.

use chrono::{DateTime, Duration, Utc};

use crate::error::{Error, Result};

/// Current time in unix epoch milliseconds.
pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Convert epoch ms to a UTC `DateTime`.
pub fn from_ms(ms: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_else(Utc::now)
}

/// Parse a relative duration like `7d`, `2w`, `1mo`, `90d`, `48h`, `30m`.
/// Months are treated as 30 days, weeks as 7 days. ISO-8601 durations
/// (`PT24H`, `P7D`, ...) are not yet supported — leaves a TODO if we need
/// them later.
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return Err(Error::Cli("duration cannot be empty".into()));
    }

    // Split into numeric prefix and unit suffix.
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num_str, unit) = s.split_at(split);
    if num_str.is_empty() {
        return Err(Error::Cli(format!("duration `{s}` missing leading number")));
    }
    let n: i64 = num_str
        .parse()
        .map_err(|_| Error::Cli(format!("duration `{s}` has invalid number")))?;

    let dur = match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => Duration::seconds(n),
        "m" | "min" | "mins" | "minute" | "minutes" => Duration::minutes(n),
        "h" | "hr" | "hrs" | "hour" | "hours" => Duration::hours(n),
        "d" | "day" | "days" => Duration::days(n),
        "w" | "wk" | "wks" | "week" | "weeks" => Duration::days(n * 7),
        "mo" | "month" | "months" => Duration::days(n * 30),
        "y" | "yr" | "yrs" | "year" | "years" => Duration::days(n * 365),
        other => {
            return Err(Error::Cli(format!(
                "duration `{s}` has unknown unit `{other}` (try s/m/h/d/w/mo/y)"
            )));
        }
    };
    Ok(dur)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_units() {
        assert_eq!(parse_duration("7d").unwrap(), Duration::days(7));
        assert_eq!(parse_duration("2w").unwrap(), Duration::days(14));
        assert_eq!(parse_duration("1mo").unwrap(), Duration::days(30));
        assert_eq!(parse_duration("48h").unwrap(), Duration::hours(48));
        assert_eq!(parse_duration("90s").unwrap(), Duration::seconds(90));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("d").is_err());
        assert!(parse_duration("7x").is_err());
        assert!(parse_duration("seven days").is_err());
    }
}
