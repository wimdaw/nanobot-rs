use crate::bus::{MessageBus, StreamEvent};
use crate::provider::{ChatRequest, LlmProvider, ProviderStreamEvent};
use crate::session::{FunctionCall, Session, SessionMessage, ToolCall};
use crate::tools::registry::ToolRegistry;
use anyhow::Result;
use chrono::Utc;
use futures::StreamExt;
use std::collections::HashMap;
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
                stream: true,
            };

            // 优先尝试真实流式处理，降级为非流式
            let (content, _reasoning, tool_calls, total_tokens) = match self.provider.stream(&req).await {
                Ok(mut stream) => {
                    let mut text_acc = String::new();
                    let mut reasoning_acc = String::new();
                    let mut tools_map: HashMap<usize, (Option<String>, Option<String>, String)> = HashMap::new();
                    let mut tokens = None;

                    while let Some(evt_res) = stream.next().await {
                        if let Ok(evt) = evt_res {
                            match evt {
                                ProviderStreamEvent::ReasoningDelta(r) => {
                                    self.bus.publish_event(StreamEvent::ReasoningDelta(r.clone()));
                                    reasoning_acc.push_str(&r);
                                }
                                ProviderStreamEvent::ContentDelta(c) => {
                                    self.bus.publish_event(StreamEvent::TextDelta(c.clone()));
                                    text_acc.push_str(&c);
                                }
                                ProviderStreamEvent::ToolCallDelta { index, id, name, arguments } => {
                                    let entry = tools_map.entry(index).or_insert((None, None, String::new()));
                                    if id.is_some() { entry.0 = id; }
                                    if name.is_some() { entry.1 = name; }
                                    entry.2.push_str(&arguments);
                                }
                                ProviderStreamEvent::Completed { total_tokens: tt, .. } => {
                                    tokens = tt;
                                }
                            }
                        }
                    }

                    let parsed_calls = if !tools_map.is_empty() {
                        let mut sorted_indices: Vec<_> = tools_map.keys().copied().collect();
                        sorted_indices.sort();
                        let calls: Vec<ToolCall> = sorted_indices
                            .into_iter()
                            .filter_map(|idx| {
                                let (id, name, args) = tools_map.remove(&idx)?;
                                Some(ToolCall {
                                    id: id.unwrap_or_else(|| format!("call_{}", idx)),
                                    call_type: "function".to_string(),
                                    function: FunctionCall {
                                        name: name.unwrap_or_default(),
                                        arguments: args,
                                    },
                                })
                            })
                            .collect();
                        if calls.is_empty() { None } else { Some(calls) }
                    } else {
                        None
                    };

                    (
                        if text_acc.is_empty() { None } else { Some(text_acc) },
                        if reasoning_acc.is_empty() { None } else { Some(reasoning_acc) },
                        parsed_calls,
                        tokens,
                    )
                }
                Err(stream_err) => {
                    warn!("流式请求失败 ({})，回退到非流式重试...", stream_err);
                    let mut non_stream_req = req.clone();
                    non_stream_req.stream = false;
                    let resp = self.provider.chat(&non_stream_req).await?;
                    if let Some(ref c) = resp.content {
                        self.bus.publish_event(StreamEvent::TextDelta(c.clone()));
                    }
                    (resp.content, resp.reasoning_content, resp.tool_calls, resp.total_tokens)
                }
            };

            if let Some(ref c) = content {
                final_content = c.clone();
            }

            // 检查是否有工具调用
            if let Some(ref calls) = tool_calls {
                if !calls.is_empty() {
                    // 记录助手输出包含工具调用的消息
                    session.messages.push(SessionMessage {
                        role: "assistant".to_string(),
                        content: content.clone(),
                        name: None,
                        tool_calls: Some(calls.clone()),
                        tool_call_id: None,
                        timestamp: Utc::now(),
                    });

                    // 依次执行每个工具调用
                    for call in calls {
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
                content: content.clone(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                timestamp: Utc::now(),
            });

            self.bus.publish_event(StreamEvent::TurnCompleted {
                total_tokens,
            });
            break;
        }

        if iterations >= self.max_turns {
            warn!("达到最大迭代轮次上限 ({})，结束本轮状态机", self.max_turns);
        }

        Ok(final_content)
    }
}
