use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const SYNC_STATE_FILE: &str = ".nanobot-sync-state.json";
const UPSTREAM_RAW_BASE: &str = "https://raw.githubusercontent.com/HKUDS/nanobot/main/nanobot";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub upstream_repo: String,
    pub tracked_branch: String,
    pub last_synced_commit: String,
    pub last_check_time: String,
    #[serde(default)]
    pub synced_files: Vec<String>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            upstream_repo: "HKUDS/nanobot".to_string(),
            tracked_branch: "main".to_string(),
            last_synced_commit: String::new(),
            last_check_time: Utc::now().to_rfc3339(),
            synced_files: vec![],
        }
    }
}

pub struct AssetSyncer {
    client: reqwest::Client,
    state_path: PathBuf,
}

impl Default for AssetSyncer {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSyncer {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .unwrap_or_default(),
            state_path: PathBuf::from(SYNC_STATE_FILE),
        }
    }

    pub fn load_state(&self) -> SyncState {
        if self.state_path.exists() {
            if let Ok(c) = std::fs::read_to_string(&self.state_path) {
                if let Ok(state) = serde_json::from_str::<SyncState>(&c) {
                    return state;
                }
            }
        }
        SyncState::default()
    }

    pub fn save_state(&self, state: &SyncState) -> Result<()> {
        let json = serde_json::to_string_pretty(state)?;
        std::fs::write(&self.state_path, json)?;
        Ok(())
    }

    /// 自动拉取上游核心 Prompt 模板并更新到本工程
    pub async fn pull_upstream_templates(&self, target_dir: &Path) -> Result<Vec<String>> {
        let template_files = [
            "templates/SOUL.md",
            "templates/AGENTS.md",
            "templates/USER.md",
            "templates/HEARTBEAT.md",
            "templates/agent/identity.md",
            "templates/agent/tool_contract.md",
        ];

        let mut updated = Vec::new();
        for rel in template_files {
            let url = format!("{}/{}", UPSTREAM_RAW_BASE, rel);
            let target_file = target_dir.join(rel.trim_start_matches("templates/"));

            if let Some(parent) = target_file.parent() {
                std::fs::create_dir_all(parent)?;
            }

            match self.client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let content = resp.text().await?;
                    std::fs::write(&target_file, content)?;
                    updated.push(rel.to_string());
                }
                Ok(resp) => {
                    tracing::warn!("拉取 {} 失败: HTTP {}", url, resp.status());
                }
                Err(e) => {
                    tracing::warn!("拉取 {} 发生网络异常: {}", url, e);
                }
            }
        }

        let mut state = self.load_state();
        state.last_check_time = Utc::now().to_rfc3339();
        state.synced_files = updated.clone();
        self.save_state(&state)?;

        Ok(updated)
    }
}
