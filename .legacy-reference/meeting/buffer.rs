//! JSONL append-only transcript buffer.
//!
//! Provides crash-safe buffering of transcript segments to a JSONL file.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::models::Segment;

/// Append-only JSONL transcript buffer.
///
/// Currently unused — forward infrastructure for real-time transcript buffering
/// when webhook-based delivery is implemented. The current flow fetches the full
/// transcript after the meeting ends via Recall.ai's API.
#[allow(dead_code)]
pub struct TranscriptBuffer {
    path: PathBuf,
    file: File,
    count: usize,
}

#[allow(dead_code)]
impl TranscriptBuffer {
    /// Create a new buffer at the given path. Creates the file if it doesn't exist.
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create buffer directory: {}", parent.display())
            })?;
        }

        // Count existing lines if the file already exists
        let count = if path.exists() {
            let file = File::open(path)
                .with_context(|| format!("Failed to open buffer file: {}", path.display()))?;
            BufReader::new(file).lines().count()
        } else {
            0
        };

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| {
                format!("Failed to open buffer file for writing: {}", path.display())
            })?;

        Ok(TranscriptBuffer {
            path: path.to_path_buf(),
            file,
            count,
        })
    }

    /// Append one segment as a JSON line and flush.
    pub fn append(&mut self, segment: &Segment) -> Result<()> {
        let json = serde_json::to_string(segment).context("Failed to serialize segment")?;
        writeln!(self.file, "{}", json).context("Failed to write segment to buffer")?;
        self.file.flush().context("Failed to flush buffer")?;
        self.count += 1;
        Ok(())
    }

    /// Read all segments from the buffer. Skips truncated last line.
    pub fn read_all(&self) -> Result<Vec<Segment>> {
        let file = File::open(&self.path)
            .with_context(|| format!("Failed to open buffer file: {}", self.path.display()))?;
        let reader = BufReader::new(file);
        let mut segments = Vec::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break, // Truncated last line from crash
            };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Segment>(&line) {
                Ok(seg) => segments.push(seg),
                Err(_) => {
                    // Skip truncated/corrupt lines (crash recovery)
                    tracing::debug!("Skipping unparseable buffer line");
                }
            }
        }

        Ok(segments)
    }

    /// Number of segments currently buffered.
    pub fn segment_count(&self) -> usize {
        self.count
    }
}
