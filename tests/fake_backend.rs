//! Behavioural tests for the in-memory backend double.
//!
//! These run with a plain `cargo test` — no memory-service required.

use psych_memory_mcp::backend::{FakeMemoryBackend, MemoryBackend};
use psych_memory_mcp::model::StoreMemoryRequest;
use serde_json::json;

fn request(content: &str, tags: Vec<String>, metadata: serde_json::Value) -> StoreMemoryRequest {
    StoreMemoryRequest {
        content: content.to_string(),
        memory_type: "fact".into(),
        tags,
        metadata,
    }
}

#[tokio::test]
async fn stores_and_reads_back() {
    let backend = FakeMemoryBackend::new();
    let stored = backend
        .store_memory(request(
            "FACT: x",
            vec!["epistemic:fact".into()],
            json!({ "k": "v" }),
        ))
        .await
        .unwrap();

    let got = backend
        .get_memory(&stored.backend_memory_id)
        .await
        .unwrap()
        .expect("present");
    assert_eq!(got.content, "FACT: x");
    assert_eq!(got.memory_type, "fact");
    assert_eq!(got.tags, vec!["epistemic:fact".to_string()]);
    assert_eq!(got.metadata, json!({ "k": "v" }));
    assert_eq!(got.content_hash, stored.backend_memory_id);
}

#[tokio::test]
async fn storing_same_content_is_idempotent() {
    let backend = FakeMemoryBackend::new();
    let r1 = backend
        .store_memory(request("dup", vec![], json!({})))
        .await
        .unwrap();
    let r2 = backend
        .store_memory(request("dup", vec![], json!({})))
        .await
        .unwrap();
    assert_eq!(
        r1.backend_memory_id, r2.backend_memory_id,
        "same content must hash to the same id"
    );
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
