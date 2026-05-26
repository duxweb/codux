use serde::Serialize;
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSnapshot {
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub memory: PerformanceMemorySnapshot,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceMemorySnapshot {
    pub main_bytes: u64,
    pub web_bytes: u64,
    pub gpu_bytes: u64,
    pub other_bytes: u64,
}

#[derive(Default)]
pub struct PerformanceMonitor {
    previous: Mutex<Option<RawSample>>,
    #[cfg(any(target_os = "macos", windows))]
    process_cache: Mutex<ProcessCache>,
}

#[derive(Debug, Clone)]
struct RawSample {
    captured_at: Instant,
    cpu_seconds: f64,
    memory_bytes: u64,
    memory: PerformanceMemorySnapshot,
    cpu_percent_override: Option<f64>,
}

impl PerformanceMonitor {
    pub fn snapshot(&self) -> PerformanceSnapshot {
        let Some(raw) = self.capture_raw_sample() else {
            return PerformanceSnapshot {
                cpu_percent: 0.0,
                memory_bytes: 0,
                memory: PerformanceMemorySnapshot::default(),
            };
        };

        let cpu_percent = self.cpu_percent(&raw);
        PerformanceSnapshot {
            cpu_percent,
            memory_bytes: raw.memory_bytes,
            memory: raw.memory.clone(),
        }
    }

    #[cfg(target_os = "macos")]
    fn capture_raw_sample(&self) -> Option<RawSample> {
        capture_raw_sample(&self.process_cache)
    }

    #[cfg(windows)]
    fn capture_raw_sample(&self) -> Option<RawSample> {
        capture_raw_sample(&self.process_cache)
    }

    #[cfg(all(not(target_os = "macos"), not(windows)))]
    fn capture_raw_sample(&self) -> Option<RawSample> {
        capture_raw_sample()
    }

    fn cpu_percent(&self, raw: &RawSample) -> f64 {
        if let Some(percent) = raw.cpu_percent_override {
            if percent.is_finite() {
                let Ok(mut previous) = self.previous.lock() else {
                    return percent;
                };
                *previous = Some(raw.clone());
                return ((normalize_cpu_percent(percent)) * 10.0).round() / 10.0;
            }
        }

        let Ok(mut previous) = self.previous.lock() else {
            return 0.0;
        };

        let percent = previous
            .as_ref()
            .map(|previous| {
                let wall_delta = raw
                    .captured_at
                    .duration_since(previous.captured_at)
                    .as_secs_f64()
                    .max(0.001);
                percent_delta(raw.cpu_seconds, previous.cpu_seconds, wall_delta)
            })
            .unwrap_or(0.0);
        *previous = Some(raw.clone());
        percent
    }
}

#[cfg(any(target_os = "macos", windows))]
#[derive(Default)]
struct ProcessCache {
    helper_pids: Vec<MonitoredProcess>,
    refreshed_at: Option<Instant>,
}

#[cfg(target_os = "macos")]
type MonitoredPid = libc::pid_t;

#[cfg(windows)]
type MonitoredPid = u32;

#[cfg(any(target_os = "macos", windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MonitoredProcessKind {
    Web,
    Gpu,
    Other,
}

#[cfg(any(target_os = "macos", windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MonitoredProcess {
    pid: MonitoredPid,
    kind: MonitoredProcessKind,
}

#[cfg(any(target_os = "macos", windows))]
fn add_memory(memory: &mut PerformanceMemorySnapshot, kind: MonitoredProcessKind, bytes: u64) {
    match kind {
        MonitoredProcessKind::Web => memory.web_bytes = memory.web_bytes.saturating_add(bytes),
        MonitoredProcessKind::Gpu => memory.gpu_bytes = memory.gpu_bytes.saturating_add(bytes),
        MonitoredProcessKind::Other => {
            memory.other_bytes = memory.other_bytes.saturating_add(bytes)
        }
    }
}

fn percent_delta(current: f64, previous: f64, wall_delta: f64) -> f64 {
    let cpu_delta = (current - previous).max(0.0);
    ((normalize_cpu_percent((cpu_delta / wall_delta) * 100.0)) * 10.0).round() / 10.0
}

#[cfg(target_os = "windows")]
fn normalize_cpu_percent(percent: f64) -> f64 {
    let logical_processors = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .max(1) as f64;
    percent / logical_processors
}

