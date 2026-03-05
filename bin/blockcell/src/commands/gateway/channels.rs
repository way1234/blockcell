use super::*;
// ---------------------------------------------------------------------------
// Channels status endpoint
// ---------------------------------------------------------------------------

/// GET /v1/channels/status — connection status for all configured channels
pub(super) async fn handle_channels_status(State(state): State<GatewayState>) -> impl IntoResponse {
    let statuses = state.channel_manager.get_status();
    let channels: Vec<serde_json::Value> = statuses
        .into_iter()
        .map(|(name, active, detail)| {
            serde_json::json!({
                "name": name,
                "active": active,
                "detail": detail,
            })
        })
        .collect();
    Json(serde_json::json!({ "channels": channels }))
}

// ---------------------------------------------------------------------------
// Channels list — all 8 supported channels with config status
// ---------------------------------------------------------------------------

/// GET /v1/channels — list all 8 supported channels with their configuration status
pub(super) async fn handle_channels_list(State(state): State<GatewayState>) -> impl IntoResponse {
    // Read from disk each time so updates via PUT take effect immediately
    // without requiring a gateway restart.
    let config_path = state.paths.config_file();
    let cfg = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .map(|c| c.channels)
        .unwrap_or_else(|| state.config.channels.clone());

    let channels = serde_json::json!([
        {
            "id": "telegram",
            "name": "Telegram",
            "icon": "telegram",
            "doc": "docs/channels/zh/01_telegram.md",
            "configured": cfg.telegram.enabled && !cfg.telegram.token.is_empty(),
            "enabled": cfg.telegram.enabled,
            "fields": [
                {"key": "token", "label": "Bot Token", "secret": true, "value": cfg.telegram.token.clone()},
                {"key": "proxy", "label": "Proxy (可选, 如 socks5://127.0.0.1:7890)", "secret": false, "value": cfg.telegram.proxy.clone().unwrap_or_default()}
            ]
        },
        {
            "id": "discord",
            "name": "Discord",
            "icon": "discord",
            "doc": "docs/channels/zh/02_discord.md",
            "configured": cfg.discord.enabled && !cfg.discord.bot_token.is_empty(),
            "enabled": cfg.discord.enabled,
            "fields": [
                {"key": "botToken", "label": "Bot Token", "secret": true, "value": cfg.discord.bot_token.clone()},
                {"key": "channels", "label": "Channel IDs (逗号分隔)", "secret": false, "value": cfg.discord.channels.join(",")}
            ]
        },
        {
            "id": "slack",
            "name": "Slack",
            "icon": "slack",
            "doc": "docs/channels/zh/03_slack.md",
            "configured": cfg.slack.enabled && !cfg.slack.bot_token.is_empty(),
            "enabled": cfg.slack.enabled,
            "fields": [
                {"key": "botToken", "label": "Bot Token", "secret": true, "value": cfg.slack.bot_token.clone()},
                {"key": "appToken", "label": "App Token", "secret": true, "value": cfg.slack.app_token.clone()},
                {"key": "channels", "label": "Channel IDs (逗号分隔)", "secret": false, "value": cfg.slack.channels.join(",")},
                {"key": "pollIntervalSecs", "label": "轮询间隔 (秒)", "secret": false, "value": cfg.slack.poll_interval_secs.to_string()}
            ]
        },
        {
            "id": "feishu",
            "name": "飞书",
            "icon": "feishu",
            "doc": "docs/channels/zh/04_feishu.md",
            "configured": cfg.feishu.enabled && !cfg.feishu.app_id.is_empty(),
            "enabled": cfg.feishu.enabled,
            "fields": [
                {"key": "appId", "label": "App ID", "secret": false, "value": cfg.feishu.app_id.clone()},
                {"key": "appSecret", "label": "App Secret", "secret": true, "value": cfg.feishu.app_secret.clone()},
                {"key": "encryptKey", "label": "Encrypt Key (事件加密密钥)", "secret": true, "value": cfg.feishu.encrypt_key.clone()},
                {"key": "verificationToken", "label": "Verification Token (事件验证Token)", "secret": true, "value": cfg.feishu.verification_token.clone()}
            ]
        },
        {
            "id": "dingtalk",
            "name": "钉钉",
            "icon": "dingtalk",
            "doc": "docs/channels/zh/05_dingtalk.md",
            "configured": cfg.dingtalk.enabled && !cfg.dingtalk.app_key.is_empty(),
            "enabled": cfg.dingtalk.enabled,
            "fields": [
                {"key": "appKey", "label": "App Key", "secret": false, "value": cfg.dingtalk.app_key.clone()},
                {"key": "appSecret", "label": "App Secret", "secret": true, "value": cfg.dingtalk.app_secret.clone()},
                {"key": "robotCode", "label": "Robot Code (机器人编码, 用于主动发消息)", "secret": false, "value": cfg.dingtalk.robot_code.clone()}
            ]
        },
        {
            "id": "wecom",
            "name": "企业微信",
            "icon": "wecom",
            "doc": "docs/channels/zh/06_wecom.md",
            "configured": cfg.wecom.enabled && !cfg.wecom.corp_id.is_empty(),
            "enabled": cfg.wecom.enabled,
            "fields": [
                {"key": "corpId", "label": "Corp ID", "secret": false, "value": cfg.wecom.corp_id.clone()},
                {"key": "corpSecret", "label": "Corp Secret", "secret": true, "value": cfg.wecom.corp_secret.clone()},
                {"key": "agentId", "label": "Agent ID", "secret": false, "value": cfg.wecom.agent_id.to_string()},
                {"key": "callbackToken", "label": "Callback Token (回调Token, 可选)", "secret": true, "value": cfg.wecom.callback_token.clone()},
                {"key": "encodingAesKey", "label": "EncodingAESKey (消息加解密密钥, 可选)", "secret": true, "value": cfg.wecom.encoding_aes_key.clone()},
                {"key": "pollIntervalSecs", "label": "轮询间隔 (秒)", "secret": false, "value": cfg.wecom.poll_interval_secs.to_string()}
            ]
        },
        {
            "id": "whatsapp",
            "name": "WhatsApp",
            "icon": "whatsapp",
            "doc": "docs/channels/zh/07_whatsapp.md",
            "configured": cfg.whatsapp.enabled && !cfg.whatsapp.bridge_url.is_empty(),
            "enabled": cfg.whatsapp.enabled,
            "fields": [
                {"key": "bridgeUrl", "label": "Bridge URL", "secret": false, "value": cfg.whatsapp.bridge_url.clone()}
            ]
        },
        {
            "id": "lark",
            "name": "Lark (飞书国际版)",
            "icon": "lark",
            "doc": "docs/channels/zh/08_lark.md",
            "configured": cfg.lark.enabled && !cfg.lark.app_id.is_empty(),
            "enabled": cfg.lark.enabled,
            "fields": [
                {"key": "appId", "label": "App ID", "secret": false, "value": cfg.lark.app_id.clone()},
                {"key": "appSecret", "label": "App Secret", "secret": true, "value": cfg.lark.app_secret.clone()},
                {"key": "encryptKey", "label": "Encrypt Key (Event encryption key)", "secret": true, "value": cfg.lark.encrypt_key.clone()},
                {"key": "verificationToken", "label": "Verification Token (Event verification)", "secret": true, "value": cfg.lark.verification_token.clone()}
            ]
        }
    ]);
    Json(serde_json::json!({ "channels": channels }))
}

