use super::{Session, SessionMessage};
use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

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

    fn file_path(&self, session_key: &str) -> PathBuf {
        let safe_name = session_key.replace(['/', ':', '\\', ' '], "_");
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
}
