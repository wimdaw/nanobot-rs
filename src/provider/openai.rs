use super::{ChatRequest, ChatResponse, LlmProvider, ProviderStreamEvent};
use crate::session::{FunctionCall, ToolCall};
use anyhow::{Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::pin::Pin;

pub struct OpenAiProvider {
    name: String,
    base_url: String,
    api_key: String,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(180))
                .build()
                .unwrap_or_default(),
        }
    }

    fn build_request_body(&self, req: &ChatRequest, stream: bool) -> Value {
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": req.messages,
            "stream": stream
        });

        if let Some(ref tools) = req.tools {
            if !tools.is_empty() {
                body["tools"] = serde_json::json!(tools);
            }
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max) = req.max_tokens {
            body["max_tokens"] = serde_json::json!(max);
        }
        if stream {
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }
        body
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request_body(req, false);

        let mut req_builder = self.client.post(&url).json(&body);
        if !self.api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req_builder
            .send()
            .await
            .with_context(|| format!("请求 LLM 端点失败: {}", url))?;

        let status = resp.status();
        let resp_text = resp.text().await?;

        if !status.is_success() {
            anyhow::bail!("LLM 端点返回 HTTP {}: {}", status, resp_text);
        }

        let json: Value = serde_json::from_str(&resp_text)
            .with_context(|| format!("解析 LLM 响应失败: {}", resp_text))?;

        let choice = json["choices"].get(0).cloned().unwrap_or_default();
        let message = &choice["message"];

        let content = message["content"].as_str().map(|s| s.to_string());
        let reasoning = message["reasoning_content"]
            .as_str()
            .or_else(|| message["reasoning"].as_str())
            .map(|s| s.to_string());
        let finish_reason = choice["finish_reason"].as_str().map(|s| s.to_string());

        let mut tool_calls = None;
        if let Some(tc_array) = message["tool_calls"].as_array() {
            let mut calls = Vec::new();
            for tc in tc_array {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let call_type = tc["type"].as_str().unwrap_or("function").to_string();
                let fn_name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let fn_args = if tc["function"]["arguments"].is_string() {
                    tc["function"]["arguments"].as_str().unwrap().to_string()
                } else {
                    tc["function"]["arguments"].to_string()
                };

                calls.push(ToolCall {
                    id,
                    call_type,
                    function: FunctionCall {
                        name: fn_name,
                        arguments: fn_args,
                    },
                });
            }
            if !calls.is_empty() {
                tool_calls = Some(calls);
            }
        }

        let total_tokens = json["usage"]["total_tokens"].as_u64().map(|v| v as usize);

        Ok(ChatResponse {
            content,
            reasoning_content: reasoning,
            tool_calls,
            finish_reason,
            total_tokens,
        })
    }

    async fn stream(
        &self,
        req: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ProviderStreamEvent>> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request_body(req, true);

        let mut req_builder = self.client.post(&url).json(&body);
        if !self.api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req_builder
            .send()
            .await
            .with_context(|| format!("建立 SSE 流式连接失败: {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("SSE 流式请求失败 HTTP {}: {}", status, err_body);
        }

        let byte_stream = resp.bytes_stream().eventsource();

        let s = try_stream! {
            tokio::pin!(byte_stream);

            while let Some(event_res) = byte_stream.next().await {
                let event = event_res?;
                let data = event.data.trim();

                if data == "[DONE]" {
                    yield ProviderStreamEvent::Completed {
                        finish_reason: Some("stop".to_string()),
                        total_tokens: None,
                    };
                    break;
                }

                if let Ok(json) = serde_json::from_str::<Value>(data) {
                    if let Some(choices) = json["choices"].as_array() {
                        if let Some(choice) = choices.get(0) {
                            let delta = &choice["delta"];

                            // 思考过程 delta
                            if let Some(reasoning) = delta["reasoning_content"]
                                .as_str()
                                .or_else(|| delta["reasoning"].as_str())
                            {
                                yield ProviderStreamEvent::ReasoningDelta(reasoning.to_string());
                            }

                            // 文本内容 delta
                            if let Some(content) = delta["content"].as_str() {
                                yield ProviderStreamEvent::ContentDelta(content.to_string());
                            }

                            // 工具调用增量
                            if let Some(tcs) = delta["tool_calls"].as_array() {
                                for tc in tcs {
                                    let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                                    let id = tc["id"].as_str().map(|s| s.to_string());
                                    let name = tc["function"]["name"].as_str().map(|s| s.to_string());
                                    let args = tc["function"]["arguments"].as_str().unwrap_or("").to_string();

                                    yield ProviderStreamEvent::ToolCallDelta {
                                        index: idx,
                                        id,
                                        name,
                                        arguments: args,
                                    };
                                }
                            }

                            // 结束标记
                            if let Some(fr) = choice["finish_reason"].as_str() {
                                let total_tokens = json["usage"]["total_tokens"].as_u64().map(|v| v as usize);
                                yield ProviderStreamEvent::Completed {
                                    finish_reason: Some(fr.to_string()),
                                    total_tokens,
                                };
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(s))
    }
}
