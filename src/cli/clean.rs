//! `moot clean` — remove old sessions or bundles (SPEC §5.9).

use clap::Args as ClapArgs;
use rusqlite::params;

use super::Context;
use crate::error::Result;
use crate::store::{Store, resolve_path};
use crate::util::time;

const STALE_SESSION_HOURS: i64 = 24;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Delete session rows for terminal meetings plus stale recording sessions.
    /// (Default if no other flag is given.)
    #[arg(long)]
    pub sessions: bool,

    /// Cascade-delete meetings older than the given duration (e.g. `90d`).
    #[arg(long, value_name = "DUR")]
    pub older_than: Option<String>,

    /// Print what would be deleted without modifying the DB.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let store_path = resolve_path(ctx.db.as_deref())?;
    let mut store = Store::open(&store_path)?;

    let mut deleted_sessions = 0usize;
    let mut deleted_meetings = 0usize;

    // Sessions are cleaned by default unless --older-than is the only flag.
    let do_sessions = args.sessions || args.older_than.is_none();
    if do_sessions {
        let stale_threshold = time::now_ms() - STALE_SESSION_HOURS * 3600 * 1000;
        let orphans = store.list_orphan_sessions(stale_threshold)?;
        for (meeting_id, status, updated_ms) in &orphans {
            tracing::info!(
                meeting_id = %meeting_id,
                status = status.as_str(),
                updated_ms = *updated_ms,
                "session orphan"
            );
            if !args.dry_run {
                store.delete_session(meeting_id)?;
            }
            deleted_sessions += 1;
        }
    }

    if let Some(dur_str) = &args.older_than {
        let dur = time::parse_duration(dur_str)?;
        let cutoff = time::now_ms() - dur.num_milliseconds();
        // Pull the ids first so we can log + count.
        let ids: Vec<String> = {
            let mut stmt = store.conn().prepare(
                "SELECT id FROM meetings WHERE COALESCE(started_at, created_at) < ?1",
            )?;
            let rows = stmt.query_map(params![cutoff], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            out
        };
        for id in &ids {
            tracing::info!(meeting_id = %id, "deleting old meeting");
            if !args.dry_run {
                store.delete_meeting(id)?;
            }
            deleted_meetings += 1;
        }
    }

    let payload = serde_json::json!({
        "deleted_sessions": deleted_sessions,
        "deleted_meetings": deleted_meetings,
        "dry_run": args.dry_run,
    });
    if ctx.json {
        println!("{payload}");
    } else if args.dry_run {
        println!(
            "(dry-run) would delete {} session(s) and {} meeting(s)",
            deleted_sessions, deleted_meetings
        );
    } else {
        println!(
            "Deleted {} session(s) and {} meeting(s)",
            deleted_sessions, deleted_meetings
        );
    }
    Ok(())
}
