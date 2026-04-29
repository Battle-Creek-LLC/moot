//! Top-level clap command tree. See SPEC §5 for the verb list and each
//! subcommand's flag set.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::error::Result;

pub mod clean;
pub mod export;
pub mod fetch;
pub mod import;
pub mod list;
pub mod notes;
pub mod render;
pub mod run;
pub mod search;
pub mod secret;
pub mod show;

#[derive(Debug, Parser)]
#[command(
    name = "moot",
    version,
    about = "Send a Recall.ai bot to a meeting, capture transcripts, generate notes",
    long_about = None,
)]
pub struct Cli {
    /// Path to the SQLite database. Overrides `MOOT_DB` and the XDG default.
    #[arg(long, global = true, value_name = "PATH", env = "MOOT_DB")]
    pub db: Option<PathBuf>,

    /// Emit JSON on stdout. Logs go to stderr in JSON too.
    #[arg(long, global = true)]
    pub json: bool,

    /// Increase log verbosity. `-v` = debug, `-vv` = trace.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Dispatch a bot to a live meeting
    Run(run::Args),
    /// Re-pull a transcript from a known Recall.ai bot ID
    Fetch(fetch::Args),
    /// Load a meeting from an existing transcript file
    Import(import::Args),
    /// Generate or regenerate notes for a captured meeting
    Notes(notes::Args),
    /// Write a captured meeting to files on disk
    Export(export::Args),
    /// List captured meetings
    List(list::Args),
    /// Show a single meeting
    Show(show::Args),
    /// Find meetings by content
    Search(search::Args),
    /// Remove old sessions or bundles
    Clean(clean::Args),
    /// Manage credentials in the OS keychain
    Secret(secret::Args),
}

impl Cli {
    pub async fn dispatch(self) -> Result<()> {
        let ctx = Context { db: self.db, json: self.json };
        match self.command {
            Command::Run(a) => run::execute(&ctx, a).await,
            Command::Fetch(a) => fetch::execute(&ctx, a).await,
            Command::Import(a) => import::execute(&ctx, a).await,
            Command::Notes(a) => notes::execute(&ctx, a).await,
            Command::Export(a) => export::execute(&ctx, a).await,
            Command::List(a) => list::execute(&ctx, a).await,
            Command::Show(a) => show::execute(&ctx, a).await,
            Command::Search(a) => search::execute(&ctx, a).await,
            Command::Clean(a) => clean::execute(&ctx, a).await,
            Command::Secret(a) => secret::execute(&ctx, a).await,
        }
    }
}

/// Shared per-invocation context derived from global flags.
#[derive(Debug, Clone)]
pub struct Context {
    pub db: Option<PathBuf>,
    pub json: bool,
}

/// Phase 2 stub: print a placeholder message to stderr and exit 1. Each
/// subcommand calls this until Phase 3 replaces it with real logic.
fn unimplemented(verb: &'static str) -> Result<()> {
    eprintln!("moot {verb}: not yet implemented");
    std::process::exit(1);
}
