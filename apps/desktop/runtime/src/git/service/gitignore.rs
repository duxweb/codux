impl GitService {
    pub fn append_gitignore(project_path: &str, paths: &[String]) -> Result<(), String> {
        let root = repository_root(project_path)?;
        let additions = paths
            .iter()
            .map(|path| path.trim())
            .filter(|path| !path.is_empty())
            .collect::<Vec<_>>();
        if additions.is_empty() {
            return Ok(());
        }
        let gitignore_path = Path::new(&root).join(".gitignore");
        let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();
        let existing_lines = existing.lines().map(str::trim).collect::<HashSet<_>>();
        let next = additions
            .into_iter()
            .filter(|path| !existing_lines.contains(path))
            .collect::<Vec<_>>();
        if next.is_empty() {
            return Ok(());
        }
        let mut content = existing;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&next.join("\n"));
        content.push('\n');
        fs::write(gitignore_path, content).map_err(|error| error.to_string())
    }
}
