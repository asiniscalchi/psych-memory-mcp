//! Evidence resolution: verifying that ids referenced by an epistemic record
//! actually resolve to existing, valid records of the right type.
//!
//! This is the domain layer's job, not the backend's. The backend only looks
//! up memories by tag; the epistemic rules — is it really a fact / pattern seed
//! / interpretation? is it unambiguous? does its metadata match? — live here.
//! All three entity types share the [`crate::resolution`] semantics, so a
//! single valid record stays usable even amid corrupt/tag-colliding records.

use crate::backend::{MemoryBackend, MemoryRecord};
use crate::errors::{PsychMemoryError, ValidationError};
use crate::model::interpretation::SCHEMA_VERSION as INTERP_SCHEMA_VERSION;
use crate::model::journal_fact::SCHEMA_VERSION as FACT_SCHEMA_VERSION;
use crate::model::pattern_seed::SCHEMA_VERSION as PATTERN_SCHEMA_VERSION;
use crate::resolution::{resolve_one_by_tag, TypedLookup};

fn has_tag(memory: &MemoryRecord, tag: &str) -> bool {
    memory.tags.iter().any(|t| t == tag)
}

fn metadata_str<'a>(memory: &'a MemoryRecord, key: &str) -> Option<&'a str> {
    memory.metadata.get(key).and_then(|v| v.as_str())
}

// --- Journal fact ---

/// A record that is structurally a journal fact (type, tags, schema) — but
/// whose `metadata.fact_id` is *not* yet checked against the requested id.
fn is_structural_fact(memory: &MemoryRecord, fact_id: &str) -> bool {
    memory.memory_type == "fact"
        && has_tag(memory, "epistemic:fact")
        && has_tag(memory, "source:froid")
        && has_tag(memory, &format!("fact_id:{fact_id}"))
        && metadata_str(memory, "schema_version") == Some(FACT_SCHEMA_VERSION)
}

fn is_valid_journal_fact(memory: &MemoryRecord, fact_id: &str) -> bool {
    is_structural_fact(memory, fact_id) && metadata_str(memory, "fact_id") == Some(fact_id)
}

/// Classify why none of the tag-matched records were valid facts: a
/// structurally-correct record means only `metadata.fact_id` is off.
fn fact_invalid_error(records: &[MemoryRecord], fact_id: &str) -> ValidationError {
    if records.iter().any(|m| is_structural_fact(m, fact_id)) {
        ValidationError::SupportingFactIdMismatch
    } else {
        ValidationError::InvalidSupportingFact
    }
}

// --- Pattern seed ---

fn is_valid_pattern_seed(memory: &MemoryRecord, pattern_id: &str) -> bool {
    memory.memory_type == "pattern_seed"
        && has_tag(memory, "epistemic:pattern_seed")
        && has_tag(memory, "status:seed")
        && has_tag(memory, &format!("pattern_id:{pattern_id}"))
        && metadata_str(memory, "pattern_id") == Some(pattern_id)
        && metadata_str(memory, "schema_version") == Some(PATTERN_SCHEMA_VERSION)
}

/// Resolve a pattern seed by its `pattern_id` tag.
pub async fn resolve_pattern_seed_by_pattern_id(
    backend: &dyn MemoryBackend,
    pattern_id: &str,
) -> Result<TypedLookup, PsychMemoryError> {
    let tag = format!("pattern_id:{pattern_id}");
    resolve_one_by_tag(backend, &tag, |m| is_valid_pattern_seed(m, pattern_id)).await
}

// --- Interpretation ---

fn is_structural_interpretation(memory: &MemoryRecord, interpretation_id: &str) -> bool {
    memory.memory_type == "interpretation"
        && has_tag(memory, "epistemic:interpretation")
        && has_tag(memory, "epistemic_status:hypothesis")
        && has_tag(memory, &format!("interpretation_id:{interpretation_id}"))
        && metadata_str(memory, "schema_version") == Some(INTERP_SCHEMA_VERSION)
}

fn is_valid_interpretation(memory: &MemoryRecord, interpretation_id: &str) -> bool {
    is_structural_interpretation(memory, interpretation_id)
        && metadata_str(memory, "interpretation_id") == Some(interpretation_id)
}

