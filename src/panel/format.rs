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

pub fn span(lang: Lang, seconds: u64) -> String {
    let minutes = (seconds + 30) / 60;
    if seconds < 60 {
        tr!(lang, format!("{seconds} 秒"), format!("{seconds} s"))
    } else if seconds < 3600 {
        tr!(
            lang,
            format!("约 {minutes} 分钟"),
            format!("about {minutes} min")
        )
    } else {
        let (hours, rest) = (minutes / 60, minutes % 60);
        match rest {
            0 => tr!(lang, format!("约 {hours} 小时"), format!("about {hours} h")),
            m => tr!(
                lang,
                format!("约 {hours} 小时 {m} 分钟"),
                format!("about {hours} h {m} min")
            ),
        }
    }
}

pub fn interval_label(lang: Lang, seconds: u64) -> String {
    if seconds < 60 || seconds % 60 != 0 {
        tr!(lang, format!("{seconds} 秒"), format!("{seconds} s"))
    } else {
        let minutes = seconds / 60;
        tr!(lang, format!("{minutes} 分钟"), format!("{minutes} min"))
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

pub(crate) fn thermal_label(lang: Lang, state: Option<ThermalState>) -> &'static str {
    match state {
        Some(ThermalState::Nominal) => tr!(lang, "正常", "Normal"),
        Some(ThermalState::Fair) => tr!(lang, "偏热", "Warm"),
        Some(ThermalState::Serious) => tr!(lang, "偏高", "Hot"),
        Some(ThermalState::Critical) => tr!(lang, "严重", "Critical"),
        None => tr!(lang, "未知", "Unknown"),
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

pub(crate) fn pressure_label(lang: Lang, pressure: Pressure) -> &'static str {
    match pressure {
        Pressure::Normal => tr!(lang, "正常", "Normal"),
        Pressure::Warning => tr!(lang, "偏高", "Elevated"),
        Pressure::Critical => tr!(lang, "严重", "Critical"),
        Pressure::Unknown => tr!(lang, "未知", "Unknown"),
    }
}

pub(crate) fn severity_word(lang: Lang, severity: Severity) -> &'static str {
    if severity >= Severity::Critical {
        tr!(lang, "严重", "Critical")
    } else {
        tr!(lang, "需要注意", "Needs attention")
    }
}

pub(crate) fn rule_name(lang: Lang, id: &str) -> &'static str {
    match id {
        "memory_pressure" => tr!(lang, "内存压力", "Memory pressure"),
        "swap_activity" => tr!(lang, "交换读写", "Swap activity"),
        "cpu_saturation" => tr!(lang, "处理器使用率", "Processor usage"),
        "disk_space" => tr!(lang, "磁盘空间", "Disk space"),
        "thermal_state" => tr!(lang, "散热", "Cooling"),
        _ => tr!(lang, "检查项", "Check"),
    }
}

pub(crate) fn tool_label(lang: Lang, tool: Tool) -> &'static str {
    match tool {
        Tool::ActivityMonitor => tr!(lang, "活动监视器", "Activity Monitor"),
        Tool::StorageSettings => tr!(lang, "“储存空间”设置", "Storage settings"),
    }
}

pub(crate) fn tool_path(lang: Lang, tool: Tool) -> &'static str {
    match tool {
        Tool::ActivityMonitor => tr!(
            lang,
            "访达 › 应用程序 › 实用工具 › 活动监视器",
            "Finder › Applications › Utilities › Activity Monitor"
        ),
        Tool::StorageSettings => tr!(
            lang,
            "苹果菜单 › 系统设置 › 通用 › 储存空间",
            "Apple menu › System Settings › General › Storage"
        ),
    }
}

/// Process names are external data: drop control characters and bound the width.
pub(crate) fn display_name(lang: Lang, name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > 32 {
        clean.chars().take(31).chain(['…']).collect()
    } else if clean.is_empty() {
        tr!(lang, "（无名称）", "(no name)").into()
    } else {
        clean
    }
}
