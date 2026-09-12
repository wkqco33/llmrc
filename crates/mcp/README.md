# llmrc-mcp

Model Context Protocol bridges and client transports for the
[`llmrc`](https://github.com/wkqco33/llmrc) LLM runtime.

`McpBridge` wraps any `McpTransport`, namespaces remote tools as
`server__tool`, and registers them into an `llmrc-agent` `ToolRegistry` so an
agent can call MCP tools like local ones. Built-in transports speak
newline-delimited JSON-RPC over stdio and JSON-RPC over Streamable HTTP.

## Features

| Feature | Default | Description |
| --- | --- | --- |
| `transports` | yes | Built-in stdio and Streamable HTTP transports |
| `rmcp` | no | Adapters for the official [`rmcp`](https://crates.io/crates/rmcp) 3.x client |

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade with the `mcp` feature instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-mcp = "0.1"

# Optional: official rmcp client adapters
llmrc-mcp = { version = "0.1", features = ["rmcp"] }
```

## Usage

```rust,no_run
# use llmrc_mcp::{McpBridge, StdioTransport};
# async fn run() -> Result<(), llmrc_mcp::McpError> {
let transport = StdioTransport::spawn("npx", &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]).await?;
let bridge = McpBridge::new("filesystem", transport);
bridge.refresh().await?;

let mut registry = llmrc_agent::ToolRegistry::new();
bridge.register_into(&mut registry).await?;
# Ok(())
# }
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-mcp)

## License

MIT
