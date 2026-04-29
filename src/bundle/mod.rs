//! Bundle writer for `moot export`.
//!
//! Layout per SPEC §5.5 / §9: `<out>/<slug>/{meeting.toml, transcript.jsonl,
//! transcript.md, notes.md}`. `notes.md` is omitted when the meeting has no
//! notes. The same shape can be streamed as a tar archive for `--out -`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::DateTime;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::store::Meeting;

/// What to include in the bundle.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Format {
    /// `meeting.toml` + `transcript.jsonl` + `transcript.md` + `notes.md`.
    All,
    /// `transcript.jsonl` only.
    Jsonl,
    /// `transcript.md` + `notes.md` only.
    Md,
}

/// In-memory representation of the files we'd write. Used by both directory
/// and tar-stream paths so format/layout stays in one place.
pub struct Bundle {
    pub slug: String,
    pub files: Vec<(String, Vec<u8>)>,
}

impl Bundle {
    pub fn build(meeting: &Meeting, tags: &[String], format: Format) -> Result<Bundle> {
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();

        let include_meta = matches!(format, Format::All);
        let include_jsonl = matches!(format, Format::All | Format::Jsonl);
        let include_md = matches!(format, Format::All | Format::Md);
        let include_notes = matches!(format, Format::All | Format::Md);

        if include_meta {
            let toml = render_meeting_toml(meeting, tags)?;
            files.push(("meeting.toml".into(), toml.into_bytes()));
        }
        if include_jsonl {
            let jsonl = meeting.transcript_jsonl.clone().unwrap_or_default();
            files.push(("transcript.jsonl".into(), jsonl.into_bytes()));
        }
        if include_md {
            let md = meeting.transcript_md.clone().unwrap_or_default();
            files.push(("transcript.md".into(), md.into_bytes()));
        }
        if include_notes {
            if let Some(notes) = &meeting.notes_md {
                files.push(("notes.md".into(), notes.clone().into_bytes()));
            }
        }

        Ok(Bundle {
            slug: meeting.slug.clone(),
            files,
        })
    }

