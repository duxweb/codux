use super::{PET_STATE_CRYPTO_NAMESPACE, PetSnapshot};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) fn decode_pet_state_data(data: &[u8], namespaces: &[&str]) -> Option<Vec<u8>> {
    for namespace in namespaces {
        if let Some(decoded) = decrypt_pet_state_data(data, namespace) {
            return Some(decoded);
        }
    }
    if serde_json::from_slice::<Value>(data).is_ok() {
        return Some(data.to_vec());
    }
    None
}

fn decrypt_pet_state_data(data: &[u8], namespace: &str) -> Option<Vec<u8>> {
    if data.len() < 28 {
        return None;
    }
    let key = pet_state_cipher_key(namespace);
    let cipher = Aes256Gcm::new(&key);
    cipher
        .decrypt(Nonce::from_slice(&data[..12]), &data[12..])
        .ok()
}

pub(super) fn encode_pet_state_data(snapshot: &PetSnapshot) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(snapshot).map_err(|error| error.to_string())?;
    encrypt_pet_state_data(&json, PET_STATE_CRYPTO_NAMESPACE)
}

fn encrypt_pet_state_data(data: &[u8], namespace: &str) -> Result<Vec<u8>, String> {
    let key = pet_state_cipher_key(namespace);
    let cipher = Aes256Gcm::new(&key);
    let mut nonce_bytes = [0_u8; 12];
    let random = *Uuid::new_v4().as_bytes();
    nonce_bytes.copy_from_slice(&random[..12]);
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), data)
        .map_err(|_| "Failed to encrypt pet state.".to_string())?;
    let mut combined = Vec::with_capacity(nonce_bytes.len() + encrypted.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&encrypted);
    Ok(combined)
}

pub(super) fn pet_state_cipher_key(namespace: &str) -> Key<Aes256Gcm> {
    let material = format!("dmux.pet.state.v2|{namespace}|codux");
    let digest = Sha256::digest(material.as_bytes());
    *Key::<Aes256Gcm>::from_slice(&digest)
}
