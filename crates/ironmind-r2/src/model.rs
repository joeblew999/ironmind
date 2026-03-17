use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    /// If this message triggered tool calls, record them here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallRecord>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// A record of a single MCP tool invocation stored with the message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub args: serde_json::Value,
    pub result: String,
    /// BLAKE3 hash of result if large (stored separately in R2 blobs/)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_key: Option<String>,
}

/// A conversation (chat session).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub user_id: String,
    /// Auto-generated from first message
    pub title: String,
    pub messages: Vec<Message>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// MCP endpoint used for this conversation
    pub mcp_url: String,
}

impl Conversation {
    pub fn new(id: String, user_id: String, mcp_url: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            user_id,
            title: "New conversation".to_string(),
            messages: vec![],
            created_at: now,
            updated_at: now,
            mcp_url,
        }
    }

    /// Derive title from first user message (first 60 chars).
    pub fn derive_title(&mut self) {
        if let Some(msg) = self.messages.iter().find(|m| m.role == MessageRole::User) {
            let t = msg.content.chars().take(60).collect::<String>();
            self.title = if msg.content.len() > 60 {
                format!("{}…", t)
            } else {
                t
            };
        }
    }
}

/// Lightweight index entry — stored in users/{user_id}/conversations.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub updated_at: DateTime<Utc>,
}

/// User profile stored in users/{user_id}/profile.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub name: String,
    /// bcrypt hash of API token
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
}
