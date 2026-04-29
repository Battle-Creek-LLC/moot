//! Seed several meetings and exercise search filters + snippets.

use moot::search::{self, Field, SearchOptions};
use moot::store::{MeetingFilters, MeetingStatus, NewMeeting, Store};
use moot::util::time;

fn seed(store: &mut Store, slug: &str, title: &str, transcript: &str, notes: &str, started_ms: i64, status: MeetingStatus) {
    let id = ulid::Ulid::new().to_string();
    store
        .insert_meeting(&NewMeeting {
            id,
            slug: slug.into(),
            title: title.into(),
            platform: Some("meet".into()),
            url: None,
            recall_bot_id: None,
            language: None,
            started_at: Some(started_ms),
            ended_at: None,
            duration_secs: None,
            status,
            transcript_jsonl: None,
            transcript_md: Some(transcript.into()),
            notes_md: Some(notes.into()),
            notes_prompt: None,
            participants_json: Some("[\"Alice\",\"Bob\"]".into()),
        })
        .unwrap();
}

fn now() -> i64 { time::now_ms() }

#[test]
fn snippets_around_matches() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        "auth-2026",
        "Auth talk",
        "We discussed authentication for thirty minutes",
        "Decision: defer auth until Q3.",
        now(),
        MeetingStatus::Active,
    );
    let hits = search::search(&store, "auth", &MeetingFilters::default(), &SearchOptions::default()).unwrap();
    assert_eq!(hits.len(), 1);
    let h = &hits[0];
    assert!(h.score >= 3); // title + transcript + notes
    assert!(h.matches.iter().any(|m| m.field == Field::Title));
    assert!(h.matches.iter().any(|m| m.field == Field::Notes));
    assert!(h.matches.iter().any(|m| m.field == Field::Transcript));
    let snippet = &h.matches[0].snippet;
    // Should contain the matched word.
    assert!(snippet.to_ascii_lowercase().contains("auth"));
}

#[test]
fn since_filter_excludes_old_meetings() {
    let mut store = Store::in_memory().unwrap();
    let day_ms = 24 * 3600 * 1000;
    seed(&mut store, "old-2025", "Auth retro", "auth", "auth", now() - 90 * day_ms, MeetingStatus::Active);
    seed(&mut store, "new-2026", "Auth standup", "auth", "auth", now() - 1 * day_ms, MeetingStatus::Active);

    let filters = MeetingFilters {
        since_ms: Some(now() - 7 * day_ms),
        ..MeetingFilters::default()
    };
    let hits = search::search(&store, "auth", &filters, &SearchOptions::default()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].meeting.slug, "new-2026");
}

#[test]
fn cancelled_hidden_unless_requested() {
    let mut store = Store::in_memory().unwrap();
    seed(&mut store, "live-2026", "Auth standup", "we discussed auth", "", now(), MeetingStatus::Active);
    seed(&mut store, "killed-2026", "Auth retro", "we discussed auth too", "", now(), MeetingStatus::Cancelled);

    // Default: cancelled is hidden.
    let hits = search::search(&store, "auth", &MeetingFilters::default(), &SearchOptions::default()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].meeting.slug, "live-2026");

    // Asking for cancelled surfaces it.
    let filters = MeetingFilters {
        status: Some(MeetingStatus::Cancelled),
        include_cancelled: true,
        ..MeetingFilters::default()
    };
    let hits = search::search(&store, "auth", &filters, &SearchOptions::default()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].meeting.slug, "killed-2026");
}

#[test]
fn no_snippets_skips_matches_payload() {
    let mut store = Store::in_memory().unwrap();
    seed(&mut store, "x-2026", "Auth", "we like auth", "", now(), MeetingStatus::Active);
    let opts = SearchOptions {
        include_snippets: false,
        ..SearchOptions::default()
    };
    let hits = search::search(&store, "auth", &MeetingFilters::default(), &opts).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].score > 0);
    assert!(hits[0].matches.is_empty());
}
