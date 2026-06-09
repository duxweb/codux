impl WorktreeService {
    pub fn create_from_request(
        &self,
        request: WorktreeCreateRequest,
    ) -> Result<WorktreeSnapshot, String> {
        let branch = request.branch_name.trim();
        if branch.is_empty() {
            return Err("Branch name cannot be empty.".to_string());
        }
        let root_path = repository_root(&request.project_path)
            .ok_or_else(|| "Not a Git repository.".to_string())?;
        if !has_head_commit(&root_path) {
            return Err("当前仓库还没有任何提交。请先创建初始提交后再创建 Worktree。".to_string());
        }
        let destination = managed_worktree_path(&root_path, branch);
        if destination.exists() {
            return Err(format!(
                "Worktree path already exists: {}",
                destination.display()
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let base = request
            .base_branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| current_branch(&root_path));
        create_worktree_with_git2(&root_path, branch, &destination, base.as_deref())?;

        let created_path = normalize_path(&destination.display().to_string());
        let created_id = worktree_uuid(&request.project_id, &created_path);
        self.sync_from_git(&request.project_id, &request.project_path)?;
        if let Some(task_title) = request.task_title.as_deref().and_then(normalized_string) {
            self.update_task_title(&created_id, &task_title)?;
        }
        self.select_worktree(&request.project_id, &created_id)?;
        Ok(self.snapshot(request.project_id, request.project_path))
    }

    pub fn remove_from_request(
        &self,
        request: WorktreeRemoveRequest,
    ) -> Result<WorktreeSnapshot, String> {
        let root_path = repository_root(&request.project_path)
            .ok_or_else(|| "Not a Git repository.".to_string())?;
        let branch_to_delete = if request.remove_branch {
            removable_worktree_branch(&root_path, &request.worktree_path)
        } else {
            None
        };
        remove_worktree_with_git2(&root_path, &request.worktree_path)?;
        if let Some(branch) = branch_to_delete.as_deref() {
            delete_local_branch(&root_path, branch)?;
        }
        self.sync_from_git(&request.project_id, &request.project_path)?;
        Ok(self.snapshot(request.project_id, request.project_path))
    }

    pub fn merge_from_request(
        &self,
        request: WorktreeMergeRequest,
    ) -> Result<WorktreeSnapshot, String> {
        let root_path = repository_root(&request.project_path)
            .ok_or_else(|| "Not a Git repository.".to_string())?;
        let branch = current_branch(&request.worktree_path)
            .ok_or_else(|| "Worktree branch cannot be resolved.".to_string())?;
        let base_branch = request
            .base_branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| current_branch(&root_path))
            .ok_or_else(|| "Base branch cannot be resolved.".to_string())?;
        if branch == base_branch {
            return Err("The default worktree cannot be merged into itself.".to_string());
        }
        let repo =
            GitRepository::discover(&root_path).map_err(|error| error.message().to_string())?;
        if current_branch_from_repo(&repo).as_deref() != Some(base_branch.as_str()) {
            checkout_branch_git2(&repo, &base_branch)?;
        }
        merge_branch_git2(&repo, &branch)?;
        if request.remove_branch.unwrap_or(false) {
            remove_worktree_with_git2(&root_path, &request.worktree_path)?;
            delete_local_branch(&root_path, &branch)?;
        }
        self.sync_from_git(&request.project_id, &request.project_path)?;
        Ok(self.snapshot(request.project_id, request.project_path))
    }

    pub fn create_worktree(
        &self,
        project_id: &str,
        project_path: &str,
    ) -> Result<WorktreeSummary, String> {
        let root_path =
            repository_root(project_path).ok_or_else(|| "Not a Git repository.".to_string())?;
        if !has_head_commit(&root_path) {
            return Err("Repository has no commits yet.".to_string());
        }
        let branch = format!("codux-gpui-{}", now_seconds());
        let destination = managed_worktree_path(&root_path, &branch);
        if destination.exists() {
            return Err(format!(
                "Worktree path already exists: {}",
                destination.display()
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let base = current_branch(&root_path);
        create_worktree_with_git2(&root_path, &branch, &destination, base.as_deref())?;

        let destination_text = destination.display().to_string();
        let created_path = normalize_path(&destination_text);
        let created_id = worktree_uuid(project_id, &created_path);
        self.sync_from_git(project_id, project_path)?;
        self.select_worktree(project_id, &created_id)?;
        Ok(self.summary(Some(project_id), Some(project_path)))
    }

    pub fn remove_worktree(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_id: &str,
        remove_branch: bool,
    ) -> Result<WorktreeSummary, String> {
        let summary = self.summary(Some(project_id), Some(project_path));
        let worktree = summary
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .ok_or_else(|| "Worktree not found.".to_string())?;
        if worktree.is_default || worktree.id == project_id {
            return Err("Default worktree cannot be removed.".to_string());
        }
        let root_path =
            repository_root(project_path).ok_or_else(|| "Not a Git repository.".to_string())?;
        let branch_to_delete = if remove_branch {
            removable_worktree_branch(&root_path, &worktree.path)
        } else {
            None
        };
        remove_worktree_with_git2(&root_path, &worktree.path)?;
        if let Some(branch) = branch_to_delete.as_deref() {
            delete_local_branch(&root_path, branch)?;
        }
        self.sync_from_git(project_id, project_path)
    }

    pub fn merge_worktree(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_id: &str,
    ) -> Result<WorktreeSummary, String> {
        let summary = self.summary(Some(project_id), Some(project_path));
        let worktree = summary
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .ok_or_else(|| "Worktree not found.".to_string())?;
        if worktree.is_default || worktree.id == project_id {
            return Err("Default worktree cannot be merged into itself.".to_string());
        }
        let branch = mergeable_branch(current_branch(&worktree.path).as_deref(), &worktree.branch)
            .ok_or_else(|| "Selected worktree branch cannot be resolved.".to_string())?;
        let root_path =
            repository_root(project_path).ok_or_else(|| "Not a Git repository.".to_string())?;
        let base_branch = summary
            .tasks
            .iter()
            .find(|task| task.worktree_id == worktree_id)
            .map(|task| task.base_branch.trim().to_string())
            .filter(|branch| !branch.is_empty())
            .or_else(|| current_branch(&root_path))
            .ok_or_else(|| "Base branch cannot be resolved.".to_string())?;
        if branch == base_branch {
            return Err("Worktree branch is already the base branch.".to_string());
        }

        let repo =
            GitRepository::discover(&root_path).map_err(|error| error.message().to_string())?;
        if current_branch_from_repo(&repo).as_deref() != Some(base_branch.as_str()) {
            checkout_branch_git2(&repo, &base_branch)?;
        }
        merge_branch_git2(&repo, &branch)?;
        self.sync_from_git(project_id, project_path)
    }
}
