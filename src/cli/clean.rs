//! `moot clean` — remove old sessions or bundles (SPEC §5.9).

use clap::Args as ClapArgs;

use super::{Context, unimplemented};
use crate::error::Result;

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

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("clean")
}
