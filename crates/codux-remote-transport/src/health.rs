use crate::{
    RemoteTransport, RemoteTransportLogHandler, RemoteTransportMessageHandler,
    RemoteTransportStateHandler,
};
use async_trait::async_trait;
use codux_protocol::{
    REMOTE_TRANSPORT_PING, REMOTE_TRANSPORT_PONG, RemoteEnvelope, RemoteOutgoingEnvelope,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

const CONTROLLER_HEALTH_PING_INTERVAL: Duration = Duration::from_secs(10);
const CONTROLLER_RELAY_PING_TIMEOUT: Duration = Duration::from_secs(12);
const CONTROLLER_DIRECT_PING_TIMEOUT: Duration = Duration::from_secs(4);
const CONTROLLER_RELAY_MAX_MISSES: u32 = 3;

pub(crate) struct ControllerHealthState {
    pub(crate) device_id: String,
    pub(crate) on_message: RemoteTransportMessageHandler,
    pub(crate) on_state: RemoteTransportStateHandler,
    pub(crate) on_log: Option<RemoteTransportLogHandler>,
    path: Mutex<String>,
    pending_ping: Mutex<Option<ControllerHealthPing>>,
    miss_count: Mutex<u32>,
    seq: Mutex<u64>,
}

#[derive(Clone)]
pub(crate) struct ControllerHealthPing {
    pub(crate) id: String,
    sent_at: Instant,
}

impl ControllerHealthState {
    pub(crate) fn new(
        device_id: String,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
        on_log: Option<RemoteTransportLogHandler>,
    ) -> Self {
        Self {
            device_id,
            on_message,
            on_state,
            on_log,
            path: Mutex::new("unknown".to_string()),
            pending_ping: Mutex::new(None),
            miss_count: Mutex::new(0),
            seq: Mutex::new(0),
        }
    }

    pub(crate) fn handle_message(&self, device_id: String, data: Vec<u8>) {
        let Ok(text) = String::from_utf8(data.clone()) else {
            (self.on_message)(device_id, data);
            return;
        };
        let Ok(envelope) = serde_json::from_str::<RemoteEnvelope>(&text) else {
            (self.on_message)(device_id, text.into_bytes());
            return;
        };
        if envelope.kind == REMOTE_TRANSPORT_PONG {
            self.record_pong(envelope.payload);
            return;
        }
        (self.on_message)(device_id, text.into_bytes());
    }

    pub(crate) fn handle_state(&self, device_id: String, state: String) {
        if let Some(path) = parse_transport_state_path(&state) {
            self.set_path(&path);
        }
        (self.on_state)(device_id, state);
    }

    pub(crate) fn next_ping(&self) -> ControllerHealthPing {
        let mut seq = self.seq.lock().unwrap_or_else(|error| error.into_inner());
        *seq += 1;
        ControllerHealthPing {
            id: format!("rust-{}-{}", unix_timestamp_millis(), *seq),
            sent_at: Instant::now(),
        }
    }

    pub(crate) fn begin_ping(&self, ping: ControllerHealthPing) -> bool {
        let mut pending = self
            .pending_ping
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if pending.is_some() {
            return false;
        }
        *pending = Some(ping);
        true
    }

    fn record_pong(&self, payload: Value) {
        let mut pending = self
            .pending_ping
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(ping) = pending.clone() else {
            return;
        };
        if let Some(id) = payload.get("id").and_then(Value::as_str) {
            if id != ping.id {
                return;
            }
        }
        *pending = None;
        *self
            .miss_count
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = 0;
        let rtt = ping.sent_at.elapsed().as_millis().min(60_000);
        let path = self.path();
        (self.on_state)(
            self.device_id.clone(),
            format!("latency:rtt={rtt};path={path}"),
        );
    }

    pub(crate) fn record_timeout(&self, ping_id: &str) -> Option<(u32, String)> {
        let mut pending = self
            .pending_ping
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(ping) = pending.as_ref() else {
            return None;
        };
        if ping.id != ping_id {
            return None;
        }
        *pending = None;
        let mut miss_count = self
            .miss_count
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *miss_count += 1;
        Some((*miss_count, self.path()))
    }

    pub(crate) fn clear_pending(&self) {
        *self
            .pending_ping
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
    }

    pub(crate) fn set_path(&self, path: &str) {
        *self.path.lock().unwrap_or_else(|error| error.into_inner()) = path.to_string();
    }

    pub(crate) fn path(&self) -> String {
        self.path
            .lock()
            .map(|path| path.clone())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    fn log(&self, message: String) {
        if let Some(on_log) = self.on_log.as_ref() {
            on_log(message);
        }
    }
}

pub(crate) struct ControllerHealthTransport {
    pub(crate) inner: Arc<dyn RemoteTransport>,
    pub(crate) health: Arc<ControllerHealthState>,
    pub(crate) closed: AtomicBool,
}

impl ControllerHealthTransport {
    pub(crate) fn start(
        inner: Arc<dyn RemoteTransport>,
        health: Arc<ControllerHealthState>,
    ) -> Arc<dyn RemoteTransport> {
        let transport = Arc::new(Self {
            inner,
            health,
            closed: AtomicBool::new(false),
        });
        transport.spawn_health_loop();
        transport as Arc<dyn RemoteTransport>
    }

    fn spawn_health_loop(self: &Arc<Self>) {
        let transport = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(CONTROLLER_HEALTH_PING_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if transport.closed.load(Ordering::SeqCst) {
                    return;
                }
                // A demoted direct route should not be permanent: keep
                // probing so the transport can re-negotiate and re-upgrade
                // (the transport applies its own retry hold-down).
                if transport.health.path() == "relay" {
                    let _ = transport.inner.probe_preferred_route();
                }
                transport.send_health_ping().await;
            }
        });
    }

    pub(crate) async fn send_health_ping(self: &Arc<Self>) {
        let ping = self.health.next_ping();
        if !self.health.begin_ping(ping.clone()) {
            return;
        }
        let envelope = RemoteOutgoingEnvelope {
            kind: REMOTE_TRANSPORT_PING.to_string(),
            device_id: Some(self.health.device_id.clone()),
            session_id: None,
            seq: None,
            payload: json!({ "id": ping.id }),
        };
        let Ok(data) = serde_json::to_vec(&envelope) else {
            self.health.clear_pending();
            return;
        };
        if !self.inner.send(data, None) {
            self.health.clear_pending();
            return;
        }
        let timeout = if self.health.path() == "direct" {
            CONTROLLER_DIRECT_PING_TIMEOUT
        } else {
            CONTROLLER_RELAY_PING_TIMEOUT
        };
        let transport = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            transport.handle_ping_timeout(&ping.id);
        });
    }

    pub(crate) fn handle_ping_timeout(&self, ping_id: &str) {
        let Some((miss_count, path)) = self.health.record_timeout(ping_id) else {
            return;
        };
        self.health.log(format!(
            "controller_health timeout miss={miss_count} path={path}"
        ));
        (self.health.on_state)(
            self.health.device_id.clone(),
            format!("latency:timeout={miss_count};path={path}"),
        );
        if path == "direct" {
            if self.inner.mark_direct_unhealthy() {
                self.health.set_path("relay");
            }
        } else if miss_count >= CONTROLLER_RELAY_MAX_MISSES {
            (self.health.on_state)(self.health.device_id.clone(), "latency:lost".to_string());
        }
    }
}

#[async_trait]
impl RemoteTransport for ControllerHealthTransport {
    fn kind(&self) -> codux_protocol::RemoteTransportKind {
        self.inner.kind()
    }

    fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool {
        if self.inner.send(data.clone(), device_id) {
            return true;
        }
        if self.health.path() == "direct" && self.mark_direct_unhealthy() {
            self.health.log(
                "controller_health send failed on direct; degraded to relay and retrying"
                    .to_string(),
            );
            return self.inner.send(data, device_id);
        }
        false
    }

    fn mark_direct_unhealthy(&self) -> bool {
        let degraded = self.inner.mark_direct_unhealthy();
        if degraded {
            self.health.set_path("relay");
        }
        degraded
    }

    fn probe_preferred_route(&self) -> bool {
        self.inner.probe_preferred_route()
    }

    async fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.inner.shutdown().await;
    }
}

pub(crate) fn parse_transport_state_path(state: &str) -> Option<String> {
    state.split([';', ':']).find_map(|part| {
        let trimmed = part.trim();
        let value = trimmed.strip_prefix("path=")?;
        matches!(value, "direct" | "relay" | "mixed" | "none").then(|| value.to_string())
    })
}

fn unix_timestamp_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
