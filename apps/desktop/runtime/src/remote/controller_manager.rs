//! Pools live `RemoteController` connections to paired hosts, keyed by the
//! device id we minted during pairing. Lazily connects from the saved-host
//! store on first use, and bridges the async controller API into the
//! synchronous `RuntimeService` domain methods via `async_runtime::block_on`.
//!
//! Each connection is wired to the controller transport's link-state callback,
//! so a dropped iroh link is detected, the dead controller is evicted, and a
//! backoff reconnect loop is spawned. The desktop polls [`link_states`] to drive
//! the per-project connection badge and to re-attach terminals on recovery.

use super::controller::{RemoteController, new_device_id, parse_pairing_ticket};
use super::controller_store::{RemoteControllerStore, SavedRemoteHost};
use codux_remote_transport::RemoteTransportStateHandler;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);

/// Liveness of the client→host iroh link for a paired device.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControllerLinkState {
    /// A connect/reconnect attempt is in flight.
    Connecting,
    /// The link is up and usable.
    Connected,
    /// The link dropped; a reconnect loop is retrying in the background.
    Disconnected,
}

impl ControllerLinkState {
    /// Map a transport `on_state` string to a link state. The controller
    /// transport emits `"connecting"`, `"connected"` (and `"connected:path=…"`),
    /// and `"closed"`; anything that isn't a connected/connecting marker is
    /// treated as a drop.
    fn from_transport_state(state: &str) -> Self {
        if state.starts_with("connected") {
            Self::Connected
        } else if state.starts_with("connecting") {
            Self::Connecting
        } else {
            Self::Disconnected
        }
    }
}

/// How the live link to a host is routed — shown next to the connected state so
/// the user can tell a LAN/p2p direct path from a relay-routed one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControllerLinkPath {
    /// Direct peer-to-peer path (LAN via mDNS, or a holepunched route).
    Direct,
    /// Routed through the relay.
    Relay,
}

impl ControllerLinkPath {
    /// Parse the path type from a transport state string. The controller transport
    /// publishes `connected:path=direct;addr=…` / `connected:path=relay;…` once a
    /// path is selected; other states (`connecting`, bare `connected`) carry none.
    fn from_transport_state(state: &str) -> Option<Self> {
        let detail = state.strip_prefix("connected:path=")?;
        match detail.split(';').next().unwrap_or(detail) {
            "direct" => Some(Self::Direct),
            "relay" => Some(Self::Relay),
            _ => None,
        }
    }
}

/// Shared manager state captured (weakly) by transport callbacks and the
/// reconnect loop, so a dropped link can evict + reconnect without a reference
/// back to the owning `RuntimeService`.
struct ManagerShared {
    store: RemoteControllerStore,
    connections: Mutex<HashMap<String, Arc<RemoteController>>>,
    connection_epochs: Mutex<HashMap<String, u64>>,
    links: Mutex<HashMap<String, ControllerLinkState>>,
    // Selected path type (direct/relay) per connected device, for the UI badge.
    // Set when the transport reports a selected path; cleared when the link drops.
    paths: Mutex<HashMap<String, ControllerLinkPath>>,
    reconnecting: Mutex<HashSet<String>>,
    // Last failure from the background reconnect loop per device, so
    // `controller_for` can surface *why* a host stays "not ready" (offline host,
    // relay unreachable, dial timeout) instead of a generic message.
    last_errors: Mutex<HashMap<String, String>>,
}

impl ManagerShared {
    fn set_link(&self, device_id: &str, state: ControllerLinkState) {
        if let Ok(mut links) = self.links.lock() {
            links.insert(device_id.to_string(), state);
        }
    }

    fn set_connecting_link(&self, device_id: &str) {
        if self.last_error(device_id).is_none() {
            self.set_link(device_id, ControllerLinkState::Connecting);
        }
    }

    fn set_path(&self, device_id: &str, path: ControllerLinkPath) {
        if let Ok(mut paths) = self.paths.lock() {
            paths.insert(device_id.to_string(), path);
        }
    }

    fn clear_path(&self, device_id: &str) {
        if let Ok(mut paths) = self.paths.lock() {
            paths.remove(device_id);
        }
    }

    fn last_error(&self, device_id: &str) -> Option<String> {
        self.last_errors
            .lock()
            .ok()
            .and_then(|errors| errors.get(device_id).cloned())
    }

