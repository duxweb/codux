use super::*;

impl RemoteHostRuntime {
    pub(super) fn remote_terminal_plan_from_envelope(
        &self,
        envelope: &RemoteEnvelope,
        terminal_id: Option<&str>,
        reuse_saved_terminal: bool,
    ) -> Result<RemoteTerminalPlan, String> {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let cwd = envelope
            .payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty());
        // A controller that added this project by browsing a path holds its OWN
        // project id, which this host's project store doesn't have — each side
        // keeps its own projects; only the host *code* is shared, not the data.
        // Git/worktree route to the host by path and work regardless; the terminal
        // scope normally resolves by id, so when the project isn't registered here
        // fall back to a scope synthesized from the cwd the controller sent (the
        // host path) — i.e. just open a terminal in that directory.
        let scope = match self.remote_project_scope_for_envelope(envelope, project_id) {
            Ok(scope) => scope,
            Err(error) => {
                let cwd = cwd.clone().ok_or(error)?;
                self.remote_terminal_scope_from_path(envelope, project_id, &cwd)
            }
        };
        let command = envelope
            .payload
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty());
        let title = envelope
            .payload
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| command.clone())
            .unwrap_or_else(|| "Terminal".to_string());
        let terminal_id = terminal_id.map(str::to_string).or_else(|| {
            if reuse_saved_terminal {
                self.saved_remote_terminal_id(&scope.layout_key)
            } else {
                None
            }
        });
        let cols = envelope
            .payload
            .get("cols")
            .and_then(Value::as_u64)
            .map(|value| value as u16);
        let rows = envelope
            .payload
            .get("rows")
            .and_then(Value::as_u64)
            .map(|value| value as u16);
        let mut config =
            remote_terminal_pty_config(&scope, terminal_id, &title, command, cwd, cols, rows);
        apply_terminal_osc_color_env(&mut config, &envelope.payload);
        // Lazy respawns (subscribe/input envelopes) carry no viewer colors;
        // fall back to the host theme so ConPTY still answers light correctly.
        if let Ok(colors) = self.terminal_osc_colors.lock()
            && let Some((foreground, background)) = colors.as_ref()
        {
            let env = config.env.get_or_insert_with(Default::default);
            env.entry("DMUX_TERMINAL_OSC_FG".to_string())
                .or_insert_with(|| foreground.clone());
            env.entry("DMUX_TERMINAL_OSC_BG".to_string())
                .or_insert_with(|| background.clone());
        }
        Ok(RemoteTerminalPlan {
            config,
            scope,
            title,
        })
    }

    pub(super) fn ensure_remote_terminal_started(
        self: &Arc<Self>,
        session_id: &str,
        envelope: &RemoteEnvelope,
    ) -> Result<(), String> {
        if self.terminals.snapshot(session_id).is_ok() {
            self.ensure_terminal_event_subscription(session_id);
            return Ok(());
        }
        let runtime = Arc::clone(self);
        let emit = move |event| {
            runtime.handle_terminal_event(event);
        };
        let plan = self.remote_terminal_plan_from_envelope(envelope, Some(session_id), true)?;
        self.set_remote_project_scope(envelope.device_id.as_deref(), &plan.scope.project_id);
        self.terminals
            .create_with_event_key(
                plan.config,
                format!("remote-terminal:{session_id}"),
                Arc::new(move |event| {
                    emit(event);
                    true
                }),
            )
            .map_err(|error| error.to_string())?;
        self.persist_remote_terminal_layout(&plan.scope.layout_key, session_id, &plan.title);
        self.publish_remote_terminal_layout_changed();
        self.mark_terminal_event_subscription(session_id);
        self.send_terminal_list(envelope.device_id.as_deref());
        Ok(())
    }

    pub(super) fn ensure_remote_project_terminal(
        self: &Arc<Self>,
        scope: &RemoteProjectScope,
        device_id: Option<&str>,
    ) -> Result<String, String> {
        let existing = self
            .remote_terminals()
            .into_iter()
            .find(|terminal| {
                terminal.get("projectId").and_then(Value::as_str) == Some(scope.project_id.as_str())
                    && terminal.get("worktreeId").and_then(Value::as_str)
                        == Some(scope.worktree_id.as_str())
            })
            .and_then(|terminal| {
                terminal
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.trim().is_empty())
                    .map(str::to_string)
            });
        if let Some(session_id) = existing {
            if self.remote_terminal_session_matches_scope(&session_id, scope) {
                self.ensure_terminal_event_subscription(&session_id);
                return Ok(session_id);
            }
        }

        let terminal_id = self
            .saved_remote_terminal_id(&scope.layout_key)
            .or_else(|| Some(remote_terminal_id_for_scope(scope)));
        let title = "Terminal".to_string();
        let config =
            remote_terminal_pty_config(scope, terminal_id.clone(), &title, None, None, None, None);
        let runtime = Arc::clone(self);
        let emit = move |event| {
            runtime.handle_terminal_event(event);
        };
        let session_id = self
            .terminals
            .create_with_event_key(
                config,
                terminal_id
                    .as_deref()
                    .map(|terminal_id| format!("remote-terminal:{terminal_id}"))
                    .unwrap_or_else(|| "remote-terminal".to_string()),
                Arc::new(move |event| {
                    emit(event);
                    true
                }),
            )
            .map_err(|error| error.to_string())?;
        self.persist_remote_terminal_layout(&scope.layout_key, &session_id, &title);
        self.publish_remote_terminal_layout_changed();
        self.mark_terminal_event_subscription(&session_id);
        self.register_terminal_viewer(&session_id, device_id);
        Ok(session_id)
    }

    pub(super) fn remote_terminal_session_matches_scope(
        &self,
        session_id: &str,
        scope: &RemoteProjectScope,
    ) -> bool {
        self.terminals
            .session(session_id)
            .ok()
            .map(|session| {
                let info = session.info();
                info.project_id == scope.worktree_id
                    && normalize_remote_path(&info.cwd)
                        == normalize_remote_path(&scope.project_path)
            })
            .unwrap_or(false)
    }

    pub(super) fn saved_remote_terminal_id(&self, layout_key: &str) -> Option<String> {
        let layout = TerminalLayoutService::new(self.support_dir.clone()).load(Some(layout_key));
        layout
            .top_panes
            .first()
            .map(|pane| pane.terminal_id.clone())
            .or_else(|| layout.tabs.first().map(|tab| tab.terminal_id.clone()))
            .filter(|id| !id.trim().is_empty())
    }

    pub(super) fn persist_remote_terminal_layout(
        &self,
        layout_key: &str,
        terminal_id: &str,
        title: &str,
    ) {
        if layout_key.trim().is_empty() {
            return;
        }
        let service = TerminalLayoutService::new(self.support_dir.clone());
        let layout = service.load(Some(layout_key));
        if layout
            .top_panes
            .iter()
            .any(|pane| pane.terminal_id == terminal_id)
            || layout.tabs.iter().any(|tab| tab.terminal_id == terminal_id)
        {
            return;
        }
        let title = if title.trim().is_empty() {
            "Terminal"
        } else {
            title.trim()
        };
        let mut layout = layout;
        layout.top_panes.push(TerminalPaneSummary {
            title: title.to_string(),
            terminal_id: terminal_id.to_string(),
        });
        layout.tabs.clear();
        let _ = service.save_summary(layout_key, layout);
    }

    pub(super) fn remote_project_scope_with_worktree(
        &self,
        project_id: &str,
        preferred_worktree_id: Option<&str>,
    ) -> Result<RemoteProjectScope, String> {
        let baseline = ProjectStore::new(self.support_dir.clone()).snapshot();
        let project = baseline
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        let preferred_worktree_id = preferred_worktree_id
            .filter(|worktree_id| {
                worktree_id.trim() == project.id
                    || baseline.worktrees.iter().any(|worktree| {
                        worktree.project_id == project.id && worktree.id == worktree_id.trim()
                    })
            })
            .map(str::to_string);
        let worktree_id = preferred_worktree_id
            .or_else(|| {
                baseline
                    .selected_worktree_id_by_project
                    .get(&project.id)
                    .cloned()
                    .filter(|worktree_id| {
                        worktree_id == &project.id
                            || baseline.worktrees.iter().any(|worktree| {
                                worktree.project_id == project.id && &worktree.id == worktree_id
                            })
                    })
            })
            .unwrap_or_else(|| project.id.clone());
        let worktree_path = baseline
            .worktrees
            .iter()
            .find(|worktree| worktree.project_id == project.id && worktree.id == worktree_id)
            .map(|worktree| worktree.path.clone())
            .filter(|path| !path.trim().is_empty())
            .unwrap_or_else(|| project.path.clone());
        Ok(RemoteProjectScope {
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            project_path: worktree_path,
            worktree_id: worktree_id.clone(),
            layout_key: terminal_layout_storage_key(&project.id, &worktree_id),
        })
    }

    pub(super) fn remote_project_scope_for_envelope(
        &self,
        envelope: &RemoteEnvelope,
        project_id: Option<&str>,
    ) -> Result<RemoteProjectScope, String> {
        let Some(scoped_project_id) = project_id
            .map(str::to_string)
            .or_else(|| self.remote_project_scope_id(envelope.device_id.as_deref()))
        else {
            return Err("Project id is required.".to_string());
        };
        let worktree_id = envelope
            .payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        self.remote_project_scope_with_worktree(&scoped_project_id, worktree_id.as_deref())
    }

    /// Build a terminal scope from a host path for a project this host doesn't
    /// have registered (the controller added it by browsing, so it holds an id
    /// we don't know). Keeps the controller's project id for stable layout
    /// keying, and uses the path as the worktree/cwd — the host runs the shell
    /// there just as it would for a local project at that path.
    pub(super) fn remote_terminal_scope_from_path(
        &self,
        envelope: &RemoteEnvelope,
        project_id: Option<&str>,
        path: &str,
    ) -> RemoteProjectScope {
        let project_id = project_id
            .map(str::to_string)
            .or_else(|| self.remote_project_scope_id(envelope.device_id.as_deref()))
            .unwrap_or_else(|| path.to_string());
        let worktree_id = envelope
            .payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| project_id.clone());
        RemoteProjectScope {
            project_id: project_id.clone(),
            project_name: default_project_name(path),
            project_path: path.to_string(),
            worktree_id: worktree_id.clone(),
            layout_key: terminal_layout_storage_key(&project_id, &worktree_id),
        }
    }

    pub(super) fn worktree_request_scope(
        &self,
        envelope: &RemoteEnvelope,
    ) -> Result<(String, String), String> {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Project id is required.".to_string())?;
        let project_path = envelope
            .payload
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                ProjectStore::new(self.support_dir.clone())
                    .projects_snapshot()
                    .into_iter()
                    .find(|project| project.id == project_id)
                    .map(|project| project.path)
            })
            .ok_or_else(|| "Project path is required.".to_string())?;
        Ok((project_id.to_string(), project_path))
    }

    pub(super) fn set_remote_project_scope(&self, device_id: Option<&str>, project_id: &str) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        if let Ok(mut scopes) = self.remote_project_scope_by_device.lock() {
            scopes.insert(device_id.to_string(), project_id.to_string());
        }
    }

    pub(super) fn remote_project_scope_id(&self, device_id: Option<&str>) -> Option<String> {
        let device_id = device_id.filter(|value| !value.trim().is_empty())?;
        self.remote_project_scope_by_device
            .lock()
            .ok()
            .and_then(|scopes| scopes.get(device_id).cloned())
    }

    pub(super) fn clear_remote_project_scope(&self, device_id: Option<&str>) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        if let Ok(mut scopes) = self.remote_project_scope_by_device.lock() {
            scopes.remove(device_id);
        }
    }

    pub(super) fn clear_remote_project_scope_for_project(&self, project_id: &str) {
        if let Ok(mut scopes) = self.remote_project_scope_by_device.lock() {
            scopes.retain(|_, scoped_project_id| scoped_project_id != project_id);
        }
    }

    pub(super) fn ensure_terminal_event_subscription(self: &Arc<Self>, session_id: &str) {
        let should_subscribe = self.mark_terminal_event_subscription(session_id);
        if !should_subscribe {
            return;
        }
        let runtime = Arc::clone(self);
        let emit = Arc::new(move |event| {
            runtime.handle_terminal_event(event);
            true
        });
        if self
            .terminals
            .subscribe_events_keyed(session_id, format!("remote-terminal:{session_id}"), emit)
            .is_err()
        {
            if let Ok(mut subscriptions) = self.terminal_event_subscriptions.lock() {
                subscriptions.remove(session_id);
            }
        }
    }

    pub(super) fn mark_terminal_event_subscription(&self, session_id: &str) -> bool {
        self.terminal_event_subscriptions
            .lock()
            .map(|mut subscriptions| subscriptions.insert(session_id.to_string()))
            .unwrap_or(false)
    }

    pub(super) fn next_terminal_output_seq(&self, session_id: &str) -> TerminalSequence {
        self.terminal_output_seq_by_session
            .lock()
            .map(|mut sequences| {
                let next = sequences.get(session_id).copied().unwrap_or(0) + 1;
                sequences.insert(session_id.to_string(), next);
                next
            })
            .unwrap_or(0)
    }

    pub(super) fn current_terminal_output_seq(&self, session_id: &str) -> TerminalSequence {
        self.terminal_output_seq_by_session
            .lock()
            .ok()
            .and_then(|sequences| sequences.get(session_id).copied())
            .unwrap_or(0)
    }

    pub(super) fn clear_terminal_output_seq(&self, session_id: &str) {
        if let Ok(mut sequences) = self.terminal_output_seq_by_session.lock() {
            sequences.remove(session_id);
        }
    }

    pub(super) fn record_terminal_output_ack(
        &self,
        session_id: &str,
        device_id: Option<&str>,
        output_seq: Option<TerminalSequence>,
    ) {
        let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };
        let Some(output_seq) = output_seq else {
            return;
        };
        if let Ok(mut acks) = self.terminal_output_ack_by_viewer.lock() {
            let key = (session_id.to_string(), device_id.to_string());
            let current = acks.get(&key).copied().unwrap_or(0);
            if output_seq > current {
                acks.insert(key, output_seq);
            }
        }
    }

    pub(super) fn terminal_viewer_ack_seq(
        &self,
        session_id: &str,
        device_id: Option<&str>,
    ) -> TerminalSequence {
        let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return 0;
        };
        self.terminal_output_ack_by_viewer
            .lock()
            .ok()
            .and_then(|acks| {
                acks.get(&(session_id.to_string(), device_id.to_string()))
                    .copied()
            })
            .unwrap_or(0)
    }

    pub(super) fn terminal_viewer_is_stale(
        &self,
        session_id: &str,
        device_id: Option<&str>,
    ) -> bool {
        let current = self.current_terminal_output_seq(session_id);
        current > 0
            && current.saturating_sub(self.terminal_viewer_ack_seq(session_id, device_id))
                > REMOTE_TERMINAL_STALE_OUTPUT_SEQ_LAG
    }

    pub(super) fn clear_terminal_viewer_ack(&self, session_id: &str, device_id: Option<&str>) {
        let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };
        if let Ok(mut acks) = self.terminal_output_ack_by_viewer.lock() {
            acks.remove(&(session_id.to_string(), device_id.to_string()));
        }
    }

    pub(super) fn clear_terminal_device_acks(&self, device_id: &str) {
        if let Ok(mut acks) = self.terminal_output_ack_by_viewer.lock() {
            acks.retain(|(_, viewer), _| viewer != device_id);
        }
    }

    pub(super) fn clear_terminal_session_acks(&self, session_id: &str) {
        if let Ok(mut acks) = self.terminal_output_ack_by_viewer.lock() {
            acks.retain(|(session, _), _| session != session_id);
        }
    }

    pub(super) fn register_terminal_viewer(
        self: &Arc<Self>,
        session_id: &str,
        device_id: Option<&str>,
    ) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.terminal_subscriptions
            .add_session_viewer(session_id, device_id);
        self.terminals.restore_remote_screen_scrollback(session_id);
        self.ensure_terminal_event_subscription(session_id);
    }

    pub(super) fn register_project_terminal_viewers(
        self: &Arc<Self>,
        project_id: &str,
        device_id: Option<&str>,
    ) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.terminal_subscriptions
            .add_project_subscriber(project_id, device_id);
        for terminal in self.remote_terminals() {
            let Some(terminal_project_id) = terminal.get("projectId").and_then(Value::as_str)
            else {
                continue;
            };
            if terminal_project_id != project_id {
                continue;
            }
            let Some(session_id) = terminal.get("id").and_then(Value::as_str) else {
                continue;
            };
            self.register_terminal_viewer(session_id, Some(device_id));
        }
    }

    pub(super) fn send_project_terminal_baselines(
        self: &Arc<Self>,
        project_id: &str,
        device_id: Option<&str>,
        envelope: &RemoteEnvelope,
    ) {
        let sessions = self
            .remote_terminals()
            .into_iter()
            .filter(|terminal| {
                terminal.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .filter_map(|terminal| {
                terminal
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        let target_session_id = envelope
            .payload
            .get("baselineSessionId")
            .or_else(|| envelope.payload.get("sessionId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        for session_id in sessions {
            let mut envelope = envelope.clone();
            if target_session_id.as_deref() != Some(session_id.as_str()) {
                if let Some(payload) = envelope.payload.as_object_mut() {
                    payload.remove("viewportCols");
                    payload.remove("viewportRows");
                }
            }
            self.spawn_terminal_baseline(&session_id, device_id, &envelope);
        }
    }

    pub(super) fn send_project_terminal_viewport_states(
        &self,
        project_id: &str,
        device_id: Option<&str>,
    ) {
        let sessions = self
            .remote_terminals()
            .into_iter()
            .filter(|terminal| {
                terminal.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .filter_map(|terminal| {
                terminal
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        for session_id in sessions {
            self.send_terminal_viewport_state(&session_id, device_id);
        }
    }

    pub(super) fn spawn_terminal_baseline(
        self: &Arc<Self>,
        session_id: &str,
        device_id: Option<&str>,
        envelope: &RemoteEnvelope,
    ) {
        let runtime = Arc::clone(self);
        let session_id = session_id.to_string();
        let device_id = device_id.map(str::to_string);
        let envelope = envelope.clone();
        crate::async_runtime::spawn(async move {
            let session_id_for_log = session_id.clone();
            if let Err(error) = crate::async_runtime::spawn_blocking(move || {
                runtime.send_terminal_baseline(&session_id, device_id.as_deref(), &envelope);
            })
            .await
            {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!(
                        "terminal_buffer baseline_task_failed session={session_id_for_log} error={error}"
                    ),
                );
            }
        });
    }

    pub(super) fn send_terminal_baseline(
        self: &Arc<Self>,
        session_id: &str,
        device_id: Option<&str>,
        envelope: &RemoteEnvelope,
    ) {
        let mut options = self.terminal_baseline_options(envelope, true);
        if options.request_id.is_none() {
            options.request_id = Some(format!("subscribe-{}-{session_id}", uuid::Uuid::new_v4()));
        }
        self.send_terminal_buffer(session_id, device_id, 0, options);
    }

    pub(super) fn terminal_baseline_options(
        &self,
        envelope: &RemoteEnvelope,
        default_tail: bool,
    ) -> TerminalBaselineOptions {
        let max_chars = envelope
            .payload
            .get("maxChars")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .filter(|value| *value > 0)
            .unwrap_or(REMOTE_TERMINAL_BUFFER_MAX_CHARS);
        let chunk_chars = envelope
            .payload
            .get("chunkChars")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .filter(|value| *value > 0)
            .map(|value| value.clamp(4 * 1024, 64 * 1024));
        let request_id = envelope
            .payload
            .get("requestId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let tail = envelope
            .payload
            .get("tail")
            .and_then(Value::as_bool)
            .unwrap_or(default_tail);
        let viewport = if tail {
            match (
                envelope
                    .payload
                    .get("viewportCols")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .filter(|value| *value > 0),
                envelope
                    .payload
                    .get("viewportRows")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .filter(|value| *value > 0),
            ) {
                (Some(cols), Some(rows)) => Some(BaselineViewport { cols, rows }),
                _ => None,
            }
        } else {
            None
        };
        TerminalBaselineOptions {
            max_chars,
            chunk_chars,
            request_id,
            tail,
            viewport,
        }
    }

    pub(super) fn remove_terminal_viewer_for_session(
        &self,
        session_id: &str,
        device_id: Option<&str>,
    ) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.terminal_subscriptions
            .remove_session_viewer(session_id, device_id);
        self.clear_terminal_viewer_ack(session_id, Some(device_id));
        if self.terminal_output_viewers(session_id).is_empty() {
            self.terminals.shrink_remote_screen_scrollback(session_id);
        }
    }

    pub(super) fn remove_project_terminal_viewers(
        &self,
        project_id: &str,
        device_id: Option<&str>,
    ) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.terminal_subscriptions
            .remove_project_subscriber(project_id, device_id);
        let session_ids = self
            .remote_terminals()
            .into_iter()
            .filter(|terminal| {
                terminal.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .filter_map(|terminal| {
                terminal
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        self.terminal_subscriptions
            .remove_project_session_viewers(session_ids.iter().map(String::as_str), device_id);
        for session_id in session_ids {
            self.clear_terminal_viewer_ack(&session_id, Some(device_id));
            if self.terminal_output_viewers(&session_id).is_empty() {
                self.terminals.shrink_remote_screen_scrollback(&session_id);
            }
        }
    }

    pub(super) fn remove_terminal_viewer(&self, device_id: Option<&str>) {
        let Some(device_id) = device_id else {
            return;
        };
        let session_ids = self
            .terminals
            .list()
            .into_iter()
            .filter(|terminal| {
                self.terminal_output_viewers(&terminal.id)
                    .contains(device_id)
            })
            .map(|terminal| terminal.id)
            .collect::<Vec<_>>();
        self.resource_subscriptions.remove_device(device_id);
        self.terminal_subscriptions.remove_device(device_id);
        self.clear_terminal_device_acks(device_id);
        self.clear_ai_stats_watcher_device(device_id);
        for session_id in session_ids {
            if self.terminal_output_viewers(&session_id).is_empty() {
                self.terminals.shrink_remote_screen_scrollback(&session_id);
            }
        }
    }

    pub(super) fn release_all_remote_viewports(&self) {
        for terminal in self.terminals.list() {
            let Ok(state) = self.terminals.viewport_state(&terminal.id) else {
                continue;
            };
            if state.owner == crate::terminal_pty::terminal_viewport_local_owner() {
                continue;
            }
            let _ = self.terminals.release_viewport(&terminal.id, &state.owner);
        }
    }

    pub(super) fn handle_terminal_event(self: &Arc<Self>, event: TerminalEvent) {
        match event {
            TerminalEvent::Output {
                session_id, text, ..
            } => {
                self.queue_terminal_output_batch(session_id, text);
            }
            TerminalEvent::Exit { session_id, .. } => {
                if let Ok(mut subscriptions) = self.terminal_event_subscriptions.lock() {
                    subscriptions.remove(&session_id);
                }
                self.terminal_subscriptions.remove_session(&session_id);
                self.clear_terminal_output_seq(&session_id);
                self.clear_terminal_session_acks(&session_id);
                self.send_terminal_data(
                    REMOTE_TERMINAL_CLOSED,
                    None,
                    Some(&session_id),
                    json!({ "id": session_id }),
                );
            }
            TerminalEvent::Error {
                session_id,
                message,
            } => {
                self.send(
                    "error",
                    None,
                    Some(&session_id),
                    json!({ "message": message }),
                );
            }
            TerminalEvent::Viewport {
                session_id,
                owner,
                cols,
                rows,
                generation,
            } => {
                let state = TerminalViewportState {
                    owner,
                    cols,
                    rows,
                    generation,
                    owner_label: None,
                };
                let viewers = self.terminal_output_viewers(&session_id);
                if viewers.is_empty() {
                    self.send_terminal_data(
                        REMOTE_TERMINAL_VIEWPORT_STATE,
                        None,
                        Some(&session_id),
                        self.terminal_viewport_state_payload(&session_id, None, &state),
                    );
                } else {
                    for device_id in viewers {
                        self.send_terminal_data(
                            REMOTE_TERMINAL_VIEWPORT_STATE,
                            Some(&device_id),
                            Some(&session_id),
                            self.terminal_viewport_state_payload(
                                &session_id,
                                Some(&device_id),
                                &state,
                            ),
                        );
                    }
                }
            }
        }
    }

    pub(super) fn queue_terminal_output_batch(self: &Arc<Self>, session_id: String, text: String) {
        if text.is_empty() {
            return;
        }
        let viewers = self.terminal_output_viewers(&session_id);
        if viewers.is_empty() {
            return;
        }
        let buffer_length = self.terminals.buffer_characters(&session_id).unwrap_or(0);
        let should_spawn = {
            let Ok(mut batches) = self.terminal_output_batches.lock() else {
                return;
            };
            let batch =
                batches
                    .entry(session_id.clone())
                    .or_insert_with(|| RemoteTerminalOutputBatch {
                        data: String::new(),
                        buffer_length,
                        viewers: HashSet::new(),
                    });
            let was_empty = batch.data.is_empty();
            batch.data.push_str(&text);
            batch.buffer_length = buffer_length;
            batch.viewers.extend(viewers);
            was_empty
        };
        if should_spawn {
            let runtime = Arc::clone(self);
            crate::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(REMOTE_TERMINAL_OUTPUT_BATCH_MS)).await;
                runtime.flush_terminal_output_batch(&session_id);
            });
        }
    }

    pub(super) fn terminal_output_viewers(&self, session_id: &str) -> HashSet<String> {
        let project_id = self
            .terminals
            .list()
            .into_iter()
            .find(|terminal| terminal.id == session_id)
            .map(|terminal| terminal.project_id)
            .filter(|value| !value.trim().is_empty());
        self.terminal_subscriptions
            .viewers_for_session(session_id, project_id.as_deref())
    }

    pub(super) fn flush_terminal_output_batch(&self, session_id: &str) {
        let batch = self
            .terminal_output_batches
            .lock()
            .ok()
            .and_then(|mut batches| batches.remove(session_id));
        let Some(batch) = batch else {
            return;
        };
        if batch.data.is_empty() || batch.viewers.is_empty() {
            return;
        }
        let output_seq = self.next_terminal_output_seq(session_id);
        let payload = terminal_live_output_payload(batch.data, batch.buffer_length, output_seq);
        // Serialize the payload once and fan it out raw, so N subscribers of the
        // same terminal don't each clone + re-serialize the whole batch. Falls
        // back to the per-device path if the payload can't be pre-serialized.
        match serde_json::value::to_raw_value(&payload) {
            Ok(payload_raw) => {
                for device_id in &batch.viewers {
                    self.send_terminal_output_raw(
                        Some(device_id.as_str()),
                        Some(session_id),
                        &payload_raw,
                    );
                }
            }
            Err(_) => {
                for device_id in &batch.viewers {
                    self.send_terminal_data(
                        REMOTE_TERMINAL_OUTPUT,
                        Some(device_id.as_str()),
                        Some(session_id),
                        payload.clone(),
                    );
                }
            }
        }
        // Self-healing ownership (design 3): ride the authoritative viewport owner
        // alongside the live output stream. A viewer that missed the one-shot
        // owner-change broadcast -- dropped on the wire, backgrounded, or
        // mid-reconnect when the desktop (or another device) took over -- would
        // otherwise keep rendering the live grid forever. Re-sending the current
        // owner on the output path makes any viewer converge on the next frame
        // instead of relying on a single broadcast landing.
        //
        // Throttled to every 8th flush (~4/s during continuous output, 0 when
        // idle) and sent ONLY to viewers that are NOT the current owner: the
        // active owner already drives the grid and would just eat redundant
        // resize/repaint ticks. The viewer's generation guard dedups the rest.
        // Idle sessions (no output) self-heal via the keepalive echo; (re)subscribe
        // self-heals via send_terminal_viewport_state on the subscribe path.
        if output_seq % REMOTE_TERMINAL_OWNER_REASSERT_EVERY == 0 {
            if let Ok(state) = self.terminals.viewport_state(session_id) {
                for device_id in &batch.viewers {
                    if state.owner != terminal_viewport_remote_owner(device_id) {
                        self.send_terminal_viewport_state_payload(
                            session_id,
                            Some(device_id.as_str()),
                            &state,
                        );
                    }
                }
            }
        }
    }
}

/// Adapts the desktop host to the shared remote-terminal router
/// ([`RemoteTerminalDispatch`]). It holds the host (so the create arm can clone
/// an `Arc` for its output-event closure) and the inbound envelope (so each
pub(super) fn remote_terminal_pty_config(
    scope: &RemoteProjectScope,
    terminal_id: Option<String>,
    title: &str,
    command: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> TerminalPtyConfig {
    let cwd = cwd
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| scope.project_path.clone());
    let terminal_id = terminal_id.filter(|value| !value.trim().is_empty());
    let session_key = terminal_id
        .as_ref()
        .map(|terminal_id| format!("gpui:{}:{terminal_id}", scope.worktree_id));
    let session_instance_id = session_key.as_ref().map(|session_key| {
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, session_key.as_bytes()).to_string()
    });
    TerminalPtyConfig {
        cwd: Some(cwd),
        command,
        cols,
        rows,
        project_id: Some(scope.worktree_id.clone()),
        worktree_id: Some(scope.worktree_id.clone()),
        project_name: Some(scope.project_name.clone()),
        terminal_id,
        session_key,
        session_instance_id,
        title: Some(title.to_string()),
        ..Default::default()
    }
}

pub(super) fn remote_terminal_id_for_scope(scope: &RemoteProjectScope) -> String {
    format!("gpui-term-{}-{}", scope.worktree_id, uuid::Uuid::new_v4())
}

#[cfg(windows)]
pub(super) fn normalize_remote_path(path: &str) -> String {
    let path = path.trim();
    let path = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    };
    trim_remote_path_tail(&path)
}

#[cfg(not(windows))]
pub(super) fn normalize_remote_path(path: &str) -> String {
    trim_remote_path_tail(path.trim())
}

pub(super) fn trim_remote_path_tail(path: &str) -> String {
    if path == "/" {
        path.to_string()
    } else {
        path.trim_end_matches(['/', '\\']).to_string()
    }
}
