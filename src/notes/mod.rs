//! Notes generation. Renders a Tera template against the transcript and
//! shells out to the Claude CLI (`claude --print`).
//!
//! See SPEC §7. Best-effort: failures bubble up as `Error::Notes` and the
//! caller decides whether to fail or just leave `notes_md` null.

use std::path::{Path, PathBuf};

use tera::{Context as TeraContext, Tera};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::{Error, Result};

const DEFAULT_TEMPLATE: &str = include_str!("../../prompts/notes.md.tera");
const TEMPLATE_NAME: &str = "notes";

/// Inputs to the notes prompt template.
#[derive(Debug, Clone)]
pub struct NotesInput<'a> {
    pub title: &'a str,
    pub platform: &'a str,
    pub meeting_id: &'a str,
    pub speakers: &'a [String],
    pub transcript_md: &'a str,
}

/// Render the prompt template. Returns the body that will be piped to
/// `claude --print`.
pub fn render_prompt(template_src: &str, input: &NotesInput<'_>) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template(TEMPLATE_NAME, template_src)?;
    let mut ctx = TeraContext::new();
    ctx.insert("title", input.title);
    ctx.insert("platform", input.platform);
    ctx.insert("meeting_id", input.meeting_id);
    ctx.insert("speakers", &input.speakers.join(", "));
    ctx.insert("transcript", input.transcript_md);
    Ok(tera.render(TEMPLATE_NAME, &ctx)?)
}

/// Load a template — either the embedded default, or read from disk if a
/// path is provided.
pub fn load_template(prompt_path: Option<&Path>) -> Result<String> {
    match prompt_path {
        None => Ok(DEFAULT_TEMPLATE.to_string()),
        Some(p) => Ok(std::fs::read_to_string(p)?),
    }
}

/// Generate notes for the given input. Returns the markdown body.
///
/// Pipes the rendered prompt to `claude --print` over stdin so we don't have
/// to worry about shell argument length / escaping.
pub async fn generate_notes(template_src: &str, input: &NotesInput<'_>) -> Result<String> {
    let prompt = render_prompt(template_src, input)?;

    let mut cmd = Command::new("claude");
    // `claude` checks CLAUDECODE to detect when invoked from inside a Claude
    // Code session. Clear it so an interactive Code session can still run
    // `moot run --notes` against the upstream CLI.
    cmd.env_remove("CLAUDECODE");
    cmd.arg("--print").arg("--output-format").arg("text");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Notes(format!("failed to spawn `claude`: {e}. Is the Claude CLI installed?")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|e| Error::Notes(format!("failed to write prompt to claude stdin: {e}")))?;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| Error::Notes(format!("failed to wait for claude: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Notes(format!(
            "claude exited with {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let body = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if body.is_empty() {
        return Err(Error::Notes("claude produced no output".into()));
    }
    Ok(body)
}

/// Convenience wrapper used by the `notes` / `run` / `fetch` / `import`
/// commands. Loads the template (default or override path), renders, and
/// shells out.
pub async fn generate(
    prompt_path: Option<PathBuf>,
    input: &NotesInput<'_>,
) -> Result<(String, String)> {
    let template = load_template(prompt_path.as_deref())?;
    let body = generate_notes(&template, input).await?;
    Ok((body, template))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_default_template() {
        let speakers = vec!["Alice".to_string(), "Bob".to_string()];
        let input = NotesInput {
            title: "Test",
            platform: "meet",
            meeting_id: "01ABC",
            speakers: &speakers,
            transcript_md: "**Alice** (00:00): hi\n",
        };
        let out = render_prompt(DEFAULT_TEMPLATE, &input).unwrap();
        assert!(out.contains("Test"));
        assert!(out.contains("meet"));
        assert!(out.contains("Alice, Bob"));
        assert!(out.contains("**Alice** (00:00): hi"));
    }
}
