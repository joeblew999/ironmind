use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level ironmind configuration — loaded from `ironmind.toml` at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: ModelConfig,
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Path to local model weights (offline — no HF hub at runtime)
    pub weights_path: PathBuf,

    /// ISQ quantisation level e.g. "Q4K", "Q6K", "Q8_0"
    pub isq: String,

    /// Enable Qwen3 thinking mode (/think injected into system prompt)
    #[serde(default = "default_true")]
    pub thinking: bool,

    /// Max tokens to generate per turn
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Max tool-call rounds before giving up
    #[serde(default = "default_max_rounds")]
    pub max_rounds: usize,
}

fn default_true() -> bool { true }
fn default_max_tokens() -> usize { 2048 }
fn default_max_rounds() -> usize { 20 }

impl Config {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
