use std::{
    ffi::CString,
    mem,
    time::{Instant, SystemTime},
};

use sysinfo::{ProcessRefreshKind, System};

use super::Collector;
use crate::activity::{self, Counters, Stamp};
use crate::model::{
    Disk, Memory, MemoryBreakdown, Pressure, Process, Snapshot, SourceStatus, Swap,
};

// libc deprecates its Mach port bindings; declare the few stable libSystem symbols directly.
extern "C" {
    fn mach_host_self() -> libc::mach_port_t;
    fn mach_port_deallocate(
        task: libc::mach_port_t,
        name: libc::mach_port_t,
    ) -> libc::kern_return_t;
    /// What the C `mach_task_self()` macro reads.
    static mach_task_self_: libc::mach_port_t;
}

pub struct MacCollector {
    /// Only used for the optional process ranking; RAM/swap come from native interfaces.
    system: System,
    host: libc::mach_port_t,
    started: Instant,
    previous: Option<(Stamp, Counters)>,
}

impl MacCollector {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            // One send right for the collector's lifetime instead of leaking one per sample.
            host: unsafe { mach_host_self() },
            started: Instant::now(),
            previous: None,
        }
    }
}

impl Drop for MacCollector {
    fn drop(&mut self) {
        unsafe {
            mach_port_deallocate(mach_task_self_, self.host);
        }
    }
}

/// Runs one source, recording success, latency and error without hiding failures.
fn measure<T>(
    sources: &mut Vec<SourceStatus>,
    source: &'static str,
    read: impl FnOnce() -> Result<T, String>,
) -> Option<T> {
    let started = Instant::now();
    let result = read();
    let elapsed_micros = started.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
    let (ok, error, value) = match result {
        Ok(value) => (true, None, Some(value)),
        Err(error) => (false, Some(error), None),
    };
    sources.push(SourceStatus {
        source,
        ok,
        elapsed_micros,
        error,
    });
    value
}

impl Collector for MacCollector {
    fn collect(&mut self, processes: bool) -> Result<Snapshot, String> {
        let mut sources = Vec::new();
        let mut limitations = Vec::new();
        let total_bytes = measure(&mut sources, "hw.memsize", || sysctl_u64("hw.memsize"))
            .filter(|&total| total > 0)
            .ok_or("无法读取物理内存总量；本次采集无效。")?;

        let vm = measure(&mut sources, "host_statistics64", || {
            vm_statistics(self.host)
        });
        let cpu_ticks = measure(&mut sources, "host_cpu_load_info", || cpu_ticks(self.host));
        let swap = measure(&mut sources, "vm.swapusage", swap_usage);
        let pressure = measure(
            &mut sources,
            "kern.memorystatus_vm_pressure_level",
            memory_pressure,
        )
        .unwrap_or(Pressure::Unknown);
        let disk = measure(&mut sources, "statfs", startup_disk);

        let stamp = Stamp {
            monotonic: self.started.elapsed(),
            wall: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| e.to_string())?,
        };
        let counters = vm.as_ref().map(|vm| Counters {
            page_size: vm.page_size,
            swap_ins: vm.swap_ins,
            swap_outs: vm.swap_outs,
            page_ins: vm.page_ins,
            cpu_ticks,
        });
        let current = counters.map(|c| (stamp, c));
        let activity = activity::derive(self.previous, current);
        // A failed sample breaks the chain so the next rate never spans an unknown gap.
        self.previous = current;

        let mut top_processes = Vec::new();
        if processes {
            // Full refresh also reads command lines and other unnecessary process metadata.
            self.system
                .refresh_processes_specifics(ProcessRefreshKind::new().with_memory());
            top_processes = self
                .system
                .processes()
                .iter()
                .filter(|(_, process)| process.memory() > 0)
                .map(|(pid, process)| Process {
                    pid: pid.as_u32(),
                    name: process.name().to_owned(),
                    resident_bytes: process.memory(),
                })
                .collect();
            top_processes.sort_unstable_by(|a, b| {
                b.resident_bytes
                    .cmp(&a.resident_bytes)
                    .then(a.pid.cmp(&b.pid))
            });
            top_processes.truncate(5);
            limitations.push("进程仅列出可读取的 RSS 前五名；RSS 不等于活动监视器的内存足迹，不能直接求和或据此判定泄漏。".into());
        }
        limitations.push("磁盘 I/O 吞吐与延迟尚未采集；容量充足不代表磁盘不繁忙。".into());
        for source in sources.iter().filter(|s| !s.ok) {
            limitations.push(format!(
                "{} 不可用：{}",
                source.source,
                source.error.as_deref().unwrap_or("未知错误")
            ));
        }

        Ok(Snapshot {
            unix_seconds: stamp.wall.as_secs(),
            platform: "macos",
            memory: Memory {
                total_bytes,
                breakdown: vm.map(|vm| vm.breakdown),
                swap,
                pressure,
            },
            activity,
            disk,
            processes_requested: processes,
            top_processes,
            sources,
            limitations,
        })
    }
}

fn sysctl_raw<T: Copy>(name: &str, value: &mut T) -> Result<(), String> {
    let name = CString::new(name).map_err(|e| e.to_string())?;
    let mut size = mem::size_of::<T>();
    // `value` is a live, correctly sized buffer; null newp makes this a read-only query.
    let result = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (value as *mut T).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if size != mem::size_of::<T>() {
        return Err(format!("返回大小 {size} 与预期不符"));
    }
    Ok(())
}

fn sysctl_u64(name: &str) -> Result<u64, String> {
    let mut value = 0u64;
    sysctl_raw(name, &mut value)?;
    Ok(value)
}

struct VmSample {
    page_size: u64,
    swap_ins: u64,
    swap_outs: u64,
    page_ins: u64,
    breakdown: MemoryBreakdown,
}

