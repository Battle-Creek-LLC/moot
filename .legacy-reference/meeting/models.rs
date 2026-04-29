//! Data types used across the meeting module.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Artifact types that can be generated from a meeting transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    Notes,
    Spec,
    Adr,
    JourneyMap,
    ActionItems,
    Transcript,
}

impl fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactType::Notes => write!(f, "notes"),
            ArtifactType::Spec => write!(f, "spec"),
            ArtifactType::Adr => write!(f, "adr"),
            ArtifactType::JourneyMap => write!(f, "journey_map"),
            ArtifactType::ActionItems => write!(f, "action_items"),
            ArtifactType::Transcript => write!(f, "transcript"),
        }
    }
}

impl FromStr for ArtifactType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "notes" => Ok(ArtifactType::Notes),
            "spec" => Ok(ArtifactType::Spec),
            "adr" => Ok(ArtifactType::Adr),
            "journey_map" | "journey-map" => Ok(ArtifactType::JourneyMap),
            "action_items" | "action-items" => Ok(ArtifactType::ActionItems),
            "transcript" | "raw-transcript" => Ok(ArtifactType::Transcript),
            _ => anyhow::bail!(
                "Unknown artifact type: '{}'. Valid types: notes, spec, adr, journey_map, action_items, transcript",
                s
            ),
        }
    }
}

impl ArtifactType {
    /// Returns all available artifact types.
    pub fn all() -> Vec<ArtifactType> {
        vec![
            ArtifactType::Notes,
            ArtifactType::Spec,
            ArtifactType::Adr,
            ArtifactType::JourneyMap,
            ArtifactType::ActionItems,
            ArtifactType::Transcript,
        ]
    }

    /// File extension for this artifact type.
    pub fn extension(&self) -> &'static str {
        match self {
            ArtifactType::Transcript => "txt",
            _ => "md",
        }
    }

    /// Human-readable description of the artifact type.
    pub fn description(&self) -> &'static str {
        match self {
            ArtifactType::Notes => "Structured meeting notes",
            ArtifactType::Spec => "Requirements specification",
            ArtifactType::Adr => "Architecture Decision Record(s)",
            ArtifactType::JourneyMap => "User/customer journey map",
            ArtifactType::ActionItems => "Action items with owners",
            ArtifactType::Transcript => "Unprocessed transcript (no LLM)",
        }
    }

    /// Returns the prompt template for this artifact type (None for transcript).
    pub fn prompt_template(&self) -> Option<&'static str> {
        match self {
            ArtifactType::Notes => Some(include_str!("prompts/notes.md.tera")),
            ArtifactType::Spec => Some(include_str!("prompts/spec.md.tera")),
            ArtifactType::Adr => Some(include_str!("prompts/adr.md.tera")),
            ArtifactType::JourneyMap => Some(include_str!("prompts/journey_map.md.tera")),
            ArtifactType::ActionItems => Some(include_str!("prompts/action_items.md.tera")),
            ArtifactType::Transcript => None,
        }
    }
}

/// One transcript segment from Recall.ai.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    /// Seconds from meeting start
    pub start: f64,
    /// Seconds from meeting start
    pub end: f64,
    /// Transcribed text
    pub text: String,
    /// Speaker display name
    pub speaker: String,
    /// Speaker ID from Recall.ai
    pub speaker_id: Option<u32>,
}

/// The full meeting transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTranscript {
    pub meeting_id: String,
    pub platform: String,
    pub segments: Vec<Segment>,
}

impl MeetingTranscript {
    /// Convert the transcript to plain text format.
    pub fn to_text(&self) -> String {
        let mut output = String::new();
        for seg in &self.segments {
            let minutes = (seg.start / 60.0).floor() as u32;
            let seconds = (seg.start % 60.0).floor() as u32;
            output.push_str(&format!(
                "[{:02}:{:02}] {}: {}\n",
                minutes, seconds, seg.speaker, seg.text
            ));
        }
        output
    }

    /// Get unique speaker names.
    pub fn speakers(&self) -> Vec<String> {
        let mut speakers: Vec<String> = self
            .segments
            .iter()
            .map(|s| s.speaker.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        speakers.sort();
        speakers
    }

    /// Estimated duration in minutes.
    pub fn duration_minutes(&self) -> f64 {
        self.segments.iter().map(|s| s.end).fold(0.0_f64, f64::max) / 60.0
    }
}

/// Session phase for crash recovery tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Connecting,
    Recording,
    Fetching,
    Processing,
    Publishing,
    Done,
}

impl fmt::Display for SessionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionPhase::Connecting => write!(f, "connecting"),
            SessionPhase::Recording => write!(f, "recording"),
            SessionPhase::Fetching => write!(f, "fetching"),
            SessionPhase::Processing => write!(f, "processing"),
            SessionPhase::Publishing => write!(f, "publishing"),
            SessionPhase::Done => write!(f, "done"),
        }
    }
}

/// Session state serialized to session.json for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub meeting_id: String,
    pub platform: String,
    pub recall_bot_id: String,
    pub phase: SessionPhase,
    pub artifacts_requested: Vec<String>,
    pub artifacts_staged: Vec<String>,
    pub artifacts_published: Vec<String>,
    pub output_dir: String,
    pub created_at: String,
    pub updated_at: String,
}
