use crate::agent::AgentRunner;
use crate::bus::MessageBus;
use crate::config::AppConfig;
use crate::provider::LlmProvider;
use crate::session::Session;
use crate::tools::registry::ToolRegistry;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Json, Response};
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
        bus,
    ));

    let state = GatewayState { runner, config };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/v1/models", get(models_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("🚀 nanobot-rs OpenAI 兼容 HTTP 网关正在监听: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
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
        // SSE 流式模式
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(32);
        let runner = state.runner.clone();
        let prompt = last_user_msg.to_string();

        tokio::spawn(async move {
            let chunk_id = format!("chatcmpl-{}", chrono::Utc::now().timestamp_millis());
            match runner.run_turn(&mut temp_session, &prompt).await {
                Ok(content) => {
                    let sse_data = json!({
                        "id": chunk_id,
                        "object": "chat.completion.chunk",
                        "created": chrono::Utc::now().timestamp(),
                        "choices": [{
                            "index": 0,
                            "delta": { "content": content },
                            "finish_reason": "stop"
                        }]
                    });
                    let _ = tx.send(Ok(Event::default().data(sse_data.to_string()))).await;
                    let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
                }
                Err(e) => {
                    let sse_err = json!({ "error": e.to_string() });
                    let _ = tx.send(Ok(Event::default().data(sse_err.to_string()))).await;
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
