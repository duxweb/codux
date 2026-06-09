use super::{
    WorktreeCreateRequest, WorktreeService,
    git_ops::{
        current_branch, delete_local_branch, mergeable_branch, removable_worktree_branch,
        worktree_slug, worktree_uuid,
    },
    scan::{ScannedTask, ScannedWorktree, ScannedWorktreeSnapshot},
    state::merge_worktree_snapshot,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn reads_and_selects_project_worktree_without_losing_unknown_fields() {
    let support_dir = temp_dir("worktree-service");
    fs::create_dir_all(&support_dir).unwrap();
    let worktree_dir = support_dir.join("wt");
    fs::create_dir_all(&worktree_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "worktrees": [
                {"id": "w1", "projectId": "p1", "name": "main", "branch": "main", "path": "/tmp/main", "status": "todo", "isDefault": true},
                {"id": "w2", "projectId": "p1", "name": "feature", "branch": "feat", "path": worktree_dir, "status": "todo", "isDefault": false}
            ],
            "worktreeTasks": [
                {"worktreeId": "w2", "title": "Feature task", "baseBranch": "main", "status": "todo"}
            ],
            "selectedWorktreeIdByProject": {"p1": "w1"},
            "unknownTopLevel": "keep"
        }))
        .unwrap(),
    )
    .unwrap();

    let service = WorktreeService::new(support_dir.clone());
    let summary = service.summary(Some("p1"), Some("/tmp/main"));
    assert!(summary.available);
    assert_eq!(summary.worktrees.len(), 2);
    assert_eq!(summary.tasks.len(), 1);
    assert_eq!(summary.selected_worktree_id.as_deref(), Some("w1"));

    service.select_worktree("p1", "w2").unwrap();
    let raw = Value::Object(crate::config::raw_state_snapshot(
        &support_dir.join("state.json"),
    ));
    assert_eq!(raw["selectedWorktreeIdByProject"]["p1"], "w2");
    assert_eq!(raw["unknownTopLevel"], "keep");
}

#[test]
fn state_summary_falls_back_to_project_worktree_when_state_is_malformed() {
    let support_dir = temp_dir("worktree-state-fallback");
    let project_dir = temp_dir("worktree-state-fallback-project");
    fs::create_dir_all(&support_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(support_dir.join("state.json"), "{ bad json").unwrap();

    let summary = WorktreeService::new(support_dir.clone()).state_summary(
        Some("project"),
        Some(project_dir.to_str().expect("project path")),
    );

    assert!(summary.available);
    assert_eq!(summary.selected_worktree_id.as_deref(), Some("project"));
    assert_eq!(summary.worktrees.len(), 1);
    assert_eq!(summary.worktrees[0].id, "project");
    assert!(summary.worktrees[0].is_default);
    assert!(summary.error.is_none());

    fs::remove_dir_all(support_dir).ok();
    fs::remove_dir_all(project_dir).ok();
}

#[test]
fn state_summary_reads_active_git_from_runtime_state() {
    let support_dir = temp_dir("worktree-state-active-git");
    let project_dir = temp_dir("worktree-state-active-git-project");
    fs::create_dir_all(&support_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();
    let project_path = project_dir.to_str().expect("project path");
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "worktrees": [
                {"id": "w1", "projectId": "p1", "name": "main", "branch": "main", "path": project_path, "status": "todo", "isDefault": true}
            ],
            "selectedWorktreeIdByProject": {"p1": "w1"}
        }))
        .unwrap(),
    )
    .unwrap();
    crate::runtime_cache::save_git_summary(
        &support_dir,
        project_path,
        &crate::git::GitSummary {
            branch: "main".to_string(),
            ahead: 2,
            behind: 1,
            is_repository: true,
            ..Default::default()
        },
    );

    let summary =
        WorktreeService::new(support_dir.clone()).state_summary(Some("p1"), Some(project_path));

    assert!(summary.active_git.is_repository);
    assert_eq!(summary.active_git.branch, "main");
    assert_eq!(summary.active_git.ahead, 2);
    assert_eq!(summary.active_git.behind, 1);

    fs::remove_dir_all(support_dir).ok();
    fs::remove_dir_all(project_dir).ok();
}

