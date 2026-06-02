//! Evidence resolution: verifying that an interpretation's supporting
//! `fact_id`s actually resolve to existing journal facts.
//!
//! This is the domain layer's job, not the backend's. The backend only knows
//! how to look up memories by tag; the epistemic rules — is it really a fact?
//! is it unambiguous? does its metadata match? — live here.

use crate::backend::{MemoryBackend, MemoryRecord};
use crate::errors::{PsychMemoryError, ValidationError};
use crate::model::journal_fact::SCHEMA_VERSION as FACT_SCHEMA_VERSION;
use crate::model::pattern_seed::SCHEMA_VERSION as PATTERN_SCHEMA_VERSION;

fn has_tag(memory: &MemoryRecord, tag: &str) -> bool {
    memory.tags.iter().any(|t| t == tag)
}

/// Validate that a single tag-matched memory is genuinely the journal fact for
/// `fact_id`. The `metadata.fact_id` exact-equality check is the final guard,
/// since tag matching may have adapter-specific (loose/prefix) semantics.
fn validate_resolved_fact(memory: &MemoryRecord, fact_id: &str) -> Result<(), ValidationError> {
    let fact_id_tag = format!("fact_id:{fact_id}");
    let structurally_a_fact = memory.memory_type == "fact"
        && has_tag(memory, "epistemic:fact")
        && has_tag(memory, "source:froid")
        && has_tag(memory, &fact_id_tag);
    if !structurally_a_fact {
        return Err(ValidationError::InvalidSupportingFact);
    }

    match memory.metadata.get("fact_id").and_then(|v| v.as_str()) {
        Some(id) if id == fact_id => {}
        _ => return Err(ValidationError::SupportingFactIdMismatch),
    }

    let schema_ok = memory
        .metadata
        .get("schema_version")
        .and_then(|v| v.as_str())
        == Some(FACT_SCHEMA_VERSION);
    if !schema_ok {
        return Err(ValidationError::InvalidSupportingFact);
    }

    Ok(())
}

/// Resolve every supporting `fact_id` to exactly one existing journal fact.
///
/// Each id must match exactly one memory via the `fact_id:<id>` tag; zero is
/// `unknown_supporting_fact`, more than one is `ambiguous_supporting_fact`, and
/// the single match must pass [`validate_resolved_fact`].
pub async fn resolve_supporting_facts(
    backend: &dyn MemoryBackend,
    supported_by_fact_ids: &[String],
) -> Result<Vec<MemoryRecord>, PsychMemoryError> {
    let mut resolved = Vec::with_capacity(supported_by_fact_ids.len());

    for fact_id in supported_by_fact_ids {
        let tag = format!("fact_id:{fact_id}");
        let mut matches = backend.find_memories_by_tag(&tag).await?;

        let memory = match matches.len() {
            0 => return Err(ValidationError::UnknownSupportingFact.into()),
            1 => matches.remove(0),
            _ => return Err(ValidationError::AmbiguousSupportingFact.into()),
        };

        validate_resolved_fact(&memory, fact_id)?;
        resolved.push(memory);
    }

    Ok(resolved)
}

/// Outcome of looking up an existing pattern seed by `pattern_id`.
#[derive(Debug)]
pub enum PatternSeedLookup {
    /// No memory carries this `pattern_id` tag.
    NotFound,
    /// Exactly one valid pattern seed exists.
    Found(MemoryRecord),
    /// More than one valid pattern seed exists for this id.
    Ambiguous(Vec<MemoryRecord>),
    /// Memories carry the tag but none are valid pattern seeds.
    InvalidMatch(Vec<MemoryRecord>),
}

/// True if `memory` is structurally a valid pattern seed for `pattern_id`,
/// including the mandatory `metadata.pattern_id` exact-equality guard.
fn is_valid_pattern_seed(memory: &MemoryRecord, pattern_id: &str) -> bool {
    let pattern_id_tag = format!("pattern_id:{pattern_id}");
    let structural = memory.memory_type == "pattern_seed"
        && has_tag(memory, "epistemic:pattern_seed")
        && has_tag(memory, "status:seed")
        && has_tag(memory, &pattern_id_tag);
    let metadata_id_ok =
        memory.metadata.get("pattern_id").and_then(|v| v.as_str()) == Some(pattern_id);
    let schema_ok = memory
        .metadata
        .get("schema_version")
        .and_then(|v| v.as_str())
        == Some(PATTERN_SCHEMA_VERSION);
    structural && metadata_id_ok && schema_ok
}

