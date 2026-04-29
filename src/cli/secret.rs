//! `moot secret` — manage credentials in the OS keychain (SPEC §5.10).

use std::io::{self, BufRead};

use clap::{Args as ClapArgs, Subcommand};

use super::Context;
use crate::error::{Error, Result};
use crate::recall::{DEFAULT_REGION, RecallApi, RecallClient};
use crate::secrets;

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

pub async fn execute(_ctx: &Context, args: Args) -> Result<()> {
    match args.action {
        Action::Set => set().await,
        Action::Get { reveal } => get(reveal),
        Action::Delete => delete(),
        Action::Check => check().await,
    }
}

async fn set() -> Result<()> {
    let stdin = io::stdin();
    let mut input = String::new();
    let bytes = stdin
        .lock()
        .read_line(&mut input)
        .map_err(|e| Error::Cli(format!("failed to read stdin: {e}")))?;
    if bytes == 0 {
        return Err(Error::Cli(
            "no input on stdin. Pipe or type the key, e.g. `pbpaste | moot secret set`".into(),
        ));
    }
    let key = input.trim();
    if key.is_empty() {
        return Err(Error::Cli("empty API key".into()));
    }
    secrets::set(key)?;
    println!("Stored Recall.ai key in the OS keychain.");
    Ok(())
}

fn get(reveal: bool) -> Result<()> {
    let key = secrets::get()?;
    if reveal {
        println!("{key}");
    } else {
        println!("{}", mask(&key));
    }
    Ok(())
}

fn mask(key: &str) -> String {
    let n = key.chars().count();
    if n <= 8 {
        return "*".repeat(n);
    }
    let head: String = key.chars().take(4).collect();
    let tail: String = key.chars().rev().take(4).collect::<String>().chars().rev().collect();
    format!("{head}…{tail} ({n} chars)")
}

fn delete() -> Result<()> {
    secrets::delete()?;
    println!("Deleted Recall.ai key from the OS keychain.");
    Ok(())
}

async fn check() -> Result<()> {
    let key = secrets::get()?;
    let region =
        std::env::var("MOOT_RECALL_REGION").unwrap_or_else(|_| DEFAULT_REGION.to_string());
    let client = RecallClient::new(&key, &region);
    client.check().await?;
    println!("OK — Recall.ai accepted the key (region {region}).");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn masks_long_keys() {
        let masked = mask("abcdefghijklmnop");
        assert!(masked.starts_with("abcd"));
        assert!(masked.contains("mnop"));
        assert!(masked.contains("16 chars"));
    }

    #[test]
    fn masks_short_keys_completely() {
        assert_eq!(mask("hi"), "**");
    }
}
