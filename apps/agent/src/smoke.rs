//! Internal smoke tests, kept behind the hidden `codux smoke <kind>` command and
//! used by CI to validate the shared crate boundary (PTY, transport, full serve).

use codux_protocol::REMOTE_HOST_INFO;
use codux_remote_transport::{
    RemoteControllerTransportConfig, RemoteHostTransportConfig, RemoteHostTransportHandlers,
    RemoteTransportCandidate, RemoteTransportFactory,
};
use codux_runtime_core::terminal::terminal_snapshot_payload;
use codux_terminal_core::{TerminalDriver, TerminalLaunchConfig, TerminalSessionHandle};
use codux_terminal_pty::LocalPtyDriver;
use std::{
    sync::{Arc, Mutex},
    thread, time,
};
use tokio::sync::oneshot;

pub fn run(kind: &str) -> Result<String, String> {
    match kind {
        "pty" => run_pty_smoke(),
        "transport" => run_transport_smoke(),
        "serve" => run_serve_smoke(),
        other => Err(format!(
            "unknown smoke kind: {other} (pty | transport | serve)"
        )),
    }
}

fn run_serve_smoke() -> Result<String, String> {
    tokio::runtime::Runtime::new()
        .map_err(|error| error.to_string())?
        .block_on(crate::host::run_serve_smoke_async())
}

fn run_pty_smoke() -> Result<String, String> {
    let driver = LocalPtyDriver::new();
    let session = driver.create(
        TerminalLaunchConfig {
            command: Some("printf codux-agent-pty-ok".to_string()),
            title: Some("Codux Agent PTY Smoke".to_string()),
            ..Default::default()
        },
        Box::new(|_| true),
    )?;

    let deadline = time::Instant::now() + time::Duration::from_secs(3);
    while time::Instant::now() < deadline {
        let snapshot = session.snapshot();
        if snapshot.contains("codux-agent-pty-ok") {
            let terminal = terminal_snapshot_payload(session.info());
            let _ = session.kill();
            return Ok(format!(
                "{snapshot}\nterminal={}",
                terminal["id"].as_str().unwrap_or_default()
            ));
        }
        thread::sleep(time::Duration::from_millis(20));
    }
    let snapshot = session.snapshot();
    let _ = session.kill();
    Err(format!("PTY smoke output not observed: {snapshot:?}"))
}

fn run_transport_smoke() -> Result<String, String> {
    tokio::runtime::Runtime::new()
        .map_err(|error| error.to_string())?
        .block_on(run_transport_smoke_async())
}

async fn run_transport_smoke_async() -> Result<String, String> {
    let config = RemoteHostTransportConfig {
        relay_url: "https://relay.example".to_string(),
        relay_preset: "global".to_string(),
        iroh_relay_url: String::new(),
        iroh_relay_authentication: String::new(),
        host_id: "host-smoke".to_string(),
        host_token: "token-smoke".to_string(),
    };
    let (received_tx, received_rx) = oneshot::channel::<String>();
    let received_tx = Arc::new(Mutex::new(Some(received_tx)));
    let host = RemoteTransportFactory::connect_host(
        &config,
        RemoteHostTransportHandlers {
            on_message: {
                let received_tx = Arc::clone(&received_tx);
                Arc::new(move |source, data| {
                    let Ok(mut guard) = received_tx.lock() else {
                        return;
                    };
                    let Some(tx) = guard.take() else {
                        return;
                    };
                    let text = String::from_utf8(data).unwrap_or_default();
                    let _ = tx.send(format!("{source}:{text}"));
                })
            },
            on_upload: Arc::new(|_| Ok(())),
            on_state: Arc::new(|_, _| {}),
            on_pairing: Arc::new(|_| None),
            on_authorize: Arc::new(|_, _| true),
            on_web_tunnel_tcp_connect: None,
            on_log: None,
        },
    )
    .await?;
    let (node_id, relay_url) = host
        .iroh_candidate()
        .ok_or_else(|| "iroh host candidate missing".to_string())?;
    let controller_config = RemoteControllerTransportConfig {
        relay_url: config.relay_url,
        host_id: config.host_id,
        device_id: "device-smoke".to_string(),
        device_token: "token-smoke".to_string(),
        transports: vec![RemoteTransportCandidate {
            kind: codux_protocol::REMOTE_TRANSPORT_IROH.to_string(),
            url: "https://relay.example/v3".to_string(),
            node_id,
            relay_url,
            ticket: host.iroh_endpoint_ticket().unwrap_or_default(),
            relay_authentication: String::new(),
        }],
    };
    let controller = RemoteTransportFactory::connect_controller(
        &controller_config,
        Arc::new(|_, _| {}),
        Arc::new(|_, _| {}),
        None,
    )
    .await?;
    let envelope = serde_json::json!({
        "type": REMOTE_HOST_INFO,
        "deviceId": controller_config.device_id,
        "payload": { "smoke": "codux-agent-transport-ok" },
    });
    let data = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
    if !controller.send(data, None) {
        return Err("iroh transport send failed".to_string());
    }
    let observed = tokio::time::timeout(time::Duration::from_secs(5), received_rx)
        .await
        .map_err(|_| "iroh transport message timeout".to_string())?
        .map_err(|_| "iroh transport message receiver closed".to_string())?;
    host.shutdown().await;
    controller.shutdown().await;
    Ok(format!("codux-agent-transport-ok\nreceived={observed}"))
}
