use super::*;
// ---------------------------------------------------------------------------
// P1: Config management endpoints
// ---------------------------------------------------------------------------

/// GET /v1/config — get config (returns plaintext API keys)
/// Always reads from disk so edits via PUT are immediately reflected.
pub(super) async fn handle_config_get(State(state): State<GatewayState>) -> impl IntoResponse {
    let config_path = state.paths.config_file();
    let config_val = match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => serde_json::from_str::<serde_json::Value>(&content).unwrap_or_default(),
        Err(_) => serde_json::to_value(&state.config).unwrap_or_default(),
    };
    Json(config_val)
}

#[derive(Deserialize)]
pub(super) struct ConfigUpdateRequest {
    #[serde(flatten)]
    config: serde_json::Value,
}

/// PUT /v1/config — update config
pub(super) async fn handle_config_update(
    State(state): State<GatewayState>,
    Json(req): Json<ConfigUpdateRequest>,
) -> impl IntoResponse {
    let config_path = state.paths.config_file();

    match serde_json::from_value::<Config>(req.config) {
        Ok(new_config) => match new_config.save(&config_path) {
            Ok(_) => Json(
                serde_json::json!({ "status": "ok", "message": "Config updated. Restart gateway to apply changes." }),
            ),
            Err(e) => Json(serde_json::json!({ "status": "error", "message": format!("{}", e) })),
        },
        Err(e) => Json(
            serde_json::json!({ "status": "error", "message": format!("Invalid config: {}", e) }),
        ),
    }
}

/// POST /v1/config/reload — reload config from disk (validates JSON format)
pub(super) async fn handle_config_reload(State(state): State<GatewayState>) -> impl IntoResponse {
    let config_path = state.paths.config_file();

    // 读取并验证配置文件
    match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => {
            // 验证JSON格式
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json_val) => {
                    // 验证配置结构
                    match serde_json::from_value::<Config>(json_val) {
                        Ok(_) => Json(serde_json::json!({
                            "status": "ok",
                            "message": "Config validated successfully. Note: Full reload requires gateway restart for some settings."
                        })),
                        Err(e) => Json(serde_json::json!({
                            "status": "error",
                            "message": format!("Invalid config structure: {}", e)
                        })),
                    }
                }
                Err(e) => Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Invalid JSON format: {}", e)
                })),
            }
        }
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to read config file: {}", e)
        })),
    }
}

/// POST /v1/config/test-provider — test a provider connection
pub(super) async fn handle_config_test_provider(
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let model = req
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-3.5-turbo");
    let api_key = req.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    let api_base = req.get("api_base").and_then(|v| v.as_str());
    let proxy = req.get("proxy").and_then(|v| v.as_str());

    if api_key.is_empty() {
        return Json(serde_json::json!({ "status": "error", "message": "api_key is required" }));
    }

    // Try a simple completion to test the connection
    // The WebUI sends the correct api_base (from form input with defaultBase fallback).
    let provider = blockcell_providers::OpenAIProvider::new_with_proxy(
        api_key,
        api_base,
        model,
        100,
        0.0,
        proxy,
        None,
        &[],
    );

    use blockcell_providers::Provider;
    let test_messages = vec![blockcell_core::types::ChatMessage::user("Say 'ok'")];
    match provider.chat(&test_messages, &[]).await {
        Ok(_) => {
            Json(serde_json::json!({ "status": "ok", "message": "Provider connection successful" }))
        }
        Err(e) => Json(serde_json::json!({ "status": "error", "message": format!("{}", e) })),
    }
}
/// GET /v1/ghost/config — get ghost agent configuration
pub(super) async fn handle_ghost_config_get(
    State(state): State<GatewayState>,
) -> impl IntoResponse {
    // Read from disk each time so updates via PUT take effect immediately
    // without requiring a gateway restart.
    let config_path = state.paths.config_file();
    let ghost = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .map(|c| c.agents.ghost)
        .unwrap_or_else(|| state.config.agents.ghost.clone());

    // GhostConfig has #[serde(rename_all = "camelCase")], so this serialization
    // automatically handles maxSyncsPerDay and autoSocial keys correctly.
    Json(ghost)
}

