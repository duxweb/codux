use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSnapshot {
    pub cpu_percent: f64,
    pub gpu_percent: f64,
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
    gpu_percent: Option<f64>,
}

impl PerformanceMonitor {
    pub fn snapshot(&self) -> PerformanceSnapshot {
        let Some(raw) = self.capture_raw_sample() else {
            return PerformanceSnapshot {
                cpu_percent: 0.0,
                gpu_percent: 0.0,
                memory_bytes: 0,
                memory: PerformanceMemorySnapshot::default(),
            };
        };

        let cpu_percent = self.cpu_percent(&raw);
        PerformanceSnapshot {
            cpu_percent,
            gpu_percent: raw.gpu_percent.unwrap_or(0.0),
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

include!("monitor/shared.rs");
include!("monitor/macos.rs");
include!("monitor/linux.rs");
include!("monitor/windows.rs");
