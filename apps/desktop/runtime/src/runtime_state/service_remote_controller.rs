// Desktop-as-controller domain: pair with remote hosts and drive their domains
// over the controller transport. Browsing/creating directories on a host backs
// the add-project remote flow; routing a hosted project's other domains builds
// on `controller_for`.

/// How long an explicit, blocking-pool remote operation (the add-project file
/// browser, the terminal attach on project open) waits for a paired host to
/// (re)connect before reporting it unreachable. Long enough to cover an iroh
/// relay/holepunch dial on first use after launch — which can take a few seconds
/// — short enough that a genuinely offline host fails with a clear message
/// instead of hanging.
const REMOTE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

impl RuntimeService {
    pub fn local_host_metrics(&self) -> codux_protocol::RemoteHostMetrics {
        codux_runtime_live::host_metrics::sample_host_metrics()
    }

    pub fn open_host_browser_url(
        &self,
        device_id: &str,
        target_url: &str,
    ) -> Result<crate::host_browser::HostBrowserOpenResult, String> {
        let controller = self.remote_controllers.controller_for(device_id)?;
        let saved = self
            .remote_controllers
            .saved_hosts()
            .into_iter()
            .find(|host| host.device_id == device_id)
            .ok_or_else(|| format!("No saved remote host for device {device_id}."))?;
        self.host_browser_proxy.open(
            saved.device_id,
            saved.device_token,
            target_url,
            controller as std::sync::Arc<dyn crate::host_browser::HostBrowserController>,
        )
    }

    pub fn open_host_browser_session(
        &self,
        device_id: &str,
    ) -> Result<crate::host_browser::HostBrowserOpenResult, String> {
        let controller = self.remote_controllers.controller_for(device_id)?;
        let saved = self
            .remote_controllers
            .saved_hosts()
            .into_iter()
            .find(|host| host.device_id == device_id)
            .ok_or_else(|| format!("No saved remote host for device {device_id}."))?;
        self.host_browser_proxy.open_session(
            saved.device_id,
            saved.device_token,
            controller as std::sync::Arc<dyn crate::host_browser::HostBrowserController>,
        )
    }

    /// Pair with a remote host from a pasted `codux://pair` ticket, persist it,
    /// and cache the live connection.
    pub fn pair_remote_host(
        &self,
        ticket: &str,
        device_name: &str,
    ) -> Result<crate::remote::SavedRemoteHost, String> {
        self.remote_controllers.pair(ticket, device_name)
    }

    /// Every host this desktop has paired with and can reconnect to.
    pub fn saved_remote_hosts(&self) -> Vec<crate::remote::SavedRemoteHost> {
        self.remote_controllers.saved_hosts()
    }

    /// Eagerly connect — and keep reconnecting — every saved host, independent
    /// of whether a project on it is open. Called on launch and from the remote
    /// "reconnect" action so a paired host holds its link on its own.
    pub fn ensure_saved_remote_hosts_connected(&self) {
        self.remote_controllers.ensure_saved_hosts_connected();
    }

    /// Drop a paired host and any live connection to it.
    pub fn forget_remote_host(
        &self,
        device_id: &str,
    ) -> Result<Vec<crate::remote::SavedRemoteHost>, String> {
        self.remote_controllers.forget(device_id)
    }

    /// Per-device client→host link states, for the project connection badge and
    /// for re-attaching terminals when a dropped host comes back. Event-driven:
    /// the controller transport's state callback updates this; we just read the
    /// cached snapshot.
    pub fn remote_controller_link_states(
        &self,
    ) -> std::collections::HashMap<String, crate::remote::ControllerLinkState> {
        self.remote_controllers.link_states()
    }

    /// Per-device link path types (direct/relay) for the connection badge. Like
    /// the link states, the transport's path-event callback updates this; we just
    /// read the cached snapshot.
    pub fn remote_controller_link_paths(
        &self,
    ) -> std::collections::HashMap<String, crate::remote::ControllerLinkPath> {
        self.remote_controllers.link_paths()
    }

