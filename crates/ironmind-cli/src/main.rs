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
    /// User management (backed by R2)
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
}

#[derive(Subcommand)]
enum UserCmd {
    /// Create a new user and print their API token
    Create {
        #[arg(help = "User ID (e.g. gerard, keita, max)")]
        id: String,
        #[arg(help = "Display name")]
        name: String,
    },
    /// List all users
    List,
    /// Rotate a user's token
    Rotate {
        #[arg(help = "User ID")]
        id: String,
    },
    /// Delete a user
    Delete {
        #[arg(help = "User ID")]
        id: String,
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
        Command::User { cmd } => user_cmd(cmd).await,
    }
}

// ── Agent run ────────────────────────────────────────────────────────────────

#[cfg(feature = "inference")]
async fn run_agent(config: String, mcp: String, prompt: String) -> Result<()> {
    use ironmind_core::{agent, config::Config, model::IronMindModel};
    use ironmind_mcp::client::{HttpTransport, McpClient};

    let cfg = Config::from_file(&config)?;
    let model = IronMindModel::load(&cfg.model).await?;
    let mcp_c = McpClient::new(HttpTransport::new(&mcp));
    let tools = mcp_c.list_tools().await?;

    let result = agent::run(&model, &cfg.agent, &tools, &prompt, |name, args| async {
        mcp_c.call_tool(&name, args).await
    })
    .await?;

    println!(
        "\n=== ironmind ({} rounds) ===\n{}",
        result.rounds, result.final_text
    );
    Ok(())
}

#[cfg(not(feature = "inference"))]
async fn run_agent(_config: String, _mcp: String, _prompt: String) -> Result<()> {
    anyhow::bail!("Rebuild with --features metal on Apple Silicon.")
}

// ── User management ──────────────────────────────────────────────────────────

async fn user_cmd(cmd: UserCmd) -> Result<()> {
    use ironmind_auth::generate_token;
    use ironmind_r2::{
        client::{R2Client, R2Config},
        model::UserProfile,
        store::ConversationStore,
    };

    let cfg = R2Config::from_env()?;
    let r2 = R2Client::new(cfg).await?;
    let store = ConversationStore::new(r2);

    match cmd {
        UserCmd::Create { id, name } => {
            if store.get_user(&id).await?.is_some() {
                anyhow::bail!(
                    "User '{}' already exists. Use `rotate` to get a new token.",
                    id
                );
            }
            let (token, token_hash) = generate_token(&id);
            let user = UserProfile {
                id: id.clone(),
                name: name.clone(),
                token_hash,
                created_at: chrono::Utc::now(),
            };
            store.save_user(&user).await?;
            println!("\n✓ User created");
            println!("  ID:    {}", id);
            println!("  Name:  {}", name);
            println!("\n  Token (save this — shown only once):");
            println!("  {}\n", token);
            println!("  Set in client: Authorization: Bearer {}", token);
        }

        UserCmd::List => {
            // We don't maintain a global user index — list by scanning R2 prefix
            // For small factories (1-5 users) this is fine
            println!("User listing requires R2 list — not yet implemented.");
            println!("Tip: users are stored at users/{{user_id}}/profile.json in your R2 bucket.");
        }

        UserCmd::Rotate { id } => {
            let mut user = store
                .get_user(&id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("User '{}' not found", id))?;
            let (token, token_hash) = generate_token(&id);
            user.token_hash = token_hash;
            store.save_user(&user).await?;
            println!("\n✓ Token rotated for '{}'", id);
            println!("  New token (save this — shown only once):");
            println!("  {}\n", token);
        }

        UserCmd::Delete { id } => {
            // Note: doesn't delete their conversations, just the profile
            println!("Delete user '{}' from R2? [y/N]", id);
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if input.trim().eq_ignore_ascii_case("y") {
                // Direct R2 delete — store doesn't expose this yet
                println!("Delete users/{}/profile.json from R2 manually for now.", id);
            }
        }
    }

    Ok(())
}
