//! Units, times and short labels shared by every page.

use super::*;

#[cfg(target_os = "macos")]
pub(crate) fn utc_offset(t: u64) -> i64 {
    let time = t as libc::time_t;
    // SAFETY: both pointers refer to live locals; localtime_r is the reentrant variant.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&time, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn utc_offset(_t: u64) -> i64 {
    0
}

/// Local wall-clock time `HH:MM:SS`.
pub fn clock(t: u64) -> String {
    let local = (t as i64 + utc_offset(t)).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        local / 3600,
        local / 60 % 60,
        local % 60
    )
}

pub(crate) fn hm(t: u64) -> String {
    clock(t)[..5].to_string()
}

pub fn span(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds} 秒")
    } else if seconds < 3600 {
        format!("约 {} 分钟", (seconds + 30) / 60)
    } else {
        let minutes = (seconds + 30) / 60;
        match minutes % 60 {
            0 => format!("约 {} 小时", minutes / 60),
            m => format!("约 {} 小时 {m} 分钟", minutes / 60),
        }
    }
}

pub fn interval_label(seconds: u64) -> String {
    if seconds < 60 || seconds % 60 != 0 {
        format!("{seconds} 秒")
    } else {
        format!("{} 分钟", seconds / 60)
    }
}

pub(crate) fn gib(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / GIB as f64)
}

/// Storage in decimal gigabytes, as Finder and System Settings show it.
pub(crate) fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / GB as f64)
}

pub(crate) fn celsius_label(celsius: f64) -> String {
    format!("{celsius:.0}°C")
}

pub(crate) fn thermal_label(state: Option<ThermalState>) -> &'static str {
    match state {
        Some(ThermalState::Nominal) => "正常",
        Some(ThermalState::Fair) => "偏热",
        Some(ThermalState::Serious) => "偏高",
        Some(ThermalState::Critical) => "严重",
        None => "未知",
    }
}

pub(crate) fn size(bytes: u64) -> String {
    if bytes >= GIB {
        format!("{} GiB", gib(bytes))
    } else {
        format!("{:.0} MiB", bytes as f64 / MIB)
    }
}

pub fn percent(value: f64) -> String {
    format!("{value:.0}%")
}

pub fn rate(bytes_per_second: f64) -> String {
    format!("{:.1} MiB/s", bytes_per_second / MIB)
}

pub(crate) fn pressure_label(pressure: Pressure) -> &'static str {
    match pressure {
        Pressure::Normal => "正常",
        Pressure::Warning => "偏高",
        Pressure::Critical => "严重",
        Pressure::Unknown => "未知",
    }
}

pub(crate) fn severity_word(severity: Severity) -> &'static str {
    if severity >= Severity::Critical {
        "严重"
    } else {
        "需要注意"
    }
}

pub(crate) fn rule_name(id: &str) -> &'static str {
    match id {
        "memory_pressure" => "内存压力",
        "swap_activity" => "交换读写",
        "cpu_saturation" => "处理器使用率",
        "disk_space" => "磁盘空间",
        "thermal_state" => "散热",
        _ => "检查项",
    }
}

/// Process names are external data: drop control characters and bound the width.
pub(crate) fn display_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > 32 {
        clean.chars().take(31).chain(['…']).collect()
    } else if clean.is_empty() {
        "（无名称）".into()
    } else {
        clean
    }
}