    /// List a directory on a remote host (for the add-project remote browser),
    /// parsed into a typed listing so the UI never touches the wire JSON.
    pub fn remote_browse_directory(
        &self,
        device_id: &str,
        path: Option<&str>,
        purpose: Option<&str>,
    ) -> Result<crate::remote::RemoteDirectoryListing, String> {
        // Runs on the blocking pool from an explicit user click: wait (bounded)
        // for the host to connect so the first browse after launch shows the
        // listing, rather than an empty pane until a second click.
        self.remote_controllers
            .controller_for_blocking(device_id, REMOTE_CONNECT_TIMEOUT)?
            .browse_directory(path, purpose)
    }

    /// Create a directory on a remote host (for the add-project remote flow).
    pub fn remote_create_directory(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<serde_json::Value, String> {
        self.remote_controllers
            .controller_for_blocking(device_id, REMOTE_CONNECT_TIMEOUT)?
            .create_directory(path)
    }

    pub fn remote_delete_path(&self, device_id: &str, path: &str) -> Result<(), String> {
        self.remote_controllers
            .controller_for_blocking(device_id, REMOTE_CONNECT_TIMEOUT)?
            .delete_path(path)
            .map(|_| ())
    }

    pub fn remote_rename_path(
        &self,
        device_id: &str,
        path: &str,
        new_path: &str,
    ) -> Result<(), String> {
        self.remote_controllers
            .controller_for_blocking(device_id, REMOTE_CONNECT_TIMEOUT)?
            .rename_path(path, new_path)
            .map(|_| ())
    }

    /// List a local directory for the in-app file picker — same shape as the
    /// remote browser, so the picker UI is unified for local and remote. Hidden
    /// entries are skipped; directories sort first.
    pub fn browse_local_directory(
        &self,
        path: Option<&str>,
        purpose: Option<&str>,
    ) -> Result<crate::remote::RemoteDirectoryListing, String> {
        use codux_runtime_core::path::{FILE_LIST_DRIVES_SENTINEL, display_path};
        let show_hidden = purpose == Some("sshKey");
        // Volume list (the Windows "all drives" root): reuse the shared core
        // listing so local and remote browsing expose drives identically.
        if path.map(str::trim) == Some(FILE_LIST_DRIVES_SENTINEL) {
            let value = codux_runtime_core::file::file_list_payload(path, purpose);
            return Ok(local_directory_listing_from_payload(&value));
        }
        let dir = match path {
            Some(value) if !value.trim().is_empty() => std::path::PathBuf::from(value.trim()),
            _ => crate::runtime_paths::home_dir(),
        };
        let dir = dir.canonicalize().unwrap_or(dir);
        let parent = match dir.parent() {
            // `display_path` strips the `\\?\` prefix that `canonicalize` adds on
            // Windows, so the picker shows `C:\…` instead of `\\?\C:\…`.
            Some(parent) => Some(display_path(&parent.to_string_lossy())),
            None => drive_root_parent(),
        };
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&dir)
            .map_err(|error| error.to_string())?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let entry_path = entry.path();
            let is_dir = entry_path.is_dir();
            entries.push(crate::remote::RemoteDirectoryEntry {
                name,
                path: display_path(&entry_path.to_string_lossy()),
                is_dir,
            });
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(crate::remote::RemoteDirectoryListing {
            path: display_path(&dir.to_string_lossy()),
            parent,
            entries,
        })
    }

