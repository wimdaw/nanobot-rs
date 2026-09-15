use super::{Session, SessionMessage};
use chrono::Utc;
use tracing::info;

pub struct SessionCompactor;

impl SessionCompactor {
    /// 检查并自动压缩超长会话历史，防止 Token 滚雪球
    pub fn maybe_compact(session: &mut Session, max_messages: usize, keep_tail: usize) -> bool {
        if session.messages.len() <= max_messages {
            return false;
        }

        info!(
            "会话 [{}] 历史消息达到 {} 轮，触发智能上下文压缩...",
            session.session_key,
            session.messages.len()
        );

        // 分离系统提示词
        let system_msg = session.messages.iter().find(|m| m.role == "system").cloned();
        let non_system: Vec<_> = session.messages.iter().filter(|m| m.role != "system").cloned().collect();

        if non_system.len() <= keep_tail {
            return false;
        }

        let mut split_idx = non_system.len().saturating_sub(keep_tail);

        // 关键安全对齐：必须保证切分点从 user 消息开始，严禁将 tool 响应与前面的 tool_call 拆散
        while split_idx < non_system.len() && non_system[split_idx].role != "user" {
            split_idx += 1;
        }

        if split_idx >= non_system.len() || split_idx == 0 {
            return false;
        }

        let older_messages = &non_system[..split_idx];
        let recent_messages = &non_system[split_idx..];

        // 提取老消息的核心对话摘要
        let mut summary = String::from("【前序会话历史提炼摘要】:\n");
        for m in older_messages {
            if let Some(ref text) = m.content {
                let char_count = text.chars().count();
                let snippet = if char_count > 60 {
                    format!("{}...", text.chars().take(60).collect::<String>())
                } else {
                    text.clone()
                };
                summary.push_str(&format!("- {}: {}\n", m.role, snippet.trim()));
            }
        }

        let mut compacted = Vec::new();
        if let Some(sys) = system_msg {
            compacted.push(sys);
        }

        // 插入提炼后的摘要作为系统上下文备忘
        compacted.push(SessionMessage {
            role: "system".to_string(),
            content: Some(summary),
            name: Some("compact_summary".to_string()),
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
        });

        compacted.extend_from_slice(recent_messages);
        session.messages = compacted;
        session.updated_at = Utc::now();

        info!("✅ 会话历史已成功安全对齐并压缩，保留最近 {} 条精细上下文", recent_messages.len());
        true
    }
}