    /// Write the bundle into `<out_dir>/<slug>/`. Refuses to overwrite an
    /// existing directory unless `force` is set.
    pub fn write_to_dir(&self, out_dir: &Path, force: bool) -> Result<PathBuf> {
        let target = out_dir.join(&self.slug);
        if target.exists() {
            if !force {
                return Err(Error::Cli(format!(
                    "{} already exists. Pass --force to overwrite.",
                    target.display()
                )));
            }
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::create_dir_all(&target)?;
        for (name, contents) in &self.files {
            let p = target.join(name);
            std::fs::write(&p, contents)?;
        }
        Ok(target)
    }

    /// Stream the bundle as a tar archive into `writer`. Each entry is
    /// rooted at `<slug>/`.
    pub fn write_tar<W: Write>(&self, writer: W) -> Result<()> {
        let mut builder = tar::Builder::new(writer);
        for (name, contents) in &self.files {
            let path = format!("{}/{}", self.slug, name);
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            builder
                .append_data(&mut header, &path, contents.as_slice())
                .map_err(|e| Error::Fs(format!("tar append failed: {e}")))?;
        }
        builder
            .finish()
            .map_err(|e| Error::Fs(format!("tar finish failed: {e}")))?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct MeetingToml {
    id: String,
    slug: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recall_bot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_secs: Option<i64>,
    status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    participants: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

fn render_meeting_toml(m: &Meeting, tags: &[String]) -> Result<String> {
    let participants = parse_participants(m.participants_json.as_deref());

    let toml = MeetingToml {
        id: m.id.clone(),
        slug: m.slug.clone(),
        title: m.title.clone(),
        platform: m.platform.clone(),
        url: m.url.clone(),
        recall_bot_id: m.recall_bot_id.clone(),
        language: m.language.clone(),
        started_at: m.started_at.map(format_iso),
        ended_at: m.ended_at.map(format_iso),
        duration_secs: m.duration_secs,
        status: m.status.as_str().to_string(),
        participants,
        tags: tags.to_vec(),
    };
    Ok(toml::to_string_pretty(&toml).map_err(|e| Error::Fs(format!("toml: {e}")))?)
}

fn parse_participants(blob: Option<&str>) -> Vec<String> {
    let Some(blob) = blob else { return Vec::new() };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(blob) else {
        return Vec::new();
    };
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                serde_json::Value::Object(map) => map
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn format_iso(ms: i64) -> String {
    let dt = DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now);
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Read all bytes from a tar entry — small helper used in tests / round-trip.
pub fn slurp<R: Read>(mut r: R) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    r.read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MeetingStatus;

    fn fake_meeting() -> Meeting {
        Meeting {
            id: "01ABC".into(),
            slug: "test-2026-04-29".into(),
            title: "Test".into(),
            platform: Some("meet".into()),
            url: Some("https://meet.google.com/x".into()),
            recall_bot_id: None,
            language: Some("en".into()),
            started_at: Some(1714406400000),
            ended_at: Some(1714409200000),
            duration_secs: Some(2800),
            status: MeetingStatus::Active,
            transcript_jsonl: Some("{\"idx\":0,\"speaker\":\"Alice\",\"ts_offset_ms\":0,\"text\":\"hi\"}\n".into()),
            transcript_md: Some("# Test\n\n**Alice** (00:00): hi\n".into()),
            notes_md: Some("# Notes\n\nAll good.\n".into()),
            notes_prompt: None,
            participants_json: Some("[\"Alice\",\"Bob\"]".into()),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn build_all_includes_four_files() {
        let m = fake_meeting();
        let tags = vec!["staff".to_string()];
        let b = Bundle::build(&m, &tags, Format::All).unwrap();
        let names: Vec<&str> = b.files.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"meeting.toml"));
        assert!(names.contains(&"transcript.jsonl"));
        assert!(names.contains(&"transcript.md"));
        assert!(names.contains(&"notes.md"));
    }

    #[test]
    fn build_md_skips_jsonl_and_meta() {
        let m = fake_meeting();
        let b = Bundle::build(&m, &[], Format::Md).unwrap();
        let names: Vec<&str> = b.files.iter().map(|(n, _)| n.as_str()).collect();
        assert!(!names.contains(&"meeting.toml"));
        assert!(!names.contains(&"transcript.jsonl"));
        assert!(names.contains(&"transcript.md"));
        assert!(names.contains(&"notes.md"));
    }

    #[test]
    fn meeting_toml_has_participants_and_tags() {
        let m = fake_meeting();
        let toml = render_meeting_toml(&m, &["staff".into(), "weekly".into()]).unwrap();
        eprintln!("---toml---\n{toml}\n---end---");
        // toml v0.8 may render arrays inline OR as multi-line tables; check
        // for the values, not the exact whitespace.
        assert!(toml.contains("\"Alice\""));
        assert!(toml.contains("\"Bob\""));
        assert!(toml.contains("\"staff\""));
        assert!(toml.contains("\"weekly\""));
        assert!(toml.contains("status = \"active\""));
    }

    #[test]
    fn write_to_dir_refuses_existing_without_force() {
        let m = fake_meeting();
        let b = Bundle::build(&m, &[], Format::All).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        b.write_to_dir(tmp.path(), false).unwrap();
        let err = b.write_to_dir(tmp.path(), false).unwrap_err();
        assert!(err.to_string().contains("already exists"));
        // With force it succeeds.
        b.write_to_dir(tmp.path(), true).unwrap();
    }

    #[test]
    fn tar_roundtrip() {
        let m = fake_meeting();
        let b = Bundle::build(&m, &[], Format::All).unwrap();
        let mut buf = Vec::new();
        b.write_tar(&mut buf).unwrap();

        let mut archive = tar::Archive::new(buf.as_slice());
        let mut found = std::collections::HashSet::new();
        for entry in archive.entries().unwrap() {
            let entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            found.insert(path);
        }
        assert!(found.contains("test-2026-04-29/meeting.toml"));
        assert!(found.contains("test-2026-04-29/transcript.jsonl"));
        assert!(found.contains("test-2026-04-29/transcript.md"));
        assert!(found.contains("test-2026-04-29/notes.md"));
    }
}
