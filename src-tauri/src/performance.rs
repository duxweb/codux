use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSnapshot {
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub graphics_bytes: u64,
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
    graphics_bytes: u64,
}

impl PerformanceMonitor {
    pub fn snapshot(&self) -> PerformanceSnapshot {
        let Some(raw) = capture_raw_sample() else {
            return PerformanceSnapshot {
                cpu_percent: 0.0,
                memory_bytes: 0,
                graphics_bytes: 0,
            };
        };

        let cpu_percent = self.cpu_percent(&raw);
        PerformanceSnapshot {
            cpu_percent,
            memory_bytes: raw.memory_bytes,
            graphics_bytes: raw.graphics_bytes,
        }
    }

    fn cpu_percent(&self, raw: &RawSample) -> f64 {
        let Ok(mut previous) = self.previous.lock() else {
            return 0.0;
        };

        let percent = previous
            .as_ref()
            .map(|previous| {
                let cpu_delta = (raw.cpu_seconds - previous.cpu_seconds).max(0.0);
                let wall_delta = raw
                    .captured_at
                    .duration_since(previous.captured_at)
                    .as_secs_f64()
                    .max(0.001);
                ((cpu_delta / wall_delta) * 100.0 * 10.0).round() / 10.0
            })
            .unwrap_or(0.0);
        *previous = Some(raw.clone());
        percent
    }
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn capture_raw_sample() -> Option<RawSample> {
    use libc::{
        integer_t, mach_msg_type_number_t, mach_task_self, mach_vm_address_t, mach_vm_size_t,
        natural_t, task_info, task_thread_times_info_data_t, KERN_SUCCESS, TASK_THREAD_TIMES_INFO,
    };
    use std::mem;

    const TASK_VM_INFO: natural_t = 22;

    #[repr(C)]
    struct TaskVmInfo {
        virtual_size: mach_vm_size_t,
        region_count: integer_t,
        page_size: integer_t,
        resident_size: mach_vm_size_t,
        resident_size_peak: mach_vm_size_t,
        device: mach_vm_size_t,
        device_peak: mach_vm_size_t,
        internal: mach_vm_size_t,
        internal_peak: mach_vm_size_t,
        external: mach_vm_size_t,
        external_peak: mach_vm_size_t,
        reusable: mach_vm_size_t,
        reusable_peak: mach_vm_size_t,
        purgeable_volatile_pmap: mach_vm_size_t,
        purgeable_volatile_resident: mach_vm_size_t,
        purgeable_volatile_virtual: mach_vm_size_t,
        compressed: mach_vm_size_t,
        compressed_peak: mach_vm_size_t,
        compressed_lifetime: mach_vm_size_t,
        phys_footprint: mach_vm_size_t,
        min_address: mach_vm_address_t,
        max_address: mach_vm_address_t,
        ledger_phys_footprint_peak: i64,
        ledger_purgeable_nonvolatile: i64,
        ledger_purgeable_novolatile_compressed: i64,
        ledger_purgeable_volatile: i64,
        ledger_purgeable_volatile_compressed: i64,
        ledger_tag_network_nonvolatile: i64,
        ledger_tag_network_nonvolatile_compressed: i64,
        ledger_tag_network_volatile: i64,
        ledger_tag_network_volatile_compressed: i64,
        ledger_tag_media_footprint: i64,
        ledger_tag_media_footprint_compressed: i64,
        ledger_tag_media_nofootprint: i64,
        ledger_tag_media_nofootprint_compressed: i64,
        ledger_tag_graphics_footprint: i64,
        ledger_tag_graphics_footprint_compressed: i64,
        ledger_tag_graphics_nofootprint: i64,
        ledger_tag_graphics_nofootprint_compressed: i64,
        ledger_tag_neural_footprint: i64,
        ledger_tag_neural_footprint_compressed: i64,
        ledger_tag_neural_nofootprint: i64,
        ledger_tag_neural_nofootprint_compressed: i64,
        limit_bytes_remaining: u64,
        decompressions: integer_t,
        ledger_swapins: i64,
        ledger_tag_neural_nofootprint_total: i64,
        ledger_tag_neural_nofootprint_peak: i64,
    }

    let captured_at = Instant::now();
    unsafe {
        let mut thread_info = mem::zeroed::<task_thread_times_info_data_t>();
        let mut thread_count = (mem::size_of::<task_thread_times_info_data_t>()
            / mem::size_of::<natural_t>()) as mach_msg_type_number_t;
        let thread_result = task_info(
            mach_task_self(),
            TASK_THREAD_TIMES_INFO,
            &mut thread_info as *mut _ as *mut integer_t,
            &mut thread_count,
        );
        if thread_result != KERN_SUCCESS {
            return None;
        }

        let mut vm_info = mem::zeroed::<TaskVmInfo>();
        let mut vm_count =
            (mem::size_of::<TaskVmInfo>() / mem::size_of::<natural_t>()) as mach_msg_type_number_t;
        let vm_result = task_info(
            mach_task_self(),
            TASK_VM_INFO,
            &mut vm_info as *mut _ as *mut integer_t,
            &mut vm_count,
        );
        if vm_result != KERN_SUCCESS {
            return None;
        }

        let cpu_seconds =
            time_value_seconds(thread_info.user_time) + time_value_seconds(thread_info.system_time);
        let total_memory = if vm_info.phys_footprint > 0 {
            vm_info.phys_footprint
        } else {
            vm_info.resident_size
        };
        let graphics_bytes = vm_info.ledger_tag_graphics_footprint.max(0) as u64;
        let memory_bytes = total_memory.saturating_sub(graphics_bytes);

        Some(RawSample {
            captured_at,
            cpu_seconds,
            memory_bytes,
            graphics_bytes,
        })
    }
}

#[cfg(target_os = "macos")]
fn time_value_seconds(value: libc::time_value_t) -> f64 {
    value.seconds as f64 + value.microseconds as f64 / 1_000_000.0
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
            graphics_bytes: 0,
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn timeval_seconds(value: libc::timeval) -> f64 {
    value.tv_sec as f64 + value.tv_usec as f64 / 1_000_000.0
}

#[cfg(windows)]
fn capture_raw_sample() -> Option<RawSample> {
    Some(RawSample {
        captured_at: Instant::now(),
        cpu_seconds: 0.0,
        memory_bytes: 0,
        graphics_bytes: 0,
    })
}
