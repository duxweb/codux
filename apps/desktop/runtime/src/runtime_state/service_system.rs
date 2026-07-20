impl RuntimeService {
    pub fn terminal_font_families(&self) -> Vec<String> {
        crate::system_fonts::terminal_font_families()
    }

    pub fn reload_update(&self, repo_root: PathBuf) -> UpdateSummary {
        load_update(&self.support_dir, repo_root)
    }

    pub fn reload_update_settings(&self, repo_root: PathBuf) -> UpdateSummary {
        UpdateService::new(self.support_dir.clone(), repo_root).settings_summary()
    }

    pub fn update_status(&self, repo_root: PathBuf, current_version: &str) -> UpdateStatus {
        UpdateService::new(self.support_dir.clone(), repo_root).status(current_version)
    }

    pub fn install_update(
        &self,
        repo_root: PathBuf,
        current_version: &str,
    ) -> Result<UpdateInstallResult, String> {
        let settings = AppSettingsStore::from_support_dir(self.support_dir.clone()).snapshot();
        crate::app_info::install_update(&settings, repo_root, current_version)
    }

    pub fn install_update_with_progress(
        &self,
        repo_root: PathBuf,
        current_version: &str,
        on_progress: impl FnMut(crate::app_info::UpdateInstallProgressEvent) + Send,
    ) -> Result<UpdateInstallResult, String> {
        let settings = AppSettingsStore::from_support_dir(self.support_dir.clone()).snapshot();
        crate::app_info::install_update_with_progress(
            &settings,
            repo_root,
            current_version,
            on_progress,
        )
    }

    pub fn request_restart(&self) -> Result<(), String> {
        crate::app_info::request_restart()
    }

    pub fn i18n_bundle(&self) -> I18nBundle {
        i18n::i18n_bundle()
    }

    pub fn about_metadata(
        &self,
        version: impl Into<String>,
        identifier: impl Into<String>,
    ) -> AppAboutMetadata {
        crate::app_info::about_metadata(version, identifier)
    }

    pub fn export_diagnostics(
        &self,
        request: DiagnosticsExportRequest,
        about: AppAboutMetadata,
        update: UpdateStatus,
    ) -> Result<DiagnosticsExportResult, String> {
        crate::app_info::export_diagnostics(
            request,
            about,
            update,
            AppDiagnosticsSnapshot {
                settings: read_json_or_default(crate::config::settings_file_path(
                    self.support_dir.clone(),
                )),
                projects: read_json_or_default(crate::config::state_file_path(
                    self.support_dir.clone(),
                )),
                ai_state: serde_json::to_value(
                    self.reload_ai_runtime_state(&self.reload_runtime_events()),
                )
                .unwrap_or_else(|_| json!({})),
                performance: serde_json::to_value(self.reload_performance())
                    .unwrap_or_else(|_| json!({})),
                ssh: serde_json::to_value(self.reload_ssh(RuntimeInventory::load().root))
                    .unwrap_or_else(|_| json!({})),
            },
        )
    }

    pub fn write_runtime_log_preview(&self) -> Result<PathBuf, String> {
        crate::app_info::write_runtime_log_preview()
    }

    pub fn ensure_live_log(&self) -> Result<PathBuf, String> {
        crate::app_info::ensure_live_log()
    }

    pub fn open_runtime_log(&self) -> Result<(), String> {
        crate::app_info::open_runtime_log()
    }

    pub fn open_live_log(&self) -> Result<(), String> {
        crate::app_info::open_live_log()
    }

    pub fn open_url(&self, url: &str) -> Result<(), String> {
        crate::app_info::open_url(url)
    }

    pub fn open_url_with_http_proxy(
        &self,
        url: &str,
        proxy_host: &str,
        proxy_port: u16,
    ) -> Result<(), String> {
        crate::app_info::open_url_with_http_proxy(url, proxy_host, proxy_port)
    }

    pub fn dispatch_notification_channels(
        &self,
        request: NotificationDispatchRequest,
    ) -> NotificationDispatchResult {
        crate::notification::dispatch_notification_channels_blocking(request)
    }

    pub fn show_native_notification(
        &self,
        title: &str,
        body: &str,
        group: &str,
    ) -> Result<(), String> {
        crate::notification::show_native_notification_blocking(title, body, group)
    }

    pub fn set_dock_badge_count(&self, count: Option<i64>) -> Result<(), String> {
        crate::dock_badge::set_dock_badge_count(count)
    }

    pub fn localized_open_dialog(
        &self,
        request: LocalizedOpenDialogRequest,
    ) -> Result<Option<Vec<String>>, String> {
        crate::dialog::localized_open_dialog(request)
    }

    pub fn localized_save_dialog(
        &self,
        request: LocalizedSaveDialogRequest,
    ) -> Result<Option<String>, String> {
        crate::dialog::localized_save_dialog(request)
    }

    pub fn localized_confirm_dialog(
        &self,
        request: LocalizedConfirmDialogRequest,
    ) -> Result<bool, String> {
        crate::dialog::localized_confirm_dialog(request)
    }

    pub fn localized_alert_dialog(
        &self,
        request: LocalizedAlertDialogRequest,
    ) -> Result<(), String> {
        crate::dialog::localized_alert_dialog(request)
    }

    pub fn desktop_pet_saved_origin(&self) -> Option<DesktopPetSavedOrigin> {
        DesktopPetService::new(self.support_dir.clone()).saved_origin()
    }

    pub fn save_desktop_pet_origin(&self, origin: DesktopPetSavedOrigin) -> Result<(), String> {
        DesktopPetService::new(self.support_dir.clone()).save_origin(origin)
    }

    pub fn desktop_pet_initial_position(
        &self,
        work_area: DesktopPetWorkArea,
    ) -> DesktopPetSavedOrigin {
        DesktopPetService::new(self.support_dir.clone()).initial_position(work_area)
    }

    pub fn desktop_pet_placement(
        &self,
        position: DesktopPetPhysicalPosition,
        size: DesktopPetPhysicalSize,
        work_area: DesktopPetWorkArea,
    ) -> DesktopPetPlacementSnapshot {
        crate::desktop_pet::desktop_pet_placement_for_window(position, size, work_area)
    }

    pub fn desktop_pet_should_click_through(
        &self,
        layout: DesktopPetHitLayout,
        cursor: DesktopPetPhysicalPosition,
        has_bubble: bool,
    ) -> bool {
        crate::desktop_pet::desktop_pet_should_click_through(layout, cursor, has_bubble)
    }

    pub fn desktop_pet_should_show(&self) -> Result<bool, String> {
        DesktopPetService::new(self.support_dir.clone()).should_show()
    }

    pub fn desktop_pet_set_bubble_visible(&self, visible: bool) -> DesktopPetVisibilitySnapshot {
        DesktopPetService::new(self.support_dir.clone()).set_bubble_visible(visible)
    }

    pub fn desktop_pet_sync_visibility(&self) -> Result<DesktopPetVisibilitySnapshot, String> {
        let snapshot = DesktopPetService::new(self.support_dir.clone()).sync_visibility()?;
        crate::runtime_trace::runtime_trace(
            "desktop-pet",
            &format!(
                "manual_sync should_show={} bubble_visible={}",
                snapshot.should_show, snapshot.bubble_visible
            ),
        );
        Ok(snapshot)
    }

    pub fn apply_desktop_pet_menu_action(&self, action_id: &str) -> Result<AppSettings, String> {
        DesktopPetService::new(self.support_dir.clone()).apply_menu_action(action_id)
    }

    pub fn reload_runtime_activity(&self) -> RuntimeActivitySummary {
        load_runtime_activity(&self.support_dir)
    }

    pub fn reload_performance(&self) -> PerformanceSummary {
        load_performance()
    }

    pub fn reload_runtime_events(&self) -> RuntimeEventSummary {
        load_runtime_events()
    }

    pub fn reload_ai_runtime_state(&self, events: &RuntimeEventSummary) -> AIRuntimeStateSummary {
        load_ai_runtime_state(&self.support_dir, events)
    }

    pub fn reload_remote(&self) -> RemoteSummary {
        self.remote_host.snapshot()
    }

    pub fn drain_remote_events(&self) -> Vec<RemoteHostEvent> {
        let mut events = self.remote_host.drain_events();
        events.extend(self.drain_hosted_workspace_updates());
        if let Ok(mut hosted) = self.hosted_runtime_events.lock() {
            events.extend(hosted.drain(..));
        }
        events
    }

    /// Notify connected mobile clients that the terminal set changed (e.g. a
    /// terminal was closed on the desktop) so they reconcile their view.
    pub fn broadcast_remote_terminal_list(&self) {
        self.remote_host.broadcast_terminal_list_change();
    }

    /// Notify connected controllers that the project list changed (created /
    /// renamed / reordered / closed on the desktop) so they reconcile live
    /// instead of waiting for a reconnect or pull-to-refresh.
    pub fn broadcast_remote_project_list(&self) {
        self.remote_host.broadcast_project_list_change();
    }

    /// Push refreshed git status to controllers after a desktop git mutation so
    /// their git view reconciles instead of showing stale status.
    pub fn broadcast_remote_git_status(&self, project_id: &str, project_path: &str) {
        self.remote_host
            .broadcast_git_status_change(project_id, project_path);
    }

    /// Notify controllers that an AI session was removed/changed on the desktop
    /// so each refreshes its own session list (drops the deleted row live).
    pub fn broadcast_remote_ai_session_changed(&self) {
        self.remote_host.broadcast_ai_session_changed();
    }

    /// Push freshly-indexed AI stats to remote devices that requested them while
    /// the project's index was still cold (see `flush_pending_ai_stats`).
    pub fn flush_remote_ai_stats(&self, state: &crate::ai_history_indexer::AIHistoryProjectState) {
        self.remote_host.flush_pending_ai_stats(state);
    }

    /// Re-push `ai.stats` to watching remote devices after the live AI runtime
    /// state changed, so their views tick like the desktop's local view.
    pub fn push_remote_ai_stats_to_watchers(&self) {
        self.remote_host.push_ai_stats_to_watchers();
    }

    pub fn shutdown_runtime_state(&self) {
        self.remote_host.shutdown();
    }

    pub fn set_remote_enabled(&self, enabled: bool) -> Result<RemoteSummary, String> {
        let service = RemoteService::new(self.support_dir.clone());
        service.set_enabled(enabled)?;
        if enabled {
            Ok(self.remote_host.start())
        } else {
            self.remote_host.stop_with_message("Remote Host stopped.");
            Ok(self.remote_host.snapshot())
        }
    }

    pub fn set_remote_relay_url(
        &self,
        relay_url: &str,
    ) -> Result<(SettingsSummary, RemoteSummary), String> {
        self.set_remote_relay_url_with_device_reset(relay_url, false)
    }

    pub fn set_remote_relay_url_with_device_reset(
        &self,
        relay_url: &str,
        reset_devices: bool,
    ) -> Result<(SettingsSummary, RemoteSummary), String> {
        let relay_url = relay_url.trim();
        let relay_preset = if relay_url.is_empty() {
            crate::remote::remote_relay_preset_for_url(relay_url)
        } else {
            "custom".to_string()
        };
        self.update_remote_relay_settings(relay_preset, relay_url.to_string(), reset_devices)
    }

    pub fn set_remote_relay_preset(
        &self,
        relay_preset: &str,
    ) -> Result<(SettingsSummary, RemoteSummary), String> {
        self.set_remote_relay_preset_with_device_reset(relay_preset, false)
    }

    pub fn set_remote_relay_preset_with_device_reset(
        &self,
        relay_preset: &str,
        reset_devices: bool,
    ) -> Result<(SettingsSummary, RemoteSummary), String> {
        let current = self.reload_settings();
        let relay_preset =
            crate::remote::normalize_remote_relay_preset(relay_preset, &current.remote_relay_url);
        let relay_url =
            crate::remote::remote_relay_url_for_preset(&relay_preset, &current.remote_relay_url);
        self.update_remote_relay_settings(relay_preset, relay_url, reset_devices)
    }

    pub fn set_remote_relay_authentication_with_device_reset(
        &self,
        relay_authentication: &str,
        reset_devices: bool,
    ) -> Result<(SettingsSummary, RemoteSummary), String> {
        let relay_authentication = relay_authentication.trim().to_string();
        let app_settings = self.update_app_settings(|settings| {
            settings.remote.relay_authentication = relay_authentication;
            if reset_devices {
                settings.remote.cached_devices.clear();
            }
        })?;
        if reset_devices {
            self.remote_host.clear_pairing_state();
        }
        let remote = if app_settings.remote.is_enabled {
            self.remote_host.reconnect()
        } else {
            self.remote_host.reload_snapshot_from_settings()
        };
        let settings = self.reload_settings();
        Ok((settings, remote))
    }

    fn update_remote_relay_settings(
        &self,
        relay_preset: String,
        relay_url: String,
        reset_devices: bool,
    ) -> Result<(SettingsSummary, RemoteSummary), String> {
        let app_settings = self.update_app_settings(|settings| {
            settings.remote.relay_preset = relay_preset;
            settings.remote.relay_url = relay_url;
            if reset_devices {
                settings.remote.cached_devices.clear();
            }
        })?;
        if reset_devices {
            self.remote_host.clear_pairing_state();
        }
        let remote = if app_settings.remote.is_enabled {
            self.remote_host.reconnect()
        } else {
            self.remote_host.reload_snapshot_from_settings()
        };
        let settings = self.reload_settings();
        Ok((settings, remote))
    }

    pub fn revoke_remote_device(&self, device_id: &str) -> Result<RemoteSummary, String> {
        RemoteService::new(self.support_dir.clone()).revoke_device(device_id)?;
        Ok(self.remote_host.reload_snapshot_from_settings())
    }

    pub fn refresh_remote_devices(&self) -> Result<RemoteSummary, String> {
        RemoteService::new(self.support_dir.clone()).refresh_devices()?;
        Ok(self.remote_host.reload_snapshot_from_settings())
    }

    pub fn register_remote_host(&self) -> Result<RemoteSummary, String> {
        RemoteService::new(self.support_dir.clone()).register_host()?;
        Ok(self.remote_host.reload_snapshot_from_settings())
    }

    pub fn reconnect_remote(&self) -> Result<RemoteSummary, String> {
        // Reconnect both directions: this machine's own relay registration AND
        // every saved outbound host link (the device list), so the button isn't
        // limited to re-registering the local host.
        self.ensure_saved_remote_hosts_connected();
        Ok(self.remote_host.reconnect())
    }

    pub fn create_remote_pairing(&self) -> Result<RemoteSummary, String> {
        self.remote_host.create_pairing()
    }

    pub async fn create_remote_pairing_async(&self) -> Result<RemoteSummary, String> {
        self.remote_host.create_pairing_async().await
    }

    pub fn poll_remote_pairing_status(
        &self,
        pairing: &RemotePairingInfo,
    ) -> Result<RemotePairingPollResult, String> {
        self.remote_host.poll_pairing_status(pairing)
    }

    pub fn cancel_remote_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        self.remote_host.cancel_pairing(pairing_id)
    }

    pub fn confirm_remote_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        self.remote_host.confirm_pairing(pairing_id)
    }

    pub fn reject_remote_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        self.remote_host.reject_pairing(pairing_id)
    }

    pub fn reload_pet(&self) -> PetSummary {
        load_pet(&self.support_dir)
    }

    pub fn pet_catalog(&self) -> PetCatalog {
        PetService::new(self.support_dir.clone()).catalog()
    }

    pub fn hydrate_custom_pet_data_url(&self, pet: PetCustomPet) -> PetCustomPet {
        PetService::new(self.support_dir.clone()).hydrate_custom_pet_data_url(pet)
    }

    pub fn custom_pet_sprite(&self, pet: PetCustomPet) -> PetCustomPet {
        self.hydrate_custom_pet_data_url(pet)
    }

    pub async fn resolve_custom_pet_install(
        &self,
        request: PetCustomPetInstallRequest,
    ) -> Result<PetCustomPetInstallPreview, String> {
        PetService::new(self.support_dir.clone())
            .resolve_custom_pet_install(request)
            .await
    }

    pub async fn install_custom_pet(
        &self,
        request: PetCustomPetInstallRequest,
    ) -> Result<PetCustomPet, String> {
        PetService::new(self.support_dir.clone())
            .install_custom_pet(request)
            .await
    }

    pub fn pet_snapshot(&self) -> Result<PetSnapshot, String> {
        PetStore::load_or_seed(self.support_dir.clone()).snapshot()
    }

    pub fn refresh_pet(&self, request: PetRefreshInput) -> Result<PetSnapshot, String> {
        PetStore::load_or_seed(self.support_dir.clone()).refresh(request)
    }

    pub fn refresh_pet_from_indexed_history(&self) -> Result<PetSummary, String> {
        let store = PetStore::load_or_seed(self.support_dir.clone());
        let current = sync_pet_memberships_from_support_dir(&self.support_dir)?;
        if current.claimed_at.is_none() {
            return Ok(self.reload_pet());
        }
        let usage_store =
            crate::ai_usage_store::AIUsageStore::at_path(self.ai_usage_database_path());
        let conn = usage_store.connect().map_err(|error| error.to_string())?;
        let intervals = pet_usage_intervals(&current.memberships, None);
        let experience_tokens = usage_store
            .normalized_tokens_in_intervals(&conn, &intervals)
            .map_err(|error| error.to_string())?;
        let today_start = crate::ai_history_normalized::local_day_start_seconds(
            crate::ai_history_normalized::now_seconds(),
        ) as i64;
        let daily_experience_tokens = usage_store
            .normalized_tokens_in_intervals(
                &conn,
                &pet_usage_intervals(&current.memberships, Some(today_start)),
            )
            .map_err(|error| error.to_string())?;
        let sessions = usage_store
            .interval_sessions(&conn, &intervals)
            .map_err(|error| error.to_string())?;
        store.refresh(PetRefreshInput {
            experience_tokens,
            daily_experience_tokens,
            computed_stats: crate::pet::pet_stats_from_history_sessions(&sessions),
        })?;
        Ok(self.reload_pet())
    }

    pub fn pet_idle_speech(
        &self,
        request: PetIdleSpeechRequest,
    ) -> Result<PetIdleSpeechResponse, String> {
        let service = SettingsService::new(self.support_dir.clone());
        let settings = service.ai_settings();
        let language = service.summary().language;
        crate::async_runtime::block_on(llm::pet_idle_speech_with_settings(
            &settings, &language, request,
        ))
    }

    pub fn claim_pet(&self, request: PetClaimInput) -> Result<PetSnapshot, String> {
        PetStore::load_or_seed(self.support_dir.clone()).claim(request)
    }

    pub fn claim_pet_from_indexed_history(
        &self,
        request: crate::pet::PetClaimRequest,
    ) -> Result<PetSnapshot, String> {
        let input = PetClaimInput {
            species: request.species,
            custom_name: request.custom_name,
            custom_pet: request.custom_pet,
            workspaces: self.pet_workspaces(),
        };
        self.claim_pet(input)
    }

    pub(super) fn sync_pet_project_memberships(&self) {
        note_pet_project_membership_change(&self.support_dir);
    }

    fn pet_workspaces(&self) -> Vec<PetWorkspace> {
        pet_workspaces_from_support_dir(&self.support_dir)
    }

    fn ai_usage_database_path(&self) -> PathBuf {
        self.support_dir.join("ai-usage.sqlite3")
    }

    pub fn rename_pet(&self, request: PetRenameRequest) -> Result<PetSnapshot, String> {
        PetStore::load_or_seed(self.support_dir.clone()).rename(request)
    }

    pub fn archive_current_pet(&self) -> Result<PetSnapshot, String> {
        self.refresh_pet_from_indexed_history()?;
        PetStore::load_or_seed(self.support_dir.clone()).archive_current()
    }

    pub fn restore_archived_pet(&self, request: PetRestoreRequest) -> Result<PetSnapshot, String> {
        // Recalibrate the displaced current pet before it is archived, so a
        // stale XP value is never baked into its legacy record.
        self.refresh_pet_from_indexed_history()?;
        PetStore::load_or_seed(self.support_dir.clone())
            .restore_archived(request, self.pet_workspaces())?;
        self.refresh_pet_from_indexed_history()?;
        self.pet_snapshot()
    }

    pub fn set_sleep_mode(&self, mode: &str) -> Result<(SettingsSummary, PowerSummary), String> {
        let settings = SettingsService::new(self.support_dir.clone()).set_sleep_mode(mode)?;
        let mut power = self.power_manager.summary(&settings.sleep_mode);
        if let Err(error) = self
            .power_manager
            .set_sleep_prevention(settings.sleep_mode.clone())
        {
            power.error = Some(error);
        } else {
            power = self.power_manager.summary(&settings.sleep_mode);
        }
        Ok((settings, power))
    }

    pub fn power_summary(&self, mode: &str) -> PowerSummary {
        self.power_manager.summary(mode)
    }

    pub fn set_power_sleep_prevention(&self, mode: &str) -> Result<bool, String> {
        self.power_manager
            .set_sleep_prevention(mode.trim().to_string())
    }

    pub fn start_power_settings_sync(&self) -> Result<(), String> {
        self.power_manager
            .start_settings_sync(Arc::new(AppSettingsStore::from_support_dir(
                self.support_dir.clone(),
            )))
    }

    pub fn sync_tool_permissions(&self) -> ToolPermissionsSummary {
        ToolPermissionsService::new(self.support_dir.clone()).sync()
    }
}

