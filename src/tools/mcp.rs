use crate::tools::{Tool, ToolResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

pub struct McpClient {
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<BufReader<ChildStdout>>>,
    request_id: AtomicU64,
}

impl McpClient {
    pub async fn connect_stdio(command: &str, args: &[String]) -> Result<Self> {
        let mut child: Child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("启动 MCP 外部进程失败: {} {:?}", command, args))?;

        let stdin = child.stdin.take().context("获取 MCP 进程 stdin 失败")?;
        let stdout = child.stdout.take().context("获取 MCP 进程 stdout 失败")?;

        let client = Self {
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            request_id: AtomicU64::new(1),
        };

        // 1. 发送 initialize 握手请求
        client
            .call_rpc(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "nanobot-rs",
                        "version": "0.1.0"
                    }
                }),
            )
            .await?;

        // 2. 发送 initialized 通知
        client.notify_rpc("notifications/initialized", json!({})).await?;

        Ok(client)
    }

    pub async fn call_rpc(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let mut req_str = serde_json::to_string(&req)?;
        req_str.push('\n');

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(req_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        let mut stdout = self.stdout.lock().await;
        let mut line = String::new();
        stdout.read_line(&mut line).await?;

        let resp: Value = serde_json::from_str(&line)
            .with_context(|| format!("解析 MCP JSON-RPC 响应失败: {}", line))?;

        if let Some(err) = resp.get("error") {
            anyhow::bail!("MCP RPC 错误: {}", err);
        }

        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn notify_rpc(&self, method: &str, params: Value) -> Result<()> {
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        let mut req_str = serde_json::to_string(&req)?;
        req_str.push('\n');

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(req_str.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn list_tools(&self) -> Result<Vec<McpToolDefinition>> {
        let res = self.call_rpc("tools/list", json!({})).await?;
        let mut tools = Vec::new();
        if let Some(arr) = res.get("tools").and_then(|t| t.as_array()) {
            for item in arr {
                if let Ok(def) = serde_json::from_value::<McpToolDefinition>(item.clone()) {
                    tools.push(def);
                }
            }
        }
        Ok(tools)
    }

    pub async fn call_tool(&self, name: &str, arguments: &Value) -> Result<ToolResult> {
        let res = self
            .call_rpc(
                "tools/call",
                json!({
                    "name": name,
                    "arguments": arguments
                }),
            )
            .await?;

        let is_error = res.get("isError").and_then(|e| e.as_bool()).unwrap_or(false);
        let mut output = String::new();

        if let Some(content_arr) = res.get("content").and_then(|c| c.as_array()) {
            for item in content_arr {
                if let Some(txt) = item.get("text").and_then(|t| t.as_str()) {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(txt);
                }
            }
        }

        if output.is_empty() {
            output = serde_json::to_string_pretty(&res).unwrap_or_default();
        }

        Ok(ToolResult { output, is_error })
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

pub struct McpToolWrapper {
    client: Arc<McpClient>,
    name: String,
    description: String,
    schema: Value,
}

impl McpToolWrapper {
    pub fn new(client: Arc<McpClient>, def: McpToolDefinition) -> Self {
        Self {
            client,
            name: def.name,
            description: def.description.unwrap_or_default(),
            schema: def.input_schema,
        }
    }
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters_schema(&self) -> Value {
        self.schema.clone()
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        self.client.call_tool(&self.name, args).await
    }
}
