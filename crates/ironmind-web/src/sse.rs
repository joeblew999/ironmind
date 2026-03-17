use crate::api::{AppState, ChatRequest};
use axum::response::sse::Event;
use futures::Stream;
use ironmind_r2::model::{Conversation, Message, MessageRole};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::error;

#[cfg(feature = "inference")]
use ironmind_mcp::client::{HttpTransport, McpClient};
#[cfg(feature = "inference")]
use ironmind_r2::model::ToolCallRecord;
#[cfg(feature = "inference")]
use tracing::info;

type SseTx = mpsc::Sender<Result<Event, std::convert::Infallible>>;

#[derive(Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Token {
        text: String,
    },
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        name: String,
        result: String,
    },
    Done {
        rounds: usize,
    },
    Error {
        message: String,
    },
}

impl ChatEvent {
    fn to_event(&self) -> Event {
        let data = serde_json::to_string(self).unwrap_or_default();
        let name = match self {
            ChatEvent::Token { .. } => "token",
            ChatEvent::ToolCall { .. } => "tool_call",
            ChatEvent::ToolResult { .. } => "tool_result",
            ChatEvent::Done { .. } => "done",
            ChatEvent::Error { .. } => "error",
        };
        Event::default().event(name).data(data)
    }
}

async fn emit(tx: &SseTx, evt: ChatEvent) {
    let _ = tx.send(Ok(evt.to_event())).await;
}

// ── SSE Stream ───────────────────────────────────────────────────────────────

pub struct SseStream {
    inner: ReceiverStream<Result<Event, std::convert::Infallible>>,
}

impl SseStream {
    pub fn new(state: Arc<AppState>, req: ChatRequest, mcp_url: String) -> Self {
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            if let Err(e) = agent_task(&state, &req, &mcp_url, &tx).await {
                error!("Agent task: {}", e);
                emit(
                    &tx,
                    ChatEvent::Error {
                        message: e.to_string(),
                    },
                )
                .await;
            }
        });
        Self {
            inner: ReceiverStream::new(rx),
        }
    }
}

impl Stream for SseStream {
    type Item = Result<Event, std::convert::Infallible>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

// ── Agent task — inference build ─────────────────────────────────────────────

#[cfg(feature = "inference")]
async fn agent_task(
    state: &AppState,
    req: &ChatRequest,
    mcp_url: &str,
    tx: &SseTx,
) -> anyhow::Result<()> {
    use ironmind_core::{agent, model::IronMindModel};

    let mut conv = match &state.store {
        Some(store) => store
            .get_conversation(&req.conversation_id)
            .await?
            .unwrap_or_else(|| {
                Conversation::new(
                    req.conversation_id.clone(),
                    req.user_id.clone().unwrap_or_else(|| "default".to_string()),
                    mcp_url.to_string(),
                )
            }),
        None => Conversation::new(
            req.conversation_id.clone(),
            req.user_id.clone().unwrap_or_else(|| "default".to_string()),
            mcp_url.to_string(),
        ),
    };

    conv.messages.push(Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        content: req.message.clone(),
        tool_calls: vec![],
        created_at: chrono::Utc::now(),
    });

    let model = IronMindModel::load(&state.config.model).await?;
    let mcp = McpClient::new(HttpTransport::new(mcp_url));
    let tools = mcp.list_tools().await?;
    info!(tools = tools.len(), conv_id = %req.conversation_id, "Agent starting");

    let tx2 = tx.clone();
    let mut tool_log: Vec<ToolCallRecord> = vec![];

    let result = agent::run(
        &model,
        &state.config.agent,
        &tools,
        &req.message,
        |name, args| {
            let mcp = &mcp;
            let tx = tx2.clone();
            let n = name.clone();
            async move {
                emit(
                    &tx,
                    ChatEvent::ToolCall {
                        name: n.clone(),
                        args: args.clone(),
                    },
                )
                .await;
                let result = mcp.call_tool(&n, args.clone()).await?;
                emit(
                    &tx,
                    ChatEvent::ToolResult {
                        name: n.clone(),
                        result: result.clone(),
                    },
                )
                .await;
                Ok(result)
            }
        },
    )
    .await?;

    for word in result.final_text.split_inclusive(' ') {
        emit(
            tx,
            ChatEvent::Token {
                text: word.to_string(),
            },
        )
        .await;
    }
    emit(
        tx,
        ChatEvent::Done {
            rounds: result.rounds,
        },
    )
    .await;

    conv.messages.push(Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::Assistant,
        content: result.final_text,
        tool_calls: tool_log,
        created_at: chrono::Utc::now(),
    });

    if let Some(store) = &state.store {
        if let Err(e) = store.save_conversation(&mut conv).await {
            error!("Failed to save conversation: {}", e);
        }
    }

    Ok(())
}

// ── Agent task — stub build (CI / no inference) ───────────────────────────────

#[cfg(not(feature = "inference"))]
async fn agent_task(
    state: &AppState,
    req: &ChatRequest,
    _mcp_url: &str,
    tx: &SseTx,
) -> anyhow::Result<()> {
    let reply = format!(
        "ironmind stub — echo: {}\n\nRebuild with --features metal for real Qwen3 inference.",
        req.message
    );

    for word in reply.split_inclusive(' ') {
        emit(
            tx,
            ChatEvent::Token {
                text: word.to_string(),
            },
        )
        .await;
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }
    emit(tx, ChatEvent::Done { rounds: 0 }).await;

    let mut conv = match &state.store {
        Some(store) => store
            .get_conversation(&req.conversation_id)
            .await
            .unwrap_or_default()
            .unwrap_or_else(|| {
                Conversation::new(
                    req.conversation_id.clone(),
                    req.user_id.clone().unwrap_or_else(|| "default".to_string()),
                    "stub".to_string(),
                )
            }),
        None => Conversation::new(
            req.conversation_id.clone(),
            req.user_id.clone().unwrap_or_else(|| "default".to_string()),
            "stub".to_string(),
        ),
    };

    conv.messages.push(Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        content: req.message.clone(),
        tool_calls: vec![],
        created_at: chrono::Utc::now(),
    });
    conv.messages.push(Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::Assistant,
        content: reply,
        tool_calls: vec![],
        created_at: chrono::Utc::now(),
    });

    if let Some(store) = &state.store {
        let _ = store.save_conversation(&mut conv).await;
    }
    Ok(())
}
