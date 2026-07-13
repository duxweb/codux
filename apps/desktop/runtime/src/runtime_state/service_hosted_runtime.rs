#[derive(Clone)]
enum HostedProjectRuntime {
    Wsl(Arc<crate::wsl::WslRuntimeClient>),
    Remote(Arc<crate::remote::RemoteController>),
}

impl RuntimeService {
    pub(crate) fn hosted_git_invoke(
        &self,
        project_path: &str,
        op: &str,
        args: Value,
    ) -> Option<Result<crate::git::GitSummary, String>> {
        let runtime = match self.hosted_runtime_for_project_path(project_path)? {
            Ok(runtime) => runtime,
            Err(error) => return Some(Err(error)),
        };
        let project_id = self
            .project_id_for_workspace_path(project_path)
            .unwrap_or_default();
        Some(runtime.git_invoke(&project_id, project_path, op, args))
    }

    pub(crate) fn hosted_git_invoke_blocking(
        &self,
        project_path: &str,
        op: &str,
        args: Value,
    ) -> Option<Result<crate::git::GitSummary, String>> {
        let runtime = match self.hosted_runtime_for_project_path_blocking(project_path)? {
            Ok(runtime) => runtime,
            Err(error) => return Some(Err(error)),
        };
        let project_id = self
            .project_id_for_workspace_path(project_path)
            .unwrap_or_default();
        Some(runtime.git_invoke(&project_id, project_path, op, args))
    }

    pub(crate) fn hosted_git_read(
        &self,
        project_path: &str,
        op: &str,
        args: Value,
    ) -> Option<Result<Value, String>> {
        let runtime = match self.hosted_runtime_for_project_path(project_path)? {
            Ok(runtime) => runtime,
            Err(error) => return Some(Err(error)),
        };
        let project_id = self
            .project_id_for_workspace_path(project_path)
            .unwrap_or_default();
        Some(runtime.git_read(&project_id, project_path, op, args))
    }

