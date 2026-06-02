//! In-memory [`MemoryBackend`] double for tests.
//!
//! Mirrors the real service's two relevant behaviours:
//!   * records are keyed by a content hash, and
//!   * storing the same content twice is idempotent (same hash).
//!
//! The hash here is a cheap deterministic digest — it only needs to be stable
//! and collision-free for distinct content, not match the service's SHA-256.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use async_trait::async_trait;

use super::{MemoryBackend, MemoryRecord};
use crate::errors::PsychMemoryError;
use crate::model::{BackendStoreResult, StoreMemoryRequest};

#[derive(Default)]
pub struct FakeMemoryBackend {
    store: Mutex<HashMap<String, MemoryRecord>>,
}

impl FakeMemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn hash(content: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        format!("fake_{:016x}", hasher.finish())
    }
}

#[async_trait]
impl MemoryBackend for FakeMemoryBackend {
    async fn store_memory(
        &self,
        request: StoreMemoryRequest,
    ) -> Result<BackendStoreResult, PsychMemoryError> {
        let content_hash = Self::hash(&request.content);
        let record = MemoryRecord {
            content: request.content,
            memory_type: request.memory_type,
            tags: request.tags,
            content_hash: content_hash.clone(),
            metadata: request.metadata,
        };
        self.store
            .lock()
            .expect("fake backend mutex poisoned")
            .insert(content_hash.clone(), record);
        Ok(BackendStoreResult {
            backend_memory_id: content_hash,
        })
    }

    async fn get_memory(
        &self,
        content_hash: &str,
    ) -> Result<Option<MemoryRecord>, PsychMemoryError> {
        Ok(self
            .store
            .lock()
            .expect("fake backend mutex poisoned")
            .get(content_hash)
            .cloned())
    }

    async fn health(&self) -> Result<(), PsychMemoryError> {
        Ok(())
    }
}
