//! Plain-language readings of rules and issues, separating current values from retained
//! trigger evidence.

use super::*;

pub(crate) struct View<'a> {
    pub(crate) monitor: &'a Monitor,
    pub(crate) health: Health,
    pub(crate) issues: Vec<Issue>,
    pub(crate) lang: Lang,
    pub(crate) ui: Ui,
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
    lang: Lang,
    monitor: &Monitor,
    id: &str,
    severity: Severity,
    sample: Option<&Sample>,
) -> String {
    let critical = severity >= Severity::Critical;
    let fallback = || {
        let word = severity_word(lang, severity);
        tr!(
            lang,
            format!("达到“{word}”提示线"),
            format!("reached the “{word}” threshold")
        )
    };
    match id {
        "memory_pressure" => match (lang, critical) {
            (Lang::Zh, true) => "系统内存压力严重".into(),
            (Lang::Zh, false) => "系统内存压力偏高".into(),
            (Lang::En, true) => "system memory pressure is critical".into(),
            (Lang::En, false) => "system memory pressure is elevated".into(),
        },
        "swap_activity" => sample
            .and_then(|s| s.swap_traffic_bytes_per_second)
            .map_or_else(fallback, |v| {
                tr!(
                    lang,
                    format!("交换读写合计 {}", rate(v)),
                    format!("swap reads + writes {}", rate(v))
                )
            }),
        "cpu_saturation" => sample
            .and_then(|s| s.cpu_busy_percent)
            .map_or_else(fallback, |v| {
                tr!(
                    lang,
                    format!("处理器使用率 {}", percent(v)),
                    format!("processor usage {}", percent(v))
                )
            }),
        "disk_space" => match (
            sample.and_then(|s| s.disk_available_bytes),
            disk_total(monitor),
        ) {
            (Some(available), Some(total)) => {
                let share = available as f64 / total as f64 * 100.0;
                tr!(
                    lang,
                    format!("启动磁盘可用 {}（{share:.1}%）", gb(available)),
                    format!("{} free on the startup disk ({share:.1}%)", gb(available))
                )
            }
            _ => fallback(),
        },
        "thermal_state" => {
            let level = match (lang, critical) {
                (Lang::Zh, true) => "系统散热等级严重",
                (Lang::Zh, false) => "系统散热等级偏高",
                (Lang::En, true) => "the system cooling level is critical",
                (Lang::En, false) => "the system cooling level is high",
            };
            match sample.and_then(|s| s.cpu_celsius) {
                Some(celsius) => tr!(
                    lang,
                    format!("{level}（芯片约 {}）", celsius_label(celsius)),
                    format!("{level} (chip about {})", celsius_label(celsius))
                ),
                None => level.into(),
            }
        }
        _ => fallback(),
    }
}

pub(crate) fn issue_title(lang: Lang, issue: &Issue) -> &'static str {
    let has = |id| issue.rules.iter().any(|r| r.id == id);
    let critical = issue.severity >= Severity::Critical;
    match issue.group {
        Group::Memory if has("memory_pressure") && has("swap_activity") => tr!(
            lang,
            "内存不足，电脑可能变慢",
            "Memory is running low; the Mac may slow down"
        ),
        Group::Memory if has("memory_pressure") => match (lang, critical) {
            (Lang::Zh, true) => "内存严重不足",
            (Lang::Zh, false) => "内存有点紧张",
            (Lang::En, true) => "Memory is critically low",
            (Lang::En, false) => "Memory is getting tight",
        },
        Group::Memory => tr!(lang, "电脑正在频繁使用交换", "The Mac is swapping heavily"),
        Group::Cpu => tr!(lang, "处理器持续繁忙", "The processor stays busy"),
        Group::Disk => match (lang, critical) {
            (Lang::Zh, true) => "磁盘空间严重不足",
            (Lang::Zh, false) => "磁盘空间不足",
            (Lang::En, true) => "Disk space is critically low",
            (Lang::En, false) => "Disk space is low",
        },
        Group::Thermal => match (lang, critical) {
            (Lang::Zh, true) => "电脑过热，性能已被大幅限制",
            (Lang::Zh, false) => "电脑有点热，可能已降速",
            (Lang::En, true) => "The Mac is overheating and heavily slowed",
            (Lang::En, false) => "The Mac is hot and may be slowed",
        },
    }
}

