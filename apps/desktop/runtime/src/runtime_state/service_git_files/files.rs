impl RuntimeService {
    pub fn create_project_file(
        &self,
        project_path: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<Vec<FileEntry>, String> {
        if let Some(device_id) = self.host_device_for_project_path(project_path) {
            let target = format!(
                "{}/{name}",
                remote_absolute_path(project_path, parent_path).trim_end_matches('/')
            );
            self.remote_controllers
                .controller_for(&device_id)?
                .write_file(&target, "")?;
            return self.remote_project_files(&device_id, project_path, parent_path);
        }
        FilesService::create_file(project_path, parent_path, name)?;
        Ok(load_file_entries(project_path, parent_path))
    }

    pub fn create_project_directory(
        &self,
        project_path: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<Vec<FileEntry>, String> {
        if let Some(device_id) = self.host_device_for_project_path(project_path) {
            let target = format!(
                "{}/{name}",
                remote_absolute_path(project_path, parent_path).trim_end_matches('/')
            );
            self.remote_controllers
                .controller_for(&device_id)?
                .create_directory(&target)?;
            return self.remote_project_files(&device_id, project_path, parent_path);
        }
        FilesService::create_dir(project_path, parent_path, name)?;
        Ok(load_file_entries(project_path, parent_path))
    }

    pub fn delete_project_file_entry(
        &self,
        project_path: &str,
        entry_path: &str,
        directory_path: Option<&str>,
    ) -> Result<Vec<FileEntry>, String> {
        if let Some(device_id) = self.host_device_for_project_path(project_path) {
            let target = remote_absolute_path(project_path, Some(entry_path));
            self.remote_controllers
                .controller_for(&device_id)?
                .delete_path(&target)?;
            return self.remote_project_files(&device_id, project_path, directory_path);
        }
        FilesService::delete(project_path, entry_path)?;
        Ok(load_file_entries(project_path, directory_path))
    }

    pub fn write_project_file(
        &self,
        project_path: &str,
        file_path: &str,
        content: &str,
    ) -> Result<String, String> {
        if let Some(device_id) = self.host_device_for_project_path(project_path) {
            let target = remote_absolute_path(project_path, Some(file_path));
            self.remote_controllers
                .controller_for(&device_id)?
                .write_file(&target, content)?;
            return Ok(content.to_string());
        }
        let result = FilesService::write_text(project_path, file_path, content)?;
        Ok(result.content)
    }

    pub fn rename_project_file_entry(
        &self,
        project_path: &str,
        entry_path: &str,
        new_name: &str,
        directory_path: Option<&str>,
    ) -> Result<(Vec<FileEntry>, String), String> {
        if let Some(device_id) = self.host_device_for_project_path(project_path) {
            let source = remote_absolute_path(project_path, Some(entry_path));
            let parent = std::path::Path::new(&source)
                .parent()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default();
            let target = format!("{}/{new_name}", parent.trim_end_matches('/'));
            self.remote_controllers
                .controller_for(&device_id)?
                .rename_path(&source, &target)?;
            let new_relative = target
                .strip_prefix(project_path.trim_end_matches('/'))
                .unwrap_or(&target)
                .trim_start_matches('/')
                .to_string();
            let entries = self.remote_project_files(&device_id, project_path, directory_path)?;
            return Ok((entries, new_relative));
        }
        let entry = FilesService::rename(project_path, entry_path, new_name)?;
        Ok((
            load_file_entries(project_path, directory_path),
            entry.relative_path,
        ))
    }

    pub fn copy_project_file_entry(
        &self,
        project_path: &str,
        entry_path: &str,
        directory_path: Option<&str>,
    ) -> Result<(Vec<FileEntry>, String), String> {
        if let Some(device_id) = self.host_device_for_project_path(project_path) {
            let controller = self.remote_controllers.controller_for(&device_id)?;
            let new_abs = controller.copy_path(
                &remote_absolute_path(project_path, Some(entry_path)),
                &remote_absolute_path(project_path, directory_path),
            )?;
            let entries = self.remote_project_files(&device_id, project_path, directory_path)?;
            return Ok((entries, remote_relative_path(project_path, &new_abs)));
        }
        let entry = FilesService::copy_to_directory(project_path, entry_path, directory_path)?;
        Ok((
            load_file_entries(project_path, directory_path),
            entry.relative_path,
        ))
    }

    pub fn move_project_file_entry(
        &self,
        project_path: &str,
        entry_path: &str,
        target_directory_path: &str,
        directory_path: Option<&str>,
    ) -> Result<(Vec<FileEntry>, String), String> {
        if let Some((entries, relative)) = self.remote_move_file_entry(
            project_path,
            entry_path,
            target_directory_path,
            directory_path,
            false,
        ) {
            return Ok((entries?, relative));
        }
        let entry = FilesService::move_to_directory(project_path, entry_path, target_directory_path)?;
        Ok((
            load_file_entries(project_path, directory_path),
            entry.relative_path,
        ))
    }

    pub fn move_project_file_entry_overwrite(
        &self,
        project_path: &str,
        entry_path: &str,
        target_directory_path: &str,
        directory_path: Option<&str>,
    ) -> Result<(Vec<FileEntry>, String), String> {
        if let Some((entries, relative)) = self.remote_move_file_entry(
            project_path,
            entry_path,
            target_directory_path,
            directory_path,
            true,
        ) {
            return Ok((entries?, relative));
        }
        let entry =
            FilesService::move_to_directory_overwrite(project_path, entry_path, target_directory_path)?;
        Ok((
            load_file_entries(project_path, directory_path),
            entry.relative_path,
        ))
    }

    pub fn import_external_project_files(
        &self,
        project_path: &str,
        source_paths: Vec<String>,
        directory_path: Option<&str>,
    ) -> Result<(Vec<FileEntry>, Option<String>), String> {
        if let Some(device_id) = self.host_device_for_project_path(project_path) {
            // Source files are local on this desktop; upload each to the host.
            let controller = self.remote_controllers.controller_for(&device_id)?;
            let directory = remote_absolute_path(project_path, directory_path);
            let mut selected = None;
            for source in &source_paths {
                let bytes = std::fs::read(source).map_err(|error| error.to_string())?;
                let name = std::path::Path::new(source)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("file");
                let new_abs = controller.write_bytes(&directory, name, &bytes)?;
                if selected.is_none() {
                    selected = Some(remote_relative_path(project_path, &new_abs));
                }
            }
            let entries = self.remote_project_files(&device_id, project_path, directory_path)?;
            return Ok((entries, selected));
        }
        let entries = crate::files::file_import_external(FileExternalCopyRequest {
            root_path: project_path.to_string(),
            source_paths,
            target_directory_path: directory_path.map(str::to_string),
        })?;
        let selected = entries.first().map(|entry| entry.relative_path.clone());
        Ok((load_file_entries(project_path, directory_path), selected))
    }

    pub fn write_project_file_bytes(
        &self,
        project_path: &str,
        directory_path: Option<&str>,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<(Vec<FileEntry>, String), String> {
        if let Some(device_id) = self.host_device_for_project_path(project_path) {
            let new_abs = self.remote_controllers.controller_for(&device_id)?.write_bytes(
                &remote_absolute_path(project_path, directory_path),
                file_name,
                &bytes,
            )?;
            let entries = self.remote_project_files(&device_id, project_path, directory_path)?;
            return Ok((entries, remote_relative_path(project_path, &new_abs)));
        }
        let entry =
            FilesService::write_bytes_to_directory(project_path, directory_path, file_name, &bytes)?;
        Ok((
            load_file_entries(project_path, directory_path),
            entry.relative_path,
        ))
    }

    /// Shared remote move (used by move + move-overwrite). `None` ⇒ local project.
    fn remote_move_file_entry(
        &self,
        project_path: &str,
        entry_path: &str,
        target_directory_path: &str,
        directory_path: Option<&str>,
        overwrite: bool,
    ) -> Option<(Result<Vec<FileEntry>, String>, String)> {
        let device_id = self.host_device_for_project_path(project_path)?;
        let controller = match self.remote_controllers.controller_for(&device_id) {
            Ok(controller) => controller,
            Err(error) => return Some((Err(error), String::new())),
        };
        let new_abs = match controller.move_path(
            &remote_absolute_path(project_path, Some(entry_path)),
            &remote_absolute_path(project_path, Some(target_directory_path)),
            overwrite,
        ) {
            Ok(path) => path,
            Err(error) => return Some((Err(error), String::new())),
        };
        let relative = remote_relative_path(project_path, &new_abs);
        Some((
            self.remote_project_files(&device_id, project_path, directory_path),
            relative,
        ))
    }

    pub fn reveal_project_file_entry(
        &self,
        project_path: &str,
        entry_path: &str,
    ) -> Result<(), String> {
        FilesService::reveal(project_path, entry_path)
    }

    pub fn open_project_file_entry(
        &self,
        project_path: &str,
        entry_path: &str,
    ) -> Result<(), String> {
        FilesService::open_path(project_path, entry_path)
    }
}
