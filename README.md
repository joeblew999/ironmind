# ironmind

Offline LLM inference engine that bridges **Qwen3** (via `mistral.rs`) to any **MCP tool server** — built for factory and industrial environments on Apple Silicon.

## Architecture

```
operator prompt
      │
  ironmind (Rust binary, Mac Mini M4 Max)
      │  mistral.rs + Metal backend
      │  Qwen3-30B-A3B — fully offline
      │
  MCP server (JSON-RPC over HTTP)
      │  e.g. plat-trunk CAD MCP
      │
  tool result → back to Qwen3 → next turn
```

## Workspace

| Crate | Role |
|---|---|
| `ironmind-core` | Model loading, agent loop |
| `ironmind-mcp` | MCP client (HTTP JSON-RPC + transport trait) |
| `ironmind-cli` | Binary entry point |

## Quick Start

```bash
# 1. Download model weights (internet machine, before factory)
huggingface-cli download Qwen/Qwen3-30B-A3B --local-dir ./models/qwen3-30b-a3b

# 2. Auto-tune quantization for your hardware
mistralrs tune -m ./models/qwen3-30b-a3b --emit-config ironmind.toml

# 3. Build (Metal backend for Apple Silicon)
cargo build --release --features metal

# 4. Run
./target/release/ironmind \
  --config ironmind.toml \
  --mcp http://localhost:8787/mcp \
  --prompt "Create an aluminium bracket 100x50x10mm with 8mm holes at each corner"
```

## Offline Deployment

Set these env vars on the factory Mac Mini:

```bash
HF_HUB_OFFLINE=1
HF_HUB_CACHE=/path/to/models
```

ironmind sets these automatically from `weights_path` in config, but setting them
at the system level is belt-and-braces for a factory environment.

## Extending

`McpTransport` is a trait — swap in a stdio transport for local MCP servers,
or a Unix socket transport for zero-network-stack deployments.
