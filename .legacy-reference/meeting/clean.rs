//! Cleanup stale staging directories from interrupted meeting runs.

use anyhow::{Context, Result};
use std::path::PathBuf;

use super::config::MeetingConfig;
use super::log;
use super::models::SessionPhase;
use super::session::SessionManager;

/// Remove stale staging directories.
///
/// Deletes staging directories where the session phase is `Done` or the
/// directory is older than 3 days.
pub async fn clean() -> Result<()> {
    let config = MeetingConfig::from_env()?;
    let staging_dir = PathBuf::from(&config.staging_dir);

    if !staging_dir.exists() {
        log("No staging directory found. Nothing to clean.");
        return Ok(());
    }

    let entries = std::fs::read_dir(&staging_dir).with_context(|| {
        format!(
            "Failed to read staging directory: {}",
            staging_dir.display()
        )
    })?;

    let mut cleaned = 0;
    let mut skipped = 0;
    let three_days_ago = chrono::Utc::now() - chrono::Duration::days(3);

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let meeting_id = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        let session_mgr = SessionManager::new(&staging_dir, &meeting_id);
        let should_clean = match session_mgr.load() {
            Ok(Some(state)) => {
                if state.phase == SessionPhase::Done {
                    true
                } else {
                    // Check if directory is older than 3 days
                    match chrono::DateTime::parse_from_rfc3339(&state.created_at) {
                        Ok(created) => created < three_days_ago,
                        Err(_) => false,
                    }
                }
            }
            Ok(None) => {
                // No session.json — check directory modification time
                match entry.metadata().and_then(|m| m.modified()) {
                    Ok(modified) => {
                        let modified: chrono::DateTime<chrono::Utc> = modified.into();
                        modified < three_days_ago
                    }
                    Err(_) => false,
                }
            }
            Err(_) => false,
        };

        if should_clean {
            session_mgr.cleanup()?;
            log(format!("Cleaned: {}", meeting_id));
            cleaned += 1;
        } else {
            skipped += 1;
        }
    }

    eprintln!();
    if cleaned == 0 {
        log("No stale staging directories found.");
    } else {
        log(format!(
            "Cleaned {} staging director{}, skipped {}.",
            cleaned,
            if cleaned == 1 { "y" } else { "ies" },
            skipped
        ));
    }

    Ok(())
}
