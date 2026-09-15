use super::{Tool, ToolResult};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn to_openai_tools(&self) -> Vec<serde_json::Value> {
        let mut list = Vec::new();
        for tool in self.tools.values() {
            list.push(serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name(),
                    "description": tool.description(),
                    "parameters": tool.parameters_schema()
                }
            }));
        }
        list
    }

    pub async fn execute(&self, name: &str, args_json: &str, workspace: &str) -> ToolResult {
        let tool = match self.get(name) {
            Some(t) => t,
            None => return ToolResult::error(format!("未知工具: '{}'", name)),
        };

        let parsed_args: serde_json::Value = match serde_json::from_str(args_json) {
            Ok(val) => val,
            Err(e) => return ToolResult::error(format!("工具参数解析失败: {}", e)),
        };

        match tool.execute(&parsed_args, workspace).await {
            Ok(res) => res,
            Err(e) => ToolResult::error(format!("工具执行异常: {}", e)),
        }
    }
}
