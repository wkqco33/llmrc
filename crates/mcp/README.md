# llmrc-mcp

Model Context Protocol (MCP) bridge and client transports for [`llmrc`](https://crates.io/crates/llmrc).

## Overview

`llmrc-mcp` connects LLM agents to external MCP tools:

- **Transports**: Built-in `StdioTransport` (child process stdin/stdout) and `StreamableHttpTransport` (HTTP/SSE).
- **Tool Pool & Namespacing**: `McpPool` and `McpBridge` for managing multiple MCP servers with namespace isolation.
- **Agent Integration**: Automatically converts MCP tool definitions into `llmrc_agent::Tool` implementations.
- **Optional Official SDK Adapter**: Feature-gated `rmcp` adapter.

## Usage

Via top-level [`llmrc`](https://crates.io/crates/llmrc):

```toml
[dependencies]
llmrc = { version = "0.1", features = ["mcp"] }
```

Direct dependency:

```toml
[dependencies]
llmrc-mcp = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-mcp).
