//! Moot library crate. Re-exports the modules used by the binary so
//! integration tests in `tests/` can import them.

pub mod bundle;
pub mod cli;
pub mod error;
pub mod logging;
pub mod notes;
pub mod paths;
pub mod recall;
pub mod search;
pub mod secrets;
pub mod session;
pub mod store;
pub mod transcript;
pub mod util;
