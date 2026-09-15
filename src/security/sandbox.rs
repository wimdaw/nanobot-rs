use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub allow_outside_workspace: bool,
    pub command_blacklist: Vec<String>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            allow_outside_workspace: false,
            command_blacklist: vec![
                "rm -rf /".to_string(),
                "mkfs".to_string(),
                ":(){ :|:& };:".to_string(),
                "dd if=/dev/zero".to_string(),
                "chmod -R 777 /".to_string(),
            ],
        }
    }
}

impl SecurityPolicy {
    /// 校验文件访问路径是否越界沙箱安全范围
    pub fn validate_path(&self, requested: impl AsRef<Path>, workspace: impl AsRef<Path>) -> Result<PathBuf> {
        let ws_canon = workspace.as_ref().canonicalize().unwrap_or_else(|_| workspace.as_ref().to_path_buf());
        let req_path = if requested.as_ref().is_absolute() {
            requested.as_ref().to_path_buf()
        } else {
            workspace.as_ref().join(requested.as_ref())
        };

        if self.allow_outside_workspace {
            return Ok(req_path);
        }

        // 规范化并校验前缀（兼顾 macOS /var 与 /private/var 符号链接及新创建文件）
        let check_path = if req_path.exists() {
            req_path.canonicalize().unwrap_or(req_path)
        } else if let Some(parent) = req_path.parent() {
            let canon_parent = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
            if let Some(name) = req_path.file_name() {
                canon_parent.join(name)
            } else {
                canon_parent
            }
        } else {
            req_path
        };

        if !check_path.starts_with(&ws_canon) {
            anyhow::bail!(
                "安全沙箱拒绝访问：路径 {:?} 超出允许的工作区范围 {:?}",
                check_path,
                ws_canon
            );
        }

        Ok(check_path)
    }

    /// 校验 Shell 指令是否触发恶意阻断规则
    pub fn validate_command(&self, cmd: &str) -> Result<()> {
        let trimmed = cmd.trim();
        for pattern in &self.command_blacklist {
            if trimmed.contains(pattern) {
                anyhow::bail!("安全审计拦截：命令包含危险指令模式 '{}'，拒绝执行", pattern);
            }
        }
        Ok(())
    }
}
