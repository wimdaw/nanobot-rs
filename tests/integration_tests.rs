use nanobot_rs::bus::{MessageBus, OutboundMessage};
use nanobot_rs::session::compactor::SessionCompactor;
use nanobot_rs::session::store::JsonlStore;
use nanobot_rs::session::{Session, SessionMessage};
use nanobot_rs::tools::builtin::fs::{EditFileTool, ReadFileTool, WriteFileTool};
use nanobot_rs::tools::builtin::patch::ApplyPatchTool;
use nanobot_rs::tools::Tool;
use chrono::Utc;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn test_message_bus_broadcasting() {
    let bus = MessageBus::new(16);

    let mut out_rx1 = bus.subscribe_outbound();
    let mut out_rx2 = bus.subscribe_outbound();

    let out_msg = OutboundMessage {
        id: "msg_1".to_string(),
        session_key: "cli:default".to_string(),
        channel: "cli".to_string(),
        recipient_id: "user".to_string(),
        content: "hello world".to_string(),
        reply_to_id: None,
        is_intermediate: false,
        created_at: Utc::now(),
    };

    bus.send_outbound(out_msg).await.unwrap();

    // 验证多渠道广播订阅：两个订阅者均能收到，互不抢占
    let rec1 = out_rx1.recv().await.unwrap();
    let rec2 = out_rx2.recv().await.unwrap();

    assert_eq!(rec1.id, "msg_1");
    assert_eq!(rec2.id, "msg_1");
    assert_eq!(rec1.content, "hello world");
}

#[tokio::test]
async fn test_jsonl_store_atomic_save_and_list() {
    let dir = tempdir().unwrap();
    let store = JsonlStore::new(dir.path()).unwrap();

    let session_key = "test:user_1";
    let messages = vec![
        SessionMessage {
            role: "user".to_string(),
            content: Some("你好".to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
        },
        SessionMessage {
            role: "assistant".to_string(),
            content: Some("你好！我是nanobot-rs".to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
        },
    ];

    // 原子写入测试
    store.save_session_atomic(session_key, &messages).unwrap();

    // 加载验证
    let loaded = store.load_session(session_key).unwrap();
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.messages[0].content.as_deref(), Some("你好"));

    // 列出会话清单测试
    let list = store.list_sessions().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].session_key, "test:user_1");
    assert_eq!(list[0].message_count, 2);
}

#[test]
fn test_session_compactor_user_boundary_alignment() {
    let mut session = Session::new("test:compact");

    session.messages.push(SessionMessage {
        role: "system".to_string(),
        content: Some("system prompt".to_string()),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        timestamp: Utc::now(),
    });

    // 构造大量消息超过 10 条
    for i in 0..15 {
        session.messages.push(SessionMessage {
            role: "user".to_string(),
            content: Some(format!("user message {}", i)),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
        });
        session.messages.push(SessionMessage {
            role: "assistant".to_string(),
            content: Some(format!("bot reply {}", i)),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
        });
    }

    // 执行压缩：上限 10 条，保留最近 4 条
    let compacted = SessionCompactor::maybe_compact(&mut session, 10, 4);
    assert!(compacted);

    // 验证：第一条依然是系统提示词，第二条是提炼摘要
    assert_eq!(session.messages[0].role, "system");
    assert_eq!(session.messages[1].role, "system");
    assert_eq!(session.messages[1].name.as_deref(), Some("compact_summary"));

    // 验证最近的一条非系统消息必定对齐到 user
    assert_eq!(session.messages[2].role, "user");
}

#[tokio::test]
async fn test_builtin_filesystem_and_patch_tools() {
    let dir = tempdir().unwrap();
    let ws = dir.path().to_str().unwrap();

    let write_tool = WriteFileTool;
    let read_tool = ReadFileTool;
    let edit_tool = EditFileTool;
    let patch_tool = ApplyPatchTool;

    // 1. 写文件
    let res = write_tool
        .execute(
            &json!({ "file_path": "hello.txt", "content": "line 1\nline 2\nline 3" }),
            ws,
        )
        .await
        .unwrap();
    assert!(!res.is_error);

    // 2. 读文件
    let read_res = read_tool
        .execute(&json!({ "file_path": "hello.txt" }), ws)
        .await
        .unwrap();
    assert!(!read_res.is_error);
    assert!(read_res.output.contains("line 1"));

    // 3. 行替换编辑
    let edit_res = edit_tool
        .execute(
            &json!({ "file_path": "hello.txt", "old_string": "line 2", "new_string": "line two modified" }),
            ws,
        )
        .await
        .unwrap();
    assert!(!edit_res.is_error);

    // 4. 应用 Diff 补丁
    let patch_res = patch_tool
        .execute(
            &json!({
                "file_path": "hello.txt",
                "patch": "@@ -1,3 +1,3 @@\n line 1\n-line two modified\n+line two patched\n line 3"
            }),
            ws,
        )
        .await
        .unwrap();
    assert!(!patch_res.is_error);

    let verify_res = read_tool
        .execute(&json!({ "file_path": "hello.txt" }), ws)
        .await
        .unwrap();
    assert!(verify_res.output.contains("line two patched"));
}
