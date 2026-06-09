use super::{
    PET_SPECIES, PetAnimationSpec, PetAtlasSpec, PetCatalog, PetCatalogItem, PetCustomPet,
    sanitize_custom_pet_id, sanitize_relative_path,
};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

pub(super) fn pet_catalog(support_dir: PathBuf) -> PetCatalog {
    let mut catalog = bundled_pet_catalog();
    catalog.custom_pets = load_custom_pets(&support_dir, true);
    catalog
}

pub(super) fn bundled_pet_catalog() -> PetCatalog {
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
        custom_pets: Vec::new(),
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

pub(super) fn load_custom_pets(
    support_dir: &std::path::Path,
    include_data_url: bool,
) -> Vec<PetCustomPet> {
    let root = custom_pets_dir(support_dir);
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

pub(super) fn hydrate_custom_pet_data_url(
    support_dir: &std::path::Path,
    mut pet: PetCustomPet,
) -> PetCustomPet {
    if pet
        .spritesheet_data_url
        .as_ref()
        .is_some_and(|value| !value.is_empty())
    {
        return pet;
    }
    let path = custom_pets_dir(support_dir)
        .join(&pet.directory_name)
        .join(&pet.spritesheet_path);
    pet.spritesheet_data_url = png_data_url(&path);
    pet
}

pub(super) fn custom_pet_from_dir(dir: PathBuf, include_data_url: bool) -> Option<PetCustomPet> {
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
pub(super) struct PetCustomPetManifest {
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

fn png_data_url(path: &PathBuf) -> Option<String> {
    let data = fs::read(path).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(data)
    ))
}

pub(super) fn persist_custom_pet_manifest(
    support_dir: &std::path::Path,
    pet: &PetCustomPet,
) -> Result<(), String> {
    let manifest_path = custom_pets_dir(support_dir)
        .join(&pet.directory_name)
        .join("pet.json");
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

fn animation(state: &str, row: usize, frame_durations_ms: &[u64]) -> PetAnimationSpec {
    PetAnimationSpec {
        state: state.to_string(),
        row,
        frame_durations_ms: frame_durations_ms.to_vec(),
    }
}

pub(super) fn custom_pets_dir(support_dir: &std::path::Path) -> PathBuf {
    support_dir.join("custom-pets")
}
