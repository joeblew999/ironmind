use crate::{
    client::R2Client,
    model::{Conversation, ConversationMeta, Message, UserProfile},
};
use anyhow::Result;
use chrono::Utc;
use tracing::debug;

/// BLAKE3 content-addressed blob threshold — results larger than this
/// are stored as deduplicated blobs rather than inline in the message JSON.
const BLOB_THRESHOLD_BYTES: usize = 4 * 1024; // 4KB

/// High-level conversation store backed by R2.
///
/// Key layout:
///   users/{user_id}/profile.json
///   users/{user_id}/conversations.json      ← index
///   conversations/{conv_id}.json            ← full conversation
///   blobs/{blake3_hex}                      ← large tool results
pub struct ConversationStore {
    r2: R2Client,
}

impl ConversationStore {
    pub fn new(r2: R2Client) -> Self {
        Self { r2 }
    }

    // ── Conversations ────────────────────────────────────────────────────────

    /// Load a conversation by ID. Returns None if not found.
    pub async fn get_conversation(&self, conv_id: &str) -> Result<Option<Conversation>> {
        let key = format!("conversations/{}.json", conv_id);
        match self.r2.get_str(&key).await? {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    /// Save (create or update) a conversation.
    pub async fn save_conversation(&self, conv: &mut Conversation) -> Result<()> {
        conv.updated_at = Utc::now();
        conv.derive_title();

        // Inline large tool results → blobs
        self.externalize_blobs(conv).await?;

        let key = format!("conversations/{}.json", conv.id);
        self.r2.put_json(&key, conv).await?;

        // Update user index
        self.update_user_index(conv).await?;

        debug!(conv_id = %conv.id, "Conversation saved");
        Ok(())
    }

    /// Delete a conversation and remove from user index.
    pub async fn delete_conversation(&self, user_id: &str, conv_id: &str) -> Result<()> {
        self.r2
            .delete(&format!("conversations/{}.json", conv_id))
            .await?;

        let mut index = self.list_conversations(user_id).await?;
        index.retain(|m| m.id != conv_id);
        self.r2
            .put_json(&format!("users/{}/conversations.json", user_id), &index)
            .await?;
        Ok(())
    }

    /// List conversation metadata for a user, sorted newest first.
    pub async fn list_conversations(&self, user_id: &str) -> Result<Vec<ConversationMeta>> {
        let key = format!("users/{}/conversations.json", user_id);
        match self.r2.get_str(&key).await? {
            Some(s) => {
                let mut index: Vec<ConversationMeta> = serde_json::from_str(&s)?;
                index.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                Ok(index)
            }
            None => Ok(vec![]),
        }
    }

    // ── Messages ─────────────────────────────────────────────────────────────

    /// Append a message to a conversation and persist.
    pub async fn append_message(&self, conv: &mut Conversation, msg: Message) -> Result<()> {
        conv.messages.push(msg);
        self.save_conversation(conv).await
    }

    // ── Users ────────────────────────────────────────────────────────────────

    pub async fn get_user(&self, user_id: &str) -> Result<Option<UserProfile>> {
        let key = format!("users/{}/profile.json", user_id);
        match self.r2.get_str(&key).await? {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub async fn save_user(&self, user: &UserProfile) -> Result<()> {
        let key = format!("users/{}/profile.json", user.id);
        self.r2.put_json(&key, user).await
    }

    // ── BLAKE3 blob deduplication ─────────────────────────────────────────────

    /// For any tool call result larger than BLOB_THRESHOLD_BYTES,
    /// store the content as a BLAKE3-addressed blob and replace the
    /// inline result with the hash key — matching ADR-0008 pattern.
    async fn externalize_blobs(&self, conv: &mut Conversation) -> Result<()> {
        for msg in &mut conv.messages {
            for tc in &mut msg.tool_calls {
                if tc.blob_key.is_none() && tc.result.len() > BLOB_THRESHOLD_BYTES {
                    let hash = blake3::hash(tc.result.as_bytes());
                    let hex = hash.to_hex().to_string();
                    let blob_key = format!("blobs/{}", hex);

                    // Only write if not already stored (content-addressed = idempotent)
                    if self.r2.get(&blob_key).await?.is_none() {
                        self.r2
                            .put(&blob_key, tc.result.as_bytes().to_vec(), "text/plain")
                            .await?;
                        debug!(blob_key, "Blob written");
                    }

                    tc.result = format!("[blob:{}]", hex);
                    tc.blob_key = Some(blob_key);
                }
            }
        }
        Ok(())
    }

    /// Resolve a blob reference back to its content.
    pub async fn resolve_blob(&self, blob_key: &str) -> Result<Option<String>> {
        self.r2.get_str(blob_key).await
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    async fn update_user_index(&self, conv: &Conversation) -> Result<()> {
        let key = format!("users/{}/conversations.json", conv.user_id);
        let mut index = self.list_conversations(&conv.user_id).await?;

        // Upsert
        let meta = ConversationMeta {
            id: conv.id.clone(),
            title: conv.title.clone(),
            updated_at: conv.updated_at,
        };
        if let Some(existing) = index.iter_mut().find(|m| m.id == conv.id) {
            *existing = meta;
        } else {
            index.push(meta);
        }

        self.r2.put_json(&key, &index).await
    }
}
