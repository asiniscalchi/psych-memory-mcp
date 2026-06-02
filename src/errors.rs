//! Error types for the psych-memory wrapper.
//!
//! Story 0 only needs backend/transport errors. Story 1 will extend this enum
//! with the `store_fact` validation variants (empty statement, missing journal
//! ref, etc.).

#[derive(Debug, thiserror::Error)]
pub enum PsychMemoryError {
    /// The request to the underlying memory-service could not be completed
    /// (connection refused, timeout, malformed response, ...).
    #[error("memory service request failed: {0}")]
    Backend(String),

    /// The memory-service was reached but reported a non-success result.
    #[error("memory service returned an error: {0}")]
    BackendStatus(String),
}
