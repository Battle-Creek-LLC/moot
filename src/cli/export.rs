//! `moot export` — write a captured meeting to files on disk (SPEC §5.5).

use std::path::PathBuf;

use clap::{Args as ClapArgs, ValueEnum};

use super::{Context, unimplemented};
use crate::error::Result;

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum Format {
    Jsonl,
    Md,
    All,
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Meeting id or slug.
    pub target: String,

    /// Output directory. Use `-` to stream a tar archive to stdout.
    #[arg(long, value_name = "DIR|-")]
    pub out: Option<PathBuf>,

    /// What to write.
    #[arg(long, value_enum, default_value_t = Format::All)]
    pub format: Format,

    /// Overwrite an existing target directory.
    #[arg(long, conflicts_with = "out")]
    pub force: bool,
}

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("export")
}
