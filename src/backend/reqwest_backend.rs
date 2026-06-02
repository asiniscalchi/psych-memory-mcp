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
    #[serde(default)]
    has_more: bool,
}

/// Accumulate every page of a paginated listing. `fetch(page)` returns that
/// page's records and whether more pages follow; a fetch error aborts (no
/// partial result). Stops on `has_more == false` or an empty page (guard
/// against a backend that never clears `has_more`).
async fn collect_all_pages<F, Fut>(mut fetch: F) -> Result<Vec<MemoryRecord>, PsychMemoryError>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<(Vec<MemoryRecord>, bool), PsychMemoryError>>,
{
    let mut all = Vec::new();
    let mut page = 1u32;
    loop {
        let (records, has_more) = fetch(page).await?;
        let empty = records.is_empty();
        all.extend(records);
        if !has_more || empty {
            break;
        }
        page += 1;
    }
    Ok(all)
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

    /// Fetch one page of memories carrying `tag`, returning the page's records
    /// and whether more pages follow.
    async fn fetch_tag_page(
        &self,
        tag: &str,
        page: u32,
    ) -> Result<(Vec<MemoryRecord>, bool), PsychMemoryError> {
        let page = page.to_string();
        let resp = self
            .client
            .get(self.url("/api/memories"))
            .query(&[("tag", tag), ("page", &page), ("page_size", "100")])
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
        let records = parsed
            .memories
            .into_iter()
            .map(|dto| MemoryRecord {
                content: dto.content,
                memory_type: dto.memory_type,
                tags: dto.tags,
                content_hash: dto.content_hash,
                metadata: dto.metadata,
            })
            .collect();
        Ok((records, parsed.has_more))
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
        // A tag may be non-unique (e.g. pattern_id:<id> matches many
        // occurrences), so exhaust every page — never silently truncate.
        collect_all_pages(|page| self.fetch_tag_page(tag, page)).await
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

#[cfg(test)]
mod pagination_tests {
    use super::*;
    use std::cell::RefCell;

    fn rec(id: &str) -> MemoryRecord {
        MemoryRecord {
            content: id.into(),
            memory_type: "x".into(),
            tags: vec![],
            content_hash: id.into(),
            metadata: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn find_memories_by_tag_fetches_all_pages() {
        let out = collect_all_pages(|page| async move {
            match page {
                1 => Ok((vec![rec("a"), rec("b")], true)),
                2 => Ok((vec![rec("c")], false)),
                _ => panic!("requested page {page}"),
            }
        })
        .await
        .unwrap();
        let ids: Vec<String> = out.into_iter().map(|r| r.content).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn find_memories_by_tag_returns_empty_when_no_pages() {
        let out = collect_all_pages(|_page| async move { Ok((vec![], false)) })
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn find_memories_by_tag_errors_if_second_page_fails() {
        let result = collect_all_pages(|page| async move {
            match page {
                1 => Ok((vec![rec("a")], true)),
                _ => Err(PsychMemoryError::Backend("page 2 failed".into())),
            }
        })
        .await;
        assert!(matches!(result, Err(PsychMemoryError::Backend(_))));
    }

    #[tokio::test]
    async fn stops_if_backend_never_clears_has_more() {
        // has_more stuck true but an empty page must still terminate the loop.
        let pages = RefCell::new(0u32);
        let out = collect_all_pages(|page| {
            *pages.borrow_mut() = page;
            async move {
                if page == 1 {
                    Ok((vec![rec("a")], true))
                } else {
                    Ok((vec![], true)) // empty but has_more still true
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(*pages.borrow(), 2);
    }
}
