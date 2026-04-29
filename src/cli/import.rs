//! `moot import` — load a meeting from an existing transcript file (SPEC §5.3).

use std::path::PathBuf;

use clap::Args as ClapArgs;

use super::{Context, unimplemented};
use crate::error::Result;

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

    /// Comma-separated participant names.
    #[arg(long, value_name = "CSV")]
    pub participants: Option<String>,

    /// ISO-8601 start time.
    #[arg(long, value_name = "ISO8601")]
    pub started_at: Option<String>,

    /// Generate notes after import.
    #[arg(long)]
    pub notes: bool,

    /// Use the provided markdown as notes; skip generation.
    #[arg(long, value_name = "PATH", conflicts_with = "notes")]
    pub notes_file: Option<PathBuf>,
}

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("import")
}
