use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct RuntimeInventory {
    pub source_root: PathBuf,
    pub root: PathBuf,
    pub runtime_src: PathBuf,
    pub locale_files: usize,
    pub wrapper_bins: usize,
    pub shell_hooks: usize,
    pub staged_rust_modules: usize,
    pub staged_files: usize,
    pub available: bool,
    pub stage_error: Option<String>,
}

impl RuntimeInventory {
    pub fn load() -> Self {
        let source_root = runtime_assets_path();
        let root = staged_runtime_root_path();
        let runtime_src = runtime_src_path();
        let stage_error = stage_runtime_assets(&source_root, &root).err();
        let available = root.is_dir() && stage_error.is_none();
        Self {
            locale_files: count_files(root.join("i18n/locales")),
            wrapper_bins: count_files(root.join("scripts/wrappers/bin")),
            shell_hooks: count_files(root.join("scripts/shell-hooks")),
            staged_files: count_files_recursive(&root),
            source_root,
            root,
            runtime_src: runtime_src.clone(),
            staged_rust_modules: count_files_recursive(&runtime_src),
            available,
            stage_error,
        }
    }

    pub fn status_label(&self) -> &'static str {
        if self.stage_error.is_some() {
            "挂接失败"
        } else if self.available {
            "已挂接"
        } else {
            "缺失"
        }
    }
}

pub fn runtime_assets_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("runtime-assets")
}

pub fn staged_runtime_root_path() -> PathBuf {
    crate::runtime_paths::runtime_root_dir()
}

pub fn runtime_src_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("runtime")
        .join("src")
}

fn stage_runtime_assets(source_root: &Path, staged_root: &Path) -> Result<(), String> {
    if !source_root.is_dir() {
        return Err(format!(
            "runtime asset source missing: {}",
            source_root.display()
        ));
    }
    copy_dir_recursive(source_root, staged_root)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            copy_file_preserving_permissions(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn copy_file_preserving_permissions(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::copy(source, destination).map_err(|error| error.to_string())?;
    let permissions = fs::metadata(source)
        .map_err(|error| error.to_string())?
        .permissions();
    fs::set_permissions(destination, permissions).map_err(|error| error.to_string())
}

fn count_files(path: PathBuf) -> usize {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };

    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|file_type| file_type.is_file())
                .unwrap_or(false)
        })
        .count()
}

fn count_files_recursive(path: &Path) -> usize {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                count_files_recursive(&path)
            } else {
                1
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn stage_runtime_assets_copies_nested_files() {
        let temp =
            std::env::temp_dir().join(format!("codux-gpui-runtime-stage-{}", Uuid::new_v4()));
        let source = temp.join("source");
        let target = temp.join("target");
        fs::create_dir_all(source.join("scripts/wrappers/bin")).unwrap();
        fs::create_dir_all(source.join("scripts/shell-hooks/zsh")).unwrap();
        fs::write(source.join("scripts/wrappers/bin/codex"), "#!/bin/sh\n").unwrap();
        fs::write(
            source.join("scripts/shell-hooks/zsh/.zshrc"),
            "source hook\n",
        )
        .unwrap();

        stage_runtime_assets(&source, &target).unwrap();

        assert!(target.join("scripts/wrappers/bin/codex").is_file());
        assert!(target.join("scripts/shell-hooks/zsh/.zshrc").is_file());
        assert_eq!(count_files_recursive(&target), 2);

        fs::remove_dir_all(temp).unwrap();
    }
}
