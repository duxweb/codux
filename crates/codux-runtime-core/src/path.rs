//! Cross-platform path primitives shared by the file picker, the remote browser
//! and the breadcrumb.
//!
//! These are built on the `typed-path` crate rather than `std::path` on purpose:
//! `std::path` only understands the *compile-target* platform's conventions, so
//! a Mac/Linux host parses `C:\Users\dux` as a single opaque component (a back
//! slash is a legal POSIX filename char and `C:` isn't a recognised prefix). The
//! desktop is frequently a controller browsing a *Windows* host's paths, so we
//! need OS-independent parsing of either flavour — exactly what `typed-path`'s
//! `Utf8TypedPath::derive` (sniff the flavour from the string) provides.

#[cfg(unix)]
use std::fs;
use std::path::Path;
use typed_path::{Utf8Component, Utf8TypedPath, Utf8UnixComponent, Utf8WindowsComponent};

/// Synthetic path that asks `file_list_payload` for the list of volumes/drives
/// instead of a real directory. On Windows this returns each mounted drive
/// (`C:`, `D:`, …) so the picker can hop between volumes; elsewhere it returns
/// the filesystem root. Kept as an exact-match sentinel so it can never collide
/// with a real absolute path.
pub const FILE_LIST_DRIVES_SENTINEL: &str = ":drives:";

/// Whether `path` uses Windows conventions (a drive/UNC prefix or leading
/// backslash), decided by `typed-path` independent of the host OS.
pub fn is_windows_path(path: &str) -> bool {
    matches!(Utf8TypedPath::derive(path), Utf8TypedPath::Windows(_))
}

/// Join `name` onto directory `parent` using the directory's own separator, so a
/// Windows `C:\…` directory stays back-slashed and a POSIX directory stays
/// forward-slashed instead of gaining a stray separator of the wrong kind.
pub fn join_path(parent: &str, name: &str) -> String {
    Utf8TypedPath::derive(parent)
        .join(name)
        .normalize()
        .into_string()
}

/// Return the parent using the path's own platform rules, independent of the
/// platform running this code.
pub fn parent_path(path: &str) -> Option<String> {
    Utf8TypedPath::derive(path)
        .parent()
        .map(|parent| parent.normalize().into_string())
}

/// Return the final component using the path's own platform rules.
pub fn file_name(path: &str) -> Option<String> {
    Utf8TypedPath::derive(path).file_name().map(str::to_string)
}

/// Normalize either Windows or POSIX syntax without consulting the local
/// filesystem. This is the canonical representation boundary for paths carried
/// over the controller/runtime protocols.
pub fn normalize_path_syntax(path: &str) -> Option<String> {
    let path = display_path(path.trim());
    if path.is_empty() {
        return None;
    }
    Some(Utf8TypedPath::derive(&path).normalize().into_string())
}

/// Stable equality key for a path whose platform is encoded in the path text.
/// Windows paths compare case-insensitively; POSIX paths preserve case.
pub fn path_identity_key(path: &str) -> Option<String> {
    let normalized = normalize_path_syntax(path)?;
    if is_windows_path(&normalized) {
        Some(normalized.replace('\\', "/").to_ascii_lowercase())
    } else {
        Some(normalized)
    }
}

