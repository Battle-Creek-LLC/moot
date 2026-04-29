//! Filesystem publisher for meeting artifacts.
//!
//! Copies staged artifacts to their final output location.

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};

/// Publish an artifact from the staging directory to the output directory.
///
/// Returns the final path of the published artifact.
pub async fn publish_artifact(
    artifact_type: &str,
    extension: &str,
    staged_path: &Path,
    output_dir: &str,
    meeting_id: &str,
) -> Result<PathBuf> {
    let content = std::fs::read_to_string(staged_path)
        .with_context(|| format!("Failed to read staged artifact: {}", staged_path.display()))?;

    let timestamp = Utc::now().format("%Y-%m-%d_%H%M");
    let filename = format!("{}_{}.{}", timestamp, artifact_type, extension);
    let dest_dir = PathBuf::from(output_dir).join(meeting_id);

    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("Failed to create output directory: {}", dest_dir.display()))?;

    let dest_path = dest_dir.join(&filename);
    std::fs::write(&dest_path, &content).with_context(|| {
        format!(
            "Failed to write published artifact: {}",
            dest_path.display()
        )
    })?;

    Ok(dest_path)
}