    fn hosted_worktree_summary(
        &self,
        runtime: &HostedProjectRuntime,
        project_id: &str,
        project_path: &str,
    ) -> crate::worktree::WorktreeSummary {
        let active_git = self.reload_project_git(project_path);
        let result = runtime.worktree_list(project_id, project_path).and_then(|value| {
            let snapshot = worktree_snapshot_from_payload(&value)?;
            let worktrees = hosted_payload_field(&value, "worktrees")?;
            let base_branches = hosted_payload_field(&value, "baseBranches")?;
            let available = value
                .get("available")
                .and_then(Value::as_bool)
                .ok_or_else(|| "Hosted worktree payload is missing available".to_string())?;
            let default_base_branch = value
                .get("defaultBaseBranch")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "Hosted worktree payload is missing defaultBaseBranch".to_string()
                })?
                .to_string();
            let payload_error = snapshot.error.clone();
            let (tasks, sync_error) = if payload_error.is_none() {
                match self.sync_hosted_project_worktree_snapshot(
                    project_id,
                    &snapshot,
                    Some(&default_base_branch),
                    false,
                ) {
                    Ok(tasks) => (tasks, None),
                    Err(error) => (snapshot.tasks.clone(), Some(error)),
                }
            } else {
                (snapshot.tasks.clone(), None)
            };
            let selected_worktree_id = ProjectStore::new(self.support_dir.clone())
                .list_snapshot()
                .selected_worktree_id_by_project
                .get(project_id)
                .cloned();
            Ok(crate::worktree::WorktreeSummary {
                available,
                selected_worktree_id,
                worktrees,
                tasks: tasks
                    .into_iter()
                    .map(|task| crate::worktree::WorktreeTaskInfo {
                        worktree_id: task.worktree_id,
                        title: task.title,
                        base_branch: task.base_branch,
                        status: task.status,
                    })
                    .collect(),
                active_git: active_git.clone(),
                base_branches,
                default_base_branch,
                error: payload_error.or(sync_error),
            })
        });
        result.unwrap_or_else(|error| crate::worktree::WorktreeSummary {
            active_git,
            error: Some(error),
            ..Default::default()
        })
    }

    fn sync_hosted_project_worktree_snapshot(
        &self,
        project_id: &str,
        snapshot: &crate::worktree::WorktreeSnapshot,
        default_base_branch: Option<&str>,
        prefer_payload_selection: bool,
    ) -> Result<Vec<crate::worktree::WorktreeTaskSnapshot>, String> {
        let store = ProjectStore::new(self.support_dir.clone());
        let existing = store.snapshot();
        let now = chrono::Utc::now().timestamp_millis();
        let preferred_worktree_id = prefer_payload_selection
            .then_some(snapshot.selected_worktree_id.trim())
            .filter(|worktree_id| !worktree_id.is_empty());
        let tasks = hosted_worktree_tasks(snapshot, &existing, default_base_branch, now);
        store.replace_project_worktree_state(
            project_id,
            snapshot
                .worktrees
                .iter()
                .map(|worktree| {
                    let previous = existing
                        .worktrees
                        .iter()
                        .find(|record| record.id == worktree.id);
                    crate::project_store::ProjectWorktreeRecord {
                        id: worktree.id.clone(),
                        project_id: project_id.to_string(),
                        name: worktree.name.clone(),
                        branch: worktree.branch.clone(),
                        path: worktree.path.clone(),
                        status: worktree.status.clone(),
                        is_default: worktree.is_default,
                        created_at: previous.map(|record| record.created_at).unwrap_or(now),
                        updated_at: previous.map(|record| record.updated_at).unwrap_or(now),
                    }
                })
                .collect(),
            tasks
                .iter()
                .map(|task| crate::project_store::WorktreeTaskRecord {
                    worktree_id: task.worktree_id.clone(),
                    title: task.title.clone(),
                    base_branch: task.base_branch.clone(),
                    base_commit: task.base_commit.clone(),
                    status: task.status.clone(),
                    created_at: task.created_at,
                    updated_at: task.updated_at,
                    started_at: task.started_at,
                    completed_at: task.completed_at,
                })
                .collect(),
            preferred_worktree_id,
        )?;
        Ok(tasks)
    }

    pub fn wsl_distributions(&self) -> Result<Vec<crate::wsl::WslDistribution>, String> {
        if !self.reload_settings().wsl_enabled {
            return Ok(Vec::new());
        }
        self.wsl_runtimes.distributions()
    }

    pub fn wsl_distribution_catalog(&self) -> Result<crate::wsl::WslDistributionCatalog, String> {
        self.require_wsl_enabled()?;
        self.wsl_runtimes.catalog()
    }

    pub fn install_wsl_distribution_with_progress(
        &self,
        distribution: &str,
        progress: impl Fn(crate::wsl::WslInstallProgress) + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.require_wsl_enabled()?;
        let version = UpdateService::new(self.support_dir.clone(), PathBuf::new())
            .latest_release_version()?;
        self.wsl_runtimes.install_distribution_with_progress(
            distribution,
            &version,
            progress,
        )
    }

    pub fn install_wsl_runtime_with_progress(
        &self,
        distribution: &str,
        progress: impl Fn(crate::wsl::WslInstallProgress) + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.require_wsl_enabled()?;
        let version = UpdateService::new(self.support_dir.clone(), PathBuf::new())
            .latest_release_version()?;
        self.wsl_runtimes
            .install_runtime_with_progress(distribution, &version, progress)
    }

    fn require_wsl_enabled(&self) -> Result<(), String> {
        self.reload_settings()
            .wsl_enabled
            .then_some(())
            .ok_or_else(|| "WSL integration is disabled in Settings".to_string())
    }

    pub fn browse_runtime_directory(
        &self,
        target: &ProjectRuntimeTarget,
        path: Option<&str>,
        purpose: Option<&str>,
    ) -> Result<crate::remote::RemoteDirectoryListing, String> {
        let Some(runtime) = self.hosted_runtime_for_target_blocking(target)? else {
            return self.browse_local_directory(path, purpose);
        };
        let value = runtime.file_list(path.unwrap_or_default(), purpose)?;
        Ok(local_directory_listing_from_payload(&value))
    }

    pub fn create_runtime_directory(
        &self,
        target: &ProjectRuntimeTarget,
        path: &str,
    ) -> Result<(), String> {
        match self.hosted_runtime_for_target_blocking(target)? {
            Some(runtime) => runtime.create_directory(path),
            None => self.create_local_directory(path),
        }
    }

    pub fn delete_runtime_path(
        &self,
        target: &ProjectRuntimeTarget,
        path: &str,
    ) -> Result<(), String> {
        match self.hosted_runtime_for_target_blocking(target)? {
            Some(runtime) => runtime.delete_path(path),
            None => self.delete_local_path(path),
        }
    }

    pub fn rename_runtime_path(
        &self,
        target: &ProjectRuntimeTarget,
        path: &str,
        new_path: &str,
    ) -> Result<(), String> {
        match self.hosted_runtime_for_target_blocking(target)? {
            Some(runtime) => runtime.rename_path(path, new_path),
            None => self.rename_local_path(path, new_path),
        }
    }

    pub fn save_file_as_runtime(
        &self,
        source_target: &ProjectRuntimeTarget,
        source_abs: &str,
        destination_target: &ProjectRuntimeTarget,
        destination_abs: &str,
    ) -> Result<(), String> {
        if source_target == destination_target {
            if let Some(runtime) = self.hosted_runtime_for_target_blocking(source_target)? {
                let destination_directory =
                    crate::path::parent_path(destination_abs).unwrap_or_default();
                let copied = runtime.copy_path(source_abs, &destination_directory)?;
                if copied != destination_abs {
                    runtime.rename_path(&copied, destination_abs)?;
                }
                return Ok(());
            }
            if let Some(parent) = std::path::Path::new(destination_abs).parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            return std::fs::copy(source_abs, destination_abs)
                .map(|_| ())
                .map_err(|error| error.to_string());
        }
        let bytes = match self.hosted_runtime_for_target_blocking(source_target)? {
            Some(runtime) => runtime.read_file_bytes(source_abs)?,
            None => std::fs::read(source_abs).map_err(|error| error.to_string())?,
        };
        match self.hosted_runtime_for_target_blocking(destination_target)? {
            Some(runtime) => {
                let directory = crate::path::parent_path(destination_abs).unwrap_or_default();
                let name = crate::path::file_name(destination_abs)
                    .ok_or_else(|| "Destination file name is invalid".to_string())?;
                runtime.write_bytes(&directory, &name, &bytes).map(|_| ())
            }
            None => {
                if let Some(parent) = std::path::Path::new(destination_abs).parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                std::fs::write(destination_abs, bytes).map_err(|error| error.to_string())
            }
        }
    }

    pub fn terminal_controller_for_target_blocking(
        &self,
        target: &ProjectRuntimeTarget,
    ) -> Result<Option<Arc<dyn crate::runtime_terminal::RuntimeTerminalController>>, String> {
        match target {
            ProjectRuntimeTarget::Local => Ok(None),
            ProjectRuntimeTarget::Wsl { distribution } => {
                self.require_wsl_enabled()?;
                self.wsl_runtimes.client_for(distribution).map(|client| {
                    Some(
                        client
                            as Arc<dyn crate::runtime_terminal::RuntimeTerminalController>,
                    )
                })
            }
            ProjectRuntimeTarget::Remote { device_id } => self
                .remote_controller_for_device_blocking(device_id)
                .map(|controller| {
                    Some(
                        controller
                            as Arc<dyn crate::runtime_terminal::RuntimeTerminalController>,
                    )
                }),
        }
    }

    pub fn terminal_controller_for_target(
        &self,
        target: &ProjectRuntimeTarget,
    ) -> Result<Option<Arc<dyn crate::runtime_terminal::RuntimeTerminalController>>, String> {
        match target {
            ProjectRuntimeTarget::Local => Ok(None),
            ProjectRuntimeTarget::Wsl { distribution } => {
                self.require_wsl_enabled()?;
                Ok(self.wsl_runtimes.current_client(distribution).map(|client| {
                    client as Arc<dyn crate::runtime_terminal::RuntimeTerminalController>
                }))
            }
            ProjectRuntimeTarget::Remote { device_id } => self
                .remote_controllers
                .controller_for(device_id)
                .map(|controller| {
                    Some(
                        controller
                            as Arc<dyn crate::runtime_terminal::RuntimeTerminalController>,
                    )
                }),
        }
    }

    fn hosted_runtime_for_project_path(
        &self,
        project_path: &str,
    ) -> Option<Result<HostedProjectRuntime, String>> {
        let target = match ProjectStore::new(self.support_dir.clone())
            .runtime_target_for_workspace_path(project_path)
        {
            Ok(target) => target,
            Err(error) => return Some(Err(error)),
        };
        match target {
            ProjectRuntimeTarget::Local => None,
            ProjectRuntimeTarget::Wsl { distribution } => Some(self.require_wsl_enabled().and_then(
                |_| self.wsl_runtimes
                    .client_for(&distribution)
                    .map(HostedProjectRuntime::Wsl),
            )),
            ProjectRuntimeTarget::Remote { device_id } => Some(
                self.remote_controllers
                    .controller_for(&device_id)
                    .map(HostedProjectRuntime::Remote),
            ),
        }
    }

    fn hosted_runtime_for_project_path_blocking(
        &self,
        project_path: &str,
    ) -> Option<Result<HostedProjectRuntime, String>> {
        let target = match ProjectStore::new(self.support_dir.clone())
            .runtime_target_for_workspace_path(project_path)
        {
            Ok(target) => target,
            Err(error) => return Some(Err(error)),
        };
        match target {
            ProjectRuntimeTarget::Local => None,
            ProjectRuntimeTarget::Wsl { distribution } => Some(self.require_wsl_enabled().and_then(
                |_| self.wsl_runtimes
                    .client_for(&distribution)
                    .map(HostedProjectRuntime::Wsl),
            )),
            ProjectRuntimeTarget::Remote { device_id } => Some(
                self.remote_controllers
                    .controller_for_blocking(&device_id, REMOTE_CONNECT_TIMEOUT)
                    .map(HostedProjectRuntime::Remote),
            ),
        }
    }

    fn hosted_runtime_for_target_blocking(
        &self,
        target: &ProjectRuntimeTarget,
    ) -> Result<Option<HostedProjectRuntime>, String> {
        match target {
            ProjectRuntimeTarget::Local => Ok(None),
            ProjectRuntimeTarget::Wsl { distribution } => {
                self.require_wsl_enabled()?;
                self.wsl_runtimes
                    .client_for(distribution)
                    .map(HostedProjectRuntime::Wsl)
                    .map(Some)
            }
            ProjectRuntimeTarget::Remote { device_id } => self
                .remote_controllers
                .controller_for_blocking(device_id, REMOTE_CONNECT_TIMEOUT)
                .map(HostedProjectRuntime::Remote)
                .map(Some),
        }
    }

    fn project_id_for_workspace_path(&self, workspace_path: &str) -> Option<String> {
        let snapshot = ProjectStore::new(self.support_dir.clone()).snapshot();
        let selected = snapshot.selected_project_id.as_deref();
        snapshot
            .projects
            .iter()
            .find(|project| {
                selected == Some(project.id.as_str())
                    && project_contains_workspace(&snapshot, project, workspace_path)
            })
            .or_else(|| {
                snapshot
                    .projects
                    .iter()
                    .find(|project| project_contains_workspace(&snapshot, project, workspace_path))
            })
            .map(|project| project.id.clone())
    }

    fn hosted_project_files(
        &self,
        runtime: &HostedProjectRuntime,
        project_path: &str,
        directory_path: Option<&str>,
    ) -> Result<Vec<FileEntry>, String> {
        let listing_dir = hosted_absolute_path(project_path, directory_path);
        let value = runtime.file_list(&listing_dir, Some("projectFiles"))?;
        Ok(value
            .get("entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .take(80)
                    .map(|entry| hosted_file_entry(project_path, entry))
                    .collect()
            })
            .unwrap_or_default())
    }
}

