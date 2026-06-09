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
