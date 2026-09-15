use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub id: String,
    pub session_key: String,
    pub channel: String,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub content: String,
    pub media_urls: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub id: String,
    pub session_key: String,
    pub channel: String,
    pub recipient_id: String,
    pub content: String,
    pub reply_to_id: Option<String>,
    pub is_intermediate: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStarted {
        call_id: String,
        name: String,
    },
    ToolCallCompleted {
        call_id: String,
        output: String,
        is_error: bool,
    },
    TurnCompleted {
        total_tokens: Option<usize>,
    },
    TurnFailed(String),
}
