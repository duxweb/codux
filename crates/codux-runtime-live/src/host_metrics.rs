use chrono::Local;
use codux_protocol::{
    RemoteHostCpuMetrics, RemoteHostDiskMetrics, RemoteHostMemoryMetrics, RemoteHostMetrics,
    RemoteHostNetworkMetrics, RemoteHostProcessMetrics, RemoteHostSystemMetrics,
};
use std::ffi::OsStr;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{DiskRefreshKind, Disks, Networks, ProcessesToUpdate, System};

static HOST_METRICS_SAMPLER: Mutex<Option<HostMetricsSampler>> = Mutex::new(None);
const DISK_METRICS_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const DISK_METRICS_WARMUP_MIN_INTERVAL: Duration = Duration::from_secs(1);

pub fn sample_host_metrics() -> RemoteHostMetrics {
    match HOST_METRICS_SAMPLER.lock() {
        Ok(mut sampler) => sampler.get_or_insert_with(HostMetricsSampler::new).sample(),
        Err(_) => HostMetricsSampler::new().sample(),
    }
}

pub struct HostMetricsSampler {
    system: System,
    disks: Disks,
    networks: Networks,
    last_sampled_at: Option<Instant>,
    last_disk_sampled_at: Option<Instant>,
    disk_rate_warmup_pending: bool,
    cached_disks: Vec<RemoteHostDiskMetrics>,
}

impl HostMetricsSampler {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
            disks: Disks::new(),
            networks: Networks::new_with_refreshed_list(),
            last_sampled_at: None,
            last_disk_sampled_at: None,
            disk_rate_warmup_pending: false,
            cached_disks: Vec::new(),
        }
    }

    pub fn sample(&mut self) -> RemoteHostMetrics {
        let now = Instant::now();
        let elapsed = self
            .last_sampled_at
            .map(|previous| now.saturating_duration_since(previous))
            .filter(|duration| !duration.is_zero());
        let first_sample = self.last_sampled_at.is_none();
        self.last_sampled_at = Some(now);

        self.system.refresh_memory();
        self.system.refresh_cpu_all();
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.networks.refresh(true);
        let disks = self.sample_disks(now);

        RemoteHostMetrics {
            sampled_at_millis: unix_time_millis(),
            system: system_metrics(),
            cpu: cpu_metrics(&self.system, first_sample),
            memory: memory_metrics(&self.system),
            network: network_metrics(&self.networks, elapsed, first_sample),
            disks,
            processes: process_metrics(&self.system),
        }
    }

    fn sample_disks(&mut self, now: Instant) -> Vec<RemoteHostDiskMetrics> {
        let since_last = self
            .last_disk_sampled_at
            .map(|previous| now.saturating_duration_since(previous));
        let refresh_due = match since_last {
            None => true,
            Some(elapsed) => {
                (self.disk_rate_warmup_pending && elapsed >= DISK_METRICS_WARMUP_MIN_INTERVAL)
                    || elapsed >= DISK_METRICS_REFRESH_INTERVAL
            }
        };
        if refresh_due {
            let first_disk_sample = since_last.is_none();
            let refresh_kind = DiskRefreshKind::nothing().with_storage().with_io_usage();
            self.disks.refresh_specifics(true, refresh_kind);
            self.cached_disks = disk_metrics(
                &self.disks,
                since_last.filter(|duration| !duration.is_zero()),
                first_disk_sample,
            );
            self.last_disk_sampled_at = Some(now);
            self.disk_rate_warmup_pending = first_disk_sample;
        }
        self.cached_disks.clone()
    }
}

impl Default for HostMetricsSampler {
    fn default() -> Self {
        Self::new()
    }
}

fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn system_metrics() -> RemoteHostSystemMetrics {
    RemoteHostSystemMetrics {
        hostname: System::host_name().unwrap_or_default(),
        os_name: System::name().unwrap_or_default(),
        os_version: System::os_version().unwrap_or_default(),
        kernel_version: System::kernel_version().unwrap_or_default(),
        arch: std::env::consts::ARCH.to_string(),
        uptime_seconds: System::uptime(),
        utc_offset_seconds: Local::now().offset().local_minus_utc(),
    }
}

fn cpu_metrics(system: &System, first_sample: bool) -> RemoteHostCpuMetrics {
    let load = System::load_average();
    RemoteHostCpuMetrics {
        total_usage_percent: if first_sample {
            0.0
        } else {
            clamp_percent(system.global_cpu_usage())
        },
        cores: system
            .cpus()
            .iter()
            .map(|cpu| {
                if first_sample {
                    0.0
                } else {
                    clamp_percent(cpu.cpu_usage())
                }
            })
            .collect(),
        load_avg: (!cfg!(windows)).then_some([load.one, load.five, load.fifteen]),
    }
}

fn memory_metrics(system: &System) -> RemoteHostMemoryMetrics {
    RemoteHostMemoryMetrics {
        total_bytes: system.total_memory(),
        used_bytes: system.used_memory(),
        available_bytes: system.available_memory(),
        free_bytes: system.free_memory(),
        swap_total_bytes: system.total_swap(),
        swap_used_bytes: system.used_swap(),
    }
}

