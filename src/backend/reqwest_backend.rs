//! Real backend adapter: the mcp-memory-service HTTP REST API.
//!
//! Verified against `doobidoo/mcp-memory-service` v10.70.3:
//!   * `POST /api/memories`            -> `{ success, message, content_hash, memory }`
//!   * `GET  /api/memories/{hash}`     -> the stored memory (404 if absent)
//!   * `GET  /api/health`              -> `{ "status": "healthy" }`

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use super::{MemoryBackend, MemoryRecord};
use crate::errors::PsychMemoryError;
use crate::model::{BackendStoreResult, StoreMemoryRequest};

#[derive(Debug, Clone)]
pub struct ReqwestMemoryBackend {
    base_url: String,
    client: Client,
}

#[derive(Deserialize)]
struct StoreResponse {
    success: bool,
    message: String,
    content_hash: Option<String>,
}

/// Shape of a stored memory as returned by the REST API. Only the fields the
/// wrapper cares about are modelled; the service sends more.
#[derive(Deserialize)]
struct MemoryDto {
    content: String,
    #[serde(default)]
    memory_type: String,
    #[serde(default)]
    tags: Vec<String>,
    content_hash: String,
    #[serde(default)]
    metadata: serde_json::Value,
}

#[derive(Deserialize)]
struct ListResponse {
    #[serde(default)]
    memories: Vec<MemoryDto>,
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
}

impl ReqwestMemoryBackend {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

impl From<reqwest::Error> for PsychMemoryError {
    fn from(e: reqwest::Error) -> Self {
        PsychMemoryError::Backend(e.to_string())
    }
}

#[async_trait]
impl MemoryBackend for ReqwestMemoryBackend {
    async fn store_memory(
        &self,
        request: StoreMemoryRequest,
    ) -> Result<BackendStoreResult, PsychMemoryError> {
        // StoreMemoryRequest serialises to exactly the REST body the service
        // expects (content / memory_type / tags / metadata).
        let resp = self
            .client
            .post(self.url("/api/memories"))
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PsychMemoryError::BackendStatus(format!(
                "store returned {status}: {text}"
            )));
        }

        let parsed: StoreResponse = resp.json().await?;
        if !parsed.success {
            return Err(PsychMemoryError::BackendStatus(parsed.message));
        }
        let backend_memory_id = parsed.content_hash.ok_or_else(|| {
            PsychMemoryError::BackendStatus("response missing content_hash".into())
        })?;
        Ok(BackendStoreResult { backend_memory_id })
    }

    async fn get_memory(
        &self,
        content_hash: &str,
    ) -> Result<Option<MemoryRecord>, PsychMemoryError> {
        let resp = self
            .client
            .get(self.url(&format!("/api/memories/{content_hash}")))
            .send()
            .await?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PsychMemoryError::BackendStatus(format!(
                "get returned {status}: {text}"
            )));
        }

        let dto: MemoryDto = resp.json().await?;
        Ok(Some(MemoryRecord {
            content: dto.content,
            memory_type: dto.memory_type,
            tags: dto.tags,
            content_hash: dto.content_hash,
            metadata: dto.metadata,
        }))
    }

    async fn find_memories_by_tag(&self, tag: &str) -> Result<Vec<MemoryRecord>, PsychMemoryError> {
        // page_size is capped at 100 by the service; an exact id-tag yields at
        // most one match, so a single page is sufficient. reqwest URL-encodes
        // the `:` in the tag value.
        let resp = self
            .client
            .get(self.url("/api/memories"))
            .query(&[("tag", tag), ("page_size", "100")])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PsychMemoryError::BackendStatus(format!(
                "list by tag returned {status}: {text}"
            )));
        }

        let parsed: ListResponse = resp.json().await?;
        Ok(parsed
            .memories
            .into_iter()
            .map(|dto| MemoryRecord {
                content: dto.content,
                memory_type: dto.memory_type,
                tags: dto.tags,
                content_hash: dto.content_hash,
                metadata: dto.metadata,
            })
            .collect())
    }

    async fn health(&self) -> Result<(), PsychMemoryError> {
        let resp = self.client.get(self.url("/api/health")).send().await?;
        if !resp.status().is_success() {
            return Err(PsychMemoryError::BackendStatus(format!(
                "health returned {}",
                resp.status()
            )));
        }
        let health: HealthResponse = resp.json().await?;
        if health.status != "healthy" {
            return Err(PsychMemoryError::BackendStatus(format!(
                "backend status is '{}'",
                health.status
            )));
        }
        Ok(())
    }
}
