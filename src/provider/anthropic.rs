use super::{ChatRequest, ChatResponse, LlmProvider, ProviderStreamEvent};
use crate::session::{FunctionCall, ToolCall};
use anyhow::{Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::pin::Pin;

pub struct AnthropicProvider {
    name: String,
    base_url: String,
    api_key: String,
    client: Client,
}

impl AnthropicProvider {
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

    fn build_anthropic_payload(&self, req: &ChatRequest, stream: bool) -> (Option<String>, Value) {
        let mut system_text = None;
        let mut anthropic_messages = Vec::new();

        for m in &req.messages {
            if m.role == "system" {
                if let Some(ref c) = m.content {
                    system_text = Some(c.clone());
                }
                continue;
            }

            let role = match m.role.as_str() {
                "assistant" => "assistant",
                _ => "user",
            };

            let content = m.content.as_deref().unwrap_or("");
            anthropic_messages.push(json!({
                "role": role,
                "content": content
            }));
        }

        let mut body = json!({
            "model": req.model,
            "messages": anthropic_messages,
            "max_tokens": req.max_tokens.unwrap_or(4096),
            "stream": stream
        });

        if let Some(ref s) = system_text {
            body["system"] = json!(s);
        }

        if let Some(ref tools) = req.tools {
            let mut anthropic_tools = Vec::new();
            for t in tools {
                let func = &t["function"];
                anthropic_tools.push(json!({
                    "name": func["name"],
                    "description": func["description"],
                    "input_schema": func["parameters"]
                }));
            }
            if !anthropic_tools.is_empty() {
                body["tools"] = json!(anthropic_tools);
            }
        }

        (system_text, body)
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/v1/messages", self.base_url);
        let (_, body) = self.build_anthropic_payload(req, false);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .with_context(|| format!("请求 Anthropic 端点失败: {}", url))?;

        let status = resp.status();
        let resp_text = resp.text().await?;

        if !status.is_success() {
            anyhow::bail!("Anthropic 返回 HTTP {}: {}", status, resp_text);
        }

        let json: Value = serde_json::from_str(&resp_text)?;
        let mut content = None;
        let mut tool_calls = None;

        if let Some(content_arr) = json["content"].as_array() {
            let mut calls = Vec::new();
            for block in content_arr {
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = block["text"].as_str() {
                            content = Some(t.to_string());
                        }
                    }
                    Some("tool_use") => {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let input = &block["input"];
                        calls.push(ToolCall {
                            id,
                            call_type: "function".to_string(),
                            function: FunctionCall {
                                name,
                                arguments: serde_json::to_string(input).unwrap_or_default(),
                            },
                        });
                    }
                    _ => {}
                }
            }
            if !calls.is_empty() {
                tool_calls = Some(calls);
            }
        }

        let usage = &json["usage"];
        let total = usage["input_tokens"].as_u64().unwrap_or(0) + usage["output_tokens"].as_u64().unwrap_or(0);

        Ok(ChatResponse {
            content,
            reasoning_content: None,
            tool_calls,
            finish_reason: json["stop_reason"].as_str().map(|s| s.to_string()),
            total_tokens: Some(total as usize),
        })
    }

    async fn stream(
        &self,
        req: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ProviderStreamEvent>> + Send>>> {
        let url = format!("{}/v1/messages", self.base_url);
        let (_, body) = self.build_anthropic_payload(req, true);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let err = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic 流式请求失败 HTTP {}: {}", status, err);
        }

        let byte_stream = resp.bytes_stream().eventsource();

        let s = try_stream! {
            tokio::pin!(byte_stream);

            while let Some(event_res) = byte_stream.next().await {
                let event = event_res?;
                let data = event.data.trim();

                if let Ok(json) = serde_json::from_str::<Value>(data) {
                    match json["type"].as_str() {
                        Some("content_block_delta") => {
                            let delta = &json["delta"];
                            if let Some(txt) = delta["text"].as_str() {
                                yield ProviderStreamEvent::ContentDelta(txt.to_string());
                            } else if let Some(th) = delta["thinking"].as_str() {
                                yield ProviderStreamEvent::ReasoningDelta(th.to_string());
                            }
                        }
                        Some("message_stop") => {
                            yield ProviderStreamEvent::Completed {
                                finish_reason: Some("stop".to_string()),
                                total_tokens: None,
                            };
                            break;
                        }
                        _ => {}
                    }
                }
            }
        };

        Ok(Box::pin(s))
    }
}