/// PUT /v1/channels/:id — update channel config fields
#[derive(Deserialize)]
pub(super) struct ChannelUpdateRequest {
    fields: serde_json::Map<String, serde_json::Value>,
    enabled: Option<bool>,
}

pub(super) async fn handle_channel_update(
    State(state): State<GatewayState>,
    AxumPath(channel_id): AxumPath<String>,
    Json(req): Json<ChannelUpdateRequest>,
) -> impl IntoResponse {
    let config_path = state.paths.config_file();
    let result: anyhow::Result<serde_json::Value> = (|| async {
        let content = std::fs::read_to_string(&config_path)?;
        let mut root: serde_json::Value = serde_json::from_str(&content)?;

        let channels = root
            .get_mut("channels")
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| anyhow::anyhow!("no channels section in config"))?;

        let ch_key = channel_id.as_str();
        let ch = channels
            .entry(ch_key)
            .or_insert_with(|| serde_json::json!({}));

        if let Some(obj) = ch.as_object_mut() {
            // Insert fields with type coercion for non-string config fields
            for (k, v) in &req.fields {
                let coerced = match k.as_str() {
                    // Option<String>: empty string → null
                    "proxy" => {
                        let s = v.as_str().unwrap_or("");
                        if s.is_empty() {
                            serde_json::Value::Null
                        } else {
                            v.clone()
                        }
                    }
                    // Vec<String>: comma-separated string → JSON array
                    "channels" => {
                        let s = v.as_str().unwrap_or("");
                        let arr: Vec<&str> = if s.is_empty() {
                            vec![]
                        } else {
                            s.split(',')
                                .map(|x| x.trim())
                                .filter(|x| !x.is_empty())
                                .collect()
                        };
                        serde_json::json!(arr)
                    }
                    // u32/i64 numeric fields: string → number
                    "pollIntervalSecs" | "agentId" => {
                        let s = v.as_str().unwrap_or("0");
                        let n: i64 = s.parse().unwrap_or(0);
                        serde_json::json!(n)
                    }
                    _ => v.clone(),
                };
                obj.insert(k.clone(), coerced);
            }
            if let Some(en) = req.enabled {
                obj.insert("enabled".to_string(), serde_json::json!(en));
            }
            // Clean up stale snake_case keys from previous buggy saves
            let stale: &[&str] = &[
                "bot_token",
                "app_token",
                "app_id",
                "app_secret",
                "app_key",
                "corp_id",
                "corp_secret",
                "agent_id",
                "bridge_url",
                "allow_from",
                "poll_interval_secs",
                "encrypt_key",
                "verification_token",
                "robot_code",
                "callback_token",
                "encoding_aes_key",
            ];
            for key in stale {
                obj.remove(*key);
            }
        }

        std::fs::write(&config_path, serde_json::to_string_pretty(&root)?)?;
        Ok(serde_json::json!({ "status": "ok", "channel": ch_key }))
    })()
    .await;

    match result {
        Ok(v) => Json(v),
        Err(e) => Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
    }
}
