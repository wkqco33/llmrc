//! Model Context Protocol client abstractions.
//!
//! The bridge is deliberately transport-agnostic. This keeps applications
//! independent of the rapidly changing client APIs while still providing
//! usable JSON-RPC stdio and Streamable HTTP transports.

use async_trait::async_trait;
use llmrc_agent::{Tool, ToolError, ToolRegistry};
use llmrc_core::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt, process::Stdio, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Mutex, RwLock},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("MCP transport error: {0}")]
    Transport(String),
    #[error("MCP protocol error: {0}")]
    Protocol(String),
    #[error("MCP request timed out")]
    Timeout,
    #[error("MCP request was cancelled")]
    Cancelled,
    #[error("MCP tool `{0}` was not found")]
    UnknownTool(String),
    #[error("MCP tool returned an error: {0}")]
    Tool(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    #[serde(default, alias = "inputSchema")]
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCallResult {
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn list_tools(
        &self,
        cancellation: CancellationToken,
    ) -> Result<Vec<McpToolInfo>, McpError>;
    async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> Result<McpCallResult, McpError>;
}

pub type SharedMcpTransport = Arc<dyn McpTransport>;
pub type McpClientTransport = dyn McpTransport;

#[async_trait]
impl<T: McpTransport + ?Sized> McpTransport for Arc<T> {
    async fn list_tools(
        &self,
        cancellation: CancellationToken,
    ) -> Result<Vec<McpToolInfo>, McpError> {
        self.as_ref().list_tools(cancellation).await
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> Result<McpCallResult, McpError> {
        self.as_ref().call_tool(name, arguments, cancellation).await
    }
}

/// A namespaced MCP server bridge. Names are safe for provider tool schemas:
/// `server__tool`.
#[derive(Clone)]
pub struct McpBridge {
    server: String,
    transport: SharedMcpTransport,
    tools: Arc<RwLock<Vec<McpToolInfo>>>,
    timeout: Option<Duration>,
}

impl fmt::Debug for McpBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpBridge")
            .field("server", &self.server)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl McpBridge {
    pub fn new(server: impl Into<String>, transport: impl McpTransport + 'static) -> Self {
        Self {
            server: sanitize(&server.into()),
            transport: Arc::new(transport),
            tools: Arc::new(RwLock::new(Vec::new())),
            timeout: Some(Duration::from_secs(30)),
        }
    }

    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub async fn refresh(&self) -> Result<Vec<ToolDefinition>, McpError> {
        let cancellation = CancellationToken::new();
        let list = self
            .request(self.transport.list_tools(cancellation))
            .await?;
        *self.tools.write().await = list;
        Ok(self.definitions().await)
    }

    pub async fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .await
            .iter()
            .map(|tool| ToolDefinition {
                name: namespace(&self.server, &tool.name),
                description: tool.description.clone(),
                parameters: if tool.input_schema.is_null() {
                    json!({})
                } else {
                    tool.input_schema.clone()
                },
            })
            .collect()
    }

    pub async fn tools(&self) -> Vec<McpExecutableTool> {
        self.tools
            .read()
            .await
            .iter()
            .cloned()
            .map(|info| McpExecutableTool {
                server: self.server.clone(),
                transport: self.transport.clone(),
                info,
                timeout: self.timeout,
            })
            .collect()
    }

    pub async fn register_into(&self, registry: &mut ToolRegistry) -> Result<(), McpError> {
        for tool in self.tools().await {
            registry
                .register(tool)
                .map_err(|error| McpError::Protocol(error.to_string()))?;
        }
        Ok(())
    }

    async fn request<T>(
        &self,
        future: impl std::future::Future<Output = Result<T, McpError>>,
    ) -> Result<T, McpError> {
        if let Some(timeout) = self.timeout {
            tokio::time::timeout(timeout, future)
                .await
                .map_err(|_| McpError::Timeout)?
        } else {
            future.await
        }
    }
}

#[derive(Clone)]
pub struct McpExecutableTool {
    server: String,
    transport: SharedMcpTransport,
    info: McpToolInfo,
    timeout: Option<Duration>,
}

impl fmt::Debug for McpExecutableTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpExecutableTool")
            .field("name", &namespace(&self.server, &self.info.name))
            .finish()
    }
}

