use crate::ai_history::{AIHistoryProjectRequest, AISessionSummary};
use crate::ai_usage_store::AIUsageProjectTotal;
use crate::paths::app_support_dir;
#[cfg(target_os = "macos")]
use crate::paths::home_dir;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use url::Url;
use uuid::Uuid;
use zip::ZipArchive;

const STATE_VERSION: u32 = 8;
const STATS_MODEL_VERSION: u32 = 3;
const STATS_REFRESH_INTERVAL_SECONDS: i64 = 3600;
const DAILY_TARGET_XP: i64 = 40_000_000;
const MAX_LEVEL: i64 = 100;
const TARGET_XP_TO_REACH_LEVEL_100: i64 = DAILY_TARGET_XP * 30;
const MIN_XP_PER_LEVEL: i64 = 2_000_000;
const MAX_XP_PER_LEVEL: i64 = 22_000_000;
const PET_STATE_CRYPTO_NAMESPACE: &str = "codux";
const PET_STATE_DECODE_NAMESPACES: &[&str] = &["codux", "codux-tauri", "prod", "dev"];
const PET_SPECIES: &[&str] = &[
    "voidcat",
    "rusthound",
    "goose",
    "chaossprite",
    "code",
    "sheep",
    "ox",
    "dragon",
    "phoenix",
    "dolphin",
    "penguin",
    "panda",
];

const CUSTOM_SPECIES_PREFIX: &str = "custom:";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PetStats {
    pub wisdom: i64,
    pub chaos: i64,
    pub night: i64,
    pub stamina: i64,
    pub empathy: i64,
}

