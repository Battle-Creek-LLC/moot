//! `moot search` — find meetings by content (SPEC §5.8).

use clap::Args as ClapArgs;

use super::{Context, unimplemented};
use crate::error::Result;

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

    /// Filter by participant name.
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

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("search")
}
