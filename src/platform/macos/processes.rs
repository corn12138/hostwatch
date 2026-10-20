//! Per-process memory and CPU time from libproc, the same kernel counters Activity Monitor
//! shows. Processes of other users or protected system processes are listed but unreadable.

use std::mem;

use crate::model::{Process, ProcessScan};

/// One readable process at one moment.
#[derive(Clone, Debug)]
pub struct ProcStat {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub resident: u64,
    /// Activity Monitor's Memory column.
    pub footprint: u64,
    /// User + system CPU time since launch, nanoseconds.
    pub cpu_ns: u64,
}

pub struct ProcTable {
    pub readable: Vec<ProcStat>,
    pub listed: usize,
}

fn all_pids() -> Result<Vec<i32>, String> {
    // SAFETY: a null buffer asks for the number of pids only.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return Err("系统没有返回任何进程信息".into());
    }
    // Leave headroom for processes started between the two calls.
    let mut pids = vec![0i32; count as usize + 64];
    let bytes = (pids.len() * mem::size_of::<i32>()) as i32;
    // SAFETY: the buffer is valid for `bytes` bytes.
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    if filled <= 0 {
        return Err("系统没有返回任何进程信息".into());
    }
    pids.truncate(filled as usize);
    pids.retain(|&pid| pid > 0);
    Ok(pids)
}

#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}

// libc deprecates its Mach bindings; declare the one stable libSystem symbol directly.
extern "C" {
    fn mach_timebase_info(info: *mut Timebase) -> libc::kern_return_t;
}

/// rusage CPU times are Mach absolute-time ticks (not nanoseconds on Apple silicon).
fn nanos_per_tick() -> f64 {
    let mut timebase = Timebase { numer: 0, denom: 0 };
    // SAFETY: writes the two fields of a live struct.
    unsafe { mach_timebase_info(&mut timebase) };
    if timebase.denom == 0 {
        1.0
    } else {
        f64::from(timebase.numer) / f64::from(timebase.denom)
    }
}

fn bsd_info(pid: i32) -> Option<libc::proc_bsdinfo> {
    let mut info: libc::proc_bsdinfo = unsafe { mem::zeroed() };
    let size = mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: the buffer is a live proc_bsdinfo of `size` bytes.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (written == size).then_some(info)
}

fn c_name(bytes: &[libc::c_char]) -> String {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    let raw: Vec<u8> = bytes[..end].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&raw).into_owned()
}

/// Reads every process the current user may inspect.
pub fn process_table() -> Result<ProcTable, String> {
    let pids = all_pids()?;
    let scale = nanos_per_tick();
    let mut readable = Vec::with_capacity(pids.len());
    for &pid in &pids {
        let mut usage: libc::rusage_info_v2 = unsafe { mem::zeroed() };
        // SAFETY: the buffer is a live rusage_info_v2, matching the requested flavor.
        let ok = unsafe {
            libc::proc_pid_rusage(
                pid,
                libc::RUSAGE_INFO_V2,
                (&mut usage as *mut libc::rusage_info_v2).cast(),
            )
        } == 0;
        if !ok {
            continue;
        }
        let Some(info) = bsd_info(pid) else {
            continue;
        };
        let mut name = c_name(&info.pbi_name);
        if name.is_empty() {
            name = c_name(&info.pbi_comm);
        }
        readable.push(ProcStat {
            pid: pid as u32,
            ppid: info.pbi_ppid,
            name,
            resident: usage.ri_resident_size,
            footprint: usage.ri_phys_footprint,
            cpu_ns: ((usage.ri_user_time + usage.ri_system_time) as f64 * scale) as u64,
        });
    }
    Ok(ProcTable {
        readable,
        listed: pids.len(),
    })
}

/// Which figure ranks the top five.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RankBy {
    /// Resident set size; the CLI's long-standing order.
    Resident,
    /// Physical footprint, matching Activity Monitor's Memory column.
    Footprint,
}

pub fn scan_processes(rank: RankBy) -> Result<ProcessScan, String> {
    let table = process_table()?;
    let key = |p: &ProcStat| match rank {
        RankBy::Resident => p.resident,
        RankBy::Footprint => p.footprint,
    };
    let mut top: Vec<&ProcStat> = table.readable.iter().filter(|p| key(p) > 0).collect();
    let unreadable = table.listed - top.len();
    top.sort_unstable_by(|a, b| key(b).cmp(&key(a)).then(a.pid.cmp(&b.pid)));
    top.truncate(5);
    Ok(ProcessScan {
        top: top
            .into_iter()
            .map(|p| Process {
                pid: p.pid,
                name: p.name.clone(),
                resident_bytes: p.resident,
                footprint_bytes: Some(p.footprint),
            })
            .collect(),
        listed: table.listed,
        unreadable,
    })
}
