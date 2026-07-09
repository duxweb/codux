use crate::{git, persistent_cache::PersistentCacheStore};
use std::path::{Path, PathBuf};

const GIT_SUMMARY_NAMESPACE: &str = "git-summary";
const GIT_REVIEW_NAMESPACE: &str = "git-review";

pub fn cached_git_summary(support_dir: &Path, project_path: &str) -> Option<git::GitSummary> {
    let summary = cache(support_dir)
        .ok()?
        .get_json::<git::GitSummary>(GIT_SUMMARY_NAMESPACE, project_path)
        .ok()
        .flatten()?;
    summary.is_repository.then_some(summary)
}

pub fn save_git_summary(support_dir: &Path, project_path: &str, summary: &git::GitSummary) {
    if let Ok(cache) = cache(support_dir) {
        let _ = cache.put_json(GIT_SUMMARY_NAMESPACE, project_path, summary);
    }
}

pub fn cached_git_review(
    support_dir: &Path,
    project_path: &str,
    base_branch: Option<&str>,
) -> Option<git::GitReviewSummary> {
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
