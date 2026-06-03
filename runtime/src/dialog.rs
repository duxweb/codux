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

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(not(target_os = "macos"))]
pub fn localized_save_dialog(
    _request: LocalizedSaveDialogRequest,
) -> Result<Option<String>, String> {
    Err("localized save dialog is only implemented on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub fn localized_confirm_dialog(request: LocalizedConfirmDialogRequest) -> Result<bool, String> {
    macos::confirm_dialog(request)
}

#[cfg(target_os = "windows")]
pub fn localized_confirm_dialog(request: LocalizedConfirmDialogRequest) -> Result<bool, String> {
    windows::confirm_dialog(request)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn localized_confirm_dialog(_request: LocalizedConfirmDialogRequest) -> Result<bool, String> {
    Err("localized confirm dialog is only implemented on macOS and Windows".to_string())
}

#[cfg(target_os = "macos")]
pub fn localized_alert_dialog(request: LocalizedAlertDialogRequest) -> Result<(), String> {
    macos::alert_dialog(request)
}

#[cfg(target_os = "windows")]
pub fn localized_alert_dialog(request: LocalizedAlertDialogRequest) -> Result<(), String> {
    windows::alert_dialog(request)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn localized_alert_dialog(_request: LocalizedAlertDialogRequest) -> Result<(), String> {
    Err("localized alert dialog is only implemented on macOS and Windows".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        DialogFilter, LocalizedAlertDialogRequest, LocalizedConfirmDialogRequest,
        LocalizedSaveDialogRequest,
    };
    use dispatch2::DispatchQueue;
    use objc2::{MainThreadMarker, rc::autoreleasepool};
    use objc2_app_kit::{
        NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSModalResponseOK, NSSavePanel,
    };
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::path::Path;

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

    pub fn confirm_dialog(request: LocalizedConfirmDialogRequest) -> Result<bool, String> {
        run_on_main(move |marker| {
            autoreleasepool(|_| {
                let alert = NSAlert::new(marker);
                if !request.title.trim().is_empty() {
                    alert.setMessageText(&NSString::from_str(&request.title));
                }
                if !request.message.trim().is_empty() {
                    alert.setInformativeText(&NSString::from_str(&request.message));
                }
                alert.setAlertStyle(NSAlertStyle::Warning);
                alert.addButtonWithTitle(&NSString::from_str(button_label(
                    &request.confirm_label,
                    "OK",
                )));
                alert.addButtonWithTitle(&NSString::from_str(button_label(
                    &request.cancel_label,
                    "Cancel",
                )));
                Ok(alert.runModal() == NSAlertFirstButtonReturn)
            })
        })
    }

    pub fn alert_dialog(request: LocalizedAlertDialogRequest) -> Result<(), String> {
        run_on_main(move |marker| {
            autoreleasepool(|_| {
                let alert = NSAlert::new(marker);
                if !request.title.trim().is_empty() {
                    alert.setMessageText(&NSString::from_str(&request.title));
                }
                if !request.message.trim().is_empty() {
                    alert.setInformativeText(&NSString::from_str(&request.message));
                }
                alert.setAlertStyle(NSAlertStyle::Warning);
                alert.addButtonWithTitle(&NSString::from_str(button_label(
                    &request.button_label,
                    "OK",
                )));
                let _ = alert.runModal();
                Ok(())
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

    fn button_label<'a>(value: &'a str, fallback: &'a str) -> &'a str {
        let value = value.trim();
        if value.is_empty() { fallback } else { value }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(target_os = "windows")]
mod windows {
    use super::{LocalizedAlertDialogRequest, LocalizedConfirmDialogRequest};
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDOK, MB_DEFBUTTON2, MB_ICONWARNING, MB_OK, MB_OKCANCEL, MessageBoxW,
    };

    pub fn confirm_dialog(request: LocalizedConfirmDialogRequest) -> Result<bool, String> {
        let title = to_wide(if request.title.trim().is_empty() {
            "Confirm"
        } else {
            request.title.trim()
        });
        let message = to_wide(if request.message.trim().is_empty() {
            "Continue?"
        } else {
            request.message.trim()
        });
        let response = unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        };
        Ok(response == IDOK)
    }

    pub fn alert_dialog(request: LocalizedAlertDialogRequest) -> Result<(), String> {
        let title = to_wide(if request.title.trim().is_empty() {
            "Alert"
        } else {
            request.title.trim()
        });
        let message = to_wide(if request.message.trim().is_empty() {
            "Operation failed."
        } else {
            request.message.trim()
        });
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONWARNING,
            )
        };
        Ok(())
    }

    fn to_wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}
