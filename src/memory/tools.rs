use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use super::MemoryStore;

pub struct MemorySaveTool {
    store: Arc<MemoryStore>,
}

impl MemorySaveTool {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemorySaveTool {
    fn name(&self) -> &str { "memory_save" }
    fn description(&self) -> &str { "将用户的习惯、关键配置、偏好或核心事实永久写入长期记忆库" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "fact": { "type": "string", "description": "要持久化记住的事实或用户偏好描述" }
            },
            "required": ["fact"]
        })
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        let fact = match args["fact"].as_str() {
            Some(f) => f,
            None => return Ok(ToolResult::error("缺少 fact 参数")),
        };
        self.store.append(fact)?;
        Ok(ToolResult::success(format!("已成功将此事实写入长期记忆: {}", fact)))
    }
}

pub struct MemorySearchTool {
    store: Arc<MemoryStore>,
}

impl MemorySearchTool {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str { "memory_search" }
    fn description(&self) -> &str { "在长期记忆库中检索相关的用户事实、过往偏好或历史备忘" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "检索关键词" }
            },
            "required": ["query"]
        })
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        let q = match args["query"].as_str() {
            Some(q) => q,
            None => return Ok(ToolResult::error("缺少 query 参数")),
        };
        let matches = self.store.search(q);
        if matches.is_empty() {
            Ok(ToolResult::success("长期记忆中未找到相关事实"))
        } else {
            Ok(ToolResult::success(matches.join("\n")))
        }
    }
}