/// Resolve an existing pattern seed by its `pattern_id` (idempotency check).
///
/// Mixed valid + invalid matches resolve to `Found` (a single valid seed stays
/// usable even if unrelated corrupt/colliding records exist); only when there
/// are matches but *none* are valid do we report `InvalidMatch`.
pub async fn resolve_pattern_seed_by_pattern_id(
    backend: &dyn MemoryBackend,
    pattern_id: &str,
) -> Result<PatternSeedLookup, PsychMemoryError> {
    let tag = format!("pattern_id:{pattern_id}");
    let matches = backend.find_memories_by_tag(&tag).await?;
    if matches.is_empty() {
        return Ok(PatternSeedLookup::NotFound);
    }

    let (mut valid, invalid): (Vec<MemoryRecord>, Vec<MemoryRecord>) = matches
        .into_iter()
        .partition(|m| is_valid_pattern_seed(m, pattern_id));

    Ok(match valid.len() {
        0 => PatternSeedLookup::InvalidMatch(invalid),
        1 => PatternSeedLookup::Found(valid.remove(0)),
        _ => PatternSeedLookup::Ambiguous(valid),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::FakeMemoryBackend;
    use crate::model::StoreMemoryRequest;
    use serde_json::json;

    /// Build a stored memory that looks like a Story 1 journal fact.
    fn fact_request(fact_id: &str, content: &str) -> StoreMemoryRequest {
        StoreMemoryRequest {
            content: content.to_string(),
            memory_type: "fact".into(),
            tags: vec![
                "epistemic:fact".into(),
                "epistemic_status:journal_reported".into(),
                "source:froid".into(),
                "fact_type:self_report".into(),
                format!("fact_id:{fact_id}"),
            ],
            metadata: json!({
                "fact_id": fact_id,
                "schema_version": FACT_SCHEMA_VERSION,
            }),
        }
    }

    async fn store(backend: &FakeMemoryBackend, req: StoreMemoryRequest) {
        backend.store_memory(req).await.unwrap();
    }

    #[tokio::test]
    async fn accepts_existing_valid_supporting_fact() {
        let backend = FakeMemoryBackend::new();
        store(&backend, fact_request("fact_a", "excerpt a")).await;

        let resolved = resolve_supporting_facts(&backend, &["fact_a".to_string()])
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].metadata["fact_id"], "fact_a");
    }

    #[tokio::test]
    async fn rejects_unknown_supporting_fact() {
        let backend = FakeMemoryBackend::new();
        let err = resolve_supporting_facts(&backend, &["fact_missing".to_string()])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            PsychMemoryError::Validation(ValidationError::UnknownSupportingFact)
        ));
    }

    #[tokio::test]
    async fn rejects_ambiguous_supporting_fact() {
        let backend = FakeMemoryBackend::new();
        // Two distinct memories carrying the same fact_id tag.
        store(&backend, fact_request("fact_dup", "excerpt one")).await;
        store(&backend, fact_request("fact_dup", "excerpt two")).await;

        let err = resolve_supporting_facts(&backend, &["fact_dup".to_string()])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            PsychMemoryError::Validation(ValidationError::AmbiguousSupportingFact)
        ));
    }

    #[tokio::test]
    async fn rejects_non_fact_supporting_memory() {
        let backend = FakeMemoryBackend::new();
        let mut req = fact_request("fact_x", "excerpt");
        req.memory_type = "interpretation".into(); // not a fact
        store(&backend, req).await;

        let err = resolve_supporting_facts(&backend, &["fact_x".to_string()])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            PsychMemoryError::Validation(ValidationError::InvalidSupportingFact)
        ));
    }

    #[tokio::test]
    async fn rejects_supporting_fact_metadata_mismatch() {
        let backend = FakeMemoryBackend::new();
        let mut req = fact_request("fact_y", "excerpt");
        // Tagged fact_id:fact_y, but metadata says something else.
        req.metadata = json!({ "fact_id": "fact_OTHER", "schema_version": FACT_SCHEMA_VERSION });
        store(&backend, req).await;

        let err = resolve_supporting_facts(&backend, &["fact_y".to_string()])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            PsychMemoryError::Validation(ValidationError::SupportingFactIdMismatch)
        ));
    }

    #[tokio::test]
    async fn rejects_wrong_schema_version() {
        let backend = FakeMemoryBackend::new();
        let mut req = fact_request("fact_z", "excerpt");
        req.metadata = json!({ "fact_id": "fact_z", "schema_version": "something.else.v9" });
        store(&backend, req).await;

        let err = resolve_supporting_facts(&backend, &["fact_z".to_string()])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            PsychMemoryError::Validation(ValidationError::InvalidSupportingFact)
        ));
    }
}

