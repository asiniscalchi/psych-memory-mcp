//! End-to-end transport proof: store a memory through the wrapper's real
//! backend adapter and read it back from a running mcp-memory-service.
//!
//! Ignored by default (needs a live service). Run against the compose stack:
//!
//! ```text
//! docker compose up -d
//! MEMORY_BASE_URL=http://127.0.0.1:8000 cargo test --test roundtrip -- --ignored
//! ```
//!
//! The `memory_type == "fact"` assertion is deliberate: it fails unless the
//! memory-service is started with `MCP_CUSTOM_MEMORY_TYPES` registering `fact`
//! (otherwise the service silently downgrades it to `observation`). That makes
//! this test a guard on the compose configuration, not just the transport.

use psych_memory_mcp::backend::{MemoryBackend, ReqwestMemoryBackend};
use psych_memory_mcp::model::StoreMemoryRequest;
use serde_json::json;

fn base_url() -> String {
    std::env::var("MEMORY_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".into())
}

#[tokio::test]
#[ignore = "requires a running mcp-memory-service (see module docs)"]
async fn store_and_read_back_through_real_service() {
    let base = base_url();
    let backend = ReqwestMemoryBackend::new(base.clone());

    backend
        .health()
        .await
        .expect("memory-service should be healthy");

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let content = format!("FACT: story0 roundtrip {nonce}");

    let stored = backend
        .store_memory(StoreMemoryRequest {
            content: content.clone(),
            memory_type: "fact".into(),
            tags: vec!["epistemic:fact".into(), "source:test".into()],
            metadata: json!({ "test": true }),
        })
        .await
        .expect("store should succeed");
    let hash = stored.backend_memory_id;

    let got = backend
        .get_memory(&hash)
        .await
        .expect("get should succeed")
        .expect("memory should exist");

    assert_eq!(got.content, content);
    assert_eq!(got.content_hash, hash);
    assert!(got.tags.contains(&"epistemic:fact".to_string()));
    assert_eq!(
        got.memory_type, "fact",
        "memory_type was '{}' — is MCP_CUSTOM_MEMORY_TYPES set on the service?",
        got.memory_type
    );

    // Clean up so the test does not pollute the store.
    let _ = reqwest::Client::new()
        .delete(format!(
            "{}/api/memories/{}",
            base.trim_end_matches('/'),
            hash
        ))
        .send()
        .await;
}
