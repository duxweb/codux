use super::*;

impl PetStore {
    pub fn load_or_seed(support_dir: PathBuf) -> Self {
        let state_file = support_dir.join("pet-state.dat");
        migrate_mac_custom_pets_if_needed(&support_dir);
        let state = PetService::new(support_dir)
            .load_snapshot()
            .map(|(snapshot, _)| sanitize_state(snapshot))
            .unwrap_or_default();
        let store = Self {
            state: Mutex::new(state),
            state_file,
        };
        if !store.state_file.is_file()
            && store
                .state
                .lock()
                .map(|state| state.claimed_at.is_some())
                .unwrap_or(false)
            && let Ok(snapshot) = store.snapshot()
        {
            let _ = store.save(&snapshot);
        }
        store
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
            let now = now_seconds();
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
            state.updated_at = now_seconds();
            Ok(())
        })
    }

    pub fn archive_current(&self) -> Result<PetSnapshot, String> {
        self.with_mut_snapshot(|state| {
            let record = legacy_record_from_state(state)
                .ok_or_else(|| "No pet has been claimed.".to_string())?;
            let mut legacy = std::mem::take(&mut state.legacy);
            legacy.insert(0, record);
            let now = now_seconds();
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
            let now = now_seconds();
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

    pub fn snapshot(&self) -> Result<PetSnapshot, String> {
        self.state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| "Pet store lock poisoned.".to_string())
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
