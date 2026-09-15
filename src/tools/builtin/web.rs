use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

pub struct WebFetchTool {
    client: reqwest::Client,
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str { "web_fetch" }
    fn description(&self) -> &str { "抓取指定 URL 网页的文本内容并提取有效信息" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "要抓取的网页 HTTP/HTTPS 链接" }
            },
            "required": ["url"]
        })
    }
    async fn execute(&self, args: &Value, _workspace: &str) -> Result<ToolResult> {
        let url = match args["url"].as_str() {
            Some(u) => u,
            None => return Ok(ToolResult::error("缺少 url 参数")),
        };

        let resp = match self.client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(format!("HTTP 请求失败: {}", e))),
        };

        let status = resp.status();
        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => return Ok(ToolResult::error(format!("读取响应体失败: {}", e))),
        };

        // 简易去除 HTML 脚本和标签
        let stripped = body
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('<') && !l.ends_with('>'))
            .collect::<Vec<_>>()
            .join("\n");

        let summary = if stripped.len() > 4000 {
            format!("{}...\n（内容过长，已截断展示前 4000 字符）", &stripped[..4000])
        } else {
            stripped
        };

        Ok(ToolResult::success(format!("HTTP {} 来自 {}:\n{}", status, url, summary)))
    }
}
