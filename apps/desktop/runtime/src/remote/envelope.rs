use super::RemoteService;
use super::types::{RemoteEnvelope, RemoteOutgoingEnvelope};
use serde::Serialize;
use serde_json::Value;
use serde_json::value::RawValue;
use std::collections::HashMap;

impl RemoteService {
    pub fn parse_incoming_envelope(&self, text: &str) -> Result<RemoteEnvelope, String> {
        serde_json::from_str::<RemoteEnvelope>(text).map_err(|error| error.to_string())
    }

    pub fn outgoing_transport_text(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        request_id: Option<&str>,
        payload: Value,
        send_seq_by_device: &mut HashMap<String, i64>,
    ) -> Option<String> {
        let seq = device_id
            .filter(|value| !value.trim().is_empty())
            .map(|device_id| {
                let seq = send_seq_by_device.get(device_id).copied().unwrap_or(0) + 1;
                send_seq_by_device.insert(device_id.to_string(), seq);
                seq
            });
        let envelope = RemoteOutgoingEnvelope {
            kind: kind.to_string(),
            device_id: device_id.map(str::to_string),
            session_id: session_id.map(str::to_string),
            request_id: request_id.map(str::to_string),
            seq,
            payload,
        };
        serde_json::to_string(&envelope).ok()
    }

    /// Like [`Self::outgoing_transport_text`] but takes a payload that was
    /// already serialized once (a [`RawValue`]). A fan-out to N subscribers
    /// then serializes the (potentially large) payload a single time and only
    /// stamps the per-device `seq` into the small envelope wrapper, instead of
    /// cloning + re-serializing the whole payload per device. The wire bytes are
    /// identical to the non-raw path.
    pub fn outgoing_transport_text_raw(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: &RawValue,
        send_seq_by_device: &mut HashMap<String, i64>,
    ) -> Option<String> {
        let seq = device_id
            .filter(|value| !value.trim().is_empty())
            .map(|device_id| {
                let seq = send_seq_by_device.get(device_id).copied().unwrap_or(0) + 1;
                send_seq_by_device.insert(device_id.to_string(), seq);
                seq
            });
        let envelope = RemoteOutgoingEnvelopeRaw {
            kind,
            device_id,
            session_id,
            seq,
            payload,
        };
        serde_json::to_string(&envelope).ok()
    }
}

/// Borrowed mirror of [`RemoteOutgoingEnvelope`] whose payload is pre-serialized
/// JSON, copied verbatim during serialization. Field names/order match exactly
/// so the produced wire bytes are identical.
#[derive(Serialize)]
struct RemoteOutgoingEnvelopeRaw<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none", rename = "deviceId")]
    device_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    session_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seq: Option<i64>,
    payload: &'a RawValue,
}
