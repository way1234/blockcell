use blockcell_agent::{
    AgentRuntime, CapabilityRegistryAdapter, ConfirmRequest, CoreEvolutionAdapter,
    MemoryStoreAdapter, MessageBus, ProviderLLMBridge, SkillScriptKind, TaskManager,
};
#[cfg(feature = "dingtalk")]
use blockcell_channels::dingtalk::DingTalkChannel;
#[cfg(feature = "discord")]
use blockcell_channels::discord::DiscordChannel;
#[cfg(feature = "feishu")]
use blockcell_channels::feishu::FeishuChannel;
#[cfg(feature = "slack")]
use blockcell_channels::slack::SlackChannel;
#[cfg(feature = "telegram")]
use blockcell_channels::telegram::TelegramChannel;
#[cfg(feature = "wecom")]
use blockcell_channels::wecom::WeComChannel;
#[cfg(feature = "whatsapp")]
use blockcell_channels::whatsapp::WhatsAppChannel;
use blockcell_channels::ChannelManager;
use blockcell_core::{Config, InboundMessage, OutboundMessage, Paths};
use blockcell_scheduler::{
    CronJob, CronService, GhostService, GhostServiceConfig, HeartbeatService, JobPayload,
    JobSchedule, JobState, ScheduleKind,
};
use blockcell_skills::{new_registry_handle, CoreEvolution};
use blockcell_skills::{EvolutionService, EvolutionServiceConfig};
use blockcell_storage::{MemoryStore, SessionStore};
use blockcell_tools::{
    CapabilityRegistryHandle, CoreEvolutionHandle, MemoryStoreHandle, ToolRegistry,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{debug, error, info, warn};

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Path as AxumPath, Query, State,
    },
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

mod alerts;
mod banner;
mod capabilities;
mod channels;
mod chat;
mod config_api;
mod cron;
mod files;
mod memory;
mod outbound;
mod sessions;
mod skills_install;
mod streams;
mod toggles;
mod webhooks;
mod websocket;
mod webui;

use alerts::*;
use banner::*;
use capabilities::*;
use channels::*;
use chat::*;
use config_api::*;
use cron::*;
use files::*;
use memory::*;
use outbound::*;
use sessions::*;
use skills_install::*;
use streams::*;
use toggles::*;
use webhooks::*;
use websocket::*;
use webui::*;

// ---------------------------------------------------------------------------
// WebSocket event types for structured protocol
// ---------------------------------------------------------------------------

/// Events broadcast from runtime to all connected WebSocket clients
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum WsEvent {
    #[serde(rename = "message_done")]
    MessageDone {
        chat_id: String,
        task_id: String,
        content: String,
        tool_calls: usize,
        duration_ms: u64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        media: Vec<String>,
    },
    #[serde(rename = "error")]
    Error { chat_id: String, message: String },
}

// ---------------------------------------------------------------------------
// Shared state passed to HTTP/WS handlers
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct GatewayState {
    inbound_tx: mpsc::Sender<InboundMessage>,
    task_manager: TaskManager,
    config: Config,
    paths: Paths,
    api_token: Option<String>,
    /// Broadcast channel for streaming events to WebSocket clients
    ws_broadcast: broadcast::Sender<String>,
    /// Pending path-confirmation requests waiting for WebUI user response (keyed by request_id)
    pending_confirms: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
    /// Pending path-confirmation requests waiting for non-ws channel user reply (keyed by "channel:chat_id")
    #[allow(dead_code)]
    pending_channel_confirms: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
    /// Session store for session CRUD
    session_store: Arc<SessionStore>,
    /// Cron service for cron CRUD
    cron_service: Arc<CronService>,
    /// Memory store handle
    memory_store: Option<MemoryStoreHandle>,
    /// Tool registry for listing tools
    tool_registry: Arc<ToolRegistry>,
    /// Password for WebUI login (configured or auto-generated)
    web_password: String,
    /// Channel manager for status reporting
    channel_manager: Arc<blockcell_channels::ChannelManager>,
    /// Shared EvolutionService for trigger/delete/status handlers
    evolution_service: Arc<Mutex<EvolutionService>>,
}

fn secure_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (&x, &y) in a.as_bytes().iter().zip(b.as_bytes().iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn url_decode(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' => {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let hi = bytes[i + 1];
                let lo = bytes[i + 2];
                let hex = |c: u8| -> Option<u8> {
                    match c {
                        b'0'..=b'9' => Some(c - b'0'),
                        b'a'..=b'f' => Some(c - b'a' + 10),
                        b'A'..=b'F' => Some(c - b'A' + 10),
                        _ => None,
                    }
                };
                let h = hex(hi)?;
                let l = hex(lo)?;
                out.push((h * 16 + l) as char);
                i += 3;
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    Some(out)
}

fn token_from_query(req: &Request<axum::body::Body>) -> Option<String> {
    let q = req.uri().query()?;
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=')?;

        if k == "token" {
            return url_decode(v);
        }
    }
    None
}

