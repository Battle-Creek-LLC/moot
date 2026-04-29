//! Session state management for crash recovery.
//!
//! Writes and reads session.json with atomic file operations.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::models::{SessionPhase, SessionState};

/// Manages session state for a meeting.
pub struct SessionManager {
    staging_dir: PathBuf,
}

impl SessionManager {
    pub fn new(staging_dir: &Path, meeting_id: &str) -> Self {
        SessionManager {
            staging_dir: staging_dir.join(meeting_id),
        }
    }

    /// Create a new session state file.
    pub fn create(&self, state: &SessionState) -> Result<()> {
        std::fs::create_dir_all(&self.staging_dir).with_context(|| {
            format!(
                "Failed to create staging directory: {}",
                self.staging_dir.display()
            )
        })?;
        self.atomic_write(state)
    }

    /// Load session state if it exists.
    pub fn load(&self) -> Result<Option<SessionState>> {
        let path = self.session_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read session file: {}", path.display()))?;
        let state: SessionState =
            serde_json::from_str(&content).context("Failed to parse session.json")?;
        Ok(Some(state))
    }

    /// Update the session phase.
    pub fn update_phase(&self, phase: SessionPhase) -> Result<()> {
        let mut state = self.load()?.context("No session state found to update")?;
        state.phase = phase;
        state.updated_at = chrono::Utc::now().to_rfc3339();
        self.atomic_write(&state)
    }

    /// Mark an artifact as staged.
    pub fn mark_staged(&self, artifact_type: &str) -> Result<()> {
        let mut state = self.load()?.context("No session state found to update")?;
        if !state.artifacts_staged.contains(&artifact_type.to_string()) {
            state.artifacts_staged.push(artifact_type.to_string());
        }
        state.updated_at = chrono::Utc::now().to_rfc3339();
        self.atomic_write(&state)
    }

    /// Mark an artifact as published.
    pub fn mark_published(&self, artifact_type: &str) -> Result<()> {
        let mut state = self.load()?.context("No session state found to update")?;
        if !state
            .artifacts_published
            .contains(&artifact_type.to_string())
        {
            state.artifacts_published.push(artifact_type.to_string());
        }
        state.updated_at = chrono::Utc::now().to_rfc3339();
        self.atomic_write(&state)
    }

    /// Delete the staging directory.
    pub fn cleanup(&self) -> Result<()> {
        if self.staging_dir.exists() {
            std::fs::remove_dir_all(&self.staging_dir).with_context(|| {
                format!(
                    "Failed to clean up staging directory: {}",
                    self.staging_dir.display()
                )
            })?;
        }
        Ok(())
    }

    /// Get the staging directory path.
    pub fn staging_path(&self) -> PathBuf {
        self.staging_dir.clone()
    }

    fn session_path(&self) -> PathBuf {
        self.staging_dir.join("session.json")
    }

    /// Atomic write: write to temp file, then rename.
    fn atomic_write(&self, state: &SessionState) -> Result<()> {
        let json =
            serde_json::to_string_pretty(state).context("Failed to serialize session state")?;
        let tmp_path = self.staging_dir.join("session.json.tmp");
        std::fs::write(&tmp_path, &json).with_context(|| {
            format!("Failed to write temp session file: {}", tmp_path.display())
        })?;
        std::fs::rename(&tmp_path, self.session_path()).context("Failed to rename session file")?;
        Ok(())
    }
}