#[cfg(test)]
mod pattern_lookup_tests {
    use super::*;
    use crate::backend::FakeMemoryBackend;
    use crate::model::StoreMemoryRequest;
    use serde_json::json;

    fn seed_request(pattern_id: &str, content: &str) -> StoreMemoryRequest {
        StoreMemoryRequest {
            content: content.to_string(),
            memory_type: "pattern_seed".into(),
            tags: vec![
                "epistemic:pattern_seed".into(),
                "epistemic_status:observation_category".into(),
                "status:seed".into(),
                format!("pattern_id:{pattern_id}"),
            ],
            metadata: json!({
                "pattern_id": pattern_id,
                "schema_version": PATTERN_SCHEMA_VERSION,
            }),
        }
    }

    async fn store(backend: &FakeMemoryBackend, req: StoreMemoryRequest) {
        backend.store_memory(req).await.unwrap();
    }

    #[tokio::test]
    async fn returns_not_found_when_no_pattern_seed_exists() {
        let backend = FakeMemoryBackend::new();
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_savior")
            .await
            .unwrap();
        assert!(matches!(r, PatternSeedLookup::NotFound));
    }

    #[tokio::test]
    async fn returns_found_when_one_valid_pattern_seed_exists() {
        let backend = FakeMemoryBackend::new();
        store(&backend, seed_request("pattern_savior", "Savior — desc")).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_savior")
            .await
            .unwrap();
        assert!(matches!(r, PatternSeedLookup::Found(_)));
    }

    #[tokio::test]
    async fn more_than_one_valid_pattern_match_returns_ambiguous() {
        let backend = FakeMemoryBackend::new();
        store(&backend, seed_request("pattern_savior", "Savior — one")).await;
        store(&backend, seed_request("pattern_savior", "Savior — two")).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_savior")
            .await
            .unwrap();
        assert!(matches!(r, PatternSeedLookup::Ambiguous(v) if v.len() == 2));
    }

    #[tokio::test]
    async fn all_invalid_pattern_matches_returns_invalid_match() {
        let backend = FakeMemoryBackend::new();
        // Carries the tag but is not a pattern_seed memory type.
        let mut bad = seed_request("pattern_savior", "tampered");
        bad.memory_type = "fact".into();
        store(&backend, bad).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_savior")
            .await
            .unwrap();
        assert!(matches!(r, PatternSeedLookup::InvalidMatch(v) if v.len() == 1));
    }

    #[tokio::test]
    async fn invalid_when_metadata_pattern_id_mismatch() {
        let backend = FakeMemoryBackend::new();
        let mut bad = seed_request("pattern_savior", "x");
        bad.metadata =
            json!({ "pattern_id": "pattern_OTHER", "schema_version": PATTERN_SCHEMA_VERSION });
        store(&backend, bad).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_savior")
            .await
            .unwrap();
        assert!(matches!(r, PatternSeedLookup::InvalidMatch(_)));
    }

    #[tokio::test]
    async fn mixed_valid_and_invalid_pattern_matches_returns_found() {
        let backend = FakeMemoryBackend::new();
        store(&backend, seed_request("pattern_savior", "Savior — valid")).await;
        let mut bad = seed_request("pattern_savior", "corrupt");
        bad.memory_type = "observation".into();
        store(&backend, bad).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_savior")
            .await
            .unwrap();
        assert!(matches!(r, PatternSeedLookup::Found(_)));
    }
}
