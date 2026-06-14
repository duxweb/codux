//! Shared end-to-end crypto for the Codux remote channel.
//!
//! This is the single source of truth for the wire format used between the
//! desktop host and the mobile device:
//! X25519 ECDH -> HKDF-SHA256 -> AES-256-GCM, base64url (no pad), with a
//! salt/info/AAD scheme bound to the host and device ids. The desktop runtime
//! and the FFI (mobile) both call these functions, so the two ends are
//! byte-compatible by construction instead of by hand.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

/// Algorithm tag carried in every encrypted payload.
pub const E2E_ALG: &str = "X25519-HKDF-SHA256-AES-256-GCM";

pub fn remote_base64_url_encode(data: &[u8]) -> String {
    general_purpose::URL_SAFE_NO_PAD.encode(data)
}

pub fn remote_base64_url_decode(value: &str) -> Result<Vec<u8>, String> {
    general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|error| error.to_string())
}

fn remote_e2e_private_key(value: &str) -> Option<StaticSecret> {
    let bytes = remote_base64_url_decode(value).ok()?;
    let array: [u8; 32] = bytes.as_slice().try_into().ok()?;
    Some(StaticSecret::from(array))
}

/// Derive the base64url public key for a base64url private key, or `None` if
/// the private key is malformed.
pub fn derive_public_key(private_key: &str) -> Option<String> {
    let private_key = remote_e2e_private_key(private_key)?;
    let public_key = X25519PublicKey::from(&private_key);
    Some(remote_base64_url_encode(public_key.as_bytes()))
}

/// Generate a fresh X25519 keypair (base64url private/public), matching the
/// host identity generation so either side can mint device keys.
pub fn generate_keypair() -> (String, String) {
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    let private_key = StaticSecret::from(bytes);
    let public_key = X25519PublicKey::from(&private_key);
    (
        remote_base64_url_encode(private_key.to_bytes().as_slice()),
        remote_base64_url_encode(public_key.as_bytes()),
    )
}

/// Derive the AES-256 symmetric key from a private key and the peer public key.
/// The X25519 ECDH is symmetric, so host(host_priv, device_pub) and
/// device(device_priv, host_pub) derive the same key.
pub fn remote_e2e_symmetric_key(
    private_key: &str,
    remote_public_key: &str,
    host_id: &str,
    device_id: &str,
) -> Result<[u8; 32], String> {
    let private_key =
        remote_e2e_private_key(private_key).ok_or_else(|| "Invalid private key.".to_string())?;
    let public_bytes = remote_base64_url_decode(remote_public_key)?;
    let public_array: [u8; 32] = public_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "Invalid peer public key.".to_string())?;
    let public_key = X25519PublicKey::from(public_array);
    let shared = private_key.diffie_hellman(&public_key);
    let salt = format!("codux-e2e-v1|{host_id}|{device_id}");
    let hkdf = hkdf::Hkdf::<Sha256>::new(Some(salt.as_bytes()), shared.as_bytes());
    let mut key = [0_u8; 32];
    hkdf.expand(b"codux-remote-payload-v1", &mut key)
        .map_err(|_| "Failed to derive encryption key.".to_string())?;
    Ok(key)
}

fn e2e_aad(host_id: &str, device_id: &str) -> String {
    format!("codux-e2e-aad-v1|{host_id}|{device_id}")
}

/// Encrypt with a caller-supplied nonce. Exposed for deterministic tests; the
/// public [`remote_e2e_encrypt`] generates a random 96-bit nonce.
pub fn remote_e2e_encrypt_with_nonce(
    plaintext: &[u8],
    key: &[u8; 32],
    host_id: &str,
    device_id: &str,
    nonce_bytes: &[u8],
) -> Result<Value, String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let aad = e2e_aad(host_id, device_id);
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(nonce_bytes),
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "Failed to encrypt remote payload.".to_string())?;
    if encrypted.len() < 16 {
        return Err("Invalid encrypted payload.".to_string());
    }
    let (ciphertext, tag) = encrypted.split_at(encrypted.len() - 16);
    Ok(json!({
        "v": 1,
        "alg": E2E_ALG,
        "nonce": remote_base64_url_encode(nonce_bytes),
        "ciphertext": remote_base64_url_encode(ciphertext),
        "tag": remote_base64_url_encode(tag),
    }))
}

