pub mod api;
pub mod sse;

use anyhow::Result;
use axum::{
    routing::{delete, get, post},
    Router,
};
use ironmind_r2::{
    client::{R2Client, R2Config},
    store::ConversationStore,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;

pub use api::AppState;

pub async fn serve(config_path: String, bind: String) -> Result<()> {
    let cfg = ironmind_core::config::Config::from_file(&config_path)?;

    let mcp_endpoint = std::env::var("IRONMIND_MCP_URL")
        .unwrap_or_else(|_| "http://localhost:8787/mcp".to_string());

    // R2 is optional — server runs without it (in-memory only)
    let store = match R2Config::from_env() {
        Ok(r2_cfg) => {
            let r2 = R2Client::new(r2_cfg).await?;
            info!("R2 storage connected");
            Some(Arc::new(ConversationStore::new(r2)))
        }
        Err(e) => {
            info!("R2 not configured ({}), running in-memory only", e);
            None
        }
    };

    let state = Arc::new(AppState {
        config: cfg,
        mcp_endpoint,
        store,
    });

    // Static files — look next to binary, then fall back to dev path
    let static_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("static")))
        .unwrap_or_else(|| std::path::PathBuf::from("crates/ironmind-web/static"));

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/api/chat", post(api::chat_handler))
        .route("/api/conversations", get(api::list_conversations))
        .route("/api/conversations/:id", get(api::get_conversation))
        .route("/api/conversations/:id", delete(api::delete_conversation))
        .fallback_service(ServeDir::new(&static_dir))
        .layer(CorsLayer::permissive())
        .with_state(state);

    info!("ironmind → http://{}", bind);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
