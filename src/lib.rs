//! psych-memory-mcp library surface.
//!
//! The binary (`main.rs`) is a thin entrypoint; the wrapper's logic lives here
//! so it can be exercised by integration tests in `tests/`.

pub mod backend;
pub mod config;
pub mod errors;
pub mod model;
pub mod server;
