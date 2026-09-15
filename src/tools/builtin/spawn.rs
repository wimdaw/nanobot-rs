use crate::agent::AgentRunner;
use crate::session::Session;
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::info;

pub struct SpawnTool {
    runner: Arc<AgentRunner>,
}

impl SpawnTool {
    pub fn new(runner: Arc<AgentRunner>) -> Self {
        Self { runner }
    }
}

#[async_trait]
impl Tool for SpawnTool {
    fn name(&self) -> &str { "spawn" }
    fn description(&self) -> &str { "派生一个独立的子智能体 (Sub-Agent) 并行执行复杂的长耗时或隔离子任务并返回其产出结论" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "description": { "type": "string", "description": "任务简短说明（3-5字）" },
                "prompt": { "type": "string", "description": "子智能体需完成的独立任务完整提示词指令" }
            },
            "required": ["description", "prompt"]
        })
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        let desc = match args["description"].as_str() {
            Some(d) => d,
            None => return Ok(ToolResult::error("缺少 description 参数")),
        };
        let prompt = match args["prompt"].as_str() {
            Some(p) => p,
            None => return Ok(ToolResult::error("缺少 prompt 参数")),
        };

        let sub_session_key = format!("subagent_{}", chrono::Utc::now().timestamp_millis());
        info!("派生子智能体 [{}] 启动: {}", sub_session_key, desc);

        let mut sub_session = Session::new(&sub_session_key);
        let runner = self.runner.clone();
        let prompt_clone = prompt.to_string();

        let res = tokio::spawn(async move {
            runner.run_turn(&mut sub_session, &prompt_clone).await
        })
        .await?;

        match res {
            Ok(conclusion) => Ok(ToolResult::success(format!(
                "=== 子智能体 [{}] 执行完成 ===\n结论: {}",
                desc, conclusion
            ))),
            Err(e) => Ok(ToolResult::error(format!("子智能体执行异常: {}", e))),
        }
    }
}
