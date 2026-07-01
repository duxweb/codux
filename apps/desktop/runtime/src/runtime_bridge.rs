use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::ai_runtime::stage_runtime_asset;

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
    runtime_assets_candidates()
        .into_iter()
        .find(|path| path.is_dir())
        .unwrap_or_else(|| runtime_package_dir().join("../runtime-assets"))
}

pub fn staged_runtime_root_path() -> PathBuf {
    crate::runtime_paths::runtime_root_dir()
}

pub fn runtime_src_path() -> PathBuf {
    runtime_src_candidates()
        .into_iter()
        .find(|path| path.is_dir())
        .unwrap_or_else(|| runtime_package_dir().join("src"))
}

fn runtime_assets_candidates() -> Vec<PathBuf> {
    let runtime_package = runtime_package_dir();
    let desktop_root = runtime_package
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| runtime_package.clone());
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = current_exe_dir();
    let mut candidates = vec![
        desktop_root.join("runtime-assets"),
        current_dir.join("apps/desktop/runtime-assets"),
        current_dir.join("runtime-assets"),
    ];
    if let Some(exe_dir) = exe_dir {
        candidates.extend([
            exe_dir.join("runtime-assets"),
            exe_dir.join("../Resources/runtime-assets"),
            exe_dir.join("../Resources/runtime-root"),
        ]);
    }
    candidates
}

fn runtime_src_candidates() -> Vec<PathBuf> {
    let runtime_package = runtime_package_dir();
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    vec![
        runtime_package.join("src"),
        current_dir.join("apps/desktop/runtime/src"),
        current_dir.join("runtime/src"),
    ]
}

fn runtime_package_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn current_exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn stage_runtime_assets(source_root: &Path, staged_root: &Path) -> Result<(), String> {
    if !source_root.is_dir() {
        return Err(format!(
            "runtime asset source missing: {}",
            source_root.display()
        ));
    }
    copy_dir_recursive(source_root, staged_root)?;
    stage_embedded_runtime_bootstrap_assets(staged_root)
}

fn stage_embedded_runtime_bootstrap_assets(staged_root: &Path) -> Result<(), String> {
    for relative_path in [
        "scripts/shell-hooks/dmux-ai-hook.zsh",
        "scripts/shell-hooks/zsh/.zlogin",
        "scripts/shell-hooks/zsh/.zprofile",
        "scripts/shell-hooks/zsh/.zshenv",
        "scripts/shell-hooks/zsh/.zshrc",
    ] {
        stage_runtime_asset(relative_path, &staged_root.join(relative_path), false)?;
    }
    stage_wrapper_helper(staged_root)?;
    Ok(())
}

fn stage_wrapper_helper(staged_root: &Path) -> Result<(), String> {
    let helper_path = staged_root.join("scripts/wrappers/codux-wrapper-helper");
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    write_if_changed_from_file(&helper_path, &current_exe)?;
    set_executable(&helper_path);
    Ok(())
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

#[cfg(unix)]
fn write_if_changed_from_file(destination: &Path, source: &Path) -> Result<(), String> {
    use std::os::unix::fs::symlink;

    if matches!(fs::read_link(destination), Ok(existing) if existing == source) {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let tmp = destination.with_extension(format!(
        "{}tmp",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ));
    let _ = fs::remove_file(&tmp);
    symlink(source, &tmp)
        .or_else(|_| fs::copy(source, &tmp).map(|_| ()))
        .map_err(|error| error.to_string())?;
    fs::rename(&tmp, destination).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn write_if_changed_from_file(destination: &Path, source: &Path) -> Result<(), String> {
    if matches!(fs::read(destination), Ok(existing) if matches!(fs::read(source), Ok(source_bytes) if existing == source_bytes))
    {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return;
    }
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
}

#[cfg(windows)]
fn set_executable(_path: &Path) {}

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
        assert!(target.join("scripts/shell-hooks/zsh/.zshenv").is_file());
        assert!(target.join("scripts/shell-hooks/zsh/.zprofile").is_file());
        assert!(
            target
                .join("scripts/shell-hooks/dmux-ai-hook.zsh")
                .is_file()
        );
        assert!(
            target
                .join("scripts/wrappers/codux-wrapper-helper")
                .exists()
        );

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn stage_runtime_assets_repairs_embedded_shell_hooks_when_source_omits_dotfiles() {
        let temp = std::env::temp_dir().join(format!(
            "codux-gpui-runtime-stage-bootstrap-{}",
            Uuid::new_v4()
        ));
        let source = temp.join("source");
        let target = temp.join("target");
        fs::create_dir_all(source.join("scripts/shell-hooks/zsh")).unwrap();

        stage_runtime_assets(&source, &target).unwrap();

        assert!(target.join("scripts/shell-hooks/zsh/.zshenv").is_file());
        assert!(target.join("scripts/shell-hooks/zsh/.zprofile").is_file());
        assert!(target.join("scripts/shell-hooks/zsh/.zshrc").is_file());
        assert!(target.join("scripts/shell-hooks/zsh/.zlogin").is_file());
        assert!(
            target
                .join("scripts/shell-hooks/dmux-ai-hook.zsh")
                .is_file()
        );

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn runtime_assets_path_resolves_after_desktop_move() {
        let path = runtime_assets_path();

        assert!(path.ends_with("apps/desktop/runtime-assets"));
        assert!(path.is_dir());
    }
}
