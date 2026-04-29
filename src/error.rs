//! Single error enum used by every Moot command.
//!
//! Variants map to SPEC §10 exit codes via [`Error::exit_code`].

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("keychain error: {0}")]
    Keychain(String),

    #[error("Recall.ai error: {0}")]
    Recall(String),

    #[error("database error: {0}")]
    Db(String),

    #[error("notes generation error: {0}")]
    Notes(String),

    #[error("filesystem error: {0}")]
    Fs(String),

    #[error("cli error: {0}")]
    Cli(String),
}

impl Error {
    /// Exit code per SPEC §10. Stable across versions.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Cli(_) => 2,
            Error::Config(_) | Error::Keychain(_) => 3,
            Error::Recall(_) | Error::Notes(_) => 4,
            Error::Db(_) => 5,
            Error::Fs(_) => 1,
        }
    }

    /// Stable machine-readable code for `--json` error output.
    pub fn code_str(&self) -> &'static str {
        match self {
            Error::Config(_) => "config",
            Error::Keychain(_) => "keychain",
            Error::Recall(_) => "recall",
            Error::Db(_) => "db",
            Error::Notes(_) => "notes",
            Error::Fs(_) => "fs",
            Error::Cli(_) => "cli",
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
