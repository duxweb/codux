#[cfg(all(unix, not(target_os = "macos")))]
fn capture_raw_sample() -> Option<RawSample> {
    use libc::{RUSAGE_SELF, getrusage, rusage};
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
            gpu_percent: None,
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn timeval_seconds(value: libc::timeval) -> f64 {
    value.tv_sec as f64 + value.tv_usec as f64 / 1_000_000.0
}
