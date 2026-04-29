//! Transcript shared types + multi-format parsers.
//!
//! - `parse_jsonl`: one utterance per line, our own export shape (also the
//!   shape Recall.ai uses internally once we run their entries through
//!   [`from_segments`]).
//! - `parse_vtt`: WebVTT.
//! - `parse_srt`: SubRip.
//! - `parse_txt`: plain text, optional `Speaker:` prefix.
//! - `render_md`: human-readable markdown with speaker labels and `mm:ss` timestamps.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::recall::Segment;

/// One utterance, normalized across all input formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utterance {
    /// Zero-based index into the meeting.
    pub idx: usize,
    pub speaker: String,
    /// Milliseconds from meeting start.
    pub ts_offset_ms: i64,
    pub text: String,
}

impl Utterance {
    pub fn from_segment(idx: usize, seg: &Segment) -> Self {
        Utterance {
            idx,
            speaker: seg.speaker.clone(),
            ts_offset_ms: (seg.start * 1000.0).round() as i64,
            text: seg.text.clone(),
        }
    }
}

pub fn from_segments(segments: &[Segment]) -> Vec<Utterance> {
    segments
        .iter()
        .enumerate()
        .map(|(i, s)| Utterance::from_segment(i, s))
        .collect()
}

/// One utterance per line, JSON-encoded. Used as both the in-DB
/// `transcript_jsonl` column and the export-on-disk `transcript.jsonl`.
pub fn render_jsonl(utterances: &[Utterance]) -> Result<String> {
    let mut out = String::new();
    for u in utterances {
        out.push_str(&serde_json::to_string(u)?);
        out.push('\n');
    }
    Ok(out)
}

/// Human-readable markdown with `**Speaker** (mm:ss): text` lines.
pub fn render_md(title: &str, platform: Option<&str>, started_iso: Option<&str>, duration_secs: Option<i64>, utterances: &[Utterance]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {title}\n"));
    let mut subs = Vec::new();
    if let Some(s) = started_iso {
        subs.push(s.to_string());
    }
    if let Some(d) = duration_secs {
        subs.push(format_duration(d));
    }
    if let Some(p) = platform {
        subs.push(platform_label(p).to_string());
    }
    if !subs.is_empty() {
        out.push_str(&format!("*{}*\n", subs.join(" · ")));
    }
    out.push('\n');
    for u in utterances {
        out.push_str(&format!(
            "**{}** ({}): {}\n",
            u.speaker,
            mm_ss(u.ts_offset_ms),
            u.text.trim()
        ));
    }
    out
}

fn platform_label(p: &str) -> &str {
    match p {
        "meet" => "Google Meet",
        "teams" => "Microsoft Teams",
        "zoom" => "Zoom",
        _ => "Meeting",
    }
}

fn mm_ss(ms: i64) -> String {
    let total_secs = (ms / 1000).max(0);
    let m = total_secs / 60;
    let s = total_secs % 60;
    format!("{m:02}:{s:02}")
}

fn format_duration(secs: i64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    if m == 0 {
        format!("{s}s")
    } else if s == 0 {
        format!("{m}m")
    } else {
        format!("{m}m{s}s")
    }
}

// ---- parsers --------------------------------------------------------------

pub fn parse_jsonl(body: &str) -> Result<Vec<Utterance>> {
    let mut out = Vec::new();
    for (lineno, line) in body.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let u: Utterance = serde_json::from_str(line)
            .map_err(|e| Error::Fs(format!("jsonl line {}: {e}", lineno + 1)))?;
        out.push(u);
    }
    if out.is_empty() {
        return Err(Error::Cli("jsonl input contained no utterances".into()));
    }
    // Re-index in case the input was sparse.
    for (i, u) in out.iter_mut().enumerate() {
        u.idx = i;
    }
    Ok(out)
}

/// WebVTT — `00:00:01.200 --> 00:00:03.500` cue line followed by one or more
/// content lines. A leading `Speaker:` on the content becomes the speaker.
pub fn parse_vtt(body: &str) -> Result<Vec<Utterance>> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;
    // Skip header (WEBVTT and optional NOTE blocks).
    while i < lines.len() && !lines[i].contains("-->") {
        i += 1;
    }
    while i < lines.len() {
        let cue = lines[i];
        if let Some((start, _end)) = parse_vtt_cue(cue) {
            i += 1;
            let mut buf = String::new();
            while i < lines.len() && !lines[i].trim().is_empty() && !lines[i].contains("-->") {
                if !buf.is_empty() {
                    buf.push(' ');
                }
                buf.push_str(lines[i].trim());
                i += 1;
            }
            let (speaker, text) = split_speaker(&buf);
            out.push(Utterance {
                idx,
                speaker,
                ts_offset_ms: start,
                text,
            });
            idx += 1;
        } else {
            i += 1;
        }
    }
    if out.is_empty() {
        return Err(Error::Cli("VTT input contained no cues".into()));
    }
    Ok(out)
}

fn parse_vtt_cue(line: &str) -> Option<(i64, i64)> {
    let (a, b) = line.split_once("-->")?;
    Some((parse_vtt_time(a.trim())?, parse_vtt_time(b.trim())?))
}

