use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::fmt;

#[derive(Parser)]
#[command(
    name = "ironmind",
    version,
    about = "Offline Qwen3 → MCP agent + chat GUI for factory/industrial use"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run agent loop with a single prompt (requires --features metal)
    Run {
        #[arg(short, long, default_value = "ironmind.toml")]
        config: String,
        #[arg(short, long, default_value = "http://localhost:8787/mcp")]
        mcp: String,
        #[arg(short, long)]
        prompt: String,
    },
    /// Start the web chat GUI + API server
    Serve {
        #[arg(short, long, default_value = "ironmind.toml")]
        config: String,
        #[arg(short, long, default_value = "0.0.0.0:3000")]
        bind: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    fmt().json().init();
    let cli = Cli::parse();

    match cli.command {
        Command::Run {
            config,
            mcp,
            prompt,
        } => run_agent(config, mcp, prompt).await,
        Command::Serve { config, bind } => ironmind_web::serve(config, bind).await,
    }
}

#[cfg(feature = "inference")]
async fn run_agent(config: String, mcp: String, prompt: String) -> Result<()> {
    use ironmind_core::{agent, config::Config, model::IronMindModel};
    use ironmind_mcp::client::{HttpTransport, McpClient};

    let cfg = Config::from_file(&config)?;
    tracing::info!("Loading model from {:?}", cfg.model.weights_path);
    let model = IronMindModel::load(&cfg.model).await?;

    let mcp_client = McpClient::new(HttpTransport::new(&mcp));
    tracing::info!("Fetching tool list from {}", mcp);
    let tools = mcp_client.list_tools().await?;
    tracing::info!("{} MCP tools available", tools.len());

    let result = agent::run(&model, &cfg.agent, &tools, &prompt, |name, args| async {
        mcp_client.call_tool(&name, args).await
    })
    .await?;

    println!(
        "\n=== ironmind result ({} rounds) ===\n{}",
        result.rounds, result.final_text
    );
    Ok(())
}

#[cfg(not(feature = "inference"))]
async fn run_agent(_config: String, _mcp: String, _prompt: String) -> Result<()> {
    anyhow::bail!(
        "Built without inference support. Rebuild with --features metal on Apple Silicon."
    )
}
