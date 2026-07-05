//! Raise the process open-file limit once at startup.

/// A Finder/Dock-launched macOS app inherits launchd's low soft `RLIMIT_NOFILE` (256); with many PTYs, watchers and probes that exhausts, and every new fd/spawn then fails at once. Raise the soft limit toward the hard cap.
#[cfg(target_os = "macos")]
pub fn raise_open_file_limit() {
    use std::mem;
    unsafe {
        let mut limit: libc::rlimit = mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
            return;
        }
        // macOS rejects rlim_cur above kern.maxfilesperproc, so cap the target there.
        let mut mib = [libc::CTL_KERN, libc::KERN_MAXFILESPERPROC];
        let mut maxfiles: libc::c_int = 0;
        let mut size = mem::size_of::<libc::c_int>();
        let queried = libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            &mut maxfiles as *mut _ as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) == 0;
        let ceiling = if queried && maxfiles > 0 {
            maxfiles as libc::rlim_t
        } else {
            limit.rlim_max
        };
        let target = ceiling.min(limit.rlim_max);
        if limit.rlim_cur < target {
            limit.rlim_cur = target;
            libc::setrlimit(libc::RLIMIT_NOFILE, &limit);
        }
    }
}

/// Linux/BSD accept raising the soft limit straight to the hard limit.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn raise_open_file_limit() {
    unsafe {
        let mut limit: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
            return;
        }
        if limit.rlim_cur < limit.rlim_max {
            limit.rlim_cur = limit.rlim_max;
            libc::setrlimit(libc::RLIMIT_NOFILE, &limit);
        }
    }
}

#[cfg(not(unix))]
pub fn raise_open_file_limit() {}
