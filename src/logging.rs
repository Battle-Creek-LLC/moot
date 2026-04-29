//! `tracing-subscriber` setup.
//!
//! Honors `MOOT_LOG` (env filter), `MOOT_NO_COLOR` (disable ANSI), and the
//! global `--json` flag (structured stderr).

use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;

/// Initialize the global tracing subscriber.
///
/// Idempotent: silently ignores subsequent calls (useful in tests).
pub fn init(json: bool, verbose: u8) {
    let default_level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    let filter = EnvFilter::try_from_env("MOOT_LOG")
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let no_color = std::env::var_os("MOOT_NO_COLOR").is_some()
        || !std::io::stderr().is_terminal();

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);

    if json {
        let _ = builder.json().try_init();
    } else {
        let _ = builder.with_ansi(!no_color).try_init();
    }
}
