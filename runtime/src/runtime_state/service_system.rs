impl RuntimeService {
    pub fn reload_update(&self, repo_root: PathBuf) -> UpdateSummary {
        load_update(&self.support_dir, repo_root)
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
                settings: read_json_or_default(self.support_dir.join("settings.json")),
                projects: read_json_or_default(self.support_dir.join("state.json")),
                ai_runtime: read_json_or_default(
                    self.support_dir.join("gpui-ai-runtime-state.json"),
                ),
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

    pub fn drain_remote_events(&self) -> Vec<RemoteSummary> {
        self.remote_host.drain_events()
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

    pub fn set_remote_server_url(&self, server_url: &str) -> Result<RemoteSummary, String> {
        let service = RemoteService::new(self.support_dir.clone());
        service.set_server_url(server_url)?;
        Ok(self.remote_host.start())
    }

    pub fn revoke_remote_device(&self, device_id: &str) -> Result<RemoteSummary, String> {
        let summary = RemoteService::new(self.support_dir.clone()).revoke_device(device_id)?;
        Ok(self.remote_host.apply_snapshot(summary))
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
        RemoteService::new(self.support_dir.clone()).register_host()?;
        Ok(self.remote_host.reconnect())
    }

    pub fn create_remote_pairing(&self) -> Result<RemoteSummary, String> {
        let summary = RemoteService::new(self.support_dir.clone()).create_pairing()?;
        Ok(self.remote_host.apply_snapshot(summary))
    }

    pub fn poll_remote_pairing_status(
        &self,
        pairing: &RemotePairingInfo,
    ) -> Result<RemotePairingPollResult, String> {
        let mut result = RemoteService::new(self.support_dir.clone()).poll_pairing_status(pairing)?;
        result.summary = self.remote_host.apply_snapshot(result.summary);
        Ok(result)
    }

    pub fn cancel_remote_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        let summary = RemoteService::new(self.support_dir.clone()).cancel_pairing(pairing_id)?;
        Ok(self.remote_host.apply_snapshot(summary))
    }

    pub fn confirm_remote_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        let summary = RemoteService::new(self.support_dir.clone()).confirm_pairing(pairing_id)?;
        Ok(self.remote_host.apply_snapshot(summary))
    }

    pub fn reject_remote_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        let summary = RemoteService::new(self.support_dir.clone()).reject_pairing(pairing_id)?;
        Ok(self.remote_host.apply_snapshot(summary))
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
        PetService::resolve_custom_pet_install(request).await
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
        let current = store.snapshot()?;
        let claimed_at = current.claimed_at;
        let cutoff = claimed_at.map(|value| value as f64);
        let project_totals =
            normalized_project_totals_since(cutoff).map_err(|error| error.to_string())?;
        let all_time_total_tokens =
            global_all_time_normalized_tokens().map_err(|error| error.to_string())?;
        let sessions = indexed_sessions_since(cutoff).map_err(|error| error.to_string())?;
        let input = refresh_input_from_indexed_history(
            claimed_at,
            project_totals,
            all_time_total_tokens,
            sessions,
        );
        store.refresh(input)?;
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
            &settings,
            &language,
            request,
        ))
    }

    pub fn claim_pet(&self, request: PetClaimInput) -> Result<PetSnapshot, String> {
        PetStore::load_or_seed(self.support_dir.clone()).claim(request)
    }

    pub fn claim_pet_from_indexed_history(
        &self,
        request: crate::pet::PetClaimRequest,
    ) -> Result<PetSnapshot, String> {
        let all_time_total_tokens =
            global_all_time_normalized_tokens().map_err(|error| error.to_string())?;
        let input = crate::pet::claim_input_from_indexed_history(request, all_time_total_tokens);
        self.claim_pet(input)
    }

    pub fn rename_pet(&self, request: PetRenameRequest) -> Result<PetSnapshot, String> {
        PetStore::load_or_seed(self.support_dir.clone()).rename(request)
    }

    pub fn archive_current_pet(&self) -> Result<PetSnapshot, String> {
        PetStore::load_or_seed(self.support_dir.clone()).archive_current()
    }

    pub fn restore_archived_pet(&self, request: PetRestoreRequest) -> Result<PetSnapshot, String> {
        PetStore::load_or_seed(self.support_dir.clone()).restore_archived(request)
    }

    pub fn forget_pet_project_baseline(&self, project_id: &str) -> Result<bool, String> {
        PetStore::load_or_seed(self.support_dir.clone()).forget_project_baseline(project_id)
    }

    pub fn forget_all_pet_project_baselines(&self) -> Result<(), String> {
        PetStore::load_or_seed(self.support_dir.clone()).forget_all_project_baselines()
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
