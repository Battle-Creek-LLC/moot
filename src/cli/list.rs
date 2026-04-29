//! `moot list` — list captured meetings (SPEC §5.6).

use clap::Args as ClapArgs;

use super::{Context, unimplemented};
use crate::error::Result;

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

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("list")
}
