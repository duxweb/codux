use super::{
    CUSTOM_SPECIES_PREFIX, PET_STATE_DECODE_NAMESPACES, PetCustomPet, PetLegacyRecord,
    PetProgressInfo, PetSnapshot, PetStats, STATS_MODEL_VERSION,
    catalog::{custom_pet_from_dir, custom_pets_dir, load_custom_pets},
    day_index, decode_pet_state_data, default_persona_id, sanitize_claim_species,
    sanitize_custom_name, sanitize_custom_pet, sanitize_species,
};
use chrono::DateTime;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, fs, io, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacPersistedPetState {
    state_version: Option<u32>,
    _stats_model_version: Option<u32>,
    claimed_at: Option<MacDate>,
    species: Option<String>,
    current_identity: Option<MacPetIdentity>,
    custom_name: Option<String>,
    #[serde(rename = "currentHatchTokens")]
    legacy_pre_xp_token_count: Option<i64>,
    current_experience_tokens: Option<i64>,
    current_stats: Option<PetStats>,
    stats_updated_day: Option<MacDate>,
    legacy: Option<Vec<MacPetLegacyRecord>>,
    global_normalized_total_watermark: Option<i64>,
    project_normalized_token_watermarks: Option<HashMap<String, i64>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacPetIdentity {
    kind: Option<String>,
    species: Option<String>,
    custom_pet: Option<MacPetCustomPet>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacPetCustomPet {
    id: String,
    display_name: String,
    description: String,
    spritesheet_path: String,
    directory_name: String,
    #[serde(alias = "sourcePageURL")]
    source_page_url: Option<String>,
    #[serde(alias = "sourceZipURL")]
    source_zip_url: Option<String>,
    installed_at: Option<MacDate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacPetLegacyRecord {
    id: String,
    species: String,
    identity: Option<MacPetIdentity>,
    custom_name: String,
    total_xp: i64,
    stats: PetStats,
    retired_at: MacDate,
}

#[derive(Debug, Clone)]
struct MacDate(i64);

impl<'de> Deserialize<'de> for MacDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        mac_date_from_value(value).ok_or_else(|| serde::de::Error::custom("invalid mac date"))
    }
}

pub(super) fn load_mac_pet_state() -> Option<(PetSnapshot, String)> {
    for path in mac_pet_state_paths() {
        let Ok(data) = fs::read(&path) else {
            continue;
        };
        if let Some(decoded) = decode_pet_state_data(&data, PET_STATE_DECODE_NAMESPACES)
            && let Ok(state) = serde_json::from_slice::<MacPersistedPetState>(&decoded)
        {
            return Some((mac_state_to_snapshot(state), path.display().to_string()));
        }
    }
    None
}

pub(super) fn migrate_mac_custom_pets_if_needed(support_dir: &std::path::Path) {
    let destination = custom_pets_dir(support_dir);
    if destination
        .read_dir()
        .ok()
        .is_some_and(|mut entries| entries.next().is_some())
    {
        return;
    }
    for source in mac_custom_pet_paths() {
        if !source.is_dir() {
            continue;
        }
        if fs::create_dir_all(&destination).is_err() {
            return;
        }
        let Ok(entries) = fs::read_dir(source) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let source_path = entry.path();
            if !source_path.is_dir() || custom_pet_from_dir(source_path.clone(), false).is_none() {
                continue;
            }
            let target = destination.join(entry.file_name());
            if target.exists() {
                continue;
            }
            let _ = copy_dir_all(&source_path, &target);
        }
        if load_custom_pets(support_dir, false).is_empty() {
            continue;
        }
        return;
    }
}

