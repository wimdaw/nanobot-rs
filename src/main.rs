use clap::{Parser, Subcommand};
use nanobot_rs::agent::AgentLoop;
use nanobot_rs::bus::MessageBus;
use nanobot_rs::channels::{CliChannel, FeishuChannel};
use nanobot_rs::config::loader::load_config;
use nanobot_rs::cron::service::CronManager;
use nanobot_rs::gateway::start_http_gateway;
use nanobot_rs::memory::tools::{MemorySaveTool, MemorySearchTool};
use nanobot_rs::memory::MemoryStore;
use nanobot_rs::provider::fallback::FallbackProvider;
use nanobot_rs::provider::openai::OpenAiProvider;
use nanobot_rs::provider::LlmProvider;
use nanobot_rs::skills::tool::LoadSkillTool;
use nanobot_rs::skills::SkillsManager;
use nanobot_rs::sync::{AssetSyncer, DiffReporter};
use nanobot_rs::tools::builtin::cron::{CronCreateTool, CronDeleteTool, CronListTool};
use nanobot_rs::tools::builtin::register_all_builtin;
use nanobot_rs::tools::builtin::sessions_tool::{SessionHistoryTool, SessionListTool};
use nanobot_rs::tools::builtin::spawn::SpawnTool;
use nanobot_rs::tools::registry::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "nanobot", version = "0.1.0", about = "极轻量、高性能 Rust 版个人 AI Agent 框架")]
struct Cli {
    #[arg(short, long, help = "自定义配置文件路径")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "单次执行任务提示词")]
    Run {
        #[arg(help = "发给 Agent 的提示词指令")]
        prompt: String,
    },
    #[command(about = "启动交互式终端 REPL 会话")]
    Chat,
    #[command(about = "启动常驻网关服务（包含飞书与 OpenAI 兼容 HTTP 服务）")]
    Gateway {
        #[arg(short, long, default_value = "18790", help = "HTTP 网关监听端口")]
        port: u16,
    },
    #[command(about = "上游 (HKUDS/nanobot) 版本同步与检查")]
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    #[command(about = "显示当前版本与平台信息")]
    Version,
}

#[derive(Subcommand)]
enum SyncAction {
    #[command(about = "检查上游 GitHub 仓库更新与提交对比")]
    Check,
    #[command(about = "自动同步并覆盖上游最新 Prompt 模板资源")]
    PullAssets,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Version => {
            println!("nanobot-rs v0.1.0 (built with Rust, powered by tokio & axum)");
            println!("Target: {} {}", std::env::consts::OS, std::env::consts::ARCH);
            return Ok(());
        }
        Commands::Sync { action } => match action {
            SyncAction::Check => {
                println!("正在检查 HKUDS/nanobot 上游版本更新...\n");
                let report = DiffReporter::generate_report().await?;
                println!("{}", report);
                return Ok(());
            }
            SyncAction::PullAssets => {
                println!("正在从上游拉取最新的 Prompt 模板与规范...\n");
                let syncer = AssetSyncer::new();
                let target_dir = PathBuf::from("src/agent/templates");
                let updated = syncer.pull_upstream_templates(&target_dir).await?;
                println!("✅ 成功同步 {} 个模板文件:", updated.len());
                for f in updated {
                    println!("  ✔ {}", f);
                }
                return Ok(());
            }
        },
        _ => {}
    }

    // 1. 加载配置
    let config = load_config(cli.config.as_deref())?;

    // 2. 初始化核心消息总线
    let bus = MessageBus::new(256);

    // 3. 构建 Provider 与故障转移降级链
    let primary_provider: Arc<dyn LlmProvider> = Arc::new(OpenAiProvider::new(
        &config.model.provider,
        &config.model.base_url,
        &config.model.api_key,
    ));

    let mut fallbacks = Vec::new();
    for fb in &config.fallback_models {
        let p: Arc<dyn LlmProvider> = Arc::new(OpenAiProvider::new(&fb.provider, &fb.base_url, &fb.api_key));
        fallbacks.push((p, fb.default.clone()));
    }
    let provider: Arc<dyn LlmProvider> = Arc::new(FallbackProvider::new(primary_provider, fallbacks));

    // 4. 初始化长期记忆存储与技能管理器
    let memory_store = Arc::new(MemoryStore::new(&config.session.storage_dir)?);
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let skills_dirs = vec![
        PathBuf::from(&home).join(".nanobot-rs").join("skills"),
        PathBuf::from(&config.session.storage_dir).join("skills"),
        PathBuf::from(&config.tools.workspace).join("skills"),
    ];
    let skills_manager = Arc::new(SkillsManager::load_from_dirs(&skills_dirs));

    // 5. 注册内置核心工具与记忆/技能专属工具
    let mut tool_registry = ToolRegistry::new();
    register_all_builtin(&mut tool_registry);
    tool_registry.register(Arc::new(MemorySaveTool::new(memory_store.clone())));
    tool_registry.register(Arc::new(MemorySearchTool::new(memory_store.clone())));
    tool_registry.register(Arc::new(LoadSkillTool::new(skills_manager.clone())));

    // 定时任务调度器与专属 Cron 工具
    let cron_manager = Arc::new(CronManager::new(&config.session.storage_dir, bus.clone())?);
    cron_manager.start_loop().await;
    tool_registry.register(Arc::new(CronCreateTool::new(cron_manager.clone())));
    tool_registry.register(Arc::new(CronListTool::new(cron_manager.clone())));
    tool_registry.register(Arc::new(CronDeleteTool::new(cron_manager.clone())));

    // 会话查看与跨会话工具
    let session_mgr = Arc::new(nanobot_rs::session::manager::SessionManager::new(&config.session.storage_dir)?);
    tool_registry.register(Arc::new(SessionListTool::new(session_mgr.clone())));
    tool_registry.register(Arc::new(SessionHistoryTool::new(session_mgr.clone())));

    // 派生子智能体工具 (SpawnTool)
    let subagent_runner = Arc::new(nanobot_rs::agent::AgentRunner::new(
        provider.clone(),
        tool_registry.clone(),
        config.model.default.clone(),
        config.tools.workspace.clone(),
        config.agent.max_turns,
        bus.clone(),
    ));
    tool_registry.register(Arc::new(SpawnTool::new(subagent_runner)));

    // 6. 初始化 Agent 运行时主循环
    let agent_loop = AgentLoop::new(
        bus.clone(),
        &config,
        provider.clone(),
        tool_registry.clone(),
        memory_store.clone(),
        skills_manager.clone(),
    )?;

    // 7. 启动后台 Agent 调度循环
    tokio::spawn(async move {
        agent_loop.run().await;
    });

    match cli.command {
        Commands::Run { prompt } => {
            let cli_channel = CliChannel::new(bus.clone());
            let _ = cli_channel.run_single(&prompt).await?;
        }
        Commands::Chat => {
            let cli_channel = CliChannel::new(bus.clone());
            cli_channel.start_repl().await?;
        }
        Commands::Gateway { port } => {
            println!("🚀 启动 nanobot-rs 网关守护模式 (HTTP 端口: {})...", port);
            if config.channels.feishu.enabled {
                let feishu = FeishuChannel::new(config.channels.feishu.clone(), bus.clone());
                feishu.start().await?;
            }

            // 启动 OpenAI 兼容 HTTP 网关
            start_http_gateway(port, config, provider, tool_registry, bus).await?;
        }
        _ => {}
    }

    Ok(())
}
