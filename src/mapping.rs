//! Map a validated journal fact to a backend-neutral [`StoreMemoryRequest`].
//!
//! Backend content rule: the stored `content` is the raw `source_excerpt`, not
//! the normalized statement. The raw journal text is more epistemically honest
//! and better for semantic retrieval; the structured fields already say this
//! memory is a fact.
//!
//! Deferred (Story 1 scope): the memory-service keys records on
//! `content_hash = sha256(content)`, and here `content = source_excerpt`. Two
//! distinct facts whose excerpt text is byte-identical (e.g. the same sentence
//! quoted in two different entries) therefore collapse to one backend memory,
//! and only the last-written `fact_id` tag survives. Resolving excerpt-level
//! collisions at the backend is out of scope for this story.

use serde_json::json;

use crate::model::journal_fact::{EPISTEMIC_STATUS, MEMORY_TYPE, SCHEMA_VERSION};
use crate::model::{StoreMemoryRequest, ValidatedJournalFact};

/// Build the backend store request for a validated fact and its `fact_id`.
pub fn map_store_journal_fact_to_backend_request(
    fact: &ValidatedJournalFact,
    fact_id: &str,
) -> StoreMemoryRequest {
    let fact_type = fact.fact_type.as_str();

    let tags = vec![
        "epistemic:fact".to_string(),
        format!("epistemic_status:{EPISTEMIC_STATUS}"),
        "source:froid".to_string(),
        format!("fact_type:{fact_type}"),
        format!("fact_id:{fact_id}"),
    ];

    let metadata = json!({
        "fact_id": fact_id,
        "source_excerpt": fact.source_excerpt,
        "normalized_statement": fact.normalized_statement,
        "fact_type": fact_type,
        "epistemic_status": EPISTEMIC_STATUS,
        "journal_entry_ref": fact.journal_entry_ref,
        "event_date": fact.event_date,
        "schema_version": SCHEMA_VERSION,
    });

    StoreMemoryRequest {
        content: fact.source_excerpt.clone(),
        memory_type: MEMORY_TYPE.to_string(),
        tags,
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FactType, JournalEntryRef};

    fn fact() -> ValidatedJournalFact {
        ValidatedJournalFact {
            source_excerpt: "I felt very strong hunger.".into(),
            normalized_statement: "Ale reported feeling strong hunger.".into(),
            fact_type: FactType::SelfReport,
            journal_entry_ref: JournalEntryRef {
                entry_id: "froid_2026_06_01_abc123".into(),
                entry_date: "2026-06-01".into(),
                char_start: Some(120),
                char_end: Some(190),
                content_hash: None,
            },
            event_date: None,
        }
    }

    fn mapped() -> StoreMemoryRequest {
        map_store_journal_fact_to_backend_request(&fact(), "fact_deadbeef")
    }

    #[test]
    fn maps_source_excerpt_to_content() {
        let req = mapped();
        assert_eq!(req.content, "I felt very strong hunger.");
        assert_eq!(req.memory_type, "fact");
    }

    #[test]
    fn maps_normalized_statement_to_metadata_not_content() {
        let req = mapped();
        assert_eq!(
            req.metadata["normalized_statement"],
            "Ale reported feeling strong hunger."
        );
        assert!(!req.content.contains("Ale reported"));
    }

    #[test]
    fn maps_expected_tags() {
        let req = mapped();
        for tag in [
            "epistemic:fact",
            "epistemic_status:journal_reported",
            "source:froid",
            "fact_type:self_report",
        ] {
            assert!(req.tags.iter().any(|t| t == tag), "missing tag {tag}");
        }
    }

    #[test]
    fn maps_fact_id_tag() {
        let req = mapped();
        assert!(req.tags.iter().any(|t| t == "fact_id:fact_deadbeef"));
        assert_eq!(req.metadata["fact_id"], "fact_deadbeef");
    }

    #[test]
    fn maps_schema_version() {
        let req = mapped();
        assert_eq!(
            req.metadata["schema_version"],
            "psych-memory.journal_fact.v1"
        );
    }

    #[test]
    fn maps_journal_entry_ref_and_epistemic_status() {
        let req = mapped();
        assert_eq!(req.metadata["epistemic_status"], "journal_reported");
        assert_eq!(
            req.metadata["journal_entry_ref"]["entry_id"],
            "froid_2026_06_01_abc123"
        );
        assert_eq!(req.metadata["event_date"], serde_json::Value::Null);
    }
}
