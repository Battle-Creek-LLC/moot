//! `sessions.state_json` payload for crash recovery. See SPEC §8.
//!
//! Stored as a JSON blob keyed by `meeting_id`. The store handles persistence;
//! this module owns the schema and helpers.

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Phase of the run, used to decide where to resume from.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Bot dispatched, polling for status.
    Polling,
    /// `call_ended` seen, waiting for `done` so we can fetch transcript.
    Processing,
    /// Transcript fetched, generating notes.
    Notes,
    /// Done — the row should be deleted; this state exists only as a
    /// transient marker before deletion.
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub meeting_id: String,
    pub phase: Phase,
    pub bot_id: String,
    pub platform: String,
    pub url: String,
    pub started_at_ms: i64,
    pub last_status: Option<String>,
    pub last_polled_ms: i64,
    /// Whether the user requested notes generation.
    #[serde(default)]
    pub notes_requested: bool,
}

impl SessionState {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(s: &str) -> Result<Self> {
        Ok(serde_json::from_str(s)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = SessionState {
            meeting_id: "01ABC".into(),
            phase: Phase::Polling,
            bot_id: "bot-1".into(),
            platform: "meet".into(),
            url: "https://meet.google.com/x".into(),
            started_at_ms: 1700000000000,
            last_status: Some("in_call_recording".into()),
            last_polled_ms: 1700000900000,
            notes_requested: true,
        };
        let json = s.to_json().unwrap();
        let back = SessionState::from_json(&json).unwrap();
        assert_eq!(back.bot_id, "bot-1");
        assert_eq!(back.phase, Phase::Polling);
    }
}
