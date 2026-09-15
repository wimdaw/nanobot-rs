use super::context::ContextBuilder;
use super::runner::AgentRunner;
use crate::bus::{InboundMessage, MessageBus, OutboundMessage};
use crate::config::AppConfig;
use crate::memory::MemoryStore;
use crate::provider::LlmProvider;
use crate::session::compactor::SessionCompactor;
use crate::session::manager::SessionManager;
use crate::session::SessionMessage;
use crate::skills::SkillsManager;
use crate::tools::registry::ToolRegistry;
use chrono::Utc;
use std::sync::Arc;
use tracing::{error, info};

pub struct AgentLoop {
    bus: MessageBus,
    sessions: SessionManager,
    runner: Arc<AgentRunner>,
    workspace: String,
    memory: Arc<MemoryStore>,
    skills: Arc<SkillsManager>,
}

impl AgentLoop {
    pub fn new(
        bus: MessageBus,
        config: &AppConfig,
        provider: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        memory: Arc<MemoryStore>,
        skills: Arc<SkillsManager>,
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
            memory,
            skills,
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
            let memory = self.memory.clone();
            let skills = self.skills.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_message(bus, sessions, runner, workspace, memory, skills, msg).await {
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
        memory: Arc<MemoryStore>,
        skills: Arc<SkillsManager>,
        msg: InboundMessage,
    ) -> anyhow::Result<()> {
        let session_key = &msg.session_key;
        let lock = sessions.get_lock(session_key);
        let _guard = lock.lock().await;

        info!("开始处理会话 [{}] 消息: {}", session_key, msg.content);

        let mut session = sessions.load_session(session_key)?;

        // 若为新会话，注入系统提示词与记忆
        if session.messages.is_empty() {
            let memories = memory.load();
            let skills_sec = skills.render_system_prompt_section();
            let sys_prompt = ContextBuilder::build_system_prompt(&workspace, &memories, &skills_sec);
            session.messages.push(SessionMessage {
                role: "system".to_string(),
                content: Some(sys_prompt),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                timestamp: Utc::now(),
            });
        }

        // 智能防滚雪球：检查是否需要上下文压缩（超过 20 条自动提炼老消息，保留最新 8 条）
        SessionCompactor::maybe_compact(&mut session, 20, 8);

        // 上下文治理动态窗口裁剪 (防止超长单轮请求打爆上游 TPM 限额)
        let gov = crate::security::ContextGovernance::default();
        session.messages = gov.govern_messages(&session.messages);

        // 执行多轮 Agent 状态机
        let reply_text = runner.run_turn(&mut session, &msg.content).await?;

        // 原子持久化当前会话状态 (防崩溃导致数据丢失)
        sessions.save_session_atomic(session_key, &session.messages)?;

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
