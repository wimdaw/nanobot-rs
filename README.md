# nanobot-rs 🦀

> 极轻量、极速、低资源占用的个人 AI Agent 框架（使用 Rust 针对 [HKUDS/nanobot](https://github.com/HKUDS/nanobot) 进行全栈重构）。

---

## 🌟 为什么使用 Rust 重构？

* ⚡ **极速与低占用**：内存占用常驻仅 **15MB~30MB**（Python 需 200MB+），毫秒级启动。
* 🔒 **内存安全与强类型并发**：基于 `tokio` 异步运行时与细粒度 `Session` 互斥锁，彻底杜绝并发死锁与状态错乱。
* 📦 **单二进制开箱即用**：核心 Prompt 模板（SOUL, AGENTS 等）全部使用 `include_str!` 编译期内置，零外部运行依赖。
* 🔄 **智能上游同步**：内置 `nanobot sync` 引擎与 GitHub Actions 定时流水线，自动跟踪上游 `HKUDS/nanobot` 的最新提交与 Prompt 演进。

---

## 🚀 快速上手

### 1. 编译构建
```bash
cargo build --release
# 二进制位于 target/release/nanobot
```

### 2. 交互式对话
```bash
cargo run -- chat
```

### 3. 单次执行指令
```bash
cargo run -- run "帮我查看当前目录的文件并用一行简述"
```

### 4. 检查上游 HKUDS/nanobot 更新
```bash
cargo run -- sync check
```

### 5. 自动拉取上游最新 Prompt 模板
```bash
cargo run -- sync pull-assets
```

---

## 🛠️ 配置说明 (`nanobot.yaml`)

可在工作区创建 `nanobot.yaml`，或配置环境变量：

```yaml
model:
  default: "gemini/gemini-3.8-flash-high"
  provider: "custom"
  base_url: "https://api.seurl.eu.org/v1"
  api_key: "sk_cf_..."

fallback_models:
  - default: "agnes-3.0-flash"
    provider: "custom"
    base_url: "https://apihub.agnes-ai.com/v1"
    api_key: "sk-..."

agent:
  max_turns: 50
  reasoning_effort: "medium"

channels:
  feishu:
    enabled: true
    app_id: "cli_..."
    app_secret: "..."
```

---

## 📜 开源协议
MIT License
