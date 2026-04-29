//! Configuration management for the meeting module.
//!
//! Reads configuration from environment variables with sensible defaults.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

/// Meeting module configuration.
pub struct MeetingConfig {
    /// Recall.ai API key (required)
    pub recall_api_key: String,
    /// Recall.ai API region
    pub recall_region: String,
    /// Claude model to use for artifact generation
    pub claude_model: String,
    /// Maximum agentic turns per artifact
    pub claude_max_turns: u32,
    /// Maximum concurrent artifact processing
    pub max_concurrent: usize,
    /// Bot display name in meetings
    pub bot_name: String,
    /// Output directory for published artifacts
    pub output_dir: String,
    /// Staging directory for session state
    pub staging_dir: String,
    /// Polling interval in seconds
    pub polling_interval_secs: u64,
}

impl MeetingConfig {
    /// Load configuration from environment variables with defaults.
    pub fn from_env() -> Result<Self> {
        let recall_api_key = std::env::var("RECALL_API_KEY")
            .context("RECALL_API_KEY environment variable is required")?;

        let recall_region =
            std::env::var("RECALL_REGION").unwrap_or_else(|_| "us-west-2".to_string());

        // Default model — update when newer Sonnet versions are released
        let claude_model = std::env::var("MEETING_CLAUDE_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

        let claude_max_turns: u32 = std::env::var("MEETING_CLAUDE_MAX_TURNS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .context("MEETING_CLAUDE_MAX_TURNS must be a number")?;

        let max_concurrent: usize = std::env::var("MEETING_MAX_CONCURRENT")
            .unwrap_or_else(|_| "3".to_string())
            .parse()
            .context("MEETING_MAX_CONCURRENT must be a number")?;

        let bot_name = std::env::var("MEETING_BOT_NAME").unwrap_or_else(|_| "Mycelium".to_string());

        let output_dir =
            std::env::var("MEETING_OUTPUT_DIR").unwrap_or_else(|_| "./meeting-output".to_string());

        let staging_dir = std::env::var("MEETING_STAGING_DIR").unwrap_or_else(|_| {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".local/share/sprout/meetings")
                .to_string_lossy()
                .to_string()
        });

        Ok(MeetingConfig {
            recall_api_key,
            recall_region,
            claude_model,
            claude_max_turns,
            max_concurrent,
            bot_name,
            output_dir,
            staging_dir,
            polling_interval_secs: 30,
        })
    }

    /// Validate that prerequisites are available.
    pub fn validate_prerequisites(&self) -> Result<()> {
        // Check that claude CLI is in PATH
        let claude_check = std::process::Command::new("claude")
            .arg("--version")
            .output();

        match claude_check {
            Ok(output) if output.status.success() => {}
            _ => {
                bail!(
                    "`claude` CLI not found in PATH.\n\
                     Install it with: npm install -g @anthropic-ai/claude-code"
                );
            }
        }

        Ok(())
    }
}
