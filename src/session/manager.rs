use super::store::{JsonlStore, SessionSummaryInfo};
use super::{Session, SessionMessage};
use anyhow::Result;
use dashmap::DashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SessionManager {
    store: Arc<JsonlStore>,
    locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl SessionManager {
    pub fn new(storage_dir: impl AsRef<Path>) -> Result<Self> {
        let store = Arc::new(JsonlStore::new(storage_dir)?);
        Ok(Self {
            store,
            locks: Arc::new(DashMap::new()),
        })
    }

    /// 获取针对特定 session_key 的细粒度并发排队互斥锁
    pub fn get_lock(&self, session_key: &str) -> Arc<Mutex<()>> {
        self.locks
            .entry(session_key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub fn load_session(&self, session_key: &str) -> Result<Session> {
        self.store.load_session(session_key)
    }

    /// 原子替换保存整个会话
    pub fn save_session_atomic(&self, session_key: &str, messages: &[SessionMessage]) -> Result<()> {
        self.store.save_session_atomic(session_key, messages)
    }

    pub fn append_message(&self, session_key: &str, msg: &SessionMessage) -> Result<()> {
        self.store.append_message(session_key, msg)
    }

    pub fn reset_session(&self, session_key: &str) -> Result<()> {
        self.store.clear_session(session_key)
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionSummaryInfo>> {
        self.store.list_sessions()
    }
}