#[test]
fn summary_includes_per_worktree_git_stats() {
    let support_dir = temp_dir("worktree-summary-git");
    let repo = temp_dir("worktree-summary-git-repo");
    fs::create_dir_all(&support_dir).unwrap();
    create_repo_with_commit(&repo);
    fs::write(repo.join("README.md"), "hello\nworld\n").unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "worktrees": [
                {"id": "w1", "projectId": "p1", "name": "main", "branch": "main", "path": repo, "status": "todo", "isDefault": true}
            ],
            "worktreeTasks": [
                {"worktreeId": "w1", "title": "Main task", "baseBranch": "main", "status": "todo"}
            ],
            "selectedWorktreeIdByProject": {"p1": "w1"}
        }))
        .unwrap(),
    )
    .unwrap();

    let summary = WorktreeService::new(support_dir.clone())
        .summary(Some("p1"), Some(repo.to_str().expect("repo path")));

    assert_eq!(summary.worktrees.len(), 1);
    assert_eq!(summary.worktrees[0].git_summary.changes, 1);
    assert_eq!(summary.worktrees[0].git_summary.additions, 1);
    assert_eq!(summary.worktrees[0].git_summary.deletions, 0);

    fs::remove_dir_all(support_dir).ok();
    fs::remove_dir_all(repo).ok();
}

#[test]
fn worktree_git_stats_match_review_totals() {
    let support_dir = temp_dir("worktree-summary-review-stats");
    let repo = temp_dir("worktree-summary-review-stats-repo");
    fs::create_dir_all(&support_dir).unwrap();
    create_repo_with_commit(&repo);
    fs::write(repo.join("README.md"), "hello\nworld\n").unwrap();
    fs::write(repo.join("UNTRACKED.md"), "one\ntwo\nthree\n").unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "worktrees": [
                {"id": "w1", "projectId": "p1", "name": "main", "branch": "main", "path": repo, "status": "todo", "isDefault": true}
            ],
            "worktreeTasks": [
                {"worktreeId": "w1", "title": "Main task", "baseBranch": "main", "status": "todo"}
            ],
            "selectedWorktreeIdByProject": {"p1": "w1"}
        }))
        .unwrap(),
    )
    .unwrap();

    let summary = WorktreeService::new(support_dir.clone())
        .summary(Some("p1"), Some(repo.to_str().expect("repo path")));
    let review = crate::git::GitService::review(repo.to_str().expect("repo path"), None);
    let review_additions: i64 = review.files.iter().map(|file| file.additions).sum();
    let review_deletions: i64 = review.files.iter().map(|file| file.deletions).sum();

    assert_eq!(summary.worktrees.len(), 1);
    assert_eq!(summary.worktrees[0].git_summary.additions, review_additions);
    assert_eq!(summary.worktrees[0].git_summary.deletions, review_deletions);
    assert_eq!(summary.worktrees[0].git_summary.additions, 4);
    assert_eq!(summary.worktrees[0].git_summary.deletions, 0);

    fs::remove_dir_all(support_dir).ok();
    fs::remove_dir_all(repo).ok();
}

