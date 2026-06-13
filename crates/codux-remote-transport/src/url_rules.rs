pub const GLOBAL_RELAY_SERVER_URL: &str = "https://codux-node.dux.plus";
pub const CHINA_RELAY_SERVER_URL: &str = "https://codux-service.dux.plus";
pub const DEFAULT_RELAY_SERVER_URL: &str = GLOBAL_RELAY_SERVER_URL;

pub fn remote_relay_url_for_preset(preset: &str, custom_url: &str) -> String {
    match preset.trim() {
        "global" => GLOBAL_RELAY_SERVER_URL.to_string(),
        "china" => CHINA_RELAY_SERVER_URL.to_string(),
        "" => GLOBAL_RELAY_SERVER_URL.to_string(),
        "custom" => custom_url.trim().to_string(),
        _ => custom_url.trim().to_string(),
    }
}

pub fn remote_relay_preset_for_url(url: &str) -> String {
    let normalized = remote_server_url(url);
    if normalized == remote_server_url(GLOBAL_RELAY_SERVER_URL) || url.trim().is_empty() {
        "global".to_string()
    } else if normalized == remote_server_url(CHINA_RELAY_SERVER_URL) {
        "china".to_string()
    } else {
        "custom".to_string()
    }
}

pub fn remote_server_url(value: &str) -> String {
    let value = value.trim();
    let value = if value.is_empty() {
        DEFAULT_RELAY_SERVER_URL
    } else {
        value
    };
    with_protocol_path(value)
}

pub fn remote_stun_urls() -> Vec<String> {
    vec![
        "stun:stun.miwifi.com:3478".to_string(),
        "stun:stun.l.google.com:19302".to_string(),
    ]
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteTurnConfig {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

/// Optional TURN relay configuration sourced from the environment:
/// `CODUX_TURN_URLS` holds comma-separated `turn:`/`turns:` URLs, with
/// optional `CODUX_TURN_USERNAME` / `CODUX_TURN_CREDENTIAL` for long-term
/// credentials. Returns `None` when no TURN URLs are configured.
pub fn remote_turn_config_from_env() -> Option<RemoteTurnConfig> {
    let urls: Vec<String> = std::env::var("CODUX_TURN_URLS")
        .ok()?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    if urls.is_empty() {
        return None;
    }
    Some(RemoteTurnConfig {
        urls,
        username: std::env::var("CODUX_TURN_USERNAME").unwrap_or_default(),
        credential: std::env::var("CODUX_TURN_CREDENTIAL").unwrap_or_default(),
    })
}

pub fn remote_url(
    base: &str,
    path: &str,
    query: &[(&str, &str)],
    websocket: bool,
) -> Result<String, String> {
    let mut url = url::Url::parse(base.trim()).map_err(|error| error.to_string())?;
    url.set_path(&join_url_path(url.path(), path));
    url.set_query(None);
    if websocket {
        let scheme = match url.scheme() {
            "https" => "wss",
            "http" => "ws",
            other => other,
        }
        .to_string();
        let _ = url.set_scheme(&scheme);
    }
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query {
            pairs.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

pub fn remote_pairing_ticket_url(base: &str, ticket: &str) -> Result<String, String> {
    let base = remote_server_url(base);
    remote_url(
        &base,
        &format!("/api/tickets/{}", ticket.trim()),
        &[],
        false,
    )
}

pub fn remote_pairing_code_url(base: &str, code: &str) -> Result<String, String> {
    let base = remote_server_url(base);
    remote_url(
        &base,
        &format!("/api/pairings/code/{}", code.trim()),
        &[],
        false,
    )
}

pub fn remote_client_websocket_url(
    base: &str,
    host_id: &str,
    device_id: &str,
    token: Option<&str>,
) -> Result<String, String> {
    let base = remote_server_url(base);
    let mut query = vec![("hostId", host_id), ("deviceId", device_id)];
    if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
        query.push(("token", token));
    }
    remote_url(&base, "/ws/client", &query, true)
}

pub fn remote_pairing_websocket_url(
    base: &str,
    host_id: &str,
    device_public_key: &str,
) -> Result<String, String> {
    let base = remote_server_url(base);
    remote_url(
        &base,
        "/ws/client",
        &[("hostId", host_id), ("deviceId", device_public_key)],
        true,
    )
}

pub fn preferred_controller_transport_kind<'a>(
    candidates: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> &'static str {
    let mut has_relay = false;
    let mut has_webrtc = false;
    for (kind, url) in candidates {
        if kind == "webRtc" && !url.trim().is_empty() {
            has_webrtc = true;
        }
        if kind == "websocketRelay" && !url.trim().is_empty() {
            has_relay = true;
        }
    }
    if has_relay && has_webrtc {
        "webRtc"
    } else if has_relay {
        "websocketRelay"
    } else {
        ""
    }
}

pub fn preferred_pairing_transport_kind<'a>(
    candidates: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> &'static str {
    let mut has_webrtc = false;
    for (kind, url) in candidates {
        if kind == "websocketRelay" && !url.trim().is_empty() {
            return "websocketRelay";
        }
        if kind == "webRtc" && !url.trim().is_empty() {
            has_webrtc = true;
        }
    }
    if has_webrtc { "webRtc" } else { "" }
}

fn join_url_path(base_path: &str, path: &str) -> String {
    let base_path = base_path.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    if base_path.is_empty() {
        format!("/{path}")
    } else if path.is_empty() {
        base_path.to_string()
    } else {
        format!("{base_path}/{path}")
    }
}

fn with_protocol_path(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return value.to_string();
    };
    if url.path().trim_matches('/').is_empty() {
        url.set_path("/v3");
    }
    url.to_string().trim_end_matches('/').to_string()
}
