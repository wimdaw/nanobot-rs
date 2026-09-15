use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct ApplyPatchTool;

fn resolve_path(workspace: &str, file_path: &str) -> PathBuf {
    let p = Path::new(file_path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(workspace).join(p)
    }
}

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str { "apply_patch" }
    fn description(&self) -> &str { "对指定文件应用标准 Unified Diff 补丁或指定块替换" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "目标文件路径" },
                "patch": { "type": "string", "description": "Unified Diff 补丁文本 (以 @@ 开始的 diff hunk)" }
            },
            "required": ["file_path", "patch"]
        })
    }
    async fn execute(&self, args: &Value, workspace: &str) -> Result<ToolResult> {
        let file_path = match args["file_path"].as_str() {
            Some(p) => resolve_path(workspace, p),
            None => return Ok(ToolResult::error("缺少 file_path 参数")),
        };
        let patch = match args["patch"].as_str() {
            Some(pt) => pt,
            None => return Ok(ToolResult::error("缺少 patch 参数")),
        };

        if !file_path.exists() {
            return Ok(ToolResult::error(format!("目标文件不存在: {:?}", file_path)));
        }

        let original = tokio::fs::read_to_string(&file_path).await?;
        let orig_lines: Vec<&str> = original.lines().collect();

        // 简易解析并应用 unified diff hunk
        let patch_lines: Vec<&str> = patch.lines().collect();
        let mut to_remove = Vec::new();
        let mut to_add = Vec::new();

        for line in patch_lines {
            if line.starts_with("---") || line.starts_with("+++") || line.starts_with("@@") {
                continue;
            }
            if let Some(stripped) = line.strip_prefix('-') {
                to_remove.push(stripped);
            } else if let Some(stripped) = line.strip_prefix('+') {
                to_add.push(stripped);
            }
        }

        if to_remove.is_empty() && to_add.is_empty() {
            return Ok(ToolResult::error("未识别到有效的补丁变更内容（缺少 +/- 前缀）"));
        }

        // 简单安全替换：如果待删除行全在原始文件里，执行替换
        let remove_block = to_remove.join("\n");
        let add_block = to_add.join("\n");

        if !remove_block.is_empty() && original.contains(&remove_block) {
            let replaced = original.replacen(&remove_block, &add_block, 1);
            tokio::fs::write(&file_path, replaced).await?;
            return Ok(ToolResult::success(format!("已成功对 {:?} 应用补丁", file_path)));
        }

        // 行级匹配
        let mut new_lines = Vec::new();
        let mut i = 0;
        let mut applied = false;
        while i < orig_lines.len() {
            if !to_remove.is_empty() && orig_lines[i] == to_remove[0] {
                // 检查后续是否连续匹配
                let match_len = to_remove.len();
                if i + match_len <= orig_lines.len() && &orig_lines[i..i+match_len] == to_remove.as_slice() {
                    for add in &to_add {
                        new_lines.push(*add);
                    }
                    i += match_len;
                    applied = true;
                    continue;
                }
            }
            new_lines.push(orig_lines[i]);
            i += 1;
        }

        if applied {
            let result_content = new_lines.join("\n");
            tokio::fs::write(&file_path, result_content).await?;
            Ok(ToolResult::success(format!("已成功对 {:?} 应用补丁", file_path)))
        } else {
            Ok(ToolResult::error("无法定位补丁中的上下文行，应用失败"))
        }
    }
}
