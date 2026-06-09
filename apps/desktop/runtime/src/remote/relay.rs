pub const GLOBAL_RELAY_SERVER_URL: &str = "https://codux-node.dux.plus";
pub const CHINA_RELAY_SERVER_URL: &str = "https://codux-service.dux.plus";
pub(crate) const DEFAULT_RELAY_SERVER_URL: &str = GLOBAL_RELAY_SERVER_URL;

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
    if normalized == remote_server_url(GLOBAL_RELAY_SERVER_URL) {
        "global".to_string()
    } else if url.trim().is_empty() {
        "global".to_string()
    } else if normalized == remote_server_url(CHINA_RELAY_SERVER_URL) {
        "china".to_string()
    } else {
        "custom".to_string()
    }
}

pub(crate) fn remote_server_url(value: &str) -> String {
    let value = value.trim();
    let value = if value.is_empty() {
        DEFAULT_RELAY_SERVER_URL
    } else {
        value
    };
    with_protocol_path(value)
}

pub(crate) fn remote_stun_urls() -> Vec<String> {
    vec![
        "stun:stun.miwifi.com:3478".to_string(),
        "stun:stun.l.google.com:19302".to_string(),
    ]
}

pub(crate) fn remote_url(
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

pub(crate) fn remote_pairing_ticket_payload(base: &str, ticket: &str) -> Result<String, String> {
    let mut url = url::Url::parse("codux://pair").map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("server", base.trim())
        .append_pair("ticket", ticket.trim());
    Ok(url.to_string())
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
