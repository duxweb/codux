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
}

#[derive(Debug, Clone)]
struct RawSample {
    captured_at: Instant,
    cpu_seconds: f64,
    memory_bytes: u64,
}

impl PerformanceMonitor {
    pub fn snapshot(&self) -> PerformanceSnapshot {
        let Some(raw) = capture_raw_sample() else {
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

fn percent_delta(current: f64, previous: f64, wall_delta: f64) -> f64 {
    let cpu_delta = (current - previous).max(0.0);
    ((cpu_delta / wall_delta) * 100.0 * 10.0).round() / 10.0
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn capture_raw_sample() -> Option<RawSample> {
    use libc::{c_char, c_int, c_void, gid_t, pid_t, uid_t};
    use std::{ffi::CStr, mem};

    const PROC_ALL_PIDS: u32 = 1;
    const PROC_PIDTASKINFO: c_int = 4;
    const PROC_PIDTBSDINFO: c_int = 3;
    const MAXCOMLEN: usize = 16;
    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

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
        fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    }

    #[derive(Clone)]
    struct ProcessSample {
        pid: pid_t,
        ppid: pid_t,
        name: String,
        path: String,
        cpu_seconds: f64,
        resident_bytes: u64,
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

            let mut path_buffer = [0i8; PROC_PIDPATHINFO_MAXSIZE];
            let path_len = proc_pidpath(
                pid,
                path_buffer.as_mut_ptr() as *mut c_void,
                path_buffer.len() as u32,
            );
            let path = if path_len > 0 {
                CStr::from_ptr(path_buffer.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            };

            Some(ProcessSample {
                pid,
                ppid: bsd_info.ppid as pid_t,
                name: c_char_array_to_string(&bsd_info.name)
                    .or_else(|| c_char_array_to_string(&bsd_info.comm))
                    .unwrap_or_default(),
                path,
                cpu_seconds: (task_info.total_user + task_info.total_system) as f64
                    / 1_000_000_000.0,
                resident_bytes: task_info.resident_size,
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

    fn is_tauri_helper(sample: &ProcessSample, main: &ProcessSample) -> bool {
        sample.ppid == main.pid
            || (!main.path.is_empty() && sample.path.starts_with(&main.path))
            || (!main.name.is_empty() && sample.name.starts_with(&main.name))
    }

    let captured_at = Instant::now();
    let main = process_sample(unsafe { libc::getpid() })?;
    let mut cpu_seconds = main.cpu_seconds;
    let mut memory_bytes = main.resident_bytes;

    for pid in list_pids() {
        if pid == main.pid {
            continue;
        }
        let Some(sample) = process_sample(pid) else {
            continue;
        };
        if !is_tauri_helper(&sample, &main) {
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
    use std::collections::{HashMap, HashSet};
    use std::mem;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_VM_READ,
    };

    #[derive(Clone)]
    struct ProcessEntry {
        pid: u32,
        parent_pid: u32,
    }

    struct OwnedHandle(HANDLE);

    impl OwnedHandle {
        fn new(handle: HANDLE) -> Option<Self> {
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return None;
            }
            Some(Self(handle))
        }
    }

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn list_processes() -> Vec<ProcessEntry> {
        let Some(snapshot) = OwnedHandle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
            return Vec::new();
        };
        let mut entry = unsafe { mem::zeroed::<PROCESSENTRY32W>() };
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut processes = Vec::new();
        let mut has_entry = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
        while has_entry {
            processes.push(ProcessEntry {
                pid: entry.th32ProcessID,
                parent_pid: entry.th32ParentProcessID,
            });
            has_entry = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
        }
        processes
    }

    fn process_descendants(root_pid: u32, processes: &[ProcessEntry]) -> HashSet<u32> {
        let mut children_by_parent = HashMap::<u32, Vec<u32>>::new();
        for process in processes {
            children_by_parent
                .entry(process.parent_pid)
                .or_default()
                .push(process.pid);
        }

        let mut ids = HashSet::from([root_pid]);
        let mut stack = vec![root_pid];
        while let Some(parent_pid) = stack.pop() {
            let Some(children) = children_by_parent.get(&parent_pid) else {
                continue;
            };
            for child_pid in children {
                if ids.insert(*child_pid) {
                    stack.push(*child_pid);
                }
            }
        }
        ids
    }

    fn filetime_seconds(value: FILETIME) -> f64 {
        let ticks = ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64;
        ticks as f64 / 10_000_000.0
    }

    fn process_sample(pid: u32) -> Option<(f64, u64)> {
        let Some(process) = OwnedHandle::new(unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid)
        }) else {
            return None;
        };

        let mut creation = unsafe { mem::zeroed::<FILETIME>() };
        let mut exit = unsafe { mem::zeroed::<FILETIME>() };
        let mut kernel = unsafe { mem::zeroed::<FILETIME>() };
        let mut user = unsafe { mem::zeroed::<FILETIME>() };
        let cpu_seconds = if unsafe {
            GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user)
        } != 0
        {
            filetime_seconds(kernel) + filetime_seconds(user)
        } else {
            0.0
        };

        let mut counters = unsafe { mem::zeroed::<PROCESS_MEMORY_COUNTERS_EX>() };
        counters.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let memory_bytes = if unsafe {
            GetProcessMemoryInfo(
                process.0,
                &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut _,
                mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            )
        } != 0
        {
            counters.PrivateUsage as u64
        } else {
            0
        };

        Some((cpu_seconds, memory_bytes))
    }

    let captured_at = Instant::now();
    let root_pid = unsafe { GetCurrentProcessId() };
    let processes = list_processes();
    let process_ids = process_descendants(root_pid, &processes);
    let mut cpu_seconds = 0.0;
    let mut memory_bytes = 0_u64;

    for pid in process_ids {
        let Some((process_cpu_seconds, process_memory_bytes)) = process_sample(pid) else {
            continue;
        };
        cpu_seconds += process_cpu_seconds;
        memory_bytes = memory_bytes.saturating_add(process_memory_bytes);
    }

    Some(RawSample {
        captured_at,
        cpu_seconds,
        memory_bytes,
    })
}
