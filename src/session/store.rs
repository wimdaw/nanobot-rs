use super::{Session, SessionMessage};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryInfo {
    pub session_key: String,
    pub message_count: usize,
    pub updated_at: chrono::DateTime<Utc>,
    pub file_size_bytes: u64,
}

pub struct JsonlStore {
    base_dir: PathBuf,
}

impl JsonlStore {
    pub fn new(dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_dir)
            .with_context(|| format!("创建会话存储目录失败: {:?}", base_dir))?;
        Ok(Self { base_dir })
    }

    pub fn file_path(&self, session_key: &str) -> PathBuf {
        let safe_name = session_key.replace(':', "__").replace(['/', '\\', ' '], "_");
        self.base_dir.join(format!("{}.jsonl", safe_name))
    }

    pub fn load_session(&self, session_key: &str) -> Result<Session> {
        let path = self.file_path(session_key);
        if !path.exists() {
            return Ok(Session::new(session_key));
        }

        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut session = Session::new(session_key);

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<SessionMessage>(&line) {
                session.messages.push(msg);
            }
        }

        Ok(session)
    }

    /// 原子写入会话：先写入临时文件，再执行原子重命名，防止崩溃导致数据损坏
    pub fn save_session_atomic(&self, session_key: &str, messages: &[SessionMessage]) -> Result<()> {
        let target_path = self.file_path(session_key);
        let tmp_path = self.base_dir.join(format!(
            ".tmp_{}_{}",
            session_key.replace(['/', ':', '\\', ' '], "_"),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp_path)
                .with_context(|| format!("创建临时会话文件失败: {:?}", tmp_path))?;

            for msg in messages {
                let json_line = serde_json::to_string(msg)?;
                writeln!(file, "{}", json_line)?;
            }
            file.flush()?;
        }

        std::fs::rename(&tmp_path, &target_path)
            .with_context(|| format!("原子替换会话文件失败: {:?} -> {:?}", tmp_path, target_path))?;

        Ok(())
    }

    pub fn append_message(&self, session_key: &str, msg: &SessionMessage) -> Result<()> {
        let path = self.file_path(session_key);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("打开会话文件失败: {:?}", path))?;

        let json_line = serde_json::to_string(msg)?;
        writeln!(file, "{}", json_line)?;
        Ok(())
    }

    pub fn clear_session(&self, session_key: &str) -> Result<()> {
        let path = self.file_path(session_key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// 真实列出所有已持久化的会话
    pub fn list_sessions(&self) -> Result<Vec<SessionSummaryInfo>> {
        let mut list = Vec::new();
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let metadata = entry.metadata()?;
                let file_size = metadata.len();
                let modified = metadata.modified()
                    .map(|t| chrono::DateTime::<Utc>::from(t))
                    .unwrap_or_else(|_| Utc::now());

                // 统计行数（消息数）
                let mut count = 0;
                if let Ok(file) = File::open(&path) {
                    count = BufReader::new(file).lines().count();
                }

                list.push(SessionSummaryInfo {
                    session_key: file_name.replace("__", ":"),
                    message_count: count,
                    updated_at: modified,
                    file_size_bytes: file_size,
                });
            }
        }
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(list)
    }
}
