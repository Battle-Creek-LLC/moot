//! Round-trip: parse a fixture transcript, build a bundle, parse the bundle
//! back, confirm the transcript content survived. Covers `import` /
//! `export` / `bundle` together without spawning a process.

use moot::bundle::{Bundle, Format};
use moot::store::{MeetingStatus, NewMeeting, Store};
use moot::transcript::{self, Utterance};

const VTT: &str = "WEBVTT\n\n\
00:00:01.000 --> 00:00:02.500\n\
Alice: Morning everyone\n\n\
00:00:02.700 --> 00:00:04.000\n\
Bob: Hey\n";

#[test]
fn vtt_round_trip_via_bundle() {
    // Parse VTT → utterances.
    let utterances = transcript::parse_vtt(VTT).unwrap();
    let jsonl = transcript::render_jsonl(&utterances).unwrap();
    let md = transcript::render_md("Standup", Some("meet"), Some("2026-04-29T15:00:00Z"), Some(4), &utterances);

    // Insert into a fresh store.
    let mut store = Store::in_memory().unwrap();
    let id = ulid::Ulid::new().to_string();
    let new = NewMeeting {
        id: id.clone(),
        slug: "standup-2026-04-29".into(),
        title: "Standup".into(),
        platform: Some("meet".into()),
        url: None,
        recall_bot_id: None,
        language: None,
        started_at: Some(0),
        ended_at: None,
        duration_secs: Some(4),
        status: MeetingStatus::Active,
        transcript_jsonl: Some(jsonl.clone()),
        transcript_md: Some(md.clone()),
        notes_md: None,
        notes_prompt: None,
        participants_json: Some(serde_json::to_string(&["Alice", "Bob"]).unwrap()),
    };
    store.insert_meeting(&new).unwrap();

    // Build a bundle and verify each file's contents survived the trip.
    let meeting = store.get_meeting(&id).unwrap().unwrap();
    let bundle = Bundle::build(&meeting, &[], Format::All).unwrap();

    let by_name: std::collections::HashMap<&str, &[u8]> = bundle
        .files
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();

    let jsonl_out = std::str::from_utf8(by_name["transcript.jsonl"]).unwrap();
    assert_eq!(jsonl_out, jsonl);

    // Parse the bundle's jsonl back to utterances; round-trip should preserve content.
    let parsed: Vec<Utterance> = transcript::parse_jsonl(jsonl_out).unwrap();
    assert_eq!(parsed.len(), utterances.len());
    assert_eq!(parsed[0].speaker, "Alice");
    assert_eq!(parsed[0].text, "Morning everyone");

    let md_out = std::str::from_utf8(by_name["transcript.md"]).unwrap();
    assert_eq!(md_out, md);

    // Bundle has no notes, so notes.md must not appear.
    assert!(!by_name.contains_key("notes.md"));

    let toml_out = std::str::from_utf8(by_name["meeting.toml"]).unwrap();
    assert!(toml_out.contains("status = \"active\""));
    assert!(toml_out.contains("\"Alice\""));
    assert!(toml_out.contains("\"Bob\""));
}
