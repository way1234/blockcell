use super::*;
// ---------------------------------------------------------------------------
// Startup banner — colored, boxed output for key information
// ---------------------------------------------------------------------------

/// ANSI color helpers
mod ansi {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const CYAN: &str = "\x1b[36m";
    pub const WHITE: &str = "\x1b[97m";
    pub const BG_YELLOW: &str = "\x1b[43m";
    // 24-bit true-color matching the Logo.tsx palette
    pub const ORANGE: &str = "\x1b[38;2;234;88;12m"; // #ea580c
    pub const NEON_GREEN: &str = "\x1b[38;2;0;255;157m"; // #00ff9d
}

pub(super) fn rand_u32() -> u32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    std::process::id().hash(&mut h);
    h.finish() as u32
}

pub(super) fn print_startup_banner(
    config: &Config,
    host: &str,
    webui_host: &str,
    webui_port: u16,
    web_password: &str,
    webui_pass_is_temp: bool,
    is_exposed: bool,
    bind_addr: &str,
) {
    let ver = env!("CARGO_PKG_VERSION");
    let (model, provider, source) = active_model_and_provider(config);
    let model_label = if source == "modelPool" {
        match provider {
            Some(p) => format!("{} (modelPool: {})", model, p),
            None => format!("{} (modelPool)", model),
        }
    } else {
        model
    };

    // ── Logo + Header ──
    eprintln!();
    //  Layered hexagon logo — bold & colorful (matches Logo.tsx)
    let o = ansi::ORANGE;
    let g = ansi::NEON_GREEN;
    let r = ansi::RESET;

    eprintln!("           {o}▄▄▄▄▄▄▄{r}");
    eprintln!("       {o}▄█████████████▄{r}");
    eprintln!("     {o}▄████▀▀     ▀▀████▄{r}      {o}▄▄{r}");
    eprintln!("    {o}▐███▀{r}   {g}█████{r}   {o}▀███▌{r}    {o}████{r}");
    eprintln!("    {o}▐███{r}    {g}█████{r}    {o}███▌{r}     {o}▀▀{r}");
    eprintln!("    {o}▐███{r}    {g}█████{r}    {o}███▌{r}");
    eprintln!("    {o}▐███{r}    {g}█████{r}    {o}███▌{r}");
    eprintln!("    {o}▐███▄{r}   {g}▀▀▀▀▀{r}   {o}▄███▌{r}");
    eprintln!("     {o}▀████▄▄     ▄▄████▀{r}");
    eprintln!("   {o}▄▄{r}  {o}▀█████████████▀{r}");
    eprintln!("  {o}████{r}     {o}▀▀▀▀▀▀▀{r}");
    eprintln!("   {o}▀▀{r}");
    eprintln!();
    eprintln!(
        "  {}{}  BLOCKCELL GATEWAY v{}  {}",
        ansi::BOLD,
        ansi::CYAN,
        ver,
        ansi::RESET
    );
    eprintln!("  {}Model: {}{}", ansi::DIM, model_label, ansi::RESET);
    eprintln!();

    // ── WebUI Password box ──
    let box_w = 62;
    if webui_pass_is_temp {
        // Temp password — show prominently, warn it changes each restart
        eprintln!("  {}┌{}┐{}", ansi::YELLOW, "─".repeat(box_w), ansi::RESET);
        let pw_label = "🔑 WebUI Password: ";
        let pw_visible = 2 + display_width(pw_label) + web_password.len();
        let pw_pad = box_w.saturating_sub(pw_visible);
        eprintln!(
            "  {}│{}  {}{}{}{}{}{}│",
            ansi::YELLOW,
            ansi::RESET,
            ansi::BOLD,
            ansi::YELLOW,
            pw_label,
            web_password,
            ansi::RESET,
            " ".repeat(pw_pad),
        );
        let hint1 = "  Temporary — changes every restart. Set gateway.webuiPass";
        let hint1_pad = box_w.saturating_sub(hint1.len());
        eprintln!(
            "  {}│{}  {}Temporary — changes every restart. Set gateway.webuiPass{}{}{}│{}",
            ansi::YELLOW,
            ansi::RESET,
            ansi::DIM,
            ansi::RESET,
            " ".repeat(hint1_pad),
            ansi::YELLOW,
            ansi::RESET,
        );
        let hint2 = "  in config.json for a stable password.";
        let hint2_pad = box_w.saturating_sub(hint2.len());
        eprintln!(
            "  {}│{}  {}in config.json for a stable password.{}{}{}│{}",
            ansi::YELLOW,
            ansi::RESET,
            ansi::DIM,
            ansi::RESET,
            " ".repeat(hint2_pad),
            ansi::YELLOW,
            ansi::RESET,
        );
        eprintln!("  {}└{}┘{}", ansi::YELLOW, "─".repeat(box_w), ansi::RESET);
    } else {
        // Configured stable password
        eprintln!("  {}┌{}┐{}", ansi::GREEN, "─".repeat(box_w), ansi::RESET);
        let pw_label = "🔑 WebUI Password: ";
        let pw_visible = 2 + display_width(pw_label) + web_password.len();
        let pw_pad = box_w.saturating_sub(pw_visible);
        eprintln!(
            "  {}│{}  {}{}{}{}{}{}│",
            ansi::GREEN,
            ansi::RESET,
            ansi::BOLD,
            ansi::GREEN,
            pw_label,
            web_password,
            ansi::RESET,
            " ".repeat(pw_pad),
        );
        let hint = "  Configured via gateway.webuiPass in config.json";
        let hint_pad = box_w.saturating_sub(hint.len());
        eprintln!(
            "  {}│{}  {}Configured via gateway.webuiPass in config.json{}{}{}│{}",
            ansi::GREEN,
            ansi::RESET,
            ansi::DIM,
            ansi::RESET,
            " ".repeat(hint_pad),
            ansi::GREEN,
            ansi::RESET,
        );
        eprintln!("  {}└{}┘{}", ansi::GREEN, "─".repeat(box_w), ansi::RESET);
    }
    eprintln!();

    // ── Security warning ──
    if is_exposed && webui_pass_is_temp {
        eprintln!(
            "  {}{}⚠  SECURITY: Binding to {} with an auto-generated token.{}",
            ansi::BG_YELLOW,
            ansi::BOLD,
            host,
            ansi::RESET
        );
        eprintln!(
            "  {}   Review gateway.apiToken in config.json before exposing to the network.{}",
            ansi::YELLOW,
            ansi::RESET
        );
        eprintln!();
    }

    // ── Channels status ──
    eprintln!("  {}{}Channels{}", ansi::BOLD, ansi::WHITE, ansi::RESET);

    let ch = &config.channels;

    struct ChannelInfo {
        name: &'static str,
        enabled: bool,
        configured: bool,
        detail: String,
    }

    let channels = vec![
        ChannelInfo {
            name: "Telegram",
            enabled: ch.telegram.enabled,
            configured: !ch.telegram.token.is_empty(),
            detail: if ch.telegram.enabled && !ch.telegram.token.is_empty() {
                format!("allow_from: {:?}", ch.telegram.allow_from)
            } else if !ch.telegram.token.is_empty() {
                "token set but not enabled".into()
            } else {
                "no token configured".into()
            },
        },
        ChannelInfo {
            name: "Slack",
            enabled: ch.slack.enabled,
            configured: !ch.slack.bot_token.is_empty(),
            detail: if ch.slack.enabled && !ch.slack.bot_token.is_empty() {
                format!("channels: {:?}", ch.slack.channels)
            } else if !ch.slack.bot_token.is_empty() {
                "bot_token set but not enabled".into()
            } else {
                "no bot_token configured".into()
            },
        },
        ChannelInfo {
            name: "Discord",
            enabled: ch.discord.enabled,
            configured: !ch.discord.bot_token.is_empty(),
            detail: if ch.discord.enabled && !ch.discord.bot_token.is_empty() {
                format!("channels: {:?}", ch.discord.channels)
            } else if !ch.discord.bot_token.is_empty() {
                "bot_token set but not enabled".into()
            } else {
                "no bot_token configured".into()
            },
        },
        ChannelInfo {
            name: "Feishu",
            enabled: ch.feishu.enabled,
            configured: !ch.feishu.app_id.is_empty(),
            detail: if ch.feishu.enabled && !ch.feishu.app_id.is_empty() {
                "connected".into()
            } else if !ch.feishu.app_id.is_empty() {
                "app_id set but not enabled".into()
            } else {
                "no app_id configured".into()
            },
        },
        ChannelInfo {
            name: "Lark",
            enabled: ch.lark.enabled,
            configured: !ch.lark.app_id.is_empty(),
            detail: if ch.lark.enabled && !ch.lark.app_id.is_empty() {
                "webhook: POST /webhook/lark".into()
            } else if !ch.lark.app_id.is_empty() {
                "app_id set but not enabled".into()
            } else {
                "no app_id configured".into()
            },
        },
        ChannelInfo {
            name: "DingTalk",
            enabled: ch.dingtalk.enabled,
            configured: !ch.dingtalk.app_key.is_empty(),
            detail: if ch.dingtalk.enabled && !ch.dingtalk.app_key.is_empty() {
                format!("robot_code: {}", ch.dingtalk.robot_code)
            } else if !ch.dingtalk.app_key.is_empty() {
                "app_key set but not enabled".into()
            } else {
                "no app_key configured".into()
            },
        },
        ChannelInfo {
            name: "WeCom",
            enabled: ch.wecom.enabled,
            configured: !ch.wecom.corp_id.is_empty(),
            detail: if ch.wecom.enabled && !ch.wecom.corp_id.is_empty() {
                format!("agent_id: {}", ch.wecom.agent_id)
            } else if !ch.wecom.corp_id.is_empty() {
                "corp_id set but not enabled".into()
            } else {
                "no corp_id configured".into()
            },
        },
        ChannelInfo {
            name: "WhatsApp",
            enabled: ch.whatsapp.enabled,
            configured: true, // always has default bridge_url
            detail: if ch.whatsapp.enabled {
                format!("bridge: {}", ch.whatsapp.bridge_url)
            } else {
                "not enabled".into()
            },
        },
    ];

    // Enabled channels box (green)
    let enabled: Vec<&ChannelInfo> = channels
        .iter()
        .filter(|c| c.enabled && c.configured)
        .collect();
    if !enabled.is_empty() {
        let box_w = 62;
        eprintln!("  {}┌{}┐{}", ansi::GREEN, "─".repeat(box_w), ansi::RESET);
        for ch_info in &enabled {
            let line = format!("  ● {}  {}", ch_info.name, ch_info.detail);
            let pad = box_w.saturating_sub(display_width(&line));
            eprintln!(
                "  {}│{} {}{}● {}{} {}{}{}│{}",
                ansi::GREEN,
                ansi::RESET,
                ansi::BOLD,
                ansi::GREEN,
                ch_info.name,
                ansi::RESET,
                ch_info.detail,
                " ".repeat(pad),
                ansi::GREEN,
                ansi::RESET,
            );
        }
        eprintln!("  {}└{}┘{}", ansi::GREEN, "─".repeat(box_w), ansi::RESET);
    }

    // Disabled/unconfigured channels (dim, no box)
    let disabled: Vec<&ChannelInfo> = channels
        .iter()
        .filter(|c| !c.enabled || !c.configured)
        .collect();
    if !disabled.is_empty() {
        for ch_info in &disabled {
            eprintln!(
                "  {}  ○ {}  — {}{}",
                ansi::DIM,
                ch_info.name,
                ch_info.detail,
                ansi::RESET,
            );
        }
    }

    if channels.iter().all(|c| !c.enabled) {
        eprintln!(
            "  {}  No channels enabled. WebSocket is the only input.{}",
            ansi::DIM,
            ansi::RESET,
        );
    }
    eprintln!();

    // ── Server info ──
    eprintln!("  {}{}Server{}", ansi::BOLD, ansi::WHITE, ansi::RESET);

    eprintln!(
        "  {}HTTP/WS:{}  http://{}",
        ansi::CYAN,
        ansi::RESET,
        bind_addr,
    );
    eprintln!(
        "  {}WebUI:{}   http://{}:{}/",
        ansi::CYAN,
        ansi::RESET,
        webui_host,
        webui_port,
    );

    let api_base = config
        .gateway
        .public_api_base
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| format!("http://{}", bind_addr));
    eprintln!(
        "  {}API:{}     POST {}/v1/chat  |  GET {}/v1/health  |  GET {}/v1/ws",
        ansi::CYAN,
        ansi::RESET,
        api_base,
        api_base,
        api_base,
    );
    eprintln!();

    // ── Ready ──
    eprintln!(
        "  {}{}✓ Gateway ready.{} Press {}Ctrl+C{} to stop.",
        ansi::BOLD,
        ansi::GREEN,
        ansi::RESET,
        ansi::BOLD,
        ansi::RESET,
    );
    eprintln!();
}

/// Calculate the visible display width of a string (ignoring ANSI escape codes).
/// This is a simplified version — counts ASCII printable chars.
fn display_width(s: &str) -> usize {
    let mut w = 0usize;
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        if ch == '\x1b' {
            in_escape = true;
            continue;
        }
        // CJK characters are typically 2 columns wide
        if ch as u32 >= 0x4E00 && ch as u32 <= 0x9FFF {
            w += 2;
        } else {
            w += 1;
        }
    }
    w
}
