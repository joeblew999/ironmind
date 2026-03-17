use anyhow::Result;
use clap::Parser;
use ironmind_core::{agent, config::Config, model::IronMindModel};
use ironmind_mcp::client::{HttpTransport, McpClient};
use tracing_subscriber::fmt;

#[derive(Parser)]
#[command(
    name    = "ironmind",
    version,
    about   = "Offline Qwen3 → MCP agent for factory / industrial use on Apple Silicon"
)]
struct Cli {
    /// Path to ironmind.toml
    #[arg(short, long, default_value = "ironmind.toml")]
    config: String,

    /// MCP server endpoint (HTTP JSON-RPC)
    #[arg(short, long, default_value = "http://localhost:8787/mcp")]
    mcp: String,

    /// Prompt / instruction to send to the agent
    #[arg(short, long)]
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // JSON structured logs — matches plat-trunk observability stack
    fmt().json().init();

    let cli = Cli::parse();
    let cfg = Config::from_file(&cli.config)?;

    tracing::info!("Loading model from {:?}", cfg.model.weights_path);
    let model = IronMindModel::load(&cfg.model).await?;

    let transport = HttpTransport::new(&cli.mcp);
    let mcp = McpClient::new(transport);

    tracing::info!("Fetching tool list from {}", cli.mcp);
    let tools = mcp.list_tools().await?;
    tracing::info!("{} MCP tools available", tools.len());

    let result = agent::run(
        &model,
        &cfg.agent,
        &tools,
        &cli.prompt,
        |name, args| async { mcp.call_tool(&name, args).await },
    )
    .await?;

    println!(
        "\n=== ironmind result ({} rounds) ===\n{}",
        result.rounds, result.final_text
    );
    Ok(())
}
