//! `moot notes` — generate or regenerate notes for a captured meeting (SPEC §5.4).

use std::path::PathBuf;

use clap::Args as ClapArgs;

use super::Context;
use crate::error::{Error, Result};
use crate::notes::NotesInput;
use crate::store::{Store, resolve_path};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Meeting id or slug.
    pub target: String,

    /// Override the default notes prompt template.
    #[arg(long, value_name = "FILE")]
    pub prompt: Option<PathBuf>,

    /// Overwrite existing notes without prompting.
    #[arg(long)]
    pub force: bool,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let store_path = resolve_path(ctx.db.as_deref())?;
    let mut store = Store::open(&store_path)?;
    let mut meeting = store.require_meeting(&args.target)?;

    if meeting.notes_md.is_some() && !args.force {
        return Err(Error::Cli(format!(
            "{} already has notes. Pass --force to overwrite.",
            meeting.slug
        )));
    }

    let transcript_md = meeting
        .transcript_md
        .as_deref()
        .ok_or_else(|| Error::Cli(format!("{} has no transcript to summarize", meeting.slug)))?;

    let speakers = participants_from_blob(meeting.participants_json.as_deref());
    let platform = meeting.platform.as_deref().unwrap_or("unknown").to_string();
    let input = NotesInput {
        title: &meeting.title,
        platform: &platform,
        meeting_id: &meeting.id,
        speakers: &speakers,
        transcript_md,
    };
    let (body, template) = crate::notes::generate(args.prompt.clone(), &input).await?;

    meeting.notes_md = Some(body);
    meeting.notes_prompt = Some(template);
    store.update_meeting(&meeting)?;

    if ctx.json {
        let payload = serde_json::json!({"id": meeting.id, "slug": meeting.slug});
        println!("{payload}");
    } else {
        println!("Updated notes for {}", meeting.slug);
    }
    Ok(())
}

fn participants_from_blob(blob: Option<&str>) -> Vec<String> {
    let Some(blob) = blob else { return Vec::new() };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(blob) else {
        return Vec::new();
    };
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                serde_json::Value::Object(map) => map
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}
