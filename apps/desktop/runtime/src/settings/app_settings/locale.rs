use super::types::AppSettings;

pub fn locale_from_language_setting(language: &str) -> String {
    match language {
        "english" => "en",
        "simplifiedChinese" | "zh-CN" | "zh_CN" | "zh-Hans" | "zh-Hans-CN" | "zh_Hans_CN" => {
            "zh-Hans"
        }
        "traditionalChinese" | "zh-TW" | "zh_TW" | "zh-Hant" | "zh-Hant-TW" | "zh_Hant_TW" => {
            "zh-Hant"
        }
        "japanese" => "ja",
        "korean" => "ko",
        "french" => "fr",
        "german" => "de",
        "spanish" => "es",
        "portugueseBrazil" => "pt-BR",
        "russian" => "ru",
        _ => locale_from_system_setting(),
    }
    .to_string()
}

pub fn sync_process_locale_preference(settings: &AppSettings) {
    #[cfg(target_os = "macos")]
    {
        macos_sync_process_locale_preference(&settings.language);
    }
}

fn locale_from_system_setting() -> &'static str {
    #[cfg(target_os = "macos")]
    if let Some(locale) = macos_global_preferred_locale() {
        return locale_from_system_locale(&locale);
    }

    sys_locale::get_locale()
        .as_deref()
        .map(locale_from_system_locale)
        .unwrap_or("en")
}

fn locale_from_system_locale(locale: &str) -> &'static str {
    let normalized = locale.replace('_', "-").to_lowercase();
    if normalized.starts_with("zh-tw")
        || normalized.starts_with("zh-hk")
        || normalized.starts_with("zh-mo")
    {
        return "zh-Hant";
    }
    if normalized.starts_with("zh") {
        return "zh-Hans";
    }
    if normalized.starts_with("ja") {
        return "ja";
    }
    if normalized.starts_with("ko") {
        return "ko";
    }
    if normalized.starts_with("fr") {
        return "fr";
    }
    if normalized.starts_with("de") {
        return "de";
    }
    if normalized.starts_with("es") {
        return "es";
    }
    if normalized.starts_with("pt-br") {
        return "pt-BR";
    }
    if normalized.starts_with("ru") {
        return "ru";
    }
    if normalized.starts_with("en") {
        return "en";
    }
    "en"
}

#[cfg(target_os = "macos")]
fn macos_global_preferred_locale() -> Option<String> {
    use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex};
    use core_foundation_sys::base::{CFRelease, kCFAllocatorDefault};
    use core_foundation_sys::preferences::{
        CFPreferencesCopyAppValue, CFPreferencesCopyValue, kCFPreferencesAnyApplication,
        kCFPreferencesAnyHost, kCFPreferencesCurrentUser,
    };
    use core_foundation_sys::string::{
        CFStringCreateWithCString, CFStringRef, kCFStringEncodingUTF8,
    };
    use std::ffi::CString;

    let key = CString::new("AppleLanguages").ok()?;
    let key_ref = unsafe {
        CFStringCreateWithCString(kCFAllocatorDefault, key.as_ptr(), kCFStringEncodingUTF8)
    };
    if key_ref.is_null() {
        return None;
    }

    let value_ref = unsafe {
        CFPreferencesCopyValue(
            key_ref,
            kCFPreferencesAnyApplication,
            kCFPreferencesCurrentUser,
            kCFPreferencesAnyHost,
        )
    };
    let value_ref = if value_ref.is_null() {
        unsafe { CFPreferencesCopyAppValue(key_ref, kCFPreferencesAnyApplication) }
    } else {
        value_ref
    };
    unsafe {
        CFRelease(key_ref.cast());
    }
    if value_ref.is_null() {
        return None;
    }

    let locale = unsafe {
        let count = CFArrayGetCount(value_ref.cast());
        let locale = if count > 0 {
            let first_ref = CFArrayGetValueAtIndex(value_ref.cast(), 0) as CFStringRef;
            cf_string_to_string(first_ref)
        } else {
            None
        };
        CFRelease(value_ref.cast());
        locale
    };

    locale
}

