//! `moot fetch` — re-pull a transcript from an existing Recall.ai bot (SPEC §5.2).

use clap::Args as ClapArgs;
use ulid::Ulid;

use super::Context;
use crate::error::{Error, Result};
use crate::recall::{DEFAULT_REGION, RecallApi, RecallClient};
use crate::store::{MeetingStatus, NewMeeting, Store, resolve_path};
use crate::transcript::{self, unique_speakers};
use crate::util::{slug, time::now_ms};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Recall.ai bot ID to fetch the transcript for.
    #[arg(long, value_name = "ID")]
    pub bot_id: String,

    /// Title for the meeting record.
    #[arg(long)]
    pub title: Option<String>,

    /// Generate notes after fetch.
    #[arg(long)]
    pub notes: bool,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let store_path = resolve_path(ctx.db.as_deref())?;
    let mut store = Store::open(&store_path)?;

    if let Some(_existing) = lookup_by_bot(&store, &args.bot_id)? {
        return Err(Error::Cli(format!(
            "bot {} already imported. Use `moot show` or delete the row first.",
            args.bot_id
        )));
    }

    let key = crate::secrets::get()?;
    let region =
        std::env::var("MOOT_RECALL_REGION").unwrap_or_else(|_| DEFAULT_REGION.to_string());
    let client = RecallClient::new(&key, &region);

    tracing::info!(bot_id = %args.bot_id, "fetching transcript");
    let segments = client.get_transcript(&args.bot_id).await?;
    if segments.is_empty() {
        return Err(Error::Recall(
            "Recall.ai returned no transcript segments".into(),
        ));
    }
    let utterances = transcript::from_segments(&segments);
    let speakers = unique_speakers(&utterances);
    let title = args
        .title
        .clone()
        .unwrap_or_else(|| format!("Meeting {}", &args.bot_id[..8.min(args.bot_id.len())]));

    let started_at_ms = now_ms();
    let started_dt = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(started_at_ms)
        .unwrap_or_else(chrono::Utc::now);
    let base_slug = slug::base(&title, started_dt);
    let final_slug = slug::disambiguate(&base_slug, |s| store.slug_taken(s).unwrap_or(false));

    let transcript_md = transcript::render_md(
        &title,
        None,
        Some(&iso(started_at_ms)),
        duration_secs(&utterances),
        &utterances,
    );
    let transcript_jsonl = transcript::render_jsonl(&utterances)?;
    let participants_json = serde_json::to_string(&speakers)?;
    let id = Ulid::new().to_string();

    let mut notes_md: Option<String> = None;
    let mut notes_prompt: Option<String> = None;
    if args.notes {
        let input = crate::notes::NotesInput {
            title: &title,
            platform: "unknown",
            meeting_id: &id,
            speakers: &speakers,
            transcript_md: &transcript_md,
        };
        match crate::notes::generate(None, &input).await {
            Ok((body, template)) => {
                notes_md = Some(body);
                notes_prompt = Some(template);
            }
            Err(e) => tracing::warn!("notes generation failed: {e}. Saving meeting without notes."),
        }
    }

    let new = NewMeeting {
        id: id.clone(),
        slug: final_slug.clone(),
        title,
        platform: None,
        url: None,
        recall_bot_id: Some(args.bot_id.clone()),
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

    if ctx.json {
        let payload = serde_json::json!({"id": id, "slug": final_slug});
        println!("{payload}");
    } else {
        println!("Fetched bot {} → {}", args.bot_id, final_slug);
    }
    Ok(())
}

fn lookup_by_bot(store: &Store, bot_id: &str) -> Result<Option<String>> {
    use rusqlite::{OptionalExtension, params};
    let row: Option<String> = store
        .conn()
        .query_row(
            "SELECT id FROM meetings WHERE recall_bot_id = ?1",
            params![bot_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(row)
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

fn iso(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
