use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const UPSTREAM_REPO: &str = "HKUDS/nanobot";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamCommit {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

pub struct UpstreamTracker {
    client: reqwest::Client,
    token: Option<String>,
}

impl Default for UpstreamTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl UpstreamTracker {
    pub fn new() -> Self {
        let token = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")).ok();
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent("nanobot-rs-updater/0.1.0")
                .build()
                .unwrap_or_default(),
            token,
        }
    }

    /// 获取上游仓库最新提交列表
    pub async fn fetch_latest_commits(&self, limit: usize) -> Result<Vec<UpstreamCommit>> {
        let url = format!(
            "https://api.github.com/repos/{}/commits?sha=main&per_page={}",
            UPSTREAM_REPO, limit
        );

        let mut req = self.client.get(&url);
        if let Some(ref t) = self.token {
            req = req.header("Authorization", format!("Bearer {}", t));
        }

        let resp = req.send().await.with_context(|| format!("请求 GitHub API 失败: {}", url))?;
        if !resp.status().is_success() {
            anyhow::bail!("GitHub API 返回错误码 HTTP {}: {}", resp.status(), resp.text().await?);
        }

        let json: serde_json::Value = resp.json().await?;
        let mut list = Vec::new();

        if let Some(arr) = json.as_array() {
            for item in arr {
                let sha = item["sha"].as_str().unwrap_or("").to_string();
                let commit = &item["commit"];
                let message = commit["message"].as_str().unwrap_or("").to_string();
                let author = commit["author"]["name"].as_str().unwrap_or("").to_string();
                let date = commit["author"]["date"].as_str().unwrap_or("").to_string();

                list.push(UpstreamCommit {
                    sha,
                    message,
                    author,
                    date,
                });
            }
        }

        Ok(list)
    }

    /// 获取上游最新 Release Tag
    pub async fn fetch_latest_tag(&self) -> Result<Option<String>> {
        let url = format!("https://api.github.com/repos/{}/releases/latest", UPSTREAM_REPO);
        let mut req = self.client.get(&url);
        if let Some(ref t) = self.token {
            req = req.header("Authorization", format!("Bearer {}", t));
        }

        let resp = req.send().await?;
        if resp.status().is_success() {
            let json: serde_json::Value = resp.json().await?;
            return Ok(json["tag_name"].as_str().map(|s| s.to_string()));
        }

        Ok(None)
    }
}
