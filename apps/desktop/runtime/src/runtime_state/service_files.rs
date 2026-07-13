impl RuntimeService {
    pub fn reload_project_files(
        &self,
        project_path: &str,
        directory_path: Option<&str>,
    ) -> Vec<FileEntry> {
        self.try_reload_project_files(project_path, directory_path)
            .unwrap_or_else(|error| {
                // Log the errno instead of a silent empty tree (EPERM/EMFILE under fd pressure or a sandbox/TCC denial).
                crate::runtime_trace::runtime_trace(
                    "files",
                    &format!("reload failed project={project_path}: {error}"),
                );
                Vec::new()
            })
    }

    pub fn try_reload_project_files(
        &self,
        project_path: &str,
        directory_path: Option<&str>,
    ) -> Result<Vec<FileEntry>, String> {
        if let Some(runtime) = self.hosted_runtime_for_project_path(project_path) {
            return self.hosted_project_files(&runtime?, project_path, directory_path);
        }
        try_load_file_entries(project_path, directory_path)
    }

    pub fn watch_project_files(
        &self,
        project_path: String,
        on_change: impl Fn(FileChangeEvent) + Send + 'static,
    ) -> Result<FileWatchRegistration, String> {
        self.file_watch_manager.watch(project_path, on_change)
    }

    pub fn unwatch_project_files(&self, project_path: String) -> Result<(), String> {
        self.file_watch_manager.unwatch(project_path)
    }

    pub fn file_watch(&self, project_path: String) -> Result<FileWatchRegistration, String> {
        let events = Arc::clone(&self.file_watch_events);
        self.watch_project_files(project_path, move |event| {
            if let Ok(mut events) = events.lock() {
                events.push_back(event);
                while events.len() > 128 {
                    events.pop_front();
                }
            }
        })
    }

    pub fn file_unwatch(&self, project_path: String) -> Result<(), String> {
        self.unwatch_project_files(project_path)
    }

    pub fn drain_file_change_events(&self) -> Vec<FileChangeEvent> {
        self.file_watch_events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }
    fn watch_active_project_files(
        &self,
        project_path: String,
        generation: u64,
    ) -> Result<Option<FileWatchRegistration>, String> {
        let registration = self.file_watch(project_path)?;
        let previous = self
            .active_project_watches
            .lock()
            .map_err(|_| "Active project watcher lock is poisoned.".to_string())?
            .then_install_file(generation, registration.project_path.clone());
        let Some(previous) = previous else {
            let _ = self.file_unwatch(registration.project_path.clone());
            return Ok(None);
        };
        if let Some(previous) = previous.filter(|path| path != &registration.project_path) {
            let _ = self.file_unwatch(previous);
        }
        Ok(Some(registration))
    }

    fn begin_project_watch_switch(&self) -> Result<u64, String> {
        let mut active = self
            .active_project_watches
            .lock()
            .map_err(|_| "Active project watcher lock is poisoned.".to_string())?;
        Ok(active.begin_switch())
    }

    fn stop_active_project_watches(&self) {
        if self.begin_project_watch_switch().is_err() {
            return;
        }
        let service = self.clone();
        drop(crate::async_runtime::spawn_blocking(move || {
            let Ok(_registration) = service.project_watch_registration.lock() else {
                return;
            };
            service.drain_pending_project_watch_cleanup();
        }));
    }

    fn drain_pending_project_watch_cleanup(&self) {
        let (file_paths, git_paths) = self
            .active_project_watches
            .lock()
            .map(|mut active| active.take_pending_unwatches())
            .unwrap_or_default();
        for path in file_paths {
            let _ = self.file_unwatch(path);
        }
        for path in git_paths {
            let _ = self.git_unwatch(path);
        }
    }

    fn project_watch_generation_is_current(&self, generation: u64) -> bool {
        self.active_project_watches
            .lock()
            .map(|active| active.generation == generation)
            .unwrap_or(false)
    }
}

impl ActiveProjectWatches {
    fn begin_switch(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        if let Some(path) = self.file_path.take() {
            self.pending_file_unwatches.push(path);
        }
        if let Some(path) = self.git_path.take() {
            self.pending_git_unwatches.push(path);
        }
        self.generation
    }

    fn take_pending_unwatches(&mut self) -> (Vec<String>, Vec<String>) {
        (
            std::mem::take(&mut self.pending_file_unwatches),
            std::mem::take(&mut self.pending_git_unwatches),
        )
    }

    fn then_install_file(&mut self, generation: u64, path: String) -> Option<Option<String>> {
        (self.generation == generation).then(|| self.file_path.replace(path))
    }

    fn then_install_git(&mut self, generation: u64, path: String) -> Option<Option<String>> {
        (self.generation == generation).then(|| self.git_path.replace(path))
    }
}
