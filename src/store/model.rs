//! Plain data types mirroring the `meetings` schema row-for-row.

use serde::{Deserialize, Serialize};

/// One of the five SPEC §3.2 status values.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingStatus {
    Recording,
    Processing,
    Active,
    Failed,
    Cancelled,
}

impl MeetingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MeetingStatus::Recording => "recording",
            MeetingStatus::Processing => "processing",
            MeetingStatus::Active => "active",
            MeetingStatus::Failed => "failed",
            MeetingStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> MeetingStatus {
        match s {
            "recording" => MeetingStatus::Recording,
            "processing" => MeetingStatus::Processing,
            "failed" => MeetingStatus::Failed,
            "cancelled" => MeetingStatus::Cancelled,
            // Default for legacy or unrecognized values.
            _ => MeetingStatus::Active,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            MeetingStatus::Active | MeetingStatus::Failed | MeetingStatus::Cancelled
        )
    }
}

/// Insert-shape: caller fills these and `created_at`/`updated_at` are stamped
/// by the store at insert time.
#[derive(Debug, Clone)]
pub struct NewMeeting {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub platform: Option<String>,
    pub url: Option<String>,
    pub recall_bot_id: Option<String>,
    pub language: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub duration_secs: Option<i64>,
    pub status: MeetingStatus,
    pub transcript_jsonl: Option<String>,
    pub transcript_md: Option<String>,
    pub notes_md: Option<String>,
    pub notes_prompt: Option<String>,
    pub participants_json: Option<String>,
}

/// Read-shape: includes the timestamps and is what every read returns.
#[derive(Debug, Clone)]
pub struct Meeting {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub platform: Option<String>,
    pub url: Option<String>,
    pub recall_bot_id: Option<String>,
    pub language: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub duration_secs: Option<i64>,
    pub status: MeetingStatus,
    pub transcript_jsonl: Option<String>,
    pub transcript_md: Option<String>,
    pub notes_md: Option<String>,
    pub notes_prompt: Option<String>,
    pub participants_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Filter set for `list` and `search`.
#[derive(Debug, Clone, Default)]
pub struct MeetingFilters {
    pub since_ms: Option<i64>,
    pub tag: Option<String>,
    pub status: Option<MeetingStatus>,
    /// When false, hide `cancelled` meetings unless the caller explicitly
    /// asked for them via `status`.
    pub include_cancelled: bool,
    pub limit: Option<usize>,
}
