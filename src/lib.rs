//! psych-memory-mcp library surface.
//!
//! The binary (`main.rs`) is a thin entrypoint; the wrapper's logic lives here
//! so it can be exercised by integration tests in `tests/`.

pub mod backend;
pub mod config;
pub mod errors;
pub mod evidence;
pub mod fact_id;
pub mod hashing;
pub mod interpretation_id;
pub mod mapping;
pub mod model;
pub mod occurrence_id;
pub mod pattern_id;
pub mod pattern_validation;
pub mod resolution;
pub mod server;
pub mod validators;