    fn next_connection_epoch(&self, device_id: &str) -> u64 {
        let mut epochs = self.connection_epochs.lock().unwrap();
        let epoch = epochs.entry(device_id.to_string()).or_insert(0);
        *epoch = epoch.saturating_add(1);
        *epoch
    }

    fn is_current_connection_epoch(&self, device_id: &str, epoch: u64) -> bool {
        self.connection_epochs
            .lock()
            .ok()
            .and_then(|epochs| epochs.get(device_id).copied())
            == Some(epoch)
    }

    fn shutdown_controller(controller: Arc<RemoteController>) {
        crate::async_runtime::spawn(async move {
            controller.shutdown().await;
        });
    }

    /// Build the transport state handler for `device_id`: it records every link
    /// transition and, on a drop, evicts the dead controller and kicks off a
    /// background reconnect. Holds only a `Weak` so a forgotten manager lets the
    /// callback become a no-op.
    fn state_handler(self: &Arc<Self>, device_id: &str, epoch: u64) -> RemoteTransportStateHandler {
        let weak = Arc::downgrade(self);
        let device_id = device_id.to_string();
        Arc::new(move |_node: String, state: String| {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            if !shared.is_current_connection_epoch(&device_id, epoch) {
                return;
            }
            let mapped = ControllerLinkState::from_transport_state(&state);
            if mapped == ControllerLinkState::Connecting {
                shared.set_connecting_link(&device_id);
            } else {
                shared.set_link(&device_id, mapped);
            }
            if let Some(path) = ControllerLinkPath::from_transport_state(&state) {
                shared.set_path(&device_id, path);
            }
            if mapped == ControllerLinkState::Disconnected {
                shared.clear_path(&device_id);
                shared.handle_disconnect(&device_id);
            }
        })
    }

    /// Ensure a background connect/reconnect loop is retrying this device.
    /// Idempotent: marks the device as reconnecting (so `controller_for`
    /// fast-fails instead of re-dialling on the calling thread) and spawns the
    /// loop only if one isn't already running. Returns `false` if the host has
    /// been forgotten — nothing to connect to.
    fn mark_reconnect_needed(self: &Arc<Self>, device_id: &str) -> Option<bool> {
        let mut reconnecting = self.reconnecting.lock().ok()?;
        // The saved host is the source of truth for "should we reconnect":
        // a forgotten device must not be resurrected.
        self.store.find(device_id)?;
        if reconnecting.contains(device_id) {
            return Some(false);
        }
        reconnecting.insert(device_id.to_string());
        Some(true)
    }

    fn spawn_reconnect_loop(self: &Arc<Self>, device_id: &str) {
        let shared = Arc::clone(self);
        let device_id = device_id.to_string();
        crate::async_runtime::spawn(async move {
            shared.reconnect_loop(device_id).await;
        });
    }

    fn ensure_reconnect_loop(self: &Arc<Self>, device_id: &str) -> bool {
        let Some(spawn_needed) = self.mark_reconnect_needed(device_id) else {
            return false;
        };
        if spawn_needed {
            self.spawn_reconnect_loop(device_id);
        }
        true
    }

    /// React to a dropped link: drop the dead controller from the pool and
    /// ensure a reconnect loop is retrying.
    fn handle_disconnect(self: &Arc<Self>, device_id: &str) {
        // Mark reconnecting BEFORE evicting, so `controller_for` never observes
        // an evicted-but-not-yet-reconnecting host — which it would try to
        // re-dial synchronously, blocking the (possibly UI-thread) caller
        // against the offline host.
        let Some(spawn_needed) = self.mark_reconnect_needed(device_id) else {
            return;
        };
        let old = self
            .connections
            .lock()
            .ok()
            .and_then(|mut connections| connections.remove(device_id));
        if let Some(old) = old {
            Self::shutdown_controller(old);
        }
        if spawn_needed {
            self.spawn_reconnect_loop(device_id);
        }
    }

