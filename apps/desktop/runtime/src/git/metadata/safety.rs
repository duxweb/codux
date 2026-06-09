fn normalized_pathspecs(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.trim().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .collect()
}

fn normalize_git_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn safe_branch_name(branch: &str) -> Result<String, String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    if branch.starts_with('-') {
        return Err("Branch name cannot start with '-'.".to_string());
    }
    if branch.contains('\\') {
        return Err("Branch name cannot contain backslashes.".to_string());
    }
    git2::Reference::is_valid_name(&format!("refs/heads/{branch}"))
        .then(|| branch.to_string())
        .ok_or_else(|| "Invalid branch name.".to_string())
}

fn safe_git_path(file_path: &str) -> Result<String, String> {
    let trimmed = file_path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err("Absolute Git file paths are not allowed.".to_string());
    }
    if path
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err("Invalid Git file path.".to_string());
    }
    Ok(trimmed.replace('\\', "/"))
}
