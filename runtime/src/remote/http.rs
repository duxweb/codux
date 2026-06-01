use super::types::RemoteSettings;
use crate::runtime_trace::{runtime_trace, runtime_trace_elapsed};
use reqwest::header::CONTENT_TYPE;
use serde_json::Value;
use std::{any::type_name, time::Instant};

pub(crate) fn default_remote_server_url() -> String {
    "http://127.0.0.1:8088".to_string()
}

pub(crate) fn remote_server_url(settings: &RemoteSettings) -> String {
    if settings.server_url.trim().is_empty() {
        default_remote_server_url()
    } else {
        settings.server_url.trim().to_string()
    }
}

pub(crate) fn remote_url(
    base: &str,
    path: &str,
    query: &[(&str, &str)],
    websocket: bool,
) -> Result<String, String> {
    let mut url = url::Url::parse(base.trim()).map_err(|error| error.to_string())?;
    url.set_path(path);
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
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query {
            pairs.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

pub(crate) fn remote_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(remote_error_message)
}

pub(crate) async fn remote_parse_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, String> {
    let started_at = Instant::now();
    let status = response.status();
    let bytes = response.bytes().await.map_err(remote_error_message)?;
    runtime_trace_elapsed(
        "remote-http",
        "parse_response",
        started_at,
        &format!(
            "status={} bytes={} target={}",
            status.as_u16(),
            bytes.len(),
            type_name::<T>()
        ),
    );
    if !status.is_success() {
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                return Err(error.to_string());
            }
        }
        return Err(String::from_utf8_lossy(&bytes).to_string());
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "Remote response decode failed: {error}. Body: {}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

pub(crate) async fn remote_post<T: serde::de::DeserializeOwned>(
    base: &str,
    path: &str,
    body: Value,
) -> Result<T, String> {
    let started_at = Instant::now();
    let url = remote_url(base, path, &[], false)?;
    runtime_trace(
        "remote-http",
        &format!("post start path={path} base={}", base.trim()),
    );
    let client = match remote_http_client() {
        Ok(client) => client,
        Err(error) => {
            runtime_trace(
                "remote-http",
                &format!("post client_failed path={path} error={error}"),
            );
            return Err(error);
        }
    };
    let response = match client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(response) => {
            runtime_trace_elapsed(
                "remote-http",
                "post response",
                started_at,
                &format!("path={path} status={}", response.status().as_u16()),
            );
            response
        }
        Err(error) => {
            let error = remote_error_message(error);
            runtime_trace_elapsed(
                "remote-http",
                "post failed",
                started_at,
                &format!("path={path} error={error}"),
            );
            return Err(error);
        }
    };
    remote_parse_response(response).await
}

pub(crate) fn remote_post_blocking<T: serde::de::DeserializeOwned + Send>(
    base: &str,
    path: &str,
    body: Value,
) -> Result<T, String> {
    crate::async_runtime::block_on(remote_post(base, path, body))
}

pub(crate) fn remote_error_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