#[test]
fn merge_snapshot_replaces_project_worktrees_and_preserves_existing_task_metadata() {
    let mut raw = serde_json::from_value::<Value>(json!({
        "worktrees": [
            {"id": "old", "projectId": "p1", "name": "old", "branch": "old", "path": "/tmp/old", "status": "done", "isDefault": false},
            {"id": "other", "projectId": "p2", "name": "other", "branch": "main", "path": "/tmp/other", "status": "todo", "isDefault": true}
        ],
        "worktreeTasks": [
            {"worktreeId": "w1", "title": "Keep title", "baseBranch": "main", "baseCommit": null, "status": "running", "createdAt": 10, "updatedAt": 11, "startedAt": 12, "completedAt": null},
            {"worktreeId": "old", "title": "Drop", "baseBranch": "main", "status": "todo"}
        ],
        "selectedWorktreeIdByProject": {"p1": "old", "p2": "other"},
        "unknownTopLevel": "keep"
    }))
    .unwrap()
    .as_object()
    .cloned()
    .unwrap();

    merge_worktree_snapshot(
        &mut raw,
        "p1",
        ScannedWorktreeSnapshot {
            selected_worktree_id: "p1".to_string(),
            worktrees: vec![
                ScannedWorktree {
                    id: "p1".to_string(),
                    project_id: "p1".to_string(),
                    name: "main".to_string(),
                    branch: "main".to_string(),
                    path: "/tmp/main".to_string(),
                    status: "todo".to_string(),
                    is_default: true,
                    created_at: 100,
                    updated_at: 100,
                },
                ScannedWorktree {
                    id: "w1".to_string(),
                    project_id: "p1".to_string(),
                    name: "feature".to_string(),
                    branch: "feature".to_string(),
                    path: "/tmp/feature".to_string(),
                    status: "todo".to_string(),
                    is_default: false,
                    created_at: 100,
                    updated_at: 100,
                },
            ],
            tasks: vec![ScannedTask {
                worktree_id: "w1".to_string(),
                title: "feature".to_string(),
                base_branch: "main".to_string(),
                base_commit: None,
                status: "todo".to_string(),
                created_at: 100,
                updated_at: 100,
                started_at: None,
                completed_at: None,
            }],
        },
    )
    .unwrap();

    let value = Value::Object(raw);
    assert_eq!(value["unknownTopLevel"], "keep");
    assert_eq!(value["selectedWorktreeIdByProject"]["p1"], "p1");
    let worktrees = value["worktrees"].as_array().unwrap();
    assert!(worktrees.iter().any(|worktree| worktree["id"] == "other"));
    assert!(!worktrees.iter().any(|worktree| worktree["id"] == "old"));
    assert!(worktrees.iter().any(|worktree| worktree["id"] == "w1"));
    let tasks = value["worktreeTasks"].as_array().unwrap();
    assert!(!tasks.iter().any(|task| task["worktreeId"] == "old"));
    let task = tasks
        .iter()
        .find(|task| task["worktreeId"] == "w1")
        .unwrap();
    assert_eq!(task["title"], "Keep title");
    assert_eq!(task["status"], "running");
    assert_eq!(task["startedAt"], 12);
}

