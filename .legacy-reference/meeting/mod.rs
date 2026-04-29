//! Meeting transcription and artifact generation commands
//!
//! Joins meetings via Recall.ai, captures transcripts, and processes
//! them into structured artifacts using the Claude Code CLI.

mod artifacts;
mod buffer;
mod claude;
mod clean;
mod config;
mod export;
mod models;
mod publisher;
mod recall;
mod run;
mod session;

// Re-export public command functions
pub use artifacts::{artifacts_list, artifacts_show};
pub use clean::clean;
pub use export::export_transcript;
pub use run::run;

/// Log a meeting status message to stderr.
fn log(msg: impl std::fmt::Display) {
    eprintln!("[meeting] {}", msg);
}
