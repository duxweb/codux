//! Minimal headless host: serve a few real runtime domains over the Iroh
//! transport so a controller (desktop client or mobile) can browse this
//! machine's files and read its host info. This is the first real slice of the
//! "headless controlled-end" — terminal/Git/AI domains follow the same
//! dispatch shape (see plan/interconnect-plan.md), reusing the stateless
//! payload builders in `codux-runtime-core`.

use codux_protocol::{
    REMOTE_ERROR, REMOTE_FILE_CREATE_DIRECTORY, REMOTE_FILE_DELETE, REMOTE_FILE_DELETED,
    REMOTE_FILE_DIRECTORY_CREATED, REMOTE_FILE_LIST, REMOTE_FILE_READ, REMOTE_FILE_RENAME,
    REMOTE_FILE_RENAMED, REMOTE_FILE_WRITE, REMOTE_FILE_WRITTEN, REMOTE_HOST_INFO,
    REMOTE_PROJECT_ADD, REMOTE_PROJECT_LIST, REMOTE_PROJECT_REMOVE, REMOTE_TRANSPORT_IROH,
    REMOTE_TRANSPORT_PING, REMOTE_TRANSPORT_PONG,
};
use codux_remote_transport::{
    RemoteHostTransportConfig, RemoteTransport, RemoteTransportCandidate, RemoteTransportFactory,
};
use codux_runtime_core::{
    file::{
        file_delete, file_list_payload, file_make_directory, file_read_payload, file_rename,
        file_write,
    },
    host::{host_info_payload, HostInfoPayload},
    project::project_list_payload,
};

use crate::projects::AgentProjectStore;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// What the agent needs to stand up a host endpoint.
pub struct AgentHostConfig {
    pub host_id: String,
    pub host_token: String,
    pub name: String,
    pub relay_preset: String,
    pub relay_url: String,
}

type TransportSlot = Arc<Mutex<Option<Arc<dyn RemoteTransport>>>>;

