use super::*;

impl PetStore {
    pub fn load_or_seed(support_dir: PathBuf) -> Arc<Self> {
        static STORES: OnceLock<Mutex<HashMap<PathBuf, Weak<PetStore>>>> = OnceLock::new();
        let state_file = support_dir.join("pet-state.dat");
        let stores = STORES.get_or_init(|| Mutex::new(HashMap::new()));
        let mut registry = stores.lock().unwrap_or_else(|error| error.into_inner());
        registry.retain(|_, store| store.strong_count() > 0);
        if let Some(store) = registry.get(&state_file).and_then(Weak::upgrade) {
            return store;
        }
        let store = Arc::new(Self::load_from_disk(support_dir, state_file.clone()));
        registry.insert(state_file, Arc::downgrade(&store));
        store
    }

    fn load_from_disk(support_dir: PathBuf, state_file: PathBuf) -> Self {
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

    pub fn sync_memberships(
        &self,
        workspaces: Vec<PetWorkspace>,
        migration_paths: Vec<String>,
    ) -> Result<PetSnapshot, String> {
        self.sync_memberships_at(workspaces, migration_paths, now_seconds())
    }

    pub(crate) fn sync_memberships_at(
        &self,
        workspaces: Vec<PetWorkspace>,
        migration_paths: Vec<String>,
        now: i64,
    ) -> Result<PetSnapshot, String> {
        self.with_mut_snapshot(|state| {
            sync_project_memberships(state, workspaces, migration_paths, now);
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
            let legacy = std::mem::take(&mut state.legacy);
            *state = PetSnapshot {
                claimed_at: Some(now),
                species,
                custom_pet,
                custom_name: sanitize_custom_name(&request.custom_name),
                persona_id: default_persona_id(),
                progress: PetProgressInfo::default(),
                daily_experience_day: Some(day_index(now)),
                legacy,
                updated_at: now,
                ..PetSnapshot::default()
            };
            sync_project_memberships(state, request.workspaces, Vec::new(), now);
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
            let now = now_seconds();
            close_open_memberships(&mut state.memberships, now);
            let record = legacy_record_from_state(state)
                .ok_or_else(|| "No pet has been claimed.".to_string())?;
            let mut legacy = std::mem::take(&mut state.legacy);
            legacy.insert(0, record);
            *state = PetSnapshot {
                legacy,
                daily_experience_day: Some(day_index(now)),
                updated_at: now,
                ..PetSnapshot::default()
            };
            Ok(())
        })
    }

    pub fn restore_archived(
        &self,
        request: PetRestoreRequest,
        workspaces: Vec<PetWorkspace>,
    ) -> Result<PetSnapshot, String> {
        self.with_mut_snapshot(|state| {
            let index = state
                .legacy
                .iter()
                .position(|record| record.id == request.legacy_id)
                .ok_or_else(|| "Archived pet not found.".to_string())?;
            let mut legacy = std::mem::take(&mut state.legacy);
            let record = legacy.remove(index);
            let now = now_seconds();
            close_open_memberships(&mut state.memberships, now);
            if let Some(current) = legacy_record_from_state(state) {
                legacy.insert(0, current);
            }
            *state = PetSnapshot {
                claimed_at: Some(now),
                species: sanitize_species(&record.species),
                custom_pet: record.custom_pet,
                custom_name: record.custom_name,
                current_experience_tokens: record.total_xp.max(0),
                memberships: record.memberships,
                experience_base_tokens: record.experience_base_tokens.max(0),
                current_stats: record.stats.sanitized(),
                persona_id: record.persona_id,
                progress: record.progress,
                stats_updated_day: Some(now),
                legacy,
                daily_experience_day: Some(day_index(now)),
                updated_at: now,
                ..PetSnapshot::default()
            };
            sync_project_memberships(state, workspaces, Vec::new(), now);
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
        let mut snapshot = state.clone();
        apply(&mut snapshot)?;
        let snapshot = sanitize_state(snapshot);
        if snapshot == *state {
            return Ok(snapshot);
        }
        self.save(&snapshot)?;
        *state = snapshot.clone();
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

fn sync_project_memberships(
    state: &mut PetSnapshot,
    workspaces: Vec<PetWorkspace>,
    migration_paths: Vec<String>,
    now: i64,
) {
    if state.claimed_at.is_none() {
        state.state_version = STATE_VERSION;
        return;
    }
    let workspaces = sanitize_workspaces(workspaces);
    let previous_memberships = state.memberships.clone();
    let previous_state_version = state.state_version;
    let previous_recalibration_pending = state.experience_recalibration_pending;
    let previous_pending_watermarks = state.pending_project_token_watermarks.clone();
    if state.state_version < STATE_VERSION {
        state.experience_recalibration_pending = true;
        let included_at = state.claimed_at.unwrap_or(now).min(now);
        state.memberships.clear();
        state.pending_project_token_watermarks.clear();
        let mut paths = migration_paths
            .into_iter()
            .map(|path| normalize_workspace_path(&path))
            .filter(|path| !path.is_empty())
            .collect::<Vec<_>>();
        paths.extend(
            workspaces
                .iter()
                .map(|workspace| workspace.project_path.clone()),
        );
        paths.sort();
        paths.dedup();
        for project_path in paths {
            let active = workspaces
                .iter()
                .any(|workspace| workspace.project_path == project_path);
            state.memberships.push(PetProjectMembership {
                project_path,
                included_at,
                excluded_at: (!active).then_some(now.max(included_at)),
            });
        }
    }
    for membership in state
        .memberships
        .iter_mut()
        .filter(|membership| membership.excluded_at.is_none())
    {
        let active = workspaces
            .iter()
            .any(|workspace| workspace.project_path == membership.project_path);
        if !active {
            membership.excluded_at = Some(now.max(membership.included_at));
        }
    }
    let migrated_start = if state.state_version < STATE_VERSION {
        state.claimed_at.unwrap_or(now).min(now)
    } else {
        now
    };
    for workspace in workspaces {
        let already_open = state.memberships.iter().any(|membership| {
            membership.excluded_at.is_none() && membership.project_path == workspace.project_path
        });
        if !already_open {
            state.memberships.push(PetProjectMembership {
                project_path: workspace.project_path,
                included_at: migrated_start,
                excluded_at: None,
            });
        }
    }
    state.state_version = STATE_VERSION;
    if state.memberships != previous_memberships
        || state.state_version != previous_state_version
        || state.experience_recalibration_pending != previous_recalibration_pending
        || state.pending_project_token_watermarks != previous_pending_watermarks
    {
        state.updated_at = now;
    }
}

fn sanitize_workspaces(workspaces: Vec<PetWorkspace>) -> Vec<PetWorkspace> {
    let mut sanitized = Vec::new();
    for workspace in workspaces {
        let project_path = normalize_workspace_path(&workspace.project_path);
        if project_path.is_empty() {
            continue;
        }
        let workspace = PetWorkspace { project_path };
        if !sanitized.contains(&workspace) {
            sanitized.push(workspace);
        }
    }
    sanitized
}

fn close_open_memberships(memberships: &mut [PetProjectMembership], now: i64) {
    for membership in memberships
        .iter_mut()
        .filter(|membership| membership.excluded_at.is_none())
    {
        membership.excluded_at = Some(now.max(membership.included_at));
    }
}