fn interpretation_invalid_error(
    records: &[MemoryRecord],
    interpretation_id: &str,
) -> ValidationError {
    if records
        .iter()
        .any(|m| is_structural_interpretation(m, interpretation_id))
    {
        ValidationError::InterpretationIdMismatch
    } else {
        ValidationError::InvalidInterpretation
    }
}

/// Resolve an interpretation by its `interpretation_id` tag.
pub async fn resolve_interpretation_by_interpretation_id(
    backend: &dyn MemoryBackend,
    interpretation_id: &str,
) -> Result<TypedLookup, PsychMemoryError> {
    let tag = format!("interpretation_id:{interpretation_id}");
    resolve_one_by_tag(backend, &tag, |m| {
        is_valid_interpretation(m, interpretation_id)
    })
    .await
}

// --- List resolvers used by the epistemic tools ---

/// Resolve every supporting `fact_id` to exactly one existing journal fact.
/// Domain-validation failures (unknown/ambiguous/invalid/mismatch) come back as
/// `Err(ValidationError)`; only transport failures are other errors.
///
/// Story 4 note: this now uses the shared filter-then-count resolver, which
/// **intentionally changes** the earlier fact-lookup corner case — one valid
/// fact plus an unrelated corrupt/colliding tag match used to be
/// `ambiguous_supporting_fact`, and is now `Found(valid)`, for consistency with
/// pattern-seed and interpretation resolution.
pub async fn resolve_supporting_facts(
    backend: &dyn MemoryBackend,
    fact_ids: &[String],
) -> Result<Vec<MemoryRecord>, PsychMemoryError> {
    let mut resolved = Vec::with_capacity(fact_ids.len());
    for fact_id in fact_ids {
        let tag = format!("fact_id:{fact_id}");
        match resolve_one_by_tag(backend, &tag, |m| is_valid_journal_fact(m, fact_id)).await? {
            TypedLookup::Found(memory) => resolved.push(memory),
            TypedLookup::NotFound => return Err(ValidationError::UnknownSupportingFact.into()),
            TypedLookup::Ambiguous(_) => {
                return Err(ValidationError::AmbiguousSupportingFact.into())
            }
            TypedLookup::InvalidMatch(records) => {
                return Err(fact_invalid_error(&records, fact_id).into())
            }
        }
    }
    Ok(resolved)
}

