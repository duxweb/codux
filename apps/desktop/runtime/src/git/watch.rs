#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWatchRegistration {
    pub project_path: String,
    pub repository_path: String,
    pub is_repository: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoryChangeEvent {
    pub project_path: String,
    pub repository_path: String,
    pub changed_paths: Vec<String>,
}

pub struct GitWatchManager {
    watchers: Mutex<HashMap<String, GitRepositoryWatcher>>,
}

struct GitRepositoryWatcher {
    _watcher: RecommendedWatcher,
    project_paths: Arc<Mutex<HashSet<String>>>,
    _repository_path: String,
    _watch_paths: Vec<PathBuf>,
}

impl Default for GitWatchManager {
    fn default() -> Self {
        Self {
            watchers: Mutex::new(HashMap::new()),
        }
    }
}

impl GitWatchManager {
    pub fn watch(
        &self,
        project_path: String,
        on_changed: impl Fn(GitRepositoryChangeEvent) + Send + Sync + 'static,
    ) -> Result<GitWatchRegistration, String> {
        let watch_target = resolve_watch_target(&project_path)?;
        let key = watch_target.repository_key.clone();
        let registration = GitWatchRegistration {
            project_path: watch_target.project_path.clone(),
            repository_path: watch_target.repository_path.clone(),
            is_repository: watch_target.is_repository,
        };

        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| "Git watcher lock is poisoned.".to_string())?;
        if let Some(existing) = watchers.get(&key) {
            if let Ok(mut paths) = existing.project_paths.lock() {
                paths.insert(watch_target.project_path.clone());
            }
            return Ok(registration);
        }

        let project_paths_for_event =
            Arc::new(Mutex::new(HashSet::from([watch_target.project_path.clone()])));
        let repository_path_for_event = watch_target.repository_path.clone();
        let repository_key = watch_target.repository_key.clone();
        let git_dir_keys = watch_target.git_dir_keys.clone();
        let on_changed = Arc::new(on_changed);
        let (change_tx, change_rx) = mpsc::channel::<Vec<String>>();
        let debounced_paths = Arc::clone(&project_paths_for_event);
        let debounced_repository_path = repository_path_for_event.clone();
        let debounced_on_changed = Arc::clone(&on_changed);
        thread::Builder::new()
            .name("codux-git-watch-debounce".to_string())
            .spawn(move || {
                run_git_watch_debounce(
                    change_rx,
                    debounced_paths,
                    debounced_repository_path,
                    debounced_on_changed,
                );
            })
            .map_err(|error| error.to_string())?;
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else {
                return;
            };
            let changed_paths = event
                .paths
                .iter()
                .filter_map(|path| {
                    let key = normalized_path_key(path);
                    should_forward_git_watch_path(&repository_key, &git_dir_keys, &key)
                        .then(|| normalized_path_display(path))
                })
                .collect::<Vec<_>>();
            if changed_paths.is_empty() {
                return;
            }
            let _ = change_tx.send(changed_paths);
        })
        .map_err(|error| error.to_string())?;

        for path in &watch_target.watch_paths {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(|error| error.to_string())?;
        }

        watchers.insert(
            key,
            GitRepositoryWatcher {
                _watcher: watcher,
                project_paths: project_paths_for_event,
                _repository_path: watch_target.repository_path,
                _watch_paths: watch_target.watch_paths,
            },
        );
        Ok(registration)
    }

    pub fn unwatch(&self, project_path: String) -> Result<(), String> {
        let requested_key = normalized_path_key(Path::new(project_path.trim()));
        let repository_key = resolve_watch_target(&project_path)
            .map(|target| target.repository_key)
            .unwrap_or_else(|_| requested_key.clone());
        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| "Git watcher lock is poisoned.".to_string())?;
        if let Some(watcher) = watchers.get(&repository_key) {
            let mut should_remove = false;
            if let Ok(mut paths) = watcher.project_paths.lock() {
                should_remove = remove_watched_project_path(&mut paths, &requested_key);
            }
            if should_remove {
                watchers.remove(&repository_key);
            }
            return Ok(());
        }
        watchers.retain(|_, watcher| {
            let mut should_remove = false;
            if let Ok(mut paths) = watcher.project_paths.lock() {
                should_remove = remove_watched_project_path(&mut paths, &requested_key);
            }
            !should_remove
        });
        Ok(())
    }
}

