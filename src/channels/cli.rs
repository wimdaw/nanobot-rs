use crate::bus::{InboundMessage, MessageBus, StreamEvent};
use anyhow::Result;
use chrono::Utc;
use std::io::{self, BufRead, Write};

pub struct CliChannel {
    bus: MessageBus,
}

impl CliChannel {
    pub fn new(bus: MessageBus) -> Self {
        Self { bus }
    }

    /// 单次问答执行模式（例如 nanobot run "帮我查看当前目录"）
    pub async fn run_single(&self, prompt: &str) -> Result<String> {
        let msg_id = format!("cli_{}", Utc::now().timestamp_millis());
        let session_key = "cli:default".to_string();

        let in_msg = InboundMessage {
            id: msg_id.clone(),
            session_key: session_key.clone(),
            channel: "cli".to_string(),
            sender_id: "user".to_string(),
            sender_name: Some("LocalUser".to_string()),
            content: prompt.to_string(),
            media_urls: vec![],
            created_at: Utc::now(),
        };

        let mut event_rx = self.bus.subscribe_events();
        self.bus.send_inbound(in_msg).await?;

        // 监听流式事件展示
        tokio::spawn(async move {
            while let Ok(evt) = event_rx.recv().await {
                match evt {
                    StreamEvent::ReasoningDelta(r) => {
                        eprint!("\x1b[90m{}\x1b[0m", r);
                        let _ = io::stderr().flush();
                    }
                    StreamEvent::TextDelta(t) => {
                        print!("{}", t);
                        let _ = io::stdout().flush();
                    }
                    StreamEvent::ToolCallStarted { name, .. } => {
                        eprintln!("\n\x1b[36m⚡ [工具执行] {}\x1b[0m", name);
                    }
                    StreamEvent::ToolCallCompleted { output, is_error, .. } => {
                        if is_error {
                            eprintln!("\x1b[31m❌ {}\x1b[0m", output);
                        } else {
                            let preview = if output.len() > 200 { format!("{}...", &output[..200]) } else { output };
                            eprintln!("\x1b[32m✔ {}\x1b[0m", preview.trim());
                        }
                    }
                    StreamEvent::TurnCompleted { .. } => {
                        println!();
                        break;
                    }
                    StreamEvent::TurnFailed(e) => {
                        eprintln!("\n\x1b[31m💥 执行失败: {}\x1b[0m", e);
                        break;
                    }
                }
            }
        });

        // 等待最终出站回复
        while let Some(out) = self.bus.recv_outbound().await {
            if out.session_key == session_key {
                return Ok(out.content);
            }
        }

        Ok(String::new())
    }

    /// 交互式 REPL 会话模式
    pub async fn start_repl(&self) -> Result<()> {
        println!("\x1b[32m🦀 欢迎使用 nanobot-rs (Rust 高性能版)\x1b[0m");
        println!("\x1b[90m输入提示词开始交互，输入 'exit' 或 'quit' 退出。\x1b[0m\n");

        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();

        loop {
            print!("\x1b[33mnanobot> \x1b[0m");
            io::stdout().flush()?;

            let line = match lines.next() {
                Some(Ok(l)) => l.trim().to_string(),
                _ => break,
            };

            if line.is_empty() {
                continue;
            }
            if line == "exit" || line == "quit" {
                println!("再见！");
                break;
            }

            self.run_single(&line).await?;
            println!();
        }

        Ok(())
    }
}