#[cfg(target_os = "macos")]
unsafe fn cf_string_to_string(value: core_foundation_sys::string::CFStringRef) -> Option<String> {
    use core_foundation_sys::string::{CFStringGetCString, kCFStringEncodingUTF8};
    use std::ffi::CStr;

    if value.is_null() {
        return None;
    }

    let mut buffer = [0i8; 128];
    if unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            kCFStringEncodingUTF8,
        )
    } == 0
    {
        return None;
    }

    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .ok()
        .map(str::to_string)
}

#[cfg(target_os = "macos")]
fn macos_sync_process_locale_preference(language: &str) {
    use core_foundation_sys::array::{CFArrayCreate, kCFTypeArrayCallBacks};
    use core_foundation_sys::base::{CFRelease, kCFAllocatorDefault};
    use core_foundation_sys::preferences::{
        CFPreferencesAppSynchronize, CFPreferencesSetAppValue, kCFPreferencesCurrentApplication,
    };
    use core_foundation_sys::propertylist::CFPropertyListRef;
    use core_foundation_sys::string::{CFStringCreateWithCString, kCFStringEncodingUTF8};
    use std::ffi::CString;
    use std::os::raw::c_void;
    use std::ptr;

    let key = CString::new("AppleLanguages").expect("static string contains no nul");
    let key_ref = unsafe {
        CFStringCreateWithCString(kCFAllocatorDefault, key.as_ptr(), kCFStringEncodingUTF8)
    };
    if key_ref.is_null() {
        return;
    }

    unsafe {
        if language == "system" {
            CFPreferencesSetAppValue(
                key_ref,
                ptr::null::<c_void>() as CFPropertyListRef,
                kCFPreferencesCurrentApplication,
            );
            let _ = CFPreferencesAppSynchronize(kCFPreferencesCurrentApplication);
            CFRelease(key_ref.cast());
            return;
        }
    }

    let locale = locale_from_language_setting(language);
    let locale = CString::new(locale).unwrap_or_else(|_| CString::new("en").unwrap());
    let locale_ref = unsafe {
        CFStringCreateWithCString(kCFAllocatorDefault, locale.as_ptr(), kCFStringEncodingUTF8)
    };
    if locale_ref.is_null() {
        unsafe {
            CFRelease(key_ref.cast());
        }
        return;
    }

    let values = [locale_ref.cast::<c_void>()];
    let languages_ref = unsafe {
        CFArrayCreate(
            kCFAllocatorDefault,
            values.as_ptr(),
            values.len() as isize,
            &kCFTypeArrayCallBacks,
        )
    };

    unsafe {
        if !languages_ref.is_null() {
            CFPreferencesSetAppValue(
                key_ref,
                languages_ref.cast(),
                kCFPreferencesCurrentApplication,
            );
            let _ = CFPreferencesAppSynchronize(kCFPreferencesCurrentApplication);
            CFRelease(languages_ref.cast());
        }
        CFRelease(locale_ref.cast());
        CFRelease(key_ref.cast());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_language_settings_map_to_supported_locales() {
        assert_eq!(locale_from_language_setting("simplifiedChinese"), "zh-Hans");
        assert_eq!(
            locale_from_language_setting("traditionalChinese"),
            "zh-Hant"
        );
        assert_eq!(locale_from_language_setting("portugueseBrazil"), "pt-BR");
    }

    #[test]
    fn system_locale_mapping_matches_frontend_locale_mapping() {
        assert_eq!(locale_from_system_locale("zh_CN"), "zh-Hans");
        assert_eq!(locale_from_system_locale("zh-Hans-CN"), "zh-Hans");
        assert_eq!(locale_from_system_locale("zh_TW"), "zh-Hant");
        assert_eq!(locale_from_system_locale("pt_BR"), "pt-BR");
        assert_eq!(locale_from_system_locale("en_US"), "en");
    }
}
