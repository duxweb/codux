use super::*;
use crate::pet::refresh::reset_daily_tokens_if_needed;

impl PetService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self { support_dir }
    }

    pub fn summary(&self) -> PetSummary {
        migrate_mac_custom_pets_if_needed(&self.support_dir);
        match self.load_snapshot() {
            Ok((snapshot, source)) => {
                let mut snapshot = sanitize_state(snapshot);
                reset_daily_tokens_if_needed(&mut snapshot, now_seconds());
                let custom_pet_count = load_custom_pets(&self.support_dir, false).len();
                PetSummary {
                    available: true,
                    claimed: snapshot.claimed_at.is_some(),
                    species: snapshot.species.clone(),
                    display_name: pet_display_name(&snapshot),
                    custom_name: snapshot.custom_name.clone(),
                    level: snapshot.progress.level,
                    total_xp: snapshot.progress.total_xp,
                    progress: snapshot.progress.progress,
                    daily_xp: snapshot.daily_experience_tokens,
                    archived_count: snapshot.legacy.len(),
                    custom_pet_count,
                    updated_at: Some(snapshot.updated_at),
                    source,
                    error: None,
                }
            }
            Err(error) => PetSummary {
                source: self.support_dir.display().to_string(),
                error: Some(error),
                ..Default::default()
            },
        }
    }

    pub fn catalog(&self) -> PetCatalog {
        migrate_mac_custom_pets_if_needed(&self.support_dir);
        pet_catalog(self.support_dir.clone())
    }

    pub fn bundled_catalog(&self) -> PetCatalog {
        bundled_pet_catalog()
    }

    pub fn hydrate_custom_pet_data_url(&self, pet: PetCustomPet) -> PetCustomPet {
        hydrate_custom_pet_data_url(&self.support_dir, pet)
    }

    pub async fn resolve_custom_pet_install(
        &self,
        request: PetCustomPetInstallRequest,
    ) -> Result<PetCustomPetInstallPreview, String> {
        super::install::resolve_custom_pet_install_with_cache(self.support_dir.clone(), request)
            .await
    }

    pub async fn install_custom_pet(
        &self,
        request: PetCustomPetInstallRequest,
    ) -> Result<PetCustomPet, String> {
        install_custom_pet(self.support_dir.clone(), request).await
    }

    pub(super) fn load_snapshot(&self) -> Result<(PetSnapshot, String), String> {
        let local_state = self.load_local_snapshot()?;
        if local_state
            .as_ref()
            .is_some_and(|(state, _)| state.claimed_at.is_some())
        {
            return Ok(local_state.unwrap());
        }
        let mac_state = load_mac_pet_state();
        if mac_state
            .as_ref()
            .is_some_and(|(state, _)| state.claimed_at.is_some())
        {
            return Ok(mac_state.unwrap());
        }
        local_state
            .or(mac_state)
            .ok_or_else(|| "pet state not found".to_string())
    }

    fn load_local_snapshot(&self) -> Result<Option<(PetSnapshot, String)>, String> {
        for path in self.local_state_paths() {
            let Ok(data) = fs::read(&path) else {
                continue;
            };
            if data.is_empty() {
                continue;
            }
            if let Some(decoded) = decode_pet_state_data(&data, PET_STATE_DECODE_NAMESPACES) {
                return serde_json::from_slice::<PetSnapshot>(&decoded)
                    .map(|snapshot| Some((snapshot, path.display().to_string())))
                    .map_err(|error| error.to_string());
            }
        }
        Ok(None)
    }

    fn local_state_paths(&self) -> Vec<PathBuf> {
        dedupe_paths(vec![
            self.support_dir.join("pet-state.dat"),
            self.support_dir.join("pet-state.json"),
        ])
    }
}
