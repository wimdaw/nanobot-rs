pub const WEBUI_HTML: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>nanobot-rs 控制台</title>
  <style>
    :root {
      --bg: #090d16;
      --card-bg: rgba(18, 26, 43, 0.7);
      --border: rgba(255, 255, 255, 0.08);
      --accent: #ff4757;
      --accent-grad: linear-gradient(135deg, #ff4757, #2ed573);
      --text: #f1f2f6;
      --text-muted: #a4b0be;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      background: var(--bg);
      color: var(--text);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", sans-serif;
      min-height: 100vh;
      display: flex;
    }
    aside {
      width: 260px;
      background: rgba(13, 19, 33, 0.9);
      border-right: 1px solid var(--border);
      padding: 24px 16px;
      display: flex;
      flex-direction: column;
      gap: 20px;
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 10px;
      font-size: 18px;
      font-weight: 800;
    }
    .brand span { color: #2ed573; font-size: 12px; background: rgba(46, 213, 115, 0.15); padding: 2px 8px; border-radius: 10px; }
    nav a {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 10px 14px;
      color: var(--text-muted);
      text-decoration: none;
      border-radius: 8px;
      font-size: 14px;
      transition: all 0.2s;
    }
    nav a.active, nav a:hover { background: rgba(255, 255, 255, 0.06); color: #fff; }
    main {
      flex: 1;
      padding: 30px;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
      gap: 24px;
    }
    .card {
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 24px;
      backdrop-filter: blur(10px);
    }
    .chat-box {
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 14px;
      max-height: 600px;
      overflow-y: auto;
      padding: 10px;
      border: 1px solid var(--border);
      border-radius: 10px;
      background: rgba(0,0,0,0.2);
    }
    .msg {
      max-width: 80%;
      padding: 10px 16px;
      border-radius: 12px;
      font-size: 14px;
      line-height: 1.5;
    }
    .msg.user { align-self: flex-end; background: #3742fa; color: #fff; }
    .msg.bot { align-self: flex-start; background: rgba(255,255,255,0.06); border: 1px solid var(--border); }
    .chat-input-bar {
      display: flex;
      gap: 10px;
      margin-top: 10px;
    }
    input[type="text"] {
      flex: 1;
      background: rgba(0,0,0,0.3);
      border: 1px solid var(--border);
      padding: 12px 16px;
      color: #fff;
      border-radius: 8px;
      outline: none;
    }
    button {
      background: #2ed573;
      color: #000;
      font-weight: 700;
      border: none;
      padding: 12px 24px;
      border-radius: 8px;
      cursor: pointer;
    }
    button:hover { opacity: 0.9; }
  </style>
</head>
<body>
  <aside>
    <div class="brand">
      🦀 nanobot-rs
      <span>Rust</span>
    </div>
    <nav>
      <a href="#" class="active">💬 实时对话</a>
      <a href="#" onclick="alert('端点：/v1/chat/completions')">🔌 API 网关</a>
      <a href="#" onclick="alert('系统运行内存约 15MB')">📊 运行时状态</a>
    </nav>
  </aside>
  <main>
    <div class="card">
      <h2 style="font-size:18px;margin-bottom:8px;">对话与任务执行</h2>
      <p style="font-size:13px;color:var(--text-muted);margin-bottom:16px;">由 Rust 原生运行时驱动，集成多轮 Tool Calling 与长期记忆</p>
      <div class="chat-box" id="chatBox">
        <div class="msg bot">你好！我是 nanobot-rs，请问有什么可以协助你的？</div>
      </div>
      <div class="chat-input-bar">
        <input type="text" id="userInput" placeholder="输入指令或提问，支持自主调用 bash/文件读写/长期记忆..." onkeydown="if(event.key==='Enter') sendMsg()">
        <button onclick="sendMsg()">发送</button>
      </div>
    </div>
  </main>
  <script>
    async function sendMsg() {
      const inp = document.getElementById('userInput');
      const text = inp.value.trim();
      if (!text) return;
      inp.value = '';
      const box = document.getElementById('chatBox');
      box.innerHTML += `<div class="msg user">${escapeHtml(text)}</div>`;
      box.scrollTop = box.scrollHeight;

      const botMsg = document.createElement('div');
      botMsg.className = 'msg bot';
      botMsg.innerText = '正在思考中...';
      box.appendChild(botMsg);
      box.scrollTop = box.scrollHeight;

      try {
        const resp = await fetch('/v1/chat/completions', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            messages: [{role: 'user', content: text}],
            stream: false
          })
        });
        const data = await resp.json();
        const content = data.choices && data.choices[0] && data.choices[0].message ? data.choices[0].message.content : (data.error ? data.error.message : '无输出');
        botMsg.innerText = content;
      } catch (e) {
        botMsg.innerText = '请求异常: ' + e.message;
      }
      box.scrollTop = box.scrollHeight;
    }
    function escapeHtml(s) {
      return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }
  </script>
</body>
</html>
"##;
