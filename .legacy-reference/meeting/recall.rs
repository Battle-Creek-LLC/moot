//! Recall.ai API client.
//!
//! Handles bot creation, status polling, and transcript retrieval.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::models::Segment;

/// Bot status as reported by Recall.ai.
#[derive(Debug, Clone)]
pub struct BotStatus {
    pub status_code: String,
    pub sub_status: Option<String>,
}

impl BotStatus {
    /// Human-readable description of the current status.
    pub fn description(&self) -> &str {
        match self.status_code.as_str() {
            "joining_call" => "Joining...",
            "in_waiting_room" => "Waiting for admission...",
            "in_call_not_recording" => "In call, waiting to record...",
            "in_call_recording" => "Recording",
            "call_ended" => "Meeting ended",
            "done" => "Transcript ready",
            "fatal" => "Fatal error",
            _ => "Unknown status",
        }
    }

    /// Whether the bot is actively recording.
    pub fn is_recording(&self) -> bool {
        self.status_code == "in_call_recording"
    }

    /// Whether the meeting is over and transcript is available.
    pub fn is_done(&self) -> bool {
        self.status_code == "done" || self.status_code == "call_ended"
    }

    /// Whether the bot encountered a fatal error.
    pub fn is_fatal(&self) -> bool {
        self.status_code == "fatal"
    }
}

/// Recall.ai API response for bot creation.
#[derive(Debug, Deserialize)]
struct CreateBotResponse {
    id: String,
}

/// Recall.ai API response for bot details (GET /bot/{id}).
#[derive(Debug, Deserialize)]
struct BotDetailResponse {
    status_changes: Vec<StatusChange>,
    #[serde(default)]
    recordings: Vec<Recording>,
}

#[derive(Debug, Deserialize)]
struct StatusChange {
    code: String,
    #[serde(default)]
    sub_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Recording {
    #[serde(default)]
    media_shortcuts: Option<MediaShortcuts>,
}

#[derive(Debug, Deserialize)]
struct MediaShortcuts {
    #[serde(default)]
    transcript: Option<TranscriptShortcut>,
}

#[derive(Debug, Deserialize)]
struct TranscriptShortcut {
    #[serde(default)]
    data: Option<TranscriptData>,
}

#[derive(Debug, Deserialize)]
struct TranscriptData {
    #[serde(default)]
    download_url: Option<String>,
}

/// Transcript download response — array of participant utterances.
#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    participant: Participant,
    words: Vec<TranscriptWord>,
}

#[derive(Debug, Deserialize)]
struct Participant {
    id: u32,
    name: String,
}

#[derive(Debug, Deserialize)]
struct TranscriptWord {
    text: String,
    start_timestamp: Timestamp,
    end_timestamp: Timestamp,
}

#[derive(Debug, Deserialize)]
struct Timestamp {
    relative: f64,
}

/// Request body for creating a bot.
#[derive(Debug, Serialize)]
struct CreateBotRequest {
    meeting_url: String,
    bot_name: String,
    recording_config: RecordingConfig,
}

#[derive(Debug, Serialize)]
struct RecordingConfig {
    transcript: TranscriptConfig,
    retention: RetentionConfig,
}

/// Media retention policy sent to Recall.ai at bot creation time.
#[derive(Debug, Serialize)]
struct RetentionConfig {
    #[serde(rename = "type")]
    retention_type: String,
    hours: u32,
}

#[derive(Debug, Serialize)]
struct TranscriptConfig {
    provider: TranscriptProvider,
}

#[derive(Debug, Serialize)]
struct TranscriptProvider {
    recallai_streaming: RecallaiStreaming,
}

#[derive(Debug, Serialize)]
struct RecallaiStreaming {}

