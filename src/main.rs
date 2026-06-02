//! psych-memory-mcp — a minimal MCP wrapper that fronts an internal
//! mcp-memory-service, exposing only explicit epistemic tools.
//!
//! Story 0 wires the transport end-to-end (config -> backend adapter -> MCP
//! server over stdio) without yet exposing any tool.

use std::sync::Arc;

use rmcp::transport::stdio;
use rmcp::ServiceExt;

use psych_memory_mcp::backend::{MemoryBackend, ReqwestMemoryBackend};
use psych_memory_mcp::config::Config;
use psych_memory_mcp::server::PsychMemoryServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Logs MUST go to stderr: stdout is the MCP protocol channel.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let config = Config::from_env();
    tracing::info!(memory_base_url = %config.memory_base_url, "starting psych-memory-mcp");

    let backend = Arc::new(ReqwestMemoryBackend::new(config.memory_base_url.clone()));

    // Best-effort readiness probe. The service may still be starting (compose
    // dependency ordering), so we log rather than abort.
    match backend.health().await {
        Ok(()) => tracing::info!("memory-service reachable"),
        Err(e) => tracing::warn!(error = %e, "memory-service not reachable at startup"),
    }

    let server = PsychMemoryServer::new(backend);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
