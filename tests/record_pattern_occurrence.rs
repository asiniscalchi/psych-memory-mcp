//! Tool-level tests for `record_pattern_occurrence`, against the in-memory
//! backend. Builds real fact / interpretation / pattern-seed records through
//! the other tool flows so the resolvers have something to find.

use std::sync::Arc;

use psych_memory_mcp::backend::{FakeMemoryBackend, MemoryBackend};
use psych_memory_mcp::model::{
    CreatePatternSeedInput, CreatePatternSeedOutput, FactType, InterpretationType, JournalEntryRef,
    OccurrencePhase, RecordPatternOccurrenceInput, RecordPatternOccurrenceOutput,
    StoreInterpretationInput, StoreInterpretationOutput, StoreJournalFactInput,
    StoreJournalFactOutput,
};
use psych_memory_mcp::server::PsychMemoryServer;

async fn make_fact(server: &PsychMemoryServer, entry_id: &str, excerpt: &str) -> String {
    let input = StoreJournalFactInput {
        source_excerpt: excerpt.into(),
        normalized_statement: "Ale reported something.".into(),
        fact_type: FactType::SelfReport,
        journal_entry_ref: Some(JournalEntryRef {
            entry_id: entry_id.into(),
            entry_date: "2026-06-01".into(),
            char_start: None,
            char_end: None,
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

async fn make_interpretation(server: &PsychMemoryServer, fact_id: &str) -> String {
    let input = StoreInterpretationInput {
        hypothesis: "Hunger may have functioned as emotional discharge.".into(),
        interpretation_type: InterpretationType::PsychologicalHypothesis,
        supported_by_fact_ids: vec![fact_id.into()],
        contradicted_by_fact_ids: vec![],
        confidence: 0.3,
        status: None,
        falsification_question: "Episodes without activation?".into(),
        review_due: None,
    };
    match server.store_interpretation_flow(input).await.unwrap() {
        StoreInterpretationOutput::Stored {
            interpretation_id, ..
        } => interpretation_id,
        StoreInterpretationOutput::Rejected { error_code, .. } => {
            panic!("interp rejected: {error_code}")
        }
    }
}

async fn make_pattern_seed(server: &PsychMemoryServer) -> String {
    let input = CreatePatternSeedInput {
        name: "Savior".into(),
        slug: "savior".into(),
        description: "A tendency to rescue or fix the other person.".into(),
        markers: vec!["urgency to intervene".into()],
        counter_markers: vec!["ability to wait".into()],
        aliases: vec![],
    };
    match server.create_pattern_seed_flow(input).await.unwrap() {
        CreatePatternSeedOutput::Stored { pattern_id, .. } => pattern_id,
        other => panic!("seed not stored: {other:?}"),
    }
}

fn occurrence(
    pattern_id: &str,
    fact_ids: Vec<String>,
    interpretation_ids: Vec<String>,
) -> RecordPatternOccurrenceInput {
    RecordPatternOccurrenceInput {
        pattern_id: pattern_id.into(),
        fact_ids,
        interpretation_ids,
        occurrence_date: "2026-06-01".into(),
        phase: OccurrencePhase::RecognizedBeforeAction,
        summary: "The rescue impulse appeared but was noticed before being acted out.".into(),
        confidence: 0.55,
        intensity: Some(0.45),
    }
}

async fn occurrence_count(backend: &FakeMemoryBackend) -> usize {
    backend
        .find_memories_by_tag("epistemic:pattern_occurrence")
        .await
        .unwrap()
        .len()
}

#[test]
fn occurrence_tool_exposed_and_no_generic() {
    let router = PsychMemoryServer::tool_router();
    assert!(router.has_route("record_pattern_occurrence"));
    for generic in ["store_memory", "save_memory", "remember"] {
        assert!(!router.has_route(generic));
    }
}

#[tokio::test]
async fn record_pattern_occurrence_uses_memory_backend_and_returns_occurrence_id() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    let fact_id = make_fact(&server, "e1", "I felt strong hunger.").await;
    let interp_id = make_interpretation(&server, &fact_id).await;
    let pattern_id = make_pattern_seed(&server).await;

    let out = server
        .record_pattern_occurrence_flow(occurrence(
            &pattern_id,
            vec![fact_id.clone()],
            vec![interp_id.clone()],
        ))
        .await
        .unwrap();

    let backend_memory_id = match out {
        RecordPatternOccurrenceOutput::Stored {
            occurrence_id,
            backend_memory_id,
            status,
        } => {
            assert!(occurrence_id.starts_with("occ_"));
            assert_eq!(status, "stored");
            backend_memory_id.unwrap()
        }
        RecordPatternOccurrenceOutput::Rejected { error_code, .. } => {
            panic!("unexpected rejection: {error_code}")
        }
    };

    let record = backend
        .get_memory(&backend_memory_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.memory_type, "pattern_occurrence");
    assert!(record.tags.contains(&format!("pattern_id:{pattern_id}")));
    assert!(record.tags.contains(&format!("supported_by:{fact_id}")));
    assert!(record
        .tags
        .contains(&format!("linked_interpretation:{interp_id}")));
    assert!(record
        .tags
        .contains(&"phase:recognized_before_action".to_string()));
    assert_eq!(
        record.metadata["epistemic_status"],
        "evidence_linked_occurrence"
    );
}

#[tokio::test]
async fn does_not_store_when_validation_fails() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let pattern_id = make_pattern_seed(&server).await;
    let fact_id = make_fact(&server, "e1", "x").await;

    let mut input = occurrence(&pattern_id, vec![fact_id], vec![]);
    input.occurrence_date = "banana".into();

    let out = server.record_pattern_occurrence_flow(input).await.unwrap();
    assert!(matches!(
        out,
        RecordPatternOccurrenceOutput::Rejected { error_code, .. } if error_code == "invalid_occurrence_date"
    ));
    assert_eq!(occurrence_count(&backend).await, 0);
}

#[tokio::test]
async fn does_not_store_when_pattern_lookup_fails() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let fact_id = make_fact(&server, "e1", "x").await;

    let out = server
        .record_pattern_occurrence_flow(occurrence("pattern_missing", vec![fact_id], vec![]))
        .await
        .unwrap();
    assert!(matches!(
        out,
        RecordPatternOccurrenceOutput::Rejected { error_code, .. } if error_code == "unknown_pattern_seed"
    ));
    assert_eq!(occurrence_count(&backend).await, 0);
}

#[tokio::test]
async fn does_not_store_when_fact_lookup_fails() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let pattern_id = make_pattern_seed(&server).await;

    let out = server
        .record_pattern_occurrence_flow(occurrence(
            &pattern_id,
            vec!["fact_missing".into()],
            vec![],
        ))
        .await
        .unwrap();
    assert!(matches!(
        out,
        RecordPatternOccurrenceOutput::Rejected { error_code, .. } if error_code == "unknown_supporting_fact"
    ));
    assert_eq!(occurrence_count(&backend).await, 0);
}

