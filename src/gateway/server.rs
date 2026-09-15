use super::webui;

use crate::agent::AgentRunner;
use crate::bus::MessageBus;
use crate::config::AppConfig;
use crate::provider::LlmProvider;
use crate::session::Session;
use crate::tools::registry::ToolRegistry;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[derive(Clone)]
pub struct GatewayState {
    runner: Arc<AgentRunner>,
    config: AppConfig,
    bus: MessageBus,
}

pub async fn start_http_gateway(
    port: u16,
    config: AppConfig,
    provider: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    bus: MessageBus,
) -> anyhow::Result<()> {
    let runner = Arc::new(AgentRunner::new(
        provider,
        tools,
        config.model.default.clone(),
        config.tools.workspace.clone(),
        config.agent.max_turns,
        bus.clone(),
    ));

    let state = GatewayState {
        runner,
        config,
        bus,
    };

    let app = Router::new()
        .route("/", get(dashboard_handler))
        .route("/dashboard", get(dashboard_handler))
        .route("/health", get(health_handler))
        .route("/v1/models", get(models_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("🚀 nanobot-rs WebUI 与 HTTP 网关正在监听: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn dashboard_handler() -> impl IntoResponse {
    Html(webui::WEBUI_HTML)
}

async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "service": "nanobot-rs",
        "version": "0.1.0"
    }))
}

async fn models_handler(State(state): State<GatewayState>) -> impl IntoResponse {
    Json(json!({
        "object": "list",
        "data": [
            {
                "id": state.config.model.default,
                "object": "model",
                "created": 1789440000,
                "owned_by": "nanobot-rs"
            }
        ]
    }))
}

async fn chat_completions_handler(
    State(state): State<GatewayState>,
    Json(payload): Json<serde_json::Value>,
) -> Response {
    let stream = payload["stream"].as_bool().unwrap_or(false);
    let messages = payload["messages"].as_array();

    let last_user_msg = messages
        .and_then(|arr| {
            arr.iter().rev().find(|m| m["role"].as_str() == Some("user"))
        })
        .and_then(|m| m["content"].as_str())
        .unwrap_or("Hello");

    let mut temp_session = Session::new("http:temp");

    if stream {
        // 真实 SSE 增量流式模式：监听事件总线，逐 Token / 思考过程实时推送
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(64);
        let runner = state.runner.clone();
        let mut event_rx = state.bus.subscribe_events();
        let prompt = last_user_msg.to_string();
        let chunk_id = format!("chatcmpl-{}", chrono::Utc::now().timestamp_millis());

        // 后台启动 Runner 执行任务
        tokio::spawn(async move {
            let _ = runner.run_turn(&mut temp_session, &prompt).await;
        });

        // 监听事件并实时推送 SSE
        tokio::spawn(async move {
            while let Ok(evt) = event_rx.recv().await {
                match evt {
                    crate::bus::StreamEvent::TextDelta(delta) => {
                        let sse_data = json!({
                            "id": chunk_id,
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "choices": [{
                                "index": 0,
                                "delta": { "content": delta },
                                "finish_reason": null
                            }]
                        });
                        let _ = tx.send(Ok(Event::default().data(sse_data.to_string()))).await;
                    }
                    crate::bus::StreamEvent::ReasoningDelta(reasoning) => {
                        let sse_data = json!({
                            "id": chunk_id,
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "choices": [{
                                "index": 0,
                                "delta": { "reasoning_content": reasoning },
                                "finish_reason": null
                            }]
                        });
                        let _ = tx.send(Ok(Event::default().data(sse_data.to_string()))).await;
                    }
                    crate::bus::StreamEvent::TurnCompleted { .. } => {
                        let end_data = json!({
                            "id": chunk_id,
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "choices": [{
                                "index": 0,
                                "delta": {},
                                "finish_reason": "stop"
                            }]
                        });
                        let _ = tx.send(Ok(Event::default().data(end_data.to_string()))).await;
                        let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
                        break;
                    }
                    crate::bus::StreamEvent::TurnFailed(err) => {
                        let err_data = json!({ "error": { "message": err, "type": "turn_failed" } });
                        let _ = tx.send(Ok(Event::default().data(err_data.to_string()))).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        let event_stream = async_stream::stream! {
            while let Some(item) = rx.recv().await {
                yield item;
            }
        };

        Sse::new(event_stream).keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15))).into_response()
    } else {
        // 阻塞非流式模式
        match state.runner.run_turn(&mut temp_session, last_user_msg).await {
            Ok(reply) => Json(json!({
                "id": format!("chatcmpl-{}", chrono::Utc::now().timestamp_millis()),
                "object": "chat.completion",
                "created": chrono::Utc::now().timestamp(),
                "model": state.config.model.default,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": reply
                    },
                    "finish_reason": "stop"
                }]
            }))
            .into_response(),
            Err(e) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "message": e.to_string(), "type": "api_error" } })),
            )
                .into_response(),
        }
    }
}
