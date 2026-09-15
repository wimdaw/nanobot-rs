use crate::session::manager::SessionManager;
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct SessionListTool {
    manager: Arc<SessionManager>,
}

impl SessionListTool {
    pub fn new(manager: Arc<SessionManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for SessionListTool {
    fn name(&self) -> &str { "session_list" }
    fn description(&self) -> &str { "查看或检索系统中已保存的历史会话列表及消息统计" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _args: &Value, _workspace: &str) -> Result<ToolResult> {
        let sessions = self.manager.list_sessions()?;
        if sessions.is_empty() {
            return Ok(ToolResult::success("当前暂无已持久化的历史会话"));
        }
        let mut out = String::from("【已持久化会话清单】:\n");
        for s in sessions {
            out.push_str(&format!(
                "- [{}] 消息轮数: {} 轮 | 最后活跃: {} | 文件大小: {} 字节\n",
                s.session_key,
                s.message_count,
                s.updated_at.format("%Y-%m-%d %H:%M:%S"),
                s.file_size_bytes
            ));
        }
        Ok(ToolResult::success(out))
    }
}

pub struct SessionHistoryTool {
    manager: Arc<SessionManager>,
}

impl SessionHistoryTool {
    pub fn new(manager: Arc<SessionManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for SessionHistoryTool {
    fn name(&self) -> &str { "session_history" }
    fn description(&self) -> &str { "读取指定 session_key 会话的历史消息记录" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_key": { "type": "string", "description": "会话标识，如 cli:default 或 feishu:xxx" }
            },
            "required": ["session_key"]
        })
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        let key = match args["session_key"].as_str() {
            Some(k) => k,
            None => return Ok(ToolResult::error("缺少 session_key 参数")),
        };
        let session = self.manager.load_session(key)?;
        if session.messages.is_empty() {
            return Ok(ToolResult::success(format!("会话 [{}] 无历史消息", key)));
        }
        let out = serde_json::to_string_pretty(&session.messages)?;
        Ok(ToolResult::success(out))
    }
}
