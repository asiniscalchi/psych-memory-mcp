//! Evidence resolution: verifying that an interpretation's supporting
//! `fact_id`s actually resolve to existing journal facts.
//!
//! This is the domain layer's job, not the backend's. The backend only knows
//! how to look up memories by tag; the epistemic rules — is it really a fact?
//! is it unambiguous? does its metadata match? — live here.

use crate::backend::{MemoryBackend, MemoryRecord};
use crate::errors::{PsychMemoryError, ValidationError};
use crate::model::journal_fact::SCHEMA_VERSION as FACT_SCHEMA_VERSION;

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
