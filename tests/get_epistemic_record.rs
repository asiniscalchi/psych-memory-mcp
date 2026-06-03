//! Tool-level tests for `get_epistemic_record`, against the in-memory backend.
//! Builds real records via the write flows, then reads them back by id.

use std::sync::Arc;

use psych_memory_mcp::backend::{FakeMemoryBackend, MemoryBackend};
use psych_memory_mcp::model::{
    CreatePatternSeedInput, CreatePatternSeedOutput, FactType, GetEpistemicRecordInput,
    GetEpistemicRecordOutput, JournalEntryRef, StoreJournalFactInput, StoreJournalFactOutput,
};
use psych_memory_mcp::server::PsychMemoryServer;

async fn make_fact(server: &PsychMemoryServer) -> String {
    let input = StoreJournalFactInput {
        source_excerpt: "I felt strong hunger.".into(),
        normalized_statement: "Ale reported hunger.".into(),
        fact_type: FactType::SelfReport,
        journal_entry_ref: Some(JournalEntryRef {
            entry_id: "e1".into(),
            entry_date: "2026-06-01".into(),
            char_start: Some(0),
            char_end: Some(10),
            content_hash: None,
        }),
        event_date: None,
    };
    match server.store_journal_fact_flow(input).await.unwrap() {
        StoreJournalFactOutput::Stored { fact_id, .. } => fact_id,
        StoreJournalFactOutput::Rejected { error_code, .. } => {
            panic!("fact rejected: {error_code}")
        }
    }
}

async fn make_seed(server: &PsychMemoryServer) -> String {
    let input = CreatePatternSeedInput {
        name: "Savior".into(),
        slug: "savior".into(),
        description: "rescue or fix.".into(),
        markers: vec!["urgency".into()],
        counter_markers: vec!["waiting".into()],
        aliases: vec![],
    };
    match server.create_pattern_seed_flow(input).await.unwrap() {
        CreatePatternSeedOutput::Stored { pattern_id, .. } => pattern_id,
        other => panic!("seed not stored: {other:?}"),
    }
}

fn input(id: &str) -> GetEpistemicRecordInput {
    GetEpistemicRecordInput { id: id.into() }
}

#[test]
fn exactly_six_tools_exposed_and_no_generic() {
    let router = PsychMemoryServer::tool_router();
    let mut names: Vec<String> = router
        .list_all()
        .into_iter()
        .map(|t| t.name.into())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "create_pattern_seed".to_string(),
            "get_epistemic_record".to_string(),
            "query_pattern_timeline".to_string(),
            "record_pattern_occurrence".to_string(),
            "store_interpretation".to_string(),
            "store_journal_fact".to_string(),
        ]
    );
    for generic in ["store_memory", "save_memory", "remember", "search"] {
        assert!(!router.has_route(generic));
    }
}

#[tokio::test]
async fn retrieves_fact_and_seed_by_id() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let fact_id = make_fact(&server).await;
    let pattern_id = make_seed(&server).await;

    let out = server
        .get_epistemic_record_flow(input(&fact_id))
        .await
        .unwrap();
    match out {
        GetEpistemicRecordOutput::Found {
            record_type,
            id,
            backend_memory_id,
            ..
        } => {
            assert_eq!(record_type.as_str(), "journal_fact");
            assert_eq!(id, fact_id);
            assert!(backend_memory_id.is_some());
        }
        GetEpistemicRecordOutput::Rejected { error_code, .. } => panic!("rejected: {error_code}"),
    }

    let seed_out = server
        .get_epistemic_record_flow(input(&pattern_id))
        .await
        .unwrap();
    assert!(matches!(
        seed_out,
        GetEpistemicRecordOutput::Found { record_type, .. } if record_type.as_str() == "pattern_seed"
    ));
}

#[tokio::test]
async fn rejects_empty_and_unsupported_and_unknown() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    for (id, code) in [
        ("", "missing_epistemic_id"),
        ("foo_123", "unsupported_epistemic_id"),
        ("fact_missing", "unknown_epistemic_record"),
    ] {
        let out = server.get_epistemic_record_flow(input(id)).await.unwrap();
        assert!(
            matches!(
                out,
                GetEpistemicRecordOutput::Rejected { error_code, .. } if error_code == code
            ),
            "id {id} expected {code}"
        );
    }
}

#[tokio::test]
async fn query_is_read_only() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let fact_id = make_fact(&server).await;

    let before = backend
        .find_memories_by_tag("epistemic:fact")
        .await
        .unwrap();
    let _ = server
        .get_epistemic_record_flow(input(&fact_id))
        .await
        .unwrap();
    let after = backend
        .find_memories_by_tag("epistemic:fact")
        .await
        .unwrap();
    assert_eq!(before, after, "records must be unchanged by a read");
}

#[tokio::test]
async fn output_has_no_inference_fields() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let fact_id = make_fact(&server).await;

    let out = server
        .get_epistemic_record_flow(input(&fact_id))
        .await
        .unwrap();
    let value = serde_json::to_value(&out).unwrap();
    let obj = value.as_object().unwrap();
    for forbidden in [
        "trend",
        "active",
        "conclusion",
        "advice",
        "psychological_summary",
    ] {
        assert!(!obj.contains_key(forbidden), "output has {forbidden}");
    }
    // Primary id is the epistemic id; backend id is secondary.
    assert_eq!(obj["id"], serde_json::json!(fact_id));
}
