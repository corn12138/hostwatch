//! Plain-language readings of rules and issues, separating current values from retained
//! trigger evidence.

use super::*;

pub(crate) struct View<'a> {
    pub(crate) monitor: &'a Monitor,
    pub(crate) health: Health,
    pub(crate) issues: Vec<Issue>,
}

/// The sample to show values from, and whether it is old (paused / stale). `None` after a
/// failed attempt or before the first sample: values are unknown, never stale-as-current.
pub(crate) fn shown<'a>(view: &View<'a>) -> Option<(&'a Latest, bool)> {
    let latest = view.monitor.current()?;
    let old = matches!(view.health, Health::Paused | Health::Stale);
    Some((latest, old))
}

pub(crate) fn last_sample(monitor: &Monitor) -> Option<&Sample> {
    monitor.history().samples().next_back()
}

pub(crate) fn sample_at(monitor: &Monitor, t: u64) -> Option<&Sample> {
    monitor.history().samples().find(|s| s.unix_seconds == t)
}

pub(crate) fn disk_total(monitor: &Monitor) -> Option<u64> {
    monitor
        .latest()
        .and_then(|l| l.snapshot.disk.as_ref())
        .map(|d| d.total_bytes)
}

/// Plain-language reading of one rule from a retained sample.
pub(crate) fn reading(
    monitor: &Monitor,
    id: &str,
    severity: Severity,
    sample: Option<&Sample>,
) -> String {
    let fallback = || format!("达到“{}”提示线", severity_word(severity));
    match id {
        "memory_pressure" => format!(
            "系统内存压力{}",
            if severity >= Severity::Critical {
                "严重"
            } else {
                "偏高"
            }
        ),
        "swap_activity" => sample
            .and_then(|s| s.swap_traffic_bytes_per_second)
            .map_or_else(fallback, |v| format!("交换读写合计 {}", rate(v))),
        "cpu_saturation" => sample
            .and_then(|s| s.cpu_busy_percent)
            .map_or_else(fallback, |v| format!("处理器使用率 {}", percent(v))),
        "disk_space" => match (
            sample.and_then(|s| s.disk_available_bytes),
            disk_total(monitor),
        ) {
            (Some(available), Some(total)) => format!(
                "启动磁盘可用 {} GiB（{:.1}%）",
                gib(available),
                available as f64 / total as f64 * 100.0
            ),
            _ => fallback(),
        },
        _ => fallback(),
    }
}

pub(crate) fn issue_title(issue: &Issue) -> &'static str {
    let has = |id| issue.rules.iter().any(|r| r.id == id);
    match issue.group {
        Group::Memory if has("memory_pressure") && has("swap_activity") => "内存不足，电脑可能变慢",
        Group::Memory if has("memory_pressure") => {
            if issue.severity >= Severity::Critical {
                "内存严重不足"
            } else {
                "内存有点紧张"
            }
        }
        Group::Memory => "电脑正在频繁使用交换",
        Group::Cpu => "处理器持续繁忙",
        Group::Disk => {
            if issue.severity >= Severity::Critical {
                "磁盘空间严重不足"
            } else {
                "磁盘空间不足"
            }
        }
    }
}

pub(crate) fn issue_reason(issue: &Issue) -> &'static str {
    let has = |id| issue.rules.iter().any(|r| r.id == id);
    match issue.group {
        Group::Memory if has("memory_pressure") && has("swap_activity") => {
            "内存已经不够用，系统正在频繁把数据写到磁盘再读回来（交换），这通常会让电脑明显变慢。"
        }
        Group::Memory if has("memory_pressure") => {
            "系统报告内存压力升高，说明内存开始不够用，电脑可能变慢。已用内存多本身很正常，压力才是需要看的信号。"
        }
        Group::Memory => {
            "系统正在持续把内存数据写到磁盘再读回（交换），会让操作变慢。只是“有交换文件”而没有持续读写时，不需要处理。"
        }
        Group::Cpu => {
            "处理器已连续多个样本接近满负荷，风扇可能变响、操作可能变慢。导出视频、编译等任务期间这是正常的。"
        }
        Group::Disk => "启动磁盘可用空间很少，系统更新、保存文件和交换都可能受影响。",
    }
}

