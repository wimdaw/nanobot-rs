pub mod fs;
pub mod search;
pub mod shell;
pub mod web;

use super::registry::ToolRegistry;
use std::sync::Arc;

pub fn register_all_builtin(registry: &mut ToolRegistry) {
    registry.register(Arc::new(shell::ShellTool));
    registry.register(Arc::new(fs::ReadFileTool));
    registry.register(Arc::new(fs::WriteFileTool));
    registry.register(Arc::new(fs::EditFileTool));
    registry.register(Arc::new(search::FindTool));
    registry.register(Arc::new(search::GrepTool));
    registry.register(Arc::new(web::WebFetchTool::new()));
}
