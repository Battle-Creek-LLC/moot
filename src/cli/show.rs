//! `moot show` — show a single meeting (SPEC §5.7).

use clap::Args as ClapArgs;

use super::Context;
use super::render;
use crate::error::Result;
use crate::store::{Store, resolve_path};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Meeting id or slug.
    pub target: String,

    /// Print the transcript to stdout.
    #[arg(long)]
    pub transcript: bool,

    /// Print the notes to stdout.
    #[arg(long)]
    pub notes: bool,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let store_path = resolve_path(ctx.db.as_deref())?;
    let store = Store::open(&store_path)?;
    let meeting = store.require_meeting(&args.target)?;
    let tags = store.tags_for(&meeting.id)?;

    if ctx.json {
        let mut payload = render::meeting_to_json(&meeting, &tags);
        if args.transcript {
            payload["transcript_md"] = serde_json::Value::String(
                meeting.transcript_md.clone().unwrap_or_default(),
            );
        }
        if args.notes {
            payload["notes_md"] =
                serde_json::Value::String(meeting.notes_md.clone().unwrap_or_default());
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    // Default human output: metadata table.
    println!("id          {}", meeting.id);
    println!("slug        {}", meeting.slug);
    println!("title       {}", meeting.title);
    if let Some(p) = &meeting.platform {
        println!("platform    {p}");
    }
    if let Some(u) = &meeting.url {
        println!("url         {u}");
    }
    println!("status      {}", meeting.status.as_str());
    println!("started     {}", render::format_started(meeting.started_at));
    println!("duration    {}", render::format_duration(meeting.duration_secs));
    if !tags.is_empty() {
        println!("tags        {}", tags.join(", "));
    }
    if let Some(blob) = &meeting.participants_json {
        if let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(blob) {
            let names: Vec<String> = items
                .into_iter()
                .filter_map(|v| match v {
                    serde_json::Value::String(s) => Some(s),
                    serde_json::Value::Object(map) => map
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string()),
                    _ => None,
                })
                .collect();
            if !names.is_empty() {
                println!("speakers    {}", names.join(", "));
            }
        }
    }

    if args.transcript {
        println!();
        if let Some(md) = &meeting.transcript_md {
            print!("{md}");
            if !md.ends_with('\n') {
                println!();
            }
        }
    }
    if args.notes {
        println!();
        if let Some(notes) = &meeting.notes_md {
            print!("{notes}");
            if !notes.ends_with('\n') {
                println!();
            }
        } else {
            println!("(no notes)");
        }
    }
    Ok(())
}
