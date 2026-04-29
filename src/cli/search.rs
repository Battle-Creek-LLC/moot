//! `moot search` — find meetings by content (SPEC §5.8).

use clap::Args as ClapArgs;

use super::Context;
use super::render;
use crate::error::Result;
use crate::search::{self, SearchOptions};
use crate::store::{MeetingFilters, MeetingStatus, Store, resolve_path};
use crate::util::time;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Free-text query. Multi-word queries are AND'd; quote phrases for exact substring.
    pub query: String,

    /// Comma-separated subset of `title,notes,transcript`.
    #[arg(long, value_name = "FIELDS", default_value = "title,notes,transcript")]
    pub r#in: String,

    /// Only meetings newer than this duration.
    #[arg(long, value_name = "DUR")]
    pub since: Option<String>,

    /// Filter by tag.
    #[arg(long)]
    pub tag: Option<String>,

    /// Filter by participant name (matches participants_json).
    #[arg(long)]
    pub participant: Option<String>,

    /// Filter by status.
    #[arg(long)]
    pub status: Option<String>,

    /// Maximum number of results.
    #[arg(long)]
    pub limit: Option<usize>,

    /// Snippet length in chars around each match.
    #[arg(long, default_value_t = 80)]
    pub context: usize,

    /// Print meeting rows only, no surrounding text.
    #[arg(long)]
    pub no_snippets: bool,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let fields = search::parse_fields(&args.r#in)?;

    let since_ms = args
        .since
        .as_deref()
        .map(|s| time::parse_duration(s).map(|d| time::now_ms() - d.num_milliseconds()))
        .transpose()?;

    let status = args.status.as_deref().map(parse_status).transpose()?;
    let filters = MeetingFilters {
        since_ms,
        tag: args.tag.clone(),
        status,
        include_cancelled: matches!(status, Some(MeetingStatus::Cancelled)),
        limit: None, // We apply limit after scoring.
    };
    let opts = SearchOptions {
        fields,
        context_chars: args.context,
        include_snippets: !args.no_snippets,
        limit: args.limit,
    };

    let store_path = resolve_path(ctx.db.as_deref())?;
    let store = Store::open(&store_path)?;
    let mut hits = search::search(&store, &args.query, &filters, &opts)?;

    if let Some(p) = &args.participant {
        let needle = p.to_ascii_lowercase();
        hits.retain(|h| {
            h.meeting
                .participants_json
                .as_deref()
                .map(|s| s.to_ascii_lowercase().contains(&needle))
                .unwrap_or(false)
        });
    }

    if ctx.json {
        let mut arr = Vec::new();
        for h in &hits {
            let tags = store.tags_for(&h.meeting.id)?;
            let matches: Vec<_> = h
                .matches
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "field": m.field.label(),
                        "snippet": m.snippet,
                        "offset": m.offset,
                    })
                })
                .collect();
            arr.push(serde_json::json!({
                "meeting": render::meeting_to_json(&h.meeting, &tags),
                "matches": matches,
                "score": h.score,
            }));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(arr))?
        );
        return Ok(());
    }

    if hits.is_empty() {
        eprintln!("No matches.");
        return Ok(());
    }

    for h in &hits {
        println!("{}", render::summary_line(&h.meeting));
        if !args.no_snippets {
            // Dedupe snippets (transcript matches often look identical).
            let mut seen = std::collections::HashSet::new();
            for m in &h.matches {
                if seen.insert(m.snippet.clone()) {
                    println!("  {}", m.snippet);
                }
                if seen.len() >= 3 {
                    break;
                }
            }
        }
        println!();
    }
    Ok(())
}

fn parse_status(s: &str) -> Result<MeetingStatus> {
    match s.to_ascii_lowercase().as_str() {
        "recording" => Ok(MeetingStatus::Recording),
        "processing" => Ok(MeetingStatus::Processing),
        "active" => Ok(MeetingStatus::Active),
        "failed" => Ok(MeetingStatus::Failed),
        "cancelled" | "canceled" => Ok(MeetingStatus::Cancelled),
        other => Err(crate::error::Error::Cli(format!(
            "unknown status `{other}`"
        ))),
    }
}