    /// Retry `connect_saved` with capped exponential backoff until the link is
    /// re-established or the host is forgotten. The fresh controller is wired to
    /// the same state handler, so a later drop is caught again.
    async fn reconnect_loop(self: Arc<Self>, device_id: String) {
        let mut delay = Duration::from_secs(1);
        let mut attempt: u32 = 0;
        loop {
            let Some(host) = self.store.find(&device_id) else {
                // Forgotten while retrying: stop and forget the link state too.
                if let Ok(mut links) = self.links.lock() {
                    links.remove(&device_id);
                }
                self.clear_path(&device_id);
                break;
            };
            attempt += 1;
            self.set_connecting_link(&device_id);
            // No path while dialing — clear the stale direct/relay marker so the
            // badge doesn't claim a route the (dropped) link no longer has.
            self.clear_path(&device_id);
            // Trace every attempt: a stuck/looping reconnect was previously
            // invisible in the runtime log (one hung dial looked identical to
            // "never tried"). The error string from `connect_saved` names the
            // real failure (relay online timeout, dial timeout, offline host).
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!("controller_connect attempt device={device_id} n={attempt}"),
            );
            let epoch = self.next_connection_epoch(&device_id);
            let on_state = self.state_handler(&device_id, epoch);
            match RemoteController::connect_saved(&host, on_state).await {
                Ok(controller) => {
                    let old = if let Ok(mut connections) = self.connections.lock() {
                        connections.insert(device_id.clone(), Arc::new(controller))
                    } else {
                        None
                    };
                    if let Some(old) = old {
                        Self::shutdown_controller(old);
                    }
                    if let Ok(mut errors) = self.last_errors.lock() {
                        errors.remove(&device_id);
                    }
                    self.set_link(&device_id, ControllerLinkState::Connected);
                    crate::runtime_trace::runtime_trace(
                        "remote",
                        &format!("controller_connect ok device={device_id} n={attempt}"),
                    );
                    break;
                }
                Err(error) => {
                    crate::runtime_trace::runtime_trace(
                        "remote",
                        &format!(
                            "controller_connect failed device={device_id} n={attempt} retry_in_ms={} error={error}",
                            delay.as_millis()
                        ),
                    );
                    // Record why the reconnect failed so `controller_for` can
                    // surface it (offline host, relay unreachable, dial timeout);
                    // otherwise the UI only ever sees the generic "not ready".
                    if let Ok(mut errors) = self.last_errors.lock() {
                        errors.insert(device_id.clone(), error);
                    }
                    self.set_link(&device_id, ControllerLinkState::Disconnected);
                    tokio::time::sleep(delay).await;
                    delay = next_reconnect_delay(delay);
                }
            }
        }
        if let Ok(mut reconnecting) = self.reconnecting.lock() {
            reconnecting.remove(&device_id);
        }
    }
}

fn next_reconnect_delay(delay: Duration) -> Duration {
    (delay * 2).min(RECONNECT_MAX_DELAY)
}

pub struct RemoteControllerManager {
    shared: Arc<ManagerShared>,
}

