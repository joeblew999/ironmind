use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// Abstraction over MCP transports.
/// Implement this to support HTTP JSON-RPC, stdio, Unix socket, etc.
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn call(&self, method: &str, params: Value) -> Result<Value>;
}