/// Resolve every linked `interpretation_id` to exactly one existing
/// interpretation. Same failure mapping shape as facts.
pub async fn resolve_linked_interpretations(
    backend: &dyn MemoryBackend,
    interpretation_ids: &[String],
) -> Result<Vec<MemoryRecord>, PsychMemoryError> {
    let mut resolved = Vec::with_capacity(interpretation_ids.len());
    for interpretation_id in interpretation_ids {
        match resolve_interpretation_by_interpretation_id(backend, interpretation_id).await? {
            TypedLookup::Found(memory) => resolved.push(memory),
            TypedLookup::NotFound => return Err(ValidationError::UnknownInterpretation.into()),
            TypedLookup::Ambiguous(_) => {
                return Err(ValidationError::AmbiguousInterpretation.into())
            }
            TypedLookup::InvalidMatch(records) => {
                return Err(interpretation_invalid_error(&records, interpretation_id).into())
            }
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod fact_tests {
    use super::*;
    use crate::backend::FakeMemoryBackend;
    use crate::model::StoreMemoryRequest;
    use serde_json::json;

    fn fact_request(fact_id: &str, content: &str) -> StoreMemoryRequest {
        StoreMemoryRequest {
            content: content.to_string(),
            memory_type: "fact".into(),
            tags: vec![
                "epistemic:fact".into(),
                "source:froid".into(),
                format!("fact_id:{fact_id}"),
            ],
            metadata: json!({ "fact_id": fact_id, "schema_version": FACT_SCHEMA_VERSION }),
        }
    }

    async fn store(backend: &FakeMemoryBackend, req: StoreMemoryRequest) {
        backend.store_memory(req).await.unwrap();
    }

    fn code(err: PsychMemoryError) -> String {
        match err {
            PsychMemoryError::Validation(v) => v.error_code().to_string(),
            other => panic!("expected validation error, got {other}"),
        }
    }

    #[tokio::test]
    async fn accepts_existing_valid_supporting_fact() {
        let backend = FakeMemoryBackend::new();
        store(&backend, fact_request("fact_a", "excerpt")).await;
        let resolved = resolve_supporting_facts(&backend, &["fact_a".to_string()])
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);
    }

    #[tokio::test]
    async fn rejects_unknown_supporting_fact() {
        let backend = FakeMemoryBackend::new();
        let err = resolve_supporting_facts(&backend, &["nope".to_string()])
            .await
            .unwrap_err();
        assert_eq!(code(err), "unknown_supporting_fact");
    }

    #[tokio::test]
    async fn rejects_ambiguous_supporting_fact_when_two_valid() {
        let backend = FakeMemoryBackend::new();
        store(&backend, fact_request("fact_dup", "one")).await;
        store(&backend, fact_request("fact_dup", "two")).await;
        let err = resolve_supporting_facts(&backend, &["fact_dup".to_string()])
            .await
            .unwrap_err();
        assert_eq!(code(err), "ambiguous_supporting_fact");
    }

    #[tokio::test]
    async fn mixed_valid_and_invalid_fact_matches_returns_found() {
        let backend = FakeMemoryBackend::new();
        store(&backend, fact_request("fact_m", "valid")).await;
        let mut bad = fact_request("fact_m", "corrupt");
        bad.memory_type = "observation".into(); // not a fact
        store(&backend, bad).await;
        // One valid + one corrupt -> Found (no longer Ambiguous).
        let resolved = resolve_supporting_facts(&backend, &["fact_m".to_string()])
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].content, "valid");
    }

    #[tokio::test]
    async fn all_invalid_non_fact_matches_returns_invalid() {
        let backend = FakeMemoryBackend::new();
        let mut bad = fact_request("fact_x", "corrupt");
        bad.memory_type = "interpretation".into();
        store(&backend, bad).await;
        let err = resolve_supporting_facts(&backend, &["fact_x".to_string()])
            .await
            .unwrap_err();
        assert_eq!(code(err), "invalid_supporting_fact");
    }

    #[tokio::test]
    async fn supporting_fact_metadata_mismatch_is_specific() {
        let backend = FakeMemoryBackend::new();
        let mut bad = fact_request("fact_y", "x");
        bad.metadata = json!({ "fact_id": "fact_OTHER", "schema_version": FACT_SCHEMA_VERSION });
        store(&backend, bad).await;
        let err = resolve_supporting_facts(&backend, &["fact_y".to_string()])
            .await
            .unwrap_err();
        assert_eq!(code(err), "supporting_fact_id_mismatch");
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
                "status:seed".into(),
                format!("pattern_id:{pattern_id}"),
            ],
            metadata: json!({ "pattern_id": pattern_id, "schema_version": PATTERN_SCHEMA_VERSION }),
        }
    }

    async fn store(backend: &FakeMemoryBackend, req: StoreMemoryRequest) {
        backend.store_memory(req).await.unwrap();
    }

    #[tokio::test]
    async fn not_found_when_none() {
        let backend = FakeMemoryBackend::new();
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_x")
            .await
            .unwrap();
        assert!(matches!(r, TypedLookup::NotFound));
    }

    #[tokio::test]
    async fn found_when_one_valid() {
        let backend = FakeMemoryBackend::new();
        store(&backend, seed_request("pattern_x", "d")).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_x")
            .await
            .unwrap();
        assert!(matches!(r, TypedLookup::Found(_)));
    }

    #[tokio::test]
    async fn ambiguous_when_two_valid() {
        let backend = FakeMemoryBackend::new();
        store(&backend, seed_request("pattern_x", "one")).await;
        store(&backend, seed_request("pattern_x", "two")).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_x")
            .await
            .unwrap();
        assert!(matches!(r, TypedLookup::Ambiguous(v) if v.len() == 2));
    }

    #[tokio::test]
    async fn invalid_match_when_all_corrupt() {
        let backend = FakeMemoryBackend::new();
        let mut bad = seed_request("pattern_x", "corrupt");
        bad.memory_type = "fact".into();
        store(&backend, bad).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_x")
            .await
            .unwrap();
        assert!(matches!(r, TypedLookup::InvalidMatch(_)));
    }

    #[tokio::test]
    async fn mixed_valid_and_invalid_returns_found() {
        let backend = FakeMemoryBackend::new();
        store(&backend, seed_request("pattern_x", "valid")).await;
        let mut bad = seed_request("pattern_x", "corrupt");
        bad.memory_type = "observation".into();
        store(&backend, bad).await;
        let r = resolve_pattern_seed_by_pattern_id(&backend, "pattern_x")
            .await
            .unwrap();
        assert!(matches!(r, TypedLookup::Found(_)));
    }
}

