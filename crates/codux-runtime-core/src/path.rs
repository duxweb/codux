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

use typed_path::{Utf8Component, Utf8TypedPath, Utf8UnixComponent, Utf8WindowsComponent};

/// Synthetic path that asks `file_list_payload` for the list of volumes/drives
/// instead of a real directory. On Windows this returns each mounted drive
/// (`C:`, `D:`, …) so the picker can hop between volumes; elsewhere it returns
/// the filesystem root. Kept as an exact-match sentinel so it can never collide
/// with a real absolute path.
pub const FILE_LIST_DRIVES_SENTINEL: &str = ":drives:";

/// Whether `path` uses Windows conventions (drive-letter root or back-slashes),
/// decided by `typed-path`'s content sniffing independent of the host OS.
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

/// Return `path` relative to `root` while preserving the path's separator
/// style. Both paths are parsed using the target path's conventions.
pub fn relative_path(root: &str, path: &str) -> Option<String> {
    Utf8TypedPath::derive(path)
        .strip_prefix(root)
        .ok()
        .map(|relative| {
            relative
                .as_str()
                .trim_start_matches(['/', '\\'])
                .to_string()
        })
}

/// Strip Windows extended-length prefixes (`\\?\` and `\\?\UNC\`) so paths render
/// as the familiar `C:\…` / `\\server\…` forms in the file picker. Unconditional
/// (not gated on the running OS) so a POSIX peer browsing a Windows host can also
/// clean a stray prefix; a no-op for paths without one.
pub fn display_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
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
    }

    #[test]
    fn strips_extended_length_prefix() {
        assert_eq!(display_path(r"\\?\C:\Users\dux"), r"C:\Users\dux");
        assert_eq!(display_path(r"\\?\UNC\server\share"), r"\\server\share");
        assert_eq!(display_path("/Users/dux"), "/Users/dux");
        assert_eq!(display_path(r"C:\Users\dux"), r"C:\Users\dux");
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
