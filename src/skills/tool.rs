use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use super::SkillsManager;

pub struct LoadSkillTool {
    manager: Arc<SkillsManager>,
}

impl LoadSkillTool {
    pub fn new(manager: Arc<SkillsManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for LoadSkillTool {
    fn name(&self) -> &str { "load_skill" }
    fn description(&self) -> &str { "按名称完整加载特定技能 (Skill) 的详细指引文档与执行规范" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "技能名称，如 weather, cron, memory 等" }
            },
            "required": ["name"]
        })
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        let name = match args["name"].as_str() {
            Some(n) => n,
            None => return Ok(ToolResult::error("缺少 name 参数")),
        };
        match self.manager.get(name) {
            Some(s) => Ok(ToolResult::success(format!("=== Skill: {} ===\n\n{}", s.name, s.content))),
            None => Ok(ToolResult::error(format!("未找到名为 '{}' 的技能", name))),
        }
    }
}
