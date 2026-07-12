fn windows_terminal_clipboard_text() -> Result<Option<String>, ()> {
    use windows_sys::Win32::Foundation::HGLOBAL;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    struct ClipboardGuard;
    struct GlobalLockGuard(HGLOBAL);

    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    impl Drop for GlobalLockGuard {
        fn drop(&mut self) {
            unsafe {
                GlobalUnlock(self.0);
            }
        }
    }

    if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
        return Err(());
    }
    let _clipboard = ClipboardGuard;
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT as u32) };
    if handle.is_null() {
        return Ok(None);
    }
    let byte_len = unsafe { GlobalSize(handle) };
    if byte_len < std::mem::size_of::<u16>() {
        return Ok(Some(String::new()));
    }
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        return Err(());
    }
    let _global_lock = GlobalLockGuard(handle);
    let words = unsafe {
        std::slice::from_raw_parts(pointer.cast::<u16>(), byte_len / std::mem::size_of::<u16>())
    };
    let text_len = words.iter().position(|word| *word == 0).unwrap_or(words.len());
    Ok(Some(String::from_utf16_lossy(&words[..text_len])))
}
