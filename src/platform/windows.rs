//! Public Win32 APIs. Missing metrics stay missing; commit is never labelled swap.
use crate::{activity, model::*, platform::Collector};
use std::{
    mem,
    path::Path,
    time::{Duration, SystemTime},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME},
    Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW},
    System::{
        ProcessStatus::*, SystemInformation::*, Threading::*,
        WindowsProgramming::QueryUnbiasedInterruptTime,
    },
};
fn last_error() -> String {
    std::io::Error::last_os_error().to_string()
}
pub fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(Some(0)).collect()
}
fn measure<T>(
    sources: &mut Vec<SourceStatus>,
    name: &'static str,
    read: impl FnOnce() -> Result<T, String>,
) -> Option<T> {
    let started = std::time::Instant::now();
    let result = read();
    sources.push(SourceStatus {
        source: name,
        ok: result.is_ok(),
        elapsed_micros: started.elapsed().as_micros() as u64,
        error: result.as_ref().err().cloned(),
    });
    result.ok()
}
#[derive(Clone, Copy)]
struct Cpu {
    idle: u64,
    kernel: u64,
    user: u64,
    awake: u64,
    wall: Duration,
}
fn cpu() -> Result<Cpu, String> {
    let (mut idle, mut kernel, mut user): (FILETIME, FILETIME, FILETIME) = unsafe { mem::zeroed() };
    let mut awake = 0;
    // Out-pointers refer to initialized, correctly sized stack storage.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0
        || unsafe { QueryUnbiasedInterruptTime(&mut awake) } == 0
    {
        return Err(last_error());
    }
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    Ok(Cpu {
        idle: ticks(idle),
        kernel: ticks(kernel),
        user: ticks(user),
        awake,
        wall: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| e.to_string())?,
    })
}
fn derive(old: Option<Cpu>, new: Option<Cpu>) -> Activity {
    let (a, b) = match (old, new) {
        (Some(a), Some(b)) => (a, b),
        (_, Some(_)) => return activity::empty(RateStatus::Warmup),
        _ => return activity::empty(RateStatus::Unavailable),
    };
    if b.awake < a.awake || b.idle < a.idle || b.kernel < a.kernel || b.user < a.user {
        return activity::empty(RateStatus::CounterReset);
    }
    let elapsed = Duration::from_nanos((b.awake - a.awake).saturating_mul(100));
    if b.wall < a.wall || b.wall - a.wall > elapsed + Duration::from_secs(5).max(elapsed / 10) {
        return activity::empty(RateStatus::SleepGap);
    }
    let total = (b.kernel - a.kernel) + (b.user - a.user); // kernel includes idle.
    if total == 0 || elapsed.is_zero() {
        return activity::empty(RateStatus::Warmup);
    }
    let mut result = activity::empty(RateStatus::Ready);
    result.interval_seconds = Some(elapsed.as_secs_f64());
    result.cpu_busy_percent =
        Some(100.0 * total.saturating_sub(b.idle - a.idle) as f64 / total as f64);
    result
}
pub fn disk(path: &Path) -> Result<Disk, String> {
    let name = wide(path.as_os_str());
    let (mut available, mut total, mut free) = (0, 0, 0);
    if unsafe { GetDiskFreeSpaceExW(name.as_ptr(), &mut available, &mut total, &mut free) } == 0 {
        return Err(last_error());
    }
    if total == 0 || available > total {
        return Err("磁盘返回无效容量".into());
    }
    Ok(Disk {
        path: path.display().to_string(),
        total_bytes: total,
        available_bytes: available,
        important_available_bytes: None,
    })
}
pub fn fixed_disks() -> Vec<Disk> {
    ('C'..='Z')
        .filter_map(|letter| {
            let root = format!("{letter}:\\");
            let name = wide(std::ffi::OsStr::new(&root));
            (unsafe { GetDriveTypeW(name.as_ptr()) } == 3)
                .then(|| disk(Path::new(&root)).ok())
                .flatten()
        })
        .collect()
}
fn processes() -> Result<Vec<Process>, String> {
    let mut ids = vec![0u32; 1024];
    let mut needed = 0;
    loop {
        if unsafe { K32EnumProcesses(ids.as_mut_ptr(), (ids.len() * 4) as u32, &mut needed) } == 0 {
            return Err(last_error());
        }
        if (needed as usize) < ids.len() * 4 {
            break;
        }
        if ids.len() >= 262144 {
            return Err("进程列表超出采集上限".into());
        }
        ids.resize(ids.len() * 2, 0);
    }
    ids.truncate(needed as usize / 4);
    let mut result = Vec::new();
    for pid in ids.into_iter().filter(|p| *p != 0) {
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
        if handle == 0 {
            continue;
        }
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { mem::zeroed() };
        let size = mem::size_of_val(&counters) as u32;
        counters.cb = size;
        let mut name = vec![0u16; 32768];
        let mut length = name.len() as u32;
        let ok = unsafe { K32GetProcessMemoryInfo(handle, &mut counters, size) } != 0
            && unsafe { QueryFullProcessImageNameW(handle, 0, name.as_mut_ptr(), &mut length) }
                != 0;
        unsafe {
            CloseHandle(handle);
        }
        if ok {
            let full = String::from_utf16_lossy(&name[..length as usize]);
            result.push(Process {
                pid,
                name: Path::new(&full)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                resident_bytes: counters.WorkingSetSize as u64,
                footprint_bytes: None,
            });
        }
    }
    result.sort_by_key(|p| std::cmp::Reverse(p.resident_bytes));
    result.truncate(5);
    Ok(result)
}
#[derive(Default)]
struct WindowsCollector {
    previous: Option<Cpu>,
}
pub fn collector() -> Result<Box<dyn Collector>, String> {
    Ok(Box::<WindowsCollector>::default())
}
impl Collector for WindowsCollector {
    fn collect(&mut self, include_processes: bool) -> Result<Snapshot, String> {
        let mut sources = Vec::new();
        let memory = measure(&mut sources, "GlobalMemoryStatusEx", || {
            let mut m: MEMORYSTATUSEX = unsafe { mem::zeroed() };
            m.dwLength = mem::size_of_val(&m) as u32;
            if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
                Err(last_error())
            } else {
                Ok(m)
            }
        })
        .ok_or("无法读取 Windows 物理内存")?;
        let perf = measure(&mut sources, "GetPerformanceInfo", || {
            let mut p: PERFORMANCE_INFORMATION = unsafe { mem::zeroed() };
            let size = mem::size_of_val(&p) as u32;
            p.cb = size;
            if unsafe { GetPerformanceInfo(&mut p, size) } == 0 {
                Err(last_error())
            } else {
                Ok(p)
            }
        });
        let current = measure(
            &mut sources,
            "GetSystemTimes/QueryUnbiasedInterruptTime",
            cpu,
        );
        let activity = derive(self.previous, current);
        self.previous = current;
        let disk = measure(&mut sources, "GetDiskFreeSpaceExW", || {
            disk(Path::new("C:\\"))
        });
        let top_processes = if include_processes {
            measure(
                &mut sources,
                "K32EnumProcesses/GetProcessMemoryInfo",
                processes,
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let commit_used = perf.as_ref().map(|p| (p.CommitTotal * p.PageSize) as u64);
        let commit_limit = perf.as_ref().map(|p| (p.CommitLimit * p.PageSize) as u64);
        let free_ratio = memory.ullAvailPhys as f64 / memory.ullTotalPhys.max(1) as f64;
        let commit_ratio = commit_used
            .zip(commit_limit)
            .map(|(u, l)| u as f64 / l.max(1) as f64)
            .unwrap_or(0.0);
        let pressure = if free_ratio < 0.05 || commit_ratio > 0.95 {
            Pressure::Critical
        } else if free_ratio < 0.10 || commit_ratio > 0.85 {
            Pressure::Warning
        } else {
            Pressure::Normal
        };
        let mut limitations = vec!["Windows 内存压力采用可用物理内存 <10%/5%、提交使用率 >85%/95% 的启发式阈值，并非系统压力等级。".into(), "提交量包含 RAM 与分页支持的承诺，不等于 pagefile 占用；交换速率、磁盘 I/O 与温度未采集。进程为可读取的工作集前五名，不能直接求和。超过 64 逻辑处理器时 CPU 仅涵盖当前处理器组。".into()];
        for source in sources.iter().filter(|s| !s.ok) {
            limitations.push(format!(
                "{}：{}",
                source.source,
                source.error.as_deref().unwrap_or("不可用")
            ));
        }
        Ok(Snapshot {
            unix_seconds: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
            platform: "windows",
            windows: Some(WindowsMetrics {
                available_physical_bytes: memory.ullAvailPhys,
                used_physical_bytes: memory.ullTotalPhys.saturating_sub(memory.ullAvailPhys),
                commit_used_bytes: commit_used,
                commit_limit_bytes: commit_limit,
                system_cache_bytes: perf.as_ref().map(|p| (p.SystemCache * p.PageSize) as u64),
            }),
            memory: Memory {
                total_bytes: memory.ullTotalPhys,
                breakdown: None,
                swap: None,
                pressure,
            },
            activity,
            disk,
            thermal: None,
            processes_requested: include_processes,
            top_processes,
            sources,
            limitations,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_handles_idle_and_sleep() {
        let a = Cpu {
            idle: 100,
            kernel: 200,
            user: 100,
            awake: 0,
            wall: Duration::ZERO,
        };
        let mut b = Cpu {
            idle: 150,
            kernel: 275,
            user: 125,
            awake: 10_000_000,
            wall: Duration::from_secs(1),
        };
        assert_eq!(derive(Some(a), Some(b)).cpu_busy_percent, Some(50.0));
        b.wall = Duration::from_secs(90);
        assert_eq!(derive(Some(a), Some(b)).status, RateStatus::SleepGap);
        b.kernel = 0;
        assert_eq!(derive(Some(a), Some(b)).status, RateStatus::CounterReset);
    }
}
