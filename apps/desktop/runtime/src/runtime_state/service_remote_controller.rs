// Desktop-as-controller domain: pair with remote hosts and drive their domains
// over the controller transport. Browsing/creating directories on a host backs
// the add-project remote flow; routing a hosted project's other domains builds
// on `controller_for`.

impl RuntimeService {
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

    /// Drop a paired host and any live connection to it.
    pub fn forget_remote_host(
        &self,
        device_id: &str,
    ) -> Result<Vec<crate::remote::SavedRemoteHost>, String> {
        self.remote_controllers.forget(device_id)
    }

    /// List a directory on a remote host (for the add-project remote browser),
    /// parsed into a typed listing so the UI never touches the wire JSON.
    pub fn remote_browse_directory(
        &self,
        device_id: &str,
        path: Option<&str>,
    ) -> Result<crate::remote::RemoteDirectoryListing, String> {
        self.remote_controllers
            .controller_for(device_id)?
            .browse_directory(path)
    }

    /// Create a directory on a remote host (for the add-project remote flow).
    pub fn remote_create_directory(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<serde_json::Value, String> {
        self.remote_controllers
            .controller_for(device_id)?
            .create_directory(path)
    }

    /// Fetch a remote host's identity/capabilities (also a reachability check).
    pub fn remote_host_info(&self, device_id: &str) -> Result<serde_json::Value, String> {
        self.remote_controllers.controller_for(device_id)?.host_info()
    }

    /// The live controller for a device (used by the terminal UI to drive a
    /// remote-hosted project's terminals).
    pub fn remote_controller_for_device(
        &self,
        device_id: &str,
    ) -> Result<std::sync::Arc<crate::remote::RemoteController>, String> {
        self.remote_controllers.controller_for(device_id)
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
                available: value.get("available").and_then(serde_json::Value::as_bool).unwrap_or(true),
                selected_worktree_id: value
                    .get("selectedWorktreeId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                worktrees: parse_typed(&value, "worktrees"),
                tasks: parse_typed(&value, "tasks"),
                active_git,
                error: value.get("error").and_then(serde_json::Value::as_str).map(str::to_string),
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
        error: value.get("error").and_then(Value::as_str).map(str::to_string),
        changed_files: parse_typed(value, "changedFiles"),
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
                        item.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
                    };
                    crate::worktree::ProjectWorktreeSnapshot {
                        id: str_field("id"),
                        project_id: str_field("projectId"),
                        name: str_field("name"),
                        branch: str_field("branch"),
                        path: str_field("path"),
                        status: str_field("status"),
                        is_default: item.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
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
        project_id: value.get("projectId").and_then(Value::as_str).unwrap_or_default().to_string(),
        selected_worktree_id: value
            .get("selectedWorktreeId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        worktrees,
        tasks: Vec::new(),
        error: value.get("error").and_then(Value::as_str).map(str::to_string),
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