    /// Create a local directory (the picker's "New folder" for local browsing).
    pub fn create_local_directory(&self, path: &str) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|error| error.to_string())
    }

    pub fn delete_local_path(&self, path: &str) -> Result<(), String> {
        crate::files::delete_absolute_path(path)
    }

    pub fn rename_local_path(&self, path: &str, new_path: &str) -> Result<(), String> {
        crate::files::rename_absolute_path(path, new_path)
    }

    /// Fetch a remote host's identity/capabilities (also a reachability check).
    pub fn remote_host_info(&self, device_id: &str) -> Result<serde_json::Value, String> {
        self.remote_controllers
            .controller_for(device_id)?
            .host_info()
    }

    pub fn remote_host_info_blocking(
        &self,
        device_id: &str,
    ) -> Result<serde_json::Value, String> {
        self.remote_controllers
            .controller_for_blocking(device_id, REMOTE_CONNECT_TIMEOUT)?
            .host_info()
    }

    pub fn remote_host_metrics(
        &self,
        device_id: &str,
    ) -> Result<codux_protocol::RemoteHostMetrics, String> {
        self.remote_controllers
            .controller_for_blocking(device_id, REMOTE_CONNECT_TIMEOUT)?
            .host_metrics()
    }

    /// The live controller for a device (used by the terminal UI to drive a
    /// remote-hosted project's terminals).
    pub fn remote_controller_for_device(
        &self,
        device_id: &str,
    ) -> Result<std::sync::Arc<crate::remote::RemoteController>, String> {
        self.remote_controllers.controller_for(device_id)
    }

    /// Like [`remote_controller_for_device`](Self::remote_controller_for_device),
    /// but waits (bounded) for the host to connect. The terminal attach on
    /// project open runs on the blocking pool and otherwise hit `controller_for`
    /// during the first few seconds after launch — before the iroh dial
    /// completes — so it failed with "not ready yet" and the pane stayed blank
    /// (the attach fires once and never retries). Waiting here lets the first
    /// terminal attach succeed once the link comes up.
    pub fn remote_controller_for_device_blocking(
        &self,
        device_id: &str,
    ) -> Result<std::sync::Arc<crate::remote::RemoteController>, String> {
        self.remote_controllers
            .controller_for_blocking(device_id, REMOTE_CONNECT_TIMEOUT)
    }

    /// Route a git mutation to the host if the project is remote, returning the
    /// refreshed `GitSummary`. `None` ⇒ the project is local (caller runs locally).
    pub(crate) fn remote_git_invoke(
        &self,
        project_path: &str,
        op: &str,
        args: serde_json::Value,
    ) -> Option<Result<crate::git::GitSummary, String>> {
        let device_id = self.host_device_for_project_path(project_path)?;
        Some(
            self.remote_controllers
                .controller_for(&device_id)
                .and_then(|controller| controller.git_invoke(op, project_path, args))
                .map(|value| git_summary_from_payload(&value)),
        )
    }

    /// Blocking-pool variant for explicit user Git operations. It waits briefly
    /// for a paired host to reconnect, matching the first-use behavior of
    /// remote directory browsing and terminal attach.
    pub(crate) fn remote_git_invoke_blocking(
        &self,
        project_path: &str,
        op: &str,
        args: serde_json::Value,
    ) -> Option<Result<crate::git::GitSummary, String>> {
        let device_id = self.host_device_for_project_path(project_path)?;
        Some(
            self.remote_controllers
                .controller_for_blocking(&device_id, REMOTE_CONNECT_TIMEOUT)
                .and_then(|controller| controller.git_invoke(op, project_path, args))
                .map(|value| git_summary_from_payload(&value)),
        )
    }

    /// Route a git read to the host if the project is remote. `None` ⇒ local.
    pub(crate) fn remote_git_read(
        &self,
        project_path: &str,
        op: &str,
        args: serde_json::Value,
    ) -> Option<Result<serde_json::Value, String>> {
        let device_id = self.host_device_for_project_path(project_path)?;
        Some(
            self.remote_controllers
                .controller_for(&device_id)
                .and_then(|controller| controller.git_read(op, project_path, args)),
        )
    }

    /// Git status of a remote-hosted project, mapped from the host's `git.status`
    /// payload.
    pub(crate) fn remote_git_summary(
        &self,
        device_id: &str,
        project_path: &str,
    ) -> crate::git::GitSummary {
        match self
            .remote_controllers
            .controller_for(device_id)
            .and_then(|controller| controller.git_status("", project_path))
        {
            Ok(value) => git_summary_from_payload(&value),
            Err(error) => crate::git::GitSummary {
                is_repository: false,
                error: Some(error),
                ..Default::default()
            },
        }
    }

    /// Worktree summary of a remote-hosted project, mapped from the host's
    /// `worktree.list` payload (`active_git` filled from the routed git status).
    pub(crate) fn remote_worktree_summary(
        &self,
        device_id: &str,
        project_id: &str,
        project_path: &str,
    ) -> crate::worktree::WorktreeSummary {
        let active_git = self.reload_project_git(project_path);
        match self
            .remote_controllers
            .controller_for(device_id)
            .and_then(|controller| controller.worktree_list(project_id, project_path))
        {
            Ok(value) => crate::worktree::WorktreeSummary {
                available: value
                    .get("available")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                selected_worktree_id: value
                    .get("selectedWorktreeId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                worktrees: parse_typed(&value, "worktrees"),
                tasks: parse_typed(&value, "tasks"),
                active_git,
                error: value
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            },
            Err(error) => crate::worktree::WorktreeSummary {
                active_git,
                error: Some(error),
                ..Default::default()
            },
        }
    }

    /// Perform a worktree mutation on the host and return a `WorktreeSnapshot`
    /// built from the refreshed worktree payload. `None` ⇒ local project.
    pub(crate) fn remote_worktree_mutation(
        &self,
        project_path: &str,
        run: impl FnOnce(
            &std::sync::Arc<crate::remote::RemoteController>,
        ) -> Result<serde_json::Value, String>,
    ) -> Option<Result<crate::worktree::WorktreeSnapshot, String>> {
        let device_id = self.host_device_for_project_path(project_path)?;
        let controller = match self.remote_controllers.controller_for(&device_id) {
            Ok(controller) => controller,
            Err(error) => return Some(Err(error)),
        };
        Some(run(&controller).map(|value| worktree_snapshot_from_payload(&value)))
    }

    /// The device hosting the project at `project_path`, if it is a remote
    /// project. Used to route a project's domains over the controller.
    pub(crate) fn host_device_for_project_path(&self, project_path: &str) -> Option<String> {
        crate::project_store::ProjectStore::new(self.support_dir.clone())
            .projects_snapshot()
            .into_iter()
            .find(|project| project.path == project_path)
            .and_then(|project| project.host_device_id)
    }

    /// The `(device_id, project_path)` of the remote project with `project_id`,
    /// if it is remote-hosted. Memory methods key on project id; the host needs
    /// the path to resolve its own project (its memory store uses host ids).
    pub(crate) fn remote_project_for_id(&self, project_id: &str) -> Option<(String, String)> {
        crate::project_store::ProjectStore::new(self.support_dir.clone())
            .projects_snapshot()
            .into_iter()
            .find(|project| project.id == project_id)
            .and_then(|project| {
                project
                    .host_device_id
                    .map(|device_id| (device_id, project.path))
            })
    }

    /// Run an AI-session op on the host of a remote project (keyed by path).
    /// Returns `None` for a local project (caller falls back to the local
    /// engine). `op`-specific args are merged with `projectPath`.
    pub(crate) fn remote_ai_session(
        &self,
        project_path: &str,
        op: &str,
        mut args: serde_json::Map<String, serde_json::Value>,
    ) -> Option<Result<serde_json::Value, String>> {
        let device_id = self.host_device_for_project_path(project_path)?;
        let controller = match self.remote_controllers.controller_for(&device_id) {
            Ok(controller) => controller,
            Err(error) => return Some(Err(error)),
        };
        args.insert("projectPath".to_string(), project_path.to_string().into());
        Some(controller.ai_session(op, serde_json::Value::Object(args)))
    }

    /// Live AI runtime sessions of a remote-hosted project, read from the
    /// host's existing `ai.stats.currentSessions` payload.
    pub fn remote_ai_current_sessions(
        &self,
        project_path: &str,
        scope_id: &str,
        include_cached: bool,
    ) -> Option<Result<Vec<crate::ai_history::AIHistoryCurrentSessionView>, String>> {
        let device_id = self.host_device_for_project_path(project_path)?;
        let controller = match self.remote_controllers.controller_for(&device_id) {
            Ok(controller) => controller,
            Err(error) => return Some(Err(error)),
        };
        Some(controller.ai_stats(scope_id).map(|payload| {
            crate::ai_history::ai_current_session_views(
                codux_runtime_core::ai_stats::current_sessions_from_payload(&payload),
                include_cached,
            )
        }))
    }

    /// Apply any `ai.stats` the host pushed for a remote-hosted project since the
    /// last tick (live AI runtime updates). Returns the latest current-session
    /// views, or `None` if the project is local or nothing was pushed.
    pub fn drain_remote_ai_current_sessions(
        &self,
        project_path: &str,
        include_cached: bool,
    ) -> Option<Vec<crate::ai_history::AIHistoryCurrentSessionView>> {
        let device_id = self.host_device_for_project_path(project_path)?;
        let controller = self.remote_controllers.controller_for(&device_id).ok()?;
        let payload = controller.drain_pushed_ai_stats().pop()?;
        Some(crate::ai_history::ai_current_session_views(
            codux_runtime_core::ai_stats::current_sessions_from_payload(&payload),
            include_cached,
        ))
    }

    /// Run a memory read on the host of a remote project. Returns `None` for a
    /// local project (caller falls back to the local engine). `op`-specific
    /// args are merged with the resolved `projectId`/`projectPath`.
    pub(crate) fn remote_memory_read(
        &self,
        project_id: &str,
        op: &str,
        mut args: serde_json::Map<String, serde_json::Value>,
    ) -> Option<Result<serde_json::Value, String>> {
        let (device_id, project_path) = self.remote_project_for_id(project_id)?;
        let controller = match self.remote_controllers.controller_for(&device_id) {
            Ok(controller) => controller,
            Err(error) => return Some(Err(error)),
        };
        args.insert("projectId".to_string(), project_id.to_string().into());
        args.insert("projectPath".to_string(), project_path.into());
        Some(controller.memory_read(op, serde_json::Value::Object(args)))
    }

    /// List a directory of a remote-hosted project as the file panel's
    /// `FileEntry`s, mapped from the host's `file.list` payload (capped to 80 to
    /// match the local loader).
    pub(crate) fn remote_project_files(
        &self,
        device_id: &str,
        project_path: &str,
        directory_path: Option<&str>,
    ) -> Result<Vec<FileEntry>, String> {
        // The UI works in project-relative paths; the host lists by absolute.
        let listing_dir = remote_absolute_path(project_path, directory_path);
        let value = self
            .remote_controllers
            .controller_for(device_id)?
            .file_list(Some(&listing_dir), Some("projectFiles"))?;
        Ok(value
            .get("entries")
            .and_then(|entries| entries.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .take(80)
                    .map(|entry| remote_file_entry(project_path, entry))
                    .collect()
            })
            .unwrap_or_default())
    }
}