fn project_contains_workspace(
    snapshot: &crate::project_store::AppSnapshot,
    project: &crate::project_store::ProjectRecord,
    workspace_path: &str,
) -> bool {
    project.path == workspace_path
        || snapshot.worktrees.iter().any(|worktree| {
            worktree.project_id == project.id && worktree.path == workspace_path
        })
}

impl HostedProjectRuntime {
    fn file_list(&self, path: &str, purpose: Option<&str>) -> Result<Value, String> {
        match self {
            Self::Wsl(client) => client.request(
                "file.list",
                json!({ "path": path, "purpose": purpose }),
            ),
            Self::Remote(controller) => controller.file_list(Some(path), purpose),
        }
    }

    fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        match self {
            Self::Wsl(client) => client
                .request("file.write", json!({ "path": path, "content": content }))
                .map(|_| ()),
            Self::Remote(controller) => controller.write_file(path, content).map(|_| ()),
        }
    }

    fn create_directory(&self, path: &str) -> Result<(), String> {
        match self {
            Self::Wsl(client) => client
                .request("file.mkdir", json!({ "path": path }))
                .map(|_| ()),
            Self::Remote(controller) => controller.create_directory(path).map(|_| ()),
        }
    }

    fn delete_path(&self, path: &str) -> Result<(), String> {
        match self {
            Self::Wsl(client) => client
                .request("file.delete", json!({ "path": path }))
                .map(|_| ()),
            Self::Remote(controller) => controller.delete_path(path).map(|_| ()),
        }
    }

    fn rename_path(&self, path: &str, new_path: &str) -> Result<(), String> {
        match self {
            Self::Wsl(client) => client
                .request(
                    "file.rename",
                    json!({ "path": path, "newPath": new_path }),
                )
                .map(|_| ()),
            Self::Remote(controller) => controller.rename_path(path, new_path).map(|_| ()),
        }
    }

    fn copy_path(&self, path: &str, target_dir: &str) -> Result<String, String> {
        match self {
            Self::Wsl(client) => client
                .request(
                    "file.copy",
                    json!({ "path": path, "targetDir": target_dir }),
                )
                .and_then(result_path),
            Self::Remote(controller) => controller.copy_path(path, target_dir),
        }
    }

    fn move_path(&self, path: &str, target_dir: &str, overwrite: bool) -> Result<String, String> {
        match self {
            Self::Wsl(client) => client
                .request(
                    "file.move",
                    json!({ "path": path, "targetDir": target_dir, "overwrite": overwrite }),
                )
                .and_then(result_path),
            Self::Remote(controller) => controller.move_path(path, target_dir, overwrite),
        }
    }

    fn read_file(&self, path: &str) -> Result<Value, String> {
        match self {
            Self::Wsl(client) => client.request("file.read", json!({ "path": path })),
            Self::Remote(controller) => controller.read_file(path),
        }
    }

    fn read_file_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        match self {
            Self::Wsl(client) => client
                .request("file.readBytes", json!({ "path": path }))
                .and_then(|value| decode_bytes(&value)),
            Self::Remote(controller) => controller.read_file_bytes(path),
        }
    }

    fn write_bytes(&self, directory: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
        match self {
            Self::Wsl(client) => {
                use base64::Engine;
                client
                    .request(
                        "file.writeBytes",
                        json!({
                            "directory": directory,
                            "name": name,
                            "bytes": base64::engine::general_purpose::STANDARD.encode(bytes),
                        }),
                    )
                    .and_then(result_path)
            }
            Self::Remote(controller) => controller.write_bytes(directory, name, bytes),
        }
    }

    fn git_status(&self, project_id: &str, project_path: &str) -> Result<Value, String> {
        match self {
            Self::Wsl(client) => {
                client.request("git.status", json!({ "projectPath": project_path }))
            }
            Self::Remote(controller) => controller.git_status(project_id, project_path),
        }
    }

    fn git_invoke(
        &self,
        project_id: &str,
        project_path: &str,
        op: &str,
        args: Value,
    ) -> Result<crate::git::GitSummary, String> {
        let value = match self {
            Self::Wsl(client) => client.request(
                "git.invoke",
                json!({ "projectPath": project_path, "op": op, "args": args }),
            ),
            Self::Remote(controller) => {
                controller.git_invoke(project_id, op, project_path, args)
            }
        }?;
        git_summary_from_payload(&value)
    }

    fn git_read(
        &self,
        project_id: &str,
        project_path: &str,
        op: &str,
        args: Value,
    ) -> Result<Value, String> {
        match self {
            Self::Wsl(client) => client.request(
                "git.read",
                json!({ "projectPath": project_path, "op": op, "args": args }),
            ),
            Self::Remote(controller) => controller.git_read(project_id, op, project_path, args),
        }
    }

    fn worktree_list(&self, project_id: &str, project_path: &str) -> Result<Value, String> {
        match self {
            Self::Wsl(client) => client.request(
                "worktree.list",
                json!({ "projectId": project_id, "projectPath": project_path }),
            ),
            Self::Remote(controller) => controller.worktree_list(project_id, project_path),
        }
    }

    fn worktree_create(
        &self,
        project_id: &str,
        project_path: &str,
        branch_name: &str,
        base_branch: Option<&str>,
        task_title: Option<&str>,
    ) -> Result<Value, String> {
        match self {
            Self::Wsl(client) => client.request(
                "worktree.create",
                json!({
                    "projectId": project_id,
                    "projectPath": project_path,
                    "branchName": branch_name,
                    "baseBranch": base_branch,
                    "taskTitle": task_title,
                }),
            ),
            Self::Remote(controller) => controller.worktree_create(
                project_id,
                project_path,
                branch_name,
                base_branch,
                task_title,
            ),
        }
    }

    fn worktree_remove(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_path: &str,
        remove_branch: bool,
    ) -> Result<Value, String> {
        match self {
            Self::Wsl(client) => client.request(
                "worktree.remove",
                json!({
                    "projectId": project_id,
                    "projectPath": project_path,
                    "worktreePath": worktree_path,
                    "removeBranch": remove_branch,
                }),
            ),
            Self::Remote(controller) => controller.worktree_remove(
                project_id,
                project_path,
                worktree_path,
                remove_branch,
            ),
        }
    }

    fn worktree_merge(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_path: &str,
        base_branch: Option<&str>,
        remove_branch: bool,
    ) -> Result<Value, String> {
        match self {
            Self::Wsl(client) => client.request(
                "worktree.merge",
                json!({
                    "projectId": project_id,
                    "projectPath": project_path,
                    "worktreePath": worktree_path,
                    "baseBranch": base_branch,
                    "removeBranch": remove_branch,
                }),
            ),
            Self::Remote(controller) => controller.worktree_merge(
                project_id,
                project_path,
                worktree_path,
                base_branch,
                remove_branch,
            ),
        }
    }
}

