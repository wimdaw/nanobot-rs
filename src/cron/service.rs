use crate::bus::{InboundMessage, MessageBus};
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub interval_minutes: u64,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: DateTime<Utc>,
}

#[derive(Clone)]
pub struct CronManager {
    file_path: PathBuf,
    jobs: Arc<RwLock<Vec<CronJob>>>,
    bus: MessageBus,
}

impl CronManager {
    pub fn new(storage_dir: impl AsRef<Path>, bus: MessageBus) -> Result<Self> {
        let file_path = storage_dir.as_ref().join("cron_jobs.json");
        let mut initial_jobs = Vec::new();

        if file_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                if let Ok(parsed) = serde_json::from_str::<Vec<CronJob>>(&content) {
                    initial_jobs = parsed;
                }
            }
        }

        Ok(Self {
            file_path,
            jobs: Arc::new(RwLock::new(initial_jobs)),
            bus,
        })
    }

    pub async fn list(&self) -> Vec<CronJob> {
        self.jobs.read().await.clone()
    }

    pub async fn add(&self, title: &str, prompt: &str, interval_minutes: u64) -> Result<CronJob> {
        let now = Utc::now();
        let job = CronJob {
            id: format!("cron_{}", now.timestamp_millis()),
            title: title.to_string(),
            prompt: prompt.to_string(),
            interval_minutes: interval_minutes.max(1),
            enabled: true,
            last_run: None,
            next_run: now + Duration::minutes(interval_minutes.max(1) as i64),
        };

        {
            let mut list = self.jobs.write().await;
            list.push(job.clone());
        }
        self.save().await?;
        info!("添加定时自动化任务 [{}]: {}", job.id, job.title);
        Ok(job)
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let found;
        {
            let mut list = self.jobs.write().await;
            let len_before = list.len();
            list.retain(|j| j.id != id);
            found = list.len() < len_before;
        }
        if found {
            self.save().await?;
        }
        Ok(found)
    }

    async fn save(&self) -> Result<()> {
        let list = self.jobs.read().await;
        let json = serde_json::to_string_pretty(&*list)?;
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.file_path, json)?;
        Ok(())
    }

    /// 启动后台定时轮询器 (每 30 秒检查一次到点任务)
    pub async fn start_loop(&self) {
        let self_clone = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                ticker.tick().await;
                let now = Utc::now();
                let mut due_jobs = Vec::new();

                {
                    let mut list = self_clone.jobs.write().await;
                    for job in list.iter_mut() {
                        if job.enabled && now >= job.next_run {
                            job.last_run = Some(now);
                            job.next_run = now + Duration::minutes(job.interval_minutes as i64);
                            due_jobs.push(job.clone());
                        }
                    }
                }

                if !due_jobs.is_empty() {
                    let _ = self_clone.save().await;
                    for job in due_jobs {
                        info!("⏰ 触发定时自动化执行: [{}] {}", job.id, job.title);
                        let in_msg = InboundMessage {
                            id: format!("cron_fire_{}", now.timestamp_millis()),
                            session_key: format!("cron:{}", job.id),
                            channel: "cron".to_string(),
                            sender_id: "system_cron".to_string(),
                            sender_name: Some("CronScheduler".to_string()),
                            content: job.prompt.clone(),
                            media_urls: vec![],
                            created_at: now,
                        };
                        if let Err(e) = self_clone.bus.send_inbound(in_msg).await {
                            error!("投递定时任务消息失败: {}", e);
                        }
                    }
                }
            }
        });
    }
}
