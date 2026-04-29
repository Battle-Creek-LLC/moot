//! `moot run` — dispatch a bot to a live meeting (SPEC §5.1).
//!
//! Flow per SPEC:
//!   1. Resolve API key.
//!   2. If `--resume <id>`, load `sessions.state_json` and skip dispatch.
//!   3. Detect platform from URL.
//!   4. POST to /bot, insert meetings row + sessions row.
//!   5. Poll /bot/{id} every 15s; transitions update meetings.status.
//!   6. On `done`, fetch transcript + populate the row.
//!   7. If `--notes`, generate via Claude.
//!   8. Set status=active, delete sessions.
//!   9. Print id + slug (or full record with --json).
//!
//! Cancellation: SIGINT triggers a graceful cancel — DELETE the bot, mark
//! the meeting `cancelled`, drop the session, exit 130. A second SIGINT
//! exits immediately.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use clap::Args as ClapArgs;
use tokio::select;
use tokio::sync::Mutex;
use ulid::Ulid;

use super::Context;
use crate::error::{Error, Result};
use crate::recall::{BotStatus, DEFAULT_REGION, RecallApi, RecallClient};
use crate::session::{Phase, SessionState};
use crate::store::{MeetingStatus, NewMeeting, Store, resolve_path};
use crate::transcript::{self, unique_speakers};
use crate::util::{platform, slug, time::now_ms};

const POLL_RECORDING_SECS: u64 = 15;
const POLL_PROCESSING_SECS: u64 = 5;
const HARD_CAP_HOURS: u64 = 12;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Meeting URL (Google Meet, Microsoft Teams, or Zoom).
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// Title for the meeting record. Defaults to a timestamped placeholder.
    #[arg(long)]
    pub title: Option<String>,

    /// Override the platform autodetected from the URL.
    #[arg(long, value_name = "meet|teams|zoom|unknown")]
    pub platform: Option<String>,

    /// Display name the bot uses when joining.
    #[arg(long, default_value = "Moot")]
    pub bot_name: String,

    /// Transcription language hint.
    #[arg(long)]
    pub language: Option<String>,

    /// Generate notes after capture finishes.
    #[arg(long)]
    pub notes: bool,

    /// Override the notes prompt template.
    #[arg(long, value_name = "FILE")]
    pub notes_prompt: Option<PathBuf>,

    /// Resume an interrupted run by meeting id.
    #[arg(long, value_name = "ID")]
    pub resume: Option<String>,

    /// Validate config and Recall.ai auth without dispatching a bot.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let key = crate::secrets::get()?;
    let region =
        std::env::var("MOOT_RECALL_REGION").unwrap_or_else(|_| DEFAULT_REGION.to_string());
    let client = Arc::new(RecallClient::new(&key, &region));

    if args.dry_run {
        client.check().await?;
        println!("OK — Recall.ai accepted the key (region {region}).");
        return Ok(());
    }

    let store_path = resolve_path(ctx.db.as_deref())?;
    let store = Arc::new(Mutex::new(Store::open(&store_path)?));

    // Resume path.
    if let Some(meeting_id) = &args.resume {
        return resume(ctx, store, client, meeting_id, args.notes_prompt.clone()).await;
    }

    // Fresh dispatch.
    let url = args.url.clone().ok_or_else(|| {
        Error::Cli("`moot run` requires --url <meeting url> (or --resume <id>)".into())
    })?;
    let detected_platform = args
        .platform
        .as_deref()
        .and_then(platform::parse)
        .unwrap_or_else(|| platform::detect(&url));

    let started_at_ms = now_ms();
    let started_dt = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(started_at_ms)
        .unwrap_or_else(chrono::Utc::now);
    let title = args.title.clone().unwrap_or_else(|| {
        format!("Meeting {}", started_dt.format("%Y-%m-%d %H:%M"))
    });
    let id = Ulid::new().to_string();
    let base_slug = slug::base(&title, started_dt);
    let final_slug = {
        let store = store.lock().await;
        slug::disambiguate(&base_slug, |s| store.slug_taken(s).unwrap_or(false))
    };

    tracing::info!(url = %url, platform = detected_platform.as_str(), "dispatching bot");
    let bot_id = client.create_bot(&url, &args.bot_name).await?;
    tracing::info!(bot_id = %bot_id, "bot dispatched");

    {
        let mut store = store.lock().await;
        store.insert_meeting(&NewMeeting {
            id: id.clone(),
            slug: final_slug.clone(),
            title: title.clone(),
            platform: Some(detected_platform.as_str().into()),
            url: Some(url.clone()),
            recall_bot_id: Some(bot_id.clone()),
            language: args.language.clone(),
            started_at: Some(started_at_ms),
            ended_at: None,
            duration_secs: None,
            status: MeetingStatus::Recording,
            transcript_jsonl: None,
            transcript_md: None,
            notes_md: None,
            notes_prompt: None,
            participants_json: None,
        })?;

        let session = SessionState {
            meeting_id: id.clone(),
            phase: Phase::Polling,
            bot_id: bot_id.clone(),
            platform: detected_platform.as_str().into(),
            url: url.clone(),
            started_at_ms,
            last_status: None,
            last_polled_ms: started_at_ms,
            notes_requested: args.notes,
        };
        store.upsert_session(&id, &session.to_json()?)?;
    }

    drive(
        ctx,
        store.clone(),
        client.clone(),
        id.clone(),
        bot_id.clone(),
        title.clone(),
        detected_platform.as_str().into(),
        url.clone(),
        args.notes,
        args.notes_prompt.clone(),
    )
    .await
}