fn network_metrics(
    networks: &Networks,
    elapsed: Option<Duration>,
    first_sample: bool,
) -> RemoteHostNetworkMetrics {
    let mut rx_total_bytes = 0_u64;
    let mut tx_total_bytes = 0_u64;
    let mut rx_delta_bytes = 0_u64;
    let mut tx_delta_bytes = 0_u64;
    for data in networks.values() {
        rx_total_bytes = rx_total_bytes.saturating_add(data.total_received());
        tx_total_bytes = tx_total_bytes.saturating_add(data.total_transmitted());
        rx_delta_bytes = rx_delta_bytes.saturating_add(data.received());
        tx_delta_bytes = tx_delta_bytes.saturating_add(data.transmitted());
    }
    RemoteHostNetworkMetrics {
        rx_bytes_per_sec: rate_per_second(rx_delta_bytes, elapsed, first_sample),
        tx_bytes_per_sec: rate_per_second(tx_delta_bytes, elapsed, first_sample),
        rx_total_bytes,
        tx_total_bytes,
    }
}

fn disk_metrics(
    disks: &Disks,
    elapsed: Option<Duration>,
    first_sample: bool,
) -> Vec<RemoteHostDiskMetrics> {
    disks
        .list()
        .iter()
        .map(|disk| {
            let usage = disk.usage();
            RemoteHostDiskMetrics {
                name: os_string(disk.name()),
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                fs_type: os_string(disk.file_system()),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                read_bytes_per_sec: rate_per_second(usage.read_bytes, elapsed, first_sample),
                write_bytes_per_sec: rate_per_second(usage.written_bytes, elapsed, first_sample),
            }
        })
        .collect()
}

fn process_metrics(system: &System) -> Vec<RemoteHostProcessMetrics> {
    let mut processes = system
        .processes()
        .values()
        .map(|process| RemoteHostProcessMetrics {
            pid: process.pid().as_u32(),
            name: os_string(process.name()),
            cpu_percent: process.cpu_usage().max(0.0),
            memory_bytes: process.memory(),
        })
        .collect::<Vec<_>>();
    processes.sort_by(|left, right| {
        right
            .cpu_percent
            .partial_cmp(&left.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.memory_bytes.cmp(&left.memory_bytes))
            .then_with(|| left.pid.cmp(&right.pid))
    });
    processes.truncate(30);
    processes
}

fn rate_per_second(bytes: u64, elapsed: Option<Duration>, first_sample: bool) -> u64 {
    if first_sample {
        return 0;
    }
    let Some(elapsed) = elapsed else {
        return 0;
    };
    let elapsed_secs = elapsed.as_secs_f64();
    if elapsed_secs <= 0.0 {
        0
    } else {
        ((bytes as f64) / elapsed_secs).max(0.0).round() as u64
    }
}

fn clamp_percent(value: f32) -> f32 {
    value.clamp(0.0, 100.0)
}

fn os_string(value: &OsStr) -> String {
    value.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn first_sample_reports_zero_rates() {
        let mut sampler = HostMetricsSampler::new();
        let metrics = sampler.sample();

        assert_eq!(metrics.network.rx_bytes_per_sec, 0);
        assert_eq!(metrics.network.tx_bytes_per_sec, 0);
        for disk in metrics.disks {
            assert_eq!(disk.read_bytes_per_sec, 0);
            assert_eq!(disk.write_bytes_per_sec, 0);
        }
    }

    #[test]
    fn process_list_is_cpu_sorted_and_limited() {
        let mut sampler = HostMetricsSampler::new();
        let _ = sampler.sample();
        thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let metrics = sampler.sample();

        assert!(metrics.processes.len() <= 30);
        for pair in metrics.processes.windows(2) {
            assert!(pair[0].cpu_percent >= pair[1].cpu_percent);
        }
    }

    #[test]
    fn rate_per_second_uses_elapsed_duration() {
        assert_eq!(
            rate_per_second(4_000, Some(Duration::from_secs(2)), false),
            2_000
        );
        assert_eq!(
            rate_per_second(4_000, Some(Duration::from_secs(2)), true),
            0
        );
        assert_eq!(rate_per_second(4_000, None, false), 0);
    }

    #[test]
    fn disk_metrics_are_cached_between_refreshes() {
        let mut sampler = HostMetricsSampler::new();
        let cached = vec![RemoteHostDiskMetrics {
            name: "cached".to_string(),
            mount_point: "/cached".to_string(),
            fs_type: "testfs".to_string(),
            total_bytes: 100,
            available_bytes: 50,
            read_bytes_per_sec: 1,
            write_bytes_per_sec: 2,
        }];
        sampler.cached_disks = cached.clone();
        sampler.last_disk_sampled_at = Some(Instant::now());

        let disks = sampler.sample_disks(Instant::now());

        assert_eq!(disks, cached);
        assert_eq!(disks[0].name, "cached");
    }

    #[test]
    fn disk_warmup_waits_for_minimum_interval() {
        let mut sampler = HostMetricsSampler::new();
        let now = Instant::now();
        sampler.cached_disks = vec![RemoteHostDiskMetrics {
            name: "warmup".to_string(),
            mount_point: "/warmup".to_string(),
            fs_type: "testfs".to_string(),
            total_bytes: 100,
            available_bytes: 50,
            read_bytes_per_sec: 0,
            write_bytes_per_sec: 0,
        }];
        sampler.last_disk_sampled_at = Some(now);
        sampler.disk_rate_warmup_pending = true;

        let disks = sampler.sample_disks(now + Duration::from_millis(10));

        assert_eq!(disks[0].name, "warmup");
        assert!(sampler.disk_rate_warmup_pending);
    }
}