impl RemoteControllerManager {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            shared: Arc::new(ManagerShared {
                store: RemoteControllerStore::new(support_dir),
                connections: Mutex::new(HashMap::new()),
                connection_epochs: Mutex::new(HashMap::new()),
                links: Mutex::new(HashMap::new()),
                paths: Mutex::new(HashMap::new()),
                reconnecting: Mutex::new(HashSet::new()),
                last_errors: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn saved_hosts(&self) -> Vec<SavedRemoteHost> {
        self.shared.store.list()
    }

    /// Per-device link states for the UI (project connection badge + terminal
    /// re-attach trigger). Only devices that have been connected at least once
    /// appear; a device absent here has not been reached yet.
    pub fn link_states(&self) -> HashMap<String, ControllerLinkState> {
        self.shared
            .links
            .lock()
            .map(|links| links.clone())
            .unwrap_or_default()
    }

    /// Per-device link path types (direct/relay) for the connection badge. Only
    /// present while a device is connected and the transport has selected a path.
    pub fn link_paths(&self) -> HashMap<String, ControllerLinkPath> {
        self.shared
            .paths
            .lock()
            .map(|paths| paths.clone())
            .unwrap_or_default()
    }

    /// Pair with a host from a pasted ticket, persist it, and cache the live
    /// connection (wired for liveness) so the new device is immediately usable.
    pub fn pair(&self, ticket_input: &str, device_name: &str) -> Result<SavedRemoteHost, String> {
        let ticket = parse_pairing_ticket(ticket_input)?;
        let device_id = new_device_id();
        let epoch = self.shared.next_connection_epoch(&device_id);
        let on_state = self.shared.state_handler(&device_id, epoch);
        let (controller, saved) = crate::async_runtime::block_on(RemoteController::pair(
            &ticket,
            device_name,
            device_id,
            on_state,
        ))?;
        self.shared.store.upsert(saved.clone())?;
        let old = self
            .shared
            .connections
            .lock()
            .ok()
            .and_then(|mut connections| {
                connections.insert(saved.device_id.clone(), Arc::new(controller))
            });
        if let Some(old) = old {
            ManagerShared::shutdown_controller(old);
        }
        self.shared
            .set_link(&saved.device_id, ControllerLinkState::Connected);
        Ok(saved)
    }

    /// Get the live controller for a paired device, or report it unavailable.
    ///
    /// This NEVER dials synchronously. The connect can take seconds (offline
    /// host → dial timeout; slow holepunch), and callers run on the UI thread
    /// (a project switch loading worktrees/git) or the blocking pool — a
    /// synchronous dial there freezes the UI and exhausts the pool (the "busy"
    /// indicator). Instead we hand off to a background connect/reconnect loop and
    /// report unavailable now; the desktop's link-state poll refreshes the
    /// project once the loop establishes the link.
    pub fn controller_for(&self, device_id: &str) -> Result<Arc<RemoteController>, String> {
        if let Ok(connections) = self.shared.connections.lock() {
            if let Some(controller) = connections.get(device_id).cloned() {
                return Ok(controller);
            }
        }
        if self.shared.ensure_reconnect_loop(device_id) {
            match self.shared.last_error(device_id) {
                Some(error) => Err(format!(
                    "Remote host {device_id} is connecting; not ready yet (last attempt failed: {error})."
                )),
                None => Err(format!(
                    "Remote host {device_id} is connecting; not ready yet."
                )),
            }
        } else {
            Err(format!("No saved remote host for device {device_id}."))
        }
    }

    /// Like [`controller_for`](Self::controller_for), but waits (bounded) for a
    /// not-yet-connected host to come up instead of failing immediately.
    ///
    /// `controller_for` never blocks because it's called from latency-sensitive
    /// and UI-thread paths. But the add-project file browser and its "new
    /// folder"/create run on the blocking pool in response to an explicit user
    /// action, where a short wait (with the picker showing its loading state) is
    /// exactly right — it turns the "first click shows nothing, second click
    /// works" race (the first call only *kicks off* the background reconnect)
    /// into a single successful call. NEVER call this from the UI thread.
    pub fn controller_for_blocking(
        &self,
        device_id: &str,
        timeout: Duration,
    ) -> Result<Arc<RemoteController>, String> {
        // Already connected (re-browsing the same host): no wait.
        if let Ok(connections) = self.shared.connections.lock() {
            if let Some(controller) = connections.get(device_id).cloned() {
                return Ok(controller);
            }
        }
        // Kick off (or join) the background reconnect loop, then poll the pool
        // until it lands the link or we hit the deadline.
        if !self.shared.ensure_reconnect_loop(device_id) {
            return Err(format!("No saved remote host for device {device_id}."));
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(connections) = self.shared.connections.lock() {
                if let Some(controller) = connections.get(device_id).cloned() {
                    return Ok(controller);
                }
            }
            if Instant::now() >= deadline {
                // Surface the real reconnect failure (offline host, relay
                // unreachable, dial timeout) so the picker shows *why*.
                return Err(match self.shared.last_error(device_id) {
                    Some(error) => format!(
                        "Remote host {device_id} is not reachable yet (last attempt failed: {error})."
                    ),
                    None => format!("Remote host {device_id} is still connecting; try again."),
                });
            }
            std::thread::sleep(Duration::from_millis(120));
        }
    }

    /// Drop a paired host and any live connection or link state for it.
    pub fn forget(&self, device_id: &str) -> Result<Vec<SavedRemoteHost>, String> {
        let old = self
            .shared
            .connections
            .lock()
            .ok()
            .and_then(|mut connections| connections.remove(device_id));
        if let Some(old) = old {
            ManagerShared::shutdown_controller(old);
        }
        if let Ok(mut epochs) = self.shared.connection_epochs.lock() {
            epochs.remove(device_id);
        }
        if let Ok(mut links) = self.shared.links.lock() {
            links.remove(device_id);
        }
        self.shared.store.remove(device_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_delay_caps_at_background_ceiling() {
        let mut delay = Duration::from_secs(1);
        for _ in 0..10 {
            delay = next_reconnect_delay(delay);
        }
        assert_eq!(delay, RECONNECT_MAX_DELAY);
    }
}
