//! The MCP server surface.
//!
//! Story 0 deliberately exposes **no tools at all** — in particular no generic
//! `store_memory` / `save_memory` / `remember` (AC6). The epistemic tools
//! (`store_fact`, ...) arrive in later stories and will be attached here. The
//! backend is already wired in so those stories only add the tool, not the
//! plumbing.

use std::sync::Arc;

use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::ServerHandler;

use crate::backend::MemoryBackend;

#[derive(Clone)]
pub struct PsychMemoryServer {
    // Consumed by `store_fact` in Story 1; unused in the Story 0 skeleton.
    #[allow(dead_code)]
    backend: Arc<dyn MemoryBackend>,
}

impl PsychMemoryServer {
    pub fn new(backend: Arc<dyn MemoryBackend>) -> Self {
        Self { backend }
    }
}

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
