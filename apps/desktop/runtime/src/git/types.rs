#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSummary {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub head_pushed: bool,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub is_repository: bool,
    pub error: Option<String>,
    pub changed_files: Vec<GitFileStatus>,
    pub branches: Vec<GitBranchSummary>,
    pub remote_branches: Vec<String>,
    pub remotes: Vec<GitRemoteSummary>,
    pub commits: Vec<GitCommitSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileStatus {
    pub path: String,
    pub index_status: String,
    pub worktree_status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranchSummary {
    pub name: String,
    pub is_current: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemoteSummary {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCredentials {
    pub username: String,
    pub password_or_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitSummary {
    pub hash: String,
    pub title: String,
    pub relative_time: String,
    pub decorations: Option<String>,
    pub graph_prefix: String,
    pub author: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewSummary {
    pub mode: String,
    pub title: String,
    pub base_branch: Option<String>,
    pub diff_stat: String,
    pub files: Vec<GitReviewFile>,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewFile {
    pub path: String,
    pub status: String,
    pub additions: i64,
    pub deletions: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitMessageContextSummary {
    pub diff: String,
    pub truncated: bool,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewContentSummary {
    pub path: String,
    pub head_content: String,
    pub base_content: Option<String>,
    pub index_content: Option<String>,
    pub worktree_content: String,
    pub added_lines: Vec<usize>,
    pub deleted_lines: Vec<usize>,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusSnapshot {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub staged: Vec<GitFileStatus>,
    pub unstaged: Vec<GitFileStatus>,
    pub untracked: Vec<GitFileStatus>,
    pub commits: Vec<GitCommitSummary>,
    pub branches: Vec<GitBranchSummary>,
    pub remote_branches: Vec<String>,
    pub remotes: Vec<GitRemoteSummary>,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBriefStatus {
    pub branch: String,
    pub ahead: i64,
    pub behind: i64,
    pub changes: usize,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranchesSnapshot {
    pub current: String,
    pub local: Vec<GitBranchSummary>,
    pub remote: Vec<GitBranchSummary>,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffSnapshot {
    pub path: String,
    pub diff: String,
    pub is_repository: bool,
    pub error: Option<String>,
}

pub type GitReviewSnapshot = GitReviewSummary;
pub type GitCommitMessageContextSnapshot = GitCommitMessageContextSummary;
pub type GitReviewContentSnapshot = GitReviewContentSummary;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPathsRequest {
    pub project_path: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitRequest {
    pub project_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranchRequest {
    pub project_path: String,
    pub branch: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCreateBranchRequest {
    pub project_path: String,
    pub branch: String,
    pub from: Option<String>,
    pub checkout: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffRequest {
    pub project_path: String,
    pub path: String,
    pub staged: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewDiffRequest {
    pub project_path: String,
    pub path: String,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewContentRequest {
    pub project_path: String,
    pub path: String,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCloneRequest {
    pub project_path: String,
    pub remote_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitActionRequest {
    pub project_path: String,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemoteRequest {
    pub project_path: String,
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDeleteBranchRequest {
    pub project_path: String,
    pub branch: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitRefRequest {
    pub project_path: String,
    pub commit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRestoreCommitRequest {
    pub project_path: String,
    pub commit: String,
    #[serde(default)]
    pub force_remote: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPushRemoteRequest {
    pub project_path: String,
    pub remote: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPushRemoteBranchRequest {
    pub project_path: String,
    pub remote_branch: String,
    pub local_branch: Option<String>,
}
