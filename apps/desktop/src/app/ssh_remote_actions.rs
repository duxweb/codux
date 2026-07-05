use super::ai_runtime_status::AgentLifecycleState;
use super::*;
use crate::app::app_events::{
    ChildWindowUpdateEvent, current_memory_update_event, publish_pet_update,
};

enum RemoteRelayChange {
    Preset(String),
    RelayUrl(String),
    Authentication(String),
}

impl CoduxApp {
    pub(super) fn apply_child_window_update_event(
        &mut self,
        event: ChildWindowUpdateEvent,
        cx: &mut Context<Self>,
    ) -> usize {
        if event.revision <= self.child_window_update_seen_revision {
            return 0;
        }
        self.child_window_update_seen_revision = event.revision;

        let mut applied = 0;
        if event.settings_revision > self.child_window_settings_seen_revision {
            self.child_window_settings_seen_revision = event.settings_revision;
            if self.apply_settings_update_event(cx) {
                applied += 1;
            }
        }
        if event.ssh_revision > self.child_window_ssh_seen_revision {
            self.child_window_ssh_seen_revision = event.ssh_revision;
            self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
            self.normalize_selected_ssh_profile();
            self.invalidate_remote_panel(cx);
            applied += 1;
        }
        if event.memory_revision > self.child_window_memory_seen_revision {
            self.child_window_memory_seen_revision = event.memory_revision;
            self.state.memory = self.runtime_service.reload_memory(
                self.state
                    .selected_project
                    .as_ref()
                    .map(|project| project.id.as_str()),
            );
            self.reload_memory_manager_snapshot();
            self.invalidate_status_bar(cx);
            applied += 1;
        }
        if event.project_revision > self.child_window_project_seen_revision {
            self.child_window_project_seen_revision = event.project_revision;
            let next = self.runtime_service.reload_state();
            self.apply_project_list_state(next, cx);
            self.reload_project_open_applications_async(cx);
            self.normalize_selected_git_branch();
            self.normalize_selected_ai_session();
            self.normalize_selected_runtime_session();
            self.normalize_selected_ssh_profile();
            self.reload_selected_project_db();
            self.normalize_selected_db_profile();
            self.invalidate_project_management(cx);
            self.invalidate_task_column(cx);
            applied += 1;
        }
        if event.worktree_revision > self.child_window_worktree_seen_revision {
            self.child_window_worktree_seen_revision = event.worktree_revision;
            if let Some(project) = self.state.selected_project.as_ref() {
                self.state.worktrees = self
                    .runtime_service
                    .reload_worktrees(Some(&project.id), Some(&project.path));
                self.invalidate_task_column(cx);
                self.refresh_git_panel_state_async(cx);
                applied += 1;
            }
        }
        if event.git_revision > self.child_window_git_seen_revision {
            self.child_window_git_seen_revision = event.git_revision;
            self.git_running_operation =
                event
                    .git_running_label
                    .clone()
                    .map(|label| GitRunningOperation {
                        label,
                        cancellable: false,
                    });
            self.refresh_git_panel_state_async(cx);
            self.invalidate_git_panel(cx);
            applied += 1;
        }
        applied
    }

