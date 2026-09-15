use chrono::Utc;

pub struct ContextBuilder;

impl ContextBuilder {
    pub fn build_system_prompt(workspace: &str, memories: &str, skills_section: &str) -> String {
        let soul = include_str!("templates/SOUL.md");
        let tool_contract = include_str!("templates/agent/tool_contract.md");
        let date_str = Utc::now().format("%Y-%m-%d").to_string();

        format!(
            "You are nanobot-rs, an ultra-lightweight, high-performance personal AI agent rewritten in Rust.\n\
            Current Platform: {} {}\n\
            Current Date: {}\n\
            Current Workspace: {}\n\n\
            # Core Persona & Guidelines\n{}\n\n\
            # Tool Contract\n{}\n\n\
            # Long-term Memories\n{}\n\n\
            {}\n\n\
            Always be direct, action-oriented, and concise. When asked to perform tasks, prefer dedicated tools over talking.",
            std::env::consts::OS,
            std::env::consts::ARCH,
            date_str,
            workspace,
            soul,
            tool_contract,
            if memories.trim().is_empty() { "（暂无持久化记忆）" } else { memories },
            skills_section
        )
    }
}
