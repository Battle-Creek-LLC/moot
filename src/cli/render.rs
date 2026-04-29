//! Shared output formatting for `show`, `list`, `search`.
//!
//! Keeps human + JSON renderings of `Meeting` rows in one place so format
//! drift stays out of the per-command files.

use chrono::DateTime;
use serde_json::json;

use crate::store::Meeting;

/// `mm:ss` if under an hour; `Hh Mm` for longer.
pub fn format_duration(secs: Option<i64>) -> String {
    let Some(secs) = secs else { return "—".into() };
    if secs < 60 {
        return format!("{secs}s");
    }
    let m = secs / 60;
    let s = secs % 60;
    if m < 60 {
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m{s}s")
        }
    } else {
        let h = m / 60;
        let m = m % 60;
        format!("{h}h{m:02}m")
    }
}

pub fn format_started(ms: Option<i64>) -> String {
    match ms {
        Some(ms) => DateTime::<chrono::Utc>::from_timestamp_millis(ms)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "—".into()),
        None => "—".into(),
    }
}

pub fn meeting_to_json(meeting: &Meeting, tags: &[String]) -> serde_json::Value {
    let participants = meeting
        .participants_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .unwrap_or(serde_json::Value::Array(vec![]));
    json!({
        "id": meeting.id,
        "slug": meeting.slug,
        "title": meeting.title,
        "platform": meeting.platform,
        "url": meeting.url,
        "recall_bot_id": meeting.recall_bot_id,
        "language": meeting.language,
        "started_at": meeting.started_at.map(to_iso),
        "ended_at": meeting.ended_at.map(to_iso),
        "duration_secs": meeting.duration_secs,
        "status": meeting.status.as_str(),
        "participants": participants,
        "tags": tags,
        "created_at": to_iso(meeting.created_at),
        "updated_at": to_iso(meeting.updated_at),
    })
}

fn to_iso(ms: i64) -> String {
    DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// One-line summary used by `list` and `search`:
///   `<slug> · <title> · <duration>`
pub fn summary_line(m: &Meeting) -> String {
    format!(
        "{} · {} · {}",
        m.slug,
        m.title,
        format_duration(m.duration_secs)
    )
}
