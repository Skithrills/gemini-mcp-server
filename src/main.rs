use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use clap::Parser;
use color_eyre::eyre::{Report, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::io;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use tokio::time::Duration;
use tracing_subscriber::{self, EnvFilter};
use uuid::Uuid;

mod error;
mod install;

pub const STUDIO_PLUGIN_PORT: u16 = 44755;
const LONG_POLL_DURATION: Duration = Duration::from_secs(15);

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum ToolArgumentValues {
    RunCode { command: String },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ToolCall {
    args: ToolArgumentValues,
    id: Option<Uuid>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RunCommandResponse {
    response: String,
    id: Uuid,
}

pub struct AppState {
    process_queue: VecDeque<ToolCall>,
    output_map: HashMap<Uuid, mpsc::UnboundedSender<Result<String, Report>>>,
    waiter: watch::Receiver<()>,
    trigger: watch::Sender<()>,
}

pub type PackedState = Arc<Mutex<AppState>>;

impl AppState {
    pub fn new() -> Self {
        let (trigger, waiter) = watch::channel(());
        Self {
            process_queue: VecDeque::new(),
            output_map: HashMap::new(),
            waiter,
            trigger,
        }
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    serve: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(io::stderr)
        .with_target(false)
        .with_thread_ids(true)
        .init();

    let args = Args::parse();
    if !args.serve {
        return install::install().await;
    }

    tracing::info!("Starting server...");

    let server_state = Arc::new(Mutex::new(AppState::new()));

    let app = axum::Router::new()
        .route("/request", get(request_handler))
        .route("/response", post(response_handler))
        .route("/prompt", post(gemini_handler))
        .with_state(server_state);

    let listener = tokio::net::TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), STUDIO_PLUGIN_PORT)).await?;

    tracing::info!("Gemini MCP server listening on http://127.0.0.1:{}", STUDIO_PLUGIN_PORT);
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Deserialize)]
struct PromptPayload {
    prompt: String,
}

async fn run_roblox_tool(state: PackedState, args: ToolArgumentValues) -> Result<String, Report> {
    let (id, mut rx) = {
        let mut state = state.lock().await;
        let id = Uuid::new_v4();
        let tool_call = ToolCall {
            args,
            id: Some(id),
        };
        let (tx, rx) = mpsc::unbounded_channel::<Result<String, Report>>();
        state.process_queue.push_back(tool_call);
        state.output_map.insert(id, tx);
        let _ = state.trigger.send(());
        (id, rx)
    };

    let result = rx.recv().await.ok_or_else(|| Report::msg("Channel closed unexpectedly"))?;
    state.lock().await.output_map.remove(&id);
    result
}

async fn gemini_handler(
    State(state): State<PackedState>,
    Json(payload): Json<PromptPayload>,
) -> impl IntoResponse {
    let api_key = match env::var("GEMINI_API_KEY") {
        Ok(val) => val,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Missing GEMINI_API_KEY").into_response(),
    };

    let client = reqwest::Client::new();
    let mut full_text = String::new();
    let mut cursor: Option<String> = None;

    loop {
        let body = serde_json::json!({
            "contents": [{
                "parts": [{ "text": payload.prompt }]
            }],
            "generationConfig": {
                "temperature": 0.7,
                "topK": 1,
                "topP": 1,
                "maxOutputTokens": 2048
            },
            "stream": true,
            "cursor": cursor
        });

        let res = client
            .post("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent")
            .bearer_auth(&api_key)
            .json(&body)
            .send()
            .await;

        let value = match res {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(json) => json,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("JSON parse error: {}", e)).into_response(),
            },
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("HTTP error: {}", e)).into_response(),
        };

        let text = value["candidates"]
            .get(0)
            .and_then(|c| c["content"]["parts"].get(0))
            .and_then(|p| p["text"].as_str())
            .unwrap_or("")
            .to_string();

        full_text += &text;

        cursor = value["candidates"]
            .get(0)
            .and_then(|c| c["cursor"].as_str())
            .map(|s| s.to_string());

        if cursor.is_none() {
            break;
        }
    }

    if let Some(code_to_run) = extract_code(&full_text) {
        let args = ToolArgumentValues::RunCode { command: code_to_run };
        match run_roblox_tool(state, args).await {
            Ok(output) => (
                StatusCode::OK,
                format!("Gemini 2.5 says:\n{}\n\nRoblox Studio output:\n{}", full_text, output),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to run code: {}", e),
            )
                .into_response(),
        }
    } else {
        (StatusCode::OK, full_text).into_response()
    }
}

fn extract_code(text: &str) -> Option<String> {
    text.find("```luau")
        .and_then(|start| text[start + 7..].find("```").map(|end| (start, start + 7 + end)))
        .and_then(|(start, end)| text.get(start..end))
        .map(|code| code.trim().to_string())
}

pub async fn request_handler(State(state): State<PackedState>) -> Response {
    let timeout_result = tokio::time::timeout(LONG_POLL_DURATION, async {
        loop {
            let mut waiter = {
                let mut state = state.lock().await;
                if let Some(task) = state.process_queue.pop_front() {
                    return Ok::<_, Report>(task);
                }
                state.waiter.clone()
            };
            if waiter.changed().await.is_err() {
                break;
            }
        }
        Err(Report::msg("Long poll timed out internally"))
    })
    .await;

    match timeout_result {
        Ok(Ok(task)) => (StatusCode::OK, Json(task)).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {e}")).into_response(),
        Err(_) => (StatusCode::ACCEPTED, "Timeout").into_response(),
    }
}

pub async fn response_handler(
    State(state): State<PackedState>,
    Json(payload): Json<RunCommandResponse>,
) -> impl IntoResponse {
    let state = state.lock().await;
    if let Some(tx) = state.output_map.get(&payload.id) {
        if tx.send(Ok(payload.response)).is_err() {
            tracing::error!("Failed to send response to channel: receiver dropped.");
        }
    } else {
        return (StatusCode::NOT_FOUND, "Unknown ID").into_response();
    }
    StatusCode::OK.into_response()
}