#[async_trait]
impl Tool for McpExecutableTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: namespace(&self.server, &self.info.name),
            description: self.info.description.clone(),
            parameters: self.info.input_schema.clone(),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        self.execute_with_cancellation(arguments, CancellationToken::new())
            .await
    }

    async fn execute_with_cancellation(
        &self,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> Result<String, ToolError> {
        let future = self
            .transport
            .call_tool(&self.info.name, arguments, cancellation.clone());
        let result = if let Some(timeout) = self.timeout {
            tokio::select! {
                _ = cancellation.cancelled() => Err(McpError::Cancelled),
                result = tokio::time::timeout(timeout, future) => {
                    result.map_err(|_| McpError::Timeout).and_then(|result| result)
                }
            }
        } else {
            tokio::select! {
                _ = cancellation.cancelled() => Err(McpError::Cancelled),
                result = future => result,
            }
        };
        match result {
            Ok(output) if output.is_error => Err(ToolError::FailedWithMessage(output.content)),
            Ok(output) => Ok(output.content),
            Err(McpError::Cancelled) => Err(ToolError::Cancelled),
            Err(McpError::Timeout) => Err(ToolError::Timeout),
            Err(McpError::UnknownTool(_name)) => Err(ToolError::Unknown),
            Err(McpError::Tool(message)) => Err(ToolError::FailedWithMessage(message)),
            Err(_) => Err(ToolError::Failed),
        }
    }
}

/// A pool of independently refreshed MCP servers.
#[derive(Clone, Default)]
pub struct McpPool {
    bridges: Arc<RwLock<BTreeMap<String, McpBridge>>>,
}

/// Backwards-compatible name for applications that model MCP servers as a
/// registry rather than a pool.
pub type McpRegistry = McpPool;
pub type McpServerRegistry = McpPool;

impl fmt::Debug for McpPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpPool").finish()
    }
}

impl McpPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn add(&self, bridge: McpBridge) -> Option<McpBridge> {
        self.bridges
            .write()
            .await
            .insert(bridge.server.clone(), bridge)
    }

    pub async fn remove(&self, server: &str) -> Option<McpBridge> {
        self.bridges.write().await.remove(server)
    }

    pub async fn refresh(&self) -> Result<Vec<ToolDefinition>, McpError> {
        let bridges = self
            .bridges
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut definitions = Vec::new();
        for bridge in bridges {
            bridge.refresh().await?;
            definitions.extend(bridge.definitions().await);
        }
        Ok(definitions)
    }

    pub async fn register_into(&self, registry: &mut ToolRegistry) -> Result<(), McpError> {
        let bridges = self
            .bridges
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for bridge in bridges {
            bridge.register_into(registry).await?;
        }
        Ok(())
    }
}

fn sanitize(value: &str) -> String {
    let value = value
        .bytes()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                c as char
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "server".into()
    } else {
        value
    }
}

pub fn namespace(server: &str, tool: &str) -> String {
    let name = format!("{}__{}", sanitize(server), sanitize(tool));
    name.chars().take(64).collect()
}

#[cfg(feature = "rmcp")]
/// Adapters backed by the official [`rmcp`](https://crates.io/crates/rmcp)
/// 3.x client and transport implementations.
pub mod rmcp_adapter {
    pub use super::{McpCallResult, McpError, McpToolInfo, McpTransport};
    use async_trait::async_trait;
    pub use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
    use rmcp::{
        model::{CallToolRequestParams, ContentBlock},
        service::{RoleClient, RunningService, ServiceError},
        transport::{
            StreamableHttpClientTransport, TokioChildProcess, child_process::ConfigureCommandExt,
        },
    };
    use serde_json::{Map, Value};
    use std::sync::Arc;
    use tokio::process::Command;
    use tokio_util::sync::CancellationToken;

    /// A bridge-compatible client backed by an official rmcp client session.
    #[derive(Clone)]
    pub struct Client {
        session: Arc<RunningService<RoleClient, ()>>,
    }