fn vm_statistics(host: libc::mach_port_t) -> Result<VmSample, String> {
    // Zeroed so fields beyond what an older kernel fills stay 0; we only read long-standing fields.
    let mut stats: libc::vm_statistics64 = unsafe { mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let result = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            (&mut stats as *mut libc::vm_statistics64).cast(),
            &mut count,
        )
    };
    if result != libc::KERN_SUCCESS {
        return Err(format!("kern_return_t {result}"));
    }
    // internal_page_count ends at 144 bytes (36 integer_t); require the kernel to have filled it.
    const REQUIRED: u32 = 36;
    if count < REQUIRED {
        return Err(format!("内核只返回 {count} 个字段"));
    }
    // vm_page_size is the kernel page size used by these counters (16 KiB on Apple silicon).
    let page = unsafe { libc::vm_page_size } as u64;
    let pages = |n: u32| u64::from(n) * page;
    let app = pages(
        stats
            .internal_page_count
            .saturating_sub(stats.purgeable_count),
    );
    let wired = pages(stats.wire_count);
    let compressed = pages(stats.compressor_page_count);
    Ok(VmSample {
        page_size: page,
        swap_ins: stats.swapins,
        swap_outs: stats.swapouts,
        page_ins: stats.pageins,
        breakdown: MemoryBreakdown {
            used_bytes: app + wired + compressed,
            app_bytes: app,
            wired_bytes: wired,
            compressed_bytes: compressed,
            cached_files_bytes: pages(stats.external_page_count) + pages(stats.purgeable_count),
            free_bytes: pages(stats.free_count),
        },
    })
}

fn cpu_ticks(host: libc::mach_port_t) -> Result<[u32; 4], String> {
    let mut info: libc::host_cpu_load_info = unsafe { mem::zeroed() };
    let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
    let result = unsafe {
        libc::host_statistics(
            host,
            libc::HOST_CPU_LOAD_INFO,
            (&mut info as *mut libc::host_cpu_load_info).cast(),
            &mut count,
        )
    };
    if result != libc::KERN_SUCCESS {
        return Err(format!("kern_return_t {result}"));
    }
    if count != libc::HOST_CPU_LOAD_INFO_COUNT {
        return Err(format!("CPU 计数字段数 {count} 不兼容"));
    }
    Ok(info.cpu_ticks)
}

fn swap_usage() -> Result<Swap, String> {
    let mut usage: libc::xsw_usage = unsafe { mem::zeroed() };
    sysctl_raw("vm.swapusage", &mut usage)?;
    if usage.xsu_used > usage.xsu_total {
        return Err("交换空间返回无效容量".into());
    }
    Ok(Swap {
        total_bytes: usage.xsu_total,
        used_bytes: usage.xsu_used,
    })
}

fn decode_pressure(value: u32) -> Result<Pressure, String> {
    match value {
        1 => Ok(Pressure::Normal),
        2 => Ok(Pressure::Warning),
        4 => Ok(Pressure::Critical),
        _ => Err(format!("未知内核压力级别 {value}")),
    }
}

fn memory_pressure() -> Result<Pressure, String> {
    // This XNU sysctl is not a stable public API. Failure must remain unknown, never healthy.
    let mut value = 0u32;
    sysctl_raw("kern.memorystatus_vm_pressure_level", &mut value)?;
    decode_pressure(value)
}

fn startup_disk() -> Result<Disk, String> {
    // APFS volumes share capacity. Report the writable Data volume once, without summing volumes.
    let path = if std::path::Path::new("/System/Volumes/Data").exists() {
        "/System/Volumes/Data"
    } else {
        "/"
    };
    let c_path = CString::new(path).unwrap();
    // Darwin statfs uses 64-bit block counters to preserve large-volume capacity.
    let mut stats = mem::MaybeUninit::<libc::statfs>::uninit();
    // statfs initializes the output only on success; c_path is NUL-terminated and remains live.
    if unsafe { libc::statfs(c_path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let stats = unsafe { stats.assume_init() };
    let total_bytes = stats
        .f_blocks
        .checked_mul(u64::from(stats.f_bsize))
        .ok_or("磁盘总容量溢出")?;
    let available_bytes = stats
        .f_bavail
        .checked_mul(u64::from(stats.f_bsize))
        .ok_or("磁盘可用容量溢出")?;
    if total_bytes == 0 || available_bytes > total_bytes {
        return Err("磁盘返回无效容量".into());
    }
    Ok(Disk {
        path: path.into(),
        total_bytes,
        available_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_pressure_is_not_normal() {
        assert!(decode_pressure(0).is_err());
        assert!(decode_pressure(8).is_err());
        assert_eq!(decode_pressure(1).unwrap(), Pressure::Normal);
        assert_eq!(decode_pressure(2).unwrap(), Pressure::Warning);
        assert_eq!(decode_pressure(4).unwrap(), Pressure::Critical);
    }

    #[test]
    fn native_sources_report_consistent_values() {
        let mut collector = MacCollector::new();
        let first = collector.collect(false).unwrap();
        let breakdown = first.memory.breakdown.as_ref().expect("VM statistics");
        assert!(breakdown.used_bytes <= first.memory.total_bytes);
        assert!(breakdown.free_bytes <= first.memory.total_bytes);
        assert!(first.sources.iter().any(|s| s.source == "vm.swapusage"));
        assert_eq!(first.activity.status, crate::model::RateStatus::Warmup);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let second = collector.collect(false).unwrap();
        assert_eq!(second.activity.status, crate::model::RateStatus::Ready);
        let cpu = second.activity.cpu_busy_percent.expect("CPU ticks");
        assert!((0.0..=100.0).contains(&cpu));
    }
}
