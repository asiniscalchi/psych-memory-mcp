//! psych-memory-mcp — a minimal MCP wrapper that fronts an internal
//! mcp-memory-service, exposing only explicit epistemic tools.
//!
//! Story 0 wires the transport end-to-end: config -> REST backend adapter ->
//! an MCP server exposed over streamable-HTTP at `/mcp`, the same shape the
//! 1MCP gateway connects to for every other service. No tool is exposed yet.

use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};

use psych_memory_mcp::backend::{MemoryBackend, ReqwestMemoryBackend};
use psych_memory_mcp::config::Config;
use psych_memory_mcp::server::PsychMemoryServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let config = Config::from_env();
    tracing::info!(
        memory_base_url = %config.memory_base_url,
        http_bind = %config.http_bind,
        "starting psych-memory-mcp"
    );

    let backend: Arc<dyn MemoryBackend> =
        Arc::new(ReqwestMemoryBackend::new(config.memory_base_url.clone()));

    // Best-effort readiness probe. The service may still be starting (compose
    // dependency ordering), so we log rather than abort.
    match backend.health().await {
        Ok(()) => tracing::info!("memory-service reachable"),
        Err(e) => tracing::warn!(error = %e, "memory-service not reachable at startup"),
    }

    // A fresh handler is built per session; each shares the same backend.
    let mcp_service = StreamableHttpService::new(
        move || Ok(PsychMemoryServer::new(backend.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let app = axum::Router::new().nest_service("/mcp", mcp_service);
    let listener = tokio::net::TcpListener::bind(&config.http_bind).await?;
    tracing::info!(addr = %config.http_bind, "serving MCP over streamable-HTTP at /mcp");
    axum::serve(listener, app).await?;
    Ok(())
}
