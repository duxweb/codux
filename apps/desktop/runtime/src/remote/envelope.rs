use super::RemoteService;
use super::crypto::{remote_e2e_decrypt, remote_e2e_encrypt, remote_e2e_symmetric_key};
use super::remote_settings_from_raw;
use super::sequence::RemoteSequenceGuard;
use super::types::{RemoteEnvelope, RemoteOutgoingEnvelope};
use serde_json::{Value, json};
use std::collections::HashMap;

impl RemoteService {
    pub fn parse_incoming_envelope(&self, text: &str) -> Result<RemoteEnvelope, String> {
        serde_json::from_str::<RemoteEnvelope>(text).map_err(|error| error.to_string())
    }

    pub(crate) fn decrypt_envelope_if_needed(
        &self,
        envelope: RemoteEnvelope,
        receive_sequence_by_device: &mut HashMap<String, RemoteSequenceGuard>,
    ) -> Result<Option<RemoteEnvelope>, String> {
        if envelope.kind != "secure.message" {
            return Ok(Some(envelope));
        }
        let device_id = match envelope.device_id.clone() {
            Some(device_id) if !device_id.trim().is_empty() => device_id,
            _ => return Ok(None),
        };
        let plaintext = self.decrypt_device_payload(&device_id, &envelope.payload)?;
        let mut inner = serde_json::from_slice::<RemoteEnvelope>(&plaintext)
            .map_err(|error| error.to_string())?;
        if inner.seq.is_some() {
            let guard = receive_sequence_by_device
                .entry(device_id.clone())
                .or_default();
            if !guard.accept(&inner.kind, inner.session_id.as_deref(), inner.seq) {
                return Ok(None);
            }
        }
        inner.device_id = Some(device_id);
        Ok(Some(inner))
    }

    pub fn encrypted_outgoing_envelope(
        &self,
        mut inner: RemoteOutgoingEnvelope,
        send_seq_by_device: &mut HashMap<String, i64>,
    ) -> Result<RemoteOutgoingEnvelope, String> {
        let Some(device_id) = inner
            .device_id
            .clone()
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(inner);
        };
        let seq = send_seq_by_device.get(&device_id).copied().unwrap_or(0) + 1;
        send_seq_by_device.insert(device_id.clone(), seq);
        inner.seq = Some(seq);
        let session_id = inner.session_id.clone();
        let plaintext = serde_json::to_vec(&inner).map_err(|error| error.to_string())?;
        let payload = self.encrypt_device_payload(&device_id, &plaintext)?;
        Ok(RemoteOutgoingEnvelope {
            kind: "secure.message".to_string(),
            device_id: Some(device_id),
            session_id,
            seq: None,
            payload,
        })
    }

    pub fn outgoing_transport_text(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
        send_seq_by_device: &mut HashMap<String, i64>,
    ) -> Option<String> {
        let inner = RemoteOutgoingEnvelope {
            kind: kind.to_string(),
            device_id: device_id.map(str::to_string),
            session_id: session_id.map(str::to_string),
            seq: None,
            payload,
        };
        let envelope = self
            .encrypted_outgoing_envelope(inner, send_seq_by_device)
            .unwrap_or_else(|_| RemoteOutgoingEnvelope {
                kind: "secure.required".to_string(),
                device_id: device_id.map(str::to_string),
                session_id: session_id.map(str::to_string),
                seq: None,
                payload: json!({
                    "message": "End-to-end encryption is required. Please pair this mobile device again."
                }),
            });
        serde_json::to_string(&envelope).ok()
    }

    pub fn encrypt_device_payload(
        &self,
        device_id: &str,
        plaintext: &[u8],
    ) -> Result<Value, String> {
        let device_id = device_id.trim();
        if device_id.is_empty() {
            return Err("Missing device id.".to_string());
        }
        let settings = remote_settings_from_raw(&self.raw_settings());
        let device = settings
            .cached_devices
            .iter()
            .find(|device| device.id == device_id && !device.public_key.trim().is_empty())
            .ok_or_else(|| "Missing device encryption key.".to_string())?;
        let key = remote_e2e_symmetric_key(
            &settings.host_private_key,
            &device.public_key,
            &settings.host_id,
            device_id,
        )?;
        remote_e2e_encrypt(plaintext, &key, &settings.host_id, device_id)
    }

    pub fn decrypt_device_payload(
        &self,
        device_id: &str,
        payload: &Value,
    ) -> Result<Vec<u8>, String> {
        let device_id = device_id.trim();
        if device_id.is_empty() {
            return Err("Missing device id.".to_string());
        }
        let settings = remote_settings_from_raw(&self.raw_settings());
        let device = settings
            .cached_devices
            .iter()
            .find(|device| device.id == device_id && !device.public_key.trim().is_empty())
            .ok_or_else(|| "Missing device encryption key.".to_string())?;
        let key = remote_e2e_symmetric_key(
            &settings.host_private_key,
            &device.public_key,
            &settings.host_id,
            device_id,
        )?;
        remote_e2e_decrypt(payload, &key, &settings.host_id, device_id)
    }
}
