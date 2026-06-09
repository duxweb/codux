use super::super::{
    PetCustomPet, PetCustomPetInstallPreview,
    catalog::{custom_pet_from_dir, custom_pets_dir, persist_custom_pet_manifest},
    now_seconds, sanitize_custom_pet_id,
};
use super::types::StagingCleanup;
use std::{fs, io, path::PathBuf};
use uuid::Uuid;
use zip::ZipArchive;

pub(super) fn install_custom_pet_package(
    support_dir: &std::path::Path,
    preview: PetCustomPetInstallPreview,
    bytes: &[u8],
) -> Result<PetCustomPet, String> {
    let package_id = sanitize_custom_pet_id(&preview.slug);
    if package_id.is_empty() {
        return Err("The Petdex package name is invalid.".to_string());
    }
    let staging_dir = std::env::temp_dir().join(format!("codux-pet-staging-{}", Uuid::new_v4()));
    let destination = custom_pets_dir(support_dir).join(&package_id);
    fs::create_dir_all(&staging_dir).map_err(|error| error.to_string())?;
    let _cleanup = StagingCleanup(staging_dir.clone());
    extract_zip_bytes(bytes, &staging_dir)?;
    let package_dir = find_pet_package_dir(&staging_dir)?;
    custom_pet_from_dir(package_dir.clone(), false).ok_or_else(|| {
        "The downloaded package does not contain a valid pet.json and spritesheet.".to_string()
    })?;
    fs::create_dir_all(custom_pets_dir(support_dir)).map_err(|error| error.to_string())?;
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
    pet.installed_at = Some(now_seconds());
    persist_custom_pet_manifest(support_dir, &pet)?;
    Ok(pet)
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
