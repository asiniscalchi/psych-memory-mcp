//! Error types for the psych-memory wrapper.

/// A domain-level validation failure for an epistemic tool input.
///
/// Each variant maps to a stable `error_code` (returned to the MCP client in
/// the structured `{ ok: false, error_code, message }` shape) and a
/// human-readable message via `Display`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("Journal fact requires a source excerpt.")]
    EmptySourceExcerpt,
    #[error("Journal fact requires a normalized statement.")]
    EmptyNormalizedStatement,
    #[error("Journal fact requires a usable JournalEntryRef.")]
    MissingJournalEntryRef,
    #[error("JournalEntryRef.entry_id must not be empty.")]
    EmptyEntryId,
    #[error("JournalEntryRef.entry_date must be YYYY-MM-DD.")]
    InvalidJournalEntryDate,
    #[error("event_date must be YYYY-MM-DD when provided.")]
    InvalidEventDate,
    #[error("JournalEntryRef.char_start cannot be greater than char_end.")]
    InvalidCharRange,
}

impl ValidationError {
    /// Stable, machine-readable error code for this failure.
    pub fn error_code(&self) -> &'static str {
        match self {
            ValidationError::EmptySourceExcerpt => "empty_source_excerpt",
            ValidationError::EmptyNormalizedStatement => "empty_normalized_statement",
            ValidationError::MissingJournalEntryRef => "missing_journal_entry_ref",
            ValidationError::EmptyEntryId => "empty_entry_id",
            ValidationError::InvalidJournalEntryDate => "invalid_journal_entry_date",
            ValidationError::InvalidEventDate => "invalid_event_date",
            ValidationError::InvalidCharRange => "invalid_char_range",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PsychMemoryError {
    /// Input rejected by domain validation. Surfaced to the client as a
    /// structured tool error, not a protocol-level failure.
    #[error("validation failed: {0}")]
    Validation(#[from] ValidationError),

    /// The request to the underlying memory-service could not be completed
    /// (connection refused, timeout, malformed response, ...).
    #[error("memory service request failed: {0}")]
    Backend(String),

    /// The memory-service was reached but reported a non-success result.
    #[error("memory service returned an error: {0}")]
    BackendStatus(String),
}
