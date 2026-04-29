//! Main orchestration flow for the meeting command.
//!
//! Implements the 10-step end-to-end flow: join → transcribe → process → publish.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use tokio::sync::Semaphore;

use super::claude::{ClaudeConfig, process_artifact};
use super::config::MeetingConfig;
use super::log;
use super::models::{ArtifactType, MeetingTranscript, SessionPhase, SessionState};
use super::publisher;
use super::recall::RecallClient;
use super::session::SessionManager;

/// Detect the meeting platform from the URL.
fn detect_platform(url: &str) -> Result<String> {
    if url.contains("meet.google.com") {
        Ok("google_meet".to_string())
    } else if url.contains("teams.microsoft.com") || url.contains("teams.live.com") {
        Ok("teams".to_string())
    } else if url.contains("zoom.us") || url.contains("zoom.com") {
        Ok("zoom".to_string())
    } else {
        bail!(
            "Could not detect platform from URL. Use --platform to specify one of: google_meet, teams, zoom"
        )
    }
}

/// Run the full meeting orchestration flow.
pub async fn run(
    meeting_url: &str,
    platform: Option<&str>,
    artifacts: &[String],
    output_dir: Option<&str>,
    resume: Option<&str>,
    bot_name: Option<&str>,
    _language: &str,
) -> Result<()> {
    // Step 1: Load config and apply CLI overrides
    let mut config = MeetingConfig::from_env()?;
    if let Some(dir) = output_dir {
        config.output_dir = dir.to_string();
    }
    if let Some(name) = bot_name {
        config.bot_name = name.to_string();
    }

    // Detect or use provided platform
    let platform = match platform {
        Some(p) => p.to_string(),
        None => detect_platform(meeting_url)?,
    };
    log(format!("Platform: {}", platform));

    // Step 2: Validate prerequisites
    config.validate_prerequisites()?;

    // Parse artifact types
    let artifact_types: Vec<ArtifactType> = artifacts
        .iter()
        .map(|a| a.parse::<ArtifactType>())
        .collect::<Result<Vec<_>>>()
        .context("Invalid artifact type specified")?;

    if artifact_types.is_empty() {
        bail!("At least one artifact type must be specified with --artifacts");
    }

    let recall_client = RecallClient::new(&config.recall_api_key, &config.recall_region);

    // Step 3: Handle resume
    if let Some(meeting_id) = resume {
        return resume_session(meeting_id, &config, &recall_client, &artifact_types).await;
    }

    // Step 4: Create bot
    let meeting_id = uuid::Uuid::new_v4().to_string();
    log(format!("Meeting ID: {}", meeting_id));
    log(format!("Sending bot to: {}", meeting_url));

    let bot_id = recall_client
        .create_bot(meeting_url, &config.bot_name)
        .await
        .context("Failed to create Recall.ai bot")?;

    log(format!("Bot created: {}", bot_id));

    // Write session state
    let session_mgr = SessionManager::new(&PathBuf::from(&config.staging_dir), &meeting_id);
    let state = SessionState {
        meeting_id: meeting_id.clone(),
        platform: platform.to_string(),
        recall_bot_id: bot_id.clone(),
        phase: SessionPhase::Connecting,
        artifacts_requested: artifacts.to_vec(),
        artifacts_staged: Vec::new(),
        artifacts_published: Vec::new(),
        output_dir: config.output_dir.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    session_mgr
        .create(&state)
        .context("Failed to create session state")?;

    // Set up Ctrl+C handler
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    ctrlc::set_handler(move || {
        shutdown_clone.store(true, Ordering::SeqCst);
    })
    .context("Failed to set Ctrl+C handler")?;

    // Step 5: Poll bot status until recording
    log("Waiting for bot to join meeting...");
    loop {
        if shutdown.load(Ordering::SeqCst) {
            log("Ctrl+C received. Fetching available transcript...");
            break;
        }

        let status = recall_client
            .get_bot(&bot_id)
            .await
            .context("Failed to poll bot status")?;

        log(format!("Bot status: {}", status.description()));

        if status.is_fatal() {
            bail!(
                "Recall.ai bot encountered a fatal error: {:?}",
                status.sub_status
            );
        }

        if status.is_recording() {
            session_mgr.update_phase(SessionPhase::Recording)?;
            log("Bot is recording.");
            break;
        }

        if status.is_done() {
            log("Meeting already ended.");
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(
            config.polling_interval_secs,
        ))
        .await;
    }

    // Step 6: Poll bot status until meeting ends
    // (Transcript is only available via download URL after the meeting,
    // so we just track status during the recording phase.)
    if !shutdown.load(Ordering::SeqCst) {
        log("Recording in progress. Waiting for meeting to end...");

        loop {
            if shutdown.load(Ordering::SeqCst) {
                log("Ctrl+C received. Will fetch available transcript...");
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(
                config.polling_interval_secs,
            ))
            .await;

            let status = recall_client
                .get_bot(&bot_id)
                .await
                .context("Failed to poll bot status")?;

            log(format!("Bot status: {}", status.description()));

            if status.is_fatal() {
                bail!(
                    "Recall.ai bot encountered a fatal error: {:?}",
                    status.sub_status
                );
            }

            // Step 7: Detect meeting end
            if status.is_done() {
                log("Meeting ended.");
                break;
            }
        }
    }

    // Fetch final transcript (with retries — transcript may take time to process)
    session_mgr.update_phase(SessionPhase::Fetching)?;
    log("Fetching transcript (may take a moment to process)...");

    let mut segments = Vec::new();
    let max_transcript_retries = 12; // Up to ~6 minutes of waiting
    for attempt in 0..max_transcript_retries {
        if shutdown.load(Ordering::SeqCst) {
            log("Ctrl+C received. Aborting transcript fetch.");
            break;
        }
        match recall_client.get_transcript(&bot_id).await {
            Ok(segs) if !segs.is_empty() => {
                segments = segs;
                break;
            }
            Ok(_) => {
                if attempt < max_transcript_retries - 1 {
                    log(format!(
                        "Transcript empty, waiting for processing... (attempt {}/{})",
                        attempt + 1,
                        max_transcript_retries
                    ));
                } else {
                    log("Transcript still empty after all retries.");
                }
            }
            Err(e) => {
                if attempt < max_transcript_retries - 1 {
                    log(format!("Transcript not ready yet ({}), retrying...", e));
                } else {
                    log(format!("Failed to fetch transcript after retries: {}", e));
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
    }

    if segments.is_empty() {
        bail!(
            "No transcript segments were captured. The transcript may still be processing — try resuming with:\n  sprout meeting run --resume {}",
            meeting_id
        );
    }

    let transcript = MeetingTranscript {
        meeting_id: meeting_id.clone(),
        platform: platform.to_string(),
        segments,
    };

    // Write transcript text file
    let transcript_path = session_mgr.staging_path().join("transcript.txt");
    std::fs::write(&transcript_path, transcript.to_text())
        .context("Failed to write transcript file")?;

    log(format!(
        "Transcript captured: {} segments, {} speakers, {:.0} minutes",
        transcript.segments.len(),
        transcript.speakers().len(),
        transcript.duration_minutes()
    ));

    // Step 8: Process artifacts
    process_artifacts(
        &config,
        &session_mgr,
        &transcript,
        &transcript_path,
        &artifact_types,
    )
    .await?;

    // Step 9: Publish artifacts
    publish_artifacts(&config, &session_mgr, &meeting_id, &artifact_types).await?;

    // Step 10: Cleanup and summary
    session_mgr.update_phase(SessionPhase::Done)?;
    session_mgr.cleanup()?;

    eprintln!();
    log("Meeting processing complete.");
    log(format!("Meeting ID: {}", meeting_id));
    log(format!("Bot ID:     {}", bot_id));
    log(format!("Output:     {}", config.output_dir));
    log(format!(
        "Export transcript: sprout meeting export --bot-id {}",
        bot_id
    ));

    Ok(())
}

/// Resume a previously interrupted session.
async fn resume_session(
    meeting_id: &str,
    config: &MeetingConfig,
    recall_client: &RecallClient,
    artifact_types: &[ArtifactType],
) -> Result<()> {
    let session_mgr = SessionManager::new(&PathBuf::from(&config.staging_dir), meeting_id);
    let state = session_mgr
        .load()?
        .with_context(|| format!("No session found for meeting ID: {}", meeting_id))?;

    log(format!(
        "Resuming session for meeting {} (phase: {})",
        meeting_id, state.phase
    ));

    match state.phase {
        SessionPhase::Connecting | SessionPhase::Recording => {
            // Check bot status and continue from there
            let status = recall_client
                .get_bot(&state.recall_bot_id)
                .await
                .context("Failed to check bot status for resume")?;

            if status.is_done() {
                log("Meeting has ended. Fetching transcript...");
                let segments = recall_client
                    .get_transcript(&state.recall_bot_id)
                    .await
                    .context("Failed to fetch transcript on resume")?;

                let transcript = MeetingTranscript {
                    meeting_id: meeting_id.to_string(),
                    platform: state.platform.clone(),
                    segments,
                };

                let transcript_path = session_mgr.staging_path().join("transcript.txt");
                std::fs::write(&transcript_path, transcript.to_text())
                    .context("Failed to write transcript file")?;

                process_artifacts(
                    config,
                    &session_mgr,
                    &transcript,
                    &transcript_path,
                    artifact_types,
                )
                .await?;

                publish_artifacts(config, &session_mgr, meeting_id, artifact_types).await?;
            } else {
                bail!(
                    "Bot is still in status '{}'. Wait for the meeting to end before resuming.",
                    status.description()
                );
            }
        }
        SessionPhase::Fetching | SessionPhase::Processing => {
            let transcript_path = session_mgr.staging_path().join("transcript.txt");

            let transcript = if !transcript_path.exists() {
                // Transcript file missing — re-fetch from recall.ai
                log("Transcript file not found locally. Fetching from Recall.ai...");
                let segments = recall_client
                    .get_transcript(&state.recall_bot_id)
                    .await
                    .context("Failed to fetch transcript from Recall.ai on resume")?;

                if segments.is_empty() {
                    bail!(
                        "Transcript is still empty at Recall.ai. Try again later with:\n  sprout meeting run --resume {}",
                        meeting_id
                    );
                }

                let transcript = MeetingTranscript {
                    meeting_id: meeting_id.to_string(),
                    platform: state.platform.clone(),
                    segments,
                };

                std::fs::write(&transcript_path, transcript.to_text())
                    .context("Failed to write transcript file")?;

                log(format!(
                    "Transcript fetched: {} segments, {} speakers, {:.0} minutes",
                    transcript.segments.len(),
                    transcript.speakers().len(),
                    transcript.duration_minutes()
                ));

                transcript
            } else {
                // Transcript file exists — reconstruct minimal transcript for processing
                log("Transcript file found. Resuming artifact processing...");
                MeetingTranscript {
                    meeting_id: meeting_id.to_string(),
                    platform: state.platform.clone(),
                    segments: Vec::new(), // We don't need segments, we have the text file
                }
            };

            process_artifacts(
                config,
                &session_mgr,
                &transcript,
                &transcript_path,
                artifact_types,
            )
            .await?;

            publish_artifacts(config, &session_mgr, meeting_id, artifact_types).await?;
        }
        SessionPhase::Publishing => {
            publish_artifacts(config, &session_mgr, meeting_id, artifact_types).await?;
        }
        SessionPhase::Done => {
            log("Session already completed. Nothing to resume.");
            return Ok(());
        }
    }

    session_mgr.update_phase(SessionPhase::Done)?;
    session_mgr.cleanup()?;
    log("Resumed session complete.");

    Ok(())
}

/// Process all artifacts using Claude CLI.
async fn process_artifacts(
    config: &MeetingConfig,
    session_mgr: &SessionManager,
    transcript: &MeetingTranscript,
    transcript_path: &std::path::Path,
    artifact_types: &[ArtifactType],
) -> Result<()> {
    session_mgr.update_phase(SessionPhase::Processing)?;
    log("Processing artifacts...");

    let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
    let mut handles = Vec::new();

    // Check which artifacts are already staged
    let state = session_mgr.load()?.unwrap();

    for artifact_type in artifact_types {
        let type_str = artifact_type.to_string();

        // Skip if already staged
        if state.artifacts_staged.contains(&type_str) {
            log(format!("Skipping {} (already staged)", type_str));
            continue;
        }

        // Render the prompt template
        let rendered_prompt = render_prompt(artifact_type, transcript, transcript_path)?;

        let output_path = session_mgr
            .staging_path()
            .join(format!("artifact_{}.{}", type_str, artifact_type.extension()));
        let transcript_path = transcript_path.to_path_buf();
        let claude_config = ClaudeConfig {
            claude_model: config.claude_model.clone(),
            claude_max_turns: config.claude_max_turns,
        };
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
            Ok::<String, anyhow::Error>(artifact_type.to_string())
        });

        handles.push(handle);
    }

    // Wait for all artifact processing to complete
    for handle in handles {
        let type_str = handle
            .await
            .context("Artifact processing task panicked")?
            .context("Artifact processing failed")?;
        session_mgr.mark_staged(&type_str)?;
    }

    Ok(())
}

/// Publish all staged artifacts.
async fn publish_artifacts(
    config: &MeetingConfig,
    session_mgr: &SessionManager,
    meeting_id: &str,
    artifact_types: &[ArtifactType],
) -> Result<()> {
    session_mgr.update_phase(SessionPhase::Publishing)?;
    log("Publishing artifacts...");

    let state = session_mgr.load()?.unwrap();

    for artifact_type in artifact_types {
        let type_str = artifact_type.to_string();

        // Skip if already published
        if state.artifacts_published.contains(&type_str) {
            log(format!("Skipping {} (already published)", type_str));
            continue;
        }

        let staged_path = session_mgr
            .staging_path()
            .join(format!("artifact_{}.{}", type_str, artifact_type.extension()));
        if !staged_path.exists() {
            log(format!(
                "WARNING: Staged artifact not found for {}, skipping publish.",
                type_str
            ));
            continue;
        }

        let published_path = publisher::publish_artifact(
            &type_str,
            artifact_type.extension(),
            &staged_path,
            &config.output_dir,
            meeting_id,
        )
        .await?;

        session_mgr.mark_published(&type_str)?;
        log(format!("Published: {}", published_path.display()));
    }

    Ok(())
}

/// Render a prompt template with meeting context.
pub(super) fn render_prompt(
    artifact_type: &ArtifactType,
    transcript: &MeetingTranscript,
    transcript_path: &std::path::Path,
) -> Result<String> {
    let template_str = match artifact_type.prompt_template() {
        Some(t) => t,
        None => return Ok(String::new()), // raw_transcript has no prompt
    };

    let mut tera = tera::Tera::default();
    tera.add_raw_template("prompt", template_str)
        .context("Failed to parse prompt template")?;

    let mut context = tera::Context::new();
    context.insert("transcript_path", &transcript_path.display().to_string());
    context.insert("output_path", "(will be provided by the system)");
    context.insert("platform", &transcript.platform);
    context.insert("meeting_id", &transcript.meeting_id);
    if transcript.segments.is_empty() {
        // Resumed session — segments unavailable, Claude will read the transcript file directly
        context.insert("speakers", "(see transcript file)");
        context.insert("duration_minutes", "unknown");
    } else {
        context.insert("speakers", &transcript.speakers().join(", "));
        context.insert(
            "duration_minutes",
            &format!("{:.0}", transcript.duration_minutes()),
        );
    }

    let rendered = tera
        .render("prompt", &context)
        .context("Failed to render prompt template")?;

    Ok(rendered)
}
