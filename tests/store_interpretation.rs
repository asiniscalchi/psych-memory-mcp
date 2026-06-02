//! Tool-level tests for `store_interpretation`, exercised against the in-memory
//! backend (no live memory-service).

use std::sync::Arc;

use psych_memory_mcp::backend::{FakeMemoryBackend, MemoryBackend};
use psych_memory_mcp::model::{
    FactType, InterpretationType, JournalEntryRef, StoreInterpretationInput,
    StoreInterpretationOutput, StoreJournalFactInput, StoreJournalFactOutput, StoreMemoryRequest,
};
use psych_memory_mcp::server::PsychMemoryServer;
use serde_json::json;

fn fact_input(entry_id: &str, excerpt: &str) -> StoreJournalFactInput {
    StoreJournalFactInput {
        source_excerpt: excerpt.into(),
        normalized_statement: "Ale reported something.".into(),
        fact_type: FactType::SelfReport,
        journal_entry_ref: Some(JournalEntryRef {
            entry_id: entry_id.into(),
            entry_date: "2026-06-01".into(),
            char_start: Some(0),
            char_end: Some(20),
            content_hash: None,
        }),
        event_date: None,
    }
}

fn interp_input(fact_ids: Vec<String>) -> StoreInterpretationInput {
    StoreInterpretationInput {
        hypothesis: "Hunger may have functioned as emotional discharge.".into(),
        interpretation_type: InterpretationType::PsychologicalHypothesis,
        supported_by_fact_ids: fact_ids,
        contradicted_by_fact_ids: vec![],
        confidence: 0.35,
        status: None,
        falsification_question: "Are there episodes without activation?".into(),
        review_due: Some("2026-06-09".into()),
    }
}

async fn store_fact(server: &PsychMemoryServer, entry_id: &str, excerpt: &str) -> String {
    match server
        .store_journal_fact_flow(fact_input(entry_id, excerpt))
        .await
        .unwrap()
    {
        StoreJournalFactOutput::Stored { fact_id, .. } => fact_id,
        StoreJournalFactOutput::Rejected { error_code, .. } => {
            panic!("fact rejected: {error_code}")
        }
    }
}

async fn no_interpretation_stored(backend: &FakeMemoryBackend) -> bool {
    backend
        .find_memories_by_tag("epistemic:interpretation")
        .await
        .unwrap()
        .is_empty()
}

#[test]
fn interpretation_tool_exposed_and_no_generic() {
    let router = PsychMemoryServer::tool_router();
    assert!(router.has_route("store_interpretation"));
    assert!(router.has_route("store_journal_fact"));
    for generic in ["store_memory", "save_memory", "remember"] {
        assert!(!router.has_route(generic));
    }
}

#[tokio::test]
async fn store_interpretation_uses_memory_backend_and_returns_interpretation_id() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let fact_id = store_fact(&server, "entry_1", "I felt strong hunger.").await;

    let out = server
        .store_interpretation_flow(interp_input(vec![fact_id.clone()]))
        .await
        .unwrap();

    let (interpretation_id, backend_memory_id) = match out {
        StoreInterpretationOutput::Stored {
            interpretation_id,
            backend_memory_id,
            status,
        } => {
            assert_eq!(status, "stored");
            (interpretation_id, backend_memory_id.unwrap())
        }
        StoreInterpretationOutput::Rejected { error_code, .. } => {
            panic!("unexpected rejection: {error_code}")
        }
    };
    assert!(interpretation_id.starts_with("interp_"));

    let record = backend
        .get_memory(&backend_memory_id)
        .await
        .unwrap()
        .expect("interpretation stored");
    assert_eq!(record.memory_type, "interpretation");
    assert!(record
        .tags
        .contains(&"epistemic:interpretation".to_string()));
    assert!(record
        .tags
        .contains(&format!("interpretation_id:{interpretation_id}")));
    assert!(record.tags.contains(&format!("supported_by:{fact_id}")));
    assert_eq!(record.metadata["interpretation_id"], interpretation_id);
    assert_eq!(record.metadata["epistemic_status"], "hypothesis");
}

#[tokio::test]
async fn store_interpretation_does_not_store_when_fact_lookup_fails() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    let out = server
        .store_interpretation_flow(interp_input(vec!["fact_does_not_exist".into()]))
        .await
        .unwrap();

    match out {
        StoreInterpretationOutput::Rejected { error_code, .. } => {
            assert_eq!(error_code, "unknown_supporting_fact");
        }
        StoreInterpretationOutput::Stored { .. } => panic!("should have been rejected"),
    }
    assert!(no_interpretation_stored(&backend).await);
}

#[tokio::test]
async fn store_interpretation_does_not_store_when_validation_fails() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    // Empty supported_by_fact_ids fails shape validation before any lookup.
    let out = server
        .store_interpretation_flow(interp_input(vec![]))
        .await
        .unwrap();

    match out {
        StoreInterpretationOutput::Rejected { error_code, .. } => {
            assert_eq!(error_code, "missing_supporting_facts");
        }
        StoreInterpretationOutput::Stored { .. } => panic!("should have been rejected"),
    }
    assert!(no_interpretation_stored(&backend).await);
}

#[tokio::test]
async fn store_interpretation_does_not_store_when_metadata_fact_id_mismatch() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    // A memory tagged fact_id:fact_bad but whose metadata.fact_id disagrees.
    backend
        .store_memory(StoreMemoryRequest {
            content: "tampered".into(),
            memory_type: "fact".into(),
            tags: vec![
                "epistemic:fact".into(),
                "source:froid".into(),
                "fact_id:fact_bad".into(),
            ],
            metadata: json!({
                "fact_id": "fact_DIFFERENT",
                "schema_version": "psych-memory.journal_fact.v1"
            }),
        })
        .await
        .unwrap();

    let out = server
        .store_interpretation_flow(interp_input(vec!["fact_bad".into()]))
        .await
        .unwrap();

    match out {
        StoreInterpretationOutput::Rejected { error_code, .. } => {
            assert_eq!(error_code, "supporting_fact_id_mismatch");
        }
        StoreInterpretationOutput::Stored { .. } => panic!("should have been rejected"),
    }
    assert!(no_interpretation_stored(&backend).await);
}
