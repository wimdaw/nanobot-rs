use crate::cron::CronManager;
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct CronCreateTool {
    manager: Arc<CronManager>,
}

impl CronCreateTool {
    pub fn new(manager: Arc<CronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for CronCreateTool {
    fn name(&self) -> &str { "cron_create" }
    fn description(&self) -> &str { "创建并持久化一个定时自动执行的 Agent 任务" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "任务简洁标题" },
                "prompt": { "type": "string", "description": "到点自动让 Agent 执行的完整指令提示词" },
                "interval_minutes": { "type": "integer", "description": "执行周期（间隔多少分钟）" }
            },
            "required": ["title", "prompt", "interval_minutes"]
        })
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        let title = match args["title"].as_str() {
            Some(t) => t,
            None => return Ok(ToolResult::error("缺少 title 参数")),
        };
        let prompt = match args["prompt"].as_str() {
            Some(p) => p,
            None => return Ok(ToolResult::error("缺少 prompt 参数")),
        };
        let interval = args["interval_minutes"].as_u64().unwrap_or(60);

        let job = self.manager.add(title, prompt, interval).await?;
        Ok(ToolResult::success(format!(
            "已成功创建定时任务 [{}] 每 {} 分钟自动执行一次。下次执行时间: {}",
            job.id, job.interval_minutes, job.next_run
        )))
    }
}

pub struct CronListTool {
    manager: Arc<CronManager>,
}

impl CronListTool {
    pub fn new(manager: Arc<CronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for CronListTool {
    fn name(&self) -> &str { "cron_list" }
    fn description(&self) -> &str { "列出当前系统中已配置的所有定时自动化任务清单" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _args: &Value, _workspace: &str) -> Result<ToolResult> {
        let jobs = self.manager.list().await;
        if jobs.is_empty() {
            return Ok(ToolResult::success("当前无配置中的定时任务"));
        }
        let out = serde_json::to_string_pretty(&jobs)?;
        Ok(ToolResult::success(out))
    }
}

pub struct CronDeleteTool {
    manager: Arc<CronManager>,
}

impl CronDeleteTool {
    pub fn new(manager: Arc<CronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for CronDeleteTool {
    fn name(&self) -> &str { "cron_delete" }
    fn description(&self) -> &str { "根据任务 ID 删除指定的定时自动化任务" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "要删除的任务 ID，如 cron_17894..." }
            },
            "required": ["id"]
        })
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        let id = match args["id"].as_str() {
            Some(i) => i,
            None => return Ok(ToolResult::error("缺少 id 参数")),
        };
        let ok = self.manager.delete(id).await?;
        if ok {
            Ok(ToolResult::success(format!("已成功删除任务 [{}]", id)))
        } else {
            Ok(ToolResult::error(format!("未找到 ID 为 [{}] 的任务", id)))
        }
    }
}
