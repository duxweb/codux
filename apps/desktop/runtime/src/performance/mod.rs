mod monitor;

pub use monitor::{PerformanceMemorySnapshot, PerformanceMonitor, PerformanceSnapshot};

use serde::Serialize;
use std::sync::OnceLock;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSummary {
    pub process_id: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub cpu_label: String,
    pub memory_label: String,
    pub source: String,
    pub error: Option<String>,
}

impl Default for PerformanceSummary {
    fn default() -> Self {
        Self {
            process_id: std::process::id(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            cpu_label: "0.0%".to_string(),
            memory_label: "0 B".to_string(),
            source: "unavailable".to_string(),
            error: None,
        }
    }
}

pub struct PerformanceService;

impl PerformanceService {
    pub fn summary() -> PerformanceSummary {
        static MONITOR: OnceLock<PerformanceMonitor> = OnceLock::new();
        let snapshot = MONITOR.get_or_init(PerformanceMonitor::default).snapshot();
        PerformanceSummary {
            process_id: std::process::id(),
            cpu_percent: snapshot.cpu_percent as f32,
            memory_bytes: snapshot.memory_bytes,
            cpu_label: format_cpu_percent(snapshot.cpu_percent as f32),
            memory_label: format_bytes(snapshot.memory_bytes),
            source: "performance-monitor".to_string(),
            error: None,
        }
    }
}

fn format_cpu_percent(value: f32) -> String {
    format!("{:.1}%", value.max(0.0))
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GB", value / GIB)
    } else if value >= MIB {
        format!("{:.0} MB", value / MIB)
    } else if value >= KIB {
        format!("{:.0} KB", value / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hud_labels() {
        assert_eq!(format_cpu_percent(1.234), "1.2%");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2 * 1024), "2 KB");
        assert_eq!(format_bytes(42 * 1024 * 1024), "42 MB");
    }

    #[test]
    fn monitor_snapshot_matches_tauri_shape() {
        let snapshot = PerformanceMonitor::default().snapshot();

        assert!(snapshot.cpu_percent >= 0.0);
        assert!(snapshot.memory_bytes >= snapshot.memory.main_bytes);
    }
}