/// Accepts `HH:MM:SS.mmm`, `MM:SS.mmm`, `HH:MM:SS,mmm`, `MM:SS,mmm`.
fn parse_vtt_time(s: &str) -> Option<i64> {
    let s = s.replace(',', ".");
    // Strip cue settings (everything after the first space).
    let s = s.split_whitespace().next()?;
    let mut secs = 0i64;
    let mut ms = 0i64;
    let (whole, frac) = s.split_once('.').unwrap_or((s, "0"));
    let parts: Vec<&str> = whole.split(':').collect();
    let nums: Result<Vec<i64>, _> = parts.iter().map(|p| p.parse::<i64>()).collect();
    let nums = nums.ok()?;
    match nums.len() {
        2 => {
            secs += nums[0] * 60;
            secs += nums[1];
        }
        3 => {
            secs += nums[0] * 3600;
            secs += nums[1] * 60;
            secs += nums[2];
        }
        _ => return None,
    }
    let pad: String = format!("{:0<3}", &frac[..frac.len().min(3)]);
    ms = pad.parse::<i64>().ok().unwrap_or(ms);
    Some(secs * 1000 + ms)
}

/// SubRip — index line, time line, content line(s), blank.
pub fn parse_srt(body: &str) -> Result<Vec<Utterance>> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        // Skip blank lines.
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        // SRT counter line — optional but usually present.
        if lines[i].trim().chars().all(|c| c.is_ascii_digit()) {
            i += 1;
            if i >= lines.len() {
                break;
            }
        }
        // Time line.
        let cue = lines[i];
        i += 1;
        let Some((start, _)) = parse_vtt_cue(cue) else {
            continue;
        };
        let mut buf = String::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(lines[i].trim());
            i += 1;
        }
        let (speaker, text) = split_speaker(&buf);
        out.push(Utterance {
            idx,
            speaker,
            ts_offset_ms: start,
            text,
        });
        idx += 1;
    }
    if out.is_empty() {
        return Err(Error::Cli("SRT input contained no cues".into()));
    }
    Ok(out)
}

/// Plain text — one utterance per line. Optional `Speaker:` prefix.
/// Timestamps are assigned synthetically (no offset).
pub fn parse_txt(body: &str) -> Result<Vec<Utterance>> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (speaker, text) = split_speaker(trimmed);
        out.push(Utterance {
            idx,
            speaker,
            ts_offset_ms: 0,
            text,
        });
        idx += 1;
    }
    if out.is_empty() {
        return Err(Error::Cli("text input contained no lines".into()));
    }
    Ok(out)
}

fn split_speaker(line: &str) -> (String, String) {
    if let Some((head, tail)) = line.split_once(':') {
        let head = head.trim();
        let tail = tail.trim();
        // Heuristic: speaker labels are short and don't contain whitespace
        // beyond a couple words, no time digits, no urls.
        if !head.is_empty()
            && head.len() <= 60
            && !head.contains('/')
            && !head.contains("http")
            && head.chars().filter(|c| c.is_whitespace()).count() <= 3
        {
            return (head.to_string(), tail.to_string());
        }
    }
    ("Speaker".to_string(), line.to_string())
}

/// Auto-detect format from the path extension.
pub fn parse_by_extension(path: &std::path::Path, body: &str) -> Result<Vec<Utterance>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("jsonl") => parse_jsonl(body),
        Some("vtt") => parse_vtt(body),
        Some("srt") => parse_srt(body),
        Some("txt") | None => parse_txt(body),
        Some(other) => Err(Error::Cli(format!("unsupported transcript format `.{other}`"))),
    }
}

pub fn unique_speakers(utterances: &[Utterance]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for u in utterances {
        seen.insert(u.speaker.clone());
    }
    seen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vtt() {
        let body = "WEBVTT\n\n00:00:01.000 --> 00:00:02.500\nAlice: Hi everyone\n\n00:00:02.700 --> 00:00:04.000\nBob: Hey\n";
        let parsed = parse_vtt(body).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].speaker, "Alice");
        assert_eq!(parsed[0].text, "Hi everyone");
        assert_eq!(parsed[0].ts_offset_ms, 1000);
        assert_eq!(parsed[1].speaker, "Bob");
    }

    #[test]
    fn parses_srt() {
        let body = "1\n00:00:01,000 --> 00:00:02,500\nAlice: Hi\n\n2\n00:00:02,700 --> 00:00:04,000\nBob: Hey\n";
        let parsed = parse_srt(body).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].speaker, "Bob");
        assert_eq!(parsed[1].ts_offset_ms, 2700);
    }

    #[test]
    fn parses_txt_with_speaker() {
        let parsed = parse_txt("Alice: hello\nBob: hi\nno-prefix line\n").unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].speaker, "Alice");
        assert_eq!(parsed[2].speaker, "Speaker");
        assert_eq!(parsed[2].text, "no-prefix line");
    }

    #[test]
    fn jsonl_round_trip() {
        let utterances = vec![Utterance {
            idx: 0,
            speaker: "Alice".into(),
            ts_offset_ms: 1200,
            text: "Hi".into(),
        }];
        let body = render_jsonl(&utterances).unwrap();
        let back = parse_jsonl(&body).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].text, "Hi");
    }

    #[test]
    fn render_md_includes_speakers_and_timestamps() {
        let utterances = vec![
            Utterance { idx: 0, speaker: "Alice".into(), ts_offset_ms: 1000, text: "Hi.".into() },
            Utterance { idx: 1, speaker: "Bob".into(),   ts_offset_ms: 65000, text: "Hey.".into() },
        ];
        let md = render_md("Test", Some("meet"), Some("2026-04-29T15:00:00Z"), Some(2843), &utterances);
        assert!(md.contains("# Test"));
        assert!(md.contains("Google Meet"));
        assert!(md.contains("**Alice** (00:01): Hi."));
        assert!(md.contains("**Bob** (01:05): Hey."));
    }
}