pub(crate) fn issue_steps(group: Group) -> [&'static str; 3] {
    match group {
        Group::Memory => [
            "先保存正在编辑的内容。",
            "关闭暂时不用的应用、窗口或浏览器标签页。",
            "在活动监视器的“内存”页查看占用较多的应用，再决定是否正常退出它。",
        ],
        Group::Cpu => [
            "如果你正在运行导出、编译等任务，等它完成即可。",
            "在活动监视器的“CPU”页按“% CPU”排序，看看是哪个应用。",
            "对不需要的应用，用它自己的“退出”命令正常退出。",
        ],
        Group::Disk => [
            "打开“储存空间”设置，看看哪一类占用最多。",
            "把不需要的大文件删除或移到其他磁盘，删除前确认已备份；不要删除系统目录或交换文件。",
            "最后清空废纸篓，空间才会真正释放。",
        ],
    }
}

pub(crate) fn rule_rule(id: &str) -> &'static str {
    match id {
        "memory_pressure" => "压力偏高连续 2 个样本提示，严重时立即提示",
        "swap_activity" => "≥ 1 MiB/s 连续 2 个样本提示，≥ 16 MiB/s 立即提示为严重",
        "cpu_saturation" => "≥ 90% 连续 2 个样本提示",
        _ => "可用 < 15% 或 < 20 GiB 提示；< 5% 或 < 5 GiB 为严重",
    }
}

/// One evidence line per active rule, separating the current reading from the triggering one.
pub(crate) fn evidence(view: &View, rule: &ActiveRule) -> String {
    let monitor = view.monitor;
    let name = rule_name(rule.id);
    match &rule.current {
        Current::Abnormal(severity, _) => {
            let latest = last_sample(monitor);
            let at =
                latest.map_or_else(String::new, |s| format!("，{} 采样", clock(s.unix_seconds)));
            format!(
                "{name}：{}（当前{at}）",
                reading(monitor, rule.id, *severity, latest)
            )
        }
        other => {
            let triggered = rule.status.last_abnormal.as_ref().map_or_else(
                || "此前达到提示线".to_string(),
                |(t, severity, _)| {
                    format!(
                        "{} 时 {}",
                        clock(*t),
                        reading(monitor, rule.id, *severity, sample_at(monitor, *t))
                    )
                },
            );
            let now = match other {
                Current::Clear => "当前已回到正常范围",
                Current::Unknown => "当前无法判断",
                _ => "本次未能读取，当前状态未知",
            };
            format!("{name}：{triggered}；{now}")
        }
    }
}

pub(crate) fn duration_line(view: &View, issue: &Issue) -> Option<String> {
    let since = issue.since?;
    let end = last_sample(view.monitor).map(|s| s.unix_seconds)?;
    let lasted = end.saturating_sub(since);
    Some(if lasted == 0 {
        format!("{} 首次出现", clock(since))
    } else {
        format!("自 {} 起，已持续{}", clock(since), span(lasted))
    })
}

pub(crate) fn recovery_line(issue: &Issue) -> String {
    match issue.recovery {
        Recovery::Ongoing => "异常仍在持续".into(),
        Recovery::Undetermined => "最新样本无法判断，恢复计数暂不前进".into(),
        Recovery::Confirming { clear, required } => {
            format!("正在确认恢复：{clear}/{required} 个正常样本")
        }
    }
}

pub(crate) fn recovery_done(issue: &Issue) -> u32 {
    match issue.recovery {
        Recovery::Confirming { clear, .. } => clear,
        Recovery::Undetermined => issue
            .rules
            .iter()
            .map(|r| r.status.clear_streak)
            .min()
            .unwrap_or(0),
        Recovery::Ongoing => 0,
    }
}
