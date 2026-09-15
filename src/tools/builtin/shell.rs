use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "在工作区执行安全的 Shell 命令行命令并返回输出"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "要执行的 shell 脚本命令"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: &Value, workspace: &str) -> Result<ToolResult> {
        let cmd = match args["command"].as_str() {
            Some(c) => c,
            None => return Ok(ToolResult::error("缺少 command 参数")),
        };

        // 安全沙箱规则校验
        let policy = crate::security::SecurityPolicy::default();
        if let Err(e) = policy.validate_command(cmd) {
            return Ok(ToolResult::error(e.to_string()));
        }

        let shell = if cfg!(target_os = "windows") { "cmd" } else { "sh" };
        let flag = if cfg!(target_os = "windows") { "/C" } else { "-c" };

        let output = Command::new(shell)
            .arg(flag)
            .arg(cmd)
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut combined = String::new();
        if !stdout.is_empty() {
            combined.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&stderr);
        }

        if output.status.success() {
            Ok(ToolResult::success(if combined.is_empty() { "（命令执行成功，无输出）".to_string() } else { combined }))
        } else {
            Ok(ToolResult::error(format!("退出码 {}: {}", output.status.code().unwrap_or(-1), combined)))
        }
    }
}
