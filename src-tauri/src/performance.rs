use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSnapshot {
    pub cpu_percent: f64,
    pub memory_bytes: u64,
}

#[derive(Default)]
pub struct PerformanceMonitor {
    previous: Mutex<Option<RawSample>>,
    #[cfg(target_os = "macos")]
    process_cache: Mutex<ProcessCache>,
}

#[derive(Debug, Clone)]
struct RawSample {
    captured_at: Instant,
    cpu_seconds: f64,
    memory_bytes: u64,
}

impl PerformanceMonitor {
    pub fn snapshot(&self) -> PerformanceSnapshot {
        let Some(raw) = self.capture_raw_sample() else {
            return PerformanceSnapshot {
                cpu_percent: 0.0,
                memory_bytes: 0,
            };
        };

        let cpu_percent = self.cpu_percent(&raw);
        PerformanceSnapshot {
            cpu_percent,
            memory_bytes: raw.memory_bytes,
        }
    }

    #[cfg(target_os = "macos")]
    fn capture_raw_sample(&self) -> Option<RawSample> {
        capture_raw_sample(&self.process_cache)
    }

    #[cfg(not(target_os = "macos"))]
    fn capture_raw_sample(&self) -> Option<RawSample> {
        capture_raw_sample()
    }

    fn cpu_percent(&self, raw: &RawSample) -> f64 {
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

#[cfg(target_os = "macos")]
#[derive(Default)]
struct ProcessCache {
    helper_pids: Vec<libc::pid_t>,
    refreshed_at: Option<Instant>,
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

    const CACHE_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);
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
        resident_bytes: u64,
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

            Some(ProcessSample {
                pid,
                cpu_seconds: (task_info.total_user + task_info.total_system) as f64
                    / 1_000_000_000.0,
                resident_bytes: task_info.resident_size,
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
        name.starts_with("com.apple.WebKit.")
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
        let latest_webkit_start = main_identity.started_at_micros.saturating_add(120_000_000);

        let mut helper_pids = Vec::new();

        for pid in list_pids() {
            if pid == main_pid {
                continue;
            }
            let Some(identity) = process_identity(pid) else {
                continue;
            };

            if identity.ppid == main_pid {
                helper_pids.push(identity.pid);
                continue;
            }

            if is_webkit_helper(&identity.name)
                && identity.started_at_micros >= earliest_webkit_start
                && identity.started_at_micros <= latest_webkit_start
            {
                helper_pids.push(identity.pid);
            }
        }

        helper_pids.sort_unstable();
        helper_pids.dedup();
        cache.helper_pids = helper_pids;
        cache.refreshed_at = Some(now);
    }

    let captured_at = Instant::now();
    let main = process_sample(unsafe { libc::getpid() })?;
    let mut cpu_seconds = main.cpu_seconds;
    let mut memory_bytes = main.resident_bytes;

    let helper_pids = if let Ok(mut cache) = cache.lock() {
        refresh_helper_pids(&mut cache, main.pid, captured_at);
        cache.helper_pids.clone()
    } else {
        Vec::new()
    };

    for pid in helper_pids {
        let Some(sample) = process_sample(pid) else {
            continue;
        };
        if sample.pid == main.pid {
            continue;
        }
        cpu_seconds += sample.cpu_seconds;
        memory_bytes = memory_bytes.saturating_add(sample.resident_bytes);
    }

    Some(RawSample {
        captured_at,
        cpu_seconds,
        memory_bytes,
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
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn timeval_seconds(value: libc::timeval) -> f64 {
    value.tv_sec as f64 + value.tv_usec as f64 / 1_000_000.0
}

#[cfg(windows)]
fn capture_raw_sample() -> Option<RawSample> {
    use std::mem;
    use windows_sys::Win32::Foundation::{FILETIME, HANDLE};
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessTimes,
    };

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

    let captured_at = Instant::now();
    let process = unsafe { GetCurrentProcess() };
    if process.is_null() {
        return None;
    }
    let (cpu_seconds, memory_bytes) = process_sample(process)?;

    Some(RawSample {
        captured_at,
        cpu_seconds,
        memory_bytes,
    })
}
