use crate::sse::SseStream;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Sse},
    Json,
};
use ironmind_core::config::Config;
use ironmind_r2::store::ConversationStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub struct AppState {
    pub config: Config,
    pub mcp_endpoint: String,
    /// None when R2 env vars not configured
    pub store: Option<Arc<ConversationStore>>,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub conversation_id: String,
    pub message: String,
    pub user_id: Option<String>,
    pub mcp_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok": true, "service": "ironmind"}))
}

/// POST /api/chat — SSE stream
pub async fn chat_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Sse<SseStream> {
    info!(conversation_id = %req.conversation_id, "Chat");
    let mcp_url = req
        .mcp_url
        .clone()
        .unwrap_or_else(|| state.mcp_endpoint.clone());
    Sse::new(SseStream::new(state, req, mcp_url))
}

/// GET /api/conversations — list for a user (?user_id=xxx)
pub async fn list_conversations(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user_id = params
        .get("user_id")
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    match &state.store {
        None => Json(serde_json::json!([])).into_response(),
        Some(store) => match store.list_conversations(&user_id).await {
            Ok(list) => Json(serde_json::json!(list)).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// GET /api/conversations/:id
pub async fn get_conversation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match &state.store {
        None => (StatusCode::NOT_FOUND, "No storage configured").into_response(),
        Some(store) => match store.get_conversation(&id).await {
            Ok(Some(conv)) => Json(serde_json::json!(conv)).into_response(),
            Ok(None) => (StatusCode::NOT_FOUND, "Not found").into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// DELETE /api/conversations/:id?user_id=xxx
pub async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user_id = params
        .get("user_id")
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    match &state.store {
        None => (StatusCode::NOT_FOUND, "No storage configured").into_response(),
        Some(store) => match store.delete_conversation(&user_id, &id).await {
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}