async fn resume(
    ctx: &Context,
    store: Arc<Mutex<Store>>,
    client: Arc<RecallClient>,
    meeting_id: &str,
    notes_prompt: Option<PathBuf>,
) -> Result<()> {
    let (state, meeting) = {
        let store = store.lock().await;
        let session = store
            .get_session(meeting_id)?
            .ok_or_else(|| Error::Cli(format!("no session for meeting {meeting_id}")))?;
        let state = SessionState::from_json(&session.0)?;
        let meeting = store.require_meeting(meeting_id)?;
        (state, meeting)
    };
    drive(
        ctx,
        store,
        client,
        meeting.id.clone(),
        state.bot_id.clone(),
        meeting.title.clone(),
        state.platform.clone(),
        state.url.clone(),
        state.notes_requested,
        notes_prompt,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn drive(
    ctx: &Context,
    store: Arc<Mutex<Store>>,
    client: Arc<RecallClient>,
    meeting_id: String,
    bot_id: String,
    title: String,
    platform_str: String,
    url: String,
    notes_requested: bool,
    notes_prompt: Option<PathBuf>,
) -> Result<()> {
    let cancel_token = Arc::new(tokio::sync::Notify::new());
    spawn_sigint_listener(cancel_token.clone());

    let drive_inner = drive_inner(
        ctx,
        store.clone(),
        client.clone(),
        meeting_id.clone(),
        bot_id.clone(),
        title,
        platform_str,
        url,
        notes_requested,
        notes_prompt,
    );

    let cancelled = cancel_token.notified();
    tokio::pin!(cancelled);
    let outcome = select! {
        res = drive_inner => Outcome::Finished(res),
        _ = &mut cancelled => Outcome::Cancelled,
    };

    match outcome {
        Outcome::Finished(res) => res,
        Outcome::Cancelled => {
            tracing::warn!("SIGINT received — cancelling bot {bot_id}");
            // Best-effort cleanup. Race a second SIGINT against the cleanup
            // path so users can ^C twice to bail.
            let cleanup = cleanup_after_cancel(store.clone(), client.clone(), &meeting_id, &bot_id);
            let second_cancel = cancel_token.notified();
            tokio::pin!(second_cancel);
            select! {
                _ = cleanup => {}
                _ = &mut second_cancel => {
                    tracing::error!("second SIGINT — exiting immediately, leaving meeting {meeting_id} in `recording` (run `moot clean` later)");
                }
            }
            std::process::exit(130);
        }
    }
}

enum Outcome {
    Finished(Result<()>),
    Cancelled,
}

#[allow(clippy::too_many_arguments)]
async fn drive_inner(
    ctx: &Context,
    store: Arc<Mutex<Store>>,
    client: Arc<RecallClient>,
    meeting_id: String,
    bot_id: String,
    title: String,
    platform_str: String,
    _url: String,
    notes_requested: bool,
    notes_prompt: Option<PathBuf>,
) -> Result<()> {
    let started_at = now_ms();
    let mut last_code: Option<String> = None;

    loop {
        if (now_ms() - started_at) / 1000 / 3600 >= HARD_CAP_HOURS as i64 {
            return Err(Error::Recall(format!(
                "hard cap of {HARD_CAP_HOURS}h reached for bot {bot_id}"
            )));
        }
        let status: BotStatus = client.get_bot(&bot_id).await?;
        if last_code.as_deref() != Some(&status.status_code) {
            tracing::info!(status = %status.description(), code = %status.status_code, "bot status");
            last_code = Some(status.status_code.clone());
            // Update meetings.status as we transition.
            let mapped = if status.is_call_ended() || status.is_done() {
                Some(MeetingStatus::Processing)
            } else if status.is_fatal() {
                Some(MeetingStatus::Failed)
            } else {
                None
            };
            if let Some(s) = mapped {
                let mut st = store.lock().await;
                st.set_status(&meeting_id, s)?;
            }
            // Persist session checkpoint.
            update_session_checkpoint(&store, &meeting_id, &status, notes_requested).await?;
        }

        if status.is_done() {
            break;
        }
        if status.is_fatal() {
            let mut st = store.lock().await;
            st.set_status(&meeting_id, MeetingStatus::Failed)?;
            return Err(Error::Recall(format!(
                "bot {bot_id} reported fatal status: {:?}",
                status.sub_status
            )));
        }

        let sleep = if status.is_call_ended() {
            POLL_PROCESSING_SECS
        } else {
            POLL_RECORDING_SECS
        };
        tokio::time::sleep(StdDuration::from_secs(sleep)).await;
    }

    // Done — fetch transcript.
    tracing::info!("fetching transcript");
    let segments = client.get_transcript(&bot_id).await?;
    let utterances = transcript::from_segments(&segments);
    let speakers = unique_speakers(&utterances);
    let duration = duration_secs(&utterances);
    let ended_at = now_ms();

    let transcript_md = transcript::render_md(
        &title,
        Some(&platform_str),
        Some(&iso(started_at)),
        duration,
        &utterances,
    );
    let transcript_jsonl = transcript::render_jsonl(&utterances)?;

    let mut notes_md: Option<String> = None;
    let mut notes_prompt_used: Option<String> = None;
    if notes_requested {
        let input = crate::notes::NotesInput {
            title: &title,
            platform: &platform_str,
            meeting_id: &meeting_id,
            speakers: &speakers,
            transcript_md: &transcript_md,
        };
        match crate::notes::generate(notes_prompt.clone(), &input).await {
            Ok((body, template)) => {
                notes_md = Some(body);
                notes_prompt_used = Some(template);
            }
            Err(e) => tracing::warn!("notes generation failed: {e}"),
        }
    }

    {
        let mut st = store.lock().await;
        let mut meeting = st.require_meeting(&meeting_id)?;
        meeting.transcript_md = Some(transcript_md);
        meeting.transcript_jsonl = Some(transcript_jsonl);
        meeting.notes_md = notes_md;
        meeting.notes_prompt = notes_prompt_used;
        meeting.participants_json = Some(serde_json::to_string(&speakers)?);
        meeting.duration_secs = duration;
        meeting.ended_at = Some(ended_at);
        meeting.status = MeetingStatus::Active;
        st.update_meeting(&meeting)?;
        st.delete_session(&meeting_id)?;
    }

    let slug = {
        let st = store.lock().await;
        st.require_meeting(&meeting_id)?.slug
    };

    if ctx.json {
        let payload = serde_json::json!({"id": meeting_id, "slug": slug});
        println!("{payload}");
    } else {
        println!("Captured {slug} ({meeting_id})");
    }
    Ok(())
}

async fn update_session_checkpoint(
    store: &Arc<Mutex<Store>>,
    meeting_id: &str,
    status: &BotStatus,
    notes_requested: bool,
) -> Result<()> {
    let mut st = store.lock().await;
    let existing = st.get_session(meeting_id)?;
    let mut session = match existing {
        Some((json, _)) => SessionState::from_json(&json)?,
        None => return Ok(()),
    };
    session.last_status = Some(status.status_code.clone());
    session.last_polled_ms = now_ms();
    session.phase = if status.is_call_ended() || status.is_done() {
        Phase::Processing
    } else {
        Phase::Polling
    };
    session.notes_requested = notes_requested;
    st.upsert_session(meeting_id, &session.to_json()?)?;
    Ok(())
}

async fn cleanup_after_cancel(
    store: Arc<Mutex<Store>>,
    client: Arc<RecallClient>,
    meeting_id: &str,
    bot_id: &str,
) {
    if let Err(e) = client.delete_bot(bot_id).await {
        tracing::warn!("failed to delete bot {bot_id}: {e}");
    }
    let mut st = store.lock().await;
    if let Err(e) = st.set_status(meeting_id, MeetingStatus::Cancelled) {
        tracing::warn!("failed to mark meeting cancelled: {e}");
    }
    if let Err(e) = st.delete_session(meeting_id) {
        tracing::warn!("failed to drop session: {e}");
    }
}

fn spawn_sigint_listener(notify: Arc<tokio::sync::Notify>) {
    tokio::spawn(async move {
        loop {
            if tokio::signal::ctrl_c().await.is_err() {
                return;
            }
            notify.notify_one();
        }
    });
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
