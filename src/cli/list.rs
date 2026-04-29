//! `moot list` — list captured meetings (SPEC §5.6).

use clap::Args as ClapArgs;

use super::Context;
use super::render;
use crate::error::{Error, Result};
use crate::store::{MeetingFilters, MeetingStatus, Store, resolve_path};
use crate::util::time;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Only meetings newer than this duration (e.g. `7d`, `2w`, `1mo`).
    #[arg(long, value_name = "DUR")]
    pub since: Option<String>,

    /// Filter by tag.
    #[arg(long, value_name = "TAG")]
    pub tag: Option<String>,

    /// Filter by status.
    #[arg(long, value_name = "STATUS")]
    pub status: Option<String>,

    /// Maximum number of rows to return.
    #[arg(long)]
    pub limit: Option<usize>,

    /// Include cancelled meetings (hidden by default).
    #[arg(long)]
    pub all: bool,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let filters = build_filters(&args)?;

    let store_path = resolve_path(ctx.db.as_deref())?;
    let store = Store::open(&store_path)?;
    let meetings = store.list_meetings(&filters)?;

    if ctx.json {
        let mut arr = Vec::new();
        for m in &meetings {
            let tags = store.tags_for(&m.id)?;
            arr.push(render::meeting_to_json(m, &tags));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(arr))?
        );
        return Ok(());
    }

    if meetings.is_empty() {
        eprintln!("No meetings match.");
        return Ok(());
    }

    let id_w = meetings.iter().map(|m| m.id.len()).max().unwrap_or(2);
    let slug_w = meetings.iter().map(|m| m.slug.len()).max().unwrap_or(4);
    println!(
        "{:<id_w$}  {:<slug_w$}  {:<16}  {:<8}  {:<10}  {}",
        "id",
        "slug",
        "started",
        "duration",
        "status",
        "title",
        id_w = id_w,
        slug_w = slug_w,
    );
    for m in &meetings {
        println!(
            "{:<id_w$}  {:<slug_w$}  {:<16}  {:<8}  {:<10}  {}",
            m.id,
            m.slug,
            render::format_started(m.started_at),
            render::format_duration(m.duration_secs),
            m.status.as_str(),
            m.title,
            id_w = id_w,
            slug_w = slug_w,
        );
    }
    Ok(())
}

pub fn build_filters(args: &Args) -> Result<MeetingFilters> {
    let since_ms = match &args.since {
        Some(s) => {
            let dur = time::parse_duration(s)?;
            Some(time::now_ms() - dur.num_milliseconds())
        }
        None => None,
    };
    let status = match &args.status {
        Some(s) => Some(parse_status(s)?),
        None => None,
    };
    Ok(MeetingFilters {
        since_ms,
        tag: args.tag.clone(),
        status,
        include_cancelled: args.all || matches!(status, Some(MeetingStatus::Cancelled)),
        limit: args.limit,
    })
}

fn parse_status(s: &str) -> Result<MeetingStatus> {
    match s.to_ascii_lowercase().as_str() {
        "recording" => Ok(MeetingStatus::Recording),
        "processing" => Ok(MeetingStatus::Processing),
        "active" => Ok(MeetingStatus::Active),
        "failed" => Ok(MeetingStatus::Failed),
        "cancelled" | "canceled" => Ok(MeetingStatus::Cancelled),
        other => Err(Error::Cli(format!(
            "unknown status `{other}` (expected recording|processing|active|failed|cancelled)"
        ))),
    }
}
