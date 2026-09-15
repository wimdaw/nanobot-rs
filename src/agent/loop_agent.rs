use super::context::ContextBuilder;
use super::runner::AgentRunner;
use crate::bus::{InboundMessage, MessageBus, OutboundMessage};
use crate::config::AppConfig;
use crate::provider::LlmProvider;
use crate::session::manager::SessionManager;
use crate::session::SessionMessage;
use crate::tools::registry::ToolRegistry;
use chrono::Utc;
use std::sync::Arc;
use tracing::{error, info};

pub struct AgentLoop {
    bus: MessageBus,
    sessions: SessionManager,
    runner: Arc<AgentRunner>,
    workspace: String,
}

impl AgentLoop {
    pub fn new(
        bus: MessageBus,
        config: &AppConfig,
        provider: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
    ) -> anyhow::Result<Self> {
        let sessions = SessionManager::new(&config.session.storage_dir)?;
        let runner = Arc::new(AgentRunner::new(
            provider,
            tools,
            config.model.default.clone(),
            config.tools.workspace.clone(),
            config.agent.max_turns,
            bus.clone(),
        ));

        Ok(Self {
            bus,
            sessions,
            runner,
            workspace: config.tools.workspace.clone(),
        })
    }

    /// 持续从消息总线接收 Inbound 消息并调度处理
    pub async fn run(&self) {
        info!("🤖 Agent 主运行时循环已启动，等待消息输入...");
        while let Some(msg) = self.bus.recv_inbound().await {
            let bus = self.bus.clone();
            let sessions = self.sessions.clone();
            let runner = self.runner.clone();
            let workspace = self.workspace.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_message(bus, sessions, runner, workspace, msg).await {
                    error!("处理消息异常: {}", e);
                }
            });
        }
    }

    async fn handle_message(
        bus: MessageBus,
        sessions: SessionManager,
        runner: Arc<AgentRunner>,
        workspace: String,
        msg: InboundMessage,
    ) -> anyhow::Result<()> {
        let session_key = &msg.session_key;
        let lock = sessions.get_lock(session_key);
        let _guard = lock.lock().await;

        info!("开始处理会话 [{}] 消息: {}", session_key, msg.content);

        let mut session = sessions.load_session(session_key)?;

        // 若为新会话，注入系统提示词
        if session.messages.is_empty() {
            let sys_prompt = ContextBuilder::build_system_prompt(&workspace);
            session.messages.push(SessionMessage {
                role: "system".to_string(),
                content: Some(sys_prompt),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                timestamp: Utc::now(),
            });
        }

        // 执行多轮 Agent 状态机
        let reply_text = runner.run_turn(&mut session, &msg.content).await?;

        // 持久化当前会话状态
        for m in &session.messages {
            sessions.append_message(session_key, m)?;
        }

        // 发送出站消息到总线
        let outbound = OutboundMessage {
            id: format!("out_{}", Utc::now().timestamp_millis()),
            session_key: session_key.to_string(),
            channel: msg.channel.clone(),
            recipient_id: msg.sender_id.clone(),
            content: reply_text,
            reply_to_id: Some(msg.id),
            is_intermediate: false,
            created_at: Utc::now(),
        };

        bus.send_outbound(outbound).await?;
        Ok(())
    }
}
