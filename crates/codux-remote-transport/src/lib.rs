use async_trait::async_trait;
pub use codux_protocol::RemoteTransportKind;
use codux_protocol::RemoteTransportPairingRequest;
#[cfg(target_os = "android")]
use std::ffi::c_void;
#[cfg(target_os = "android")]
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::Once;
#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicBool, Ordering};

mod control_messages;
mod iroh_link;
mod local_memory;
mod url_rules;
mod web_tunnel;

pub use iroh_link::{CODUX_REMOTE_ALPN, RemoteIrohControllerTransport, RemoteIrohHostTransport};
pub use local_memory::{LocalMemoryTransport, LocalMemoryTransportHub};
pub use web_tunnel::{
    CODUX_WEB_TUNNEL_ALPN, WebTunnelIoStream, WebTunnelResponse, WebTunnelTcpConnectRequest,
};

pub use url_rules::{
    DEFAULT_RELAY_SERVER_URL, GLOBAL_IROH_RELAY_SERVER_URL, GLOBAL_RELAY_SERVER_URL,
    RemoteRelayPreset, iroh_relay_preset_for_url, iroh_relay_url_for_preset,
    normalize_remote_relay_preset, preferred_controller_transport_kind,
    preferred_pairing_transport_kind, remote_relay_preset_for_url, remote_relay_presets,
    remote_relay_presets_json, remote_relay_url, remote_relay_url_for_preset, remote_url,
};

pub type RemoteTransportMessageHandler = Arc<dyn Fn(String, Vec<u8>) + Send + Sync + 'static>;
pub type RemoteTransportUploadHandler =
    Arc<dyn Fn(RemoteTransportUpload) -> Result<(), String> + Send + Sync + 'static>;
pub type RemoteTransportStateHandler = Arc<dyn Fn(String, String) + Send + Sync + 'static>;
pub type RemoteTransportPairingHandler =
    Arc<dyn Fn(RemoteTransportPairingRequest) -> Option<serde_json::Value> + Send + Sync + 'static>;
pub type RemoteTransportAuthorizationHandler =
    Arc<dyn Fn(&str, &str) -> bool + Send + Sync + 'static>;
pub type RemoteTransportLogHandler = Arc<dyn Fn(String) + Send + Sync + 'static>;
pub type WebTunnelTcpConnectHandler =
    Arc<dyn Fn(WebTunnelTcpConnectRequest) -> Result<(), String> + Send + Sync + 'static>;

#[derive(Clone)]
pub struct RemoteHostTransportHandlers {
    pub on_message: RemoteTransportMessageHandler,
    pub on_upload: RemoteTransportUploadHandler,
    pub on_state: RemoteTransportStateHandler,
    pub on_pairing: RemoteTransportPairingHandler,
    pub on_authorize: RemoteTransportAuthorizationHandler,
    pub on_web_tunnel_tcp_connect: Option<WebTunnelTcpConnectHandler>,
    pub on_log: Option<RemoteTransportLogHandler>,
}

#[cfg(target_os = "android")]
static ANDROID_JNI_CONTEXT_INSTALLED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "android")]
pub unsafe fn install_android_jni_context(
    java_vm: *mut c_void,
    application_context: *mut c_void,
) -> Result<(), String> {
    if java_vm.is_null() {
        return Err("missing Android JavaVM".to_string());
    }
    if application_context.is_null() {
        return Err("missing Android application context".to_string());
    }
    if ANDROID_JNI_CONTEXT_INSTALLED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        iroh_dns::install_android_jni_context(java_vm, application_context);
    }));
    if result.is_err() {
        ANDROID_JNI_CONTEXT_INSTALLED.store(false, Ordering::SeqCst);
        return Err("failed to initialize Android JNI context for Iroh DNS".to_string());
    }
    Ok(())
}

#[async_trait]
pub trait RemoteTransport: Send + Sync {
    fn kind(&self) -> RemoteTransportKind;
    fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool;
    fn send_terminal(&self, data: Vec<u8>, device_id: Option<&str>) -> bool {
        self.send(data, device_id)
    }
    fn send_terminal_upload(&self, _upload: RemoteTransportUpload) -> bool {
        false
    }
    fn set_device_credentials(&self, _device_id: &str, _device_token: &str) -> bool {
        false
    }
    fn iroh_candidate(&self) -> Option<(String, String)> {
        None
    }
    fn iroh_endpoint_ticket(&self) -> Option<String> {
        None
    }
    /// Add bytes to the local blob store and return a shareable ticket; the peer
    /// fetches them with [`RemoteTransport::fetch_blob`]. Binary-safe,
    /// content-addressed transfer over iroh-blobs — the same path the terminal
    /// file upload uses, generalized for any file byte transfer.
    async fn publish_blob(&self, _bytes: Vec<u8>) -> Result<String, String> {
        Err("blob transfer is not supported by this transport".to_string())
    }
    /// Download the bytes a peer published under `ticket`.
    async fn fetch_blob(&self, _ticket: &str) -> Result<Vec<u8>, String> {
        Err("blob transfer is not supported by this transport".to_string())
    }
    /// Open a transparent TCP stream from the controller-side browser to a
    /// host-side target using the host's DNS, hosts file, VPN, and LAN view.
    async fn web_tunnel_tcp_connect(
        &self,
        _request: WebTunnelTcpConnectRequest,
    ) -> Result<Box<dyn WebTunnelIoStream>, String> {
        Err("web tunnel TCP connect is not supported by this transport".to_string())
    }
    async fn shutdown(&self);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteTransportUpload {
    pub device_id: String,
    pub session_id: String,
    pub name: String,
    pub mime: String,
    pub kind: String,
    pub bytes: Vec<u8>,
    pub ticket: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteControllerTransportConfig {
    pub relay_url: String,
    pub host_id: String,
    pub device_id: String,
    pub device_token: String,
    pub transports: Vec<RemoteTransportCandidate>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteTransportCandidate {
    pub kind: String,
    pub url: String,
    pub node_id: String,
    pub relay_url: String,
    pub ticket: String,
    pub relay_authentication: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteHostTransportConfig {
    pub relay_url: String,
    pub relay_preset: String,
    pub iroh_relay_url: String,
    pub iroh_relay_authentication: String,
    pub host_id: String,
    pub host_token: String,
}

pub struct RemoteTransportFactory;

impl RemoteTransportFactory {
    pub async fn connect_host(
        config: &RemoteHostTransportConfig,
        handlers: RemoteHostTransportHandlers,
    ) -> Result<Arc<dyn RemoteTransport>, String> {
        install_rustls_crypto_provider();
        RemoteIrohHostTransport::connect(config, handlers)
            .await
            .map(|transport| transport as Arc<dyn RemoteTransport>)
    }

    pub async fn connect_controller(
        config: &RemoteControllerTransportConfig,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
        on_log: Option<RemoteTransportLogHandler>,
    ) -> Result<Arc<dyn RemoteTransport>, String> {
        install_rustls_crypto_provider();
        let kind = preferred_controller_transport_kind(
            config
                .transports
                .iter()
                .map(|candidate| (candidate.kind.as_str(), candidate.url.as_str())),
        );
        if kind != codux_protocol::REMOTE_TRANSPORT_IROH {
            return Err("missing iroh controller transport candidate".to_string());
        }
        RemoteIrohControllerTransport::connect(config, on_message, on_state, on_log)
            .await
            .map(|transport| transport as Arc<dyn RemoteTransport>)
    }
}

fn install_rustls_crypto_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(test)]
mod tests;
