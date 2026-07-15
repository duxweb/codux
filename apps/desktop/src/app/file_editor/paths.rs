use super::*;

pub(super) fn file_editor_i18n(
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut impl AppContext,
    key: &str,
    fallback: &str,
) -> String {
    cx.update_entity(&app_entity, |app, _cx| {
        let locale = locale_from_language_setting(&app.state.settings.language);
        translate(&locale, key, fallback)
    })
}

pub(super) fn file_editor_label(relative_path: &str) -> String {
    Path::new(relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(relative_path)
        .to_string()
}

pub(in crate::app) fn file_editor_window_title(relative_path: &str) -> String {
    file_editor_label(relative_path)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum FilePreviewKind {
    Text,
    Markdown,
    Image,
    External,
}

pub(in crate::app) fn file_preview_kind_for_path(relative_path: &str) -> FilePreviewKind {
    let extension = Path::new(relative_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "apng" | "avif" | "bmp" | "gif" | "heic" | "heif" | "ico" | "jpeg" | "jpg" | "jxl"
        | "png" | "svg" | "svgz" | "tif" | "tiff" | "webp" => FilePreviewKind::Image,
        "md" | "markdown" => FilePreviewKind::Markdown,
        "3gp" | "7z" | "aac" | "aif" | "aiff" | "avi" | "dmg" | "doc" | "docx" | "eot" | "exe"
        | "flac" | "gz" | "jar" | "m4a" | "m4v" | "mkv" | "mov" | "mp3" | "mp4" | "mpeg"
        | "mpg" | "ogg" | "otf" | "pdf" | "pkg" | "ppt" | "pptx" | "rar" | "tar" | "ttf"
        | "wav" | "webm" | "woff" | "woff2" | "xls" | "xlsx" | "zip" => FilePreviewKind::External,
        _ => FilePreviewKind::Text,
    }
}

pub(super) fn file_language_for_path(relative_path: &str) -> &'static str {
    let path = Path::new(relative_path);
    let file_name = path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match file_name.as_str() {
        "makefile" => return "make",
        "cmakelists.txt" => return "cmake",
        _ => {}
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "rs" => "rust",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "tsx" => "tsx",
        "jsx" => "javascript",
        "json" | "jsonc" => "json",
        "md" | "markdown" | "mdx" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "html" | "htm" | "vue" | "xml" | "xhtml" => "html",
        "css" | "scss" | "sass" | "less" => "css",
        "sh" | "bash" | "zsh" => "bash",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "ex" | "exs" => "elixir",
        "graphql" | "gql" => "graphql",
        "kt" | "kts" | "ktm" => "kotlin",
        "php" | "php3" | "php4" | "php5" | "phtml" => "php",
        "proto" => "proto",
        "rb" => "ruby",
        "scala" => "scala",
        "svelte" => "svelte",
        "swift" => "swift",
        "lua" => "lua",
        "zig" => "zig",
        "sql" => "sql",
        "diff" | "patch" => "diff",
        "cmake" => "cmake",
        "make" | "mk" => "make",
        "ejs" => "ejs",
        "erb" => "erb",
        "astro" => "astro",
        _ => "text",
    }
}

pub(super) fn changed_file_event_relative_paths(
    events: &[FileChangeEvent],
    worktree_path: &str,
) -> HashSet<String> {
    events
        .iter()
        .flat_map(|event| event.changed_paths.iter())
        .filter_map(|path| relative_file_watch_path(worktree_path, path))
        .collect()
}

fn relative_file_watch_path(worktree: &str, changed_path: &str) -> Option<String> {
    codux_runtime::path::relative_path(worktree, changed_path)
        .filter(|relative| !relative.is_empty())
        .map(file_watch_relative_path_display)
}

fn file_watch_relative_path_display(path: String) -> String {
    #[cfg(windows)]
    {
        path.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        path
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    use super::relative_file_watch_path;
    use super::{
        FilePreviewKind, changed_file_event_relative_paths, file_language_for_path,
        file_preview_kind_for_path, file_watch_relative_path_display,
    };
    use codux_runtime::files::FileChangeEvent;

    #[test]
    fn file_preview_kind_detects_images_without_treating_markdown_as_image() {
        assert_eq!(
            file_preview_kind_for_path("assets/logo.png"),
            FilePreviewKind::Image
        );
        assert_eq!(
            file_preview_kind_for_path("README.md"),
            FilePreviewKind::Markdown
        );
        assert_eq!(
            file_preview_kind_for_path("src/main.rs"),
            FilePreviewKind::Text
        );
        assert_eq!(
            file_preview_kind_for_path("demo.mp4"),
            FilePreviewKind::External
        );
        assert_eq!(
            file_preview_kind_for_path("report.pdf"),
            FilePreviewKind::External
        );
    }

    #[test]
    fn file_language_for_path_maps_supported_highlight_languages() {
        let cases = [
            ("src/main.rs", "rust"),
            ("src/app.ts", "typescript"),
            ("src/app.tsx", "tsx"),
            ("src/App.svelte", "svelte"),
            ("src/App.vue", "html"),
            ("src/page.astro", "astro"),
            ("src/main.kt", "kotlin"),
            ("src/index.php", "php"),
            ("src/schema.graphql", "graphql"),
            ("src/view.erb", "erb"),
            ("src/view.ejs", "ejs"),
            ("src/lib.rb", "ruby"),
            ("src/query.sql", "sql"),
            ("src/change.patch", "diff"),
            ("src/layout.xml", "html"),
            ("Makefile", "make"),
            ("CMakeLists.txt", "cmake"),
        ];

        for (path, language) in cases {
            assert_eq!(file_language_for_path(path), language, "{path}");
        }
    }

    #[test]
    fn file_watch_paths_match_current_worktree_only() {
        let events = vec![FileChangeEvent {
            project_path: "/tmp/project".to_string(),
            changed_paths: vec![
                "/tmp/project/src/main.rs".to_string(),
                "/tmp/project-b/src/main.rs".to_string(),
                "/tmp/project".to_string(),
            ],
        }];
        let paths = changed_file_event_relative_paths(&events, "/tmp/project");

        assert!(paths.contains("src/main.rs"));
        assert!(!paths.contains("../project-b/src/main.rs"));
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn file_watch_paths_normalize_windows_separators() {
        let relative =
            codux_runtime::path::relative_path("C:/Work/App", "c:\\work\\app\\src\\README.md")
                .expect("relative Windows path");
        #[cfg(windows)]
        assert_eq!(file_watch_relative_path_display(relative), "src/README.md");
        #[cfg(not(windows))]
        assert_eq!(file_watch_relative_path_display(relative), r"src\README.md");
    }

    #[cfg(not(windows))]
    #[test]
    fn file_watch_paths_preserve_posix_backslashes() {
        assert_eq!(
            relative_file_watch_path("/repo", r"/repo/file\name"),
            Some(r"file\name".to_string())
        );
    }
}
