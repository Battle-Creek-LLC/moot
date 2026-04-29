//! `moot fetch` — re-pull a transcript from an existing Recall.ai bot (SPEC §5.2).

use clap::Args as ClapArgs;

use super::{Context, unimplemented};
use crate::error::Result;

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

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("fetch")
}
