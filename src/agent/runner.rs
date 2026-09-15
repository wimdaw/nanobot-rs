use crate::bus::{MessageBus, StreamEvent};
use crate::provider::{ChatRequest, LlmProvider};
use crate::session::{Session, SessionMessage};
use crate::tools::registry::ToolRegistry;
use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;
use tracing::{info, warn};

pub struct AgentRunner {
    provider: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    model: String,
    workspace: String,
    max_turns: usize,
    bus: MessageBus,
}

impl AgentRunner {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        model: String,
        workspace: String,
        max_turns: usize,
        bus: MessageBus,
    ) -> Self {
        Self {
            provider,
            tools,
            model,
            workspace,
            max_turns,
            bus,
        }
    }

    pub async fn run_turn(&self, session: &mut Session, user_input: &str) -> Result<String> {
        // 1. 添加用户消息到会话
        session.messages.push(SessionMessage {
            role: "user".to_string(),
            content: Some(user_input.to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
        });

        let openai_tools = self.tools.to_openai_tools();
        let mut final_content = String::new();
        let mut iterations = 0;

        // 2. 状态机迭代循环 (Model <-> Tools)
        while iterations < self.max_turns {
            iterations += 1;

            let req = ChatRequest {
                model: self.model.clone(),
                messages: session.messages.clone(),
                tools: if openai_tools.is_empty() { None } else { Some(openai_tools.clone()) },
                temperature: Some(0.7),
                max_tokens: Some(4096),
                stream: false,
            };

            let resp = self.provider.chat(&req).await?;

            // 发送思考与文本流式事件
            if let Some(ref r) = resp.reasoning_content {
                self.bus.publish_event(StreamEvent::ReasoningDelta(r.clone()));
            }
            if let Some(ref c) = resp.content {
                self.bus.publish_event(StreamEvent::TextDelta(c.clone()));
                final_content = c.clone();
            }

            // 检查是否有工具调用
            if let Some(ref tool_calls) = resp.tool_calls {
                if !tool_calls.is_empty() {
                    // 记录助手输出包含工具调用的消息
                    session.messages.push(SessionMessage {
                        role: "assistant".to_string(),
                        content: resp.content.clone(),
                        name: None,
                        tool_calls: Some(tool_calls.clone()),
                        tool_call_id: None,
                        timestamp: Utc::now(),
                    });

                    // 依次执行每个工具调用
                    for call in tool_calls {
                        info!("调用工具 [{}]: {}", call.function.name, call.function.arguments);
                        self.bus.publish_event(StreamEvent::ToolCallStarted {
                            call_id: call.id.clone(),
                            name: call.function.name.clone(),
                        });

                        let res = self.tools
                            .execute(&call.function.name, &call.function.arguments, &self.workspace)
                            .await;

                        self.bus.publish_event(StreamEvent::ToolCallCompleted {
                            call_id: call.id.clone(),
                            output: res.output.clone(),
                            is_error: res.is_error,
                        });

                        // 回塞工具返回消息
                        session.messages.push(SessionMessage {
                            role: "tool".to_string(),
                            content: Some(res.output),
                            name: Some(call.function.name.clone()),
                            tool_calls: None,
                            tool_call_id: Some(call.id.clone()),
                            timestamp: Utc::now(),
                        });
                    }

                    // 继续下一轮模型推理，直到模型生成最终答复
                    continue;
                }
            }

            // 无工具调用，单轮对话完成
            session.messages.push(SessionMessage {
                role: "assistant".to_string(),
                content: resp.content.clone(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                timestamp: Utc::now(),
            });

            self.bus.publish_event(StreamEvent::TurnCompleted {
                total_tokens: resp.total_tokens,
            });
            break;
        }

        if iterations >= self.max_turns {
            warn!("达到最大迭代轮次上限 ({})，结束本轮状态机", self.max_turns);
        }

        Ok(final_content)
    }
}
