//! `moot run` — dispatch a bot to a live meeting (SPEC §5.1).

use std::path::PathBuf;

use clap::Args as ClapArgs;

use super::{Context, unimplemented};
use crate::error::Result;

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

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("run")
}
