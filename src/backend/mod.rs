//! Backend adapter for the underlying memory-service.
//!
//! Everything the wrapper persists goes through the [`MemoryBackend`] trait.
//! There are two implementations:
//!
//! * [`ReqwestMemoryBackend`] — the real adapter, speaking the mcp-memory-service
//!   HTTP REST API (`POST /api/memories`, `GET /api/memories/{content_hash}`,
//!   `GET /api/health`).
//! * [`FakeMemoryBackend`] — an in-memory double used by unit tests so behaviour
//!   can be verified without a running service.
//!
//! Keeping both behind one trait is what lets Story 0 prove the real transport
//! once (the `#[ignore]`d round-trip integration test) while everything else is
//! tested against the fake.

mod fake;
mod reqwest_backend;

pub use fake::FakeMemoryBackend;
pub use reqwest_backend::ReqwestMemoryBackend;

use crate::errors::PsychMemoryError;
use crate::model::{BackendStoreResult, StoreMemoryRequest};
use async_trait::async_trait;

/// A memory as returned by the backend.
///
/// The memory-service identifies records by `content_hash` (a SHA-256 of the
/// content); the wrapper surfaces this as `backend_memory_id`. It is distinct
/// from the wrapper's own `fact_id`, which is what later epistemic links point
/// at.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryRecord {
    pub content: String,
    pub memory_type: String,
    pub tags: Vec<String>,
    pub content_hash: String,
    pub metadata: serde_json::Value,
}

#[async_trait]
pub trait MemoryBackend: Send + Sync {
    /// Store a memory and return the backend's id for it.
    async fn store_memory(
        &self,
        request: StoreMemoryRequest,
    ) -> Result<BackendStoreResult, PsychMemoryError>;

    /// Fetch a memory by `content_hash`. Returns `None` if it does not exist.
    async fn get_memory(
        &self,
        content_hash: &str,
    ) -> Result<Option<MemoryRecord>, PsychMemoryError>;

    /// Return every memory carrying the exact `tag`.
    ///
    /// This is deliberately generic transport: it knows nothing about facts or
    /// interpretations. Epistemic rules (which memory counts as a valid fact,
    /// ambiguity, metadata checks) live in the domain layer.
    async fn find_memories_by_tag(&self, tag: &str) -> Result<Vec<MemoryRecord>, PsychMemoryError>;

    /// Liveness/readiness check against the backend.
    async fn health(&self) -> Result<(), PsychMemoryError>;
}
