use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn resolve_path(workspace: &str, file_path: &str) -> std::result::Result<PathBuf, String> {
    let p = Path::new(file_path);
    let policy = crate::security::SecurityPolicy::default();
    policy.validate_path(p, workspace).map_err(|e| e.to_string())
}

pub struct ReadFileTool;
#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "读取指定文件的文本内容，支持行号和内容分块" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件绝对路径或相对工作区的相对路径" },
                "offset": { "type": "integer", "description": "开始行号 (从1开始)" },
                "limit": { "type": "integer", "description": "读取最多行数" }
            },
            "required": ["file_path"]
        })
    }
    async fn execute(&self, args: &Value, workspace: &str) -> Result<ToolResult> {
        let file_path = match args["file_path"].as_str() {
            Some(p) => match resolve_path(workspace, p) {
                Ok(path) => path,
                Err(e) => return Ok(ToolResult::error(e)),
            },
            None => return Ok(ToolResult::error("缺少 file_path 参数")),
        };
        if !file_path.exists() {
            return Ok(ToolResult::error(format!("文件不存在: {:?}", file_path)));
        }
        let content = tokio::fs::read_to_string(&file_path).await?;
        let lines: Vec<&str> = content.lines().collect();
        let offset = args["offset"].as_u64().unwrap_or(1).max(1) as usize - 1;
        let limit = args["limit"].as_u64().unwrap_or(2000) as usize;

        let end = (offset + limit).min(lines.len());
        if offset >= lines.len() {
            return Ok(ToolResult::success("（已超过文件最大行数，无更多内容）"));
        }

        let slice = &lines[offset..end];
        let numbered: Vec<String> = slice
            .iter()
            .enumerate()
            .map(|(idx, line)| format!("{:4}\t{}", offset + idx + 1, line))
            .collect();

        Ok(ToolResult::success(numbered.join("\n")))
    }
}

pub struct WriteFileTool;
#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "完全写入或覆盖文件内容" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件路径" },
                "content": { "type": "string", "description": "要写入的文本内容" }
            },
            "required": ["file_path", "content"]
        })
    }
    async fn execute(&self, args: &Value, workspace: &str) -> Result<ToolResult> {
        let file_path = match args["file_path"].as_str() {
            Some(p) => match resolve_path(workspace, p) {
                Ok(path) => path,
                Err(e) => return Ok(ToolResult::error(e)),
            },
            None => return Ok(ToolResult::error("缺少 file_path 参数")),
        };
        let content = match args["content"].as_str() {
            Some(c) => c,
            None => return Ok(ToolResult::error("缺少 content 参数")),
        };
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file_path, content).await?;
        Ok(ToolResult::success(format!("成功写入文件: {:?}", file_path)))
    }
}

pub struct EditFileTool;
#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str { "在文件中执行精确的原文字符串替换" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件路径" },
                "old_string": { "type": "string", "description": "待替换的原始文本内容" },
                "new_string": { "type": "string", "description": "替换后的新文本内容" }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }
    async fn execute(&self, args: &Value, workspace: &str) -> Result<ToolResult> {
        let file_path = match args["file_path"].as_str() {
            Some(p) => match resolve_path(workspace, p) {
                Ok(path) => path,
                Err(e) => return Ok(ToolResult::error(e)),
            },
            None => return Ok(ToolResult::error("缺少 file_path 参数")),
        };
        let old_str = match args["old_string"].as_str() {
            Some(s) => s,
            None => return Ok(ToolResult::error("缺少 old_string 参数")),
        };
        let new_str = match args["new_string"].as_str() {
            Some(s) => s,
            None => return Ok(ToolResult::error("缺少 new_string 参数")),
        };

        if !file_path.exists() {
            return Ok(ToolResult::error(format!("文件不存在: {:?}", file_path)));
        }
        let content = tokio::fs::read_to_string(&file_path).await?;
        if !content.contains(old_str) {
            return Ok(ToolResult::error("未能找到完全匹配的 old_string，请核对缩进与换行"));
        }
        let replaced = content.replacen(old_str, new_str, 1);
        tokio::fs::write(&file_path, replaced).await?;
        Ok(ToolResult::success(format!("已成功更新文件: {:?}", file_path)))
    }
}
