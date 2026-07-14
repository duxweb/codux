use crate::{git, persistent_cache::PersistentCacheStore};
use std::path::{Path, PathBuf};

const GIT_REVIEW_NAMESPACE: &str = "git-review";
const GIT_WORKSPACE_NAMESPACE: &str = "git-workspace";

pub fn cached_git_workspace(
    support_dir: &Path,
    project_path: &str,
) -> Option<git::GitWorkspaceSnapshot> {
    let snapshot = cache(support_dir)
        .ok()?
        .get_json::<git::GitWorkspaceSnapshot>(GIT_WORKSPACE_NAMESPACE, project_path)
        .ok()
        .flatten()?;
    snapshot.status.is_repository.then_some(snapshot)
}

pub fn save_git_workspace(
    support_dir: &Path,
    project_path: &str,
    snapshot: &git::GitWorkspaceSnapshot,
) {
    if let Ok(cache) = cache(support_dir) {
        let _ = cache.put_json(GIT_WORKSPACE_NAMESPACE, project_path, snapshot);
    }
}

pub fn cached_git_review(
    support_dir: &Path,
    project_path: &str,
    base_branch: Option<&str>,
) -> Option<git::GitReviewSummary> {
    if base_branch.is_none()
        && let Some(snapshot) = cached_git_workspace(support_dir, project_path)
    {
        return Some(snapshot.review);
    }
    let key = git_review_key(project_path, base_branch);
    cache(support_dir)
        .ok()?
        .get_json::<git::GitReviewSummary>(GIT_REVIEW_NAMESPACE, &key)
        .ok()
        .flatten()
}

pub fn save_git_review(
    support_dir: &Path,
    project_path: &str,
    base_branch: Option<&str>,
    review: &git::GitReviewSummary,
) {
    let key = git_review_key(project_path, base_branch);
    if let Ok(cache) = cache(support_dir) {
        let _ = cache.put_json(GIT_REVIEW_NAMESPACE, &key, review);
    }
}

fn git_review_key(project_path: &str, base_branch: Option<&str>) -> String {
    format!("{}\0{}", project_path, base_branch.unwrap_or_default())
}

fn cache(support_dir: &Path) -> Result<std::sync::Arc<PersistentCacheStore>, String> {
    PersistentCacheStore::for_support_dir(PathBuf::from(support_dir))
}