#[tokio::test]
async fn does_not_store_when_interpretation_lookup_fails() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let pattern_id = make_pattern_seed(&server).await;
    let fact_id = make_fact(&server, "e1", "x").await;

    let out = server
        .record_pattern_occurrence_flow(occurrence(
            &pattern_id,
            vec![fact_id],
            vec!["interp_missing".into()],
        ))
        .await
        .unwrap();
    assert!(matches!(
        out,
        RecordPatternOccurrenceOutput::Rejected { error_code, .. } if error_code == "unknown_interpretation"
    ));
    assert_eq!(occurrence_count(&backend).await, 0);
}

#[tokio::test]
async fn does_not_mutate_pattern_seed() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let fact_id = make_fact(&server, "e1", "I felt strong hunger.").await;
    let pattern_id = make_pattern_seed(&server).await;

    // Snapshot the seed record before recording an occurrence.
    let seed_before = backend
        .find_memories_by_tag(&format!("pattern_id:{pattern_id}"))
        .await
        .unwrap();
    assert_eq!(seed_before.len(), 1);
    let before = seed_before[0].clone();

    server
        .record_pattern_occurrence_flow(occurrence(&pattern_id, vec![fact_id], vec![]))
        .await
        .unwrap();

    let after = backend
        .get_memory(&before.content_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.memory_type, "pattern_seed");
    assert_eq!(after.content, before.content);
    assert_eq!(after.tags, before.tags);
    assert_eq!(after.metadata, before.metadata);
}
