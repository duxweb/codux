use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetSummary {
    pub available: bool,
    pub claimed: bool,
    pub species: String,
    pub display_name: String,
    pub custom_name: String,
    pub level: i64,
    pub total_xp: i64,
    pub progress: f64,
    pub daily_xp: i64,
    pub archived_count: usize,
    pub custom_pet_count: usize,
    pub updated_at: Option<i64>,
    pub source: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PetProjectTokenTotal {
    pub project_id: String,
    pub total_tokens: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetRefreshRequest {
    #[serde(rename = "projects", default)]
    pub _projects: Vec<crate::ai_history_normalized::AIHistoryProjectRequest>,
}

#[derive(Clone, Debug)]
pub struct PetRefreshInput {
    pub project_totals: Vec<PetProjectTokenTotal>,
    pub fallback_total_tokens: i64,
    pub computed_stats: PetStats,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetClaimRequest {
    pub species: String,
    pub custom_name: String,
    pub custom_pet: Option<PetCustomPet>,
    #[serde(rename = "projects", default)]
    pub _projects: Vec<crate::ai_history_normalized::AIHistoryProjectRequest>,
}

#[derive(Clone, Debug)]
pub struct PetClaimInput {
    pub species: String,
    pub custom_name: String,
    pub custom_pet: Option<PetCustomPet>,
    pub project_totals: Vec<PetProjectTokenTotal>,
    pub fallback_total_tokens: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetRenameRequest {
    pub custom_name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetRestoreRequest {
    pub legacy_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCatalog {
    pub species: Vec<PetCatalogItem>,
    pub custom_pets: Vec<PetCustomPet>,
    pub atlas: PetAtlasSpec,
}

#[derive(Clone, Debug, Serialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCustomPetInstallRequest {
    pub page_url: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCustomPetInstallPreview {
    pub page_url: String,
    pub zip_url: String,
    pub slug: String,
    pub display_name: String,
    pub description: String,
    pub image_url: Option<String>,
    pub local_image_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAtlasSpec {
    pub columns: usize,
    pub rows: usize,
    pub cell_width: usize,
    pub cell_height: usize,
    pub animations: Vec<PetAnimationSpec>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAnimationSpec {
    pub state: String,
    pub row: usize,
    pub frame_durations_ms: Vec<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PetStats {
    pub wisdom: i64,
    pub chaos: i64,
    pub night: i64,
    pub stamina: i64,
    pub empathy: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetSnapshot {
    pub state_version: u32,
    pub stats_model_version: u32,
    pub claimed_at: Option<i64>,
    pub species: String,
    pub custom_pet: Option<PetCustomPet>,
    pub custom_name: String,
    #[serde(alias = "currentHatchTokens")]
    pub current_experience_tokens: i64,
    pub current_stats: PetStats,
    #[serde(default = "super::default_persona_id")]
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetLegacyRecord {
    pub id: String,
    pub species: String,
    pub custom_pet: Option<PetCustomPet>,
    pub custom_name: String,
    pub total_xp: i64,
    pub stats: PetStats,
    #[serde(default = "super::default_persona_id")]
    pub persona_id: String,
    #[serde(default)]
    pub progress: PetProgressInfo,
    pub retired_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetProgressInfo {
    pub level: i64,
    pub xp_in_level: i64,
    pub xp_for_level: i64,
    pub total_xp: i64,
    pub progress: f64,
    pub is_at_max_level: bool,
}
