//! Backend-neutral persistence types.
//!
//! The mapping layer turns a validated domain object into a
//! [`StoreMemoryRequest`], which any [`crate::backend::MemoryBackend`] knows how
//! to persist. Keeping this separate from the REST DTOs means the epistemic
//! tools never depend on a specific backend transport.

use serde::Serialize;
use serde_json::Value;

/// A request to store one memory, independent of the backend transport.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoreMemoryRequest {
    pub content: String,
    pub memory_type: String,
    pub tags: Vec<String>,
    pub metadata: Value,
}

/// What the backend reports after storing a memory.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendStoreResult {
    /// The backend's own id for the stored memory (for the memory-service this
    /// is the `content_hash`). Distinct from the wrapper's `fact_id`.
    pub backend_memory_id: String,
}
