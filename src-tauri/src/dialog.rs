use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogFilter {
    #[serde(rename = "name")]
    pub _name: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedOpenDialogRequest {
    pub title: String,
    pub message: String,
    pub prompt: String,
    pub default_path: Option<String>,
    pub filters: Vec<DialogFilter>,
    pub directory: bool,
    pub multiple: bool,
    pub can_create_directories: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedSaveDialogRequest {
    pub title: String,
    pub message: String,
    pub prompt: String,
    pub default_path: Option<String>,
    pub filters: Vec<DialogFilter>,
    pub can_create_directories: Option<bool>,
}

#[cfg(target_os = "macos")]
pub fn localized_open_dialog(request: LocalizedOpenDialogRequest) -> Result<Option<Vec<String>>, String> {
    macos::open_dialog(request)
}

#[cfg(not(target_os = "macos"))]
pub fn localized_open_dialog(_request: LocalizedOpenDialogRequest) -> Result<Option<Vec<String>>, String> {
    Err("localized open dialog is only implemented on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub fn localized_save_dialog(request: LocalizedSaveDialogRequest) -> Result<Option<String>, String> {
    macos::save_dialog(request)
}

#[cfg(not(target_os = "macos"))]
pub fn localized_save_dialog(_request: LocalizedSaveDialogRequest) -> Result<Option<String>, String> {
    Err("localized save dialog is only implemented on macOS".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{DialogFilter, LocalizedOpenDialogRequest, LocalizedSaveDialogRequest};
    use dispatch2::DispatchQueue;
    use objc2::{rc::autoreleasepool, MainThreadMarker};
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSSavePanel};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::path::Path;

    pub fn open_dialog(request: LocalizedOpenDialogRequest) -> Result<Option<Vec<String>>, String> {
        run_on_main(move |marker| {
            autoreleasepool(|_| {
                let panel = NSOpenPanel::openPanel(marker);
                configure_save_panel(&panel, &request.title, &request.message, &request.prompt, request.default_path.as_deref(), &request.filters, request.can_create_directories);
                panel.setCanChooseDirectories(request.directory);
                panel.setCanChooseFiles(!request.directory);
                panel.setAllowsMultipleSelection(request.multiple);
                let response = panel.runModal();
                if response != NSModalResponseOK {
                    return Ok(None);
                }
                let urls = panel.URLs();
                let mut paths = Vec::new();
                for url in urls.iter() {
                    if let Some(path) = url.to_file_path() {
                        paths.push(path.to_string_lossy().into_owned());
                    }
                }
                Ok(Some(paths))
            })
        })
    }

    pub fn save_dialog(request: LocalizedSaveDialogRequest) -> Result<Option<String>, String> {
        run_on_main(move |marker| {
            autoreleasepool(|_| {
                let panel = NSSavePanel::savePanel(marker);
                configure_save_panel(&panel, &request.title, &request.message, &request.prompt, request.default_path.as_deref(), &request.filters, request.can_create_directories);
                let response = panel.runModal();
                if response != NSModalResponseOK {
                    return Ok(None);
                }
                Ok(panel
                    .URL()
                    .and_then(|url| url.to_file_path())
                    .map(|path| path.to_string_lossy().into_owned()))
            })
        })
    }

    fn run_on_main<R, F>(f: F) -> R
    where
        R: Send + 'static,
        F: FnOnce(MainThreadMarker) -> R + Send + 'static,
    {
        if let Some(marker) = MainThreadMarker::new() {
            return f(marker);
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        DispatchQueue::main().exec_sync(move || {
            let marker = unsafe { MainThreadMarker::new_unchecked() };
            let _ = sender.send(f(marker));
        });
        receiver.recv().expect("main queue did not return a dialog result")
    }

    fn configure_save_panel(
        panel: &NSSavePanel,
        title: &str,
        message: &str,
        prompt: &str,
        default_path: Option<&str>,
        filters: &[DialogFilter],
        can_create_directories: Option<bool>,
    ) {
        if !title.trim().is_empty() {
            panel.setTitle(Some(&NSString::from_str(title)));
        }
        if !message.trim().is_empty() {
            panel.setMessage(Some(&NSString::from_str(message)));
        }
        if !prompt.trim().is_empty() {
            panel.setPrompt(Some(&NSString::from_str(prompt)));
        }
        if let Some(can_create) = can_create_directories {
            panel.setCanCreateDirectories(can_create);
        }
        if let Some(path) = default_path {
            apply_default_path(panel, path);
        }
        apply_filters(panel, filters);
    }

    fn apply_default_path(panel: &NSSavePanel, path: &str) {
        let path = Path::new(path);
        if path.is_dir() {
            if let Some(url) = NSURL::from_directory_path(path) {
                panel.setDirectoryURL(Some(&url));
            }
            return;
        }
        if let Some(parent) = path.parent() {
            if let Some(url) = NSURL::from_directory_path(parent) {
                panel.setDirectoryURL(Some(&url));
            }
        }
        if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
            panel.setNameFieldStringValue(&NSString::from_str(file_name));
        }
    }

    fn apply_filters(panel: &NSSavePanel, filters: &[DialogFilter]) {
        let extensions = filters
            .iter()
            .flat_map(|filter| filter.extensions.iter())
            .filter(|extension| !extension.trim().is_empty())
            .map(|extension| NSString::from_str(extension.trim().trim_start_matches('.')))
            .collect::<Vec<_>>();
        if extensions.is_empty() {
            return;
        }
        let allowed = NSArray::from_retained_slice(&extensions);
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&allowed));
    }
}