fn mac_pet_state_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let support = crate::runtime_paths::home_dir()
            .join("Library")
            .join("Application Support");
        vec![
            support.join("Codux").join("pet-state.dat"),
            support.join("Codux-dev").join("pet-state.dat"),
            support.join("Codux-debug").join("pet-state.dat"),
            support.join("dmux").join("pet-state.dat"),
            support.join("dmux-dev").join("pet-state.dat"),
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

fn mac_custom_pet_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let support = crate::runtime_paths::home_dir()
            .join("Library")
            .join("Application Support");
        vec![
            support.join("Codux").join("custom-pets"),
            support.join("Codux-dev").join("custom-pets"),
            support.join("Codux-debug").join("custom-pets"),
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

fn mac_date_from_value(value: Value) -> Option<MacDate> {
    const APPLE_REFERENCE_UNIX_SECONDS: i64 = 978_307_200;
    match value {
        Value::Number(number) => {
            let seconds = number.as_f64()?;
            if !seconds.is_finite() {
                return None;
            }
            Some(MacDate(
                APPLE_REFERENCE_UNIX_SECONDS + seconds.round() as i64,
            ))
        }
        Value::String(text) => DateTime::parse_from_rfc3339(&text)
            .ok()
            .map(|date| MacDate(date.timestamp())),
        _ => None,
    }
}

fn mac_state_to_snapshot(state: MacPersistedPetState) -> PetSnapshot {
    let now = super::now_seconds();
    let current_identity = state.current_identity;
    let custom_pet = current_identity
        .as_ref()
        .and_then(mac_identity_custom_pet)
        .and_then(sanitize_custom_pet);
    let species = if custom_pet.is_some() {
        custom_pet
            .as_ref()
            .map(|pet| format!("{CUSTOM_SPECIES_PREFIX}{}", pet.id))
            .unwrap_or_else(|| "voidcat".to_string())
    } else {
        current_identity
            .as_ref()
            .and_then(mac_identity_species)
            .or(state.species)
            .map(|value| sanitize_species(&value))
            .unwrap_or_else(|| "voidcat".to_string())
    };
    let claimed_at = state.claimed_at.map(|date| date.0).or_else(|| {
        let has_legacy_xp = state.legacy_pre_xp_token_count.unwrap_or(0) > 0
            || state.current_experience_tokens.unwrap_or(0) > 0;
        if state.state_version == Some(4) && has_legacy_xp {
            Some(now)
        } else {
            None
        }
    });
    let project_watermarks = state
        .project_normalized_token_watermarks
        .unwrap_or_default();
    let total_normalized_tokens = state
        .global_normalized_total_watermark
        .unwrap_or_else(|| project_watermarks.values().copied().sum())
        .max(0);
    let legacy = state
        .legacy
        .unwrap_or_default()
        .into_iter()
        .filter_map(mac_legacy_record_to_snapshot_record)
        .collect();
    PetSnapshot {
        state_version: state.state_version.unwrap_or(0),
        stats_model_version: STATS_MODEL_VERSION,
        claimed_at,
        species,
        custom_pet,
        custom_name: state.custom_name.unwrap_or_default(),
        current_experience_tokens: state.current_experience_tokens.unwrap_or_default().max(0),
        current_stats: state.current_stats.unwrap_or_default().sanitized(),
        persona_id: default_persona_id(),
        progress: PetProgressInfo::default(),
        stats_updated_day: state.stats_updated_day.map(|date| date.0),
        global_normalized_total_watermark: state
            .global_normalized_total_watermark
            .map(|value| value.max(0)),
        project_normalized_token_watermarks: project_watermarks,
        total_normalized_tokens,
        daily_experience_tokens: 0,
        daily_experience_day: Some(day_index(now)),
        legacy,
        updated_at: now,
    }
}

fn mac_identity_species(identity: &MacPetIdentity) -> Option<String> {
    if identity.kind.as_deref() == Some("custom") {
        return None;
    }
    identity.species.clone()
}

fn mac_identity_custom_pet(identity: &MacPetIdentity) -> Option<PetCustomPet> {
    if identity.kind.as_deref() == Some("custom") {
        return identity.custom_pet.clone().map(mac_custom_pet_to_pet);
    }
    None
}

fn mac_custom_pet_to_pet(pet: MacPetCustomPet) -> PetCustomPet {
    PetCustomPet {
        id: pet.id,
        display_name: pet.display_name,
        description: pet.description,
        spritesheet_path: pet.spritesheet_path,
        directory_name: pet.directory_name,
        spritesheet_data_url: None,
        source_page_url: pet.source_page_url,
        source_zip_url: pet.source_zip_url,
        installed_at: pet.installed_at.map(|date| date.0),
    }
}

fn mac_legacy_record_to_snapshot_record(record: MacPetLegacyRecord) -> Option<PetLegacyRecord> {
    let custom_pet = record
        .identity
        .as_ref()
        .and_then(mac_identity_custom_pet)
        .and_then(sanitize_custom_pet);
    Some(PetLegacyRecord {
        id: if record.id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            record.id
        },
        species: sanitize_claim_species(&record.species, custom_pet.as_ref()),
        custom_pet,
        custom_name: sanitize_custom_name(&record.custom_name),
        total_xp: record.total_xp.max(0),
        stats: record.stats.sanitized(),
        persona_id: default_persona_id(),
        progress: PetProgressInfo::default(),
        retired_at: record.retired_at.0,
    })
}

fn copy_dir_all(source: &PathBuf, destination: &PathBuf) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_all(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_state_migrates_to_current_snapshot_without_resetting_progress() {
        let mac_json = r#"{
          "stateVersion": 8,
          "statsModelVersion": 3,
          "claimedAt": "2026-05-18T10:00:00Z",
          "species": "dragon",
          "currentIdentity": { "kind": "bundled", "species": "dragon" },
          "customName": "Spark",
          "currentExperienceTokens": 1200,
          "currentStats": { "wisdom": 1, "chaos": 2, "night": 3, "stamina": 4, "empathy": 5 },
          "globalNormalizedTotalWatermark": 5000,
          "projectNormalizedTokenWatermarks": { "project-a": 5000 },
          "legacy": []
        }"#;
        let mac = serde_json::from_str::<MacPersistedPetState>(mac_json).unwrap();
        let snapshot = super::super::sanitize_state(mac_state_to_snapshot(mac));
        assert_eq!(snapshot.species, "dragon");
        assert_eq!(snapshot.custom_name, "Spark");
        assert_eq!(snapshot.current_experience_tokens, 1200);
        assert_eq!(snapshot.current_stats.chaos, 2);
        assert_eq!(snapshot.global_normalized_total_watermark, Some(5000));
        assert_eq!(
            snapshot
                .project_normalized_token_watermarks
                .get("project-a"),
            Some(&5000)
        );
    }
}
