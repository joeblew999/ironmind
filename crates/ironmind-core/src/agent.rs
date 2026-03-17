use crate::{config::AgentConfig, model::IronMindModel};
use anyhow::Result;
use serde_json::Value;
use tracing::{debug, info, warn};

/// A single MCP tool descriptor.
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Result of one completed agent run.
pub struct AgentResult {
    pub final_text: String,
    pub rounds: usize,
}

#[cfg(feature = "inference")]
use mistralrs::{Function, TextMessageRole, TextMessages, Tool, ToolChoice, ToolType};

#[cfg(feature = "inference")]
impl From<&McpTool> for Tool {
    fn from(t: &McpTool) -> Self {
        Tool {
            tp: ToolType::Function,
            function: Function {
                name: t.name.clone(),
                description: Some(t.description.clone()),
                parameters: Some(t.parameters.clone()),
            },
        }
    }
}

/// Core agent loop — only available with inference feature.
#[cfg(feature = "inference")]
pub async fn run<F, Fut>(
    model: &IronMindModel,
    cfg: &AgentConfig,
    tools: &[McpTool],
    user_input: &str,
    mut dispatch: F,
) -> Result<AgentResult>
where
    F: FnMut(String, Value) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    let mistral_tools: Vec<Tool> = tools.iter().map(Tool::from).collect();

    let system = "You are an AI agent controlling a B-Rep CAD system via tools. \
                  Think step by step before every tool call. /think";

    let mut messages = TextMessages::new()
        .add_message(TextMessageRole::System, system)
        .add_message(TextMessageRole::User, user_input);

    let mut rounds = 0;

    loop {
        if rounds >= cfg.max_rounds {
            warn!(rounds, "Max tool-call rounds reached");
            return Ok(AgentResult {
                final_text: "Max rounds reached without a final answer.".into(),
                rounds,
            });
        }

        info!(round = rounds + 1, "Agent turn");

        let response = model
            .inner
            .send_chat_request(
                messages
                    .clone()
                    .with_tools(mistral_tools.clone())
                    .with_tool_choice(ToolChoice::Auto),
            )
            .await?;

        let choice = &response.choices[0];
        rounds += 1;

        match &choice.message.tool_calls {
            Some(calls) if !calls.is_empty() => {
                for call in calls {
                    let args: Value =
                        serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
                    debug!(tool = %call.function.name, ?args, "Dispatching tool call");
                    let result = dispatch(call.function.name.clone(), args).await?;
                    debug!(tool = %call.function.name, %result, "Tool result");
                    messages = messages.add_tool_result(call.id.clone(), result);
                }
            }
            _ => {
                let text = choice
                    .message
                    .content
                    .clone()
                    .unwrap_or_else(|| "Done.".into());
                info!(rounds, "Agent finished");
                return Ok(AgentResult {
                    final_text: text,
                    rounds,
                });
            }
        }
    }
}

/// Stub for non-inference builds (CI).
#[cfg(not(feature = "inference"))]
pub async fn run<F, Fut>(
    _model: &IronMindModel,
    _cfg: &AgentConfig,
    _tools: &[McpTool],
    _user_input: &str,
    _dispatch: F,
) -> Result<AgentResult>
where
    F: FnMut(String, Value) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    anyhow::bail!("inference feature not enabled")
}
