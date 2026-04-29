//! Artifact type listing and template display.

use anyhow::{Result, bail};

use super::models::ArtifactType;

/// List all available artifact types.
pub async fn artifacts_list() -> Result<()> {
    println!("Available artifact types:");
    println!();

    for artifact in ArtifactType::all() {
        println!("  {:<16} {}", artifact, artifact.description());
    }

    println!();
    println!("Use `sprout meeting artifacts-show <type>` to see the prompt template.");

    Ok(())
}

/// Show the prompt template for an artifact type.
pub async fn artifacts_show(name: &str) -> Result<()> {
    let artifact_type: ArtifactType = name.parse()?;

    match artifact_type.prompt_template() {
        Some(template) => {
            println!("Prompt template for '{}':", artifact_type);
            println!("----------------------------------------");
            println!("{}", template);
        }
        None => {
            bail!(
                "Artifact type '{}' does not use a prompt template (it copies the raw transcript).",
                artifact_type
            );
        }
    }

    Ok(())
}
