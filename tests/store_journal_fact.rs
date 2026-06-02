//! Tool-level tests for `store_journal_fact`, exercised against the in-memory
//! backend so no live memory-service is required.

use std::sync::Arc;

use psych_memory_mcp::backend::{FakeMemoryBackend, MemoryBackend};
use psych_memory_mcp::model::{
    FactType, JournalEntryRef, StoreJournalFactInput, StoreJournalFactOutput,
};
use psych_memory_mcp::server::PsychMemoryServer;

fn valid_input() -> StoreJournalFactInput {
    StoreJournalFactInput {
        source_excerpt: "I felt very strong hunger after an emotionally charged conversation."
            .into(),
        normalized_statement: "Ale reported feeling strong hunger after a charged conversation."
            .into(),
        fact_type: FactType::SelfReport,
        journal_entry_ref: Some(JournalEntryRef {
            entry_id: "froid_2026_06_01_abc123".into(),
            entry_date: "2026-06-01".into(),
            char_start: Some(120),
            char_end: Some(190),
            content_hash: None,
        }),
        event_date: None,
    }
}

#[test]
fn tool_is_registered_and_no_generic_tool_exposed() {
    let router = PsychMemoryServer::tool_router();
    assert!(router.has_route("store_journal_fact"));

    let names: Vec<String> = router
        .list_all()
        .into_iter()
        .map(|t| t.name.into())
        .collect();
    assert_eq!(names, vec!["store_journal_fact".to_string()]);
    for generic in ["store_memory", "save_memory", "remember"] {
        assert!(!router.has_route(generic), "generic tool {generic} exposed");
    }
}

#[tokio::test]
async fn store_journal_fact_uses_memory_backend_and_returns_fact_id() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    let out = server
        .store_journal_fact_flow(valid_input())
        .await
        .expect("flow should not hit a backend error");

    let (fact_id, backend_memory_id) = match out {
        StoreJournalFactOutput::Stored {
            fact_id,
            backend_memory_id,
            status,
        } => {
            assert_eq!(status, "stored");
            (fact_id, backend_memory_id.expect("backend id present"))
        }
        StoreJournalFactOutput::Rejected { error_code, .. } => {
            panic!("unexpected rejection: {error_code}")
        }
    };
    assert!(fact_id.starts_with("fact_"));

    // The backend actually persisted the fact as a source-anchored memory.
    let record = backend
        .get_memory(&backend_memory_id)
        .await
        .unwrap()
        .expect("memory stored");
    assert_eq!(
        record.content,
        "I felt very strong hunger after an emotionally charged conversation."
    );
    assert_eq!(record.memory_type, "fact");
    assert!(record.tags.contains(&"epistemic:fact".to_string()));
    assert!(record.tags.contains(&format!("fact_id:{fact_id}")));
    assert_eq!(record.metadata["fact_id"], fact_id);
    assert_eq!(record.metadata["epistemic_status"], "journal_reported");
}

#[tokio::test]
async fn missing_journal_entry_ref_is_rejected_without_storing() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    let mut input = valid_input();
    input.journal_entry_ref = None;

    let out = server.store_journal_fact_flow(input).await.unwrap();
    match out {
        StoreJournalFactOutput::Rejected { error_code, .. } => {
            assert_eq!(error_code, "missing_journal_entry_ref");
        }
        StoreJournalFactOutput::Stored { .. } => panic!("should have been rejected"),
    }
}
