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

use crate::model::interpretation::{
    EPISTEMIC_STATUS as INTERP_EPISTEMIC_STATUS, MEMORY_TYPE as INTERP_MEMORY_TYPE,
    SCHEMA_VERSION as INTERP_SCHEMA_VERSION,
};
use crate::model::journal_fact::{EPISTEMIC_STATUS, MEMORY_TYPE, SCHEMA_VERSION};
use crate::model::{
    InterpretationStatus, StoreMemoryRequest, ValidatedInterpretation, ValidatedJournalFact,
};

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

/// Build the backend store request for a validated interpretation.
///
/// Backend content rule: `content` is the hypothesis text (no `INTERPRETATION:`
/// prefix); memory_type, tags and metadata already convey structure.
///
/// Known collision risk (deferred, see README): the backend keys records on
/// `content_hash = sha256(content) = sha256(hypothesis)`. Two interpretations
/// with the same hypothesis but different supporting facts have *different*
/// `interpretation_id`s yet identical backend content, so one may overwrite the
/// other. Story 2 documents this; it is not solved here.
pub fn map_store_interpretation_to_backend_request(
    interpretation: &ValidatedInterpretation,
    interpretation_id: &str,
) -> StoreMemoryRequest {
    let interpretation_type = interpretation.interpretation_type.as_str();
    let status = InterpretationStatus::Candidate.as_str();

    let mut tags = vec![
        "epistemic:interpretation".to_string(),
        format!("epistemic_status:{INTERP_EPISTEMIC_STATUS}"),
        format!("interpretation_type:{interpretation_type}"),
        format!("status:{status}"),
        format!("interpretation_id:{interpretation_id}"),
    ];
    for fact_id in &interpretation.supported_by_fact_ids {
        tags.push(format!("supported_by:{fact_id}"));
    }

    let metadata = json!({
        "interpretation_id": interpretation_id,
        "hypothesis": interpretation.hypothesis,
        "interpretation_type": interpretation_type,
        "epistemic_status": INTERP_EPISTEMIC_STATUS,
        "status": status,
        "supported_by_fact_ids": interpretation.supported_by_fact_ids,
        "contradicted_by_fact_ids": interpretation.contradicted_by_fact_ids,
        "confidence": interpretation.confidence,
        "falsification_question": interpretation.falsification_question,
        "review_due": interpretation.review_due,
        "schema_version": INTERP_SCHEMA_VERSION,
    });

    StoreMemoryRequest {
        content: interpretation.hypothesis.clone(),
        memory_type: INTERP_MEMORY_TYPE.to_string(),
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

#[cfg(test)]
mod interpretation_tests {
    use super::*;
    use crate::model::InterpretationType;

    fn interp() -> ValidatedInterpretation {
        ValidatedInterpretation {
            hypothesis: "Hunger may have functioned as emotional discharge.".into(),
            interpretation_type: InterpretationType::PsychologicalHypothesis,
            supported_by_fact_ids: vec!["fact_a".into(), "fact_b".into()],
            contradicted_by_fact_ids: vec![],
            confidence: 0.35,
            falsification_question: "Are there episodes without activation?".into(),
            review_due: Some("2026-06-09".into()),
        }
    }

    fn mapped() -> StoreMemoryRequest {
        map_store_interpretation_to_backend_request(&interp(), "interp_dead")
    }

    #[test]
    fn maps_hypothesis_to_content() {
        let req = mapped();
        assert_eq!(
            req.content,
            "Hunger may have functioned as emotional discharge."
        );
        assert_eq!(req.memory_type, "interpretation");
        assert!(!req.content.starts_with("INTERPRETATION:"));
    }

    #[test]
    fn maps_expected_tags() {
        let req = mapped();
        for tag in [
            "epistemic:interpretation",
            "epistemic_status:hypothesis",
            "interpretation_type:psychological_hypothesis",
            "status:candidate",
        ] {
            assert!(req.tags.iter().any(|t| t == tag), "missing tag {tag}");
        }
    }

    #[test]
    fn maps_interpretation_id_tag() {
        let req = mapped();
        assert!(req
            .tags
            .iter()
            .any(|t| t == "interpretation_id:interp_dead"));
        assert_eq!(req.metadata["interpretation_id"], "interp_dead");
    }

    #[test]
    fn maps_supported_by_tags() {
        let req = mapped();
        assert!(req.tags.iter().any(|t| t == "supported_by:fact_a"));
        assert!(req.tags.iter().any(|t| t == "supported_by:fact_b"));
    }

    #[test]
    fn maps_metadata() {
        let req = mapped();
        assert_eq!(req.metadata["hypothesis"], interp().hypothesis);
        assert_eq!(req.metadata["epistemic_status"], "hypothesis");
        assert_eq!(req.metadata["status"], "candidate");
        assert_eq!(
            req.metadata["supported_by_fact_ids"],
            serde_json::json!(["fact_a", "fact_b"])
        );
        assert_eq!(
            req.metadata["falsification_question"],
            "Are there episodes without activation?"
        );
    }

    #[test]
    fn maps_schema_version() {
        assert_eq!(
            mapped().metadata["schema_version"],
            "psych-memory.interpretation.v1"
        );
    }
}