/// PUT /v1/ghost/config — update ghost agent configuration
pub(super) async fn handle_ghost_config_update(
    State(state): State<GatewayState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let config_path = state.paths.config_file();
    let mut config: Config = match std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(c) => c,
        None => state.config.clone(),
    };

    if let Some(v) = req.get("enabled").and_then(|v| v.as_bool()) {
        config.agents.ghost.enabled = v;
    }
    if let Some(v) = req.get("model") {
        if v.is_null() {
            config.agents.ghost.model = None;
        } else {
            config.agents.ghost.model = v.as_str().map(|s| s.to_string());
        }
    }
    if let Some(v) = req.get("schedule").and_then(|v| v.as_str()) {
        config.agents.ghost.schedule = v.to_string();
    }
    if let Some(v) = req.get("maxSyncsPerDay").and_then(|v| v.as_u64()) {
        config.agents.ghost.max_syncs_per_day = v as u32;
    }
    if let Some(v) = req.get("autoSocial").and_then(|v| v.as_bool()) {
        config.agents.ghost.auto_social = v;
    }

    match config.save(&config_path) {
        Ok(_) => Json(serde_json::json!({
            "status": "ok",
            "message": "Ghost config updated. Changes take effect on next cycle.",
            "config": config.agents.ghost,
        })),
        Err(e) => Json(serde_json::json!({ "status": "error", "message": format!("{}", e) })),
    }
}

/// GET /v1/ghost/activity — get ghost agent activity log from session files
pub(super) async fn handle_ghost_activity(
    State(state): State<GatewayState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let sessions_dir = state.paths.sessions_dir();
    let limit: usize = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let mut activities: Vec<serde_json::Value> = Vec::new();

    // Scan session files for ghost sessions (chat_id starts with "ghost_")
    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        let mut ghost_files: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("ghost_") && n.ends_with(".jsonl"))
                    .unwrap_or(false)
            })
            .collect();

        // Sort by modification time, newest first
        ghost_files.sort_by(|a, b| {
            let ta = a.metadata().and_then(|m| m.modified()).ok();
            let tb = b.metadata().and_then(|m| m.modified()).ok();
            tb.cmp(&ta)
        });

        for entry in ghost_files.into_iter().take(limit) {
            let path = entry.path();
            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            if let Ok(content) = std::fs::read_to_string(&path) {
                let lines: Vec<&str> = content.lines().collect();
                let message_count = lines.len();

                // Extract timestamp from session_id (ghost_YYYYMMDD_HHMMSS)
                // and normalize to "YYYY-MM-DD HH:MM" for display.
                let raw_ts = session_id
                    .strip_prefix("ghost_")
                    .unwrap_or(&session_id)
                    .to_string();
                let timestamp = chrono::NaiveDateTime::parse_from_str(&raw_ts, "%Y%m%d_%H%M%S")
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or(raw_ts);

                // Get first user message (the routine prompt) and last assistant message (summary)
                let mut routine_prompt = String::new();
                let mut summary = String::new();
                let mut tool_calls: Vec<String> = Vec::new();

                for line in &lines {
                    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
                        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                        match role {
                            "user" if routine_prompt.is_empty() => {
                                routine_prompt = msg
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .chars()
                                    .take(200)
                                    .collect();
                            }
                            "assistant" => {
                                if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                                    summary = content.chars().take(500).collect();
                                }
                                if let Some(calls) =
                                    msg.get("tool_calls").and_then(|v| v.as_array())
                                {
                                    for call in calls {
                                        if let Some(name) = call
                                            .get("function")
                                            .and_then(|f| f.get("name"))
                                            .and_then(|n| n.as_str())
                                        {
                                            tool_calls.push(name.to_string());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                activities.push(serde_json::json!({
                    "session_id": session_id,
                    "timestamp": timestamp,
                    "message_count": message_count,
                    "routine_prompt": routine_prompt,
                    "summary": summary,
                    "tool_calls": tool_calls,
                }));
            }
        }
    }

    let count = activities.len();
    Json(serde_json::json!({
        "activities": activities,
        "count": count,
    }))
}

pub(super) async fn handle_ghost_model_options_get(
    State(state): State<GatewayState>,
) -> impl IntoResponse {
    let config_path = state.paths.config_file();
    let config: Config = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| state.config.clone());

    let mut providers: Vec<String> = config
        .providers
        .iter()
        .filter_map(|(name, p)| {
            if p.api_key.trim().is_empty() {
                None
            } else {
                Some(name.clone())
            }
        })
        .collect();
    providers.sort();
    let (default_model, _, _) = active_model_and_provider(&config);

    Json(serde_json::json!({
        "providers": providers,
        "default_model": default_model,
    }))
}
