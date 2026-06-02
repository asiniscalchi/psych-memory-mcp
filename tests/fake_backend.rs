//! Behavioural tests for the in-memory backend double.
//!
//! These run with a plain `cargo test` — no memory-service required.

use psych_memory_mcp::backend::{FakeMemoryBackend, MemoryBackend};
use serde_json::json;

#[tokio::test]
async fn stores_and_reads_back() {
    let backend = FakeMemoryBackend::new();
    let hash = backend
        .store_memory(
            "FACT: x".into(),
            "fact".into(),
            vec!["epistemic:fact".into()],
            json!({ "k": "v" }),
        )
        .await
        .unwrap();

    let got = backend.get_memory(&hash).await.unwrap().expect("present");
    assert_eq!(got.content, "FACT: x");
    assert_eq!(got.memory_type, "fact");
    assert_eq!(got.tags, vec!["epistemic:fact".to_string()]);
    assert_eq!(got.metadata, json!({ "k": "v" }));
    assert_eq!(got.content_hash, hash);
}

#[tokio::test]
async fn storing_same_content_is_idempotent() {
    let backend = FakeMemoryBackend::new();
    let h1 = backend
        .store_memory("dup".into(), "fact".into(), vec![], json!({}))
        .await
        .unwrap();
    let h2 = backend
        .store_memory("dup".into(), "fact".into(), vec![], json!({}))
        .await
        .unwrap();
    assert_eq!(h1, h2, "same content must hash to the same id");
}

#[tokio::test]
async fn missing_hash_returns_none() {
    let backend = FakeMemoryBackend::new();
    assert!(backend
        .get_memory("does-not-exist")
        .await
        .unwrap()
        .is_none());
}
