//! Slug generation for meeting records.
//!
//! Format: `<title-slug>-YYYY-MM-DD`. On collision, append `-2`, `-3`, ...
//! Slug uniqueness is enforced by `meetings.slug` UNIQUE constraint.

use chrono::{DateTime, Utc};

/// Build the base slug for a title and start time. The caller resolves
/// collisions with [`disambiguate`] against existing slugs.
pub fn base(title: &str, started_at: DateTime<Utc>) -> String {
    let title_part = if title.trim().is_empty() {
        "meeting".to_string()
    } else {
        slug::slugify(title)
    };
    let date_part = started_at.format("%Y-%m-%d");
    format!("{title_part}-{date_part}")
}

/// Given a base slug and a callable that reports whether a candidate is taken,
/// return the first available variant. Tries the base, then `<base>-2`,
/// `<base>-3`, etc.
pub fn disambiguate<F>(base: &str, mut taken: F) -> String
where
    F: FnMut(&str) -> bool,
{
    if !taken(base) {
        return base.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn date(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 15, 0, 0).unwrap()
    }

    #[test]
    fn base_slug_format() {
        assert_eq!(base("Weekly Staff Sync", date(2026, 4, 29)), "weekly-staff-sync-2026-04-29");
    }

    #[test]
    fn empty_title_falls_back() {
        assert_eq!(base("", date(2026, 4, 29)), "meeting-2026-04-29");
        assert_eq!(base("   ", date(2026, 4, 29)), "meeting-2026-04-29");
    }

    #[test]
    fn collisions_increment() {
        let existing = ["foo-2026-04-29", "foo-2026-04-29-2"];
        let result = disambiguate("foo-2026-04-29", |s| existing.contains(&s));
        assert_eq!(result, "foo-2026-04-29-3");
    }

    #[test]
    fn no_collision_returns_base() {
        assert_eq!(disambiguate("foo", |_| false), "foo");
    }
}
