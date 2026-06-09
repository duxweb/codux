impl RuntimeService {
    pub fn create_project_file(
        &self,
        project_path: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<Vec<FileEntry>, String> {
        FilesService::create_file(project_path, parent_path, name)?;
        Ok(load_file_entries(project_path, parent_path))
    }

    pub fn create_project_directory(
        &self,
        project_path: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<Vec<FileEntry>, String> {
        FilesService::create_dir(project_path, parent_path, name)?;
        Ok(load_file_entries(project_path, parent_path))
    }

    pub fn delete_project_file_entry(
        &self,
        project_path: &str,
        entry_path: &str,
        directory_path: Option<&str>,
    ) -> Result<Vec<FileEntry>, String> {
        FilesService::delete(project_path, entry_path)?;
        Ok(load_file_entries(project_path, directory_path))
    }

    pub fn write_project_file(
        &self,
        project_path: &str,
        file_path: &str,
        content: &str,
    ) -> Result<String, String> {
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
        let entry =
            FilesService::write_bytes_to_directory(project_path, directory_path, file_name, &bytes)?;
        Ok((
            load_file_entries(project_path, directory_path),
            entry.relative_path,
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
