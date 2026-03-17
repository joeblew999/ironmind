use crate::transport::McpTransport;
use anyhow::Result;
use async_trait::async_trait;
use ironmind_core::agent::McpTool;
use serde_json::{json, Value};
use tracing::debug;

/// High-level MCP client. Wraps any transport.
pub struct McpClient {
    transport: Box<dyn McpTransport>,
}

impl McpClient {
    pub fn new(transport: impl McpTransport + 'static) -> Self {
        Self {
            transport: Box::new(transport),
        }
    }

    /// Fetch the full tool list from the MCP server (tools/list).
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let result = self.transport.call("tools/list", json!({})).await?;
        let tools = result["tools"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("tools/list: expected 'tools' array in response"))?
            .iter()
            .map(|t| McpTool {
                name: t["name"].as_str().unwrap_or("").to_string(),
                description: t["description"].as_str().unwrap_or("").to_string(),
                parameters: t["inputSchema"].clone(),
            })
            .collect();
        Ok(tools)
    }

    /// Dispatch a single tool call (tools/call) and return the text result.
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<String> {
        debug!(tool = name, "Calling MCP tool");

        let result = self
            .transport
            .call("tools/call", json!({ "name": name, "arguments": args }))
            .await?;

        // MCP spec: result.content is an array of content blocks
        let text = result["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|block| block["text"].as_str())
            .unwrap_or("ok")
            .to_string();

        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// HTTP JSON-RPC transport — for plat-trunk MCP on CF Worker or local Hono dev
// ---------------------------------------------------------------------------

pub struct HttpTransport {
    endpoint: String,
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({
            "jsonrpc": "2.0",
            "id":      1,
            "method":  method,
            "params":  params,
        });

        let resp = self
            .client
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;

        if let Some(err) = resp.get("error") {
            anyhow::bail!("MCP server error: {}", err);
        }

        Ok(resp["result"].clone())
    }
}