/// Build the message handler that dispatches incoming envelopes to the served
/// domains and replies through the (post-connect) transport handle.
fn make_handler(
    slot: TransportSlot,
    host_id: String,
    name: String,
) -> codux_remote_transport::RemoteTransportMessageHandler {
    Arc::new(move |_source: String, data: Vec<u8>| {
        let Ok(envelope) = serde_json::from_slice::<Value>(&data) else {
            return;
        };
        let kind = envelope.get("type").and_then(Value::as_str).unwrap_or("");
        let device_id = envelope.get("deviceId").and_then(Value::as_str);
        let request_id = envelope.get("requestId").and_then(Value::as_str);
        let payload = envelope.get("payload").cloned().unwrap_or(Value::Null);

        // (reply_kind, reply_payload). `None` => nothing to send back.
        let reply: Option<(&str, Value)> = match kind {
            REMOTE_TRANSPORT_PING => Some((REMOTE_TRANSPORT_PONG, json!({}))),
            REMOTE_HOST_INFO => Some((
                REMOTE_HOST_INFO,
                // Transports left empty: the controller already knows the path
                // it connected on; host.info here carries identity/capabilities.
                host_info_payload(HostInfoPayload {
                    host_id: host_id.clone(),
                    runtime_instance_id: format!("{host_id}-agent"),
                    name: name.clone(),
                    platform: std::env::consts::OS.to_string(),
                    app: "codux-agent".to_string(),
                    transports: Vec::new(),
                }),
            )),
            REMOTE_FILE_LIST => {
                let path = payload.get("path").and_then(Value::as_str);
                let purpose = payload.get("purpose").and_then(Value::as_str);
                Some((REMOTE_FILE_LIST, file_list_payload(path, purpose)))
            }
            REMOTE_FILE_READ => match payload.get("path").and_then(Value::as_str) {
                Some(path) => match file_read_payload(path) {
                    Ok(value) => Some((REMOTE_FILE_READ, value)),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                None => Some((REMOTE_ERROR, json!({ "message": "File path is required." }))),
            },
            REMOTE_FILE_WRITE => match (
                payload.get("path").and_then(Value::as_str),
                payload.get("content").and_then(Value::as_str),
            ) {
                (Some(path), Some(content)) => match file_write(path, content) {
                    Ok(()) => Some((REMOTE_FILE_WRITTEN, json!({ "path": path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                _ => Some((
                    REMOTE_ERROR,
                    json!({ "message": "File path and content are required." }),
                )),
            },
            REMOTE_FILE_RENAME => match (
                payload.get("path").and_then(Value::as_str),
                payload.get("newPath").and_then(Value::as_str),
            ) {
                (Some(path), Some(new_path)) => match file_rename(path, new_path) {
                    Ok(()) => Some((REMOTE_FILE_RENAMED, json!({ "path": path, "newPath": new_path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                _ => Some((
                    REMOTE_ERROR,
                    json!({ "message": "File path and newPath are required." }),
                )),
            },
            REMOTE_FILE_DELETE => match payload.get("path").and_then(Value::as_str) {
                Some(path) => match file_delete(path) {
                    Ok(()) => Some((REMOTE_FILE_DELETED, json!({ "path": path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                None => Some((REMOTE_ERROR, json!({ "message": "File path is required." }))),
            },
            REMOTE_FILE_CREATE_DIRECTORY => match payload.get("path").and_then(Value::as_str) {
                Some(path) => match file_make_directory(path) {
                    Ok(()) => Some((REMOTE_FILE_DIRECTORY_CREATED, json!({ "path": path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                None => Some((REMOTE_ERROR, json!({ "message": "Directory path is required." }))),
            },
            REMOTE_PROJECT_LIST => Some((
                REMOTE_PROJECT_LIST,
                project_list_payload(AgentProjectStore::new().list(), None, None),
            )),
            REMOTE_PROJECT_ADD => match payload.get("path").and_then(Value::as_str) {
                Some(path) => {
                    let name = payload.get("name").and_then(Value::as_str);
                    match AgentProjectStore::new().add(path, name) {
                        Ok(items) => {
                            Some((REMOTE_PROJECT_LIST, project_list_payload(items, None, None)))
                        }
                        Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                    }
                }
                None => Some((REMOTE_ERROR, json!({ "message": "Project path is required." }))),
            },
            REMOTE_PROJECT_REMOVE => {
                let id = payload
                    .get("id")
                    .or_else(|| payload.get("projectId"))
                    .and_then(Value::as_str);
                match id {
                    Some(id) => match AgentProjectStore::new().remove(id) {
                        Ok(items) => {
                            Some((REMOTE_PROJECT_LIST, project_list_payload(items, None, None)))
                        }
                        Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                    },
                    None => Some((REMOTE_ERROR, json!({ "message": "Project id is required." }))),
                }
            }
            _ => None,
        };

        let Some((reply_kind, reply_payload)) = reply else {
            return;
        };
        let mut reply_envelope = json!({ "type": reply_kind, "payload": reply_payload });
        if let Some(device_id) = device_id {
            reply_envelope["deviceId"] = json!(device_id);
        }
        if let Some(request_id) = request_id {
            reply_envelope["requestId"] = json!(request_id);
        }
        let Ok(bytes) = serde_json::to_vec(&reply_envelope) else {
            return;
        };
        if let Ok(guard) = slot.lock() {
            if let Some(transport) = guard.as_ref() {
                transport.send(bytes, device_id);
            }
        }
    })
}

/// Connect a host transport with the dispatch handler. Returns the transport
/// handle and the slot it has been stored in (for replies).
async fn connect_serving_host(
    cfg: &AgentHostConfig,
) -> Result<(Arc<dyn RemoteTransport>, TransportSlot), String> {
    let slot: TransportSlot = Arc::new(Mutex::new(None));
    let config = RemoteHostTransportConfig {
        relay_url: cfg.relay_url.clone(),
        relay_preset: cfg.relay_preset.clone(),
        iroh_relay_url: String::new(),
        iroh_relay_authentication: String::new(),
        host_id: cfg.host_id.clone(),
        host_token: cfg.host_token.clone(),
    };
    let host = RemoteTransportFactory::connect_host(
        &config,
        make_handler(Arc::clone(&slot), cfg.host_id.clone(), cfg.name.clone()),
        Arc::new(|_| Ok(())),
        Arc::new(|_, _| {}),
        Arc::new(|_| {}),
        None,
    )
    .await?;
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(Arc::clone(&host));
    }
    Ok((host, slot))
}

/// Run the headless host until the process is stopped, printing the pairing
/// candidate so a controller can connect.
pub async fn run_host(cfg: AgentHostConfig) -> Result<(), String> {
    let (host, _slot) = connect_serving_host(&cfg).await?;
    println!("codux-agent host ready");
    println!("hostId={}", cfg.host_id);
    println!("name={}", cfg.name);
    println!("platform={}", std::env::consts::OS);
    if let Some((node_id, relay_url)) = host.iroh_candidate() {
        println!("nodeId={node_id}");
        println!("relay={relay_url}");
    }
    if let Some(ticket) = host.iroh_endpoint_ticket() {
        println!("ticket={ticket}");
    }
    // Serve until the process is terminated.
    std::future::pending::<()>().await;
    Ok(())
}

/// In-process round trip: stand up the serving host, connect a controller, and
/// exercise the full file domain (list, mkdir, write, read, delete) end to end.
/// Proves the headless host actually serves real domains over the transport.
pub async fn run_serve_smoke_async() -> Result<String, String> {
    use codux_remote_transport::RemoteControllerTransportConfig;
    use tokio::sync::mpsc;

    let cfg = AgentHostConfig {
        host_id: "host-serve-smoke".to_string(),
        host_token: "token-serve-smoke".to_string(),
        name: "codux-agent-smoke".to_string(),
        relay_preset: "global".to_string(),
        relay_url: "https://relay.example".to_string(),
    };
    // Keep the smoke's project store out of the real ~/.codux-agent.
    let data_dir = std::env::temp_dir().join(format!("codux-agent-data-{}", std::process::id()));
    // Safe: the smoke sets this before any host/controller task is spawned.
    unsafe {
        std::env::set_var("CODUX_AGENT_DATA_DIR", &data_dir);
    }
    let (host, _slot) = connect_serving_host(&cfg).await?;
    let (node_id, relay_url) = host
        .iroh_candidate()
        .ok_or_else(|| "iroh host candidate missing".to_string())?;

    // Every reply from the host is forwarded as (type, payload).
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel::<(String, Value)>();
    let device_id = "device-serve-smoke".to_string();
    let controller_config = RemoteControllerTransportConfig {
        relay_url: cfg.relay_url.clone(),
        host_id: cfg.host_id.clone(),
        device_id: device_id.clone(),
        device_token: "token-serve-smoke".to_string(),
        transports: vec![RemoteTransportCandidate {
            kind: REMOTE_TRANSPORT_IROH.to_string(),
            url: "https://relay.example/v3".to_string(),
            node_id,
            relay_url,
            ticket: host.iroh_endpoint_ticket().unwrap_or_default(),
            relay_authentication: String::new(),
        }],
    };
    let controller = RemoteTransportFactory::connect_controller(
        &controller_config,
        Arc::new(move |_source: String, data: Vec<u8>| {
            if let Ok(envelope) = serde_json::from_slice::<Value>(&data) {
                if let Some(kind) = envelope.get("type").and_then(Value::as_str) {
                    let payload = envelope.get("payload").cloned().unwrap_or(Value::Null);
                    let _ = reply_tx.send((kind.to_string(), payload));
                }
            }
        }),
        Arc::new(|_, _| {}),
        None,
    )
    .await?;

    let controller_ref = &controller;
    let device_ref = &device_id;
    let request = |kind: &str, payload: Value| -> Result<(), String> {
        let envelope = json!({ "type": kind, "deviceId": device_ref, "payload": payload });
        let bytes = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
        if !controller_ref.send(bytes, None) {
            return Err(format!("controller send failed for {kind}"));
        }
        Ok(())
    };
    async fn expect(
        rx: &mut mpsc::UnboundedReceiver<(String, Value)>,
        want: &str,
    ) -> Result<Value, String> {
        loop {
            let (kind, payload) = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                .await
                .map_err(|_| format!("timeout waiting for {want}"))?
                .ok_or_else(|| format!("channel closed waiting for {want}"))?;
            if kind == want {
                return Ok(payload);
            }
            if kind == REMOTE_ERROR {
                return Err(format!(
                    "host error while waiting for {want}: {}",
                    payload.get("message").and_then(Value::as_str).unwrap_or("")
                ));
            }
        }
    }

    let run = async {
        // 1. list
        request(REMOTE_FILE_LIST, json!({ "purpose": "projectFiles" }))?;
        let listed = expect(&mut reply_rx, REMOTE_FILE_LIST).await?;
        if listed.get("entries").is_none() {
            return Err("file.list reply missing entries".to_string());
        }

        // 2. mkdir / write / read / delete in a unique temp dir
        let dir = std::env::temp_dir().join(format!("codux-agent-serve-{}", std::process::id()));
        let dir = dir.to_string_lossy().to_string();
        let file = format!("{dir}/probe.txt");

        request(REMOTE_FILE_CREATE_DIRECTORY, json!({ "path": dir }))?;
        expect(&mut reply_rx, REMOTE_FILE_DIRECTORY_CREATED).await?;
        if !std::path::Path::new(&dir).is_dir() {
            return Err("createDirectory did not create the directory".to_string());
        }

        request(REMOTE_FILE_WRITE, json!({ "path": file, "content": "codux-agent" }))?;
        expect(&mut reply_rx, REMOTE_FILE_WRITTEN).await?;

        request(REMOTE_FILE_READ, json!({ "path": file }))?;
        let read = expect(&mut reply_rx, REMOTE_FILE_READ).await?;
        if read.get("content").and_then(Value::as_str) != Some("codux-agent") {
            return Err(format!("file.read returned unexpected content: {read}"));
        }

        request(REMOTE_FILE_DELETE, json!({ "path": dir }))?;
        expect(&mut reply_rx, REMOTE_FILE_DELETED).await?;
        if std::path::Path::new(&dir).exists() {
            return Err("delete did not remove the directory".to_string());
        }

        // 3. project domain (add / remove)
        let project_path = format!("/tmp/codux-agent-project-{}", std::process::id());
        request(REMOTE_PROJECT_ADD, json!({ "path": project_path, "name": "Smoke" }))?;
        let added = expect(&mut reply_rx, REMOTE_PROJECT_LIST).await?;
        let project_id = added
            .get("projects")
            .and_then(Value::as_array)
            .and_then(|projects| {
                projects.iter().find(|project| {
                    project.get("path").and_then(Value::as_str) == Some(project_path.as_str())
                })
            })
            .and_then(|project| project.get("id").and_then(Value::as_str))
            .ok_or_else(|| "project.add did not register the project".to_string())?
            .to_string();

        request(REMOTE_PROJECT_REMOVE, json!({ "id": project_id }))?;
        let removed = expect(&mut reply_rx, REMOTE_PROJECT_LIST).await?;
        let still_present = removed
            .get("projects")
            .and_then(Value::as_array)
            .map(|projects| {
                projects.iter().any(|project| {
                    project.get("path").and_then(Value::as_str) == Some(project_path.as_str())
                })
            })
            .unwrap_or(false);
        if still_present {
            return Err("project.remove did not remove the project".to_string());
        }
        Ok::<(), String>(())
    };
    let result = run.await;

    host.shutdown().await;
    controller.shutdown().await;
    let _ = std::fs::remove_dir_all(&data_dir);
    result?;
    Ok(
        "codux-agent-serve-ok\nfile domain (list/mkdir/write/read/delete) + project domain (add/remove) verified"
            .to_string(),
    )
}
