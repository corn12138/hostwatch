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
        "runaway_process" => match view_runaway(monitor) {
            Some(runaways) => runaways
                .iter()
                .take(2)
                .map(|r| runaway_line(lang, r))
                .collect::<Vec<_>>()
                .join(tr!(lang, "；", "; ")),
            None => fallback(),
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

/// Runaways in the newest trail reading, if any.
fn view_runaway(monitor: &Monitor) -> Option<Vec<trail::Runaway>> {
    match monitor.trail().runaway() {
        trail::RunawayCheck::Found(found) => Some(found),
        _ => None,
    }
}

pub(crate) fn runaway_line(lang: Lang, runaway: &trail::Runaway) -> String {
    let name = display_name(lang, &runaway.name);
    let percent = runaway.percent;
    match (runaway.background, lang) {
        (true, Lang::Zh) => format!("后台进程 {name} 5 分钟平均 {percent:.0}% 处理器"),
        (false, Lang::Zh) => format!("{name} 5 分钟平均 {percent:.0}% 处理器（不在前台）"),
        (true, Lang::En) => {
            format!("background process {name} averaged {percent:.0}% CPU for 5 minutes")
        }
        (false, Lang::En) => {
            format!("{name} averaged {percent:.0}% CPU for 5 minutes while not in front")
        }
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
        Group::Cpu if !has("cpu_saturation") => tr!(
            lang,
            "有进程一直占满处理器",
            "A process keeps the processor busy"
        ),
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
        Group::Cpu if !has("cpu_saturation") => tr!(
            lang,
            "整台电脑还不算满，但有一个应用或后台进程已持续 5 分钟占满至少一个核心。它会让风扇变响、耗电变快，也常常是程序卡住或陷入循环的迹象；导出、编译或备份期间则属于正常。",
            "The Mac as a whole is not maxed out, but one app or background process has kept at least a full core busy for 5 minutes. That makes fans louder and drains the battery, and it is often a sign of a stuck or looping program; during an export, build or backup it is normal."
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
        "runaway_process" => tr!(
            lang,
            "某个应用或后台进程 5 分钟平均 ≥ 100%（满一个核心）且期间不在前台，连续 2 次记录提示",
            "An app or background process averages ≥ 100% (a full core) over 5 minutes while not in front, at 2 records in a row"
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

/// What the per-app trail shows for the minutes before an issue began; `None` for disk space,
/// which has no app to name, or when the start is unknown.
pub(crate) fn lead(view: &View, issue: &Issue) -> Option<Lead> {
    let focus = Focus::for_group(issue.group)?;
    Some(view.monitor.trail().lead(focus, issue.since?))
}

pub(crate) fn rise_line(lang: Lang, rise: &Rise) -> String {
    let label = |name: &str, background: bool| {
        let name = display_name(lang, name);
        match (background, lang) {
            (true, Lang::Zh) => format!("{name}（后台进程）"),
            (true, Lang::En) => format!("{name} (background process)"),
            (false, _) => name,
        }
    };
    match rise {
        Rise::Memory {
            name,
            background,
            from: Some(from),
            to,
        } => {
            let (name, grew) = (label(name, *background), size(to.saturating_sub(*from)));
            let (from, to) = (size(*from), size(*to));
            tr!(
                lang,
                format!("{name}：{from} → {to}（+{grew}）"),
                format!("{name}: {from} → {to} (+{grew})")
            )
        }
        Rise::Memory {
            name,
            background: false,
            from: None,
            to,
        } => {
            let (name, to) = (display_name(lang, name), size(*to));
            tr!(
                lang,
                format!("{name}：期间新打开，现占 {to}"),
                format!("{name}: opened meanwhile, now {to}")
            )
        }
        Rise::Memory {
            name,
            background: true,
            from: None,
            to,
        } => {
            let (name, to) = (label(name, true), size(*to));
            let floor = size(trail::BACKGROUND_MEMORY);
            tr!(
                lang,
                format!("{name}：此前低于 {floor}，现占 {to}"),
                format!("{name}: under {floor} before, now {to}")
            )
        }
        Rise::Cpu {
            name,
            background,
            percent,
        } => {
            let name = label(name, *background);
            tr!(
                lang,
                format!("{name}：平均 {percent:.0}% 处理器"),
                format!("{name}: {percent:.0}% CPU on average")
            )
        }
    }
}

/// The Lead as lines for the issue page: what rose, then how it was measured.
pub(crate) fn lead_blocks(view: &View, issue: &Issue, blocks: &mut Vec<Block>) {
    let Some(lead) = lead(view, issue) else {
        return;
    };
    let lang = view.lang;
    let memory = issue.group == Group::Memory;
    blocks.push(Block::Heading(
        tr!(lang, "开始之前", "Before it started").into(),
    ));
    let every = trail::EVERY;
    match lead {
        Lead::NotEnough => blocks.push(Block::Text(tr!(
            lang,
            format!("记录还不够：Hostwatch 每 {every} 秒记录一次已打开应用和较忙后台进程的用量，只保留最近约 10 分钟；这个问题开始前的记录不足以比较。"),
            format!("Not enough record yet: Hostwatch notes open apps' and busy background processes' use every {every} seconds and keeps about the last 10 minutes; there is too little before this issue began to compare.")
        ))),
        Lead::Quiet { from, to } => {
            let (from, to) = (hm(from), hm(to));
            blocks.push(Block::Text(if memory {
                tr!(
                    lang,
                    format!("{from}–{to} 之间没有哪个应用或后台进程的内存增加 256 MiB 以上；可能是多个进程一起增长，或来自读不到的系统进程。"),
                    format!("Between {from} and {to}, no app or background process grew by 256 MiB or more; several may have grown together, or it comes from system processes Hostwatch cannot read.")
                )
            } else {
                tr!(
                    lang,
                    format!("{from}–{to} 之间没有哪个应用或后台进程平均占用 25% 以上处理器；可能是许多进程一起，或来自读不到的系统进程。"),
                    format!("Between {from} and {to}, no app or background process averaged 25% CPU or more; it may be many processes together, or system processes Hostwatch cannot read.")
                )
            }));
        }
        Lead::Rose { from, to, rises } => {
            let (from, to) = (hm(from), hm(to));
            blocks.push(Block::Text(if memory {
                tr!(
                    lang,
                    format!("{from}–{to} 之间内存增长最多的（应用含辅助进程）："),
                    format!("Grew the most in memory between {from} and {to} (apps include their helpers):")
                )
            } else {
                tr!(
                    lang,
                    format!("{from}–{to} 之间处理器占用最高的（应用含辅助进程）："),
                    format!("Used the most CPU between {from} and {to} (apps include their helpers):")
                )
            }));
            blocks.push(Block::Bullets(
                rises.iter().map(|r| rise_line(lang, r)).collect(),
            ));
        }
    }
    blocks.push(Block::Note(
        tr!(
            lang,
            "只在内存中保留，不写入磁盘；后台进程只在处理器 ≥ 5% 或内存 ≥ 512 MiB 时记录，读不到的系统进程不在其中。上升不等于就是原因，可结合“一键优化”的当前占用判断。",
            "Kept in memory only, never written to disk; background processes are noted only above 5% CPU or 512 MiB, and system processes Hostwatch cannot read are missing. Rising is not proof of cause; compare with the current use shown by Optimize."
        )
        .into(),
    ));
}
