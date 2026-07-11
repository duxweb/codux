use super::ai_runtime_status::AgentLifecycleState;
use super::*;
use codux_runtime::remote::ControllerLinkState;

impl CoduxApp {
    pub(super) fn selected_project_id(&self) -> Option<String> {
        self.state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
    }

    pub(super) fn ensure_project_list_state(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<ProjectListState> {
        if let Some(state) = &self.project_list_state {
            return state.clone();
        }

        let lifecycle = self.project_lifecycle_snapshot();
        let state = cx.new(|_| {
            let mut state =
                ProjectListState::new(self.state.projects.clone(), self.selected_project_id());
            state.lifecycle = lifecycle;
            state
        });
        self.project_list_state = Some(state.clone());
        state
    }

    pub(super) fn sync_project_list_state(&mut self, cx: &mut Context<Self>) {
        let state = self.ensure_project_list_state(cx);
        let projects = self.state.projects.clone();
        let selected_project_id = self.selected_project_id();
        let lifecycle = self.project_lifecycle_snapshot();
        let links = self.remote_link_states.clone();
        state.update(cx, |state, cx| {
            state.set_snapshot(projects, selected_project_id, cx);
            state.set_lifecycle(lifecycle, cx);
            state.set_links(links, cx);
        });
        // Mirror the refreshed project list to connected controllers (pad/phone)
        // so a desktop-side create/rename/reorder/close shows up live instead of
        // only after reconnect or pull-to-refresh. Idempotent + cheap: a no-op
        // when nothing is connected/subscribed.
        self.runtime_service.broadcast_remote_project_list();
    }

    pub(super) fn sync_project_lifecycle_state(&mut self, cx: &mut Context<Self>) {
        let state = self.ensure_project_list_state(cx);
        let lifecycle = self.project_lifecycle_snapshot();
        state.update(cx, |state, cx| state.set_lifecycle(lifecycle, cx));
    }

    /// Pull the latest client→host link states from the runtime (a cheap cached
    /// read — the controller transport updates it event-driven). On change: push
    /// to the project badge, and when a host transitions back to Connected,
    /// re-attach that host's terminals so a dropped remote shell recovers.
    pub(super) fn refresh_remote_link_states(&mut self, cx: &mut Context<Self>) {
        let links = self.runtime_service.remote_controller_link_states();
        // Persistent outbound-host registry (disk-backed) for the status-bar
        // count. Read here on the slow tick — never from a render path — because
        // link states are transient runtime data that undercounts saved hosts
        // not yet reached (and reset to empty on every app restart).
        let saved_host_ids: Vec<String> = self
            .runtime_service
            .saved_remote_hosts()
            .into_iter()
            .map(|host| host.device_id)
            .collect();
        let links_changed = links != self.remote_link_states;
        let saved_changed = saved_host_ids != self.remote_saved_host_ids;
        if !links_changed && !saved_changed {
            return;
        }
        let reconnected: Vec<String> = if links_changed {
            links
                .iter()
                .filter(|(device_id, state)| {
                    **state == ControllerLinkState::Connected
                        && self
                            .remote_link_states
                            .get(device_id.as_str())
                            .copied()
                            // Absent previous = a host we've never seen up: a
                            // first-launch connect counts as "newly connected"
                            // too, so its project (loaded empty while connecting)
                            // refreshes.
                            .map(|previous| previous != ControllerLinkState::Connected)
                            .unwrap_or(true)
                })
                .map(|(device_id, _)| device_id.clone())
                .collect()
        } else {
            Vec::new()
        };
        self.remote_link_states = links.clone();
        self.remote_saved_host_ids = saved_host_ids;
        let state = self.ensure_project_list_state(cx);
        state.update(cx, |state, cx| state.set_links(links, cx));
        self.invalidate_ui(cx, [UiRegion::ProjectColumn]);
        // The status-bar "N/M" count derives from both the link states and the
        // saved-host registry, so refresh it whenever either changes.
        self.invalidate_status_bar(cx);
        // Terminal rebind is NOT edge-triggered here — the slow tick reconciles
        // every remote pane against the pooled controller identity, which also
        // covers reconnects this 1 Hz poll never saw as a Disconnected edge.
        if !reconnected.is_empty() {
            self.reload_selected_project_for_reconnected_hosts(&reconnected, cx);
        }
    }

    /// When the selected project's host comes (back) online, re-run its data
    /// loads. `controller_for` reports remote hosts unavailable while connecting,
    /// so a switch made while offline left worktrees/tasks/AI/memory empty; this
    /// repopulates them without freezing the UI on the connect.
    fn reload_selected_project_for_reconnected_hosts(
        &mut self,
        reconnected: &[String],
        cx: &mut Context<Self>,
    ) {
        let on_reconnected_host = self
            .state
            .selected_project
            .as_ref()
            .and_then(|project| project.host_device_id.as_ref())
            .is_some_and(|host| reconnected.iter().any(|device| device == host));
        if !on_reconnected_host {
            return;
        }
        let Some(project_id) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            return;
        };
        self.project_switch_generation = self.project_switch_generation.wrapping_add(1);
        let generation = self.project_switch_generation;
        self.spawn_project_switch_load(project_id, generation, cx);
    }

    fn project_lifecycle_snapshot(&self) -> HashMap<String, AgentLifecycleState> {
        self.state
            .projects
            .iter()
            .filter_map(|project| {
                self.project_agent_lifecycle(project)
                    .map(|lifecycle| (project.id.clone(), lifecycle))
            })
            .collect()
    }

    pub(in crate::app) fn visible_pet_sprite_frame(&self, frame_count: usize) -> usize {
        if self.state.settings.pet_static_mode {
            0
        } else {
            self.pet_sprite_frame % frame_count.max(1)
        }
    }

    pub(super) fn project_column_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<ProjectColumnView> {
        let app_entity = cx.entity();
        let project_list_state = self.ensure_project_list_state(cx);
        let collapsed = self.project_column_collapsed;
        let language = self.state.settings.language.clone();
        let scroll_handle = self.project_scroll_handle.clone();

        if let Some(view) = &self.project_column_view {
            view.update(cx, |view, cx| {
                let changed = view.collapsed != collapsed || view.language != language;

                if !changed {
                    return;
                }

                view.collapsed = collapsed;
                view.language = language;
                view.scroll_handle = scroll_handle;
                cx.notify();
            });
            return view.clone();
        }
        let view = cx.new(|_| ProjectColumnView {
            app_entity: app_entity.clone(),
            project_list_state,
            collapsed,
            language,
            scroll_handle,
            _observe_project_list_state: None,
        });
        view.update(cx, |view, cx| {
            view._observe_project_list_state =
                Some(cx.observe(&view.project_list_state, |_, _, cx| cx.notify()));
        });
        self.project_column_view = Some(view.clone());
        view
    }
}
