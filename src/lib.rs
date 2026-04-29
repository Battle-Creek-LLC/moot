//! Moot library crate. Re-exports the modules used by the binary so
//! integration tests in `tests/` can import them.

pub mod cli;
pub mod error;
pub mod logging;
pub mod paths;
pub mod secrets;
