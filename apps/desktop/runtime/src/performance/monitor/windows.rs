#[cfg(windows)]
fn capture_raw_sample(cache: &Mutex<ProcessCache>) -> Option<RawSample> {
    use std::collections::HashSet;
    use std::mem;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetProcessTimes, OpenProcess,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    const CACHE_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

    fn filetime_seconds(value: FILETIME) -> f64 {
        let ticks = ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64;
        ticks as f64 / 10_000_000.0
    }

    fn process_sample(process: HANDLE) -> Option<(f64, u64)> {
        let mut creation = unsafe { mem::zeroed::<FILETIME>() };
        let mut exit = unsafe { mem::zeroed::<FILETIME>() };
        let mut kernel = unsafe { mem::zeroed::<FILETIME>() };
        let mut user = unsafe { mem::zeroed::<FILETIME>() };
        let cpu_seconds = if unsafe {
            GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user)
        } != 0
        {
            filetime_seconds(kernel) + filetime_seconds(user)
        } else {
            0.0
        };

        let mut counters = unsafe { mem::zeroed::<PROCESS_MEMORY_COUNTERS_EX2>() };
        counters.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
        let memory_bytes = if unsafe {
            GetProcessMemoryInfo(
                process,
                &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut _,
                mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
            )
        } != 0
        {
            counters.WorkingSetSize as u64
        } else {
            0
        };

        Some((cpu_seconds, memory_bytes))
    }

    fn process_handle(pid: u32) -> Option<HANDLE> {
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
        (!handle.is_null()).then_some(handle)
    }

    fn list_process_entries() -> Vec<PROCESSENTRY32W> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }

        let mut entries = Vec::new();
        let mut entry = PROCESSENTRY32W {
            dwSize: mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while ok {
            entries.push(entry);
            ok = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }

        unsafe {
            CloseHandle(snapshot);
        }
        entries
    }

    fn refresh_helper_pids(cache: &mut ProcessCache, main_pid: u32, now: Instant) {
        if cache
            .refreshed_at
            .is_some_and(|refreshed_at| now.duration_since(refreshed_at) < CACHE_REFRESH_INTERVAL)
        {
            return;
        }

        let entries = list_process_entries();
        let mut known_parents = HashSet::from([main_pid]);
        let mut helper_pids = Vec::new();
        let mut changed = true;

        while changed {
            changed = false;
            for entry in &entries {
                let pid = entry.th32ProcessID;
                if pid == 0 || pid == main_pid || known_parents.contains(&pid) {
                    continue;
                }
                if known_parents.contains(&entry.th32ParentProcessID) {
                    helper_pids.push(MonitoredProcess {
                        pid,
                        kind: MonitoredProcessKind::Other,
                    });
                    known_parents.insert(pid);
                    changed = true;
                }
            }
        }

        helper_pids.sort_unstable_by_key(|process| process.pid);
        helper_pids.dedup_by_key(|process| process.pid);
        cache.helper_pids = helper_pids;
        cache.refreshed_at = Some(now);
    }

    let captured_at = Instant::now();
    let process = unsafe { GetCurrentProcess() };
    if process.is_null() {
        return None;
    }
    let (cpu_seconds, memory_bytes) = process_sample(process)?;
    let main_pid = unsafe { GetCurrentProcessId() };
    let helper_pids = if let Ok(mut cache) = cache.lock() {
        refresh_helper_pids(&mut cache, main_pid, captured_at);
        cache.helper_pids.clone()
    } else {
        Vec::new()
    };

    let mut total_cpu_seconds = cpu_seconds;
    let mut total_memory_bytes = memory_bytes;
    for helper in helper_pids {
        let Some(handle) = process_handle(helper.pid) else {
            continue;
        };
        if let Some((cpu_seconds, memory_bytes)) = process_sample(handle) {
            total_cpu_seconds += cpu_seconds;
            total_memory_bytes = total_memory_bytes.saturating_add(memory_bytes);
        }
        unsafe {
            CloseHandle(handle);
        }
    }

    Some(RawSample {
        captured_at,
        cpu_seconds: total_cpu_seconds,
        memory_bytes: total_memory_bytes,
        memory: PerformanceMemorySnapshot {
            main_bytes: memory_bytes,
            web_bytes: 0,
            gpu_bytes: 0,
            other_bytes: total_memory_bytes.saturating_sub(memory_bytes),
        },
        cpu_percent_override: None,
        gpu_percent: None,
    })
}