fn parse_typed<T: serde::de::DeserializeOwned + Default>(
    value: &serde_json::Value,
    key: &str,
) -> T {
    value
        .get(key)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// Map a host `git.status` payload into the desktop's `GitSummary`. Missing
/// fields (e.g. a host that doesn't compute ahead/behind/commits) default.
fn git_summary_from_payload(value: &serde_json::Value) -> crate::git::GitSummary {
    use serde_json::Value;
    crate::git::GitSummary {
        branch: value
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        upstream: value
            .get("upstream")
            .and_then(Value::as_str)
            .map(str::to_string),
        ahead: value.get("ahead").and_then(Value::as_i64).unwrap_or(0),
        behind: value.get("behind").and_then(Value::as_i64).unwrap_or(0),
        head_pushed: value
            .get("headPushed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        staged: value.get("staged").and_then(Value::as_u64).unwrap_or(0) as usize,
        unstaged: value.get("unstaged").and_then(Value::as_u64).unwrap_or(0) as usize,
        untracked: value.get("untracked").and_then(Value::as_u64).unwrap_or(0) as usize,
        is_repository: value
            .get("isRepository")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        error: value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string),
        changed_files: parse_typed(value, "changedFiles"),
        flat_changed_files: parse_typed(value, "flatChangedFiles"),
        branches: parse_typed(value, "branches"),
        remote_branches: parse_typed(value, "remoteBranches"),
        remotes: parse_typed(value, "remotes"),
        commits: parse_typed(value, "commits"),
    }
}

/// Build a `WorktreeSnapshot` from a host worktree payload (worktree.list /
/// worktree.updated shape).
fn worktree_snapshot_from_payload(value: &serde_json::Value) -> crate::worktree::WorktreeSnapshot {
    use serde_json::Value;
    let worktrees = value
        .get("worktrees")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let str_field = |key: &str| {
                        item.get(key)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    };
                    crate::worktree::ProjectWorktreeSnapshot {
                        id: str_field("id"),
                        project_id: str_field("projectId"),
                        name: str_field("name"),
                        branch: str_field("branch"),
                        path: str_field("path"),
                        status: str_field("status"),
                        is_default: item
                            .get("isDefault")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        created_at: 0,
                        updated_at: 0,
                        git_summary: item
                            .get("gitSummary")
                            .cloned()
                            .and_then(|summary| serde_json::from_value(summary).ok())
                            .unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    crate::worktree::WorktreeSnapshot {
        project_id: value
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        selected_worktree_id: value
            .get("selectedWorktreeId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        worktrees,
        tasks: Vec::new(),
        error: value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// Strip the project root from a host-absolute path to a project-relative one.
pub(crate) fn remote_relative_path(project_path: &str, absolute: &str) -> String {
    absolute
        .strip_prefix(project_path.trim_end_matches('/'))
        .unwrap_or(absolute)
        .trim_start_matches('/')
        .to_string()
}

/// Pull a string field out of a git.read `result` payload.
pub(crate) fn remote_git_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Resolve a project-relative path (as the UI uses) to the host's absolute path.
/// An empty/`None` relative path means the project root.
pub(crate) fn remote_absolute_path(project_path: &str, relative: Option<&str>) -> String {
    let root = project_path.trim_end_matches('/');
    match relative.map(str::trim).filter(|value| !value.is_empty()) {
        Some(relative) => format!("{root}/{}", relative.trim_start_matches('/')),
        None => root.to_string(),
    }
}

/// Build the file panel's `FileEntry` from one host `file.list` entry, computing
/// the project-relative path the panel expects.
fn remote_file_entry(project_path: &str, entry: &serde_json::Value) -> FileEntry {
    let path = entry
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let is_directory = entry
        .get("isDirectory")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let relative_path = path
        .strip_prefix(project_path)
        .unwrap_or(path)
        .trim_start_matches('/')
        .to_string();
    FileEntry {
        name: entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        relative_path,
        kind: if is_directory {
            FileKind::Directory
        } else {
            FileKind::File
        },
        size: entry
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    }
}

/// Step-up target for a local volume root as an `Option` for the picker listing:
/// `Some(drive list)` on Windows, `None` on POSIX where `/` is the top.
fn drive_root_parent() -> Option<String> {
    let parent = codux_runtime_core::path::drive_root_parent();
    (!parent.is_empty()).then_some(parent)
}

/// Parse a `file_list_payload` value into the typed listing the picker uses —
/// same shape as the remote controller's `browse_directory`.
fn local_directory_listing_from_payload(
    value: &serde_json::Value,
) -> crate::remote::RemoteDirectoryListing {
    use serde_json::Value;
    crate::remote::RemoteDirectoryListing {
        path: value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        parent: value
            .get("parent")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        entries: value
            .get("entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| crate::remote::RemoteDirectoryEntry {
                        name: entry
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        path: entry
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        is_dir: entry
                            .get("isDirectory")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}
