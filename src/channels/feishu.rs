use crate::bus::{InboundMessage, MessageBus};
use crate::config::FeishuConfig;
use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;
use tracing::{error, info};

pub struct FeishuChannel {
    config: FeishuConfig,
    bus: MessageBus,
}

impl FeishuChannel {
    pub fn new(config: FeishuConfig, bus: MessageBus) -> Self {
        Self { config, bus }
    }

    /// 获取 Tenant Access Token
    pub async fn get_tenant_access_token(&self) -> Result<String> {
        let client = reqwest::Client::new();
        let resp = client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&serde_json::json!({
                "app_id": self.config.app_id,
                "app_secret": self.config.app_secret
            }))
            .send()
            .await?;

        let json: Value = resp.json().await?;
        let token = json["tenant_access_token"]
            .as_str()
            .context("获取飞书 tenant_access_token 失败")?;
        Ok(token.to_string())
    }

    /// 发送飞书消息回复
    pub async fn send_message(&self, receive_id_type: &str, receive_id: &str, content: &str) -> Result<()> {
        let token = self.get_tenant_access_token().await?;
        let client = reqwest::Client::new();

        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": serde_json::json!({ "text": content }).to_string()
        });

        let url = format!(
            "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type={}",
            receive_id_type
        );

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err = resp.text().await?;
            error!("发送飞书消息失败: {}", err);
        }

        Ok(())
    }

    /// 启动飞书渠道消息轮询监听或出站回复监听器
    pub async fn start(&self) -> Result<()> {
        info!("飞书渠道组件已就绪 (App ID: {})", self.config.app_id);

        let bus = self.bus.clone();
        let self_feishu = FeishuChannel {
            config: self.config.clone(),
            bus: self.bus.clone(),
        };

        // 监听总线出站消息并回复飞书 (广播订阅，不与其他通道竞争抢消息)
        tokio::spawn(async move {
            let mut out_rx = bus.subscribe_outbound();
            while let Ok(out) = out_rx.recv().await {
                if out.channel == "feishu" {
                    let _ = self_feishu.send_message("open_id", &out.recipient_id, &out.content).await;
                }
            }
        });

        Ok(())
    }

    /// 收到飞书消息事件投递入总线
    pub async fn on_incoming_message(&self, chat_id: &str, sender_id: &str, text: &str) -> Result<()> {
        let msg = InboundMessage {
            id: format!("fs_{}", Utc::now().timestamp_millis()),
            session_key: format!("feishu:{}", chat_id),
            channel: "feishu".to_string(),
            sender_id: sender_id.to_string(),
            sender_name: None,
            content: text.to_string(),
            media_urls: vec![],
            created_at: Utc::now(),
        };
        self.bus.send_inbound(msg).await?;
        Ok(())
    }
}