/// Client for the Recall.ai REST API.
pub struct RecallClient {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl RecallClient {
    pub fn new(api_key: &str, region: &str) -> Self {
        let base_url = format!("https://{}.recall.ai/api/v1", region);
        RecallClient {
            api_key: api_key.to_string(),
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Create a bot and send it to a meeting. Returns the bot ID.
    pub async fn create_bot(&self, meeting_url: &str, bot_name: &str) -> Result<String> {
        let body = CreateBotRequest {
            meeting_url: meeting_url.to_string(),
            bot_name: bot_name.to_string(),
            recording_config: RecordingConfig {
                transcript: TranscriptConfig {
                    provider: TranscriptProvider {
                        recallai_streaming: RecallaiStreaming {},
                    },
                },
                retention: RetentionConfig {
                    retention_type: "timed".to_string(),
                    hours: 72,
                },
            },
        };

        let resp = self
            .client
            .post(format!("{}/bot", self.base_url))
            .header("Authorization", format!("Token {}", self.api_key))
            .json(&body)
            .send()
            .await
            .context("Failed to create Recall.ai bot")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("Recall.ai bot creation failed (HTTP {}): {}", status, text);
        }

        let result: CreateBotResponse = resp
            .json()
            .await
            .context("Failed to parse Recall.ai bot creation response")?;

        Ok(result.id)
    }

    /// Get the current status of a bot.
    pub async fn get_bot(&self, bot_id: &str) -> Result<BotStatus> {
        let detail = self.get_bot_detail(bot_id).await?;

        let latest = detail
            .status_changes
            .last()
            .map(|sc| BotStatus {
                status_code: sc.code.clone(),
                sub_status: sc.sub_code.clone(),
            })
            .unwrap_or(BotStatus {
                status_code: "unknown".to_string(),
                sub_status: None,
            });

        Ok(latest)
    }

    /// Get the full bot detail response.
    async fn get_bot_detail(&self, bot_id: &str) -> Result<BotDetailResponse> {
        let resp = self
            .client
            .get(format!("{}/bot/{}", self.base_url, bot_id))
            .header("Authorization", format!("Token {}", self.api_key))
            .send()
            .await
            .context("Failed to get bot details from Recall.ai")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("Recall.ai bot detail failed (HTTP {}): {}", status, text);
        }

        resp.json()
            .await
            .context("Failed to parse Recall.ai bot detail response")
    }

    /// Get transcript segments from the bot.
    ///
    /// Fetches the bot detail to find the transcript download URL,
    /// then downloads and parses the transcript.
    pub async fn get_transcript(&self, bot_id: &str) -> Result<Vec<Segment>> {
        let detail = self.get_bot_detail(bot_id).await?;

        // Extract transcript download URL from recordings
        let download_url = detail
            .recordings
            .first()
            .and_then(|r| r.media_shortcuts.as_ref())
            .and_then(|ms| ms.transcript.as_ref())
            .and_then(|t| t.data.as_ref())
            .and_then(|d| d.download_url.as_ref())
            .context("Transcript download URL not available yet. The meeting may still be in progress or processing.")?;

        tracing::debug!("Downloading transcript from: {}", download_url);

        let resp = self
            .client
            .get(download_url)
            .send()
            .await
            .context("Failed to download transcript from Recall.ai")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!(
                "Recall.ai transcript download failed (HTTP {}): {}",
                status,
                text
            );
        }

        let entries: Vec<TranscriptEntry> = resp
            .json()
            .await
            .context("Failed to parse Recall.ai transcript download")?;

        let mut segments = Vec::new();
        for entry in entries {
            if entry.words.is_empty() {
                continue;
            }
            let text: String = entry
                .words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let start = entry
                .words
                .first()
                .map(|w| w.start_timestamp.relative)
                .unwrap_or(0.0);
            let end = entry
                .words
                .last()
                .map(|w| w.end_timestamp.relative)
                .unwrap_or(0.0);
            segments.push(Segment {
                start,
                end,
                text,
                speaker: entry.participant.name.clone(),
                speaker_id: Some(entry.participant.id),
            });
        }

        Ok(segments)
    }
}
