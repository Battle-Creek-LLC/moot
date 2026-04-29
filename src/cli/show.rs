//! `moot show` — show a single meeting (SPEC §5.7).

use clap::Args as ClapArgs;

use super::{Context, unimplemented};
use crate::error::Result;

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

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("show")
}