#[cfg(not(target_os = "windows"))]
fn normalize_cpu_percent(percent: f64) -> f64 {
    percent
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn capture_raw_sample(cache: &Mutex<ProcessCache>) -> Option<RawSample> {
    use libc::{c_char, c_int, c_void, gid_t, pid_t, uid_t};
    use std::mem;

    const CACHE_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
    const PROC_ALL_PIDS: u32 = 1;
    const PROC_PIDTASKINFO: c_int = 4;
    const PROC_PIDTBSDINFO: c_int = 3;
    const MAXCOMLEN: usize = 16;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ProcTaskInfo {
        virtual_size: u64,
        resident_size: u64,
        total_user: u64,
        total_system: u64,
        threads_user: u64,
        threads_system: u64,
        policy: i32,
        faults: i32,
        pageins: i32,
        cow_faults: i32,
        messages_sent: i32,
        messages_received: i32,
        syscalls_mach: i32,
        syscalls_unix: i32,
        csw: i32,
        threadnum: i32,
        numrunning: i32,
        priority: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ProcBsdInfo {
        flags: u32,
        status: u32,
        xstatus: u32,
        pid: u32,
        ppid: u32,
        uid: uid_t,
        gid: gid_t,
        ruid: uid_t,
        rgid: gid_t,
        svuid: uid_t,
        svgid: gid_t,
        rfu_1: u32,
        comm: [c_char; MAXCOMLEN],
        name: [c_char; MAXCOMLEN * 2],
        nfiles: u32,
        pgid: u32,
        pjobc: u32,
        e_tdev: u32,
        e_tpgid: u32,
        nice: i32,
        start_tvsec: u64,
        start_tvusec: u64,
    }

    #[link(name = "proc")]
    extern "C" {
        fn proc_listpids(
            typeinfo: u32,
            typeinfo2: u32,
            buffer: *mut c_void,
            buffersize: c_int,
        ) -> c_int;
        fn proc_pidinfo(
            pid: c_int,
            flavor: c_int,
            arg: u64,
            buffer: *mut c_void,
            buffersize: c_int,
        ) -> c_int;
    }

    #[derive(Clone)]
    struct ProcessSample {
        pid: pid_t,
        cpu_seconds: f64,
        footprint_bytes: u64,
    }

    struct ProcessIdentity {
        pid: pid_t,
        ppid: pid_t,
        name: String,
        started_at_micros: u64,
    }

    fn process_sample(pid: pid_t) -> Option<ProcessSample> {
        unsafe {
            let mut task_info = mem::zeroed::<ProcTaskInfo>();
            let task_size = mem::size_of::<ProcTaskInfo>() as c_int;
            if proc_pidinfo(
                pid,
                PROC_PIDTASKINFO,
                0,
                &mut task_info as *mut _ as *mut c_void,
                task_size,
            ) != task_size
            {
                return None;
            }

            let usage = process_usage(pid);
            Some(ProcessSample {
                pid,
                cpu_seconds: usage
                    .as_ref()
                    .map(|usage| usage.cpu_seconds)
                    .unwrap_or_else(|| {
                        (task_info.total_user + task_info.total_system) as f64 / 1_000_000_000.0
                    }),
                footprint_bytes: usage
                    .as_ref()
                    .and_then(|usage| (usage.phys_footprint > 0).then_some(usage.phys_footprint))
                    .unwrap_or(task_info.resident_size),
            })
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RUsageInfoV4 {
        uuid: [u8; 16],
        user_time: u64,
        system_time: u64,
        pkg_idle_wkups: u64,
        interrupt_wkups: u64,
        pageins: u64,
        wired_size: u64,
        resident_size: u64,
        phys_footprint: u64,
        proc_start_abstime: u64,
        proc_exit_abstime: u64,
        child_user_time: u64,
        child_system_time: u64,
        child_pkg_idle_wkups: u64,
        child_interrupt_wkups: u64,
        child_pageins: u64,
        child_elapsed_abstime: u64,
        diskio_bytesread: u64,
        diskio_byteswritten: u64,
        cpu_time_qos_default: u64,
        cpu_time_qos_maintenance: u64,
        cpu_time_qos_background: u64,
        cpu_time_qos_utility: u64,
        cpu_time_qos_legacy: u64,
        cpu_time_qos_user_initiated: u64,
        cpu_time_qos_user_interactive: u64,
        billed_system_time: u64,
        serviced_system_time: u64,
        logical_writes: u64,
        lifetime_max_phys_footprint: u64,
        instructions: u64,
        cycles: u64,
        billed_energy: u64,
        serviced_energy: u64,
        interval_max_phys_footprint: u64,
        runnable_time: u64,
    }

    #[link(name = "proc")]
    extern "C" {
        fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut c_void) -> c_int;
    }

    struct ProcessUsage {
        cpu_seconds: f64,
        phys_footprint: u64,
    }

    fn process_usage(pid: pid_t) -> Option<ProcessUsage> {
        const RUSAGE_INFO_V4: c_int = 4;
        unsafe {
            let mut usage = mem::zeroed::<RUsageInfoV4>();
            if proc_pid_rusage(pid, RUSAGE_INFO_V4, &mut usage as *mut _ as *mut c_void) != 0 {
                return None;
            }
            Some(ProcessUsage {
                cpu_seconds: (usage.user_time + usage.system_time) as f64 / 1_000_000_000.0,
                phys_footprint: usage.phys_footprint,
            })
        }
    }

    fn process_identity(pid: pid_t) -> Option<ProcessIdentity> {
        unsafe {
            let mut bsd_info = mem::zeroed::<ProcBsdInfo>();
            let bsd_size = mem::size_of::<ProcBsdInfo>() as c_int;
            if proc_pidinfo(
                pid,
                PROC_PIDTBSDINFO,
                0,
                &mut bsd_info as *mut _ as *mut c_void,
                bsd_size,
            ) != bsd_size
            {
                return None;
            }

            Some(ProcessIdentity {
                pid,
                ppid: bsd_info.ppid as pid_t,
                name: c_char_array_to_string(&bsd_info.name)
                    .or_else(|| c_char_array_to_string(&bsd_info.comm))
                    .unwrap_or_default(),
                started_at_micros: bsd_info
                    .start_tvsec
                    .saturating_mul(1_000_000)
                    .saturating_add(bsd_info.start_tvusec),
            })
        }
    }

    fn c_char_array_to_string(chars: &[c_char]) -> Option<String> {
        let bytes: Vec<u8> = chars
            .iter()
            .take_while(|&&ch| ch != 0)
            .map(|&ch| ch as u8)
            .collect();
        if bytes.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn list_pids() -> Vec<pid_t> {
        unsafe {
            let hint = proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0);
            if hint <= 0 {
                return Vec::new();
            }
            let mut pids = vec![0 as pid_t; hint as usize];
            let bytes = proc_listpids(
                PROC_ALL_PIDS,
                0,
                pids.as_mut_ptr() as *mut c_void,
                (pids.len() * mem::size_of::<pid_t>()) as c_int,
            );
            if bytes <= 0 {
                return Vec::new();
            }
            let count = (bytes as usize / mem::size_of::<pid_t>()).min(pids.len());
            pids.truncate(count);
            pids.into_iter().filter(|pid| *pid > 0).collect()
        }
    }

    fn is_webkit_helper(name: &str) -> bool {
        name.starts_with("com.apple.WebKit")
    }

    fn classify_webkit_helper(name: &str) -> MonitoredProcessKind {
        if name.contains("GPU") {
            MonitoredProcessKind::Gpu
        } else if name.contains("WebContent") {
            MonitoredProcessKind::Web
        } else {
            MonitoredProcessKind::Other
        }
    }

    fn responsible_pid(pid: pid_t) -> Option<pid_t> {
        type ResponsibilityFn = unsafe extern "C" fn(pid_t) -> pid_t;
        static RESPONSIBILITY_FN: OnceLock<Option<ResponsibilityFn>> = OnceLock::new();

        let function = RESPONSIBILITY_FN.get_or_init(|| unsafe {
            let symbol = b"responsibility_get_pid_responsible_for_pid\0";
            let handle = (-2isize) as *mut libc::c_void;
            let pointer = libc::dlsym(handle, symbol.as_ptr().cast());
            if pointer.is_null() {
                None
            } else {
                Some(mem::transmute::<*mut libc::c_void, ResponsibilityFn>(
                    pointer,
                ))
            }
        });

        function.and_then(|function| {
            let responsible = unsafe { function(pid) };
            (responsible > 0).then_some(responsible)
        })
    }

    fn refresh_helper_pids(cache: &mut ProcessCache, main_pid: pid_t, now: Instant) {
        if cache
            .refreshed_at
            .is_some_and(|refreshed_at| now.duration_since(refreshed_at) < CACHE_REFRESH_INTERVAL)
        {
            return;
        }

        let Some(main_identity) = process_identity(main_pid) else {
            return;
        };
        let earliest_webkit_start = main_identity.started_at_micros.saturating_sub(10_000_000);
        let latest_webkit_start = main_identity.started_at_micros.saturating_add(600_000_000);

        let mut helper_pids = Vec::new();

        for pid in list_pids() {
            if pid == main_pid {
                continue;
            }
            let Some(identity) = process_identity(pid) else {
                continue;
            };

            if identity.ppid == main_pid {
                let kind = if is_webkit_helper(&identity.name) {
                    classify_webkit_helper(&identity.name)
                } else {
                    MonitoredProcessKind::Other
                };
                helper_pids.push(MonitoredProcess {
                    pid: identity.pid,
                    kind,
                });
                continue;
            }

            if is_webkit_helper(&identity.name) {
                if responsible_pid(identity.pid) == Some(main_pid)
                    || (identity.started_at_micros >= earliest_webkit_start
                        && identity.started_at_micros <= latest_webkit_start
                        && responsible_pid(identity.pid).is_none())
                {
                    helper_pids.push(MonitoredProcess {
                        pid: identity.pid,
                        kind: classify_webkit_helper(&identity.name),
                    });
                }
            }
        }

        helper_pids.sort_unstable_by_key(|process| process.pid);
        helper_pids.dedup_by_key(|process| process.pid);
        cache.helper_pids = helper_pids;
        cache.refreshed_at = Some(now);
    }

    let captured_at = Instant::now();
    let main = process_sample(unsafe { libc::getpid() })?;
    let mut cpu_seconds = main.cpu_seconds;
    let mut memory_bytes = main.footprint_bytes;
    let mut memory = PerformanceMemorySnapshot {
        main_bytes: main.footprint_bytes,
        ..PerformanceMemorySnapshot::default()
    };

    let helper_pids = if let Ok(mut cache) = cache.lock() {
        refresh_helper_pids(&mut cache, main.pid, captured_at);
        cache.helper_pids.clone()
    } else {
        Vec::new()
    };

    for helper in helper_pids {
        let Some(sample) = process_sample(helper.pid) else {
            continue;
        };
        if sample.pid == main.pid {
            continue;
        }
        cpu_seconds += sample.cpu_seconds;
        add_memory(&mut memory, helper.kind, sample.footprint_bytes);
        if helper.kind != MonitoredProcessKind::Gpu {
            memory_bytes = memory_bytes.saturating_add(sample.footprint_bytes);
        }
    }

    Some(RawSample {
        captured_at,
        cpu_seconds,
        memory_bytes,
        memory,
        cpu_percent_override: None,
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn capture_raw_sample() -> Option<RawSample> {
    use libc::{getrusage, rusage, RUSAGE_SELF};
    use std::fs;
    use std::mem;

    let captured_at = Instant::now();
    unsafe {
        let mut usage = mem::zeroed::<rusage>();
        if getrusage(RUSAGE_SELF, &mut usage) != 0 {
            return None;
        }

        let statm = fs::read_to_string("/proc/self/statm").ok();
        let resident_pages = statm
            .as_deref()
            .and_then(|text| text.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let page_size = libc::sysconf(libc::_SC_PAGESIZE).max(0) as u64;
        let memory_bytes = resident_pages.saturating_mul(page_size);

        Some(RawSample {
            captured_at,
            cpu_seconds: timeval_seconds(usage.ru_utime) + timeval_seconds(usage.ru_stime),
            memory_bytes,
            memory: PerformanceMemorySnapshot {
                main_bytes: memory_bytes,
                ..PerformanceMemorySnapshot::default()
            },
            cpu_percent_override: None,
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn timeval_seconds(value: libc::timeval) -> f64 {
    value.tv_sec as f64 + value.tv_usec as f64 / 1_000_000.0
}

#[cfg(windows)]
fn capture_raw_sample(cache: &Mutex<ProcessCache>) -> Option<RawSample> {
    use std::collections::HashSet;
    use std::mem;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
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
    })
}
