#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn capture_raw_sample(cache: &Mutex<ProcessCache>) -> Option<RawSample> {
    use libc::{c_char, c_int, c_void, gid_t, pid_t, uid_t};
    use std::mem;
    use std::sync::OnceLock;

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
    unsafe extern "C" {
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
    unsafe extern "C" {
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

    fn process_cpu_percent_from_ps(pid: pid_t) -> Option<f64> {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "%cpu="])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&output.stdout);
        value.trim().parse::<f64>().ok()
    }

    let captured_at = Instant::now();
    let main = process_sample(unsafe { libc::getpid() })?;
    let cpu_percent_override = process_cpu_percent_from_ps(main.pid);
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
        cpu_percent_override,
        gpu_percent: None,
    })
}
