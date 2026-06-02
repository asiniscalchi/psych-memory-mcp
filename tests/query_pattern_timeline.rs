//! Tool-level tests for `query_pattern_timeline`, against the in-memory backend.
//! Builds real seed/fact/occurrence records through the write flows, then reads.

use std::sync::Arc;

use psych_memory_mcp::backend::{FakeMemoryBackend, MemoryBackend};
use psych_memory_mcp::model::{
    CreatePatternSeedInput, CreatePatternSeedOutput, FactType, JournalEntryRef, OccurrencePhase,
    QueryPatternTimelineInput, QueryPatternTimelineOutput, RecordPatternOccurrenceInput,
    RecordPatternOccurrenceOutput, StoreJournalFactInput, StoreJournalFactOutput,
};
use psych_memory_mcp::server::PsychMemoryServer;

async fn make_fact(server: &PsychMemoryServer, entry_id: &str) -> String {
    let input = StoreJournalFactInput {
        source_excerpt: format!("excerpt for {entry_id}"),
        normalized_statement: "n.".into(),
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

async fn make_seed(server: &PsychMemoryServer) -> String {
    let input = CreatePatternSeedInput {
        name: "Savior".into(),
        slug: "savior".into(),
        description: "A tendency to rescue or fix.".into(),
        markers: vec!["urgency".into()],
        counter_markers: vec!["waiting".into()],
        aliases: vec![],
    };
    match server.create_pattern_seed_flow(input).await.unwrap() {
        CreatePatternSeedOutput::Stored { pattern_id, .. } => pattern_id,
        other => panic!("seed not stored: {other:?}"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn record(
    server: &PsychMemoryServer,
    pattern_id: &str,
    fact_id: &str,
    date: &str,
    phase: OccurrencePhase,
    summary: &str,
) {
    let input = RecordPatternOccurrenceInput {
        pattern_id: pattern_id.into(),
        fact_ids: vec![fact_id.into()],
        interpretation_ids: vec![],
        occurrence_date: date.into(),
        phase,
        summary: summary.into(),
        confidence: 0.5,
        intensity: None,
    };
    match server.record_pattern_occurrence_flow(input).await.unwrap() {
        RecordPatternOccurrenceOutput::Stored { .. } => {}
        RecordPatternOccurrenceOutput::Rejected { error_code, .. } => {
            panic!("occurrence rejected: {error_code}")
        }
    }
}

fn query(pattern_id: &str) -> QueryPatternTimelineInput {
    QueryPatternTimelineInput {
        pattern_id: pattern_id.into(),
        date_from: None,
        date_to: None,
        phases: vec![],
        include_invalid_record_warnings: None,
    }
}

#[test]
fn exactly_five_tools_exposed_and_no_generic() {
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
            "query_pattern_timeline".to_string(),
            "record_pattern_occurrence".to_string(),
            "store_interpretation".to_string(),
            "store_journal_fact".to_string(),
        ]
    );
    for generic in ["store_memory", "save_memory", "remember"] {
        assert!(!router.has_route(generic));
    }
}

#[tokio::test]
async fn returns_timeline_grouped_by_date() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let fact = make_fact(&server, "e1").await;
    let pattern_id = make_seed(&server).await;

    record(
        &server,
        &pattern_id,
        &fact,
        "2026-06-12",
        OccurrencePhase::NotActivated,
        "not seen",
    )
    .await;
    record(
        &server,
        &pattern_id,
        &fact,
        "2026-06-01",
        OccurrencePhase::RecognizedBeforeAction,
        "noticed early",
    )
    .await;

    let out = server
        .query_pattern_timeline_flow(query(&pattern_id))
        .await
        .unwrap();
    match out {
        QueryPatternTimelineOutput::Found {
            total_occurrences,
            timeline,
            warnings,
            pattern,
            ..
        } => {
            assert_eq!(total_occurrences, 2);
            assert!(warnings.is_empty());
            assert_eq!(pattern.name, "Savior");
            // Sorted ascending by date.
            assert_eq!(timeline[0].date, "2026-06-01");
            assert_eq!(timeline[1].date, "2026-06-12");
        }
        QueryPatternTimelineOutput::Rejected { error_code, .. } => panic!("rejected: {error_code}"),
    }
}

#[tokio::test]
async fn empty_timeline_for_existing_pattern() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let pattern_id = make_seed(&server).await;

    let out = server
        .query_pattern_timeline_flow(query(&pattern_id))
        .await
        .unwrap();
    match out {
        QueryPatternTimelineOutput::Found {
            total_occurrences,
            timeline,
            ..
        } => {
            assert_eq!(total_occurrences, 0);
            assert!(timeline.is_empty());
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_unknown_pattern() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let out = server
        .query_pattern_timeline_flow(query("pattern_missing"))
        .await
        .unwrap();
    assert!(matches!(
        out,
        QueryPatternTimelineOutput::Rejected { error_code, .. } if error_code == "unknown_pattern_seed"
    ));
}

#[tokio::test]
async fn returns_structured_error_on_invalid_input() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let mut input = query("pattern_savior");
    input.date_from = Some("banana".into());
    let out = server.query_pattern_timeline_flow(input).await.unwrap();
    assert!(matches!(
        out,
        QueryPatternTimelineOutput::Rejected { error_code, .. } if error_code == "invalid_date_from"
    ));
}

#[tokio::test]
async fn query_is_read_only() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let fact = make_fact(&server, "e1").await;
    let pattern_id = make_seed(&server).await;
    record(
        &server,
        &pattern_id,
        &fact,
        "2026-06-01",
        OccurrencePhase::Activated,
        "seen",
    )
    .await;

    // Snapshot the whole store, run the query, and confirm nothing changed.
    let before = backend
        .find_memories_by_tag("epistemic:pattern_occurrence")
        .await
        .unwrap();
    let seeds_before = backend
        .find_memories_by_tag("epistemic:pattern_seed")
        .await
        .unwrap();

    let _ = server
        .query_pattern_timeline_flow(query(&pattern_id))
        .await
        .unwrap();

    let after = backend
        .find_memories_by_tag("epistemic:pattern_occurrence")
        .await
        .unwrap();
    let seeds_after = backend
        .find_memories_by_tag("epistemic:pattern_seed")
        .await
        .unwrap();
    assert_eq!(before, after, "occurrences must be unchanged by a query");
    assert_eq!(
        seeds_before, seeds_after,
        "seeds must be unchanged by a query"
    );
}

#[tokio::test]
async fn output_has_no_trend_or_activation_fields() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());
    let pattern_id = make_seed(&server).await;
    let out = server
        .query_pattern_timeline_flow(query(&pattern_id))
        .await
        .unwrap();
    let value = serde_json::to_value(&out).unwrap();
    let obj = value.as_object().unwrap();
    for forbidden in [
        "trend",
        "active",
        "active_since",
        "last_seen",
        "conclusion",
        "psychological_summary",
    ] {
        assert!(!obj.contains_key(forbidden), "output has {forbidden}");
    }
}
