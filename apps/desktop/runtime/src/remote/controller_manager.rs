//! Pools live `RemoteController` connections to paired hosts, keyed by the
//! device id we minted during pairing. Lazily connects from the saved-host
//! store on first use, and bridges the async controller API into the
//! synchronous `RuntimeService` domain methods via `async_runtime::block_on`.
//!
//! Each connection is wired to the controller transport's link-state callback,
//! so a dropped iroh link is detected, the dead controller is evicted, and a
//! backoff reconnect loop is spawned. The desktop polls [`link_states`] to drive
//! the per-project connection badge and to re-attach terminals on recovery.

use super::controller::{new_device_id, parse_pairing_ticket, RemoteController};
use super::controller_store::{RemoteControllerStore, SavedRemoteHost};
use codux_remote_transport::RemoteTransportStateHandler;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// Shared manager state captured (weakly) by transport callbacks and the
/// reconnect loop, so a dropped link can evict + reconnect without a reference
/// back to the owning `RuntimeService`.
struct ManagerShared {
    store: RemoteControllerStore,
    connections: Mutex<HashMap<String, Arc<RemoteController>>>,
    links: Mutex<HashMap<String, ControllerLinkState>>,
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

    fn last_error(&self, device_id: &str) -> Option<String> {
        self.last_errors
            .lock()
            .ok()
            .and_then(|errors| errors.get(device_id).cloned())
    }

    /// Build the transport state handler for `device_id`: it records every link
    /// transition and, on a drop, evicts the dead controller and kicks off a
    /// background reconnect. Holds only a `Weak` so a forgotten manager lets the
    /// callback become a no-op.
    fn state_handler(self: &Arc<Self>, device_id: &str) -> RemoteTransportStateHandler {
        let weak = Arc::downgrade(self);
        let device_id = device_id.to_string();
        Arc::new(move |_node: String, state: String| {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            let mapped = ControllerLinkState::from_transport_state(&state);
            shared.set_link(&device_id, mapped);
            if mapped == ControllerLinkState::Disconnected {
                shared.handle_disconnect(&device_id);
            }
        })
    }

    /// Ensure a background connect/reconnect loop is retrying this device.
    /// Idempotent: marks the device as reconnecting (so `controller_for`
    /// fast-fails instead of re-dialling on the calling thread) and spawns the
    /// loop only if one isn't already running. Returns `false` if the host has
    /// been forgotten — nothing to connect to.
    fn ensure_reconnect_loop(self: &Arc<Self>, device_id: &str) -> bool {
        {
            let mut reconnecting = match self.reconnecting.lock() {
                Ok(guard) => guard,
                Err(_) => return false,
            };
            // The saved host is the source of truth for "should we reconnect":
            // a forgotten device must not be resurrected.
            if self.store.find(device_id).is_none() {
                return false;
            }
            if reconnecting.contains(device_id) {
                return true;
            }
            reconnecting.insert(device_id.to_string());
        }
        let shared = Arc::clone(self);
        let device_id = device_id.to_string();
        crate::async_runtime::spawn(async move {
            shared.reconnect_loop(device_id).await;
        });
        true
    }

    /// React to a dropped link: drop the dead controller from the pool and
    /// ensure a reconnect loop is retrying.
    fn handle_disconnect(self: &Arc<Self>, device_id: &str) {
        // Mark reconnecting BEFORE evicting, so `controller_for` never observes
        // an evicted-but-not-yet-reconnecting host — which it would try to
        // re-dial synchronously, blocking the (possibly UI-thread) caller
        // against the offline host.
        if !self.ensure_reconnect_loop(device_id) {
            return;
        }
        if let Ok(mut connections) = self.connections.lock() {
            connections.remove(device_id);
        }
    }

    /// Retry `connect_saved` with capped exponential backoff until the link is
    /// re-established or the host is forgotten. The fresh controller is wired to
    /// the same state handler, so a later drop is caught again.
    async fn reconnect_loop(self: Arc<Self>, device_id: String) {
        let mut delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(15);
        loop {
            let Some(host) = self.store.find(&device_id) else {
                // Forgotten while retrying: stop and forget the link state too.
                if let Ok(mut links) = self.links.lock() {
                    links.remove(&device_id);
                }
                break;
            };
            self.set_link(&device_id, ControllerLinkState::Connecting);
            let on_state = self.state_handler(&device_id);
            match RemoteController::connect_saved(&host, on_state).await {
                Ok(controller) => {
                    if let Ok(mut connections) = self.connections.lock() {
                        connections.insert(device_id.clone(), Arc::new(controller));
                    }
                    if let Ok(mut errors) = self.last_errors.lock() {
                        errors.remove(&device_id);
                    }
                    self.set_link(&device_id, ControllerLinkState::Connected);
                    break;
                }
                Err(error) => {
                    // Record why the reconnect failed so `controller_for` can
                    // surface it (offline host, relay unreachable, dial timeout);
                    // otherwise the UI only ever sees the generic "not ready".
                    if let Ok(mut errors) = self.last_errors.lock() {
                        errors.insert(device_id.clone(), error);
                    }
                    self.set_link(&device_id, ControllerLinkState::Disconnected);
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(max_delay);
                }
            }
        }
        if let Ok(mut reconnecting) = self.reconnecting.lock() {
            reconnecting.remove(&device_id);
        }
    }
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
                links: Mutex::new(HashMap::new()),
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

    /// Pair with a host from a pasted ticket, persist it, and cache the live
    /// connection (wired for liveness) so the new device is immediately usable.
    pub fn pair(&self, ticket_input: &str, device_name: &str) -> Result<SavedRemoteHost, String> {
        let ticket = parse_pairing_ticket(ticket_input)?;
        let device_id = new_device_id();
        let on_state = self.shared.state_handler(&device_id);
        let (controller, saved) = crate::async_runtime::block_on(RemoteController::pair(
            &ticket,
            device_name,
            device_id,
            on_state,
        ))?;
        self.shared.store.upsert(saved.clone())?;
        if let Ok(mut connections) = self.shared.connections.lock() {
            connections.insert(saved.device_id.clone(), Arc::new(controller));
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
                None => Err(format!("Remote host {device_id} is connecting; not ready yet.")),
            }
        } else {
            Err(format!("No saved remote host for device {device_id}."))
        }
    }

    /// Drop a paired host and any live connection or link state for it.
    pub fn forget(&self, device_id: &str) -> Result<Vec<SavedRemoteHost>, String> {
        if let Ok(mut connections) = self.shared.connections.lock() {
            connections.remove(device_id);
        }
        if let Ok(mut links) = self.shared.links.lock() {
            links.remove(device_id);
        }
        self.shared.store.remove(device_id)
    }
}
