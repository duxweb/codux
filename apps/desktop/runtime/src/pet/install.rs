mod html;
mod package;
mod types;

#[cfg(test)]
mod tests;

use super::{PetCustomPet, PetCustomPetInstallPreview, PetCustomPetInstallRequest};
use html::{resolve_custom_pet_install_from_html, validate_petdex_url};
use package::install_custom_pet_package;
use std::{
    fs,
    path::{Path, PathBuf},
};
use url::Url;

pub(super) async fn resolve_custom_pet_install(
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
    resolve_custom_pet_install_from_html(request, &html, &page_url)
}

pub(super) async fn resolve_custom_pet_install_with_cache(
    support_dir: PathBuf,
    request: PetCustomPetInstallRequest,
) -> Result<PetCustomPetInstallPreview, String> {
    let mut preview = resolve_custom_pet_install(request).await?;
    preview.local_image_path = cache_preview_image(&support_dir, &preview).await;
    Ok(preview)
}

pub(super) async fn install_custom_pet(
    support_dir: PathBuf,
    request: PetCustomPetInstallRequest,
) -> Result<PetCustomPet, String> {
    let preview = resolve_custom_pet_install(request).await?;
    let zip_url = Url::parse(&preview.zip_url)
        .map_err(|_| "The Petdex package URL is invalid.".to_string())?;
    let bytes = reqwest::get(zip_url)
        .await
        .map_err(|_| "Failed to download the pet package.".to_string())?
        .error_for_status()
        .map_err(|_| "Failed to download the pet package.".to_string())?
        .bytes()
        .await
        .map_err(|_| "Failed to download the pet package.".to_string())?;
    install_custom_pet_package(&support_dir, preview, &bytes)
}

async fn cache_preview_image(
    support_dir: &Path,
    preview: &PetCustomPetInstallPreview,
) -> Option<String> {
    let image_url = preview.image_url.as_ref()?;
    let url = Url::parse(image_url).ok()?;
    let bytes = reqwest::get(url)
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .await
        .ok()?;
    let extension = preview_image_extension(image_url, bytes.as_ref());
    let cache_dir = support_dir.join("cache").join("pet-previews");
    fs::create_dir_all(&cache_dir).ok()?;
    let path = cache_dir.join(format!(
        "{}.{}",
        sanitize_preview_cache_name(&preview.slug),
        extension
    ));
    fs::write(&path, bytes.as_ref()).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn preview_image_extension(image_url: &str, bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"GIF") {
        return "gif";
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "png";
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return "jpg";
    }
    if image_url.to_ascii_lowercase().contains(".webp") {
        return "webp";
    }
    "img"
}

fn sanitize_preview_cache_name(value: &str) -> String {
    let name: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = name.trim_matches('-');
    if trimmed.is_empty() {
        "custom-pet".to_string()
    } else {
        trimmed.to_string()
    }
}
