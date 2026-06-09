use super::super::{
    PetCustomPetInstallPreview, PetCustomPetInstallRequest, sanitize_custom_display_name,
};
use super::types::PetInstallRequestInternal;
use url::Url;

pub(super) fn resolve_custom_pet_install_from_html(
    request: PetCustomPetInstallRequest,
    html: &str,
    page_url: &Url,
) -> Result<PetCustomPetInstallPreview, String> {
    let install = install_request_from_html(html, page_url)?;
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
        local_image_path: None,
    })
}

pub(super) fn validate_petdex_url(url: &Url) -> Result<(), String> {
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
