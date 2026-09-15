use anyhow::Result;
use std::path::{Path, PathBuf};

pub mod tools;

#[derive(Clone)]
pub struct MemoryStore {
    file_path: PathBuf,
}

impl MemoryStore {
    pub fn new(storage_dir: impl AsRef<Path>) -> Result<Self> {
        let mem_dir = storage_dir.as_ref().join("memory");
        std::fs::create_dir_all(&mem_dir)?;
        let file_path = mem_dir.join("MEMORY.md");
        if !file_path.exists() {
            std::fs::write(&file_path, "# 长期记忆库 (Long-term Memories)\n\n")?;
        }
        Ok(Self { file_path })
    }

    pub fn load(&self) -> String {
        std::fs::read_to_string(&self.file_path).unwrap_or_default()
    }

    pub fn append(&self, fact: &str) -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        writeln!(file, "- [{}] {}", chrono::Utc::now().format("%Y-%m-%d %H:%M"), fact.trim())?;
        Ok(())
    }

    pub fn search(&self, query: &str) -> Vec<String> {
        let content = self.load();
        let q_lower = query.to_lowercase();
        content
            .lines()
            .filter(|line| line.to_lowercase().contains(&q_lower))
            .map(|l| l.to_string())
            .collect()
    }
}
