//! XDG-aware path resolution.
//!
//! See SPEC §3.1. PLAN reference data pins the qualifier triple to
//! `("dev", "battlecreek", "moot")`; that wins over the spec's earlier draft
//! `("ai", "Moot", "moot")` so paths align with the rest of the
//! `dev.battlecreek` namespace (matches `dev.battlecreek.moot` on macOS).

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::error::{Error, Result};

const QUALIFIER: &str = "dev";
const ORGANIZATION: &str = "battlecreek";
const APPLICATION: &str = "moot";

const DB_FILENAME: &str = "moot.db";
const CONFIG_FILENAME: &str = "config.toml";

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).ok_or_else(|| {
        Error::Config("could not determine user data directory".into())
    })
}

/// Resolve the SQLite DB path. Honors `--db` override (passed in) and the
/// `MOOT_DB` env var; falls back to the XDG data directory.
pub fn db_path(override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("MOOT_DB") {
        return Ok(PathBuf::from(p));
    }
    let dirs = project_dirs()?;
    Ok(dirs.data_dir().join(DB_FILENAME))
}

/// Resolve the optional config TOML path.
pub fn config_path() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    Ok(dirs.config_dir().join(CONFIG_FILENAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_over_env_and_default() {
        let p = Path::new("/tmp/explicit.db");
        let resolved = db_path(Some(p)).unwrap();
        assert_eq!(resolved, p);
    }
}
