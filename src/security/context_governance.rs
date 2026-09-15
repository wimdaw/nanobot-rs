use crate::session::SessionMessage;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ContextGovernance {
    pub max_prompt_tokens: usize,
    pub reserve_completion_tokens: usize,
}

impl Default for ContextGovernance {
    fn default() -> Self {
        Self {
            max_prompt_tokens: 128_000,
            reserve_completion_tokens: 4_096,
        }
    }
}

impl ContextGovernance {
    /// 估算单条消息的大致 Token 数量 (中英文字符加权经验估算)
    pub fn estimate_tokens(text: &str) -> usize {
        let mut tokens = 0;
        for c in text.chars() {
            if c.is_ascii() {
                tokens += 1; // 约 4 chars = 1 token, 这里保守估计
            } else {
                tokens += 2; // 中文大约 1 字符 = 1~2 tokens
            }
        }
        tokens / 2 + 1
    }

    /// 上下文治理检查与安全窗口裁剪：确保发给 LLM 的总提示词在安全预算之内
    pub fn govern_messages(&self, messages: &[SessionMessage]) -> Vec<SessionMessage> {
        let budget = self.max_prompt_tokens.saturating_sub(self.reserve_completion_tokens);
        let mut total_tokens = 0;
        let mut tokens_per_msg = Vec::with_capacity(messages.len());

        for m in messages {
            let mut t = 4; // 角色基础开销
            if let Some(ref c) = m.content {
                t += Self::estimate_tokens(c);
            }
            if let Some(ref tc) = m.tool_calls {
                for c in tc {
                    t += Self::estimate_tokens(&c.function.name) + Self::estimate_tokens(&c.function.arguments);
                }
            }
            tokens_per_msg.push(t);
            total_tokens += t;
        }

        if total_tokens <= budget {
            return messages.to_vec();
        }

        info!(
            "上下文治理介入：当前预估 Token ({}) 超过安全预算 ({})，执行动态滑动窗口截断...",
            total_tokens, budget
        );

        // 保留 system 提示词 + 从后向前截取最新对话，直到触达预算上限
        let mut governed = Vec::new();
        if let Some(first) = messages.first() {
            if first.role == "system" {
                governed.push(first.clone());
            }
        }

        let mut accumulated = 0;
        let mut keep_from_idx = messages.len();

        for i in (0..messages.len()).rev() {
            if messages[i].role == "system" {
                continue;
            }
            if accumulated + tokens_per_msg[i] > budget {
                break;
            }
            accumulated += tokens_per_msg[i];
            keep_from_idx = i;
        }

        // 确保从 user 消息作为切入点
        while keep_from_idx < messages.len() && messages[keep_from_idx].role != "user" {
            keep_from_idx += 1;
        }

        for i in keep_from_idx..messages.len() {
            governed.push(messages[i].clone());
        }

        governed
    }
}