#[cfg(test)]
mod interpretation_tests {
    use super::*;
    use crate::backend::FakeMemoryBackend;
    use crate::model::StoreMemoryRequest;
    use serde_json::json;

    fn interp_request(interpretation_id: &str, content: &str) -> StoreMemoryRequest {
        StoreMemoryRequest {
            content: content.to_string(),
            memory_type: "interpretation".into(),
            tags: vec![
                "epistemic:interpretation".into(),
                "epistemic_status:hypothesis".into(),
                format!("interpretation_id:{interpretation_id}"),
            ],
            metadata: json!({
                "interpretation_id": interpretation_id,
                "schema_version": INTERP_SCHEMA_VERSION,
            }),
        }
    }

    async fn store(backend: &FakeMemoryBackend, req: StoreMemoryRequest) {
        backend.store_memory(req).await.unwrap();
    }

    fn code(err: PsychMemoryError) -> String {
        match err {
            PsychMemoryError::Validation(v) => v.error_code().to_string(),
            other => panic!("expected validation error, got {other}"),
        }
    }

    #[tokio::test]
    async fn accepts_existing_valid_interpretation() {
        let backend = FakeMemoryBackend::new();
        store(&backend, interp_request("interp_a", "h")).await;
        let resolved = resolve_linked_interpretations(&backend, &["interp_a".to_string()])
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);
    }

    #[tokio::test]
    async fn rejects_unknown_interpretation() {
        let backend = FakeMemoryBackend::new();
        let err = resolve_linked_interpretations(&backend, &["nope".to_string()])
            .await
            .unwrap_err();
        assert_eq!(code(err), "unknown_interpretation");
    }

    #[tokio::test]
    async fn rejects_ambiguous_interpretation() {
        let backend = FakeMemoryBackend::new();
        store(&backend, interp_request("interp_d", "one")).await;
        store(&backend, interp_request("interp_d", "two")).await;
        let err = resolve_linked_interpretations(&backend, &["interp_d".to_string()])
            .await
            .unwrap_err();
        assert_eq!(code(err), "ambiguous_interpretation");
    }

    #[tokio::test]
    async fn mixed_valid_and_invalid_interpretation_matches_returns_found() {
        let backend = FakeMemoryBackend::new();
        store(&backend, interp_request("interp_m", "valid")).await;
        let mut bad = interp_request("interp_m", "corrupt");
        bad.memory_type = "observation".into();
        store(&backend, bad).await;
        let resolved = resolve_linked_interpretations(&backend, &["interp_m".to_string()])
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);
    }

    #[tokio::test]
    async fn all_invalid_interpretation_matches_rejected() {
        let backend = FakeMemoryBackend::new();
        let mut bad = interp_request("interp_x", "corrupt");
        bad.memory_type = "fact".into();
        store(&backend, bad).await;
        let err = resolve_linked_interpretations(&backend, &["interp_x".to_string()])
            .await
            .unwrap_err();
        assert_eq!(code(err), "invalid_interpretation");
    }

    #[tokio::test]
    async fn interpretation_metadata_mismatch_is_specific() {
        let backend = FakeMemoryBackend::new();
        let mut bad = interp_request("interp_y", "x");
        bad.metadata =
            json!({ "interpretation_id": "interp_OTHER", "schema_version": INTERP_SCHEMA_VERSION });
        store(&backend, bad).await;
        let err = resolve_linked_interpretations(&backend, &["interp_y".to_string()])
            .await
            .unwrap_err();
        assert_eq!(code(err), "interpretation_id_mismatch");
    }
}
