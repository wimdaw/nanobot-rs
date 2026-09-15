use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use walkdir::WalkDir;

pub mod tool;

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub content: String,
}

#[derive(Clone, Default)]
pub struct SkillsManager {
    skills: Arc<HashMap<String, Skill>>,
}

impl SkillsManager {
    pub fn load_from_dirs(dirs: &[PathBuf]) -> Self {
        let mut map = HashMap::new();
        for dir in dirs {
            if !dir.exists() {
                continue;
            }
            for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_file() && p.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
                    if let Ok(content) = std::fs::read_to_string(p) {
                        let name = p
                            .parent()
                            .and_then(|d| d.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        // 提取第一行或简要描述
                        let desc = content
                            .lines()
                            .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
                            .unwrap_or("无描述")
                            .trim()
                            .to_string();

                        map.insert(
                            name.clone(),
                            Skill {
                                name,
                                description: desc,
                                path: p.to_path_buf(),
                                content,
                            },
                        );
                    }
                }
            }
        }
        Self {
            skills: Arc::new(map),
        }
    }

    pub fn get(&self, name: &str) -> Option<Skill> {
        self.skills.get(name).cloned()
    }

    pub fn list(&self) -> Vec<Skill> {
        self.skills.values().cloned().collect()
    }

    pub fn render_system_prompt_section(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }
        let mut out = String::from("# Available Skills\n\nWhen user requests a matching skill, call `load_skill` to read its full instructions before proceeding:\n\n");
        for s in self.skills.values() {
            out.push_str(&format!("- **{}**: {}\n", s.name, s.description));
        }
        out.push('\n');
        out
    }
}
