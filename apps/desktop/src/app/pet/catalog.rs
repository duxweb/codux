use super::*;

pub(super) fn legacy_pet_sprite_path(
    runtime_asset_root: &Path,
    support_dir: &Path,
    record: &PetLegacyRecord,
) -> ImageSource {
    if let Some(custom_pet) = record.custom_pet.as_ref() {
        return custom_pet_sprite_path(support_dir, custom_pet).into();
    }

    pet_sprite_path(
        runtime_asset_root,
        support_dir,
        &PetSummary {
            species: record.species.clone(),
            ..PetSummary::default()
        },
        &[],
    )
}

pub(super) fn pet_date_label(timestamp: i64) -> String {
    use chrono::{Datelike, Local, TimeZone};

    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|date| format!("{}/{}/{}", date.year(), date.month(), date.day()))
        .unwrap_or_else(|| "Unknown date".to_string())
}

pub(super) fn pet_accent_color(species: &str) -> u32 {
    match species {
        "voidcat" => 0x6A5CFF,
        "rusthound" => 0xFF8A3D,
        "goose" => 0x3E86F6,
        "chaossprite" => 0xFF4FA3,
        "code" => 0x2F8FFF,
        "sheep" => 0xF28FB8,
        "ox" => 0xF3B43F,
        "dragon" => 0xE04435,
        "phoenix" => 0xFF7A22,
        "dolphin" => 0x1E9BFF,
        "penguin" => 0x5C6D85,
        "panda" => 0x6A6F78,
        _ => theme::ACCENT,
    }
}
pub(super) fn pet_catalog_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

pub(super) fn pet_format_placeholders(template: &str, values: &[String]) -> String {
    let mut output = template.to_string();
    for value in values {
        output = output.replacen("%@", value, 1);
    }
    output
}

pub(super) fn pet_species_name(species: &str) -> String {
    match species.strip_prefix("custom:").unwrap_or(species) {
        "voidcat" => "Voidcat",
        "rusthound" => "Rusthound",
        "goose" => "Goose",
        "chaossprite" => "Chaos Sprite",
        "code" => "Code",
        "sheep" => "Sheep",
        "ox" => "Ox",
        "dragon" => "Dragon",
        "phoenix" => "Phoenix",
        "dolphin" => "Dolphin",
        "penguin" => "Penguin",
        "panda" => "Panda",
        "" => "Voidcat",
        value => value,
    }
    .to_string()
}

pub(super) fn pet_species_subtitle(species: &str) -> String {
    match species.strip_prefix("custom:").unwrap_or(species) {
        "voidcat" => "Quietly watches code changes",
        "rusthound" => "Likes Rust and compiler feedback",
        "goose" => "Keeps an eye on task rhythm",
        "chaossprite" => "Best for fast experiments",
        "code" => "Default coding companion",
        "sheep" => "Gentle long-running companion",
        "ox" => "Steady task mover",
        "dragon" => "Built for refactors and sprints",
        "phoenix" => "Good for recovery and review",
        "dolphin" => "Good for collaboration and exploration",
        "penguin" => "Good for terminal workflows",
        "panda" => "Good for quiet maintenance",
        _ => "Codux pet companion",
    }
    .to_string()
}
