use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::path::Path;
use walkdir::WalkDir;

pub struct FindTool;
#[async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str { "find_files" }
    fn description(&self) -> &str { "在目录中递归搜索匹配特定模式的文件名" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "文件名匹配正则或子字符串" },
                "path": { "type": "string", "description": "搜索起始目录 (默认当前工作区)" }
            },
            "required": ["pattern"]
        })
    }
    async fn execute(&self, args: &Value, workspace: &str) -> Result<ToolResult> {
        let pattern = match args["pattern"].as_str() {
            Some(p) => p,
            None => return Ok(ToolResult::error("缺少 pattern 参数")),
        };
        let re = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(format!("正则表达式无效: {}", e))),
        };

        let start_dir = args["path"].as_str().unwrap_or(workspace);
        let mut results = Vec::new();

        for entry in WalkDir::new(start_dir).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if re.is_match(name) {
                    results.push(p.display().to_string());
                    if results.len() >= 100 {
                        results.push("... (已达到最大 100 条匹配上限)".to_string());
                        break;
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(ToolResult::success("未找到匹配的文件"))
        } else {
            Ok(ToolResult::success(results.join("\n")))
        }
    }
}

pub struct GrepTool;
#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep_content" }
    fn description(&self) -> &str { "在文件中使用正则表达式搜索包含特定内容的行" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "正则搜索模式" },
                "path": { "type": "string", "description": "目标文件或目录路径" }
            },
            "required": ["pattern"]
        })
    }
    async fn execute(&self, args: &Value, workspace: &str) -> Result<ToolResult> {
        let pattern = match args["pattern"].as_str() {
            Some(p) => p,
            None => return Ok(ToolResult::error("缺少 pattern 参数")),
        };
        let re = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(format!("正则表达式无效: {}", e))),
        };

        let target_path = args["path"].as_str().unwrap_or(workspace);
        let p = Path::new(target_path);
        let mut matches = Vec::new();

        let walker: Box<dyn Iterator<Item = _>> = if p.is_file() {
            Box::new(std::iter::once(p.to_path_buf()))
        } else {
            Box::new(
                WalkDir::new(p)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .map(|e| e.into_path()),
            )
        };

        for file_path in walker {
            // 忽略二进制文件或大文件
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                for (idx, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        matches.push(format!("{}:{}: {}", file_path.display(), idx + 1, line.trim()));
                        if matches.len() >= 100 {
                            matches.push("... (已达到最大 100 条匹配上限)".to_string());
                            break;
                        }
                    }
                }
            }
            if matches.len() >= 100 {
                break;
            }
        }

        if matches.is_empty() {
            Ok(ToolResult::success("未找到匹配的内容"))
        } else {
            Ok(ToolResult::success(matches.join("\n")))
        }
    }
}