#[test]
fn merge_snapshot_preserves_existing_non_default_worktree_name() {
    let mut raw = serde_json::from_value::<Value>(json!({
        "worktrees": [
            {"id": "p1", "projectId": "p1", "name": "main", "branch": "main", "path": "/tmp/main", "status": "todo", "isDefault": true},
            {"id": "w1", "projectId": "p1", "name": "20260527-105412", "branch": "20260527-105412", "path": "/tmp/main/.codux/worktrees/20260527-105412", "status": "todo", "isDefault": false}
        ],
        "worktreeTasks": [
            {"worktreeId": "w1", "title": "20260527-105412", "baseBranch": "main", "status": "todo"}
        ],
        "selectedWorktreeIdByProject": {"p1": "w1"}
    }))
    .unwrap()
    .as_object()
    .cloned()
    .unwrap();

    merge_worktree_snapshot(
        &mut raw,
        "p1",
        ScannedWorktreeSnapshot {
            selected_worktree_id: "p1".to_string(),
            worktrees: vec![
                ScannedWorktree {
                    id: "p1".to_string(),
                    project_id: "p1".to_string(),
                    name: "main".to_string(),
                    branch: "main".to_string(),
                    path: "/tmp/main".to_string(),
                    status: "todo".to_string(),
                    is_default: true,
                    created_at: 100,
                    updated_at: 100,
                },
                ScannedWorktree {
                    id: "w1".to_string(),
                    project_id: "p1".to_string(),
                    name: "main".to_string(),
                    branch: "20260527-105412".to_string(),
                    path: "/tmp/main/.codux/worktrees/20260527-105412".to_string(),
                    status: "todo".to_string(),
                    is_default: false,
                    created_at: 100,
                    updated_at: 100,
                },
            ],
            tasks: vec![ScannedTask {
                worktree_id: "w1".to_string(),
                title: "main".to_string(),
                base_branch: "main".to_string(),
                base_commit: None,
                status: "todo".to_string(),
                created_at: 100,
                updated_at: 100,
                started_at: None,
                completed_at: None,
            }],
        },
    )
    .unwrap();

    let value = Value::Object(raw);
    let worktree = value["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|worktree| worktree["id"] == "w1")
        .unwrap();
    assert_eq!(worktree["name"], "20260527-105412");
    let task = value["worktreeTasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["worktreeId"] == "w1")
        .unwrap();
    assert_eq!(task["title"], "20260527-105412");
}

#[test]
fn tauri_snapshot_reports_non_git_repository_without_failing() {
    let support_dir = temp_dir("worktree-tauri-snapshot-support");
    let project_dir = temp_dir("worktree-tauri-snapshot-project");
    fs::create_dir_all(&support_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();
    let snapshot = WorktreeService::new(support_dir.clone()).snapshot(
        "project".to_string(),
        project_dir.to_string_lossy().to_string(),
    );

    assert_eq!(snapshot.project_id, "project");
    assert_eq!(snapshot.selected_worktree_id, "project");
    assert_eq!(snapshot.worktrees.len(), 1);
    assert_eq!(snapshot.worktrees[0].id, "project");
    assert!(snapshot.worktrees[0].is_default);
    assert!(snapshot.error.is_some());

    fs::remove_dir_all(support_dir).ok();
    fs::remove_dir_all(project_dir).ok();
}

#[test]
fn tauri_create_request_uses_requested_branch_and_task_title() {
    let repo = temp_dir("worktree-tauri-create");
    create_repo_with_commit(&repo);
    let support_dir = temp_dir("worktree-tauri-create-support");
    fs::create_dir_all(&support_dir).unwrap();
    let service = WorktreeService::new(support_dir.clone());

    let snapshot = service
        .create_from_request(WorktreeCreateRequest {
            project_id: "project".to_string(),
            project_path: repo.to_string_lossy().to_string(),
            base_branch: None,
            branch_name: "feature/demo".to_string(),
            task_title: Some("Demo task".to_string()),
        })
        .unwrap();

    let created = snapshot
        .worktrees
        .iter()
        .find(|worktree| !worktree.is_default)
        .expect("created worktree");
    assert_eq!(created.branch, "feature/demo");
    assert_eq!(snapshot.selected_worktree_id, created.id);
    assert!(branch_exists(repo.to_str().expect("repo"), "feature/demo"));
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.worktree_id == created.id)
        .expect("created task");
    assert_eq!(task.title, "Demo task");

    fs::remove_dir_all(repo).ok();
    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn generates_stable_worktree_slugs_and_ids() {
    assert_eq!(worktree_slug("feature/One!"), "feature-one");
    assert_eq!(
        worktree_uuid("project", "/repo/.codux/worktrees/feature-one"),
        worktree_uuid("project", "/repo/.codux/worktrees/feature-one")
    );
}

#[test]
fn resolves_only_mergeable_worktree_branches() {
    assert_eq!(
        mergeable_branch(Some("feature/one"), "fallback").as_deref(),
        Some("feature/one")
    );
    assert_eq!(
        mergeable_branch(Some("HEAD"), "fallback").as_deref(),
        Some("fallback")
    );
    assert!(mergeable_branch(None, "detached abc1234").is_none());
    assert!(mergeable_branch(Some("detached HEAD"), "fallback").is_none());
    assert!(mergeable_branch(None, " ").is_none());
}

#[test]
fn removable_worktree_branch_skips_current_default_branch() {
    let repo = temp_dir("worktree-default-branch");
    create_repo_with_commit(&repo);

    assert_eq!(
        removable_worktree_branch(repo.to_str().expect("repo"), repo.to_str().expect("repo")),
        None
    );

    fs::remove_dir_all(repo).ok();
}

#[test]
fn delete_local_branch_ignores_missing_branch_and_rejects_current_branch() {
    let repo = temp_dir("worktree-delete-branch");
    create_repo_with_commit(&repo);
    create_branch(&repo, "topic/delete-me");

    delete_local_branch(repo.to_str().expect("repo"), "topic/delete-me").expect("delete branch");
    delete_local_branch(repo.to_str().expect("repo"), "topic/delete-me")
        .expect("ignore missing branch");
    assert!(!branch_exists(
        repo.to_str().expect("repo"),
        "topic/delete-me"
    ));
    let current = current_branch(repo.to_str().expect("repo")).expect("current branch");
    assert!(delete_local_branch(repo.to_str().expect("repo"), &current).is_err());

    fs::remove_dir_all(repo).ok();
}

#[test]
fn remove_worktree_can_delete_matching_local_branch() {
    let repo = temp_dir("worktree-remove-branch");
    create_repo_with_commit(&repo);
    let support_dir = temp_dir("worktree-remove-branch-support");
    let service = WorktreeService::new(support_dir.clone());
    service
        .create_worktree("project", repo.to_str().expect("repo"))
        .expect("create worktree");
    let summary = service.summary(Some("project"), Some(repo.to_str().expect("repo")));
    let created = summary
        .worktrees
        .iter()
        .find(|worktree| !worktree.is_default)
        .expect("created worktree");
    let branch = created.branch.clone();
    assert!(branch_exists(repo.to_str().expect("repo"), &branch));

    service
        .remove_worktree("project", repo.to_str().expect("repo"), &created.id, true)
        .expect("remove worktree and branch");

    assert!(!branch_exists(repo.to_str().expect("repo"), &branch));
    fs::remove_dir_all(repo).ok();
    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn merge_worktree_reports_conflicts() {
    let repo = temp_dir("worktree-merge-conflict");
    create_repo_with_commit(&repo);
    let support_dir = temp_dir("worktree-merge-conflict-support");
    let service = WorktreeService::new(support_dir.clone());
    let snapshot = service
        .create_from_request(WorktreeCreateRequest {
            project_id: "project".to_string(),
            project_path: repo.to_string_lossy().to_string(),
            base_branch: None,
            branch_name: "feature/conflict".to_string(),
            task_title: Some("Conflict task".to_string()),
        })
        .expect("create worktree");
    let created = snapshot
        .worktrees
        .iter()
        .find(|worktree| !worktree.is_default)
        .expect("created worktree");

    commit_file(&repo, "README.md", "hello\nbase\n", "base change");
    commit_file(
        Path::new(&created.path),
        "README.md",
        "hello\nfeature\n",
        "feature change",
    );

    let error = service
        .merge_worktree("project", repo.to_str().expect("repo"), &created.id)
        .expect_err("merge should report conflict");

    assert!(error.contains("Merge produced conflicts"));
    fs::remove_dir_all(repo).ok();
    fs::remove_dir_all(support_dir).ok();
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codux-gpui-{label}-{nanos}"))
}

fn create_repo_with_commit(repo: &Path) {
    fs::create_dir_all(repo).expect("create repo dir");
    let git = super::GitRepository::init(repo).expect("init repo");
    let mut config = git.config().expect("repo config");
    config
        .set_str("user.email", "codux@example.test")
        .expect("set user email");
    config.set_str("user.name", "Codux").expect("set user name");
    fs::write(repo.join("README.md"), "hello\n").expect("write readme");
    let mut index = git.index().expect("index");
    index.add_path(Path::new("README.md")).expect("add readme");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = git.find_tree(tree_id).expect("find tree");
    let signature = git2::Signature::now("Codux", "codux@example.test").expect("test signature");
    git.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .expect("commit");
}

fn commit_file(repo_path: &Path, relative_path: &str, content: &str, message: &str) {
    let git = super::GitRepository::discover(repo_path).expect("discover repo");
    fs::write(repo_path.join(relative_path), content).expect("write file");
    let mut index = git.index().expect("index");
    index
        .add_path(Path::new(relative_path))
        .expect("add changed file");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = git.find_tree(tree_id).expect("find tree");
    let parent = git
        .head()
        .and_then(|head| head.peel_to_commit())
        .expect("head commit");
    let signature = git2::Signature::now("Codux", "codux@example.test").expect("test signature");
    git.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent],
    )
    .expect("commit file");
}

fn create_branch(repo: &Path, branch: &str) {
    let git = super::GitRepository::discover(repo).expect("discover repo");
    let head = git
        .head()
        .and_then(|head| head.peel_to_commit())
        .expect("head commit");
    git.branch(branch, &head, false).expect("create branch");
}

fn branch_exists(repo: &str, branch: &str) -> bool {
    let Ok(git) = super::GitRepository::discover(repo) else {
        return false;
    };
    git.find_branch(branch, git2::BranchType::Local).is_ok()
}