pub(crate) fn issue_reason(lang: Lang, issue: &Issue) -> &'static str {
    let has = |id| issue.rules.iter().any(|r| r.id == id);
    match issue.group {
        Group::Memory if has("memory_pressure") && has("swap_activity") => tr!(
            lang,
            "内存已经不够用，系统正在频繁把数据写到磁盘再读回来（交换），这通常会让电脑明显变慢。",
            "Memory has run short, so macOS keeps writing data to disk and reading it back (swapping). That usually makes the Mac noticeably slower."
        ),
        Group::Memory if has("memory_pressure") => tr!(
            lang,
            "系统报告内存压力升高，说明内存开始不够用，电脑可能变慢。已用内存多本身很正常，压力才是需要看的信号。",
            "macOS reports rising memory pressure: memory is starting to run short and the Mac may slow down. High memory use alone is normal; pressure is the signal that matters."
        ),
        Group::Memory => tr!(
            lang,
            "系统正在持续把内存数据写到磁盘再读回（交换），会让操作变慢。只是“有交换文件”而没有持续读写时，不需要处理。",
            "macOS keeps moving memory to disk and back (swapping), which slows things down. A swap file without ongoing reads and writes needs no action."
        ),
        Group::Cpu => tr!(
            lang,
            "处理器已连续多个样本接近满负荷，风扇可能变响、操作可能变慢。导出视频、编译等任务期间这是正常的。",
            "The processor has been near full load for several samples, so fans may get louder and things may slow down. This is normal while exporting video or compiling."
        ),
        Group::Disk => tr!(
            lang,
            "启动磁盘可用空间很少，系统更新、保存文件和交换都可能受影响。",
            "The startup disk is almost full, which can affect system updates, saving files and swap."
        ),
        Group::Thermal => tr!(
            lang,
            "macOS 报告散热压力，正在降低处理器速度来保护硬件，所以电脑会变慢。通常由持续高负载、通风不好或环境太热引起。",
            "macOS reports heat pressure and is slowing the processor to protect the hardware, so the Mac gets slower. Usually caused by sustained heavy load, poor airflow or a hot room."
        ),
    }
}

pub(crate) fn issue_steps(lang: Lang, group: Group) -> [&'static str; 3] {
    match (group, lang) {
        (Group::Memory, Lang::Zh) => [
            "先保存正在编辑的内容。",
            "关闭暂时不用的应用、窗口或浏览器标签页。",
            "在活动监视器的“内存”页查看占用较多的应用，再决定是否正常退出它。",
        ],
        (Group::Memory, Lang::En) => [
            "Save what you are working on first.",
            "Close apps, windows or browser tabs you are not using.",
            "In Activity Monitor's Memory tab, find the apps using the most, then decide whether to quit them normally.",
        ],
        (Group::Cpu, Lang::Zh) => [
            "如果你正在运行导出、编译等任务，等它完成即可。",
            "在活动监视器的“CPU”页按“% CPU”排序，看看是哪个应用。",
            "对不需要的应用，用它自己的“退出”命令正常退出。",
        ],
        (Group::Cpu, Lang::En) => [
            "If an export, build or similar task is running, let it finish.",
            "In Activity Monitor's CPU tab, sort by “% CPU” to see which app it is.",
            "Quit apps you do not need with their own Quit command.",
        ],
        (Group::Disk, Lang::Zh) => [
            "打开“储存空间”设置，看看哪一类占用最多。",
            "把不需要的大文件删除或移到其他磁盘，删除前确认已备份；不要删除系统目录或交换文件。",
            "最后清空废纸篓，空间才会真正释放。",
        ],
        (Group::Disk, Lang::En) => [
            "Open Storage settings to see what takes the most space.",
            "Delete or move large files you do not need, after checking they are backed up. Never delete system folders or swap files.",
            "Empty the Trash afterwards; only then is the space actually freed.",
        ],
        (Group::Thermal, Lang::Zh) => [
            "让电脑底部和出风口保持通风，不要放在被褥或软垫上。",
            "退出暂时不用的高负载应用，可以用“一键优化”或活动监视器的“CPU”页。",
            "如果没有在做重活也一直过热，检查风扇和灰尘，或联系售后。",
        ],
        (Group::Thermal, Lang::En) => [
            "Keep the bottom and vents clear; avoid beds, blankets and soft surfaces.",
            "Quit heavy apps you are not using, with Optimize or Activity Monitor's CPU tab.",
            "If it stays hot without heavy work, check the fans and dust, or contact service.",
        ],
    }
}

