//! `moot import` — load a meeting from an existing transcript file (SPEC §5.3).

use std::path::PathBuf;

use chrono::DateTime;
use clap::Args as ClapArgs;
use ulid::Ulid;

use super::Context;
use crate::error::{Error, Result};
use crate::store::{MeetingStatus, NewMeeting, Store, resolve_path};
use crate::transcript::{self, unique_speakers};
use crate::util::{platform, slug, time::now_ms};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Path to a transcript file (.jsonl, .vtt, .srt, or .txt).
    #[arg(short = 'f', long, value_name = "PATH")]
    pub file: PathBuf,

    /// Meeting title.
    #[arg(long)]
    pub title: String,

    /// Platform tag for the meeting.
    #[arg(long, value_name = "meet|teams|zoom|unknown")]
    pub platform: Option<String>,

    /// Comma-separated participant names. Defaults to speakers from the transcript.
    #[arg(long, value_name = "CSV")]
    pub participants: Option<String>,

    /// ISO-8601 start time (defaults to now).
    #[arg(long, value_name = "ISO8601")]
    pub started_at: Option<String>,

    /// Generate notes after import.
    #[arg(long)]
    pub notes: bool,

    /// Use the provided markdown as notes; skip generation.
    #[arg(long, value_name = "PATH", conflicts_with = "notes")]
    pub notes_file: Option<PathBuf>,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let body = std::fs::read_to_string(&args.file)
        .map_err(|e| Error::Fs(format!("read {}: {e}", args.file.display())))?;
    let utterances = transcript::parse_by_extension(&args.file, &body)?;
    let speakers = unique_speakers(&utterances);
    let participants = match args.participants.as_deref() {
        Some(csv) => csv.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>(),
        None => speakers.clone(),
    };

    let started_at_ms = match args.started_at.as_deref() {
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map_err(|e| Error::Cli(format!("invalid --started-at: {e}")))?
            .with_timezone(&chrono::Utc)
            .timestamp_millis(),
        None => now_ms(),
    };

    let detected_platform = args
        .platform
        .as_deref()
        .and_then(platform::parse)
        .map(|p| p.as_str().to_string());

    let id = Ulid::new().to_string();
    let started_dt = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(started_at_ms)
        .unwrap_or_else(chrono::Utc::now);
    let base_slug = slug::base(&args.title, started_dt);

    let store_path = resolve_path(ctx.db.as_deref())?;
    let mut store = Store::open(&store_path)?;
    let final_slug = slug::disambiguate(&base_slug, |s| store.slug_taken(s).unwrap_or(false));

    let transcript_md = transcript::render_md(
        &args.title,
        detected_platform.as_deref(),
        Some(&format_iso(started_at_ms)),
        duration_secs(&utterances),
        &utterances,
    );
    let transcript_jsonl = transcript::render_jsonl(&utterances)?;
    let participants_json = serde_json::to_string(&participants)?;

    let notes_md = if let Some(p) = &args.notes_file {
        Some(std::fs::read_to_string(p).map_err(|e| Error::Fs(format!("read {}: {e}", p.display())))?)
    } else {
        None
    };
    let mut notes_prompt: Option<String> = None;

    if args.notes {
        let input = crate::notes::NotesInput {
            title: &args.title,
            platform: detected_platform.as_deref().unwrap_or("unknown"),
            meeting_id: &id,
            speakers: &speakers,
            transcript_md: &transcript_md,
        };
        match crate::notes::generate(None, &input).await {
            Ok((body, template)) => {
                notes_prompt = Some(template);
                let new = NewMeeting {
                    id: id.clone(),
                    slug: final_slug.clone(),
                    title: args.title.clone(),
                    platform: detected_platform.clone(),
                    url: None,
                    recall_bot_id: None,
                    language: None,
                    started_at: Some(started_at_ms),
                    ended_at: None,
                    duration_secs: duration_secs(&utterances),
                    status: MeetingStatus::Active,
                    transcript_jsonl: Some(transcript_jsonl.clone()),
                    transcript_md: Some(transcript_md.clone()),
                    notes_md: Some(body),
                    notes_prompt,
                    participants_json: Some(participants_json.clone()),
                };
                store.insert_meeting(&new)?;
                announce(ctx, &id, &final_slug);
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("notes generation failed: {e}. Importing meeting without notes.");
            }
        }
    }

    let new = NewMeeting {
        id: id.clone(),
        slug: final_slug.clone(),
        title: args.title.clone(),
        platform: detected_platform,
        url: None,
        recall_bot_id: None,
        language: None,
        started_at: Some(started_at_ms),
        ended_at: None,
        duration_secs: duration_secs(&utterances),
        status: MeetingStatus::Active,
        transcript_jsonl: Some(transcript_jsonl),
        transcript_md: Some(transcript_md),
        notes_md,
        notes_prompt,
        participants_json: Some(participants_json),
    };
    store.insert_meeting(&new)?;
    announce(ctx, &id, &final_slug);
    Ok(())
}

fn duration_secs(utterances: &[crate::transcript::Utterance]) -> Option<i64> {
    if utterances.len() < 2 {
        return None;
    }
    let last = utterances.last()?.ts_offset_ms;
    if last <= 0 {
        return None;
    }
    Some((last / 1000).max(1))
}

fn format_iso(ms: i64) -> String {
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now);
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn announce(ctx: &Context, id: &str, slug: &str) {
    if ctx.json {
        let payload = serde_json::json!({"id": id, "slug": slug});
        println!("{payload}");
    } else {
        println!("Imported meeting {slug} ({id})");
    }
}