// Shared by RuntimeService and the remote host runtime so every project
// list mutation records pet membership boundaries.
pub(crate) fn sync_pet_memberships_from_support_dir(
    support_dir: &std::path::Path,
) -> Result<PetSnapshot, String> {
    let store = PetStore::load_or_seed(support_dir.to_path_buf());
    let migration_paths = if store.snapshot()?.state_version
        < crate::pet::PetSnapshot::default().state_version
    {
        let usage_store =
            crate::ai_usage_store::AIUsageStore::at_path(support_dir.join("ai-usage.sqlite3"));
        let conn = usage_store.connect().map_err(|error| error.to_string())?;
        usage_store
            .indexed_project_paths(&conn)
            .map_err(|error| error.to_string())?
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    store.sync_memberships(
        pet_workspaces_from_support_dir(support_dir),
        migration_paths,
    )
}

pub(crate) fn note_pet_project_membership_change(support_dir: &std::path::Path) {
    if let Err(error) = sync_pet_memberships_from_support_dir(support_dir) {
        crate::runtime_trace::runtime_trace(
            "pet",
            &format!("project membership sync failed: {error}"),
        );
    }
}

fn pet_workspaces_from_support_dir(support_dir: &std::path::Path) -> Vec<PetWorkspace> {
    ai_history_workspace_requests_from_support_dir(support_dir)
        .into_iter()
        .map(|workspace| PetWorkspace {
            project_path: workspace.path,
        })
        .collect()
}

fn pet_usage_intervals(
    memberships: &[PetProjectMembership],
    lower_bound: Option<i64>,
) -> Vec<crate::ai_usage_store::AIUsageInterval> {
    memberships
        .iter()
        .filter_map(|membership| {
            let included_at = lower_bound
                .map(|bound| membership.included_at.max(bound))
                .unwrap_or(membership.included_at);
            if membership
                .excluded_at
                .is_some_and(|excluded_at| excluded_at <= included_at)
            {
                return None;
            }
            Some(crate::ai_usage_store::AIUsageInterval {
                project_path: membership.project_path.clone(),
                included_at,
                excluded_at: membership.excluded_at,
            })
        })
        .collect()
}
