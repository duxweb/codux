use crate::app_settings::AppSettingsStore;
use crate::paths::app_display_name;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct PowerManager {
    assertion: Mutex<Option<PlatformSleepAssertion>>,
    sync_started: Mutex<bool>,
}

impl Default for PowerManager {
    fn default() -> Self {
        Self {
            assertion: Mutex::new(None),
            sync_started: Mutex::new(false),
        }
    }
}

impl PowerManager {
    pub fn start_settings_sync(
        self: &Arc<Self>,
        settings: Arc<AppSettingsStore>,
    ) -> Result<(), String> {
        {
            let mut started = self
                .sync_started
                .lock()
                .map_err(|_| "Power manager sync lock poisoned.".to_string())?;
            if *started {
                return Ok(());
            }
            *started = true;
        }

        let manager = Arc::clone(self);
        manager.set_sleep_prevention(settings.snapshot().sleep_mode)?;
        let _ = thread::Builder::new()
            .name("codux-power-settings-sync".to_string())
            .spawn(move || loop {
                thread::sleep(Duration::from_secs(60));
                let _ = manager.set_sleep_prevention(settings.snapshot().sleep_mode);
            });
        Ok(())
    }

    pub fn set_sleep_prevention(&self, mode: String) -> Result<bool, String> {
        let enabled = match mode.as_str() {
            "always" => true,
            "powerAdapterOnly" => platform_power_adapter_connected().unwrap_or(true),
            _ => false,
        };
        let mut assertion = self
            .assertion
            .lock()
            .map_err(|_| "Power manager lock poisoned.".to_string())?;

        if !enabled {
            if let Some(existing) = assertion.take() {
                existing.release();
            }
            return Ok(false);
        }

        if assertion.is_none() {
            *assertion = Some(PlatformSleepAssertion::create()?);
        }
        Ok(assertion.is_some())
    }
}

#[cfg(target_os = "macos")]
fn platform_power_adapter_connected() -> Option<bool> {
    use core_foundation_sys::base::{CFRelease, CFTypeRef};
    use core_foundation_sys::dictionary::CFDictionaryRef;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPSCopyExternalPowerAdapterDetails() -> CFDictionaryRef;
    }

    let details = unsafe { IOPSCopyExternalPowerAdapterDetails() };
    if details.is_null() {
        return Some(false);
    }
    unsafe {
        CFRelease(details as CFTypeRef);
    }
    Some(true)
}

