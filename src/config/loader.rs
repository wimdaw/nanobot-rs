use super::AppConfig;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn default_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".nanobot-rs").join("config.yaml")
}

pub fn load_config(custom_path: Option<&Path>) -> Result<AppConfig> {
    let path = match custom_path {
        Some(p) => p.to_path_buf(),
        None => {
            let local_cfg = Path::new("nanobot.yaml");
            if local_cfg.exists() {
                local_cfg.to_path_buf()
            } else {
                default_config_path()
            }
        }
    };

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("读取配置文件失败: {:?}", path))?;
        serde_yaml::from_str::<AppConfig>(&content)
            .with_context(|| format!("解析配置文件失败: {:?}", path))?
    } else {
        AppConfig::default()
    };

    // 环境变量优先覆盖
    if let Ok(key) = std::env::var("NANOBOT_API_KEY") {
        config.model.api_key = key;
    }
    if let Ok(url) = std::env::var("NANOBOT_BASE_URL") {
        config.model.base_url = url;
    }
    if let Ok(model) = std::env::var("NANOBOT_MODEL") {
        config.model.default = model;
    }
    if let Ok(ws) = std::env::var("NANOBOT_WORKSPACE") {
        config.tools.workspace = ws;
    }

    Ok(config)
}
