//! Tool-level tests for `create_pattern_seed`, against the in-memory backend.

use std::sync::Arc;

use psych_memory_mcp::backend::{FakeMemoryBackend, MemoryBackend};
use psych_memory_mcp::model::{
    CreatePatternSeedInput, CreatePatternSeedOutput, StoreMemoryRequest,
};
use psych_memory_mcp::server::PsychMemoryServer;
use serde_json::json;

fn input(slug: &str, description: &str) -> CreatePatternSeedInput {
    CreatePatternSeedInput {
        name: "Savior".into(),
        slug: slug.into(),
        description: description.into(),
        markers: vec!["urgency to intervene".into()],
        counter_markers: vec!["ability to wait".into()],
        aliases: vec!["rescuer".into()],
    }
}

/// A raw pattern-seed memory, for setting up ambiguous/invalid lookup states.
fn raw_seed(pattern_id: &str, content: &str) -> StoreMemoryRequest {
    StoreMemoryRequest {
        content: content.into(),
        memory_type: "pattern_seed".into(),
        tags: vec![
            "epistemic:pattern_seed".into(),
            "status:seed".into(),
            format!("pattern_id:{pattern_id}"),
        ],
        metadata: json!({
            "pattern_id": pattern_id,
            "schema_version": "psych-memory.pattern_seed.v1",
        }),
    }
}

async fn seed_count(backend: &FakeMemoryBackend) -> usize {
    backend
        .find_memories_by_tag("epistemic:pattern_seed")
        .await
        .unwrap()
        .len()
}

#[test]
fn exactly_three_tools_exposed_and_no_generic() {
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
            "store_interpretation".to_string(),
            "store_journal_fact".to_string(),
        ]
    );
    for generic in ["store_memory", "save_memory", "remember"] {
        assert!(!router.has_route(generic));
    }
}

#[tokio::test]
async fn create_pattern_seed_uses_memory_backend_and_returns_pattern_id() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    let out = server
        .create_pattern_seed_flow(input("savior", "A tendency to rescue or fix the other."))
        .await
        .unwrap();

    let backend_memory_id = match out {
        CreatePatternSeedOutput::Stored {
            pattern_id,
            backend_memory_id,
        } => {
            assert_eq!(pattern_id, "pattern_savior");
            backend_memory_id.unwrap()
        }
        other => panic!("expected Stored, got {other:?}"),
    };

    let record = backend
        .get_memory(&backend_memory_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.memory_type, "pattern_seed");
    assert!(record
        .tags
        .contains(&"pattern_id:pattern_savior".to_string()));
    assert!(record.tags.contains(&"pattern_slug:savior".to_string()));
    assert!(record.tags.contains(&"pattern_alias:rescuer".to_string()));
    assert_eq!(record.metadata["status"], "seed");
    assert_eq!(record.metadata["epistemic_status"], "observation_category");
}

#[tokio::test]
async fn create_pattern_seed_returns_already_exists_when_seed_exists() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    server
        .create_pattern_seed_flow(input("savior", "first description"))
        .await
        .unwrap();
    assert_eq!(seed_count(&backend).await, 1);

    let out = server
        .create_pattern_seed_flow(input("savior", "first description"))
        .await
        .unwrap();
    assert!(matches!(
        out,
        CreatePatternSeedOutput::AlreadyExists { pattern_id, .. } if pattern_id == "pattern_savior"
    ));
    assert_eq!(seed_count(&backend).await, 1, "no duplicate stored");
}

#[tokio::test]
async fn same_slug_refined_description_returns_already_exists_without_mutating() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    let first = server
        .create_pattern_seed_flow(input("savior", "ORIGINAL description"))
        .await
        .unwrap();
    let original_id = match first {
        CreatePatternSeedOutput::Stored {
            backend_memory_id, ..
        } => backend_memory_id.unwrap(),
        other => panic!("expected Stored, got {other:?}"),
    };

    let out = server
        .create_pattern_seed_flow(input("savior", "REFINED different description"))
        .await
        .unwrap();
    assert!(matches!(out, CreatePatternSeedOutput::AlreadyExists { .. }));

    // The original record is untouched; the refinement was not applied.
    let record = backend.get_memory(&original_id).await.unwrap().unwrap();
    assert_eq!(record.metadata["description"], "ORIGINAL description");
    assert_eq!(seed_count(&backend).await, 1);
}

#[tokio::test]
async fn create_pattern_seed_does_not_store_when_validation_fails() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    let out = server
        .create_pattern_seed_flow(input("Bad-Slug", "desc"))
        .await
        .unwrap();
    assert!(matches!(
        out,
        CreatePatternSeedOutput::Rejected { error_code, .. } if error_code == "invalid_pattern_slug"
    ));
    assert_eq!(seed_count(&backend).await, 0);
}

#[tokio::test]
async fn create_pattern_seed_does_not_store_when_existing_seed_is_ambiguous() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    // Two valid seeds already share pattern_savior.
    backend
        .store_memory(raw_seed("pattern_savior", "one"))
        .await
        .unwrap();
    backend
        .store_memory(raw_seed("pattern_savior", "two"))
        .await
        .unwrap();

    let out = server
        .create_pattern_seed_flow(input("savior", "desc"))
        .await
        .unwrap();
    assert!(matches!(
        out,
        CreatePatternSeedOutput::Rejected { error_code, .. } if error_code == "ambiguous_pattern_seed"
    ));
    assert_eq!(seed_count(&backend).await, 2, "nothing new stored");
}

#[tokio::test]
async fn create_pattern_seed_does_not_store_when_existing_seed_match_is_invalid() {
    let backend = Arc::new(FakeMemoryBackend::new());
    let server = PsychMemoryServer::new(backend.clone());

    // A memory tagged pattern_savior but not a valid pattern seed.
    let mut bad = raw_seed("pattern_savior", "corrupt");
    bad.memory_type = "observation".into();
    backend.store_memory(bad).await.unwrap();

    let out = server
        .create_pattern_seed_flow(input("savior", "desc"))
        .await
        .unwrap();
    assert!(matches!(
        out,
        CreatePatternSeedOutput::Rejected { error_code, .. } if error_code == "invalid_pattern_seed_match"
    ));
}
