//! The MCP server surface.
//!
//! Only explicit epistemic tools are exposed — there is no generic
//! `store_memory` / `save_memory` / `remember`. Story 1 adds the first tool,
//! `store_journal_fact`.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};

use crate::backend::MemoryBackend;
use crate::errors::PsychMemoryError;
use crate::fact_id::generate_fact_id;
use crate::mapping::map_store_journal_fact_to_backend_request;
use crate::model::{StoreJournalFactInput, StoreJournalFactOutput};
use crate::validators::validate_store_journal_fact;

#[derive(Clone)]
pub struct PsychMemoryServer {
    backend: Arc<dyn MemoryBackend>,
    tool_router: ToolRouter<Self>,
}

impl PsychMemoryServer {
    pub fn new(backend: Arc<dyn MemoryBackend>) -> Self {
        Self {
            backend,
            tool_router: Self::tool_router(),
        }
    }

    /// The `store_journal_fact` flow, independent of the MCP envelope.
    ///
    /// Returns a structured outcome: validation failures become
    /// [`StoreJournalFactOutput::Rejected`] (a tool-level error), while a
    /// backend failure is propagated as `Err` (an infrastructure problem).
    pub async fn store_journal_fact_flow(
        &self,
        input: StoreJournalFactInput,
    ) -> Result<StoreJournalFactOutput, PsychMemoryError> {
        let validated = match validate_store_journal_fact(&input) {
            Ok(v) => v,
            Err(err) => {
                return Ok(StoreJournalFactOutput::Rejected {
                    error_code: err.error_code().to_string(),
                    message: err.to_string(),
                });
            }
        };

        let fact_id = generate_fact_id(&validated);
        let request = map_store_journal_fact_to_backend_request(&validated, &fact_id);
        let stored = self.backend.store_memory(request).await?;

        Ok(StoreJournalFactOutput::Stored {
            fact_id,
            backend_memory_id: Some(stored.backend_memory_id),
            status: "stored".to_string(),
        })
    }
}

#[tool_router(vis = "pub")]
impl PsychMemoryServer {
    #[tool(
        description = "Store a journal-derived fact anchored to its raw Froid source excerpt. \
                       Provide the verbatim source_excerpt, a normalized_statement, a fact_type, \
                       and a journal_entry_ref. All facts are recorded as journal-reported; the \
                       system never claims to have observed reality."
    )]
    async fn store_journal_fact(
        &self,
        Parameters(input): Parameters<StoreJournalFactInput>,
    ) -> Result<CallToolResult, ErrorData> {
        // A backend failure is infrastructure, not a tool-input problem, so it
        // surfaces as a protocol-level error. Validation failures, by contrast,
        // come back as Rejected and become a structured tool error (is_error).
        let outcome = self
            .store_journal_fact_flow(input)
            .await
            .map_err(|e| ErrorData::internal_error(format!("backend store failed: {e}"), None))?;

        let content = Content::json(&outcome)?;
        match outcome {
            StoreJournalFactOutput::Stored { .. } => Ok(CallToolResult::success(vec![content])),
            StoreJournalFactOutput::Rejected { .. } => Ok(CallToolResult::error(vec![content])),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PsychMemoryServer {
    fn get_info(&self) -> ServerInfo {
        // ServerInfo is #[non_exhaustive]; build from default and set fields.
        let mut info = ServerInfo::default();
        // env! expands in this crate, so it names psych-memory-mcp (not rmcp).
        let mut implementation = Implementation::default();
        implementation.name = env!("CARGO_PKG_NAME").to_string();
        implementation.version = env!("CARGO_PKG_VERSION").to_string();
        info.server_info = implementation;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "Psychological memory wrapper. Only explicit epistemic tools are \
             exposed; there is no generic memory-writing tool."
                .to_string(),
        );
        info
    }
}
