use super::*;

pub(super) fn sanitize_state(mut state: PetSnapshot) -> PetSnapshot {
    let pending_membership_migration =
        state.state_version < STATE_VERSION && state.claimed_at.is_some();
    if pending_membership_migration {
        state.experience_recalibration_pending = true;
    }
    let rebuild_stats = state.stats_model_version < STATS_MODEL_VERSION;
    if !pending_membership_migration {
        state.state_version = STATE_VERSION;
    }
    state.stats_model_version = STATS_MODEL_VERSION;
    state.current_experience_tokens = state.current_experience_tokens.max(0);
    state.current_stats = state.current_stats.sanitized();
    state.experience_base_tokens = state.experience_base_tokens.max(0);
    state.daily_experience_tokens = state.daily_experience_tokens.max(0);
    state.custom_pet = state.custom_pet.and_then(sanitize_custom_pet);
    state.species = sanitize_claim_species(&state.species, state.custom_pet.as_ref());
    state.custom_name = sanitize_custom_name(&state.custom_name);
    state.memberships = sanitize_memberships(state.memberships);
    state
        .pending_project_token_watermarks
        .retain(|project_id, _| !project_id.trim().is_empty());
    if rebuild_stats {
        state.current_stats = PetStats::default();
        state.stats_updated_day = None;
    }
    state.legacy = state
        .legacy
        .into_iter()
        .filter_map(sanitize_legacy_record)
        .collect();
    apply_derived_snapshot_fields(&mut state);
    state
}

fn sanitize_legacy_record(mut record: PetLegacyRecord) -> Option<PetLegacyRecord> {
    if record.id.trim().is_empty() {
        return None;
    }
    record.custom_pet = record.custom_pet.and_then(sanitize_custom_pet);
    record.species = sanitize_claim_species(&record.species, record.custom_pet.as_ref());
    record.custom_name = sanitize_custom_name(&record.custom_name);
    record.total_xp = record.total_xp.max(0);
    record.memberships = sanitize_memberships(record.memberships);
    record.experience_base_tokens = if record.memberships.is_empty() {
        record.experience_base_tokens.max(record.total_xp).max(0)
    } else {
        record.experience_base_tokens.max(0)
    };
    record.stats = record.stats.sanitized();
    record.persona_id = pet_persona_id(&record.stats).to_string();
    record.progress = pet_progress_info(record.total_xp);
    Some(record)
}

pub(super) fn legacy_record_from_state(state: &PetSnapshot) -> Option<PetLegacyRecord> {
    state.claimed_at?;
    Some(PetLegacyRecord {
        id: Uuid::new_v4().to_string(),
        species: sanitize_claim_species(&state.species, state.custom_pet.as_ref()),
        custom_pet: state.custom_pet.clone(),
        custom_name: sanitize_custom_name(&state.custom_name),
        total_xp: state.current_experience_tokens.max(0),
        memberships: state.memberships.clone(),
        experience_base_tokens: state.experience_base_tokens.max(0),
        stats: state.current_stats.clone().sanitized(),
        persona_id: pet_persona_id(&state.current_stats).to_string(),
        progress: pet_progress_info(state.current_experience_tokens),
        retired_at: now_seconds(),
    })
}

fn sanitize_memberships(memberships: Vec<PetProjectMembership>) -> Vec<PetProjectMembership> {
    let mut sanitized = Vec::new();
    for mut membership in memberships {
        membership.project_path = normalize_workspace_path(&membership.project_path);
        if membership.project_path.is_empty() {
            continue;
        }
        if membership
            .excluded_at
            .is_some_and(|excluded_at| excluded_at <= membership.included_at)
        {
            membership.excluded_at = Some(membership.included_at);
        }
        if !sanitized.contains(&membership) {
            sanitized.push(membership);
        }
    }
    sanitized
}

pub(super) fn normalize_workspace_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    codux_runtime_core::path::normalize_local_path(std::path::Path::new(path))
}

pub(super) fn sanitize_claim_species(species: &str, custom_pet: Option<&PetCustomPet>) -> String {
    if let Some(pet) = custom_pet {
        return format!("{CUSTOM_SPECIES_PREFIX}{}", pet.id);
    }
    sanitize_species(species)
}

pub(super) fn sanitize_species(species: &str) -> String {
    let trimmed = species.trim();
    if PET_SPECIES.iter().any(|candidate| candidate == &trimmed) {
        trimmed.to_string()
    } else {
        "voidcat".to_string()
    }
}

pub(super) fn sanitize_custom_pet(mut pet: PetCustomPet) -> Option<PetCustomPet> {
    pet.id = sanitize_custom_pet_id(&pet.id);
    if pet.id.is_empty() {
        return None;
    }
    pet.display_name = sanitize_custom_display_name(&pet.display_name).unwrap_or(pet.id.clone());
    pet.description = pet.description.trim().chars().take(280).collect();
    pet.spritesheet_path = sanitize_relative_path(&pet.spritesheet_path)?;
    pet.spritesheet_data_url = None;
    pet.directory_name = sanitize_custom_pet_id(&pet.directory_name);
    if pet.directory_name.is_empty() {
        pet.directory_name = pet.id.clone();
    }
    Some(pet)
}

pub(super) fn sanitize_custom_pet_id(value: &str) -> String {
    value
        .trim()
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

pub(super) fn sanitize_custom_display_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    (!trimmed.is_empty()).then(|| trimmed.chars().take(64).collect())
}

pub(super) fn sanitize_relative_path(path: &str) -> Option<String> {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() || trimmed.starts_with('/') || trimmed.contains("..") {
        return None;
    }
    Some(trimmed)
}

pub(super) fn sanitize_custom_name(name: &str) -> String {
    name.trim().chars().take(40).collect()
}

pub(super) fn pet_display_name(snapshot: &PetSnapshot) -> String {
    if !snapshot.custom_name.trim().is_empty() {
        return snapshot.custom_name.clone();
    }
    snapshot
        .custom_pet
        .as_ref()
        .map(|pet| pet.display_name.clone())
        .unwrap_or_else(|| {
            snapshot
                .species
                .trim_start_matches(CUSTOM_SPECIES_PREFIX)
                .to_string()
        })
}
