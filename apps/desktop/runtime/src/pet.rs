use crate::ai_history_normalized::AISessionSummary;
use crate::ai_usage_store::AIUsageProjectTotal;

mod catalog;
mod constants;
mod crypto;
mod defaults;
mod history_inputs;
mod install;
mod migration;
mod progress;
mod refresh;
mod sanitize;
mod service;
mod stats;
mod store;
#[cfg(test)]
mod tests;
mod time;
mod types;

use catalog::{bundled_pet_catalog, hydrate_custom_pet_data_url, load_custom_pets, pet_catalog};
use chrono::{Datelike, Local, TimeZone};
use constants::*;
#[cfg(test)]
use crypto::pet_state_cipher_key;
use crypto::{decode_pet_state_data, encode_pet_state_data};
pub use history_inputs::refresh_input_from_indexed_history;
use install::install_custom_pet;
use migration::{load_mac_pet_state, migrate_mac_custom_pets_if_needed};
use progress::{default_persona_id, pet_progress_info};
use refresh::{apply_derived_snapshot_fields, refresh_state, sanitize_project_totals};
use sanitize::{
    legacy_record_from_state, pet_display_name, sanitize_claim_species,
    sanitize_custom_display_name, sanitize_custom_name, sanitize_custom_pet,
    sanitize_custom_pet_id, sanitize_relative_path, sanitize_species, sanitize_state,
};
use stats::{apply_stats, pet_persona_id, pet_stats_from_sessions};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use time::{day_index, dedupe_paths, now_seconds};
pub use types::*;
use uuid::Uuid;

pub struct PetService {
    support_dir: PathBuf,
}

pub struct PetStore {
    state: Mutex<PetSnapshot>,
    state_file: PathBuf,
}
