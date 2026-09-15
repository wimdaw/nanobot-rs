use clap::{Parser, Subcommand};
use nanobot_rs::agent::AgentLoop;
use nanobot_rs::bus::MessageBus;
use nanobot_rs::channels::{CliChannel, FeishuChannel};
use nanobot_rs::config::loader::load_config;
use nanobot_rs::provider::fallback::FallbackProvider;
use nanobot_rs::provider::openai::OpenAiProvider;
use nanobot_rs::provider::LlmProvider;
use nanobot_rs::sync::{AssetSyncer, DiffReporter};
use nanobot_rs::tools::builtin::register_all_builtin;
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
    #[command(about = "启动常驻网关服务（支持飞书等通道）")]
    Gateway,
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
            println!("nanobot-rs v0.1.0 (built with Rust, powered by tokio)");
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

    // 加载配置
    let config = load_config(cli.config.as_deref())?;

    // 初始化核心消息总线
    let bus = MessageBus::new(256);

    // 构建 Provider 与降级链
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

    // 注册内置工具
    let mut tool_registry = ToolRegistry::new();
    register_all_builtin(&mut tool_registry);

    // 初始化 Agent 运行时主循环
    let agent_loop = AgentLoop::new(bus.clone(), &config, provider, tool_registry)?;

    // 启动后台 Agent 调度循环
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
        Commands::Gateway => {
            println!("🚀 启动 nanobot-rs 常驻网关守护模式...");
            if config.channels.feishu.enabled {
                let feishu = FeishuChannel::new(config.channels.feishu.clone(), bus.clone());
                feishu.start().await?;
            }
            println!("✅ 网关已就绪。按 Ctrl+C 停止。");
            tokio::signal::ctrl_c().await?;
            println!("\n网关已安全退出。");
        }
        _ => {}
    }

    Ok(())
}
