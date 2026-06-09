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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedConfirmDialogRequest {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub cancel_label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedAlertDialogRequest {
    pub title: String,
    pub message: String,
    pub button_label: String,
}

#[cfg(target_os = "macos")]
pub fn localized_open_dialog(
    request: LocalizedOpenDialogRequest,
) -> Result<Option<Vec<String>>, String> {
    macos::open_dialog(request)
}

#[cfg(target_os = "windows")]
pub fn localized_open_dialog(
    request: LocalizedOpenDialogRequest,
) -> Result<Option<Vec<String>>, String> {
    native_open_dialog(request)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn localized_open_dialog(
    _request: LocalizedOpenDialogRequest,
) -> Result<Option<Vec<String>>, String> {
    Err("localized open dialog is only implemented on macOS and Windows".to_string())
}

#[cfg(target_os = "macos")]
pub fn localized_save_dialog(
    request: LocalizedSaveDialogRequest,
) -> Result<Option<String>, String> {
    macos::save_dialog(request)
}

#[cfg(target_os = "windows")]
pub fn localized_save_dialog(
    request: LocalizedSaveDialogRequest,
) -> Result<Option<String>, String> {
    native_save_dialog(request)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn localized_save_dialog(
    _request: LocalizedSaveDialogRequest,
) -> Result<Option<String>, String> {
    Err("localized save dialog is only implemented on macOS and Windows".to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn localized_confirm_dialog(request: LocalizedConfirmDialogRequest) -> Result<bool, String> {
    native_confirm_dialog(request)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn localized_confirm_dialog(_request: LocalizedConfirmDialogRequest) -> Result<bool, String> {
    Err("localized confirm dialog is only implemented on macOS and Windows".to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn localized_alert_dialog(request: LocalizedAlertDialogRequest) -> Result<(), String> {
    native_alert_dialog(request)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn localized_alert_dialog(_request: LocalizedAlertDialogRequest) -> Result<(), String> {
    Err("localized alert dialog is only implemented on macOS and Windows".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{DialogFilter, LocalizedOpenDialogRequest, LocalizedSaveDialogRequest};
    use dispatch2::DispatchQueue;
    use objc2::{MainThreadMarker, rc::autoreleasepool};
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSSavePanel};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::path::Path;

    pub fn open_dialog(request: LocalizedOpenDialogRequest) -> Result<Option<Vec<String>>, String> {
        run_on_main(move |marker| {
            autoreleasepool(|_| {
                let panel = NSOpenPanel::openPanel(marker);
                configure_open_panel(&panel, &request);
                let response = panel.runModal();
                if response != NSModalResponseOK {
                    return Ok(None);
                }
                Ok(Some(
                    panel
                        .URLs()
                        .iter()
                        .filter_map(|url| url.to_file_path())
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect(),
                ))
            })
        })
    }

    pub fn save_dialog(request: LocalizedSaveDialogRequest) -> Result<Option<String>, String> {
        run_on_main(move |marker| {
            autoreleasepool(|_| {
                let panel = NSSavePanel::savePanel(marker);
                configure_save_panel(
                    &panel,
                    &request.title,
                    &request.message,
                    &request.prompt,
                    request.default_path.as_deref(),
                    &request.filters,
                    request.can_create_directories,
                );
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
        receiver
            .recv()
            .expect("main queue did not return a dialog result")
    }

    fn configure_open_panel(panel: &NSOpenPanel, request: &LocalizedOpenDialogRequest) {
        configure_panel_text(
            panel,
            &request.title,
            &request.message,
            &request.prompt,
            request.can_create_directories,
        );
        panel.setCanChooseDirectories(request.directory);
        panel.setCanChooseFiles(!request.directory);
        panel.setAllowsMultipleSelection(request.multiple);
        if let Some(path) = request.default_path.as_deref() {
            apply_default_path(panel, path);
        }
        apply_filters(panel, &request.filters);
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
        configure_panel_text(panel, title, message, prompt, can_create_directories);
        if let Some(path) = default_path {
            apply_default_path(panel, path);
        }
        apply_filters(panel, filters);
    }

    fn configure_panel_text(
        panel: &NSSavePanel,
        title: &str,
        message: &str,
        prompt: &str,
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

#[cfg(target_os = "windows")]
fn native_open_dialog(request: LocalizedOpenDialogRequest) -> Result<Option<Vec<String>>, String> {
    let mut dialog = rfd::FileDialog::new();
    if !request.title.trim().is_empty() {
        dialog = dialog.set_title(request.title.trim());
    }
    if let Some(default_path) = request.default_path.as_deref() {
        let default_path = default_path.trim();
        if !default_path.is_empty() {
            dialog = dialog.set_directory(default_path);
        }
    }
    if let Some(can_create_directories) = request.can_create_directories {
        dialog = dialog.set_can_create_directories(can_create_directories);
    }
    for filter in &request.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(|extension| extension.trim().trim_start_matches('.'))
            .filter(|extension| !extension.is_empty())
            .collect::<Vec<_>>();
        if !extensions.is_empty() {
            let name = filter._name.trim();
            dialog = dialog.add_filter(if name.is_empty() { "Files" } else { name }, &extensions);
        }
    }
    let paths = if request.directory {
        if request.multiple {
            dialog.pick_folders()
        } else {
            dialog.pick_folder().map(|path| vec![path])
        }
    } else if request.multiple {
        dialog.pick_files()
    } else {
        dialog.pick_file().map(|path| vec![path])
    };
    Ok(paths.map(|paths| paths.into_iter().map(native_path_string).collect()))
}

#[cfg(target_os = "windows")]
fn native_save_dialog(request: LocalizedSaveDialogRequest) -> Result<Option<String>, String> {
    let mut dialog = rfd::FileDialog::new();
    if !request.title.trim().is_empty() {
        dialog = dialog.set_title(request.title.trim());
    }
    if let Some(default_path) = request.default_path.as_deref() {
        let default_path = default_path.trim();
        if !default_path.is_empty() {
            let path = std::path::Path::new(default_path);
            if path.is_dir() {
                dialog = dialog.set_directory(path);
            } else {
                if let Some(parent) = path.parent() {
                    dialog = dialog.set_directory(parent);
                }
                if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
                    dialog = dialog.set_file_name(file_name);
                }
            }
        }
    }
    if let Some(can_create_directories) = request.can_create_directories {
        dialog = dialog.set_can_create_directories(can_create_directories);
    }
    for filter in &request.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(|extension| extension.trim().trim_start_matches('.'))
            .filter(|extension| !extension.is_empty())
            .collect::<Vec<_>>();
        if !extensions.is_empty() {
            let name = filter._name.trim();
            dialog = dialog.add_filter(if name.is_empty() { "Files" } else { name }, &extensions);
        }
    }
    Ok(dialog.save_file().map(native_path_string))
}

#[cfg(target_os = "windows")]
fn native_path_string(path: std::path::PathBuf) -> String {
    let value = path.to_string_lossy().into_owned();
    #[cfg(target_os = "windows")]
    {
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    value
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn native_confirm_dialog(request: LocalizedConfirmDialogRequest) -> Result<bool, String> {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

    let confirm_label = dialog_label(&request.confirm_label, "OK");
    let cancel_label = dialog_label(&request.cancel_label, "Cancel");
    let result = MessageDialog::new()
        .set_title(dialog_label(&request.title, "Confirm"))
        .set_description(dialog_label(&request.message, "Continue?"))
        .set_level(MessageLevel::Warning)
        .set_buttons(MessageButtons::OkCancelCustom(
            confirm_label.clone(),
            cancel_label,
        ))
        .show();
    Ok(result == MessageDialogResult::Custom(confirm_label) || result == MessageDialogResult::Ok)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn native_alert_dialog(request: LocalizedAlertDialogRequest) -> Result<(), String> {
    use rfd::{MessageButtons, MessageDialog, MessageLevel};

    MessageDialog::new()
        .set_title(dialog_label(&request.title, "Alert"))
        .set_description(dialog_label(&request.message, "Operation failed."))
        .set_level(MessageLevel::Warning)
        .set_buttons(MessageButtons::OkCustom(dialog_label(
            &request.button_label,
            "OK",
        )))
        .show();
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn dialog_label(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}
