//! Runtime configuration, read from the environment.

/// Wrapper configuration.
///
/// In the two-container deployment the wrapper reaches the memory-service over
/// the internal compose network at `http://memory:8000`. Locally (tests, dev)
/// it defaults to the published port on loopback.
#[derive(Debug, Clone)]
pub struct Config {
    /// Base URL of the mcp-memory-service REST API, without a trailing slash.
    pub memory_base_url: String,
    /// Address the wrapper's streamable-HTTP MCP endpoint binds to.
    pub http_bind: String,
}

impl Config {
    pub fn from_env() -> Self {
        let memory_base_url = std::env::var("MEMORY_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
        let http_bind = std::env::var("HTTP_BIND").unwrap_or_else(|_| "0.0.0.0:8000".to_string());
        Self {
            memory_base_url: memory_base_url.trim_end_matches('/').to_string(),
            http_bind,
        }
    }
}
