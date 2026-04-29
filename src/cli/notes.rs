//! `moot notes` — generate or regenerate notes for a captured meeting (SPEC §5.4).

use std::path::PathBuf;

use clap::Args as ClapArgs;

use super::{Context, unimplemented};
use crate::error::Result;

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

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("notes")
}