fn validate_workspace_relative_path(path: &str) -> Result<std::path::PathBuf, String> {
    if path.trim().is_empty() {
        return Err("path is required".to_string());
    }
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    let mut normalized = std::path::PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(s) => normalized.push(s),
            std::path::Component::ParentDir => {
                return Err("path traversal (..) is not allowed".to_string());
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err("invalid path".to_string());
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err("invalid path".to_string());
    }
    Ok(normalized)
}

fn primary_pool_entry(config: &Config) -> Option<&blockcell_core::config::ModelEntry> {
    config
        .agents
        .defaults
        .model_pool
        .iter()
        .min_by(|a, b| a.priority.cmp(&b.priority).then(b.weight.cmp(&a.weight)))
}

fn active_model_and_provider(config: &Config) -> (String, Option<String>, &'static str) {
    if let Some(entry) = primary_pool_entry(config) {
        return (
            entry.model.clone(),
            Some(entry.provider.clone()),
            "modelPool",
        );
    }

    (
        config.agents.defaults.model.clone(),
        config.agents.defaults.provider.clone(),
        "agents.defaults",
    )
}

// ---------------------------------------------------------------------------
// Bearer token authentication middleware
// ---------------------------------------------------------------------------

async fn auth_middleware(
    State(state): State<GatewayState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let token = match &state.api_token {
        Some(t) if !t.is_empty() => t,
        _ => return next.run(req).await,
    };

    if req.uri().path() == "/v1/health" || req.uri().path() == "/v1/auth/login" {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let authorized = match auth_header {
        Some(h) if h.starts_with("Bearer ") => secure_eq(&h[7..], token.as_str()),
        _ => false,
    };

    let authorized = authorized
        || token_from_query(&req)
            .map(|v| secure_eq(&v, token.as_str()))
            .unwrap_or(false);

    if authorized {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            "Unauthorized: invalid or missing Bearer token",
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// Main gateway entry point
// ---------------------------------------------------------------------------

pub async fn run(cli_host: Option<String>, cli_port: Option<u16>) -> anyhow::Result<()> {
    let paths = Paths::new();
    let mut config = Config::load_or_default(&paths)?;

    // Ensure autoUpgrade.manifestUrl has a value (migrates old configs with empty string)
    if config.auto_upgrade.manifest_url.is_empty() {
        config.auto_upgrade.manifest_url =
            "https://github.com/blockcell-labs/blockcell/releases/latest/download/manifest.json"
                .to_string();
        let _ = config.save(&paths.config_file());
    }

    // Auto-generate and persist node_alias if not set (short 8-char hex, e.g. "54c6be7b").
    // This becomes the stable display name for this node in the community hub.
    if config.community_hub.node_alias.is_none() {
        let alias = uuid::Uuid::new_v4().to_string().replace('-', "")[..8].to_string();
        config.community_hub.node_alias = Some(alias.clone());
        if let Err(e) = config.save(&paths.config_file()) {
            warn!("Failed to persist node_alias to config.json: {}", e);
        } else {
            info!(node_alias = %alias, "Generated and persisted node_alias to config.json");
        }
    }

    // If Community Hub is configured but apiKey is missing/empty, auto-register and persist.
    if let Some(hub_url) = config.community_hub_url() {
        if config.community_hub_api_key().is_none() {
            let register_url = format!("{}/v1/auth/register", hub_url.trim_end_matches('/'));
            let name = config
                .community_hub
                .node_alias
                .clone()
                .unwrap_or_else(|| "blockcell-gateway".to_string());

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default();

            let body = serde_json::json!({
                "name": name,
                "email": null,
                "github_id": null,
            });

            match client.post(&register_url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    if status.is_success() {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(api_key) = v.get("api_key").and_then(|x| x.as_str()) {
                                if !api_key.trim().is_empty() {
                                    config.community_hub.api_key = Some(api_key.trim().to_string());
                                    if let Err(e) = config.save(&paths.config_file()) {
                                        warn!(error = %e, "Failed to persist community hub apiKey to config file");
                                    } else {
                                        info!("Registered with Community Hub and persisted apiKey to config");
                                    }
                                }
                            }
                        }
                    } else {
                        warn!(status = %status, body = %text, "Community Hub register failed");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to register with Community Hub");
                }
            }
        }
    }

    // Resolve host/port: CLI args override config values
    let host = cli_host.unwrap_or_else(|| config.gateway.host.clone());
    let port = cli_port.unwrap_or(config.gateway.port);

    // Auto-generate and persist api_token if not configured or empty.
    // This ensures a stable token across restarts without manual setup.
    let needs_token = config
        .gateway
        .api_token
        .as_deref()
        .map(|t| t.trim().is_empty())
        .unwrap_or(true);
    if needs_token {
        let env_token = std::env::var("BLOCKCELL_API_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        if let Some(token) = env_token {
            // Use env var but don't persist — user manages it externally
            config.gateway.api_token = Some(token);
        } else {
            // Generate a 64-char token (bc_ + 4×UUID hex = 3+32*4=131 chars, take first 61 for bc_+61=64)
            let raw = format!(
                "{}{}{}{}",
                uuid::Uuid::new_v4().to_string().replace('-', ""),
                uuid::Uuid::new_v4().to_string().replace('-', ""),
                uuid::Uuid::new_v4().to_string().replace('-', ""),
                uuid::Uuid::new_v4().to_string().replace('-', ""),
            );
            let generated = format!("bc_{}", &raw[..61]);
            config.gateway.api_token = Some(generated);
            if let Err(e) = config.save(&paths.config_file()) {
                warn!(
                    "Failed to persist auto-generated apiToken to config.json: {}",
                    e
                );
            } else {
                info!("Auto-generated apiToken persisted to config.json");
            }
        }
    }

    info!(host = %host, port = port, "Starting blockcell gateway");

    // ── Multi-provider dispatch (same logic as agent CLI) ──
    let provider_pool = blockcell_providers::ProviderPool::from_config(&config)?;

    // ── Initialize memory store (SQLite + FTS5) ──
    let memory_db_path = paths.memory_dir().join("memory.db");
    let memory_store_handle: Option<MemoryStoreHandle> = match MemoryStore::open(&memory_db_path) {
        Ok(store) => {
            if let Err(e) = store.migrate_from_files(&paths.memory_dir()) {
                warn!("Memory migration failed: {}", e);
            }
            let adapter = MemoryStoreAdapter::new(store);
            Some(Arc::new(adapter))
        }
        Err(e) => {
            warn!(
                "Failed to open memory store: {}. Memory tools will be unavailable.",
                e
            );
            None
        }
    };

    // ── Initialize tool evolution registry and core evolution engine ──
    let cap_registry_dir = paths.evolved_tools_dir();
    let cap_registry_raw = new_registry_handle(cap_registry_dir);
    {
        let mut reg = cap_registry_raw.lock().await;
        let _ = reg.load();
        let rehydrated = reg.rehydrate_executors();
        if rehydrated > 0 {
            info!("Rehydrated {} evolved tool executors from disk", rehydrated);
        }
    }

    let llm_timeout_secs = 300u64;
    let mut core_evo = CoreEvolution::new(
        paths.workspace().to_path_buf(),
        cap_registry_raw.clone(),
        llm_timeout_secs,
    );
    if let Ok(evo_provider) = super::provider::create_provider(&config) {
        let llm_bridge = Arc::new(ProviderLLMBridge::new(evo_provider));
        core_evo.set_llm_provider(llm_bridge);
        info!("Core evolution LLM provider configured");
    }
    let core_evo_raw = Arc::new(Mutex::new(core_evo));

    let cap_registry_adapter = CapabilityRegistryAdapter::new(cap_registry_raw.clone());
    let cap_registry_handle: CapabilityRegistryHandle = Arc::new(Mutex::new(cap_registry_adapter));

    let core_evo_adapter = CoreEvolutionAdapter::new(core_evo_raw.clone());
    let core_evo_handle: CoreEvolutionHandle = Arc::new(Mutex::new(core_evo_adapter));

    // ── Create message bus ──
    let bus = MessageBus::new(100);
    let ((inbound_tx, inbound_rx), (outbound_tx, outbound_rx)) = bus.split();

    // ── Create WebSocket broadcast channel ──
    let (ws_broadcast_tx, _) = broadcast::channel::<String>(1000);

    // ── Create shutdown channel ──
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // ── Create shared task manager ──
    let task_manager = TaskManager::new();

    // ── Create tool registry (shared for listing tools) ──
    let tool_registry = ToolRegistry::with_defaults();
    let tool_registry_shared = Arc::new(tool_registry.clone());

    // ── Create agent runtime with full component wiring ──
    let mut runtime = AgentRuntime::new(
        config.clone(),
        paths.clone(),
        std::sync::Arc::clone(&provider_pool),
        tool_registry,
    )?;
    runtime.mount_mcp_servers().await;

    // 如果配置了独立的 evolution_model 或 evolution_provider，创建独立的 evolution provider
    if config.agents.defaults.evolution_model.is_some()
        || config.agents.defaults.evolution_provider.is_some()
    {
        match super::provider::create_evolution_provider(&config) {
            Ok(evo_provider) => {
                runtime.set_evolution_provider(evo_provider);
                info!("Evolution provider configured with independent model");
            }
            Err(e) => {
                warn!(
                    "Failed to create evolution provider: {}, using main provider",
                    e
                );
            }
        }
    }

    // ── Set up path confirmation channel (channel-aware) ──
    // pending_ws_confirms: keyed by request_id, for WebUI (ws) confirmations
    let pending_ws_confirms: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    // pending_channel_confirms: keyed by "channel:chat_id", for non-ws channel confirmations
    let pending_channel_confirms: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let (confirm_tx, mut confirm_rx) = mpsc::channel::<ConfirmRequest>(16);
    runtime.set_confirm(confirm_tx);

    // Clone outbound_tx before it is moved into the runtime, so the confirm
    // handler can send confirmation prompts to non-ws channels.
    let outbound_tx_for_confirm = outbound_tx.clone();

    // Spawn confirm handler: routes confirmation requests to the correct channel.
    // - ws channel → broadcast confirm_request event to WebUI
    // - non-ws channels → send text prompt via outbound_tx to originating channel
    let pending_ws_for_handler = Arc::clone(&pending_ws_confirms);
    let pending_ch_for_handler = Arc::clone(&pending_channel_confirms);
    let ws_broadcast_for_confirm = ws_broadcast_tx.clone();
    tokio::spawn(async move {
        while let Some(req) = confirm_rx.recv().await {
            if req.channel == "ws" {
                // WebUI: broadcast structured event, wait for confirm_response WS message
                let request_id = format!("confirm_{}", chrono::Utc::now().timestamp_millis());
                {
                    let mut map = pending_ws_for_handler.lock().await;
                    map.insert(request_id.clone(), req.response_tx);
                }
                let event = serde_json::json!({
                    "type": "confirm_request",
                    "request_id": request_id,
                    "tool": req.tool_name,
                    "paths": req.paths,
                });
                let _ = ws_broadcast_for_confirm.send(event.to_string());
                info!(request_id = %request_id, tool = %req.tool_name, "Sent confirm_request to WebUI");
            } else {
                // Non-ws channel (lark, telegram, etc.): send text prompt to the
                // originating channel and wait for the user's text reply.
                let confirm_key = format!("{}:{}", req.channel, req.chat_id);
                let paths_display: Vec<String> =
                    req.paths.iter().map(|p| format!("  📁 {}", p)).collect();
                let prompt = format!(
                    "⚠️ 安全确认: 工具 `{}` 需要访问工作区外的路径:\n{}\n\n回复 \"允许\" 或 \"y\" 授权，回复其他内容拒绝。",
                    req.tool_name,
                    paths_display.join("\n"),
                );
                {
                    let mut map = pending_ch_for_handler.lock().await;
                    map.insert(confirm_key.clone(), req.response_tx);
                }
                let outbound_msg = OutboundMessage::new(&req.channel, &req.chat_id, &prompt);
                if let Err(e) = outbound_tx_for_confirm.send(outbound_msg).await {
                    warn!(confirm_key = %confirm_key, error = %e, "Failed to send confirm prompt to channel");
                    // Remove the pending entry since we couldn't send
                    let mut map = pending_ch_for_handler.lock().await;
                    if let Some(tx) = map.remove(&confirm_key) {
                        let _ = tx.send(false);
                    }
                } else {
                    info!(confirm_key = %confirm_key, tool = %req.tool_name, "Sent confirm_request to channel");
                }
            }
        }
    });

    runtime.set_outbound(outbound_tx);
    runtime.set_task_manager(task_manager.clone());
    if let Some(ref store) = memory_store_handle {
        runtime.set_memory_store(store.clone());
    }
    runtime.set_capability_registry(cap_registry_handle.clone());
    runtime.set_core_evolution(core_evo_handle.clone());
    runtime.set_event_tx(ws_broadcast_tx.clone());

    // ── Create channel manager for outbound dispatch ──
    let channel_manager = ChannelManager::new(config.clone(), paths.clone(), inbound_tx.clone());

    // ── Create session store ──
    let session_store = Arc::new(SessionStore::new(paths.clone()));

    // ── Create scheduler services ──
    let cron_service = Arc::new(CronService::new(paths.clone(), inbound_tx.clone()));
    cron_service.load().await?;

    let heartbeat_service = Arc::new(HeartbeatService::new(paths.clone(), inbound_tx.clone()));

    // Optional: register this gateway with the configured community hub.
    // This runs in the background and does not block gateway startup.
    if let Some(hub_url) = config.community_hub_url() {
        let client = reqwest::Client::new();
        let register_url = format!("{}/v1/nodes/heartbeat", hub_url.trim_end_matches('/'));
        let api_key = config.community_hub_api_key();
        let version = env!("CARGO_PKG_VERSION").to_string();
        let public_url = if host != "0.0.0.0" {
            Some(format!("http://{}:{}", host, port))
        } else {
            None
        };
        let node_alias = config.community_hub.node_alias.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(240));
            loop {
                interval.tick().await;

                let body = serde_json::json!({
                    "name": node_alias,
                    "version": version,
                    "public_url": public_url,
                    "tags": ["gateway", "cli"],
                    "skills": [],
                });

                let mut req = client.post(&register_url).json(&body);
                if let Some(key) = &api_key {
                    req = req.header("Authorization", format!("Bearer {}", key));
                }

                if let Err(e) = req.send().await {
                    warn!("Failed to send heartbeat to hub: {}", e);
                } else {
                    debug!("Sent heartbeat to hub");
                }
            }
        });
    }

    // ── Create Ghost Agent service ──
    let ghost_config = GhostServiceConfig::from_config(&config);
    let ghost_service = GhostService::new(ghost_config, paths.clone(), inbound_tx.clone());

    // ── Inbound interceptor: check for pending channel confirm replies ──
    // Sits between channel inbound_rx and the runtime, intercepting confirm
    // replies from non-ws channels before they reach the runtime loop.
    let (filtered_inbound_tx, filtered_inbound_rx) = mpsc::channel::<InboundMessage>(100);
    let pending_ch_for_interceptor = Arc::clone(&pending_channel_confirms);
    let mut interceptor_shutdown_rx = shutdown_tx.subscribe();
    let interceptor_handle = tokio::spawn(async move {
        let mut inbound_rx = inbound_rx;
        loop {
            let msg = tokio::select! {
                msg = inbound_rx.recv() => match msg {
                    Some(m) => m,
                    None => break,
                },
                _ = interceptor_shutdown_rx.recv() => break,
            };
            // Check if this message is a reply to a pending channel confirm
            if msg.channel != "ws"
                && msg.channel != "cli"
                && msg.channel != "cron"
                && msg.channel != "system"
            {
                let confirm_key = format!("{}:{}", msg.channel, msg.chat_id);
                let maybe_tx = {
                    let mut map = pending_ch_for_interceptor.lock().await;
                    map.remove(&confirm_key)
                };
                if let Some(tx) = maybe_tx {
                    // Parse the reply as a confirm response
                    let text = msg.content.trim().to_lowercase();
                    let approved = text == "y"
                        || text == "yes"
                        || text.contains("允许")
                        || text.contains("确认")
                        || text.contains("同意")
                        || text.contains("ok");
                    info!(
                        confirm_key = %confirm_key,
                        approved = approved,
                        reply = %msg.content.trim(),
                        "Channel confirm reply intercepted"
                    );
                    let _ = tx.send(approved);
                    continue; // Don't forward this message to the runtime
                }
            }
            // Not a confirm reply — forward to runtime
            if filtered_inbound_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    // ── Spawn core tasks ──
    let runtime_shutdown_rx = shutdown_tx.subscribe();
    let runtime_handle = tokio::spawn(async move {
        runtime
            .run_loop(filtered_inbound_rx, Some(runtime_shutdown_rx))
            .await;
    });

    // Wrap channel_manager in Arc so it can be shared between the outbound bridge and gateway state
    let channel_manager = Arc::new(channel_manager);

    // Outbound → WS broadcast bridge + external channel dispatch
    let ws_broadcast_for_bridge = ws_broadcast_tx.clone();
    let outbound_shutdown_rx = shutdown_tx.subscribe();
    let channel_manager_for_bridge = Arc::clone(&channel_manager);
    let outbound_handle = tokio::spawn(async move {
        outbound_to_ws_bridge(
            outbound_rx,
            ws_broadcast_for_bridge,
            channel_manager_for_bridge,
            outbound_shutdown_rx,
        )
        .await;
    });

    let cron_handle = {
        let cron = cron_service.clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            cron.run_loop(shutdown_rx).await;
        })
    };

    let heartbeat_handle = {
        let heartbeat = heartbeat_service.clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            heartbeat.run_loop(shutdown_rx).await;
        })
    };

    let ghost_handle = {
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            ghost_service.run_loop(shutdown_rx).await;
        })
    };

    // ── Start messaging channels ──
    #[cfg(feature = "telegram")]
    let telegram_handle = {
        let telegram = Arc::new(TelegramChannel::new(config.clone(), inbound_tx.clone()));
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            telegram.run_loop(shutdown_rx).await;
        })
    };

    #[cfg(feature = "whatsapp")]
    let whatsapp_handle = {
        let whatsapp = Arc::new(WhatsAppChannel::new(config.clone(), inbound_tx.clone()));
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            whatsapp.run_loop(shutdown_rx).await;
        })
    };

    #[cfg(feature = "feishu")]
    let feishu_handle = {
        let feishu = Arc::new(FeishuChannel::new(config.clone(), inbound_tx.clone()));
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            feishu.run_loop(shutdown_rx).await;
        })
    };

    #[cfg(feature = "slack")]
    let slack_handle = {
        let slack = Arc::new(SlackChannel::new(config.clone(), inbound_tx.clone()));
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            slack.run_loop(shutdown_rx).await;
        })
    };

    #[cfg(feature = "discord")]
    let discord_handle = {
        let discord = Arc::new(DiscordChannel::new(config.clone(), inbound_tx.clone()));
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            discord.run_loop(shutdown_rx).await;
        })
    };

    #[cfg(feature = "dingtalk")]
    let dingtalk_handle = {
        let dingtalk = Arc::new(DingTalkChannel::new(config.clone(), inbound_tx.clone()));
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            dingtalk.run_loop(shutdown_rx).await;
        })
    };

    #[cfg(feature = "wecom")]
    let wecom_handle = {
        let wecom = Arc::new(WeComChannel::new(config.clone(), inbound_tx.clone()));
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            wecom.run_loop(shutdown_rx).await;
        })
    };

    // ── Build HTTP/WebSocket server ──
    // Guarantee api_token is Some and non-empty — defensive fallback in case auto-gen above
    // somehow produced None or empty (e.g. env var was whitespace-only).
    if config
        .gateway
        .api_token
        .as_deref()
        .map(|t| t.trim().is_empty())
        .unwrap_or(true)
    {
        let raw = format!(
            "{}{}{}{}",
            uuid::Uuid::new_v4().to_string().replace('-', ""),
            uuid::Uuid::new_v4().to_string().replace('-', ""),
            uuid::Uuid::new_v4().to_string().replace('-', ""),
            uuid::Uuid::new_v4().to_string().replace('-', ""),
        );
        let fallback = format!("bc_{}", &raw[..61]);
        warn!("api_token was missing/empty before building GatewayState; using in-memory fallback");
        config.gateway.api_token = Some(fallback);
    }
    let api_token = config.gateway.api_token.clone();

    // Determine WebUI login password:
    // - If gateway.webuiPass is set in config → use it (stable across restarts)
    // - Otherwise → generate a random temp password printed at startup (NOT saved)
    let (web_password, webui_pass_is_temp) = match &config.gateway.webui_pass {
        Some(p) if !p.is_empty() => (p.clone(), false),
        _ => {
            let tmp = format!("{:08x}", rand_u32());
            (tmp, true)
        }
    };

    let is_exposed = host == "0.0.0.0" || host == "::";

    // Create a shared EvolutionService for the HTTP handlers (trigger, delete, status).
    // This is separate from the one inside AgentRuntime but shares the same disk records.
    let shared_evo_service = Arc::new(Mutex::new(EvolutionService::new(
        paths.skills_dir(),
        EvolutionServiceConfig::default(),
    )));

    let gateway_state = GatewayState {
        inbound_tx: inbound_tx.clone(),
        task_manager,
        config: config.clone(),
        paths: paths.clone(),
        api_token: api_token.clone(),
        ws_broadcast: ws_broadcast_tx.clone(),
        pending_confirms: Arc::clone(&pending_ws_confirms),
        pending_channel_confirms: Arc::clone(&pending_channel_confirms),
        session_store,
        cron_service: cron_service.clone(),
        memory_store: memory_store_handle.clone(),
        tool_registry: tool_registry_shared,
        web_password: web_password.clone(),
        channel_manager: Arc::clone(&channel_manager),
        evolution_service: shared_evo_service,
    };

    let app = Router::new()
        // Auth
        .route("/v1/auth/login", post(handle_login))
        // P0: Core
        .route("/v1/chat", post(handle_chat))
        .route("/v1/health", get(handle_health))
        .route("/v1/tasks", get(handle_tasks))
        .route("/v1/ws", get(handle_ws_upgrade))
        // P0: Sessions
        .route("/v1/sessions", get(handle_sessions_list))
        .route(
            "/v1/sessions/:id",
            get(handle_session_get).delete(handle_session_delete),
        )
        .route("/v1/sessions/:id/rename", put(handle_session_rename))
        // P1: Config
        .route(
            "/v1/config",
            get(handle_config_get).put(handle_config_update),
        )
        .route("/v1/config/reload", post(handle_config_reload))
        .route(
            "/v1/config/test-provider",
            post(handle_config_test_provider),
        )
        // Ghost Agent
        .route(
            "/v1/ghost/config",
            get(handle_ghost_config_get).put(handle_ghost_config_update),
        )
        .route("/v1/ghost/activity", get(handle_ghost_activity))
        .route(
            "/v1/ghost/model-options",
            get(handle_ghost_model_options_get),
        )
        // P1: Memory
        .route(
            "/v1/memory",
            get(handle_memory_list).post(handle_memory_create),
        )
        .route("/v1/memory/stats", get(handle_memory_stats))
        .route("/v1/memory/:id", delete(handle_memory_delete))
        // P1: Tools / Skills / Evolution / Stats
        .route("/v1/tools", get(handle_tools))
        .route("/v1/skills", get(handle_skills))
        .route("/v1/skills/search", post(handle_skills_search))
        .route("/v1/evolution", get(handle_evolution))
        .route(
            "/v1/evolution/tool-evolutions",
            get(handle_evolution_tool_evolutions),
        )
        .route("/v1/evolution/summary", get(handle_evolution_summary))
        .route("/v1/evolution/trigger", post(handle_evolution_trigger))
        .route("/v1/evolution/test", post(handle_evolution_test))
        .route(
            "/v1/evolution/test-suggest",
            post(handle_evolution_test_suggest),
        )
        .route(
            "/v1/evolution/versions/:skill",
            get(handle_evolution_versions),
        )
        .route(
            "/v1/evolution/tool-versions/:id",
            get(handle_evolution_tool_versions),
        )
        .route(
            "/v1/evolution/:id",
            get(handle_evolution_detail).delete(handle_evolution_delete),
        )
        .route("/v1/channels/status", get(handle_channels_status))
        .route("/v1/channels", get(handle_channels_list))
        .route("/v1/channels/:id", put(handle_channel_update))
        .route("/v1/skills/:name", delete(handle_skill_delete))
        .route("/v1/hub/skills", get(handle_hub_skills))
        .route(
            "/v1/hub/skills/:name/install",
            post(handle_hub_skill_install),
        )
        .route(
            "/v1/skills/install-external",
            post(handle_skill_install_external),
        )
        .route("/v1/stats", get(handle_stats))
        // P1: Cron
        .route("/v1/cron", get(handle_cron_list).post(handle_cron_create))
        .route("/v1/cron/:id", delete(handle_cron_delete))
        .route("/v1/cron/:id/run", post(handle_cron_run))
        // Toggles
        .route(
            "/v1/toggles",
            get(handle_toggles_get).put(handle_toggles_update),
        )
        // P2: Alerts
        .route(
            "/v1/alerts",
            get(handle_alerts_list).post(handle_alerts_create),
        )
        .route("/v1/alerts/history", get(handle_alerts_history))
        .route(
            "/v1/alerts/:id",
            put(handle_alerts_update).delete(handle_alerts_delete),
        )
        // P2: Streams
        .route("/v1/streams", get(handle_streams_list))
        .route("/v1/streams/:id/data", get(handle_stream_data))
        // Persona files (AGENTS.md, SOUL.md, USER.md, etc.)
        .route("/v1/persona/files", get(handle_persona_list))
        .route(
            "/v1/persona/file",
            get(handle_persona_read).put(handle_persona_write),
        )
        // Pool status
        .route("/v1/pool/status", get(handle_pool_status))
        // P2: Files
        .route("/v1/files", get(handle_files_list))
        .route("/v1/files/content", get(handle_files_content))
        .route("/v1/files/download", get(handle_files_download))
        .route("/v1/files/serve", get(handle_files_serve))
        .route("/v1/files/upload", post(handle_files_upload))
        .layer(middleware::from_fn_with_state(
            gateway_state.clone(),
            auth_middleware,
        ))
        .layer(build_api_cors_layer(&config))
        // Webhook endpoints — public (no auth), must be outside auth middleware
        .route("/webhook/lark", post(handle_lark_webhook))
        .route(
            "/webhook/wecom",
            get(handle_wecom_webhook).post(handle_wecom_webhook),
        )
        .with_state(gateway_state);

    let bind_addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    let http_shutdown_rx = shutdown_tx.subscribe();
    let http_handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let mut rx = http_shutdown_rx;
                let _ = rx.recv().await;
            })
            .await
            .ok();
    });

    // ── WebUI static file server (embedded via rust-embed) ──
    let webui_host = config.gateway.webui_host.clone();
    let webui_port = config.gateway.webui_port;
    let webui_bind = format!("{}:{}", webui_host, webui_port);
    let webui_config = config.clone();
    let webui_app = Router::new()
        .route(
            "/env.js",
            get(move || {
                let cfg = webui_config.clone();
                async move { handle_webui_env_js(cfg).await }
            }),
        )
        .fallback(handle_webui_static)
        .layer(build_webui_cors_layer(&config));
    let webui_listener = tokio::net::TcpListener::bind(&webui_bind).await?;
    let webui_shutdown_rx = shutdown_tx.subscribe();
    let webui_handle = tokio::spawn(async move {
        axum::serve(webui_listener, webui_app)
            .with_graceful_shutdown(async move {
                let mut rx = webui_shutdown_rx;
                let _ = rx.recv().await;
            })
            .await
            .ok();
    });

    // ── Print beautiful startup banner ──
    print_startup_banner(
        &config,
        &host,
        &webui_host,
        webui_port,
        &web_password,
        webui_pass_is_temp,
        is_exposed,
        &bind_addr,
    );

    // ── Wait for shutdown signal ──
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received, draining tasks...");

    let _ = shutdown_tx.send(());
    drop(inbound_tx);
    // Drop local services that still hold inbound_tx clones so runtime can observe
    // channel closure and exit promptly.
    drop(cron_service);
    drop(heartbeat_service);

    let mut handles: Vec<(&str, tokio::task::JoinHandle<()>)> = vec![
        ("http_server", http_handle),
        ("webui_server", webui_handle),
        ("runtime", runtime_handle),
        ("outbound", outbound_handle),
        ("interceptor", interceptor_handle),
        ("cron", cron_handle),
        ("heartbeat", heartbeat_handle),
        ("ghost", ghost_handle),
    ];

    #[cfg(feature = "telegram")]
    handles.push(("telegram", telegram_handle));

    #[cfg(feature = "whatsapp")]
    handles.push(("whatsapp", whatsapp_handle));

    #[cfg(feature = "feishu")]
    handles.push(("feishu", feishu_handle));

    #[cfg(feature = "slack")]
    handles.push(("slack", slack_handle));

    #[cfg(feature = "discord")]
    handles.push(("discord", discord_handle));

    #[cfg(feature = "dingtalk")]
    handles.push(("dingtalk", dingtalk_handle));

    #[cfg(feature = "wecom")]
    handles.push(("wecom", wecom_handle));

    let total = handles.len();
    let graceful_timeout = std::time::Duration::from_secs(30);
    let deadline = tokio::time::Instant::now() + graceful_timeout;

    // Wait briefly for graceful shutdown.
    loop {
        if handles.iter().all(|(_, h)| h.is_finished()) {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Force-stop any stragglers so Ctrl+C returns quickly.
    let mut aborted = 0;
    for (name, handle) in &handles {
        if !handle.is_finished() {
            warn!(
                task = *name,
                "Task did not exit in graceful window, aborting"
            );
            handle.abort();
            aborted += 1;
        }
    }

    let mut failed = 0;
    for (name, handle) in handles {
        match handle.await {
            Ok(()) => {}
            Err(e) if e.is_cancelled() => {
                debug!(task = name, "Task cancelled during shutdown");
            }
            Err(e) => {
                error!(task = name, error = %e, "Task panicked during shutdown");
                failed += 1;
            }
        }
    }

    if failed == 0 {
        info!(total, aborted, "Gateway shutdown complete");
    } else {
        warn!(
            failed,
            total, aborted, "Gateway shutdown completed with task failures"
        );
    }

    info!("Gateway stopped");
    Ok(())
}

fn build_api_cors_layer(config: &Config) -> CorsLayer {
    let _ = config;
    CorsLayer::permissive().allow_credentials(false)
}

fn build_webui_cors_layer(config: &Config) -> CorsLayer {
    let _ = config;
    CorsLayer::permissive().allow_credentials(false)
}
