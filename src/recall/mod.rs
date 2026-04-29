//! Recall.ai REST client. Ported from `.legacy-reference/meeting/recall.rs`.
//!
//! Auth: `Authorization: Token <key>`. See SPEC §6 for endpoint shapes and
//! status code semantics.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Default API region. Override via `MOOT_RECALL_REGION` or config.
pub const DEFAULT_REGION: &str = "us-west-2";

/// One transcript segment — a participant's continuous turn, joined into a
/// single string. Mirrors the legacy `models::Segment` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    /// Seconds from meeting start.
    pub start: f64,
    /// Seconds from meeting start.
    pub end: f64,
    pub text: String,
    pub speaker: String,
    pub speaker_id: Option<u32>,
}

/// Bot status as reported by Recall.ai.
#[derive(Debug, Clone)]
pub struct BotStatus {
    pub status_code: String,
    pub sub_status: Option<String>,
}

impl BotStatus {
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

    pub fn is_recording(&self) -> bool {
        self.status_code == "in_call_recording"
    }

    pub fn is_call_ended(&self) -> bool {
        self.status_code == "call_ended"
    }

    pub fn is_done(&self) -> bool {
        self.status_code == "done"
    }

    pub fn is_fatal(&self) -> bool {
        self.status_code == "fatal"
    }
}

/// Trait abstraction so command code can be exercised with a mock client.
#[allow(async_fn_in_trait)]
pub trait RecallApi {
    async fn create_bot(&self, meeting_url: &str, bot_name: &str) -> Result<String>;
    async fn get_bot(&self, bot_id: &str) -> Result<BotStatus>;
    async fn get_transcript(&self, bot_id: &str) -> Result<Vec<Segment>>;
    async fn delete_bot(&self, bot_id: &str) -> Result<()>;
    async fn check(&self) -> Result<()>;
}

/// HTTP client for the Recall.ai REST API.
pub struct RecallClient {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl RecallClient {
    pub fn new(api_key: &str, region: &str) -> Self {
        let base_url = format!("https://{region}.recall.ai/api/v1");
        RecallClient {
            api_key: api_key.to_string(),
            base_url,
            client: reqwest::Client::new(),
        }
    }

    fn auth(&self) -> String {
        format!("Token {}", self.api_key)
    }
}

impl RecallApi for RecallClient {
    async fn create_bot(&self, meeting_url: &str, bot_name: &str) -> Result<String> {
        let body = CreateBotRequest {
            meeting_url: meeting_url.into(),
            bot_name: bot_name.into(),
            recording_config: RecordingConfig {
                transcript: TranscriptConfig {
                    provider: TranscriptProvider {
                        recallai_streaming: RecallaiStreaming {},
                    },
                },
                retention: RetentionConfig {
                    retention_type: "timed".into(),
                    hours: 72,
                },
            },
        };
        let resp = self
            .client
            .post(format!("{}/bot", self.base_url))
            .header("Authorization", self.auth())
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Recall(format!(
                "bot creation failed (HTTP {status}): {text}"
            )));
        }
        let result: CreateBotResponse = resp.json().await?;
        Ok(result.id)
    }

    async fn get_bot(&self, bot_id: &str) -> Result<BotStatus> {
        let detail = self.get_bot_detail(bot_id).await?;
        let latest = detail
            .status_changes
            .last()
            .map(|sc| BotStatus {
                status_code: sc.code.clone(),
                sub_status: sc.sub_code.clone(),
            })
            .unwrap_or(BotStatus {
                status_code: "unknown".into(),
                sub_status: None,
            });
        Ok(latest)
    }

    async fn get_transcript(&self, bot_id: &str) -> Result<Vec<Segment>> {
        let detail = self.get_bot_detail(bot_id).await?;
        let download_url = detail
            .recordings
            .first()
            .and_then(|r| r.media_shortcuts.as_ref())
            .and_then(|ms| ms.transcript.as_ref())
            .and_then(|t| t.data.as_ref())
            .and_then(|d| d.download_url.as_ref())
            .ok_or_else(|| {
                Error::Recall(
                    "transcript download URL not available yet — meeting may still be processing".into(),
                )
            })?;

        tracing::debug!(url = %download_url, "downloading transcript");
        // The signed download URL does not need our auth header.
        let resp = self.client.get(download_url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Recall(format!(
                "transcript download failed (HTTP {status}): {text}"
            )));
        }
        let entries: Vec<TranscriptEntry> = resp.json().await?;

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
            let start = entry.words.first().map(|w| w.start_timestamp.relative).unwrap_or(0.0);
            let end = entry.words.last().map(|w| w.end_timestamp.relative).unwrap_or(0.0);
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

    async fn delete_bot(&self, bot_id: &str) -> Result<()> {
        let resp = self
            .client
            .delete(format!("{}/bot/{}", self.base_url, bot_id))
            .header("Authorization", self.auth())
            .send()
            .await?;
        // 200 / 204 / 404 (already gone) are all acceptable.
        let status = resp.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        let text = resp.text().await.unwrap_or_default();
        Err(Error::Recall(format!("bot deletion failed (HTTP {status}): {text}")))
    }

    async fn check(&self) -> Result<()> {
        // Anonymous endpoints exist, so HEAD a bot listing under our token.
        // A 401/403 means the key is wrong; 200 means the key works.
        let resp = self
            .client
            .get(format!("{}/bot", self.base_url))
            .header("Authorization", self.auth())
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(Error::Recall(format!(
                "Recall.ai rejected the API key (HTTP {status})"
            )));
        }
        let text = resp.text().await.unwrap_or_default();
        Err(Error::Recall(format!("Recall.ai check failed (HTTP {status}): {text}")))
    }
}

impl RecallClient {
    async fn get_bot_detail(&self, bot_id: &str) -> Result<BotDetailResponse> {
        let resp = self
            .client
            .get(format!("{}/bot/{}", self.base_url, bot_id))
            .header("Authorization", self.auth())
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Recall(format!(
                "bot detail failed (HTTP {status}): {text}"
            )));
        }
        Ok(resp.json().await?)
    }
}

// ---- wire types -----------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateBotResponse {
    id: String,
}

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