    impl std::fmt::Debug for Client {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Client").finish_non_exhaustive()
        }
    }

    impl Client {
        async fn from_transport<T, E, A>(transport: T) -> Result<Self, McpError>
        where
            T: rmcp::transport::IntoTransport<RoleClient, E, A>,
            E: std::error::Error + Send + Sync + 'static,
        {
            let session = rmcp::serve_client((), transport)
                .await
                .map_err(|error| McpError::Transport(error.to_string()))?;
            Ok(Self {
                session: Arc::new(session),
            })
        }
    }

    /// Official rmcp stdio client adapter.
    #[derive(Clone, Debug)]
    pub struct StdioTransport(Client);

    impl StdioTransport {
        /// Spawn an MCP server and establish an rmcp client session over stdio.
        pub async fn spawn(
            program: impl AsRef<std::ffi::OsStr>,
            args: &[&str],
        ) -> Result<Self, McpError> {
            let command = Command::new(program).configure(|command| {
                command.args(args);
            });
            let transport = TokioChildProcess::new(command)
                .map_err(|error| McpError::Transport(error.to_string()))?;
            Client::from_transport(transport).await.map(Self)
        }
    }

    /// Official rmcp Streamable HTTP client adapter.
    #[derive(Clone, Debug)]
    pub struct StreamableHttpTransport(Client);

    impl StreamableHttpTransport {
        /// Connect to an MCP server using rmcp's Streamable HTTP transport.
        pub async fn new(endpoint: impl Into<Arc<str>>) -> Result<Self, McpError> {
            Self::connect(endpoint).await
        }

        /// Connect to an MCP server using rmcp's Streamable HTTP transport.
        pub async fn connect(endpoint: impl Into<Arc<str>>) -> Result<Self, McpError> {
            let config = StreamableHttpClientTransportConfig::with_uri(endpoint);
            let transport =
                StreamableHttpClientTransport::with_client(rmcp_reqwest::Client::new(), config);
            Client::from_transport(transport).await.map(Self)
        }

        /// Connect using a caller-configured rmcp Streamable HTTP transport.
        pub async fn connect_with_config(
            config: StreamableHttpClientTransportConfig,
        ) -> Result<Self, McpError> {
            let transport =
                StreamableHttpClientTransport::with_client(rmcp_reqwest::Client::new(), config);
            Client::from_transport(transport).await.map(Self)
        }
    }

    #[async_trait]
    impl McpTransport for Client {
        async fn list_tools(
            &self,
            cancellation: CancellationToken,
        ) -> Result<Vec<McpToolInfo>, McpError> {
            let result = tokio::select! {
                _ = cancellation.cancelled() => return Err(McpError::Cancelled),
                result = self.session.list_tools(None) => {
                    result.map_err(service_error)?
                }
            };
            Ok(result
                .tools
                .into_iter()
                .map(|tool| McpToolInfo {
                    name: tool.name.into_owned(),
                    description: tool.description.map(|description| description.into_owned()),
                    input_schema: Value::Object((*tool.input_schema).clone()),
                })
                .collect())
        }

        async fn call_tool(
            &self,
            name: &str,
            arguments: Value,
            cancellation: CancellationToken,
        ) -> Result<McpCallResult, McpError> {
            let arguments = match arguments {
                Value::Object(arguments) => arguments,
                Value::Null => Map::new(),
                _ => {
                    return Err(McpError::Protocol(
                        "tool arguments must be a JSON object".into(),
                    ));
                }
            };
            let params = CallToolRequestParams::new(name.to_owned()).with_arguments(arguments);
            let result = tokio::select! {
                _ = cancellation.cancelled() => return Err(McpError::Cancelled),
                result = self.session.call_tool(params) => {
                    result.map_err(service_error)?
                }
            };
            Ok(McpCallResult {
                content: result.content.into_iter().map(content_to_string).collect(),
                is_error: result.is_error.unwrap_or(false),
            })
        }
    }

    #[async_trait]
    impl McpTransport for StdioTransport {
        async fn list_tools(
            &self,
            cancellation: CancellationToken,
        ) -> Result<Vec<McpToolInfo>, McpError> {
            self.0.list_tools(cancellation).await
        }

        async fn call_tool(
            &self,
            name: &str,
            arguments: Value,
            cancellation: CancellationToken,
        ) -> Result<McpCallResult, McpError> {
            self.0.call_tool(name, arguments, cancellation).await
        }
    }

    #[async_trait]
    impl McpTransport for StreamableHttpTransport {
        async fn list_tools(
            &self,
            cancellation: CancellationToken,
        ) -> Result<Vec<McpToolInfo>, McpError> {
            self.0.list_tools(cancellation).await
        }

        async fn call_tool(
            &self,
            name: &str,
            arguments: Value,
            cancellation: CancellationToken,
        ) -> Result<McpCallResult, McpError> {
            self.0.call_tool(name, arguments, cancellation).await
        }
    }

    fn service_error(error: ServiceError) -> McpError {
        McpError::Protocol(error.to_string())
    }

    fn content_to_string(content: ContentBlock) -> String {
        match content {
            ContentBlock::Text(text) => text.text,
            content => serde_json::to_string(&content).unwrap_or_else(|_| String::new()),
        }
    }
}

