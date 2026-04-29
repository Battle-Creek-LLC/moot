//! Export a transcript from Recall.ai by bot ID.
//!
//! Fetches the transcript directly from the Recall.ai API and writes it
//! to a file or stdout. Optionally generates artifacts (notes, spec, etc.)
//! from the transcript using the Claude CLI.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::Semaphore;

use super::claude::{ClaudeConfig, process_artifact};
use super::config::MeetingConfig;
use super::log;
use super::models::{ArtifactType, MeetingTranscript};
use super::publisher;
use super::recall::RecallClient;
use super::run::render_prompt;

/// Export a transcript from Recall.ai using the bot ID.
///
/// If `output` is provided, writes the transcript to that file path.
/// Otherwise prints to stdout.
///
/// If `artifacts` is non-empty, generates the requested artifact types
/// from the transcript and publishes them to `output_dir`.
pub async fn export_transcript(
    bot_id: &str,
    output: Option<&str>,
    artifacts: &[String],
    output_dir: Option<&str>,
) -> Result<()> {
    // Parse & validate artifact types upfront (fail fast)
    let artifact_types: Vec<ArtifactType> = artifacts
        .iter()
        .map(|a| a.parse::<ArtifactType>())
        .collect::<Result<Vec<_>>>()
        .context("Invalid artifact type specified")?;

    let config = MeetingConfig::from_env()?;
    let recall_client = RecallClient::new(&config.recall_api_key, &config.recall_region);

    log(format!("Fetching transcript for bot {}...", bot_id));

    let segments = recall_client
        .get_transcript(bot_id)
        .await
        .context("Failed to fetch transcript from Recall.ai. The media may have expired (72-hour retention).")?;

    if segments.is_empty() {
        anyhow::bail!("Transcript is empty. The meeting may still be processing, or the media may have expired.");
    }

    let transcript = MeetingTranscript {
        meeting_id: bot_id.to_string(),
        platform: String::new(),
        segments,
    };

    let text = transcript.to_text();

    log(format!(
        "Transcript: {} speakers, {:.0} minutes",
        transcript.speakers().len(),
        transcript.duration_minutes()
    ));

    // Write transcript to --output or stdout (preserving existing behavior)
    match output {
        Some(path) => {
            let path = PathBuf::from(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }
            std::fs::write(&path, &text)
                .with_context(|| format!("Failed to write transcript to: {}", path.display()))?;
            log(format!("Transcript written to: {}", path.display()));
        }
        None => {
            if artifact_types.is_empty() {
                // Only print to stdout when no artifacts requested (otherwise it clutters output)
                print!("{}", text);
            }
        }
    }

    // If no artifacts requested, we're done
    if artifact_types.is_empty() {
        return Ok(());
    }

    // Create temp staging dir
    let staging_dir = tempfile::tempdir().context("Failed to create temporary staging directory")?;
    let transcript_path = staging_dir.path().join("transcript.txt");
    std::fs::write(&transcript_path, &text).context("Failed to write transcript to staging")?;

    // Process artifacts concurrently
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
    let mut handles = Vec::new();

    let claude_config = ClaudeConfig {
        claude_model: config.claude_model.clone(),
        claude_max_turns: config.claude_max_turns,
    };

    for artifact_type in &artifact_types {
        let rendered_prompt = render_prompt(artifact_type, &transcript, &transcript_path)?;

        let output_path = staging_dir
            .path()
            .join(format!("artifact_{}.{}", artifact_type, artifact_type.extension()));
        let transcript_path = transcript_path.clone();
        let claude_config = claude_config.clone();
        let artifact_type_for_push = artifact_type.clone();
        let artifact_type = artifact_type.clone();
        let sem = semaphore.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem
                .acquire()
                .await
                .map_err(|e| anyhow::anyhow!("Semaphore error: {}", e))?;

            log(format!("Generating {} artifact...", artifact_type));

            process_artifact(
                &artifact_type,
                &transcript_path,
                &output_path,
                &rendered_prompt,
                &claude_config,
            )
            .await?;

            log(format!("{} artifact generated.", artifact_type));
            Ok::<(String, PathBuf), anyhow::Error>((artifact_type.to_string(), output_path))
        });

        handles.push((artifact_type_for_push, handle));
    }

    // Resolve output directory
    let final_output_dir = output_dir
        .map(|d| d.to_string())
        .unwrap_or_else(|| config.output_dir.clone());

    // Wait for all artifact processing and publish
    for (artifact_type, handle) in handles {
        let (type_str, staged_path) = handle
            .await
            .context("Artifact processing task panicked")?
            .context("Artifact processing failed")?;

        let published_path = publisher::publish_artifact(
            &type_str,
            artifact_type.extension(),
            &staged_path,
            &final_output_dir,
            bot_id,
        )
        .await?;

        log(format!("Published: {}", published_path.display()));
    }

    // Also publish the transcript
    let published_transcript = publisher::publish_artifact(
        "transcript",
        "txt",
        &transcript_path,
        &final_output_dir,
        bot_id,
    )
    .await?;

    log(format!(
        "Transcript published: {}",
        published_transcript.display()
    ));

    // staging_dir auto-cleaned on drop

    eprintln!();
    log("Export complete.");
    log(format!("Output: {}/{}", final_output_dir, bot_id));

    Ok(())
}
