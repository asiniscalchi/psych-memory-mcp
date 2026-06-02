//! Error types for the psych-memory wrapper.

/// A domain-level validation failure for an epistemic tool input.
///
/// Each variant maps to a stable `error_code` (returned to the MCP client in
/// the structured `{ ok: false, error_code, message }` shape) and a
/// human-readable message via `Display`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
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

    // --- Interpretation (Story 2) ---
    #[error("Interpretation requires a hypothesis.")]
    EmptyHypothesis,
    #[error("Interpretation requires at least one supporting fact_id.")]
    MissingSupportingFacts,
    #[error("supported_by_fact_ids must not contain empty values.")]
    EmptySupportingFactId,
    #[error("contradicted_by_fact_ids must not contain empty values.")]
    EmptyContradictedFactId,
    #[error("confidence must be finite and between 0.0 and 1.0.")]
    InvalidConfidence,
    #[error("confidence > {threshold} requires at least {min} supporting facts.")]
    OverconfidentInterpretation { threshold: f32, min: usize },
    #[error("Story 2 only allows status = candidate.")]
    UnsupportedInterpretationStatus,
    #[error("Interpretation requires a falsification question.")]
    MissingFalsificationQuestion,
    #[error("review_due must be YYYY-MM-DD when provided.")]
    InvalidReviewDue,
    #[error("One or more supporting fact_ids could not be resolved.")]
    UnknownSupportingFact,
    #[error("More than one fact matched the same fact_id.")]
    AmbiguousSupportingFact,
    #[error("Resolved supporting memory is not a valid journal fact.")]
    InvalidSupportingFact,
    #[error("Resolved fact metadata.fact_id does not match the requested fact_id.")]
    SupportingFactIdMismatch,

    // --- Pattern seed (Story 3) ---
    #[error("Pattern seed requires a name.")]
    EmptyPatternName,
    #[error(
        "Pattern slug must match ^[a-z][a-z0-9_]*$ and must not contain consecutive underscores."
    )]
    InvalidPatternSlug,
    #[error("Pattern seed requires a description.")]
    EmptyPatternDescription,
    #[error("Pattern seed requires at least one marker.")]
    MissingPatternMarkers,
    #[error("Pattern markers must not contain empty values.")]
    EmptyPatternMarker,
    #[error("Pattern seed requires at least one counter-marker.")]
    MissingPatternCounterMarkers,
    #[error("Pattern counter-markers must not contain empty values.")]
    EmptyPatternCounterMarker,
    #[error(
        "Pattern seed must not claim that the user has, is, or actively expresses the pattern."
    )]
    PatternIdentityClaim,
    #[error("Pattern aliases must not be empty after normalization.")]
    EmptyPatternAlias,
    #[error("Pattern aliases must be unique after normalization.")]
    DuplicatePatternAlias,
    #[error("Pattern alias must not duplicate the slug.")]
    AliasEqualsSlug,
    #[error("More than one pattern seed matched the same pattern_id.")]
    AmbiguousPatternSeed,
    #[error("Memories tagged with this pattern_id are not valid pattern seeds.")]
    InvalidPatternSeedMatch,
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
            ValidationError::EmptyHypothesis => "empty_hypothesis",
            ValidationError::MissingSupportingFacts => "missing_supporting_facts",
            ValidationError::EmptySupportingFactId => "empty_supporting_fact_id",
            ValidationError::EmptyContradictedFactId => "empty_contradicted_fact_id",
            ValidationError::InvalidConfidence => "invalid_confidence",
            ValidationError::OverconfidentInterpretation { .. } => "overconfident_interpretation",
            ValidationError::UnsupportedInterpretationStatus => "unsupported_interpretation_status",
            ValidationError::MissingFalsificationQuestion => "missing_falsification_question",
            ValidationError::InvalidReviewDue => "invalid_review_due",
            ValidationError::UnknownSupportingFact => "unknown_supporting_fact",
            ValidationError::AmbiguousSupportingFact => "ambiguous_supporting_fact",
            ValidationError::InvalidSupportingFact => "invalid_supporting_fact",
            ValidationError::SupportingFactIdMismatch => "supporting_fact_id_mismatch",
            ValidationError::EmptyPatternName => "empty_pattern_name",
            ValidationError::InvalidPatternSlug => "invalid_pattern_slug",
            ValidationError::EmptyPatternDescription => "empty_pattern_description",
            ValidationError::MissingPatternMarkers => "missing_pattern_markers",
            ValidationError::EmptyPatternMarker => "empty_pattern_marker",
            ValidationError::MissingPatternCounterMarkers => "missing_pattern_counter_markers",
            ValidationError::EmptyPatternCounterMarker => "empty_pattern_counter_marker",
            ValidationError::PatternIdentityClaim => "pattern_identity_claim",
            ValidationError::EmptyPatternAlias => "empty_pattern_alias",
            ValidationError::DuplicatePatternAlias => "duplicate_pattern_alias",
            ValidationError::AliasEqualsSlug => "alias_equals_slug",
            ValidationError::AmbiguousPatternSeed => "ambiguous_pattern_seed",
            ValidationError::InvalidPatternSeedMatch => "invalid_pattern_seed_match",
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