fn result_path(value: Value) -> Result<String, String> {
    value
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Hosted runtime did not return a path".to_string())
}

fn decode_bytes(value: &Value) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let encoded = value
        .get("bytes")
        .and_then(Value::as_str)
        .ok_or_else(|| "Hosted runtime did not return file bytes".to_string())?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| error.to_string())
}

fn hosted_relative_path(project_path: &str, absolute: &str) -> String {
    let relative = crate::path::relative_path(project_path, absolute)
        .unwrap_or_else(|| absolute.to_string());
    if crate::path::is_windows_path(project_path) {
        relative.replace('\\', "/")
    } else {
        relative
    }
}

fn hosted_git_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn hosted_absolute_path(project_path: &str, relative: Option<&str>) -> String {
    match relative.map(str::trim).filter(|value| !value.is_empty()) {
        Some(relative) => crate::path::join_path(
            project_path,
            relative.trim_start_matches(['/', '\\']),
        ),
        None => project_path.to_string(),
    }
}

fn hosted_file_entry(project_path: &str, entry: &Value) -> FileEntry {
    let path = entry.get("path").and_then(Value::as_str).unwrap_or_default();
    let is_directory = entry
        .get("isDirectory")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let relative_path = hosted_relative_path(project_path, path);
    FileEntry {
        name: entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        relative_path,
        kind: if is_directory {
            FileKind::Directory
        } else {
            FileKind::File
        },
        size: entry.get("size").and_then(Value::as_u64).unwrap_or(0),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostedWorktreeSnapshotPayload {
    project_id: String,
    selected_worktree_id: Option<String>,
    worktrees: Vec<crate::worktree::ProjectWorktreeSnapshot>,
    tasks: Vec<crate::worktree::WorktreeTaskSnapshot>,
    error: Option<String>,
}

fn git_summary_from_payload(value: &Value) -> Result<crate::git::GitSummary, String> {
    serde_json::from_value(value.clone())
        .map_err(|error| format!("Invalid hosted Git payload: {error}"))
}

fn worktree_snapshot_from_payload(
    value: &Value,
) -> Result<crate::worktree::WorktreeSnapshot, String> {
    let payload = serde_json::from_value::<HostedWorktreeSnapshotPayload>(value.clone())
        .map_err(|error| format!("Invalid hosted worktree payload: {error}"))?;
    Ok(crate::worktree::WorktreeSnapshot {
        project_id: payload.project_id,
        selected_worktree_id: payload.selected_worktree_id.unwrap_or_default(),
        worktrees: payload.worktrees,
        tasks: payload.tasks,
        error: payload.error,
    })
}

fn add_created_worktree_task(
    snapshot: &mut crate::worktree::WorktreeSnapshot,
    payload: &Value,
    task_title: Option<&str>,
    base_branch: Option<&str>,
) {
    let worktree_id = snapshot.selected_worktree_id.trim();
    if worktree_id.is_empty()
        || worktree_id == snapshot.project_id
        || snapshot
            .tasks
            .iter()
            .any(|task| task.worktree_id == worktree_id)
    {
        return;
    }
    let Some(worktree) = snapshot
        .worktrees
        .iter()
        .find(|worktree| worktree.id == worktree_id && !worktree.is_default)
    else {
        return;
    };
    let title = task_title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&worktree.name)
        .to_string();
    let base_branch = base_branch
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            payload
                .get("defaultBaseBranch")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default()
        .to_string();
    let now = chrono::Utc::now().timestamp_millis();
    snapshot.tasks.push(crate::worktree::WorktreeTaskSnapshot {
        worktree_id: worktree.id.clone(),
        title,
        base_branch,
        base_commit: None,
        status: "todo".to_string(),
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
    });
}

fn hosted_worktree_tasks(
    snapshot: &crate::worktree::WorktreeSnapshot,
    existing: &crate::project_store::AppSnapshot,
    default_base_branch: Option<&str>,
    now: i64,
) -> Vec<crate::worktree::WorktreeTaskSnapshot> {
    let valid_worktree_ids = snapshot
        .worktrees
        .iter()
        .map(|worktree| worktree.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let incoming_task_ids = snapshot
        .tasks
        .iter()
        .filter(|task| valid_worktree_ids.contains(task.worktree_id.as_str()))
        .map(|task| task.worktree_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut tasks = snapshot
        .tasks
        .iter()
        .filter(|task| valid_worktree_ids.contains(task.worktree_id.as_str()))
        .map(|task| {
            let previous = existing
                .worktree_tasks
                .iter()
                .find(|record| record.worktree_id == task.worktree_id);
            crate::worktree::WorktreeTaskSnapshot {
                worktree_id: task.worktree_id.clone(),
                title: task.title.clone(),
                base_branch: task.base_branch.clone(),
                base_commit: task.base_commit.clone(),
                status: task.status.clone(),
                created_at: previous
                    .map(|record| record.created_at)
                    .or_else(|| (task.created_at > 0).then_some(task.created_at))
                    .unwrap_or(now),
                updated_at: (task.updated_at > 0)
                    .then_some(task.updated_at)
                    .or_else(|| previous.map(|record| record.updated_at))
                    .unwrap_or(now),
                started_at: task.started_at,
                completed_at: task.completed_at,
            }
        })
        .collect::<Vec<_>>();
    tasks.extend(
        existing
            .worktree_tasks
            .iter()
            .filter(|task| {
                valid_worktree_ids.contains(task.worktree_id.as_str())
                    && !incoming_task_ids.contains(task.worktree_id.as_str())
            })
            .map(|task| crate::worktree::WorktreeTaskSnapshot {
                worktree_id: task.worktree_id.clone(),
                title: task.title.clone(),
                base_branch: task.base_branch.clone(),
                base_commit: task.base_commit.clone(),
                status: task.status.clone(),
                created_at: task.created_at,
                updated_at: task.updated_at,
                started_at: task.started_at,
                completed_at: task.completed_at,
            }),
    );
    let task_ids = tasks
        .iter()
        .map(|task| task.worktree_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let default_base_branch = default_base_branch
        .map(str::trim)
        .filter(|branch| !branch.is_empty());
    tasks.extend(
        snapshot
            .worktrees
            .iter()
            .filter(|worktree| !worktree.is_default && !task_ids.contains(worktree.id.as_str()))
            .map(|worktree| crate::worktree::WorktreeTaskSnapshot {
                worktree_id: worktree.id.clone(),
                title: worktree.name.clone(),
                base_branch: default_base_branch.unwrap_or_default().to_string(),
                base_commit: None,
                status: "todo".to_string(),
                created_at: if worktree.created_at > 0 {
                    worktree.created_at
                } else {
                    now
                },
                updated_at: if worktree.updated_at > 0 {
                    worktree.updated_at
                } else {
                    now
                },
                started_at: None,
                completed_at: None,
            }),
    );
    tasks
}

fn hosted_payload_field<T: serde::de::DeserializeOwned>(
    value: &Value,
    key: &str,
) -> Result<T, String> {
    let field = value
        .get(key)
        .cloned()
        .ok_or_else(|| format!("Hosted payload is missing {key}"))?;
    serde_json::from_value(field)
        .map_err(|error| format!("Invalid hosted payload field {key}: {error}"))
}

#[cfg(test)]
mod hosted_runtime_payload_tests {
    use super::*;

    #[test]
    fn disabled_wsl_integration_rejects_runtime_access_before_platform_launch() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-wsl-disabled-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&support_dir).expect("create support directory");
        std::fs::write(
            support_dir.join("settings.json"),
            serde_json::to_vec(&json!({ "wslEnabled": false })).expect("settings json"),
        )
        .expect("write settings");
        let service = RuntimeService::new(support_dir.clone());

        assert!(service.wsl_distributions().unwrap().is_empty());
        let error = service
            .terminal_controller_for_target_blocking(&ProjectRuntimeTarget::Wsl {
                distribution: "Ubuntu".to_string(),
            })
            .err()
            .expect("disabled WSL should reject terminal access");
        assert_eq!(error, "WSL integration is disabled in Settings");

        std::fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn parses_complete_hosted_git_payload() {
        let payload = json!({
            "branch": "main",
            "upstream": "origin/main",
            "ahead": 1,
            "behind": 2,
            "headPushed": true,
            "staged": 3,
            "unstaged": 4,
            "untracked": 5,
            "isRepository": true,
            "error": null,
            "changedFiles": [],
            "branches": [{ "name": "main", "isCurrent": true }],
            "remoteBranches": ["origin/main"],
            "remotes": [{ "name": "origin", "url": "git@example.test:repo.git" }],
            "commits": [],
            "stashes": [],
            "tags": ["v1"]
        });

        let summary = git_summary_from_payload(&payload).unwrap();
        assert_eq!(summary.branch, "main");
        assert_eq!(summary.ahead, 1);
        assert_eq!(summary.branches[0].name, "main");
        assert_eq!(summary.tags, ["v1"]);
    }

    #[test]
    fn rejects_malformed_hosted_git_payload() {
        let error = git_summary_from_payload(&json!({ "branch": 42 })).unwrap_err();
        assert!(error.starts_with("Invalid hosted Git payload:"));
    }

    #[test]
    fn parses_complete_hosted_worktree_payload() {
        let payload = json!({
            "projectId": "project-1",
            "selectedWorktreeId": "worktree-1",
            "worktrees": [{
                "id": "worktree-1",
                "projectId": "project-1",
                "name": "Feature",
                "branch": "feature",
                "path": "/workspace/feature",
                "status": "active",
                "isDefault": false
            }],
            "tasks": [{
                "worktreeId": "worktree-1",
                "title": "Feature",
                "baseBranch": "main",
                "status": "active"
            }],
            "error": null
        });

        let snapshot = worktree_snapshot_from_payload(&payload).unwrap();
        assert_eq!(snapshot.project_id, "project-1");
        assert_eq!(snapshot.selected_worktree_id, "worktree-1");
        assert_eq!(snapshot.worktrees[0].branch, "feature");
        assert_eq!(snapshot.tasks[0].base_branch, "main");
    }

    #[test]
    fn rejects_malformed_hosted_worktree_payload() {
        let error = worktree_snapshot_from_payload(&json!({
            "projectId": "project-1",
            "worktrees": "invalid",
            "tasks": []
        }))
        .unwrap_err();
        assert!(error.starts_with("Invalid hosted worktree payload:"));
    }

    #[test]
    fn hosted_tasks_preserve_existing_metadata_for_live_worktrees() {
        let snapshot = crate::worktree::WorktreeSnapshot {
            project_id: "project-1".to_string(),
            selected_worktree_id: "worktree-1".to_string(),
            worktrees: vec![crate::worktree::ProjectWorktreeSnapshot {
                id: "worktree-1".to_string(),
                project_id: "project-1".to_string(),
                name: "Feature".to_string(),
                branch: "feature".to_string(),
                path: "/workspace/feature".to_string(),
                status: "active".to_string(),
                is_default: false,
                created_at: 1,
                updated_at: 2,
                git_summary: Default::default(),
            }],
            tasks: Vec::new(),
            error: None,
        };
        let existing = crate::project_store::AppSnapshot {
            worktree_tasks: vec![
                crate::project_store::WorktreeTaskRecord {
                    worktree_id: "worktree-1".to_string(),
                    title: "Saved title".to_string(),
                    base_branch: "release".to_string(),
                    base_commit: Some("abc123".to_string()),
                    status: "running".to_string(),
                    created_at: 3,
                    updated_at: 4,
                    started_at: Some(5),
                    completed_at: None,
                },
                crate::project_store::WorktreeTaskRecord {
                    worktree_id: "removed-worktree".to_string(),
                    title: "Removed".to_string(),
                    base_branch: "main".to_string(),
                    base_commit: None,
                    status: "todo".to_string(),
                    created_at: 3,
                    updated_at: 4,
                    started_at: None,
                    completed_at: None,
                },
            ],
            ..Default::default()
        };

        let tasks = hosted_worktree_tasks(&snapshot, &existing, Some("main"), 10);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Saved title");
        assert_eq!(tasks[0].base_branch, "release");
        assert_eq!(tasks[0].base_commit.as_deref(), Some("abc123"));
        assert_eq!(tasks[0].status, "running");
    }

    #[test]
    fn hosted_tasks_seed_unseen_non_default_worktrees() {
        let snapshot = crate::worktree::WorktreeSnapshot {
            project_id: "project-1".to_string(),
            selected_worktree_id: "worktree-1".to_string(),
            worktrees: vec![crate::worktree::ProjectWorktreeSnapshot {
                id: "worktree-1".to_string(),
                project_id: "project-1".to_string(),
                name: "Feature".to_string(),
                branch: "feature".to_string(),
                path: "/workspace/feature".to_string(),
                status: "active".to_string(),
                is_default: false,
                created_at: 1,
                updated_at: 2,
                git_summary: Default::default(),
            }],
            tasks: Vec::new(),
            error: None,
        };

        let tasks = hosted_worktree_tasks(
            &snapshot,
            &crate::project_store::AppSnapshot::default(),
            Some("main"),
            10,
        );

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].worktree_id, "worktree-1");
        assert_eq!(tasks[0].title, "Feature");
        assert_eq!(tasks[0].base_branch, "main");
        assert_eq!(tasks[0].status, "todo");
    }

    #[test]
    fn hosted_paths_follow_the_runtime_platform() {
        assert_eq!(
            hosted_absolute_path(r"C:\workspace\codux", Some("src/main.rs")),
            r"C:\workspace\codux\src\main.rs"
        );
        assert_eq!(
            hosted_relative_path(
                r"C:\workspace\codux",
                r"C:\workspace\codux\src\main.rs"
            ),
            "src/main.rs"
        );
        assert_eq!(
            hosted_absolute_path("/workspace/codux", Some("src/main.rs")),
            "/workspace/codux/src/main.rs"
        );
        assert_eq!(
            hosted_relative_path(
                "/workspace/codux",
                "/workspace/codux/notes\\draft.txt"
            ),
            "notes\\draft.txt"
        );
    }
}
