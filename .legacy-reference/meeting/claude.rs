//! Claude CLI subprocess management.
//!
//! Spawns the `claude` CLI to process transcripts into artifacts.

use anyhow::{Context, Result, bail};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::models::ArtifactType;

/// Configuration subset needed for Claude CLI invocation.
#[derive(Clone)]
pub struct ClaudeConfig {
    pub claude_model: String,
    pub claude_max_turns: u32,
}

/// Process a single artifact by spawning a `claude` CLI subprocess.
///
/// For `raw_transcript`, skips Claude entirely and copies the transcript text.
/// For all other types, pipes the prompt via stdin to `claude --print` and
/// writes stdout to the output path.
pub async fn process_artifact(
    artifact_type: &ArtifactType,
    transcript_path: &Path,
    output_path: &Path,
    rendered_prompt: &str,
    config: &ClaudeConfig,
) -> Result<()> {
    // For raw transcript, just copy the file — no LLM needed
    if *artifact_type == ArtifactType::Transcript {
        std::fs::copy(transcript_path, output_path)
            .with_context(|| format!("Failed to copy transcript to {}", output_path.display()))?;
        return Ok(());
    }

    let full_prompt = format!(
        "{}\n\nThe meeting transcript is at: {}\nWrite the output to: {}",
        rendered_prompt,
        transcript_path.display(),
        output_path.display(),
    );

    // Grant Claude access to the staging directory (transcript + output paths)
    let mut add_dirs: Vec<String> = Vec::new();
    if let Some(parent) = transcript_path.parent() {
        add_dirs.push(parent.to_string_lossy().to_string());
    }
    if let Some(parent) = output_path.parent() {
        let dir = parent.to_string_lossy().to_string();
        if !add_dirs.contains(&dir) {
            add_dirs.push(dir);
        }
    }

    let mut cmd = Command::new("claude");
    // Clear CLAUDECODE env var to allow invocation from within a Claude Code session
    cmd.env_remove("CLAUDECODE");
    cmd.arg("--print")
        .arg("--output-format")
        .arg("text")
        .arg("--model")
        .arg(&config.claude_model)
        .arg("--max-turns")
        .arg(config.claude_max_turns.to_string())
        .arg("--permission-mode")
        .arg("acceptEdits");

    for dir in &add_dirs {
        cmd.arg("--add-dir").arg(dir);
    }

    // Pipe the prompt via stdin to avoid shell argument length/parsing issues
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn claude CLI process")?;

    // Write prompt to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(full_prompt.as_bytes())
            .await
            .context("Failed to write prompt to claude stdin")?;
        // Drop stdin to close it, signaling EOF
    }

    let output = child
        .wait_with_output()
        .await
        .context("Failed to wait for claude CLI process")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "claude CLI exited with status {} while processing {} artifact.\nstderr: {}\nstdout: {}",
            output.status,
            artifact_type,
            stderr.trim(),
            stdout.chars().take(500).collect::<String>()
        );
    }

    // If Claude didn't write the file via tool use, write stdout to the output path
    if !output_path.exists() {
        let content = String::from_utf8_lossy(&output.stdout);
        if content.trim().is_empty() {
            bail!("claude produced no output for {} artifact", artifact_type);
        }
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output_path, content.as_bytes())
            .with_context(|| format!("Failed to write artifact to {}", output_path.display()))?;
    }

    Ok(())
}