#[cfg(target_os = "windows")]
fn platform_power_adapter_connected() -> Option<bool> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    let mut status = SYSTEM_POWER_STATUS {
        ACLineStatus: 0,
        BatteryFlag: 0,
        BatteryLifePercent: 0,
        SystemStatusFlag: 0,
        BatteryLifeTime: 0,
        BatteryFullLifeTime: 0,
    };
    let ok = unsafe { GetSystemPowerStatus(&mut status) };
    if ok == 0 {
        return None;
    }
    Some(status.ACLineStatus == 1)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_power_adapter_connected() -> Option<bool> {
    use std::fs;

    let entries = fs::read_dir("/sys/class/power_supply").ok()?;
    let mut found_adapter = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = fs::read_to_string(path.join("type")).ok()?;
        let kind = kind.trim();
        if kind != "Mains" && kind != "USB" && kind != "USB-C" {
            continue;
        }
        found_adapter = true;
        let online = fs::read_to_string(path.join("online")).ok()?;
        if online.trim() == "1" {
            return Some(true);
        }
    }
    if found_adapter {
        Some(false)
    } else {
        None
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
fn platform_power_adapter_connected() -> Option<bool> {
    None
}

#[cfg(target_os = "macos")]
struct PlatformSleepAssertion(u32);

#[cfg(target_os = "macos")]
impl PlatformSleepAssertion {
    fn create() -> Result<Self, String> {
        use core_foundation_sys::base::CFRelease;
        use core_foundation_sys::string::CFStringRef;

        type IOReturn = i32;
        type IOPMAssertionID = u32;
        type IOPMAssertionLevel = u32;

        const K_IO_PM_ASSERTION_LEVEL_ON: IOPMAssertionLevel = 255;

        #[link(name = "IOKit", kind = "framework")]
        extern "C" {
            fn IOPMAssertionCreateWithName(
                assertion_type: CFStringRef,
                assertion_level: IOPMAssertionLevel,
                assertion_name: CFStringRef,
                assertion_id: *mut IOPMAssertionID,
            ) -> IOReturn;
        }

        let assertion_type = cf_string("PreventUserIdleSystemSleep");
        let reason = format!("{} active task", app_display_name());
        let name = cf_string(&reason);
        let mut id = 0;
        let result = unsafe {
            IOPMAssertionCreateWithName(assertion_type, K_IO_PM_ASSERTION_LEVEL_ON, name, &mut id)
        };
        unsafe {
            CFRelease(assertion_type.cast());
            CFRelease(name.cast());
        }
        if result != 0 {
            return Err(format!("Failed to create sleep assertion: {result}"));
        }
        Ok(Self(id))
    }

    fn release(self) {
        let mut assertion = self;
        assertion.release_inner();
    }

    fn release_inner(&mut self) {
        if self.0 == 0 {
            return;
        }
        type IOReturn = i32;
        type IOPMAssertionID = u32;

        #[link(name = "IOKit", kind = "framework")]
        extern "C" {
            fn IOPMAssertionRelease(assertion_id: IOPMAssertionID) -> IOReturn;
        }

        let id = self.0;
        self.0 = 0;
        let _ = unsafe { IOPMAssertionRelease(id) };
    }
}

#[cfg(target_os = "macos")]
impl Drop for PlatformSleepAssertion {
    fn drop(&mut self) {
        self.release_inner();
    }
}

#[cfg(target_os = "macos")]
fn cf_string(value: &str) -> core_foundation_sys::string::CFStringRef {
    use core_foundation_sys::base::kCFAllocatorDefault;
    use core_foundation_sys::string::{
        kCFStringEncodingUTF8, CFStringCreateWithBytes, CFStringRef,
    };

    unsafe {
        CFStringCreateWithBytes(
            kCFAllocatorDefault,
            value.as_ptr(),
            value.len() as isize,
            kCFStringEncodingUTF8,
            0,
        ) as CFStringRef
    }
}

#[cfg(target_os = "windows")]
struct PlatformSleepAssertion {
    handle: windows_sys::Win32::Foundation::HANDLE,
    _reason: Vec<u16>,
}

#[cfg(target_os = "windows")]
impl PlatformSleepAssertion {
    fn create() -> Result<Self, String> {
        use windows_sys::core::PWSTR;
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Power::{
            PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
        };
        use windows_sys::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
        use windows_sys::Win32::System::Threading::{
            POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
        };

        let reason_text = format!("{} active task", app_display_name());
        let mut reason: Vec<u16> = reason_text.encode_utf16().chain([0]).collect();
        let context = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: reason.as_mut_ptr() as PWSTR,
            },
        };
        let handle = unsafe { PowerCreateRequest(&context) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err("Failed to create Windows sleep assertion.".to_string());
        }
        let ok = unsafe { PowerSetRequest(handle, PowerRequestSystemRequired) };
        if ok == 0 {
            unsafe {
                CloseHandle(handle);
            }
            return Err("Failed to enable Windows sleep assertion.".to_string());
        }
        Ok(Self {
            handle,
            _reason: reason,
        })
    }

    fn release(self) {
        let mut assertion = self;
        assertion.release_inner();
    }

    fn release_inner(&mut self) {
        if self.handle.is_null() {
            return;
        }
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Power::{PowerClearRequest, PowerRequestSystemRequired};
        let handle = self.handle;
        self.handle = std::ptr::null_mut();
        unsafe {
            let _ = PowerClearRequest(handle, PowerRequestSystemRequired);
            let _ = CloseHandle(handle);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe impl Send for PlatformSleepAssertion {}

#[cfg(target_os = "windows")]
impl Drop for PlatformSleepAssertion {
    fn drop(&mut self) {
        self.release_inner();
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
struct PlatformSleepAssertion {
    child: Option<std::process::Child>,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl PlatformSleepAssertion {
    fn create() -> Result<Self, String> {
        use std::process::{Command, Stdio};

        let who = format!("--who={}", app_display_name());
        let why = format!("--why={} active task", app_display_name());
        let reason = format!("{} active task", app_display_name());
        let candidates: [(&str, Vec<&str>); 2] = [
            (
                "systemd-inhibit",
                vec![
                    "--what=idle:sleep",
                    who.as_str(),
                    why.as_str(),
                    "--mode=block",
                    "sleep",
                    "infinity",
                ],
            ),
            (
                "gnome-session-inhibit",
                vec![
                    "--inhibit",
                    "idle:suspend",
                    "--reason",
                    reason.as_str(),
                    "sleep",
                    "infinity",
                ],
            ),
        ];

        let mut errors = Vec::new();
        for (program, args) in candidates {
            match Command::new(program)
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => return Ok(Self { child: Some(child) }),
                Err(error) => errors.push(format!("{program}: {error}")),
            }
        }

        let joined = errors.join("; ");
        return Err(format!(
            "Failed to create Linux sleep assertion. Tried systemd-inhibit and gnome-session-inhibit. {joined}"
        ));
    }

    fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for PlatformSleepAssertion {
    fn drop(&mut self) {
        self.release_inner();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
struct PlatformSleepAssertion;

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
impl PlatformSleepAssertion {
    fn create() -> Result<Self, String> {
        Ok(Self)
    }

    fn release(self) {}
}
