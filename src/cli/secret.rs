//! `moot secret` — manage credentials in the OS keychain (SPEC §5.10).

use clap::{Args as ClapArgs, Subcommand};

use super::{Context, unimplemented};
use crate::error::Result;

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Read the API key from stdin and store it in the keychain.
    Set,
    /// Print the stored key (masked unless --reveal).
    Get {
        #[arg(long)]
        reveal: bool,
    },
    /// Delete the stored key.
    Delete,
    /// Verify the stored key against the Recall.ai API.
    Check,
}

pub async fn execute(_ctx: &Context, _args: Args) -> Result<()> {
    unimplemented("secret")
}