#[derive(Debug)]
struct StdioClient {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    child: Mutex<Child>,
}

/// JSON-RPC line transport for MCP servers launched as child processes.
#[derive(Debug)]
pub struct StdioTransport {
    client: Arc<StdioClient>,
    next_id: Mutex<u64>,
    request_lock: Mutex<()>,
}

impl StdioTransport {
    pub async fn spawn(
        program: impl AsRef<std::ffi::OsStr>,
        args: &[&str],
    ) -> Result<Self, McpError> {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| McpError::Transport(e.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Transport("missing stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Transport("missing stdout".into()))?;
        Ok(Self {
            client: Arc::new(StdioClient {
                stdin: Mutex::new(stdin),
                stdout: Mutex::new(BufReader::new(stdout)),
                child: Mutex::new(child),
            }),
            next_id: Mutex::new(1),
            request_lock: Mutex::new(()),
        })
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        cancellation: CancellationToken,
    ) -> Result<Value, McpError> {
        let _request_guard = self.request_lock.lock().await;
        let mut id = self.next_id.lock().await;
        let request = json!({"jsonrpc":"2.0","id":*id,"method":method,"params":params});
        *id += 1;
        let line = serde_json::to_vec(&request).map_err(|e| McpError::Protocol(e.to_string()))?;
        let mut stdin = self.client.stdin.lock().await;
        stdin
            .write_all(&line)
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;
        drop(stdin);
        let mut stdout = self.client.stdout.lock().await;
        let mut response = String::new();
        tokio::select! {
            _ = cancellation.cancelled() => Err(McpError::Cancelled),
            result = stdout.read_line(&mut response) => {
                result.map_err(|e| McpError::Transport(e.to_string()))?;
                if response.is_empty() { return Err(McpError::Transport("MCP process closed".into())); }
                let value: Value = serde_json::from_str(&response).map_err(|e| McpError::Protocol(e.to_string()))?;
                if let Some(error) = value.get("error") { return Err(McpError::Protocol(error.to_string())); }
                Ok(value.get("result").cloned().unwrap_or(Value::Null))
            }
        }
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn list_tools(
        &self,
        cancellation: CancellationToken,
    ) -> Result<Vec<McpToolInfo>, McpError> {
        let value = self.request("tools/list", json!({}), cancellation).await?;
        serde_json::from_value(
            value
                .get("tools")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
        .map_err(|e| McpError::Protocol(e.to_string()))
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> Result<McpCallResult, McpError> {
        let value = self
            .request(
                "tools/call",
                json!({"name":name,"arguments":arguments}),
                cancellation,
            )
            .await?;
        parse_call_result(value)
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if let Ok(mut child) = self.client.child.try_lock() {
            let _ = child.start_kill();
        }
    }
}

/// Streamable HTTP MCP transport. The endpoint is expected to accept JSON-RPC
/// POST requests and return a JSON-RPC response body.
#[derive(Clone)]
pub struct StreamableHttpTransport {
    client: reqwest::Client,
    endpoint: String,
    headers: Arc<BTreeMap<String, String>>,
    next_id: Arc<Mutex<u64>>,
}

impl fmt::Debug for StreamableHttpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamableHttpTransport")
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

impl StreamableHttpTransport {
    pub fn new(endpoint: impl Into<String>) -> Result<Self, McpError> {
        Ok(Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.into(),
            headers: Arc::new(BTreeMap::new()),
            next_id: Arc::new(Mutex::new(1)),
        })
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let mut headers = (*self.headers).clone();
        headers.insert(name.into(), value.into());
        self.headers = Arc::new(headers);
        self
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        cancellation: CancellationToken,
    ) -> Result<Value, McpError> {
        let mut id = self.next_id.lock().await;
        let body = json!({"jsonrpc":"2.0","id":*id,"method":method,"params":params});
        *id += 1;
        let mut request = self.client.post(&self.endpoint).json(&body);
        for (name, value) in self.headers.iter() {
            request = request.header(name, value);
        }
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(McpError::Cancelled),
            result = request.send() => result.map_err(|e| McpError::Transport(e.to_string()))?,
        };
        let value: Value = response
            .json()
            .await
            .map_err(|e| McpError::Protocol(e.to_string()))?;
        if let Some(error) = value.get("error") {
            return Err(McpError::Protocol(error.to_string()));
        }
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    }
}

#[async_trait]
impl McpTransport for StreamableHttpTransport {
    async fn list_tools(
        &self,
        cancellation: CancellationToken,
    ) -> Result<Vec<McpToolInfo>, McpError> {
        let value = self.request("tools/list", json!({}), cancellation).await?;
        serde_json::from_value(
            value
                .get("tools")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
        .map_err(|e| McpError::Protocol(e.to_string()))
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> Result<McpCallResult, McpError> {
        let value = self
            .request(
                "tools/call",
                json!({"name":name,"arguments":arguments}),
                cancellation,
            )
            .await?;
        parse_call_result(value)
    }
}

fn parse_call_result(value: Value) -> Result<McpCallResult, McpError> {
    let is_error = value
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let content = match value.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(content) => content
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_else(|| content.to_string()),
        None => value.to_string(),
    };
    Ok(McpCallResult { content, is_error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        list_count: AtomicUsize,
        tools: Vec<McpToolInfo>,
    }

    #[async_trait]
    impl McpTransport for Fake {
        async fn list_tools(&self, _: CancellationToken) -> Result<Vec<McpToolInfo>, McpError> {
            self.list_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.tools.clone())
        }
        async fn call_tool(
            &self,
            name: &str,
            _: Value,
            _: CancellationToken,
        ) -> Result<McpCallResult, McpError> {
            if name == "missing" {
                return Err(McpError::UnknownTool(name.into()));
            }
            Ok(McpCallResult {
                content: "ok".into(),
                is_error: false,
            })
        }
    }

    #[tokio::test]
    async fn refresh_namespaces_and_registers_tools() {
        let fake = Arc::new(Fake {
            list_count: AtomicUsize::new(0),
            tools: vec![McpToolInfo {
                name: "search".into(),
                description: Some("find".into()),
                input_schema: json!({"type":"object"}),
            }],
        });
        let bridge = McpBridge::new("docs.server", fake.clone());
        assert_eq!(
            bridge.refresh().await.unwrap()[0].name,
            "docs_server__search"
        );
        let mut registry = ToolRegistry::new();
        bridge.register_into(&mut registry).await.unwrap();
        assert_eq!(registry.definitions()[0].name, "docs_server__search");
        assert_eq!(fake.list_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pool_keeps_servers_isolated() {
        let pool = McpPool::new();
        pool.add(McpBridge::new(
            "one",
            Arc::new(Fake {
                list_count: AtomicUsize::new(0),
                tools: vec![McpToolInfo {
                    name: "run".into(),
                    description: None,
                    input_schema: json!({}),
                }],
            }),
        ))
        .await;
        pool.add(McpBridge::new(
            "two",
            Arc::new(Fake {
                list_count: AtomicUsize::new(0),
                tools: vec![McpToolInfo {
                    name: "run".into(),
                    description: None,
                    input_schema: json!({}),
                }],
            }),
        ))
        .await;
        let names = pool
            .refresh()
            .await
            .unwrap()
            .into_iter()
            .map(|d| d.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["one__run", "two__run"]);
    }
}