pub(crate) fn rule_rule(lang: Lang, id: &str) -> &'static str {
    match id {
        "memory_pressure" => tr!(
            lang,
            "压力偏高连续 2 个样本提示，严重时立即提示",
            "Elevated pressure for 2 samples in a row; critical pressure at once"
        ),
        "swap_activity" => tr!(
            lang,
            "≥ 1 MiB/s 连续 2 个样本提示，≥ 16 MiB/s 立即提示为严重",
            "≥ 1 MiB/s for 2 samples in a row; ≥ 16 MiB/s is critical at once"
        ),
        "cpu_saturation" => tr!(
            lang,
            "≥ 90% 连续 2 个样本提示",
            "≥ 90% for 2 samples in a row"
        ),
        "thermal_state" => tr!(
            lang,
            "macOS 散热等级“偏高”连续 2 个样本提示，“严重”立即提示；温度数字本身不触发提示",
            "macOS cooling level “Hot” for 2 samples in a row, “Critical” at once; temperatures alone never alert"
        ),
        _ => tr!(
            lang,
            "可用 < 15% 或 < 20 GB 提示；< 5% 或 < 5 GB 为严重",
            "Free < 15% or < 20 GB; < 5% or < 5 GB is critical"
        ),
    }
}

/// One evidence line per active rule, separating the current reading from the triggering one.
pub(crate) fn evidence(view: &View, rule: &ActiveRule) -> String {
    let (lang, monitor) = (view.lang, view.monitor);
    let name = rule_name(lang, rule.id);
    match &rule.current {
        Current::Abnormal(severity, _) => {
            let latest = last_sample(monitor);
            let value = reading(lang, monitor, rule.id, *severity, latest);
            match latest.map(|s| clock(s.unix_seconds)) {
                Some(at) => tr!(
                    lang,
                    format!("{name}：{value}（当前，{at} 采样）"),
                    format!("{name}: {value} (now, sampled {at})")
                ),
                None => tr!(
                    lang,
                    format!("{name}：{value}（当前）"),
                    format!("{name}: {value} (now)")
                ),
            }
        }
        other => {
            let triggered = rule.status.last_abnormal.as_ref().map_or_else(
                || tr!(lang, "此前达到提示线", "earlier reached the threshold").to_string(),
                |(t, severity, _)| {
                    let value = reading(lang, monitor, rule.id, *severity, sample_at(monitor, *t));
                    let at = clock(*t);
                    tr!(
                        lang,
                        format!("{at} 时 {value}"),
                        format!("at {at}: {value}")
                    )
                },
            );
            let now = match other {
                Current::Clear => tr!(lang, "当前已回到正常范围", "now back in the normal range"),
                Current::Unknown => tr!(lang, "当前无法判断", "cannot be judged right now"),
                _ => tr!(
                    lang,
                    "本次未能读取，当前状态未知",
                    "the latest reading failed, so the current state is unknown"
                ),
            };
            tr!(
                lang,
                format!("{name}：{triggered}；{now}"),
                format!("{name}: {triggered}; {now}")
            )
        }
    }
}

pub(crate) fn duration_line(view: &View, issue: &Issue) -> Option<String> {
    let lang = view.lang;
    let since = issue.since?;
    let end = last_sample(view.monitor).map(|s| s.unix_seconds)?;
    let lasted = end.saturating_sub(since);
    let start = clock(since);
    Some(if lasted == 0 {
        tr!(
            lang,
            format!("{start} 首次出现"),
            format!("first seen at {start}")
        )
    } else {
        let length = span(lang, lasted);
        tr!(
            lang,
            format!("自 {start} 起，已持续{length}"),
            format!("since {start}, for {length}")
        )
    })
}

pub(crate) fn recovery_line(lang: Lang, issue: &Issue) -> String {
    match issue.recovery {
        Recovery::Ongoing => tr!(lang, "异常仍在持续", "Still ongoing").into(),
        Recovery::Undetermined => tr!(
            lang,
            "最新样本无法判断，恢复计数暂不前进",
            "The latest sample cannot be judged; recovery does not advance"
        )
        .into(),
        Recovery::Confirming { clear, required } => tr!(
            lang,
            format!("正在确认恢复：{clear}/{required} 个正常样本"),
            format!("Confirming recovery: {clear}/{required} normal samples")
        ),
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
