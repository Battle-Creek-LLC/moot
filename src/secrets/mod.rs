//! OS keychain access for the Recall.ai API key.
//!
//! Keyring service `moot`, entry `recall-api-key`. Env fallback
//! `MOOT_RECALL_API_KEY` for headless installs (CI, servers).

use crate::error::{Error, Result};

const SERVICE: &str = "moot";
const ENTRY: &str = "recall-api-key";
const ENV_VAR: &str = "MOOT_RECALL_API_KEY";

fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, ENTRY)
        .map_err(|e| Error::Keychain(format!("could not open keychain entry: {e}")))
}

/// Fetch the API key. Tries the env var first (so it can short-circuit on
/// headless boxes without a Secret Service daemon), then the keychain.
pub fn get() -> Result<String> {
    if let Ok(v) = std::env::var(ENV_VAR) {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    entry()?.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => Error::Config(format!(
            "no Recall.ai API key found. Run `moot secret set` or export {ENV_VAR}."
        )),
        other => Error::Keychain(other.to_string()),
    })
}

pub fn set(value: &str) -> Result<()> {
    entry()?
        .set_password(value)
        .map_err(|e| Error::Keychain(e.to_string()))
}

pub fn delete() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Keychain(e.to_string())),
    }
}