fn run_git_watch_debounce(
    rx: mpsc::Receiver<Vec<String>>,
    watched_project_paths: Arc<Mutex<HashSet<String>>>,
    repository_path: String,
    on_changed: Arc<impl Fn(GitRepositoryChangeEvent) + Send + Sync + 'static>,
) {
    while let Ok(paths) = rx.recv() {
        let mut changed_paths = paths;
        loop {
            match rx.recv_timeout(Duration::from_millis(GIT_WATCH_DEBOUNCE_MS)) {
                Ok(next_paths) => push_unique_strings(&mut changed_paths, next_paths),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let project_paths = watched_project_paths
            .lock()
            .map(|paths| paths.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for project_path in project_paths {
            on_changed(GitRepositoryChangeEvent {
                project_path,
                repository_path: repository_path.clone(),
                changed_paths: changed_paths.clone(),
            });
        }
    }
}

fn push_unique_strings(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        if !target.iter().any(|existing| existing == &value) {
            target.push(value);
        }
    }
}

fn remove_watched_project_path(paths: &mut HashSet<String>, requested_key: &str) -> bool {
    paths.retain(|path| normalized_path_key(Path::new(path)) != requested_key);
    paths.is_empty()
}

struct GitWatchTarget {
    project_path: String,
    repository_path: String,
    repository_key: String,
    git_dir_keys: Vec<String>,
    watch_paths: Vec<PathBuf>,
    is_repository: bool,
}

fn resolve_watch_target(project_path: &str) -> Result<GitWatchTarget, String> {
    let project = PathBuf::from(project_path.trim());
    if project.as_os_str().is_empty() {
        return Err("Project path cannot be empty.".to_string());
    }
    if !project.exists() {
        return Err(format!(
            "Project path does not exist: {}",
            project.display()
        ));
    }

    let project_path = normalized_path_display(&project);
    let root = repository_root(project_path.as_str()).ok();
    let is_repository = root.is_some();
    let repository_path = root.unwrap_or_else(|| project_path.clone());
    let repository_path_buf = PathBuf::from(&repository_path);
    let repository_key = normalized_path_key(&repository_path_buf);
    let git_dirs = if is_repository {
        repository_git_dirs(&repository_path_buf)
    } else {
        vec![repository_path_buf.join(".git")]
    };
    let git_dir_keys = git_dirs
        .iter()
        .map(|path| normalized_path_key(path))
        .collect::<Vec<_>>();

    let mut watch_paths = Vec::new();
    push_unique_path(&mut watch_paths, repository_path_buf);
    for git_dir in git_dirs {
        if git_dir.exists() {
            push_unique_path(&mut watch_paths, git_dir);
        }
    }

    Ok(GitWatchTarget {
        project_path,
        repository_path,
        repository_key,
        git_dir_keys,
        watch_paths,
        is_repository,
    })
}

fn repository_git_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(repo) = GitRepository::discover(root) {
        push_unique_path(&mut dirs, repo.path().to_path_buf());
        push_unique_path(&mut dirs, repo.commondir().to_path_buf());
    }
    if dirs.is_empty() {
        dirs.push(root.join(".git"));
    }
    dirs
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    let key = normalized_path_key(&path);
    if paths
        .iter()
        .any(|existing| normalized_path_key(existing) == key)
    {
        return;
    }
    paths.push(path);
}

fn normalized_path_key(path: &Path) -> String {
    let normalized_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut key = normalized_path.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    #[cfg(windows)]
    {
        key = key.to_ascii_lowercase();
    }
    key
}

fn normalized_path_display(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn should_forward_git_watch_path(
    repository_key: &str,
    git_dir_keys: &[String],
    path_key: &str,
) -> bool {
    for git_dir_key in git_dir_keys {
        let is_git_path = path_key == git_dir_key
            || path_key
                .strip_prefix(git_dir_key)
                .is_some_and(|suffix| suffix.starts_with('/'));
        if !is_git_path {
            continue;
        }

        let relative = path_key
            .strip_prefix(git_dir_key)
            .unwrap_or("")
            .trim_start_matches('/');
        return is_allowed_git_metadata_path(relative);
    }

    let repository_git_key = format!("{repository_key}/.git");
    if path_key == repository_git_key
        || path_key
            .strip_prefix(&repository_git_key)
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        let relative = path_key
            .strip_prefix(&repository_git_key)
            .unwrap_or("")
            .trim_start_matches('/');
        return is_allowed_git_metadata_path(relative);
    }

    true
}

fn is_allowed_git_metadata_path(relative: &str) -> bool {
    let relative = relative.trim_start_matches('/');
    if relative.is_empty() {
        return false;
    }

    #[cfg(windows)]
    {
        let relative = relative.to_ascii_lowercase();
        match relative.as_str() {
            "head" | "index" | "fetch_head" | "orig_head" | "packed-refs" => true,
            _ => relative.starts_with("refs/") || relative.starts_with("logs/head"),
        }
    }

    #[cfg(not(windows))]
    {
        match relative {
            "HEAD" | "index" | "FETCH_HEAD" | "ORIG_HEAD" | "packed-refs" => true,
            _ => relative.starts_with("refs/") || relative.starts_with("logs/HEAD"),
        }
    }
}