impl PetStats {
    fn max_value(&self) -> i64 {
        [
            self.wisdom,
            self.chaos,
            self.night,
            self.stamina,
            self.empathy,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    fn sanitized(mut self) -> Self {
        self.wisdom = self.wisdom.max(0);
        self.chaos = self.chaos.max(0);
        self.night = self.night.max(0);
        self.stamina = self.stamina.max(0);
        self.empathy = self.empathy.max(0);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetProjectTokenTotal {
    pub project_id: String,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetRefreshRequest {
    #[serde(rename = "projects", default)]
    pub _projects: Vec<AIHistoryProjectRequest>,
}

#[derive(Debug, Clone)]
pub struct PetRefreshInput {
    pub project_totals: Vec<PetProjectTokenTotal>,
    pub fallback_total_tokens: i64,
    pub computed_stats: PetStats,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetClaimRequest {
    pub species: String,
    pub custom_name: String,
    pub custom_pet: Option<PetCustomPet>,
    #[serde(rename = "projects", default)]
    pub _projects: Vec<AIHistoryProjectRequest>,
}

#[derive(Debug, Clone)]
pub struct PetClaimInput {
    pub species: String,
    pub custom_name: String,
    pub custom_pet: Option<PetCustomPet>,
    pub project_totals: Vec<PetProjectTokenTotal>,
    pub fallback_total_tokens: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetRenameRequest {
    pub custom_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetRestoreRequest {
    pub legacy_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetLegacyRecord {
    pub id: String,
    pub species: String,
    pub custom_pet: Option<PetCustomPet>,
    pub custom_name: String,
    pub total_xp: i64,
    pub stats: PetStats,
    #[serde(default = "default_persona_id")]
    pub persona_id: String,
    #[serde(default)]
    pub progress: PetProgressInfo,
    pub retired_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetSnapshot {
    pub state_version: u32,
    pub stats_model_version: u32,
    pub claimed_at: Option<i64>,
    pub species: String,
    pub custom_pet: Option<PetCustomPet>,
    pub custom_name: String,
    pub current_experience_tokens: i64,
    pub current_stats: PetStats,
    #[serde(default = "default_persona_id")]
    pub persona_id: String,
    #[serde(default)]
    pub progress: PetProgressInfo,
    pub stats_updated_day: Option<i64>,
    pub global_normalized_total_watermark: Option<i64>,
    pub project_normalized_token_watermarks: HashMap<String, i64>,
    pub total_normalized_tokens: i64,
    #[serde(default)]
    pub daily_experience_tokens: i64,
    #[serde(default)]
    pub daily_experience_day: Option<i64>,
    #[serde(default)]
    pub legacy: Vec<PetLegacyRecord>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetProgressInfo {
    pub level: i64,
    pub xp_in_level: i64,
    pub xp_for_level: i64,
    pub total_xp: i64,
    pub progress: f64,
    pub is_at_max_level: bool,
}

impl Default for PetProgressInfo {
    fn default() -> Self {
        pet_progress_info(0)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCatalog {
    pub species: Vec<PetCatalogItem>,
    pub custom_pets: Vec<PetCustomPet>,
    pub atlas: PetAtlasSpec,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCatalogItem {
    pub species: String,
    pub asset_folder: String,
    pub manifest_id: String,
    pub name_key: String,
    pub claim_title_key: String,
    pub subtitle_key: String,
    pub description_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCustomPetInstallRequest {
    pub page_url: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCustomPetInstallPreview {
    pub page_url: String,
    pub zip_url: String,
    pub slug: String,
    pub display_name: String,
    pub description: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PetCustomPet {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub spritesheet_path: String,
    pub directory_name: String,
    pub spritesheet_data_url: Option<String>,
    pub source_page_url: Option<String>,
    pub source_zip_url: Option<String>,
    pub installed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAtlasSpec {
    pub columns: usize,
    pub rows: usize,
    pub cell_width: usize,
    pub cell_height: usize,
    pub animations: Vec<PetAnimationSpec>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAnimationSpec {
    pub state: String,
    pub row: usize,
    pub frame_durations_ms: Vec<u64>,
}

impl Default for PetSnapshot {
    fn default() -> Self {
        Self {
            state_version: STATE_VERSION,
            stats_model_version: STATS_MODEL_VERSION,
            claimed_at: None,
            species: "voidcat".to_string(),
            custom_pet: None,
            custom_name: String::new(),
            current_experience_tokens: 0,
            current_stats: PetStats::default(),
            persona_id: default_persona_id(),
            progress: PetProgressInfo::default(),
            stats_updated_day: None,
            global_normalized_total_watermark: None,
            project_normalized_token_watermarks: HashMap::new(),
            total_normalized_tokens: 0,
            daily_experience_tokens: 0,
            daily_experience_day: None,
            legacy: Vec::new(),
            updated_at: Utc::now().timestamp(),
        }
    }
}

pub struct PetStore {
    state: Mutex<PetSnapshot>,
    state_file: PathBuf,
}

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

impl PetStore {
    pub fn load_or_seed() -> Self {
        let state_file = state_file_path();
        let loaded_state = load_state(&state_file);
        let should_persist_migrated_state = loaded_state.is_some()
            && (!state_file.is_file() || loaded_state_claimed(&loaded_state));
        let state = sanitize_state(loaded_state.unwrap_or_default());
        migrate_mac_custom_pets_if_needed();
        let store = Self {
            state: Mutex::new(state.clone()),
            state_file,
        };
        if should_persist_migrated_state {
            let _ = store.save(&state);
        }
        store
    }

    pub fn catalog() -> PetCatalog {
        PetCatalog {
            species: PET_SPECIES
                .iter()
                .map(|species| PetCatalogItem {
                    species: (*species).to_string(),
                    asset_folder: (*species).to_string(),
                    manifest_id: format!("{species}-default"),
                    name_key: format!("pet.species.{species}.base"),
                    claim_title_key: format!("pet.claim.{species}.title"),
                    subtitle_key: format!("pet.claim.{species}.subtitle"),
                    description_key: format!("pet.claim.{species}.description"),
                })
                .collect(),
            custom_pets: load_custom_pets(true),
            atlas: PetAtlasSpec {
                columns: 8,
                rows: 9,
                cell_width: 192,
                cell_height: 208,
                animations: vec![
                    animation("idle", 0, &[280, 110, 110, 140, 140, 320]),
                    animation(
                        "running-right",
                        1,
                        &[120, 120, 120, 120, 120, 120, 120, 220],
                    ),
                    animation("running-left", 2, &[120, 120, 120, 120, 120, 120, 120, 220]),
                    animation("waving", 3, &[140, 140, 140, 280]),
                    animation("jumping", 4, &[140, 140, 140, 140, 280]),
                    animation("failed", 5, &[140, 140, 140, 140, 140, 140, 140, 240]),
                    animation("waiting", 6, &[150, 150, 150, 150, 150, 260]),
                    animation("running", 7, &[120, 120, 120, 120, 120, 220]),
                    animation("review", 8, &[150, 150, 150, 150, 150, 280]),
                ],
            },
        }
    }

    pub fn refresh(&self, request: PetRefreshInput) -> Result<PetSnapshot, String> {
        self.with_mut_snapshot(|state| {
            refresh_state(state, request);
            Ok(())
        })
    }

    pub fn claim(&self, request: PetClaimInput) -> Result<PetSnapshot, String> {
        self.with_mut_snapshot(|state| {
            if state.claimed_at.is_some() {
                return Err("Pet is already claimed.".to_string());
            }
            let custom_pet = request.custom_pet.and_then(sanitize_custom_pet);
            let species = sanitize_claim_species(&request.species, custom_pet.as_ref());
            let now = Utc::now().timestamp();
            let project_totals = sanitize_project_totals(request.project_totals);
            let fallback_total = request.fallback_total_tokens.max(0);
            let total_normalized_tokens = if project_totals.is_empty() {
                fallback_total
            } else {
                project_totals.values().sum()
            };
            let legacy = std::mem::take(&mut state.legacy);
            *state = PetSnapshot {
                claimed_at: Some(now),
                species,
                custom_pet,
                custom_name: sanitize_custom_name(&request.custom_name),
                persona_id: default_persona_id(),
                progress: PetProgressInfo::default(),
                global_normalized_total_watermark: Some(total_normalized_tokens),
                project_normalized_token_watermarks: project_totals,
                total_normalized_tokens,
                daily_experience_day: Some(day_index(now)),
                legacy,
                updated_at: now,
                ..PetSnapshot::default()
            };
            Ok(())
        })
    }

    pub fn rename(&self, request: PetRenameRequest) -> Result<PetSnapshot, String> {
        self.with_mut_snapshot(|state| {
            if state.claimed_at.is_none() {
                return Err("No pet has been claimed.".to_string());
            }
            state.custom_name = sanitize_custom_name(&request.custom_name);
            state.updated_at = Utc::now().timestamp();
            Ok(())
        })
    }

    pub fn archive_current(&self) -> Result<PetSnapshot, String> {
        self.with_mut_snapshot(|state| {
            let record = legacy_record_from_state(state)
                .ok_or_else(|| "No pet has been claimed.".to_string())?;
            let mut legacy = std::mem::take(&mut state.legacy);
            legacy.insert(0, record);
            let now = Utc::now().timestamp();
            *state = PetSnapshot {
                legacy,
                daily_experience_day: Some(day_index(now)),
                updated_at: now,
                ..PetSnapshot::default()
            };
            Ok(())
        })
    }

    pub fn restore_archived(&self, request: PetRestoreRequest) -> Result<PetSnapshot, String> {
        self.with_mut_snapshot(|state| {
            let index = state
                .legacy
                .iter()
                .position(|record| record.id == request.legacy_id)
                .ok_or_else(|| "Archived pet not found.".to_string())?;
            let mut legacy = std::mem::take(&mut state.legacy);
            let record = legacy.remove(index);
            if let Some(current) = legacy_record_from_state(state) {
                legacy.insert(0, current);
            }
            let now = Utc::now().timestamp();
            *state = PetSnapshot {
                claimed_at: Some(now),
                species: sanitize_species(&record.species),
                custom_pet: record.custom_pet,
                custom_name: record.custom_name,
                current_experience_tokens: record.total_xp.max(0),
                current_stats: record.stats.sanitized(),
                persona_id: record.persona_id,
                progress: record.progress,
                stats_updated_day: Some(now),
                legacy,
                daily_experience_day: Some(day_index(now)),
                updated_at: now,
                ..PetSnapshot::default()
            };
            Ok(())
        })
    }

    pub fn forget_project_baseline(&self, project_id: &str) -> Result<bool, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Pet store lock poisoned.".to_string())?;
        if state
            .project_normalized_token_watermarks
            .remove(project_id)
            .is_none()
        {
            return Ok(false);
        }
        state.global_normalized_total_watermark =
            if state.project_normalized_token_watermarks.is_empty() {
                None
            } else {
                Some(
                    state
                        .project_normalized_token_watermarks
                        .values()
                        .copied()
                        .sum(),
                )
            };
        state.updated_at = Utc::now().timestamp();
        let snapshot = state.clone();
        drop(state);
        self.save(&snapshot)?;
        Ok(true)
    }

    pub fn forget_all_project_baselines(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Pet store lock poisoned.".to_string())?;
        if state.project_normalized_token_watermarks.is_empty()
            && state.global_normalized_total_watermark.is_none()
        {
            return Ok(());
        }
        state.project_normalized_token_watermarks.clear();
        state.global_normalized_total_watermark = None;
        state.updated_at = Utc::now().timestamp();
        let snapshot = state.clone();
        drop(state);
        self.save(&snapshot)
    }

    pub fn snapshot(&self) -> Result<PetSnapshot, String> {
        self.state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| "Pet store lock poisoned.".to_string())
    }

    pub async fn resolve_custom_pet_install(
        request: PetCustomPetInstallRequest,
    ) -> Result<PetCustomPetInstallPreview, String> {
        let raw_url = request.page_url.trim();
        let page_url =
            Url::parse(raw_url).map_err(|_| "Please enter a Petdex pet page URL.".to_string())?;
        validate_petdex_url(&page_url)?;
        let html = reqwest::get(page_url.clone())
            .await
            .map_err(|_| "Failed to load the Petdex page.".to_string())?
            .error_for_status()
            .map_err(|_| "Failed to load the Petdex page.".to_string())?
            .text()
            .await
            .map_err(|_| "Unable to read the Petdex page.".to_string())?;
        let install = install_request_from_html(&html, &page_url)?;
        let display_name = sanitize_custom_display_name(&request.display_name)
            .filter(|name| !name.is_empty())
            .or_else(|| install.display_name.clone())
            .unwrap_or_else(|| install.slug.clone());
        Ok(PetCustomPetInstallPreview {
            page_url: page_url.to_string(),
            zip_url: install.zip_url.to_string(),
            slug: install.slug,
            display_name,
            description: install.description.unwrap_or_default(),
            image_url: install.image_url.map(|url| url.to_string()),
        })
    }

    pub async fn install_custom_pet(
        request: PetCustomPetInstallRequest,
    ) -> Result<PetCustomPet, String> {
        let preview = Self::resolve_custom_pet_install(request).await?;
        let zip_url = Url::parse(&preview.zip_url)
            .map_err(|_| "The Petdex package URL is invalid.".to_string())?;
        let package_id = sanitize_custom_pet_id(&preview.slug);
        if package_id.is_empty() {
            return Err("The Petdex package name is invalid.".to_string());
        }
        let bytes = reqwest::get(zip_url)
            .await
            .map_err(|_| "Failed to download the pet package.".to_string())?
            .error_for_status()
            .map_err(|_| "Failed to download the pet package.".to_string())?
            .bytes()
            .await
            .map_err(|_| "Failed to download the pet package.".to_string())?;
        let staging_dir =
            std::env::temp_dir().join(format!("codux-pet-staging-{}", Uuid::new_v4()));
        let destination = custom_pets_dir().join(&package_id);
        fs::create_dir_all(&staging_dir).map_err(|error| error.to_string())?;
        let _cleanup = StagingCleanup(staging_dir.clone());
        extract_zip_bytes(&bytes, &staging_dir)?;
        let package_dir = find_pet_package_dir(&staging_dir)?;
        custom_pet_from_dir(package_dir.clone(), false).ok_or_else(|| {
            "The downloaded package does not contain a valid pet.json and spritesheet.".to_string()
        })?;
        fs::create_dir_all(custom_pets_dir()).map_err(|error| error.to_string())?;
        if destination.exists() {
            fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
        }
        copy_dir_all(&package_dir, &destination).map_err(|error| error.to_string())?;
        let mut pet = custom_pet_from_dir(destination, true)
            .ok_or_else(|| "Installed pet package could not be verified.".to_string())?;
        pet.display_name = preview.display_name;
        if pet.description.trim().is_empty() {
            pet.description = preview.description;
        }
        pet.source_page_url = Some(preview.page_url);
        pet.source_zip_url = Some(preview.zip_url);
        pet.installed_at = Some(Utc::now().timestamp());
        persist_custom_pet_manifest(&pet)?;
        Ok(pet)
    }

    fn with_mut_snapshot(
        &self,
        apply: impl FnOnce(&mut PetSnapshot) -> Result<(), String>,
    ) -> Result<PetSnapshot, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Pet store lock poisoned.".to_string())?;
        apply(&mut state)?;
        let snapshot = sanitize_state(state.clone());
        *state = snapshot.clone();
        drop(state);
        self.save(&snapshot)?;
        Ok(snapshot)
    }

    fn save(&self, snapshot: &PetSnapshot) -> Result<(), String> {
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let data = encode_pet_state_data(snapshot)?;
        fs::write(&self.state_file, data).map_err(|error| error.to_string())
    }
}

fn load_custom_pets(include_data_url: bool) -> Vec<PetCustomPet> {
    let root = custom_pets_dir();
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut pets = entries
        .filter_map(Result::ok)
        .filter_map(|entry| custom_pet_from_dir(entry.path(), include_data_url))
        .collect::<Vec<_>>();
    pets.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    pets
}

fn custom_pet_from_dir(dir: PathBuf, include_data_url: bool) -> Option<PetCustomPet> {
    if !dir.is_dir() {
        return None;
    }
    let manifest_path = dir.join("pet.json");
    let data = fs::read(manifest_path).ok()?;
    let manifest = serde_json::from_slice::<PetCustomPetManifest>(&data).ok()?;
    let id = sanitize_custom_pet_id(&manifest.id);
    if id.is_empty() {
        return None;
    }
    let spritesheet_path = sanitize_relative_path(&manifest.spritesheet_path)?;
    let spritesheet_file = dir.join(&spritesheet_path);
    if !spritesheet_file.is_file() {
        return None;
    }
    let display_name = manifest.display_name.trim();
    let directory_name = dir.file_name()?.to_string_lossy().to_string();
    Some(PetCustomPet {
        id: id.clone(),
        display_name: if display_name.is_empty() {
            id
        } else {
            display_name.chars().take(64).collect()
        },
        description: manifest.description.trim().chars().take(280).collect(),
        spritesheet_path,
        directory_name,
        spritesheet_data_url: if include_data_url {
            png_data_url(&spritesheet_file)
        } else {
            None
        },
        source_page_url: manifest.source_page_url,
        source_zip_url: manifest.source_zip_url,
        installed_at: manifest.installed_at,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetCustomPetManifest {
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    spritesheet_path: String,
    #[serde(default)]
    source_page_url: Option<String>,
    #[serde(default)]
    source_zip_url: Option<String>,
    #[serde(default)]
    installed_at: Option<i64>,
}

fn persist_custom_pet_manifest(pet: &PetCustomPet) -> Result<(), String> {
    let manifest_path = custom_pets_dir().join(&pet.directory_name).join("pet.json");
    let manifest = PetCustomPetManifest {
        id: pet.id.clone(),
        display_name: pet.display_name.clone(),
        description: pet.description.clone(),
        spritesheet_path: pet.spritesheet_path.clone(),
        source_page_url: pet.source_page_url.clone(),
        source_zip_url: pet.source_zip_url.clone(),
        installed_at: pet.installed_at,
    };
    let data = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    fs::write(manifest_path, data).map_err(|error| error.to_string())
}

fn sanitize_relative_path(path: &str) -> Option<String> {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() || trimmed.starts_with('/') || trimmed.contains("..") {
        return None;
    }
    Some(trimmed)
}

fn png_data_url(path: &PathBuf) -> Option<String> {
    let data = fs::read(path).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(data)
    ))
}

fn sanitize_custom_pet_id(id: &str) -> String {
    id.trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '_'])
        .to_string()
}

struct PetInstallRequestInternal {
    zip_url: Url,
    slug: String,
    display_name: Option<String>,
    description: Option<String>,
    image_url: Option<Url>,
}

struct StagingCleanup(PathBuf);

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn validate_petdex_url(url: &Url) -> Result<(), String> {
    let scheme = url.scheme().to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err("Please enter a Petdex pet page URL.".to_string());
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return Err("Please enter a Petdex pet page URL.".to_string());
    };
    if host == "petdex.crafter.run" || host.ends_with(".petdex.crafter.run") {
        Ok(())
    } else {
        Err("Please enter a Petdex pet page URL.".to_string())
    }
}

fn install_request_from_html(
    html: &str,
    page_url: &Url,
) -> Result<PetInstallRequestInternal, String> {
    let zip_url = extract_zip_url(html)
        .ok_or_else(|| "Unable to find a Petdex package on this page.".to_string())?;
    Ok(PetInstallRequestInternal {
        zip_url,
        slug: pet_slug_from_url(page_url),
        display_name: extract_meta_content(html, "og:title")
            .and_then(|value| value.split(" — ").next().map(str::to_string))
            .or_else(|| extract_jsonld_string(html, "name")),
        description: extract_meta_content(html, "description")
            .or_else(|| extract_jsonld_string(html, "description")),
        image_url: extract_meta_url(html, "og:image", page_url)
            .or_else(|| extract_jsonld_url(html, "image", page_url)),
    })
}

fn pet_slug_from_url(url: &Url) -> String {
    let segments = url
        .path_segments()
        .map(|parts| parts.collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(index) = segments.iter().position(|segment| *segment == "pets") {
        if let Some(slug) = segments.get(index + 1) {
            return (*slug).to_string();
        }
    }
    segments.last().copied().unwrap_or("custom-pet").to_string()
}

fn extract_zip_url(html: &str) -> Option<Url> {
    for marker in ["zipUrl", "zip_url"] {
        if let Some(index) = html.find(marker) {
            if let Some(url) = first_zip_url_after(&html[index..]) {
                return Some(url);
            }
        }
    }
    first_zip_url_after(html)
}

fn first_zip_url_after(text: &str) -> Option<Url> {
    let start = text.find("https://").or_else(|| text.find("http://"))?;
    let tail = &text[start..];
    let end = tail
        .find(|ch: char| ch == '"' || ch == '\'' || ch == '\\' || ch.is_whitespace() || ch == '<')
        .unwrap_or(tail.len());
    let candidate = tail[..end].replace("\\/", "/");
    if !candidate.ends_with(".zip") && !candidate.contains(".zip?") {
        return None;
    }
    Url::parse(&candidate).ok()
}

fn extract_meta_content(html: &str, name: &str) -> Option<String> {
    let needle = format!(r#"name="{name}""#);
    let property = format!(r#"property="{name}""#);
    let index = html.find(&needle).or_else(|| html.find(&property))?;
    extract_attr_value(&html[index..], "content").map(html_unescape)
}

fn extract_meta_url(html: &str, name: &str, base_url: &Url) -> Option<Url> {
    let value = extract_meta_content(html, name)?;
    resolve_url(&value, base_url)
}

fn extract_attr_value(fragment: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=");
    let index = fragment.find(&needle)? + needle.len();
    let quote = fragment[index..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &fragment[index + quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn extract_jsonld_string(html: &str, field: &str) -> Option<String> {
    let marker = format!(r#""{field}""#);
    let index = html.find(&marker)?;
    let tail = &html[index + marker.len()..];
    let colon = tail.find(':')?;
    let tail = tail[colon + 1..].trim_start();
    if tail.starts_with('[') {
        let rest = tail[1..].trim_start();
        if !rest.starts_with('"') {
            return None;
        }
        let rest = &rest[1..];
        let end = rest.find('"')?;
        return Some(html_unescape(&rest[..end]));
    }
    if !tail.starts_with('"') {
        return None;
    }
    let rest = &tail[1..];
    let end = rest.find('"')?;
    Some(html_unescape(&rest[..end]))
}

fn extract_jsonld_url(html: &str, field: &str, base_url: &Url) -> Option<Url> {
    let value = extract_jsonld_string(html, field)?;
    resolve_url(&value, base_url)
}

fn resolve_url(value: &str, base_url: &Url) -> Option<Url> {
    let trimmed = html_unescape(value).trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    Url::parse(&trimmed)
        .ok()
        .or_else(|| base_url.join(&trimmed).ok())
}

fn html_unescape(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn extract_zip_bytes(bytes: &[u8], destination: &PathBuf) -> Result<(), String> {
    let reader = io::Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(reader).map_err(|_| "Failed to unpack the pet package.".to_string())?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| "Failed to unpack the pet package.".to_string())?;
        let Some(path) = file.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let output = destination.join(path);
        if file.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut out = fs::File::create(&output).map_err(|error| error.to_string())?;
        io::copy(&mut file, &mut out)
            .map_err(|_| "Failed to unpack the pet package.".to_string())?;
    }
    Ok(())
}

fn find_pet_package_dir(root: &PathBuf) -> Result<PathBuf, String> {
    if root.join("pet.json").is_file() {
        return Ok(root.clone());
    }
    let entries = fs::read_dir(root).map_err(|error| error.to_string())?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() && path.join("pet.json").is_file() {
            return Ok(path);
        }
    }
    Err("The downloaded package does not contain a valid pet.json and spritesheet.".to_string())
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

fn animation(state: &str, row: usize, frame_durations_ms: &[u64]) -> PetAnimationSpec {
    PetAnimationSpec {
        state: state.to_string(),
        row,
        frame_durations_ms: frame_durations_ms.to_vec(),
    }
}

pub fn refresh_input_from_indexed_history(
    claimed_at: Option<i64>,
    project_totals: Vec<AIUsageProjectTotal>,
    all_time_total_tokens: i64,
    sessions: Vec<AISessionSummary>,
) -> PetRefreshInput {
    PetRefreshInput {
        project_totals: if claimed_at.is_some() {
            project_totals
                .into_iter()
                .map(|item| PetProjectTokenTotal {
                    project_id: item.project_id,
                    total_tokens: item.total_tokens,
                })
                .collect()
        } else {
            Vec::new()
        },
        fallback_total_tokens: all_time_total_tokens.max(0),
        computed_stats: pet_stats_from_sessions(&sessions),
    }
}

pub fn claim_input_from_indexed_history(
    request: PetClaimRequest,
    all_time_total_tokens: i64,
) -> PetClaimInput {
    PetClaimInput {
        species: request.species,
        custom_name: request.custom_name,
        custom_pet: request.custom_pet,
        project_totals: Vec::new(),
        fallback_total_tokens: all_time_total_tokens.max(0),
    }
}

fn refresh_state(state: &mut PetSnapshot, request: PetRefreshInput) {
    let now = Utc::now().timestamp();
    reset_daily_tokens_if_needed(state, now);
    let project_totals = sanitize_project_totals(request.project_totals);
    let fallback_total = request.fallback_total_tokens.max(0);
    let total_normalized_tokens = if project_totals.is_empty() {
        fallback_total
    } else {
        project_totals.values().sum()
    };

    if state.claimed_at.is_none() {
        state.total_normalized_tokens = total_normalized_tokens;
        state.updated_at = now;
        return;
    }

    let delta_tokens = if project_totals.is_empty() {
        let previous = state
            .global_normalized_total_watermark
            .unwrap_or(fallback_total)
            .max(0);
        let delta = (fallback_total - previous).max(0);
        if state.global_normalized_total_watermark.is_none() || fallback_total > previous {
            state.global_normalized_total_watermark = Some(fallback_total);
        }
        delta
    } else {
        let current_project_ids = project_totals.keys().cloned().collect::<Vec<_>>();
        state
            .project_normalized_token_watermarks
            .retain(|project_id, _| current_project_ids.contains(project_id));
        let mut delta = 0;
        for (project_id, total) in &project_totals {
            let previous = state
                .project_normalized_token_watermarks
                .get(project_id)
                .copied()
                .unwrap_or(*total);
            delta += (*total - previous).max(0);
            if *total > previous
                || !state
                    .project_normalized_token_watermarks
                    .contains_key(project_id)
            {
                state
                    .project_normalized_token_watermarks
                    .insert(project_id.clone(), *total);
            }
        }
        state.global_normalized_total_watermark = Some(
            state
                .project_normalized_token_watermarks
                .values()
                .copied()
                .sum(),
        );
        delta
    };

    if delta_tokens > 0 {
        state.current_experience_tokens = state
            .current_experience_tokens
            .saturating_add(delta_tokens)
            .max(0);
        state.daily_experience_tokens = state
            .daily_experience_tokens
            .saturating_add(delta_tokens)
            .max(0);
    }
    apply_stats(state, request.computed_stats.sanitized(), now);
    state.total_normalized_tokens = total_normalized_tokens;
    apply_derived_snapshot_fields(state);
    state.updated_at = now;
}

fn reset_daily_tokens_if_needed(state: &mut PetSnapshot, now: i64) {
    let day = day_index(now);
    if state.daily_experience_day == Some(day) {
        return;
    }
    state.daily_experience_day = Some(day);
    state.daily_experience_tokens = 0;
}

fn day_index(timestamp: i64) -> i64 {
    let Some(date) = Local.timestamp_opt(timestamp, 0).single() else {
        return timestamp.div_euclid(86_400);
    };
    let Some(start) = Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
    else {
        return timestamp.div_euclid(86_400);
    };
    start.timestamp().div_euclid(86_400)
}

fn apply_stats(state: &mut PetSnapshot, computed: PetStats, now: i64) {
    if computed.max_value() <= 0 {
        return;
    }
    if state.stats_updated_day.is_none() || state.current_stats == PetStats::default() {
        state.current_stats = computed;
        state.stats_updated_day = Some(now);
        return;
    }
    let should_damp = state
        .stats_updated_day
        .map(|updated_at| now - updated_at >= STATS_REFRESH_INTERVAL_SECONDS)
        .unwrap_or(true);
    if should_damp && state.current_stats != computed {
        state.current_stats = damp_stats(&state.current_stats, &computed);
        state.stats_updated_day = Some(now);
    }
}

fn damp_stats(current: &PetStats, target: &PetStats) -> PetStats {
    fn damp(current: i64, next: i64) -> i64 {
        let delta = ((next - current) as f64 * 0.25).round() as i64;
        if delta == 0 && current != next {
            return (current + if next > current { 1 } else { -1 }).max(0);
        }
        (current + delta).max(0)
    }
    PetStats {
        wisdom: damp(current.wisdom, target.wisdom),
        chaos: damp(current.chaos, target.chaos),
        night: damp(current.night, target.night),
        stamina: damp(current.stamina, target.stamina),
        empathy: damp(current.empathy, target.empathy),
    }
}

fn pet_stats_from_sessions(sessions: &[AISessionSummary]) -> PetStats {
    if sessions.is_empty() {
        return PetStats::default();
    }

    let total_requests: i64 = sessions.iter().map(|session| session.request_count).sum();
    let total_tokens: i64 = sessions.iter().map(|session| session.total_tokens).sum();
    let total_secs: i64 = sessions
        .iter()
        .map(|session| session.active_duration_seconds)
        .sum();
    let session_count = sessions.len().max(1);

    if sessions.len() < 3 || total_requests < 5 || total_tokens < 20_000 {
        return PetStats {
            wisdom: 100,
            chaos: 100,
            night: 100,
            stamina: 100,
            empathy: 100,
        };
    }

    let avg_tok_per_req = if total_requests > 0 {
        total_tokens as f64 / total_requests as f64
    } else {
        0.0
    };
    let req_per_hour = if total_secs > 0 {
        total_requests as f64 / (total_secs as f64 / 3600.0)
    } else {
        0.0
    };
    let short_count = sessions
        .iter()
        .filter(|session| session.active_duration_seconds < 300)
        .count();
    let night_count = sessions
        .iter()
        .filter(|session| {
            timestamp_hour_local(session.first_seen_at)
                .map(|hour| hour >= 22 || hour < 6)
                .unwrap_or(false)
        })
        .count();
    let sustained_seconds = sessions
        .iter()
        .map(|session| {
            let active_seconds = session.active_duration_seconds.max(0);
            let wall_clock_seconds = (session.last_seen_at - session.first_seen_at)
                .max(0.0)
                .round() as i64;
            active_seconds.max(wall_clock_seconds)
        })
        .collect::<Vec<_>>();
    let max_secs = sustained_seconds.iter().copied().max().unwrap_or(0);
    let multi_turn_sessions = sessions
        .iter()
        .filter(|session| session.request_count >= 4)
        .count();
    let repair_sessions = sessions.iter().filter(|session| {
        if session.request_count < 3 || session.total_tokens <= 0 {
            return false;
        }
        let avg_per_turn = session.total_tokens as f64 / session.request_count as f64;
        session.active_duration_seconds >= 360 && avg_per_turn >= 120.0 && avg_per_turn <= 4200.0
    });
    let repair_token_budget: i64 = repair_sessions.map(|session| session.total_tokens).sum();

    fn smoothed_ratio(positive: usize, total: usize) -> f64 {
        (positive.max(0) as f64 + 2.0) / (total.max(0) as f64 + 4.0)
    }
    fn sat_ratio(value: f64, target: f64) -> f64 {
        if value > 0.0 && target > 0.0 {
            value / (value + target)
        } else {
            0.0
        }
    }
    fn display_pts(ratio: f64, weight: f64, exponent: f64) -> f64 {
        if ratio > 0.0 && weight > 0.0 {
            ratio.clamp(0.0, 1.0).powf(exponent).min(1.0) * weight
        } else {
            0.0
        }
    }

    let depth = display_pts(sat_ratio(avg_tok_per_req, 6000.0), 230.0, 0.6);
    let deep_sessions = sessions
        .iter()
        .filter(|session| {
            session.request_count > 0
                && session.total_tokens as f64 / session.request_count as f64 >= 2000.0
        })
        .count();
    let focus = display_pts(smoothed_ratio(deep_sessions, sessions.len()), 80.0, 0.55);
    let burst = display_pts(smoothed_ratio(short_count, sessions.len()), 200.0, 0.55);
    let rate = display_pts(sat_ratio(req_per_hour, 6.0), 130.0, 0.65);
    let core = display_pts(smoothed_ratio(night_count, session_count), 240.0, 0.55);
    let streak = display_pts(sat_ratio(night_count as f64, 8.0), 70.0, 0.6);
    let long_count = sustained_seconds
        .iter()
        .filter(|seconds| **seconds >= 1800)
        .count();
    let long = display_pts(smoothed_ratio(long_count, sessions.len()), 200.0, 0.55);
    let peak = display_pts(sat_ratio(max_secs as f64, 3600.0), 130.0, 0.6);
    let repair_share = if total_tokens > 0 {
        repair_token_budget as f64 / total_tokens as f64
    } else {
        0.0
    };
    let repair = display_pts(repair_share.min(1.0), 210.0, 0.55);
    let collaboration = display_pts(
        smoothed_ratio(multi_turn_sessions, session_count),
        120.0,
        0.55,
    );

    PetStats {
        wisdom: (depth + focus).round().max(0.0) as i64,
        chaos: (burst + rate).round().max(0.0) as i64,
        night: (core + streak).round().max(0.0) as i64,
        stamina: (long + peak).round().max(0.0) as i64,
        empathy: (repair + collaboration).round().max(0.0) as i64,
    }
}

fn timestamp_hour_local(seconds: f64) -> Option<u32> {
    if !seconds.is_finite() {
        return None;
    }
    Local
        .timestamp_opt(seconds.floor() as i64, 0)
        .single()
        .map(|date| date.hour())
}

fn pet_persona_id(stats: &PetStats) -> &'static str {
    let mut values = [
        ("wisdom", stats.wisdom),
        ("chaos", stats.chaos),
        ("night", stats.night),
        ("stamina", stats.stamina),
        ("empathy", stats.empathy),
    ];
    values.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    let strongest = values[0];
    let second = values.get(1).map(|item| item.1).unwrap_or(0);
    if strongest.1 <= 0 {
        return "observer";
    }
    let dominant_gap = strongest.1 - second;
    let dominance_ratio = if second > 0 {
        strongest.1 as f64 / second as f64
    } else {
        strongest.1 as f64
    };
    if dominant_gap < 18.max(strongest.1 / 8) || dominance_ratio < 1.12 {
        return "balanced";
    }
    if strongest.0 == "wisdom"
        && stats.wisdom >= (stats.chaos + 60).max((second as f64 * 1.18) as i64)
    {
        return if stats.night >= (stats.wisdom as f64 * 0.72) as i64 {
            "midnight_thinker"
        } else {
            "philosopher"
        };
    }
    if strongest.0 == "chaos" && stats.stamina >= (stats.chaos as f64 * 0.7) as i64 {
        return "mad_scientist";
    }
    if strongest.0 == "night" && stats.empathy >= (stats.night as f64 * 0.55) as i64 {
        return "night_companion";
    }
    if strongest.0 == "stamina" && stats.empathy >= (stats.stamina as f64 * 0.6) as i64 {
        return "debug_comrade";
    }
    if strongest.0 == "night" {
        return "night_owl";
    }
    if strongest.0 == "chaos" {
        return if dominant_gap > 40 {
            "firebrand"
        } else {
            "action_seeker"
        };
    }
    if strongest.0 == "stamina" {
        return if dominant_gap > 40 {
            "marathoner"
        } else {
            "steady_type"
        };
    }
    if strongest.0 == "empathy" {
        return "debug_buddy";
    }
    if strongest.0 == "wisdom" {
        return "wise_type";
    }
    "observer"
}

fn default_persona_id() -> String {
    "observer".to_string()
}

fn apply_derived_snapshot_fields(state: &mut PetSnapshot) {
    state.persona_id = pet_persona_id(&state.current_stats).to_string();
    state.progress = pet_progress_info(state.current_experience_tokens);
    for record in &mut state.legacy {
        record.persona_id = pet_persona_id(&record.stats).to_string();
        record.progress = pet_progress_info(record.total_xp);
    }
}

fn pet_progress_info(total_xp: i64) -> PetProgressInfo {
    let safe_xp = total_xp.max(0);
    let level = level_from_xp(safe_xp);
    let consumed = total_xp_required_to_reach(level);
    let xp_for_level = xp_for_level(level);
    let xp_in_level = (safe_xp - consumed).max(0);
    PetProgressInfo {
        level,
        xp_in_level,
        xp_for_level,
        total_xp: safe_xp,
        progress: if xp_for_level > 0 {
            (xp_in_level as f64 / xp_for_level as f64).clamp(0.0, 1.0)
        } else {
            1.0
        },
        is_at_max_level: level >= MAX_LEVEL,
    }
}

fn level_from_xp(total_xp: i64) -> i64 {
    let mut level = 1;
    let mut remaining = total_xp.max(0);
    loop {
        let needed = xp_for_level(level);
        if remaining < needed {
            break;
        }
        remaining -= needed;
        level += 1;
    }
    level
}

fn xp_for_level(level: i64) -> i64 {
    let requirements = level_requirements();
    if level >= MAX_LEVEL {
        return requirements.last().copied().unwrap_or(MAX_XP_PER_LEVEL);
    }
    let index = (level - 1).max(0) as usize;
    requirements.get(index).copied().unwrap_or(MAX_XP_PER_LEVEL)
}

fn total_xp_required_to_reach(level: i64) -> i64 {
    if level <= 1 {
        return 0;
    }
    let capped_level = level.min(MAX_LEVEL);
    let capped_index = (capped_level - 2).max(0) as usize;
    let sums = level_prefix_sums();
    let mut total = sums.get(capped_index).copied().unwrap_or(0);
    if level > MAX_LEVEL {
        total += (level - MAX_LEVEL) * xp_for_level(MAX_LEVEL);
    }
    total
}

fn level_requirements() -> Vec<i64> {
    let count = (MAX_LEVEL - 1) as usize;
    let weights = (0..count)
        .map(|index| {
            let progress = if count == 1 {
                0.0
            } else {
                index as f64 / (count - 1) as f64
            };
            MIN_XP_PER_LEVEL as f64 + (MAX_XP_PER_LEVEL - MIN_XP_PER_LEVEL) as f64 * progress
        })
        .collect::<Vec<_>>();
    let weight_total: f64 = weights.iter().sum();
    let mut scaled = weights
        .iter()
        .map(|value| ((value / weight_total) * TARGET_XP_TO_REACH_LEVEL_100 as f64).floor() as i64)
        .collect::<Vec<_>>();
    let remainder = TARGET_XP_TO_REACH_LEVEL_100 - scaled.iter().sum::<i64>();
    for offset in 0..remainder.min(count as i64) {
        let centered_index =
            (((offset as f64 + 0.5) * count as f64) / remainder as f64).floor() as usize;
        scaled[centered_index.min(count - 1)] += 1;
    }
    scaled
}

fn level_prefix_sums() -> Vec<i64> {
    let mut running = 0;
    level_requirements()
        .into_iter()
        .map(|requirement| {
            running += requirement;
            running
        })
        .collect()
}

fn sanitize_project_totals(items: Vec<PetProjectTokenTotal>) -> HashMap<String, i64> {
    let mut totals = HashMap::new();
    for item in items {
        let project_id = item.project_id.trim();
        if project_id.is_empty() {
            continue;
        }
        totals.insert(project_id.to_string(), item.total_tokens.max(0));
    }
    totals
}

fn sanitize_state(mut state: PetSnapshot) -> PetSnapshot {
    state.state_version = STATE_VERSION;
    state.stats_model_version = STATS_MODEL_VERSION;
    state.current_experience_tokens = state.current_experience_tokens.max(0);
    state.current_stats = state.current_stats.sanitized();
    state.total_normalized_tokens = state.total_normalized_tokens.max(0);
    state.daily_experience_tokens = state.daily_experience_tokens.max(0);
    state.custom_pet = state.custom_pet.and_then(sanitize_custom_pet);
    state.species = sanitize_claim_species(&state.species, state.custom_pet.as_ref());
    state.custom_name = sanitize_custom_name(&state.custom_name);
    state
        .project_normalized_token_watermarks
        .retain(|project_id, total| !project_id.trim().is_empty() && *total >= 0);
    state.legacy = state
        .legacy
        .into_iter()
        .filter_map(sanitize_legacy_record)
        .collect();
    apply_derived_snapshot_fields(&mut state);
    state
}

pub fn hydrate_custom_pet_data_url(mut pet: PetCustomPet) -> PetCustomPet {
    if pet
        .spritesheet_data_url
        .as_ref()
        .is_some_and(|value| !value.is_empty())
    {
        return pet;
    }
    let path = custom_pets_dir()
        .join(&pet.directory_name)
        .join(&pet.spritesheet_path);
    pet.spritesheet_data_url = png_data_url(&path);
    pet
}

fn sanitize_legacy_record(mut record: PetLegacyRecord) -> Option<PetLegacyRecord> {
    if record.id.trim().is_empty() {
        return None;
    }
    record.custom_pet = record.custom_pet.and_then(sanitize_custom_pet);
    record.species = sanitize_claim_species(&record.species, record.custom_pet.as_ref());
    record.custom_name = sanitize_custom_name(&record.custom_name);
    record.total_xp = record.total_xp.max(0);
    record.stats = record.stats.sanitized();
    record.persona_id = pet_persona_id(&record.stats).to_string();
    record.progress = pet_progress_info(record.total_xp);
    Some(record)
}

fn sanitize_species(species: &str) -> String {
    let trimmed = species.trim();
    if PET_SPECIES.iter().any(|candidate| candidate == &trimmed) {
        trimmed.to_string()
    } else {
        "voidcat".to_string()
    }
}

fn sanitize_claim_species(species: &str, custom_pet: Option<&PetCustomPet>) -> String {
    if let Some(pet) = custom_pet {
        return format!("{CUSTOM_SPECIES_PREFIX}{}", pet.id);
    }
    sanitize_species(species)
}

fn sanitize_custom_pet(mut pet: PetCustomPet) -> Option<PetCustomPet> {
    pet.id = sanitize_custom_pet_id(&pet.id);
    if pet.id.is_empty() {
        return None;
    }
    pet.display_name =
        sanitize_custom_display_name(&pet.display_name).unwrap_or_else(|| pet.id.clone());
    pet.description = pet.description.trim().chars().take(280).collect();
    pet.spritesheet_path = sanitize_relative_path(&pet.spritesheet_path)?;
    pet.spritesheet_data_url = None;
    pet.directory_name = sanitize_custom_pet_id(&pet.directory_name);
    if pet.directory_name.is_empty() {
        pet.directory_name = pet.id.clone();
    }
    Some(pet)
}

fn sanitize_custom_display_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(64).collect())
    }
}

fn sanitize_custom_name(name: &str) -> String {
    name.trim().chars().take(40).collect()
}

fn legacy_record_from_state(state: &PetSnapshot) -> Option<PetLegacyRecord> {
    state.claimed_at?;
    Some(PetLegacyRecord {
        id: Uuid::new_v4().to_string(),
        species: sanitize_claim_species(&state.species, state.custom_pet.as_ref()),
        custom_pet: state.custom_pet.clone(),
        custom_name: sanitize_custom_name(&state.custom_name),
        total_xp: state.current_experience_tokens.max(0),
        stats: state.current_stats.clone().sanitized(),
        persona_id: pet_persona_id(&state.current_stats).to_string(),
        progress: pet_progress_info(state.current_experience_tokens),
        retired_at: Utc::now().timestamp(),
    })
}

fn load_state(path: &PathBuf) -> Option<PetSnapshot> {
    let local_state = load_tauri_state(path)
        .or_else(|| load_tauri_json_state())
        .map(sanitize_state);
    if local_state
        .as_ref()
        .is_some_and(|state| state.claimed_at.is_some())
    {
        return local_state;
    }
    let mac_state = load_mac_pet_state().map(sanitize_state);
    if mac_state
        .as_ref()
        .is_some_and(|state| state.claimed_at.is_some())
    {
        return mac_state;
    }
    local_state.or(mac_state)
}

fn loaded_state_claimed(state: &Option<PetSnapshot>) -> bool {
    state
        .as_ref()
        .is_some_and(|snapshot| snapshot.claimed_at.is_some())
}

fn load_tauri_state(path: &PathBuf) -> Option<PetSnapshot> {
    let data = fs::read(path).ok()?;
    if data.is_empty() {
        return None;
    }
    decode_pet_state_data(&data, PET_STATE_DECODE_NAMESPACES)
        .and_then(|decoded| serde_json::from_slice(&decoded).ok())
}

fn load_tauri_json_state() -> Option<PetSnapshot> {
    let json_path = app_support_dir().join("pet-state.json");
    let data = fs::read(json_path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn load_mac_pet_state() -> Option<PetSnapshot> {
    for path in mac_pet_state_paths() {
        let Ok(data) = fs::read(path) else {
            continue;
        };
        if let Some(decoded) = decode_pet_state_data(&data, PET_STATE_DECODE_NAMESPACES) {
            if let Ok(state) = serde_json::from_slice::<MacPersistedPetState>(&decoded) {
                return Some(mac_state_to_snapshot(state));
            }
        }
    }
    None
}

fn mac_pet_state_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let support = home_dir().join("Library").join("Application Support");
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

fn migrate_mac_custom_pets_if_needed() {
    let destination = custom_pets_dir();
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
        if load_custom_pets(false).is_empty() {
            continue;
        }
        return;
    }
}

fn mac_custom_pet_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let support = home_dir().join("Library").join("Application Support");
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

fn decode_pet_state_data(data: &[u8], namespaces: &[&str]) -> Option<Vec<u8>> {
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
    if data.len() < 12 + 16 {
        return None;
    }
    let key = pet_state_cipher_key(namespace);
    let cipher = Aes256Gcm::new(&key);
    cipher
        .decrypt(Nonce::from_slice(&data[..12]), &data[12..])
        .ok()
}

fn encode_pet_state_data(snapshot: &PetSnapshot) -> Result<Vec<u8>, String> {
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

fn pet_state_cipher_key(namespace: &str) -> Key<Aes256Gcm> {
    use sha2::{Digest, Sha256};

    let material = format!("dmux.pet.state.v2|{namespace}|codux");
    let digest = Sha256::digest(material.as_bytes());
    *Key::<Aes256Gcm>::from_slice(&digest)
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
    let now = Utc::now().timestamp();
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
        state_version: STATE_VERSION,
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

fn state_file_path() -> PathBuf {
    app_support_dir().join("pet-state.dat")
}

fn custom_pets_dir() -> PathBuf {
    app_support_dir().join("custom-pets")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codux-pet-test-{name}-{}.dat", Uuid::new_v4()))
    }

    #[test]
    fn refresh_state_does_not_auto_claim_or_grant_history() {
        let mut state = PetSnapshot::default();

        refresh_state(
            &mut state,
            PetRefreshInput {
                project_totals: vec![PetProjectTokenTotal {
                    project_id: "project-a".to_string(),
                    total_tokens: 100,
                }],
                fallback_total_tokens: 100,
                computed_stats: PetStats::default(),
            },
        );

        assert_eq!(state.claimed_at, None);
        assert_eq!(state.current_experience_tokens, 0);
        assert_eq!(state.total_normalized_tokens, 100);
        assert!(state.project_normalized_token_watermarks.is_empty());
    }

    #[test]
    fn claim_bootstraps_baseline_without_xp() {
        let state_file = test_state_file("claim");
        let store = PetStore {
            state: Mutex::new(PetSnapshot::default()),
            state_file: state_file.clone(),
        };

        let snapshot = store
            .claim(PetClaimInput {
                species: "panda".to_string(),
                custom_name: " Bao ".to_string(),
                custom_pet: None,
                project_totals: vec![PetProjectTokenTotal {
                    project_id: "project-a".to_string(),
                    total_tokens: 100,
                }],
                fallback_total_tokens: 100,
            })
            .unwrap();

        assert!(snapshot.claimed_at.is_some());
        assert_eq!(snapshot.species, "panda");
        assert_eq!(snapshot.custom_name, "Bao");
        assert_eq!(snapshot.current_experience_tokens, 0);
        assert_eq!(
            snapshot
                .project_normalized_token_watermarks
                .get("project-a")
                .copied(),
            Some(100)
        );
        let _ = fs::remove_file(state_file);
    }

    #[test]
    fn refresh_state_applies_only_positive_project_deltas() {
        let mut state = PetSnapshot {
            claimed_at: Some(1),
            current_experience_tokens: 100,
            project_normalized_token_watermarks: HashMap::from([("project-a".to_string(), 100)]),
            ..PetSnapshot::default()
        };

        refresh_state(
            &mut state,
            PetRefreshInput {
                project_totals: vec![PetProjectTokenTotal {
                    project_id: "project-a".to_string(),
                    total_tokens: 130,
                }],
                fallback_total_tokens: 130,
                computed_stats: PetStats::default(),
            },
        );

        assert_eq!(state.current_experience_tokens, 130);
        assert_eq!(
            state
                .project_normalized_token_watermarks
                .get("project-a")
                .copied(),
            Some(130)
        );
    }

    #[test]
    fn refresh_state_bootstraps_new_project_without_granting_old_history() {
        let mut state = PetSnapshot {
            claimed_at: Some(1),
            current_experience_tokens: 100,
            project_normalized_token_watermarks: HashMap::from([("project-a".to_string(), 130)]),
            ..PetSnapshot::default()
        };

        refresh_state(
            &mut state,
            PetRefreshInput {
                project_totals: vec![
                    PetProjectTokenTotal {
                        project_id: "project-a".to_string(),
                        total_tokens: 130,
                    },
                    PetProjectTokenTotal {
                        project_id: "project-b".to_string(),
                        total_tokens: 900,
                    },
                ],
                fallback_total_tokens: 1_030,
                computed_stats: PetStats::default(),
            },
        );

        assert_eq!(state.current_experience_tokens, 100);
        assert_eq!(
            state
                .project_normalized_token_watermarks
                .get("project-b")
                .copied(),
            Some(900)
        );
    }

    #[test]
    fn daily_experience_day_uses_local_calendar_day() {
        let timestamp = Local
            .with_ymd_and_hms(2026, 5, 22, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let start = Local
            .with_ymd_and_hms(2026, 5, 22, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp();

        assert_eq!(day_index(timestamp), start.div_euclid(86_400));
    }

    #[test]
    fn apply_stats_replaces_neutral_stats_even_with_existing_timestamp() {
        let mut state = PetSnapshot {
            claimed_at: Some(1),
            current_stats: PetStats::default(),
            stats_updated_day: Some(1_700_000_000),
            ..PetSnapshot::default()
        };

        apply_stats(
            &mut state,
            PetStats {
                wisdom: 88,
                chaos: 12,
                night: 44,
                stamina: 10,
                empathy: 9,
            },
            1_700_000_120,
        );

        assert_eq!(
            state.current_stats,
            PetStats {
                wisdom: 88,
                chaos: 12,
                night: 44,
                stamina: 10,
                empathy: 9,
            }
        );
        assert_eq!(state.stats_updated_day, Some(1_700_000_120));
    }

    #[test]
    fn pet_persona_uses_mac_balanced_tie_breaker() {
        assert_eq!(
            pet_persona_id(&PetStats {
                wisdom: 100,
                chaos: 100,
                night: 100,
                stamina: 100,
                empathy: 100,
            }),
            "balanced"
        );
    }

    #[test]
    fn pet_stats_derives_values_from_sustained_sessions() {
        let sessions = (0..5)
            .map(|index| AISessionSummary {
                session_id: format!("session-{index}"),
                external_session_id: None,
                project_id: "project-a".to_string(),
                project_name: "Project A".to_string(),
                project_path: "/tmp/project-a".to_string(),
                session_title: "Terminal".to_string(),
                first_seen_at: 1_800_000_000.0 + index as f64 * 7200.0,
                last_seen_at: 1_800_000_000.0 + index as f64 * 7200.0 + 2400.0,
                last_tool: None,
                last_model: None,
                request_count: 5,
                total_input_tokens: 20_000,
                total_output_tokens: 10_000,
                total_tokens: 30_000,
                cached_input_tokens: 1000,
                active_duration_seconds: 2400,
                today_tokens: 30_000,
                today_cached_input_tokens: 1000,
            })
            .collect::<Vec<_>>();

        let stats = pet_stats_from_sessions(&sessions);

        assert!(stats.wisdom > 0);
        assert!(stats.stamina > 0);
    }

    #[test]
    fn pet_progress_matches_mac_level_cap_target() {
        let cap_target = DAILY_TARGET_XP * 30;
        let capped = pet_progress_info(cap_target);
        assert_eq!(capped.level, 100);
        assert!(capped.is_at_max_level);
        assert_eq!(capped.xp_in_level, 0);
        assert_eq!(capped.progress, 0.0);
        assert_eq!(pet_progress_info(cap_target - 1).level, 99);

        let after_cap = pet_progress_info(cap_target + 1);
        assert_eq!(after_cap.level, 100);
        assert_eq!(after_cap.xp_in_level, 1);
        assert!(after_cap.xp_for_level > 0);
    }

    #[test]
    fn archive_and_restore_preserve_pet_record() {
        let state_file = test_state_file("archive");
        let store = PetStore {
            state: Mutex::new(PetSnapshot {
                claimed_at: Some(1),
                species: "dragon".to_string(),
                custom_name: "Spark".to_string(),
                current_experience_tokens: 42,
                ..PetSnapshot::default()
            }),
            state_file: state_file.clone(),
        };

        let archived = store.archive_current().unwrap();
        assert!(archived.claimed_at.is_none());
        assert_eq!(archived.legacy.len(), 1);

        let restored = store
            .restore_archived(PetRestoreRequest {
                legacy_id: archived.legacy[0].id.clone(),
            })
            .unwrap();
        assert_eq!(restored.species, "dragon");
        assert_eq!(restored.custom_name, "Spark");
        assert_eq!(restored.current_experience_tokens, 42);
        assert!(restored.legacy.is_empty());
        let _ = fs::remove_file(state_file);
    }

    #[test]
    fn sanitize_state_preserves_claimed_pet_across_version_migrations() {
        let snapshot = sanitize_state(PetSnapshot {
            state_version: 7,
            claimed_at: Some(1),
            species: "dragon".to_string(),
            custom_name: " Spark ".to_string(),
            current_experience_tokens: 200,
            project_normalized_token_watermarks: HashMap::from([("project-a".to_string(), 200)]),
            ..PetSnapshot::default()
        });

        assert_eq!(snapshot.state_version, STATE_VERSION);
        assert_eq!(snapshot.claimed_at, Some(1));
        assert_eq!(snapshot.species, "dragon");
        assert_eq!(snapshot.custom_name, "Spark");
        assert_eq!(snapshot.current_experience_tokens, 200);
        assert_eq!(
            snapshot
                .project_normalized_token_watermarks
                .get("project-a")
                .copied(),
            Some(200)
        );
    }

    #[test]
    fn snapshot_returns_current_state_without_refreshing() {
        let store = PetStore {
            state: Mutex::new(PetSnapshot {
                current_experience_tokens: 42,
                total_normalized_tokens: 42,
                ..PetSnapshot::default()
            }),
            state_file: test_state_file("snapshot"),
        };

        let snapshot = store.snapshot().unwrap();

        assert_eq!(snapshot.current_experience_tokens, 42);
        assert_eq!(snapshot.total_normalized_tokens, 42);
    }

    #[test]
    fn pet_state_dat_roundtrips_encrypted() {
        let snapshot = PetSnapshot {
            claimed_at: Some(123),
            species: "panda".to_string(),
            custom_name: "Bao".to_string(),
            current_experience_tokens: 88,
            ..PetSnapshot::default()
        };
        let data = encode_pet_state_data(&snapshot).unwrap();
        assert!(serde_json::from_slice::<Value>(&data).is_err());
        let decoded = decode_pet_state_data(&data, &[PET_STATE_CRYPTO_NAMESPACE]).unwrap();
        let restored = serde_json::from_slice::<PetSnapshot>(&decoded).unwrap();
        assert_eq!(restored.species, "panda");
        assert_eq!(restored.custom_name, "Bao");
        assert_eq!(restored.current_experience_tokens, 88);
    }

    #[test]
    fn loaded_state_claimed_marks_recovered_pet_for_persist() {
        assert!(!loaded_state_claimed(&None));
        assert!(!loaded_state_claimed(&Some(PetSnapshot::default())));
        assert!(loaded_state_claimed(&Some(PetSnapshot {
            claimed_at: Some(123),
            ..PetSnapshot::default()
        })));
    }

    #[test]
    fn mac_state_migrates_to_current_snapshot() {
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
        let snapshot = sanitize_state(mac_state_to_snapshot(mac));
        assert_eq!(snapshot.species, "dragon");
        assert_eq!(snapshot.custom_name, "Spark");
        assert_eq!(snapshot.current_experience_tokens, 1200);
        assert_eq!(snapshot.current_stats.chaos, 2);
        assert_eq!(
            snapshot
                .project_normalized_token_watermarks
                .get("project-a"),
            Some(&5000)
        );
    }
}
