use super::transport::{
    RemoteTransport, RemoteTransportControlHandler, RemoteTransportKind,
    RemoteTransportMessageHandler, RemoteTransportPairingHandler, RemoteTransportStateHandler,
};
use super::types::{RemoteEnvelope, RemoteTransportPairingRequest};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;

pub(crate) struct RemoteWebSocketHostTransport {
    tx: Mutex<Option<mpsc::UnboundedSender<String>>>,
    on_message: RemoteTransportMessageHandler,
    on_state: RemoteTransportStateHandler,
    on_pairing: RemoteTransportPairingHandler,
    on_control: Option<RemoteTransportControlHandler>,
}

impl RemoteWebSocketHostTransport {
    pub(crate) async fn connect(
        ws_url: String,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
        on_pairing: RemoteTransportPairingHandler,
        on_control: Option<RemoteTransportControlHandler>,
    ) -> Result<Arc<Self>, String> {
        let (socket, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|error| error.to_string())?;
        let (mut write, mut read) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let transport = Arc::new(Self {
            tx: Mutex::new(Some(tx)),
            on_message,
            on_state,
            on_pairing,
            on_control,
        });

        let writer = Arc::clone(&transport);
        crate::async_runtime::spawn(async move {
            while let Some(message) = rx.recv().await {
                if write
                    .send(WebSocketMessage::Text(message.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            writer.close_sender();
        });

        let reader = Arc::clone(&transport);
        crate::async_runtime::spawn(async move {
            while let Some(message) = read.next().await {
                match message {
                    Ok(WebSocketMessage::Text(text)) => {
                        reader.handle_text(text.to_string());
                    }
                    Ok(WebSocketMessage::Close(_)) => break,
                    Ok(_) => {}
                    Err(error) => {
                        crate::runtime_trace::runtime_trace(
                            "remote",
                            &format!("websocket_recv failed error={error}"),
                        );
                        break;
                    }
                }
            }
            reader.close_sender();
            (reader.on_state)(String::new(), "closed".to_string());
        });

        Ok(transport)
    }

    fn handle_text(&self, text: String) {
        let Ok(raw) = serde_json::from_str::<RemoteEnvelope>(&text) else {
            crate::runtime_trace::runtime_trace("remote", "websocket_recv drop reason=decode");
            return;
        };
        if raw.kind == "pairing.request" {
            if let Some(handshake) = pairing_handshake_from_envelope(&raw) {
                (self.on_pairing)(handshake);
            }
        }
        let device_id = raw
            .device_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_default();
        if !device_id.is_empty() {
            (self.on_state)(device_id.clone(), "connected".to_string());
        }
        if self
            .on_control
            .as_ref()
            .map(|handler| handler(device_id.clone(), raw))
            .unwrap_or(false)
        {
            return;
        }
        (self.on_message)(device_id, text.into_bytes());
    }

    fn close_sender(&self) {
        if let Ok(mut tx) = self.tx.lock() {
            *tx = None;
        }
    }
}

#[async_trait]
impl RemoteTransport for RemoteWebSocketHostTransport {
    fn kind(&self) -> RemoteTransportKind {
        RemoteTransportKind::WebSocketRelay
    }

    fn send(&self, data: Vec<u8>, _device_id: Option<&str>) -> bool {
        let Ok(text) = String::from_utf8(data) else {
            return false;
        };
        self.tx
            .lock()
            .ok()
            .and_then(|tx| tx.clone())
            .map(|tx| tx.send(text).is_ok())
            .unwrap_or(false)
    }

    async fn shutdown(&self) {
        self.close_sender();
    }
}

fn pairing_handshake_from_envelope(
    envelope: &RemoteEnvelope,
) -> Option<RemoteTransportPairingRequest> {
    let device_id = envelope
        .device_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            envelope
                .payload
                .get("deviceId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })?;
    let device_name = envelope
        .payload
        .get("deviceName")
        .and_then(Value::as_str)
        .unwrap_or("Mobile Device")
        .to_string();
    let device_public_key = envelope
        .payload
        .get("devicePublicKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some(RemoteTransportPairingRequest {
        device_id,
        device_name,
        device_public_key,
        pairing_id: envelope
            .payload
            .get("pairingId")
            .and_then(Value::as_str)
            .map(str::to_string),
        pairing_code: envelope
            .payload
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_string),
        pairing_secret: envelope
            .payload
            .get("secret")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}