    pub(super) fn selected_ssh_profile(&self) -> Option<&SSHProfileSummary> {
        self.selected_ssh_profile_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .ssh
                    .profiles
                    .iter()
                    .find(|profile| profile.id == id)
            })
            .or_else(|| self.state.ssh.profiles.first())
    }

    pub(super) fn normalize_selected_ssh_profile(&mut self) {
        let selected_still_exists = self
            .selected_ssh_profile_id
            .as_deref()
            .map(|id| {
                self.state
                    .ssh
                    .profiles
                    .iter()
                    .any(|profile| profile.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_ssh_profile_id = None;
        }
    }

    pub(super) fn select_ssh_profile(
        &mut self,
        profile_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .state
            .ssh
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
        else {
            self.status_message = "SSH profile is no longer available".to_string();
            self.normalize_selected_ssh_profile();
            self.invalidate_remote_panel(cx);
            return;
        };
        self.selected_ssh_profile_id = Some(profile.id.clone());
        self.status_message = format!("selected SSH profile: {}", profile.name);
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn connect_selected_ssh_profile(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.ssh.wrapper_available {
            self.status_message = "codux-ssh wrapper is not available".to_string();
            self.invalidate_remote_panel(cx);
            return;
        }
        let Some(profile) = self.selected_ssh_profile().cloned() else {
            self.status_message = "no SSH profile selected".to_string();
            self.invalidate_remote_panel(cx);
            return;
        };
        match self.runtime_service.ssh_launch_command(profile.id.clone()) {
            Ok(command) => {
                self.send_to_active_terminal(&terminal_command_text(&command.command), cx);
                if let Some(view) = self.active_terminal_view() {
                    view.read(cx).focus_handle().focus(window, cx);
                }
                self.status_message = format!("SSH connect sent: {}", profile.name);
            }
            Err(error) => {
                self.status_message = format!("failed to build SSH launch command: {error}");
            }
        }
        self.sync_project_activity_state(cx);
        self.invalidate_task_column(cx);
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn apply_ssh_draft(&mut self, profile: SSHConnectionProfile) {
        self.ssh_draft_id = Some(profile.id);
        self.ssh_draft_name = profile.name;
        self.ssh_draft_host = profile.host;
        self.ssh_draft_port = profile.port.to_string();
        self.ssh_draft_username = profile.username;
        self.ssh_draft_credential_kind = profile.credential_kind;
        self.ssh_draft_private_key_path = profile.private_key_path;
        self.ssh_draft_password = profile.password.unwrap_or_default();
        self.ssh_draft_key_passphrase = profile.key_passphrase.unwrap_or_default();
    }

    pub(super) fn clear_ssh_test_result(&mut self) {
        self.ssh_test_result = None;
    }

    fn set_ssh_test_result(&mut self, message: String, ok: bool) {
        self.ssh_test_result = Some(SSHProfileTestDisplay { message, ok });
    }

    fn ssh_test_testing_message(&self) -> String {
        let locale = locale_from_language_setting(&self.state.settings.language);
        translate(&locale, "ssh.profile.test.testing", "Testing...")
    }

    pub(super) fn set_ssh_draft_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_name = value;
        self.clear_ssh_test_result();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_ssh_draft_host(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_host = value;
        self.clear_ssh_test_result();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_ssh_draft_port(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_port = value;
        self.clear_ssh_test_result();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_ssh_draft_username(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_username = value;
        self.clear_ssh_test_result();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_ssh_draft_credential_kind(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_credential_kind = value;
        self.clear_ssh_test_result();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_ssh_draft_private_key_path(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_private_key_path = value;
        self.clear_ssh_test_result();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn choose_ssh_private_key_path(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let default_path = self.ssh_draft_private_key_path.trim();
        let start_path = if default_path.is_empty() {
            None
        } else {
            std::path::Path::new(default_path)
                .parent()
                .map(|parent| parent.to_string_lossy().to_string())
        };
        self.open_file_picker_window(
            FilePickerMode::OpenFile,
            FilePickerTarget::SshPrivateKeyPath,
            None,
            start_path,
            None,
            window,
            cx,
        );
    }

    pub(super) fn set_ssh_draft_password(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_password = value;
        self.clear_ssh_test_result();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_ssh_draft_key_passphrase(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_key_passphrase = value;
        self.clear_ssh_test_result();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn ssh_draft_request(&self) -> Result<SSHProfileUpsertRequest, String> {
        let port = self
            .ssh_draft_port
            .trim()
            .parse::<u16>()
            .map_err(|_| "SSH port must be a number from 1 to 65535.".to_string())?;
        Ok(SSHProfileUpsertRequest {
            id: self.ssh_draft_id.clone(),
            name: self.ssh_draft_name.clone(),
            host: self.ssh_draft_host.clone(),
            port,
            username: self.ssh_draft_username.clone(),
            credential_kind: self.ssh_draft_credential_kind.clone(),
            private_key_path: Some(self.ssh_draft_private_key_path.clone()),
            password: Some(self.ssh_draft_password.clone()),
            key_passphrase: Some(self.ssh_draft_key_passphrase.clone()),
        })
    }

    pub(super) fn save_ssh_profile_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let request = match self.ssh_draft_request() {
            Ok(request) => request,
            Err(error) => {
                self.status_message = format!("failed to save SSH profile: {error}");
                self.invalidate_remote_panel(cx);
                return;
            }
        };
        let requested_id = request.id.clone();
        match self.runtime_service.upsert_ssh_profile(request) {
            Ok(snapshot) => {
                self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
                self.selected_ssh_profile_id = requested_id.or_else(|| {
                    snapshot
                        .profiles
                        .iter()
                        .max_by_key(|profile| profile.updated_at)
                        .map(|profile| profile.id.clone())
                });
                self.normalize_selected_ssh_profile();
                self.ssh_draft_open = false;
                self.status_message = "SSH profile saved".to_string();
                publish_ssh_update();
                publish_child_window_update(ChildWindowUpdateKind::Ssh);
                if self.window_mode == AppWindowMode::SshProfileEditor {
                    window.remove_window();
                }
            }
            Err(error) => self.status_message = format!("failed to save SSH profile: {error}"),
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn delete_selected_ssh_profile(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile_id) = self
            .ssh_draft_id
            .clone()
            .or_else(|| self.selected_ssh_profile_id.clone())
        else {
            self.status_message = "no SSH profile selected".to_string();
            self.invalidate_remote_panel(cx);
            return;
        };
        match self.runtime_service.delete_ssh_profile(profile_id) {
            Ok(_) => {
                self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
                self.normalize_selected_ssh_profile();
                self.ssh_draft_open = false;
                self.status_message = "SSH profile deleted".to_string();
            }
            Err(error) => self.status_message = format!("failed to delete SSH profile: {error}"),
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn test_ssh_profile_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.ssh_testing {
            self.status_message = "SSH test is already running".to_string();
            self.set_ssh_test_result(self.ssh_test_testing_message(), true);
            self.invalidate_remote_panel(cx);
            return;
        }
        let request = match self.ssh_draft_request() {
            Ok(request) => request,
            Err(error) => {
                self.set_ssh_test_result(error.clone(), false);
                self.status_message = format!("SSH test failed: {error}");
                self.invalidate_remote_panel(cx);
                return;
            }
        };
        let service = self.runtime_service.clone();
        let runtime_root = self.runtime.root.clone();
        self.ssh_testing = true;
        self.set_ssh_test_result(self.ssh_test_testing_message(), true);
        self.status_message = "SSH test started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.test_ssh_profile(request, runtime_root)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_ssh_test_result(result, cx);
            });
        })
        .detach();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn apply_ssh_test_result(
        &mut self,
        result: Result<codux_runtime::ssh::SSHProfileTestResult, String>,
        cx: &mut Context<Self>,
    ) {
        self.ssh_testing = false;
        match result {
            Ok(result) => {
                self.set_ssh_test_result(result.message.clone(), result.ok);
                self.status_message = result.message;
            }
            Err(error) => {
                self.set_ssh_test_result(error.clone(), false);
                self.status_message = format!("SSH test failed: {error}");
            }
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn toggle_remote_host(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.state.remote.enabled;
        match self.runtime_service.set_remote_enabled(next) {
            Ok(remote) => {
                let settings = self.runtime_service.reload_state().settings;
                self.apply_settings_summary(settings);
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = format!(
                    "remote host setting saved: {}",
                    if self.state.remote.enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save remote setting: {error}"),
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_remote_relay_url(
        &mut self,
        relay_url: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.settings.remote_relay_url.trim() == relay_url.trim() {
            return;
        }
        if self.remote_relay_change_requires_confirmation() {
            self.confirm_remote_relay_change(RemoteRelayChange::RelayUrl(relay_url), cx);
            return;
        }
        self.apply_remote_relay_url_change(relay_url, false, cx);
    }

    fn apply_remote_relay_url_change(
        &mut self,
        relay_url: String,
        reset_devices: bool,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_remote_relay_url_with_device_reset(&relay_url, reset_devices)
        {
            Ok((settings, remote)) => {
                self.apply_settings_summary(settings);
                self.state.remote = remote;
                self.remote_reconnecting = self.state.remote.status == "connecting";
                self.normalize_selected_remote_device();
                self.status_message = "remote relay setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save remote relay setting: {error}");
            }
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_remote_relay_authentication(
        &mut self,
        relay_authentication: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.settings.remote_relay_authentication.trim() == relay_authentication.trim() {
            return;
        }
        if self.remote_relay_change_requires_confirmation() {
            self.confirm_remote_relay_change(
                RemoteRelayChange::Authentication(relay_authentication),
                cx,
            );
            return;
        }
        self.apply_remote_relay_authentication_change(relay_authentication, false, cx);
    }

    fn apply_remote_relay_authentication_change(
        &mut self,
        relay_authentication: String,
        reset_devices: bool,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_remote_relay_authentication_with_device_reset(&relay_authentication, reset_devices)
        {
            Ok((settings, remote)) => {
                self.apply_settings_summary(settings);
                self.state.remote = remote;
                self.remote_reconnecting = self.state.remote.status == "connecting";
                self.normalize_selected_remote_device();
                self.status_message = "remote relay setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save remote relay setting: {error}");
            }
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_remote_relay_preset(
        &mut self,
        relay_preset: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.settings.remote_relay_preset == relay_preset {
            return;
        }
        if self.remote_relay_change_requires_confirmation() {
            self.confirm_remote_relay_change(RemoteRelayChange::Preset(relay_preset), cx);
            return;
        }
        self.apply_remote_relay_preset_change(relay_preset, false, cx);
    }

    fn apply_remote_relay_preset_change(
        &mut self,
        relay_preset: String,
        reset_devices: bool,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_remote_relay_preset_with_device_reset(&relay_preset, reset_devices)
        {
            Ok((settings, remote)) => {
                self.apply_settings_summary(settings);
                self.state.remote = remote;
                self.remote_reconnecting = self.state.remote.status == "connecting";
                self.normalize_selected_remote_device();
                self.status_message = "remote relay setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save remote relay setting: {error}");
            }
        }
        self.invalidate_remote_panel(cx);
    }

    fn remote_relay_change_requires_confirmation(&self) -> bool {
        self.state.remote.devices > 0 || self.state.remote.pending_pairings > 0
    }

    fn confirm_remote_relay_change(&mut self, change: RemoteRelayChange, cx: &mut Context<Self>) {
        let service = self.runtime_service.clone();
        let title = self.text("settings.remote.relay_change.title", "Change Relay Network");
        let message = self.text(
            "settings.remote.relay_change.message",
            "Changing the relay will clear paired devices. Pair mobile devices again after the change.",
        );
        let confirm_label = self.text("common.confirm", "Confirm");
        let cancel_label = self.text("common.cancel", "Cancel");
        self.status_message = "waiting for remote relay change confirmation".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_confirm_dialog(LocalizedConfirmDialogRequest {
                    title,
                    message,
                    confirm_label,
                    cancel_label,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| match result {
                Ok(true) => match change {
                    RemoteRelayChange::Preset(relay_preset) => {
                        app.apply_remote_relay_preset_change(relay_preset, true, cx)
                    }
                    RemoteRelayChange::RelayUrl(relay_url) => {
                        app.apply_remote_relay_url_change(relay_url, true, cx)
                    }
                    RemoteRelayChange::Authentication(relay_authentication) => {
                        app.apply_remote_relay_authentication_change(relay_authentication, true, cx)
                    }
                },
                Ok(false) => {
                    app.status_message = "remote relay change canceled".to_string();
                    app.invalidate_remote_panel(cx);
                }
                Err(error) => {
                    app.status_message =
                        format!("failed to show remote relay confirmation: {error}");
                    app.invalidate_remote_panel(cx);
                }
            });
        })
        .detach();
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn reconnect_remote(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.remote_reconnecting {
            return;
        }

        let service = self.runtime_service.clone();
        self.remote_reconnecting = true;
        self.status_message = "remote reconnect requested".to_string();
        self.runtime_trace("remote", "reconnect start");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result =
                codux_runtime::async_runtime::spawn_blocking(move || service.reconnect_remote())
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result);
            let _ = this.update(cx, |app, cx| {
                app.apply_remote_reconnect_result(result, cx);
            });
        })
        .detach();
        self.invalidate_remote_panel(cx);
    }

    fn apply_remote_reconnect_result(
        &mut self,
        result: Result<RemoteSummary, String>,
        cx: &mut Context<Self>,
    ) {
        self.remote_reconnecting = false;
        match result {
            Ok(remote) => {
                self.state.remote = remote;
                self.remote_reconnecting = self.state.remote.status == "connecting";
                self.normalize_selected_remote_device();
                self.status_message = "remote reconnect requested".to_string();
                self.runtime_trace("remote", "reconnect ok");
            }
            Err(error) => {
                self.status_message = format!("failed to reconnect remote: {error}");
                self.runtime_trace("remote", &format!("reconnect failed error={error}"));
            }
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn refresh_remote_devices(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.refresh_remote_devices() {
            Ok(remote) => {
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = "remote devices refreshed".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to refresh remote devices: {error}")
            }
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn create_remote_pairing(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.remote_pairing_creating {
            return;
        }

        let service = self.runtime_service.clone();
        self.remote_pairing_sheet_open = true;
        self.remote_pairing_creating = true;
        self.remote_pairing_error = None;
        self.status_message = "remote pairing request started".to_string();
        self.runtime_trace("remote", "pairing_create start");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn(async move {
                service.create_remote_pairing_async().await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            let _ = this.update(cx, |app, cx| {
                app.apply_remote_pairing_create_result(result, cx);
            });
        })
        .detach();
        self.invalidate_remote_panel(cx);
    }

    fn apply_remote_pairing_create_result(
        &mut self,
        result: Result<RemoteSummary, String>,
        cx: &mut Context<Self>,
    ) {
        self.remote_pairing_creating = false;
        match result {
            Ok(remote) => {
                let code = remote
                    .pairing
                    .as_ref()
                    .map(|pairing| pairing.code.clone())
                    .unwrap_or_default();
                let pairing = remote.pairing.clone();
                self.state.remote = remote;
                self.remote_pairing_error = None;
                self.remote_pairing_sheet_open = self.state.remote.pairing.is_some();
                self.normalize_selected_remote_device();
                self.status_message = if code.is_empty() {
                    "remote pairing created".to_string()
                } else {
                    format!("remote pairing code: {code}")
                };
                if let Some(pairing) = pairing {
                    self.start_remote_pairing_poll(pairing, cx);
                }
                self.runtime_trace("remote", "pairing_create ok");
            }
            Err(error) => {
                self.remote_pairing_sheet_open = true;
                self.remote_pairing_error = Some(error.clone());
                self.runtime_trace("remote", &format!("pairing_create failed error={error}"));
                self.status_message = format!("failed to create remote pairing: {error}");
            }
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn start_remote_pairing_poll(
        &mut self,
        pairing: RemotePairingInfo,
        cx: &mut Context<Self>,
    ) {
        self.remote_pairing_poll_generation = self.remote_pairing_poll_generation.wrapping_add(1);
        let generation = self.remote_pairing_poll_generation;
        let service = self.runtime_service.clone();

        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_secs(1)).await;

                let should_poll = this
                    .update(cx, |app, _| {
                        app.remote_pairing_poll_generation == generation
                            && app
                                .state
                                .remote
                                .pairing
                                .as_ref()
                                .map(|current| current.pairing_id == pairing.pairing_id)
                                .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !should_poll {
                    return;
                }

                let worker_pairing = pairing.clone();
                let worker_service = service.clone();
                let result = codux_runtime::async_runtime::spawn_blocking(move || {
                    worker_service.poll_remote_pairing_status(&worker_pairing)
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);

                let finished = this
                    .update(cx, |app, cx| {
                        app.apply_remote_pairing_poll_result(generation, &pairing, result, cx)
                    })
                    .unwrap_or(true);
                if finished {
                    return;
                }
            }
        })
        .detach();
    }

    pub(super) fn close_remote_pairing_sheet(&mut self, cx: &mut Context<Self>) {
        self.remote_pairing_sheet_open = false;
        self.remote_pairing_creating = false;
        self.remote_pairing_error = None;
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn apply_remote_pairing_poll_result(
        &mut self,
        generation: u64,
        pairing: &RemotePairingInfo,
        result: Result<RemotePairingPollResult, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.remote_pairing_poll_generation != generation
            || self
                .state
                .remote
                .pairing
                .as_ref()
                .map(|current| current.pairing_id.as_str())
                != Some(pairing.pairing_id.as_str())
        {
            return true;
        }

        match result {
            Ok(result) => {
                let finished = result.finished;
                self.state.remote = result.summary;
                self.remote_pairing_error = None;
                self.remote_pairing_sheet_open = self.state.remote.pairing.is_some();
                self.normalize_selected_remote_device();
                self.status_message = self.state.remote.message.clone();
                self.invalidate_remote_panel(cx);
                finished
            }
            Err(error) => {
                self.state.remote.pairing = None;
                self.remote_pairing_sheet_open = false;
                self.remote_pairing_error = Some(error.clone());
                self.status_message = format!("remote pairing poll failed: {error}");
                self.invalidate_remote_panel(cx);
                true
            }
        }
    }

    pub(super) fn cancel_remote_pairing(
        &mut self,
        pairing_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_pairing_poll_generation = self.remote_pairing_poll_generation.wrapping_add(1);
        self.remote_pairing_sheet_open = false;
        self.remote_pairing_error = None;
        self.state.remote.pairing = None;
        self.status_message = "remote pairing cancelled".to_string();

        let service = self.runtime_service.clone();
        self.runtime_trace("remote", "pairing_cancel start");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.cancel_remote_pairing(&pairing_id)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            let _ = this.update(cx, |app, cx| {
                app.apply_remote_pairing_cancel_result(result, cx);
            });
        })
        .detach();
        self.invalidate_remote_panel(cx);
    }

    /// Copy the `codux://pair` ticket to the clipboard so another desktop
    /// controller (which can't scan the QR) can paste it to connect to this host.
    pub(super) fn copy_remote_pairing_link(&mut self, payload: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem {
            entries: vec![gpui::ClipboardEntry::String(gpui::ClipboardString::new(
                payload,
            ))],
        });
        self.status_message = "pairing link copied".to_string();
        self.invalidate_remote_panel(cx);
    }

    /// Forget a host this desktop paired to (controller side): drop the saved
    /// host + any live connection. Removes it from the unified device list and
    /// from the add-project device picker.
    pub(super) fn forget_remote_host_device(&mut self, device_id: String, cx: &mut Context<Self>) {
        match self.runtime_service.forget_remote_host(&device_id) {
            Ok(_) => self.status_message = "forgot remote host".to_string(),
            Err(error) => self.status_message = format!("forget host failed: {error}"),
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn open_remote_connect(&mut self, cx: &mut Context<Self>) {
        self.remote_connect_open = true;
        self.remote_connect_ticket = String::new();
        self.remote_connect_name = codux_runtime::remote::remote_host_name();
        self.remote_connect_error = None;
        self.remote_connect_busy = false;
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn close_remote_connect(&mut self, cx: &mut Context<Self>) {
        self.remote_connect_open = false;
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_remote_connect_ticket(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_connect_ticket = value;
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn set_remote_connect_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_connect_name = value;
        self.invalidate_remote_panel(cx);
    }

    /// Pair this desktop (as controller) to another host by its `codux://pair`
    /// ticket. The host then appears in the unified device list and is selectable
    /// when adding a project.
    pub(super) fn submit_remote_connect(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let ticket = self.remote_connect_ticket.trim().to_string();
        if ticket.is_empty() {
            self.remote_connect_error =
                Some(self.text("remote.connect.ticket_required", "Paste a pairing ticket."));
            self.invalidate_remote_panel(cx);
            return;
        }
        let device_name = {
            let name = self.remote_connect_name.trim();
            if name.is_empty() {
                codux_runtime::remote::remote_host_name()
            } else {
                name.to_string()
            }
        };
        let runtime_service = self.runtime_service.clone();
        self.remote_connect_busy = true;
        self.remote_connect_error = None;
        self.invalidate_remote_panel(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                runtime_service.pair_remote_host(&ticket, &device_name)
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join pairing: {error}")));
            let _ = this.update(cx, |app, cx| {
                app.remote_connect_busy = false;
                match result {
                    Ok(saved) => {
                        app.status_message = format!(
                            "paired with {}",
                            if saved.host_name.is_empty() {
                                saved.host_id.clone()
                            } else {
                                saved.host_name.clone()
                            }
                        );
                        app.remote_connect_open = false;
                        app.remote_connect_ticket = String::new();
                        app.remote_connect_name = String::new();
                        app.remote_connect_error = None;
                    }
                    Err(error) => {
                        app.remote_connect_error = Some(error);
                    }
                }
                app.invalidate_remote_panel(cx);
            });
        })
        .detach();
    }

    fn apply_remote_pairing_cancel_result(
        &mut self,
        result: Result<RemoteSummary, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(remote) => {
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = "remote pairing cancelled".to_string();
                self.runtime_trace("remote", "pairing_cancel ok");
            }
            Err(error) => {
                self.status_message = format!("failed to cancel remote pairing: {error}");
                self.runtime_trace("remote", &format!("pairing_cancel failed error={error}"));
            }
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn confirm_remote_pairing(
        &mut self,
        pairing_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_pairing_poll_generation = self.remote_pairing_poll_generation.wrapping_add(1);
        match self.runtime_service.confirm_remote_pairing(&pairing_id) {
            Ok(remote) => {
                self.state.remote = remote;
                self.finish_remote_pairing_decision(&pairing_id);
                self.normalize_selected_remote_device();
                self.status_message = "remote pairing confirmed".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to confirm remote pairing: {error}");
            }
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn reject_remote_pairing(
        &mut self,
        pairing_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_pairing_poll_generation = self.remote_pairing_poll_generation.wrapping_add(1);
        match self.runtime_service.reject_remote_pairing(&pairing_id) {
            Ok(remote) => {
                self.state.remote = remote;
                self.finish_remote_pairing_decision(&pairing_id);
                self.normalize_selected_remote_device();
                self.status_message = "remote pairing rejected".to_string();
            }
            Err(error) => self.status_message = format!("failed to reject remote pairing: {error}"),
        }
        self.invalidate_remote_panel(cx);
    }

    fn finish_remote_pairing_decision(&mut self, pairing_id: &str) {
        self.state.remote.pairing = None;
        self.state
            .remote
            .pending_pairing_list
            .retain(|pairing| pairing.id != pairing_id);
        self.state.remote.pending_pairings = self.state.remote.pending_pairing_list.len();
        self.remote_pairing_sheet_open = false;
        self.remote_pairing_creating = false;
        self.remote_pairing_error = None;
    }

    pub(super) fn selected_remote_device(&self) -> Option<&RemoteDeviceSummary> {
        self.selected_remote_device_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .remote
                    .device_list
                    .iter()
                    .find(|device| device.id == id)
            })
            .or_else(|| self.state.remote.device_list.first())
    }

    pub(super) fn normalize_selected_remote_device(&mut self) {
        let selected_still_exists = self
            .selected_remote_device_id
            .as_deref()
            .map(|id| {
                self.state
                    .remote
                    .device_list
                    .iter()
                    .any(|device| device.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_remote_device_id = self
                .state
                .remote
                .device_list
                .first()
                .map(|device| device.id.clone());
        }
    }

    pub(super) fn select_remote_device(
        &mut self,
        device_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(device) = self
            .state
            .remote
            .device_list
            .iter()
            .find(|device| device.id == device_id)
        else {
            self.status_message = "remote device is no longer available".to_string();
            self.normalize_selected_remote_device();
            self.invalidate_remote_panel(cx);
            return;
        };
        self.selected_remote_device_id = Some(device.id.clone());
        self.status_message = format!("selected remote device: {}", empty_label(&device.name));
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn revoke_selected_remote_device(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(device) = self.selected_remote_device().cloned() else {
            self.status_message = "no remote device selected".to_string();
            self.invalidate_remote_panel(cx);
            return;
        };
        match self.runtime_service.revoke_remote_device(&device.id) {
            Ok(remote) => {
                self.state.remote = remote;
                self.state.settings = self.runtime_service.reload_state().settings;
                self.selected_remote_device_id = None;
                self.normalize_selected_remote_device();
                self.status_message =
                    format!("remote device revoked: {}", empty_label(&device.name));
            }
            Err(error) => self.status_message = format!("failed to revoke remote device: {error}"),
        }
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn apply_runtime_activity_tick(
        &mut self,
        visible: bool,
        focused: bool,
        include_scheduled_tick: bool,
        cx: &mut Context<Self>,
    ) -> RuntimeActivityTickResult {
        let dock_badge_count = if include_scheduled_tick {
            let window_state = self.runtime_service.app_window_state(visible, focused);
            let _ = self
                .runtime_service
                .set_dock_badge_count(window_state.dock_badge_count);
            window_state.dock_badge_count
        } else {
            let _ = self
                .runtime_service
                .mark_main_window_state(visible, focused);
            None
        };
        if include_scheduled_tick {
            self.runtime_service.tick_project_activity();
        }
        let applied_settings_events = usize::from(self.apply_settings_update_event(cx));
        let child_window_events =
            self.apply_child_window_update_event(current_child_window_update_event(), cx);
        let project_events = self.runtime_service.drain_project_activity_events();
        let applied_project_events = self.apply_project_activity_events(project_events, cx);
        let applied_pet_events =
            usize::from(self.sync_pet_custom_install_event_for_activity_tick());
        let applied_pet_update_events = usize::from(self.sync_pet_update_event_for_activity_tick());
        let ai_history_result = self.runtime_service.drain_ai_history_events();
        let applied_runtime_pet_update =
            self.apply_runtime_pet_refresh_result(&ai_history_result, cx);
        let applied_ai_history_events = self.apply_ai_history_events(ai_history_result.events, cx);
        let file_events = self.runtime_service.drain_file_change_events();
        let applied_file_events = self.apply_file_change_events(file_events, cx);
        let remote_events = self.runtime_service.drain_remote_events();
        let mut remote_summary_events = 0;
        let mut remote_terminal_layout_events = 0;
        for event in &remote_events {
            match event {
                RemoteHostEvent::Summary(remote) => {
                    remote_summary_events += 1;
                    self.state.remote = remote.clone();
                    if self.remote_reconnecting && self.state.remote.status != "connecting" {
                        self.remote_reconnecting = false;
                    }
                    self.normalize_selected_remote_device();
                }
                RemoteHostEvent::TerminalLayoutChanged(_) => {
                    remote_terminal_layout_events += 1;
                }
            }
        }
        if remote_terminal_layout_events > 0 {
            self.reconcile_remote_terminal_layout(cx);
        }
        let scheduled_refresh = self.pending_runtime_refresh.take();
        let has_scheduled_refresh = scheduled_refresh.is_some();
        if let Some(refresh) = scheduled_refresh {
            self.state.runtime_activity = refresh.runtime_activity;
            self.state.remote = refresh.remote;
            self.normalize_selected_remote_device();
        }
        let drained = self
            .runtime_service
            .drain_ai_runtime_events_and_enqueue_memory();
        self.dispatch_ai_completion_notifications(&drained.events);
        self.refresh_dock_badge_for_ai_runtime_events(&drained.events, cx);
        self.ai_runtime_state_save_tick = self.ai_runtime_state_save_tick.wrapping_add(1);
        // Drained events are the authoritative "something changed" signal (the
        // supervisor only emits State on a real mutation, and a brand-new
        // session always arrives with one). So on a fully idle app — no events
        // and nothing currently tracked — skip the scheduled/periodic rebuild
        // entirely instead of re-summarizing an empty snapshot every 30s.
        let should_refresh_ai_state = !drained.events.is_empty()
            || ((include_scheduled_tick || self.ai_runtime_state_save_tick % 30 == 0)
                && !self.state.ai_runtime_state.sessions.is_empty());
        let mut ai_activity_changed = false;
        let mut pane_lifecycle_changed = false;
        if should_refresh_ai_state {
            let previous_project_states = self.state.ai_runtime_state.project_states.clone();
            let live_ai_snapshot = self.runtime_service.ai_runtime_state_snapshot();
            self.state.ai_runtime_state = self
                .runtime_service
                .summarize_ai_runtime_state_snapshot(&live_ai_snapshot);
            pane_lifecycle_changed = self.sync_pane_agent_lifecycle();
            self.state.refresh_ai_history_stats();
            ai_activity_changed = super::ai_runtime_status::ai_activity_project_states_changed(
                &previous_project_states,
                &self.state.ai_runtime_state.project_states,
            );
            if ai_activity_changed {
                self.runtime_service.push_remote_ai_stats_to_watchers();
            }
            let previous_level = self.state.pet.level;
            self.state.pet = self.runtime_service.reload_pet();
            self.note_pet_level_transition(previous_level);
            if let Ok(snapshot) = self.runtime_service.pet_snapshot() {
                self.pet_snapshot = snapshot;
            }
        } else if !self.pane_agent_lifecycle.is_empty() {
            pane_lifecycle_changed = self.sync_pane_agent_lifecycle();
        }
        self.maybe_refresh_git_for_agent_activity(pane_lifecycle_changed, cx);
        if include_scheduled_tick {
            self.refresh_global_today_ai_tokens();
        }
        let remote_ai_stats_changed = self.apply_pushed_remote_ai_stats();
        let memory_event = current_memory_update_event();
        let memory_update_event = memory_event.revision > self.memory_seen_revision;
        if memory_update_event {
            self.memory_seen_revision = memory_event.revision;
        }
        if !drained.memory.is_empty() || memory_update_event {
            self.state.memory = self.runtime_service.reload_memory(
                self.state
                    .selected_project
                    .as_ref()
                    .map(|project| project.id.as_str()),
            );
            self.reload_memory_manager_snapshot();
            self.invalidate_status_bar(cx);
            if self.state.memory_manager.extraction.queued > 0
                || self.state.memory_manager.extraction.running > 0
            {
                self.process_queued_memory_extraction_async(cx);
            }
        }
        let changed = applied_project_events > 0
            || applied_file_events > 0
            || applied_ai_history_events > 0
            || applied_pet_events > 0
            || applied_pet_update_events > 0
            || applied_runtime_pet_update
            || applied_settings_events > 0
            || child_window_events > 0
            || !remote_events.is_empty()
            || ai_activity_changed
            || pane_lifecycle_changed
            || remote_ai_stats_changed
            || !drained.memory.is_empty()
            || memory_update_event
            || has_scheduled_refresh;
        if changed {
            if ai_activity_changed {
                self.sync_project_activity_state(cx);
                self.invalidate_task_column(cx);
            } else if pane_lifecycle_changed {
                self.invalidate_task_column(cx);
            }
            self.runtime_trace(
                "runtime-activity",
                &format!(
                    "tick scheduled={} settings={} child_windows={} project={} files={} pet_catalog={} pet_updates={} runtime_pet={} ai_history={} ai_events={} memory={} remote={} remote_layout={} scheduled_refresh={} ai_state_error={}",
                    include_scheduled_tick,
                    applied_settings_events,
                    child_window_events,
                    applied_project_events,
                    applied_file_events,
                    applied_pet_events,
                    applied_pet_update_events,
                    applied_runtime_pet_update,
                    applied_ai_history_events,
                    drained.events.len(),
                    drained.memory.len(),
                    remote_summary_events,
                    remote_terminal_layout_events,
                    has_scheduled_refresh,
                    "none"
                ),
            );
        }
        RuntimeActivityTickResult {
            project_events: applied_project_events,
            file_events: applied_file_events,
            ai_history_events: applied_ai_history_events,
            pet_events: applied_pet_events,
            pet_update_events: applied_pet_update_events + usize::from(applied_runtime_pet_update),
            ai_runtime_events: drained.events.len(),
            ai_activity_changed,
            memory_events: drained.memory.len(),
            dock_badge_count,
            changed,
            ai_state_error: None,
        }
    }

    /// Apply any `ai.stats` the host pushed for the selected remote-hosted
    /// project since the last tick (live AI runtime updates). Returns whether the
    /// view changed. No-op for a local project.
    fn apply_pushed_remote_ai_stats(&mut self) -> bool {
        let Some(project) = self
            .state
            .selected_project
            .as_ref()
            .filter(|project| project.host_device_id.is_some())
            .cloned()
        else {
            return false;
        };
        let include_cached = self.state.settings.statistics_mode.trim() == "includingCache";
        let Some(views) = self
            .runtime_service
            .drain_remote_ai_current_sessions(&project.path, include_cached)
        else {
            return false;
        };
        self.state.remote_ai_current_sessions = views;
        self.state.refresh_ai_history_stats();
        true
    }

    fn maybe_refresh_git_for_agent_activity(
        &mut self,
        pane_lifecycle_changed: bool,
        cx: &mut Context<Self>,
    ) {
        let completed_now = pane_lifecycle_changed
            && self
                .pane_agent_lifecycle
                .values()
                .any(|lifecycle| lifecycle.state == AgentLifecycleState::Completed);
        let working_now = self
            .pane_agent_lifecycle
            .values()
            .any(|lifecycle| lifecycle.state == AgentLifecycleState::Working);
        if completed_now {
            self.refresh_git_panel_state_async_quiet(cx);
            self.agent_git_refresh_after =
                Some(Instant::now() + Duration::from_secs(5));
            return;
        }
        if working_now
            && self
                .agent_git_refresh_after
                .is_none_or(|deadline| Instant::now() >= deadline)
        {
            self.refresh_git_panel_state_async_quiet(cx);
            self.agent_git_refresh_after =
                Some(Instant::now() + Duration::from_secs(5));
        } else if !working_now {
            self.agent_git_refresh_after = None;
        }
    }

    pub(super) fn apply_ai_runtime_activity_tick(
        &mut self,
        cx: &mut Context<Self>,
    ) -> RuntimeActivityTickResult {
        let drained = self
            .runtime_service
            .drain_ai_runtime_events_and_enqueue_memory();
        let memory_event = current_memory_update_event();
        let memory_update_event = memory_event.revision > self.memory_seen_revision;
        if memory_update_event {
            self.memory_seen_revision = memory_event.revision;
        }
        if drained.events.is_empty() && drained.memory.is_empty() && !memory_update_event {
            if !self.pane_agent_lifecycle.is_empty() {
                let pane_lifecycle_changed = self.sync_pane_agent_lifecycle();
                if pane_lifecycle_changed {
                    self.invalidate_task_column(cx);
                    return RuntimeActivityTickResult {
                        changed: true,
                        ..RuntimeActivityTickResult::default()
                    };
                }
            }
            return RuntimeActivityTickResult::default();
        }

        self.dispatch_ai_completion_notifications(&drained.events);
        self.refresh_dock_badge_for_ai_runtime_events(&drained.events, cx);
        let previous_project_states = self.state.ai_runtime_state.project_states.clone();
        let live_ai_snapshot = self.runtime_service.ai_runtime_state_snapshot();
        self.state.ai_runtime_state = self
            .runtime_service
            .summarize_ai_runtime_state_snapshot(&live_ai_snapshot);
        let pane_lifecycle_changed = self.sync_pane_agent_lifecycle();
        self.state.refresh_ai_history_stats();
        let ai_activity_changed = super::ai_runtime_status::ai_activity_project_states_changed(
            &previous_project_states,
            &self.state.ai_runtime_state.project_states,
        );
        if ai_activity_changed {
            self.runtime_service.push_remote_ai_stats_to_watchers();
        }
        let previous_level = self.state.pet.level;
        self.state.pet = self.runtime_service.reload_pet();
        self.note_pet_level_transition(previous_level);
        if let Ok(snapshot) = self.runtime_service.pet_snapshot() {
            self.pet_snapshot = snapshot;
        }

        if !drained.memory.is_empty() || memory_update_event {
            self.state.memory = self.runtime_service.reload_memory(
                self.state
                    .selected_project
                    .as_ref()
                    .map(|project| project.id.as_str()),
            );
            self.reload_memory_manager_snapshot();
            self.invalidate_status_bar(cx);
            if self.state.memory_manager.extraction.queued > 0
                || self.state.memory_manager.extraction.running > 0
            {
                self.process_queued_memory_extraction_async(cx);
            }
        }

        if ai_activity_changed {
            self.sync_project_activity_state(cx);
            self.invalidate_task_column(cx);
        } else if pane_lifecycle_changed {
            self.invalidate_task_column(cx);
        }
        let memory_events = drained.memory.len() + usize::from(memory_update_event);
        let changed = ai_activity_changed || memory_events > 0 || pane_lifecycle_changed;
        if changed {
            self.runtime_trace(
                "runtime-activity",
                &format!(
                    "ai_fast_tick ai_events={} memory={} ai_state_error={}",
                    drained.events.len(),
                    memory_events,
                    "none"
                ),
            );
        }

        RuntimeActivityTickResult {
            ai_runtime_events: drained.events.len(),
            ai_activity_changed,
            memory_events,
            changed,
            ai_state_error: None,
            ..RuntimeActivityTickResult::default()
        }
    }

    pub(super) fn dispatch_ai_completion_notifications(
        &self,
        events: &[codux_runtime::ai_runtime::AIRuntimeSupervisorEvent],
    ) {
        let completions = events
            .iter()
            .filter_map(|event| match event {
                codux_runtime::ai_runtime::AIRuntimeSupervisorEvent::Completion { completion } => {
                    Some(completion.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if completions.is_empty() {
            return;
        }

        let channels = self
            .state
            .notifications
            .channels
            .iter()
            .filter(|channel| channel.enabled && !channel.endpoint.trim().is_empty())
            .map(|channel| NotificationChannelConfig {
                id: channel.id.clone(),
                endpoint: channel.endpoint.clone(),
                token: channel.token.clone(),
            })
            .collect::<Vec<_>>();
        let locale = locale_from_language_setting(&self.state.settings.language);

        for completion in completions {
            let service = self.runtime_service.clone();
            let channels = channels.clone();
            let locale = locale.clone();
            codux_runtime::async_runtime::spawn_blocking(move || {
                let title = if completion.was_interrupted {
                    translate(
                        &locale,
                        "ai.notification.task_interrupted",
                        "Task interrupted",
                    )
                } else {
                    translate(&locale, "ai.notification.task_completed", "Task completed")
                };
                let project_name = completion.project_name.trim();
                let native_title = if project_name.is_empty() {
                    "Codux".to_string()
                } else {
                    project_name.to_string()
                };
                let body = completion.project_name.clone();
                let group = "codux-task";
                if let Err(error) = service.show_native_notification(&native_title, &title, group) {
                    service.runtime_trace_frontend(
                        "notification",
                        &format!("native notification failed error={error}"),
                    );
                }
                if !channels.is_empty() {
                    service.dispatch_notification_channels(NotificationDispatchRequest {
                        channels,
                        title,
                        body,
                        group: group.to_string(),
                    });
                }
            });
        }
    }

    fn refresh_dock_badge_for_ai_runtime_events(
        &mut self,
        events: &[codux_runtime::ai_runtime::AIRuntimeSupervisorEvent],
        cx: &mut Context<Self>,
    ) {
        if !events.iter().any(|event| {
            matches!(
                event,
                codux_runtime::ai_runtime::AIRuntimeSupervisorEvent::Completion { .. }
            )
        }) {
            return;
        }
        self.refresh_dock_badge_now(cx);
    }

    pub(super) fn refresh_global_today_ai_tokens(&mut self) -> bool {
        let Ok(tokens) = self.runtime_service.global_today_normalized_ai_tokens() else {
            return false;
        };
        let tokens = tokens.max(0);
        if self.state.ai_global_history.today_total_tokens == tokens {
            return false;
        }
        self.state.ai_global_history.today_total_tokens = tokens;
        self.state.refresh_daily_level();
        true
    }

    pub(super) fn refresh_today_level_after_day_change(&mut self) -> bool {
        let day_start =
            codux_runtime::ai_history_normalized::local_day_start_seconds(app_now_seconds());
        if (day_start - self.today_level_day_start).abs() < 1.0 {
            return false;
        }
        self.today_level_day_start = day_start;
        self.refresh_global_today_ai_tokens();
        true
    }

    fn apply_runtime_pet_refresh_result(
        &mut self,
        result: &codux_runtime::runtime_state::AIHistoryDrainResult,
        _cx: &mut Context<Self>,
    ) -> bool {
        if let Some(error) = result.pet_error.as_deref() {
            self.runtime_trace(
                "pet",
                &format!("indexed_history_refresh failed error={error}"),
            );
        }
        let Some(pet) = result.pet.clone() else {
            return false;
        };
        let previous_level = self.state.pet.level;
        self.state.pet = pet;
        self.note_pet_level_transition(previous_level);
        if let Some(snapshot) = result.pet_snapshot.clone() {
            self.pet_snapshot = snapshot;
        }
        let revision = publish_pet_update();
        if revision > 0 {
            self.pet_update_seen_revision = revision;
        }
        true
    }

    pub(super) fn apply_ai_history_events(
        &mut self,
        events: Vec<AIHistoryEvent>,
        cx: &mut Context<Self>,
    ) -> usize {
        if events.is_empty() {
            return 0;
        }
        let selected_project = self.state.selected_project.clone();
        let selected_id = selected_project.as_ref().map(|project| project.id.as_str());
        let selected_path = selected_project
            .as_ref()
            .map(|project| project.path.as_str());
        let selected_worktree = super::ai_runtime_status::selected_worktree_info(&self.state);
        let selected_history_id = selected_worktree
            .as_ref()
            .map(|worktree| worktree.id.as_str())
            .or(selected_id);
        let selected_history_path = selected_worktree
            .as_ref()
            .map(|worktree| worktree.path.as_str())
            .or(selected_path);
        let selected_worktree_key = current_worktree_scope_key(&self.state);
        let mut applied = 0;
        let previous_active_index_count = self.ai_history_active_index_count;

        for event in events {
            // Push freshly-indexed stats to any remote device that requested
            // `ai.stats` while this project's index was cold. Independent of the
            // desktop's own selection (a remote may view a different project).
            if let AIHistoryEvent::ProjectState { state } = &event {
                self.runtime_service.flush_remote_ai_stats(state);
            }
            match event {
                AIHistoryEvent::ProjectState { state }
                    if selected_history_id == Some(state.project_id.as_str())
                        || selected_history_path == Some(state.project_path.as_str()) =>
                {
                    let is_loading = state.is_loading || state.queued;
                    let summary =
                        ai_history_summary_from_state_or_status(&self.state.ai_history, &state);
                    if let Some(key) = selected_worktree_key.clone() {
                        self.merge_worktree_ai_history_if_current(key, summary.clone());
                    }
                    if ai_history_should_replace(&self.state.ai_history, &summary) {
                        self.state.ai_history = summary;
                        self.state.refresh_ai_history_stats();
                    }
                    if !is_loading {
                        self.ai_history_refreshing = false;
                    }
                    if is_loading {
                        self.ai_index_progress_visible_until = self
                            .ai_index_progress_visible_until
                            .max(app_now_seconds() + 3.0);
                        self.ai_index_progress_generation =
                            self.ai_index_progress_generation.wrapping_add(1);
                        self.schedule_ai_index_progress_expiry(
                            self.ai_index_progress_generation,
                            cx,
                        );
                    }
                    self.normalize_selected_ai_session();
                    applied += 1;
                }
                AIHistoryEvent::Project { snapshot }
                    if selected_history_id == Some(snapshot.project_id.as_str()) =>
                {
                    let summary = normalized_ai_history_snapshot_to_summary(snapshot);
                    if let Some(key) = selected_worktree_key.clone() {
                        self.merge_worktree_ai_history_if_current(key, summary.clone());
                    }
                    if ai_history_should_replace(&self.state.ai_history, &summary) {
                        self.state.ai_history = summary;
                        self.state.refresh_ai_history_stats();
                        self.normalize_selected_ai_session();
                        self.ai_history_refreshing = false;
                        applied += 1;
                    }
                }
                AIHistoryEvent::Global { snapshot } => {
                    self.state.ai_global_history =
                        normalized_global_ai_history_snapshot_to_summary(snapshot);
                    applied += 1;
                }
                AIHistoryEvent::Status { project_id, .. }
                    if project_id.as_deref().is_none() || project_id.as_deref() == selected_id =>
                {
                    applied += 1;
                }
                _ => {}
            }
        }

        self.ai_history_active_index_count = self.runtime_service.active_ai_history_index_count();
        let global_changed = self.ai_history_active_index_count != previous_active_index_count;

        if applied > 0 {
            self.invalidate_task_column(cx);
        }

        if global_changed {
            applied.max(1)
        } else {
            applied
        }
    }

    pub(super) fn apply_project_activity_events(
        &mut self,
        events: Vec<ProjectActivityEvent>,
        cx: &mut Context<Self>,
    ) -> usize {
        let selected_project = self.state.selected_project.clone();
        let selected_path = selected_project
            .as_ref()
            .map(|project| project.path.as_str());
        let selected_id = selected_project.as_ref().map(|project| project.id.as_str());
        let selected_worktree = super::ai_runtime_status::selected_worktree_info(&self.state);
        let selected_git_path = selected_worktree
            .as_ref()
            .map(|worktree| worktree.path.as_str())
            .or(selected_path);
        let selected_history_id = selected_worktree
            .as_ref()
            .map(|worktree| worktree.id.as_str())
            .or(selected_id);
        let selected_history_path = selected_worktree
            .as_ref()
            .map(|worktree| worktree.path.as_str())
            .or(selected_path);
        let selected_worktree_key = current_worktree_scope_key(&self.state);
        let mut applied = 0;

        for event in events {
            match event {
                ProjectActivityEvent::GitStatus {
                    project_path,
                    snapshot,
                    ..
                } if selected_git_path == Some(project_path.as_str())
                    || selected_path == Some(project_path.as_str()) =>
                {
                    self.state.git = snapshot;
                    self.sync_current_worktree_git_summary_from_current_git();
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    applied += 1;
                }
                ProjectActivityEvent::GitReview {
                    project_path,
                    snapshot,
                    ..
                } if selected_git_path == Some(project_path.as_str())
                    || selected_path == Some(project_path.as_str()) =>
                {
                    self.git_review = snapshot;
                    self.sync_current_worktree_git_summary_from_current_git();
                    self.normalize_selected_git_file();
                    applied += 1;
                }
                ProjectActivityEvent::WorktreeSnapshot {
                    project_id,
                    project_path,
                    snapshot,
                } if selected_id == Some(project_id.as_str())
                    || selected_path == Some(project_path.as_str()) =>
                {
                    self.merge_selected_project_worktrees(snapshot);
                    applied += 1;
                }
                ProjectActivityEvent::GitChanged { project_path, .. }
                    if selected_path == Some(project_path.as_str()) =>
                {
                    self.refresh_file_tree_state();
                    applied += 1;
                }
                ProjectActivityEvent::AIHistory {
                    project_id,
                    project_path,
                    snapshot,
                    ..
                } if selected_history_id == Some(project_id.as_str())
                    || selected_history_path == Some(project_path.as_str()) =>
                {
                    let summary = normalized_ai_history_snapshot_to_summary(snapshot);
                    if let Some(key) = selected_worktree_key.clone() {
                        self.merge_worktree_ai_history_if_current(key, summary.clone());
                    }
                    if ai_history_should_replace(&self.state.ai_history, &summary) {
                        self.state.ai_history = summary;
                        self.state.refresh_ai_history_stats();
                        self.normalize_selected_ai_session();
                        applied += 1;
                    }
                }
                _ => {}
            }
        }

        if applied > 0 {
            self.invalidate_task_column(cx);
            self.invalidate_status_bar(cx);
        }

        applied
    }

    pub(super) fn apply_file_change_events(
        &mut self,
        events: Vec<FileChangeEvent>,
        cx: &mut Context<Self>,
    ) -> usize {
        let selected_path = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.path.clone());
        let applied = events
            .iter()
            .filter(|event| selected_path.as_deref() == Some(event.project_path.as_str()))
            .count();

        if applied > 0 {
            self.refresh_file_tree_state();
            self.normalize_selected_file_entry();
        }

        let reloaded_tabs = self.reload_clean_file_editor_tabs_for_file_events(&events, cx);

        applied + reloaded_tabs
    }

    pub(super) fn dismiss_worktree_ai_completion(
        &mut self,
        worktree_id: &str,
        cx: &mut Context<Self>,
    ) {
        if worktree_id.trim().is_empty() {
            return;
        }
        let mut changed = self
            .runtime_service
            .ai_runtime_dismiss_completion(worktree_id);
        if let Some(worktree) = self
            .state
            .worktrees
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
        {
            changed |= self
                .runtime_service
                .ai_runtime_dismiss_completion(&worktree.project_id);
            self.dismissed_worktree_ai_completion_at
                .insert(worktree.project_id.clone(), app_now_seconds());
        }
        if !changed {
            return;
        }
        self.dismissed_worktree_ai_completion_at
            .insert(worktree_id.to_string(), app_now_seconds());
        let snapshot = self.runtime_service.ai_runtime_state_snapshot();
        self.state.ai_runtime_state = self
            .runtime_service
            .summarize_ai_runtime_state_snapshot(&snapshot);
        let _ = self.sync_pane_agent_lifecycle();
        self.state.refresh_ai_history_stats();
        self.refresh_dock_badge_now(cx);
        self.sync_project_activity_state(cx);
        self.invalidate_task_column(cx);
        self.invalidate_remote_panel(cx);
    }

    fn refresh_dock_badge_now(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        let _ = self
            .runtime_service
            .set_dock_badge_count(self.runtime_service.ai_runtime_dock_badge_count());
        self.invalidate_status_bar(cx);
    }
}
