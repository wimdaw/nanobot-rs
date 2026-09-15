pub mod loader;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub fallback_models: Vec<ModelConfig>,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub session: SessionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "default_model_name")]
    pub default: String,
    #[serde(default = "default_provider_name")]
    pub provider: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
}

fn default_model_name() -> String {
    "gemini/gemini-3.8-flash-high".to_string()
}
fn default_provider_name() -> String {
    "custom".to_string()
}
fn default_base_url() -> String {
    "https://api.seurl.eu.org/v1".to_string()
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            default: default_model_name(),
            provider: default_provider_name(),
            base_url: default_base_url(),
            api_key: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
}

fn default_max_turns() -> usize {
    50
}
fn default_reasoning_effort() -> String {
    "medium".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: default_max_turns(),
            verbose: false,
            reasoning_effort: default_reasoning_effort(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelsConfig {
    #[serde(default)]
    pub feishu: FeishuConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeishuConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub app_secret: String,
    #[serde(default)]
    pub verification_token: String,
    #[serde(default)]
    pub encrypt_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_exec_timeout")]
    pub exec_timeout: u64,
    #[serde(default = "default_workspace")]
    pub workspace: String,
}

fn default_exec_timeout() -> u64 {
    60
}
fn default_workspace() -> String {
    ".".to_string()
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            exec_timeout: default_exec_timeout(),
            workspace: default_workspace(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_session_dir")]
    pub storage_dir: String,
}

fn default_session_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("{}/.nanobot-rs/sessions", home)
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            storage_dir: default_session_dir(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
            fallback_models: vec![],
            agent: AgentConfig::default(),
            channels: ChannelsConfig::default(),
            tools: ToolsConfig::default(),
            session: SessionConfig::default(),
        }
    }
}
