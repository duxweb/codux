use super::*;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn lists_nested_children_and_blocks_root_escape() {
    let root = temp_dir("files");
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::write(root.join("src").join("main.rs"), "fn main() {}\n").expect("main file");
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::write(root.join(".git").join("config"), "hidden").expect("hidden file");

    let top = FilesService::list_children(root.to_str().expect("root"), None).expect("top");
    assert!(top.iter().any(|entry| entry.name == "src"));
    assert!(!top.iter().any(|entry| entry.name == ".git"));

    let nested =
        FilesService::list_children(root.to_str().expect("root"), Some("src")).expect("nested");
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].relative_path, "src/main.rs");

    let preview =
        FilesService::read_text(root.to_str().expect("root"), "src/main.rs").expect("preview");
    assert!(preview.content.contains("fn main"));

    let escaped = FilesService::list_children(root.to_str().expect("root"), Some("../"));
    assert!(escaped.is_err());

    fs::remove_dir_all(root).ok();
}

#[test]
fn reads_project_symlinked_text_file() {
    let root = temp_dir("files-symlink-read");
    fs::write(root.join("memory.md"), "managed memory\n").expect("target file");
    create_symlink(root.join("memory.md"), root.join("AGENTS.md")).expect("symlink");

    let preview = FilesService::read_text(root.to_str().expect("root"), "AGENTS.md")
        .expect("read project symlink");
    assert_eq!(preview.relative_path, "AGENTS.md");
    assert!(preview.content.contains("managed memory"));

    fs::remove_dir_all(root).ok();
}

#[test]
fn creates_files_and_directories_with_root_guards() {
    let root = temp_dir("files-create");
    fs::create_dir_all(root.join("src")).expect("src dir");

    let file = FilesService::create_file(root.to_str().expect("root"), Some("src"), "note.txt")
        .expect("create file");
    assert_eq!(file.relative_path, "src/note.txt");
    assert!(root.join("src").join("note.txt").is_file());

    let directory = FilesService::create_dir(root.to_str().expect("root"), Some("src"), "nested")
        .expect("create dir");
    assert_eq!(directory.relative_path, "src/nested");
    assert!(root.join("src").join("nested").is_dir());

    assert!(
        FilesService::create_file(root.to_str().expect("root"), Some("src"), "note.txt").is_err()
    );
    assert!(
        FilesService::create_file(root.to_str().expect("root"), Some("src"), "../bad.txt").is_err()
    );
    assert!(FilesService::create_dir(root.to_str().expect("root"), Some("../"), "bad").is_err());

    fs::remove_dir_all(root).ok();
}

#[test]
fn writes_and_renames_text_files_safely() {
    let root = temp_dir("files-write");
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::write(root.join("src").join("note.txt"), "old\n").expect("note");
    fs::write(root.join("src").join("taken.txt"), "taken\n").expect("taken");

    let written = FilesService::write_text(root.to_str().expect("root"), "src/note.txt", "new\n")
        .expect("write file");
    assert_eq!(written.content, "new\n");
    assert_eq!(
        fs::read_to_string(root.join("src").join("note.txt")).expect("read written"),
        "new\n"
    );

    let renamed = FilesService::rename(root.to_str().expect("root"), "src/note.txt", "renamed.txt")
        .expect("rename file");
    assert_eq!(renamed.relative_path, "src/renamed.txt");
    assert!(root.join("src").join("renamed.txt").is_file());

    assert!(
        FilesService::rename(root.to_str().expect("root"), "src/renamed.txt", "taken.txt").is_err()
    );
    assert!(
        FilesService::rename(
            root.to_str().expect("root"),
            "src/renamed.txt",
            "../bad.txt"
        )
        .is_err()
    );
    assert!(FilesService::write_text(root.to_str().expect("root"), "src", "bad").is_err());

    fs::remove_dir_all(root).ok();
}

