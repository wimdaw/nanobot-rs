use super::{ChatRequest, ChatResponse, LlmProvider};
use crate::session::{FunctionCall, ToolCall};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

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
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": req.messages,
            "stream": false
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
}