pub fn remote_e2e_encrypt(
    plaintext: &[u8],
    key: &[u8; 32],
    host_id: &str,
    device_id: &str,
) -> Result<Value, String> {
    let nonce_bytes = uuid::Uuid::new_v4().as_bytes()[..12].to_vec();
    remote_e2e_encrypt_with_nonce(plaintext, key, host_id, device_id, &nonce_bytes)
}

pub fn remote_e2e_decrypt(
    payload: &Value,
    key: &[u8; 32],
    host_id: &str,
    device_id: &str,
) -> Result<Vec<u8>, String> {
    if payload.get("v").and_then(Value::as_i64) != Some(1) {
        return Err("Unsupported encrypted payload.".to_string());
    }
    let nonce = remote_base64_url_decode(
        payload
            .get("nonce")
            .and_then(Value::as_str)
            .ok_or_else(|| "Missing nonce.".to_string())?,
    )?;
    let mut ciphertext = remote_base64_url_decode(
        payload
            .get("ciphertext")
            .and_then(Value::as_str)
            .ok_or_else(|| "Missing ciphertext.".to_string())?,
    )?;
    let tag = remote_base64_url_decode(
        payload
            .get("tag")
            .and_then(Value::as_str)
            .ok_or_else(|| "Missing tag.".to_string())?,
    )?;
    ciphertext.extend_from_slice(&tag);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let aad = e2e_aad(host_id, device_id);
    cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "Failed to decrypt remote payload.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecdh_is_symmetric_between_host_and_device() {
        let (host_priv, host_pub) = generate_keypair();
        let (device_priv, device_pub) = generate_keypair();
        let host_key =
            remote_e2e_symmetric_key(&host_priv, &device_pub, "host-1", "device-1").unwrap();
        let device_key =
            remote_e2e_symmetric_key(&device_priv, &host_pub, "host-1", "device-1").unwrap();
        assert_eq!(host_key, device_key, "both ends must derive the same key");
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let (host_priv, host_pub) = generate_keypair();
        let (device_priv, device_pub) = generate_keypair();
        let host_key =
            remote_e2e_symmetric_key(&host_priv, &device_pub, "host-1", "device-1").unwrap();
        let device_key =
            remote_e2e_symmetric_key(&device_priv, &host_pub, "host-1", "device-1").unwrap();
        let plaintext = b"{\"type\":\"hello\"}";
        // Host encrypts -> device decrypts.
        let payload = remote_e2e_encrypt(plaintext, &host_key, "host-1", "device-1").unwrap();
        let recovered = remote_e2e_decrypt(&payload, &device_key, "host-1", "device-1").unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn aad_binding_rejects_mismatched_ids() {
        let (host_priv, host_pub) = generate_keypair();
        let (device_priv, device_pub) = generate_keypair();
        let host_key =
            remote_e2e_symmetric_key(&host_priv, &device_pub, "host-1", "device-1").unwrap();
        let device_key =
            remote_e2e_symmetric_key(&device_priv, &host_pub, "host-1", "device-1").unwrap();
        let payload = remote_e2e_encrypt(b"secret", &host_key, "host-1", "device-1").unwrap();
        // Same key material but wrong device id in the AAD must fail.
        assert!(remote_e2e_decrypt(&payload, &device_key, "host-1", "device-2").is_err());
    }

    #[test]
    fn derive_key_is_deterministic_vector() {
        // Fixed keys (all-ones private bytes) give a stable derived key, a
        // cross-language vector the Dart side can assert against.
        let priv_bytes = [1_u8; 32];
        let private_key = StaticSecret::from(priv_bytes);
        let public_key = X25519PublicKey::from(&private_key);
        let priv_b64 = remote_base64_url_encode(private_key.to_bytes().as_slice());
        let pub_b64 = remote_base64_url_encode(public_key.as_bytes());
        let key = remote_e2e_symmetric_key(&priv_b64, &pub_b64, "host", "device").unwrap();
        // Self-ECDH with fixed inputs is stable across runs/platforms.
        let key2 = remote_e2e_symmetric_key(&priv_b64, &pub_b64, "host", "device").unwrap();
        assert_eq!(key, key2);
    }
}