pub fn paths_equal(left: &str, right: &str) -> bool {
    match (path_identity_key(left), path_identity_key(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// Normalize a path that belongs to this process' filesystem. Existing paths
/// resolve symlinks first; missing paths still receive host-native syntax
/// normalization.
pub fn normalize_local_path(path: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let normalized = resolved.components().collect::<std::path::PathBuf>();
    #[cfg(windows)]
    {
        display_path(&normalized.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        normalized.to_string_lossy().to_string()
    }
}

pub fn local_path_identity_key(path: &Path) -> Option<String> {
    let path = normalize_local_path(path);
    if path.is_empty() {
        return None;
    }
    #[cfg(windows)]
    {
        return Some(path.replace('\\', "/").to_ascii_lowercase());
    }
    #[cfg(not(windows))]
    {
        Some(path)
    }
}

/// Compare two paths on this process' filesystem, including hard-link identity
/// on Unix and case-insensitive path identity on Windows.
pub fn local_paths_equal(left: &Path, right: &Path) -> bool {
    if same_file_metadata(left, right) {
        return true;
    }
    match (
        local_path_identity_key(left),
        local_path_identity_key(right),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

pub fn optional_local_path_equals(left: Option<&str>, right: &str) -> bool {
    left.is_some_and(|left| local_paths_equal(Path::new(left), Path::new(right)))
}

#[cfg(unix)]
fn same_file_metadata(left: &Path, right: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let Ok(left_metadata) = fs::metadata(left) else {
        return false;
    };
    let Ok(right_metadata) = fs::metadata(right) else {
        return false;
    };
    left_metadata.dev() == right_metadata.dev() && left_metadata.ino() == right_metadata.ino()
}

#[cfg(not(unix))]
fn same_file_metadata(_left: &Path, _right: &Path) -> bool {
    false
}

/// Return `path` relative to `root` while preserving the path's separator
/// style. Both paths are parsed using the target path's conventions.
pub fn relative_path(root: &str, path: &str) -> Option<String> {
    let root = normalize_path_syntax(root)?;
    let path = normalize_path_syntax(path)?;
    match (Utf8TypedPath::derive(&root), Utf8TypedPath::derive(&path)) {
        (Utf8TypedPath::Windows(root), Utf8TypedPath::Windows(path)) => relative_components(
            root.components().map(|component| component.as_str()),
            path.components().map(|component| component.as_str()),
            true,
            "\\",
        ),
        (Utf8TypedPath::Unix(root), Utf8TypedPath::Unix(path)) => relative_components(
            root.components().map(|component| component.as_str()),
            path.components().map(|component| component.as_str()),
            false,
            "/",
        ),
        _ => None,
    }
}

fn relative_components<'a>(
    root: impl Iterator<Item = &'a str>,
    path: impl Iterator<Item = &'a str>,
    case_insensitive: bool,
    separator: &str,
) -> Option<String> {
    let root = root.collect::<Vec<_>>();
    let path = path.collect::<Vec<_>>();
    if root.len() > path.len()
        || !root.iter().zip(&path).all(|(left, right)| {
            if case_insensitive {
                left.eq_ignore_ascii_case(right)
            } else {
                left == right
            }
        })
    {
        return None;
    }
    Some(path[root.len()..].join(separator))
}

/// Strip Windows extended-length prefixes (`\\?\` and `\\?\UNC\`) so paths render
/// as the familiar `C:\…` / `\\server\…` forms in the file picker. Unconditional
/// (not gated on the running OS) so a POSIX peer browsing a Windows host can also
/// clean a stray prefix; a no-op for paths without one.
pub fn display_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix("//?/UNC/") {
        return format!("//{rest}");
    }
    path.strip_prefix(r"\\?\")
        .or_else(|| path.strip_prefix("//?/"))
        .unwrap_or(path)
        .to_string()
}

/// Step-up target from a volume root: the drive list on Windows so other volumes
/// stay reachable, empty (no `..`) on POSIX where `/` is already the top. Decided
/// by the host OS because it runs where the listing is produced.
pub fn drive_root_parent() -> String {
    #[cfg(target_os = "windows")]
    {
        FILE_LIST_DRIVES_SENTINEL.to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        String::new()
    }
}

/// Split an absolute path into a root navigate target plus per-level
/// `(label, target)` breadcrumb segments. Uses `typed-path` components so Windows
/// drive / back-slash / UNC paths and POSIX paths are all split correctly on any
/// host OS. The root target is the drive list on Windows (so other volumes are
/// reachable) and `/` on POSIX; the drive-list view itself yields no segments.
pub fn breadcrumb_segments(path: &str) -> (String, Vec<(String, String)>) {
    let trimmed = path.trim();
    if trimmed == FILE_LIST_DRIVES_SENTINEL {
        return (FILE_LIST_DRIVES_SENTINEL.to_string(), Vec::new());
    }
    match Utf8TypedPath::derive(trimmed) {
        Utf8TypedPath::Windows(path) => {
            let mut segments = Vec::new();
            let mut cumulative = String::new();
            for component in path.components() {
                let text = component.as_str();
                match component {
                    // The drive (`C:`) or UNC share is the first crumb; it
                    // navigates to that volume's root (`C:\`).
                    Utf8WindowsComponent::Prefix(_) => {
                        cumulative = text.to_string();
                        segments.push((text.to_string(), format!("{text}\\")));
                    }
                    Utf8WindowsComponent::Normal(_) => {
                        cumulative = format!("{}\\{text}", cumulative.trim_end_matches('\\'));
                        segments.push((text.to_string(), cumulative.clone()));
                    }
                    // RootDir is folded into the drive crumb's target above;
                    // CurDir/ParentDir don't occur in absolute display paths.
                    _ => {}
                }
            }
            (FILE_LIST_DRIVES_SENTINEL.to_string(), segments)
        }
        Utf8TypedPath::Unix(path) => {
            let absolute = path.is_absolute();
            let mut cumulative = String::new();
            let mut segments = Vec::new();
            for component in path.components() {
                if let Utf8UnixComponent::Normal(name) = component {
                    cumulative = if absolute {
                        format!("{}/{name}", cumulative.trim_end_matches('/'))
                    } else if cumulative.is_empty() {
                        name.to_string()
                    } else {
                        format!("{cumulative}/{name}")
                    };
                    segments.push((name.to_string(), cumulative.clone()));
                }
            }
            ("/".to_string(), segments)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_windows_paths() {
        assert!(is_windows_path(r"C:\Users"));
        assert!(is_windows_path(r"\\server\share"));
        assert!(!is_windows_path("/Users/dux"));
        assert!(!is_windows_path("relative/posix"));
    }

    #[test]
    fn joins_with_native_separator() {
        assert_eq!(join_path(r"C:\Users\dux", "proj"), r"C:\Users\dux\proj");
        assert_eq!(join_path(r"C:\", "Users"), r"C:\Users");
        assert_eq!(join_path("/Users/dux", "proj"), "/Users/dux/proj");
        assert_eq!(join_path("/", "Users"), "/Users");
    }

    #[test]
    fn parses_parent_and_file_name_for_both_platforms() {
        assert_eq!(
            parent_path(r"C:\Users\dux\notes.txt").as_deref(),
            Some(r"C:\Users\dux")
        );
        assert_eq!(
            parent_path("/Users/dux/notes.txt").as_deref(),
            Some("/Users/dux")
        );
        assert_eq!(
            file_name(r"C:\Users\dux\notes.txt").as_deref(),
            Some("notes.txt")
        );
        assert_eq!(
            file_name("/Users/dux/notes.txt").as_deref(),
            Some("notes.txt")
        );
    }

    #[test]
    fn identity_keys_follow_the_paths_platform() {
        assert!(paths_equal(
            r"\\?\C:\Users\Dux\project\",
            "c:/users/dux/project"
        ));
        assert!(paths_equal(
            r"\\?\UNC\server\share\project",
            "//SERVER/share/project"
        ));
        assert!(!paths_equal("/repo/Project", "/repo/project"));
        assert!(!paths_equal("/repo/project", "/repo/project-child"));
    }

    #[cfg(unix)]
    #[test]
    fn local_identity_preserves_posix_backslashes() {
        assert!(!local_paths_equal(
            Path::new(r"/repo/project\name"),
            Path::new("/repo/project/name")
        ));
    }

    #[test]
    fn syntax_normalization_resolves_dot_segments() {
        assert_eq!(
            normalize_path_syntax(r"C:\Users\Dux\..\Project").as_deref(),
            Some(r"C:\Users\Project")
        );
        assert_eq!(
            normalize_path_syntax("/repo/./src/../README.md").as_deref(),
            Some("/repo/README.md")
        );
    }

    #[test]
    fn strips_roots_for_both_platforms() {
        assert_eq!(
            relative_path(r"C:\Users\dux", r"C:\Users\dux\src\main.rs").as_deref(),
            Some(r"src\main.rs")
        );
        assert_eq!(
            relative_path("/Users/dux", "/Users/dux/src/main.rs").as_deref(),
            Some("src/main.rs")
        );
        assert_eq!(relative_path(r"C:\Users\dux", r"D:\src\main.rs"), None);
        assert_eq!(
            relative_path(r"C:\Users\Dux", r"c:/users/dux/Src/Main.rs").as_deref(),
            Some(r"Src\Main.rs")
        );
        assert_eq!(relative_path("/repo/App", "/repo/app/main.rs"), None);
    }

    #[test]
    fn strips_extended_length_prefix() {
        assert_eq!(display_path(r"\\?\C:\Users\dux"), r"C:\Users\dux");
        assert_eq!(display_path(r"\\?\UNC\server\share"), r"\\server\share");
        assert_eq!(display_path("/Users/dux"), "/Users/dux");
        assert_eq!(display_path(r"C:\Users\dux"), r"C:\Users\dux");
        assert_eq!(display_path("//?/C:/Users/dux"), "C:/Users/dux");
        assert_eq!(display_path("//?/UNC/server/share"), "//server/share");
    }

    #[test]
    fn breadcrumb_posix_splits_on_forward_slash() {
        let (root, segments) = breadcrumb_segments("/Users/dux/project");
        assert_eq!(root, "/");
        assert_eq!(
            segments,
            vec![
                ("Users".to_string(), "/Users".to_string()),
                ("dux".to_string(), "/Users/dux".to_string()),
                ("project".to_string(), "/Users/dux/project".to_string()),
            ]
        );
    }

    #[test]
    fn breadcrumb_windows_drive_path_splits_into_crumbs() {
        let (root, segments) = breadcrumb_segments(r"C:\Users\dux");
        assert_eq!(root, FILE_LIST_DRIVES_SENTINEL);
        assert_eq!(
            segments,
            vec![
                ("C:".to_string(), r"C:\".to_string()),
                ("Users".to_string(), r"C:\Users".to_string()),
                ("dux".to_string(), r"C:\Users\dux".to_string()),
            ]
        );
    }

    #[test]
    fn breadcrumb_windows_forward_slash_also_splits() {
        let (_root, segments) = breadcrumb_segments("C:/Users/dux");
        let targets: Vec<String> = segments.into_iter().map(|(_, target)| target).collect();
        assert_eq!(
            targets,
            vec![
                r"C:\".to_string(),
                r"C:\Users".to_string(),
                r"C:\Users\dux".to_string()
            ]
        );
    }

    #[test]
    fn breadcrumb_drive_list_view_has_no_segments() {
        let (root, segments) = breadcrumb_segments(FILE_LIST_DRIVES_SENTINEL);
        assert_eq!(root, FILE_LIST_DRIVES_SENTINEL);
        assert!(segments.is_empty());
    }
}
