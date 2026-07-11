use codux_protocol::{
    REMOTE_TRANSPORT_PING, REMOTE_TRANSPORT_PONG, RemoteEnvelope, RemoteOutgoingEnvelope,
    RemoteTransportPairingRequest,
};
use serde_json::Value;

pub(crate) fn transport_pong_for_ping(
    envelope: &RemoteEnvelope,
    fallback_device_id: Option<&str>,
) -> Option<String> {
    if envelope.kind != REMOTE_TRANSPORT_PING {
        return None;
    }
    let device_id = envelope
        .device_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fallback_device_id.filter(|value| !value.trim().is_empty()))
        .map(str::to_string);
    serde_json::to_string(&RemoteOutgoingEnvelope {
        kind: REMOTE_TRANSPORT_PONG.to_string(),
        device_id,
        session_id: None,
        request_id: envelope.request_id.clone(),
        seq: None,
        payload: envelope.payload.clone(),
    })
    .ok()
}

pub(crate) fn pairing_handshake_from_envelope(
    envelope: &RemoteEnvelope,
) -> Option<RemoteTransportPairingRequest> {
    let envelope_device_id = envelope
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let payload_device_id = envelope
        .payload
        .get("deviceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if envelope_device_id.is_some()
        && payload_device_id.is_some()
        && envelope_device_id != payload_device_id
    {
        return None;
    }
    let device_id = envelope_device_id.or(payload_device_id)?.to_string();
    let device_name = envelope
        .payload
        .get("deviceName")
        .and_then(Value::as_str)
        .unwrap_or("Mobile Device")
        .to_string();
    let platform = envelope
        .payload
        .get("platform")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    Some(RemoteTransportPairingRequest {
        device_id,
        device_name,
        platform,
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