#[test]
fn moves_files_and_directories_to_existing_directory() {
    let root = temp_dir("files-move");
    fs::create_dir_all(root.join("src").join("nested")).expect("nested dir");
    fs::create_dir_all(root.join("docs")).expect("docs dir");
    fs::write(root.join("src").join("note.txt"), "hello\n").expect("note");

    let moved =
        FilesService::move_to_directory(root.to_str().expect("root"), "src/note.txt", "docs")
            .expect("move file");
    assert_eq!(moved.relative_path, "docs/note.txt");
    assert!(root.join("docs").join("note.txt").is_file());

    let moved_dir =
        FilesService::move_to_directory(root.to_str().expect("root"), "src/nested", "docs")
            .expect("move directory");
    assert_eq!(moved_dir.relative_path, "docs/nested");
    assert!(root.join("docs").join("nested").is_dir());

    assert!(
        FilesService::move_to_directory(root.to_str().expect("root"), "docs", "docs/nested")
            .is_err()
    );
    assert!(
        FilesService::move_to_directory(root.to_str().expect("root"), "docs/note.txt", "../")
            .is_err()
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn move_to_directory_overwrite_replaces_conflicting_entry() {
    let root = temp_dir("files-move-overwrite");
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::create_dir_all(root.join("docs")).expect("docs dir");
    fs::write(root.join("src").join("note.txt"), "new\n").expect("source note");
    fs::write(root.join("docs").join("note.txt"), "old\n").expect("target note");

    assert!(
        FilesService::move_to_directory(root.to_str().expect("root"), "src/note.txt", "docs")
            .is_err()
    );

    let moved = FilesService::move_to_directory_overwrite(
        root.to_str().expect("root"),
        "src/note.txt",
        "docs",
    )
    .expect("move with overwrite");
    assert_eq!(moved.relative_path, "docs/note.txt");
    assert_eq!(
        fs::read_to_string(root.join("docs").join("note.txt")).expect("read target"),
        "new\n"
    );
    assert!(!root.join("src").join("note.txt").exists());

    fs::remove_dir_all(root).ok();
}

#[test]
fn copies_files_and_directories_with_available_names() {
    let root = temp_dir("files-copy");
    fs::create_dir_all(root.join("src").join("nested")).expect("nested dir");
    fs::write(root.join("src").join("note.txt"), "hello\n").expect("note");
    fs::write(root.join("src").join("nested").join("child.txt"), "child\n").expect("child");

    let copied =
        FilesService::copy_to_directory(root.to_str().expect("root"), "src/note.txt", Some("src"))
            .expect("copy file");
    assert_eq!(copied.relative_path, "src/note copy 1.txt");
    assert_eq!(
        fs::read_to_string(root.join("src").join("note copy 1.txt")).expect("copied file"),
        "hello\n"
    );

    let copied_again =
        FilesService::copy_to_directory(root.to_str().expect("root"), "src/note.txt", Some("src"))
            .expect("copy file again");
    assert_eq!(copied_again.relative_path, "src/note copy 2.txt");

    let copied_dir =
        FilesService::copy_to_directory(root.to_str().expect("root"), "src/nested", Some("src"))
            .expect("copy directory");
    assert_eq!(copied_dir.relative_path, "src/nested copy 1");
    assert!(
        root.join("src")
            .join("nested copy 1")
            .join("child.txt")
            .is_file()
    );

    assert!(
        FilesService::copy_to_directory(
            root.to_str().expect("root"),
            "src/nested",
            Some("src/nested")
        )
        .is_err()
    );
    assert!(
        FilesService::copy_to_directory(
            root.to_str().expect("root"),
            "../outside.txt",
            Some("src")
        )
        .is_err()
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn writes_clipboard_image_bytes_with_available_name() {
    let root = temp_dir("files-clipboard-image");
    fs::create_dir_all(root.join("assets")).expect("assets dir");
    fs::write(root.join("assets").join("pasted-image.png"), [1, 2, 3]).expect("existing image");

    let pasted = FilesService::write_bytes_to_directory(
        root.to_str().expect("root"),
        Some("assets"),
        "pasted-image.png",
        &[4, 5, 6],
    )
    .expect("write image");

    assert_eq!(pasted.relative_path, "assets/pasted-image copy 1.png");
    assert_eq!(
        fs::read(root.join("assets").join("pasted-image copy 1.png")).expect("read pasted"),
        vec![4, 5, 6]
    );
    assert!(
        FilesService::write_bytes_to_directory(
            root.to_str().expect("root"),
            Some("../"),
            "bad.png",
            &[1]
        )
        .is_err()
    );
    assert!(
        FilesService::write_bytes_to_directory(
            root.to_str().expect("root"),
            Some("assets"),
            "../bad.png",
            &[1]
        )
        .is_err()
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn resolve_existing_paths_for_system_actions_stay_inside_root() {
    let root = temp_dir("files-system-actions");
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::write(root.join("src").join("note.txt"), "hello\n").expect("note");
    let canonical = root.canonicalize().expect("canonical root");

    let resolved = resolve_existing_path(&canonical, "src/note.txt").expect("resolve file");
    assert_eq!(resolved, canonical.join("src").join("note.txt"));
    assert!(resolve_existing_path(&canonical, "../outside.txt").is_err());
    assert!(resolve_existing_path(&canonical, "/tmp/outside.txt").is_err());

    fs::remove_dir_all(root).ok();
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("codux-gpui-{label}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[cfg(unix)]
fn create_symlink(
    original: impl AsRef<std::path::Path>,
    link: impl AsRef<std::path::Path>,
) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn create_symlink(
    original: impl AsRef<std::path::Path>,
    link: impl AsRef<std::path::Path>,
) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(original, link)
}
