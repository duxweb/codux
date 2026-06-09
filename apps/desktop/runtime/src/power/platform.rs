#[cfg(target_os = "macos")]
pub(super) fn platform_power_adapter_connected() -> Option<bool> {
    use core_foundation_sys::base::{CFRelease, CFTypeRef};
    use core_foundation_sys::dictionary::CFDictionaryRef;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
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
pub(super) fn platform_power_adapter_connected() -> Option<bool> {
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
pub(super) fn platform_power_adapter_connected() -> Option<bool> {
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
    found_adapter.then_some(false)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
pub(super) fn platform_power_adapter_connected() -> Option<bool> {
    None
}

#[cfg(target_os = "macos")]
pub(super) struct PlatformSleepAssertion(u32);

#[cfg(target_os = "macos")]
impl PlatformSleepAssertion {
    pub(super) fn create() -> Result<Self, String> {
        use core_foundation_sys::base::CFRelease;
        use core_foundation_sys::string::CFStringRef;

        type IOReturn = i32;
        type IOPMAssertionID = u32;
        type IOPMAssertionLevel = u32;

        const K_IO_PM_ASSERTION_LEVEL_ON: IOPMAssertionLevel = 255;

        #[link(name = "IOKit", kind = "framework")]
        unsafe extern "C" {
            fn IOPMAssertionCreateWithName(
                assertion_type: CFStringRef,
                assertion_level: IOPMAssertionLevel,
                assertion_name: CFStringRef,
                assertion_id: *mut IOPMAssertionID,
            ) -> IOReturn;
        }

        let assertion_type = cf_string("PreventUserIdleSystemSleep");
        let reason = format!("{} active task", crate::runtime_paths::app_display_name());
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

    pub(super) fn release(self) {
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
        unsafe extern "C" {
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
        CFStringCreateWithBytes, CFStringRef, kCFStringEncodingUTF8,
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
pub(super) struct PlatformSleepAssertion {
    handle: windows_sys::Win32::Foundation::HANDLE,
    _reason: Vec<u16>,
}

#[cfg(target_os = "windows")]
impl PlatformSleepAssertion {
    pub(super) fn create() -> Result<Self, String> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Power::{
            PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
        };
        use windows_sys::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
        use windows_sys::Win32::System::Threading::{
            POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
        };
        use windows_sys::core::PWSTR;

        let reason_text = format!("{} active task", crate::runtime_paths::app_display_name());
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

    pub(super) fn release(self) {
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
pub(super) struct PlatformSleepAssertion {
    child: Option<std::process::Child>,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl PlatformSleepAssertion {
    pub(super) fn create() -> Result<Self, String> {
        use std::process::{Command, Stdio};

        let app_name = crate::runtime_paths::app_display_name();
        let who = format!("--who={app_name}");
        let why = format!("--why={app_name} active task");
        let reason = format!("{app_name} active task");
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
        Err(format!(
            "Failed to create Linux sleep assertion. Tried systemd-inhibit and gnome-session-inhibit. {}",
            errors.join("; ")
        ))
    }

    pub(super) fn release(mut self) {
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
pub(super) struct PlatformSleepAssertion;

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
impl PlatformSleepAssertion {
    pub(super) fn create() -> Result<Self, String> {
        Ok(Self)
    }

    pub(super) fn release(self) {}
}
