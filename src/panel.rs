//! Pages of the menu-bar popover as plain data. `menubar` turns `Block`s into AppKit views and
//! sends `Command`s back; all copy, numbers and chart geometry come from here so text, charts
//! and accessibility summaries share one source.

use crate::advice::{GIB, MIB};
use crate::alerts::Sample;
use crate::model::{Pressure, RateStatus, Severity};
use crate::monitor::{
    unmeasured, ActiveRule, Current, Group, Health, Issue, Latest, Mark, Monitor, Processes,
    Recovery, Tool, INTERVAL_PRESETS,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Memory,
    Cpu,
    Disk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Home,
    Detail(Resource),
    Issues,
    Handling(Group),
    Processes,
    Settings,
}

impl Route {
    pub fn title(self) -> &'static str {
        match self {
            Route::Home => "概览",
            Route::Detail(Resource::Memory) => "内存",
            Route::Detail(Resource::Cpu) => "处理器",
            Route::Detail(Resource::Disk) => "磁盘空间",
            Route::Issues => "问题列表",
            Route::Handling(_) => "处理方法",
            Route::Processes => "进程排行",
            Route::Settings => "设置",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warning,
    Critical,
    Unknown,
    Paused,
    Recovering,
    Loading,
    Neutral,
}

impl Tone {
    fn of(severity: Severity) -> Self {
        if severity >= Severity::Critical {
            Tone::Critical
        } else {
            Tone::Warning
        }
    }

    /// Spoken/visible name of the status shape, so state never depends on color alone.
    pub fn label(self) -> &'static str {
        match self {
            Tone::Ok => "正常",
            Tone::Warning => "需要注意",
            Tone::Critical => "严重",
            Tone::Unknown => "未知",
            Tone::Paused => "已暂停",
            Tone::Recovering => "正在确认恢复",
            Tone::Loading => "正在读取",
            Tone::Neutral => "提示",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Go(Route),
    Back,
    TogglePause,
    SetInterval(u64),
    Open(Tool),
    /// Collect a sample now.
    Retry,
    RefreshProcesses,
    More,
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub title: String,
    pub command: Command,
    pub primary: bool,
    pub enabled: bool,
}

impl Button {
    fn new(title: impl Into<String>, command: Command) -> Self {
        Self {
            title: title.into(),
            command,
            primary: false,
            enabled: true,
        }
    }

    fn primary(mut self) -> Self {
        self.primary = true;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gap {
    pub from: u64,
    pub to: u64,
    pub label: &'static str,
}

/// A time series in data units; the renderer maps `t0..t1` and `0..y_max` to its bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub title: String,
    pub unit: String,
    pub y_max: f64,
    pub y_ticks: Vec<(f64, String)>,
    pub thresholds: Vec<(f64, String)>,
    pub t0: u64,
    pub t1: u64,
    /// Connected runs; a run of one point is drawn as a dot.
    pub segments: Vec<Vec<(u64, f64)>>,
    pub gaps: Vec<Gap>,
    pub x_labels: Vec<(u64, String)>,
    pub summary: String,
    pub note: String,
    /// Replaces the plot when there is nothing honest to draw.
    pub empty: Option<String>,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Graphic {
    Chart(Chart),
    /// Small inline trend without axes.
    Spark(Chart),
    Bar {
        fraction: f64,
        tone: Tone,
        label: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRow {
    pub rank: usize,
    pub name: String,
    pub size: String,
    pub fraction: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Conclusion {
        tone: Tone,
        title: String,
        subtitle: String,
    },
    Header {
        back: String,
        title: String,
        badge: Option<(Tone, String)>,
    },
    Resource {
        resource: Resource,
        tone: Tone,
        name: String,
        value: String,
        status: String,
        graphic: Option<Graphic>,
        dim: bool,
        label: String,
    },
    Card {
        tone: Tone,
        title: String,
        body: Vec<String>,
        buttons: Vec<Button>,
    },
    Heading(String),
    Text(String),
    Note(String),
    Big {
        value: String,
        unit: String,
    },
    Graphic(Graphic),
    Facts(Vec<(String, String)>),
    Steps(Vec<String>),
    Bullets(Vec<String>),
    Buttons(Vec<Button>),
    /// Mutually exclusive options; the selected one is `primary`.
    Choices(Vec<Button>),
    Progress {
        done: u32,
        total: u32,
        label: String,
    },
    Processes(Vec<ProcessRow>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Footer {
    pub pause: Button,
    pub interval: Button,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub blocks: Vec<Block>,
    pub footer: Footer,
}

// ---------------------------------------------------------------- formatting

#[cfg(target_os = "macos")]
fn utc_offset(t: u64) -> i64 {
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
fn utc_offset(_t: u64) -> i64 {
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

fn hm(t: u64) -> String {
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

fn gib(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / GIB as f64)
}

fn size(bytes: u64) -> String {
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

fn pressure_label(pressure: Pressure) -> &'static str {
    match pressure {
        Pressure::Normal => "正常",
        Pressure::Warning => "偏高",
        Pressure::Critical => "严重",
        Pressure::Unknown => "未知",
    }
}

fn severity_word(severity: Severity) -> &'static str {
    if severity >= Severity::Critical {
        "严重"
    } else {
        "需要注意"
    }
}

fn rule_name(id: &str) -> &'static str {
    match id {
        "memory_pressure" => "内存压力",
        "swap_activity" => "交换读写",
        "cpu_saturation" => "处理器使用率",
        "disk_space" => "磁盘空间",
        _ => "检查项",
    }
}

/// Process names are external data: drop control characters and bound the width.
fn display_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > 32 {
        clean.chars().take(31).chain(['…']).collect()
    } else if clean.is_empty() {
        "（无名称）".into()
    } else {
        clean
    }
}

// ---------------------------------------------------------------- shared reading helpers

struct View<'a> {
    monitor: &'a Monitor,
    health: Health,
    issues: Vec<Issue>,
}

/// The sample to show values from, and whether it is old (paused / stale). `None` after a
/// failed attempt or before the first sample: values are unknown, never stale-as-current.
fn shown<'a>(view: &View<'a>) -> Option<(&'a Latest, bool)> {
    let latest = view.monitor.current()?;
    let old = matches!(view.health, Health::Paused | Health::Stale);
    Some((latest, old))
}

fn last_sample(monitor: &Monitor) -> Option<&Sample> {
    monitor.history().samples().next_back()
}

fn sample_at(monitor: &Monitor, t: u64) -> Option<&Sample> {
    monitor.history().samples().find(|s| s.unix_seconds == t)
}

fn disk_total(monitor: &Monitor) -> Option<u64> {
    monitor
        .latest()
        .and_then(|l| l.snapshot.disk.as_ref())
        .map(|d| d.total_bytes)
}

/// Plain-language reading of one rule from a retained sample.
fn reading(monitor: &Monitor, id: &str, severity: Severity, sample: Option<&Sample>) -> String {
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

fn issue_title(issue: &Issue) -> &'static str {
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

fn issue_reason(issue: &Issue) -> &'static str {
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

fn issue_steps(group: Group) -> [&'static str; 3] {
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

fn rule_rule(id: &str) -> &'static str {
    match id {
        "memory_pressure" => "压力偏高连续 2 个样本提示，严重时立即提示",
        "swap_activity" => "≥ 1 MiB/s 连续 2 个样本提示，≥ 16 MiB/s 立即提示为严重",
        "cpu_saturation" => "≥ 90% 连续 2 个样本提示",
        _ => "可用 < 15% 或 < 20 GiB 提示；< 5% 或 < 5 GiB 为严重",
    }
}

/// One evidence line per active rule, separating the current reading from the triggering one.
fn evidence(view: &View, rule: &ActiveRule) -> String {
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

fn duration_line(view: &View, issue: &Issue) -> Option<String> {
    let since = issue.since?;
    let end = last_sample(view.monitor).map(|s| s.unix_seconds)?;
    let lasted = end.saturating_sub(since);
    Some(if lasted == 0 {
        format!("{} 首次出现", clock(since))
    } else {
        format!("自 {} 起，已持续{}", clock(since), span(lasted))
    })
}

fn recovery_line(issue: &Issue) -> String {
    match issue.recovery {
        Recovery::Ongoing => "异常仍在持续".into(),
        Recovery::Undetermined => "最新样本无法判断，恢复计数暂不前进".into(),
        Recovery::Confirming { clear, required } => {
            format!("正在确认恢复：{clear}/{required} 个正常样本")
        }
    }
}

fn recovery_done(issue: &Issue) -> u32 {
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

// ---------------------------------------------------------------- charts

fn gap_label(monitor: &Monitor, a: u64, b: u64) -> Option<&'static str> {
    let between = |m: &&Mark| match **m {
        Mark::Failed(t) | Mark::Paused(t) | Mark::Resumed(t) => a < t && t <= b,
        Mark::SleepGap(t) => t == b,
        Mark::Interval(..) => false,
    };
    let marks: Vec<&Mark> = monitor.marks().filter(between).collect();
    if marks
        .iter()
        .any(|m| matches!(m, Mark::Paused(_) | Mark::Resumed(_)))
    {
        return Some("暂停");
    }
    if marks.iter().any(|m| matches!(m, Mark::SleepGap(_))) {
        return Some("睡眠");
    }
    if marks.iter().any(|m| matches!(m, Mark::Failed(_))) {
        return Some("采集失败");
    }
    let expected = monitor.interval_at(a).max(monitor.interval_at(b));
    // Timer tolerance is 10%; anything well beyond one cadence is missing data.
    (b - a > expected * 3 / 2 + 2).then_some("无数据")
}

fn series(
    monitor: &Monitor,
    value: impl Fn(&Sample) -> Option<f64>,
) -> (Vec<Vec<(u64, f64)>>, Vec<Gap>) {
    let mut segments: Vec<Vec<(u64, f64)>> = Vec::new();
    let mut gaps = Vec::new();
    let mut run: Vec<(u64, f64)> = Vec::new();
    let mut previous: Option<u64> = None;
    for sample in monitor.history().samples() {
        let t = sample.unix_seconds;
        if let Some(a) = previous {
            if let Some(label) = gap_label(monitor, a, t) {
                gaps.push(Gap {
                    from: a,
                    to: t,
                    label,
                });
                if !run.is_empty() {
                    segments.push(std::mem::take(&mut run));
                }
            }
        }
        match value(sample) {
            Some(v) => run.push((t, v)),
            None => {
                if !run.is_empty() {
                    segments.push(std::mem::take(&mut run));
                }
            }
        }
        previous = Some(t);
    }
    if !run.is_empty() {
        segments.push(run);
    }
    (segments, gaps)
}

fn nice_ceiling(value: f64) -> f64 {
    let mut step = 1.0;
    loop {
        for m in [2.0, 5.0, 10.0] {
            if value <= step * m {
                return step * m;
            }
        }
        step *= 10.0;
    }
}

struct Spec<'a> {
    title: &'a str,
    unit: &'a str,
    format: fn(f64) -> String,
    empty: &'a str,
    tone: Tone,
}

fn chart(
    monitor: &Monitor,
    spec: Spec,
    y_max: f64,
    y_ticks: Vec<(f64, String)>,
    thresholds: Vec<(f64, String)>,
    value: impl Fn(&Sample) -> Option<f64>,
) -> Chart {
    let (segments, gaps) = series(monitor, &value);
    let samples = monitor.history().len();
    let points: Vec<(u64, f64)> = segments.iter().flatten().copied().collect();
    let t0 = monitor
        .history()
        .samples()
        .next()
        .map_or(0, |s| s.unix_seconds);
    let t1 = last_sample(monitor).map_or(0, |s| s.unix_seconds);
    let empty = points.is_empty().then(|| spec.empty.to_string());
    let x_labels = if t1 > t0 {
        let label = if t1 - t0 < 120 { clock } else { hm };
        vec![(t0, label(t0)), (t1, label(t1))]
    } else {
        Vec::new()
    };
    let mut summary = format!("{}，", spec.title);
    let mut note = String::new();
    if let (Some(&(first, _)), Some(&(last, latest))) = (points.first(), points.last()) {
        let min = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        summary.push_str(&format!(
            "{} 至 {}，最低 {}，最高 {}，最近 {}（{}）",
            clock(first),
            clock(last),
            (spec.format)(min),
            (spec.format)(max),
            (spec.format)(latest),
            clock(last)
        ));
        if !gaps.is_empty() {
            let list: Vec<String> = gaps
                .iter()
                .map(|g| format!("{}–{} {}", hm(g.from), hm(g.to), g.label))
                .collect();
            summary.push_str(&format!("；有 {} 段空白：{}", gaps.len(), list.join("，")));
        }
        note = if samples < 10 {
            format!("历史不足：已有 {samples} 个样本（最多保留 120 个），只显示已有范围。")
        } else {
            format!(
                "最近 {samples} 个样本，{}–{}（{}）。空白表示没有数据，不按 0 计算。",
                hm(t0),
                hm(t1),
                span(t1 - t0)
            )
        };
    } else {
        summary.push_str(spec.empty);
    }
    Chart {
        title: spec.title.into(),
        unit: spec.unit.into(),
        y_max,
        y_ticks,
        thresholds,
        t0,
        t1,
        segments,
        gaps,
        x_labels,
        summary,
        note,
        empty,
        tone: spec.tone,
    }
}

fn rate_empty(monitor: &Monitor) -> &'static str {
    if monitor.history().len() < 2 {
        "还没有历史数据：需要至少 2 个样本才能计算速率"
    } else {
        "数据暂不可用（显示为未知，不是 0）"
    }
}

pub fn cpu_chart(monitor: &Monitor) -> Chart {
    chart(
        monitor,
        Spec {
            title: "处理器使用率",
            unit: "%",
            format: percent,
            empty: rate_empty(monitor),
            tone: Tone::Neutral,
        },
        100.0,
        vec![
            (0.0, "0".into()),
            (50.0, "50".into()),
            (100.0, "100".into()),
        ],
        vec![(90.0, "90".into())],
        |s| s.cpu_busy_percent,
    )
}

pub fn memory_chart(monitor: &Monitor, total: u64) -> Chart {
    let total_gib = total as f64 / GIB as f64;
    chart(
        monitor,
        Spec {
            title: "已用内存",
            unit: "GiB",
            format: |v| format!("{v:.1} GiB"),
            empty: if monitor.history().len() == 0 {
                "正在读取，还没有历史数据"
            } else {
                "数据暂不可用（显示为未知，不是 0）"
            },
            tone: Tone::Neutral,
        },
        total_gib,
        vec![(0.0, "0".into()), (total_gib, format!("{total_gib:.0}"))],
        Vec::new(),
        |s| s.memory_used_bytes.map(|b| b as f64 / GIB as f64),
    )
}

pub fn swap_chart(monitor: &Monitor) -> Chart {
    let peak = monitor
        .history()
        .samples()
        .filter_map(|s| s.swap_traffic_bytes_per_second)
        .fold(0.0, f64::max)
        / MIB;
    let y_max = nice_ceiling((peak * 1.2).max(2.0));
    let thresholds = [(1.0, "1 需要注意"), (16.0, "16 严重")]
        .into_iter()
        .filter(|(v, _)| *v <= y_max)
        .map(|(v, l)| (v, l.to_string()))
        .collect();
    chart(
        monitor,
        Spec {
            title: "交换读写合计",
            unit: "MiB/s",
            format: |v| format!("{v:.1} MiB/s"),
            empty: rate_empty(monitor),
            tone: Tone::Neutral,
        },
        y_max,
        vec![(0.0, "0".into()), (y_max, format!("{y_max:.0}"))],
        thresholds,
        |s| s.swap_traffic_bytes_per_second.map(|v| v / MIB),
    )
}

// ---------------------------------------------------------------- page assembly

pub fn page(monitor: &Monitor, route: Route, back: Option<Route>, now: u64) -> Page {
    let view = View {
        monitor,
        health: monitor.health(now),
        issues: monitor.issues(),
    };
    let mut blocks = Vec::new();
    if let Some(back) = back {
        blocks.push(Block::Header {
            back: back.title().into(),
            title: match route {
                Route::Processes => "占用内存最多的进程".into(),
                Route::Settings => "设置与数据说明".into(),
                Route::Issues => "需要注意的问题".into(),
                Route::Handling(group) => view
                    .issues
                    .iter()
                    .find(|i| i.group == group)
                    .map_or("处理方法", issue_title)
                    .into(),
                other => other.title().into(),
            },
            badge: badge(&view, route),
        });
    }
    match route {
        Route::Home => home(&view, &mut blocks),
        Route::Detail(Resource::Memory) => memory_detail(&view, &mut blocks),
        Route::Detail(Resource::Cpu) => cpu_detail(&view, &mut blocks),
        Route::Detail(Resource::Disk) => disk_detail(&view, &mut blocks),
        Route::Issues => issues(&view, &mut blocks),
        Route::Handling(group) => handling(&view, group, &mut blocks),
        Route::Processes => processes(&view, &mut blocks),
        Route::Settings => settings(&view, &mut blocks),
    }
    Page {
        blocks,
        footer: Footer {
            pause: Button::new(
                if monitor.paused() {
                    "继续采集"
                } else {
                    "暂停采集"
                },
                Command::TogglePause,
            ),
            interval: Button::new(
                format!("每 {}采样", interval_label(monitor.interval())),
                Command::Go(Route::Settings),
            ),
        },
    }
}

fn badge(view: &View, route: Route) -> Option<(Tone, String)> {
    match route {
        Route::Detail(resource) => {
            let row = resource_row(view, resource);
            match row {
                Block::Resource { tone, status, .. } => Some((tone, status)),
                _ => None,
            }
        }
        Route::Handling(group) => view.issues.iter().find(|i| i.group == group).map(|i| {
            if matches!(i.recovery, Recovery::Confirming { .. }) {
                (Tone::Recovering, "正在确认恢复".to_string())
            } else {
                (Tone::of(i.severity), severity_word(i.severity).to_string())
            }
        }),
        _ => None,
    }
}

/// One plain sentence for the status item, the home title and accessibility.
pub fn conclusion(monitor: &Monitor, now: u64) -> (Tone, String) {
    let view = View {
        monitor,
        health: monitor.health(now),
        issues: monitor.issues(),
    };
    let (tone, title, _) = headline(&view);
    (tone, title)
}

fn sampled_at(view: &View) -> String {
    last_sample(view.monitor)
        .map_or_else(String::new, |s| format!("{} 采样", clock(s.unix_seconds)))
}

fn headline(view: &View) -> (Tone, String, String) {
    let monitor = view.monitor;
    match view.health {
        Health::Loading => (
            Tone::Loading,
            "正在读取系统数据…".into(),
            "首次采样通常只需片刻".into(),
        ),
        Health::Paused => {
            let mut subtitle = format!(
                "{} 暂停 · 数据停留在 {}",
                monitor.paused_at().map_or("—".into(), clock),
                last_sample(monitor).map_or("—".into(), |s| clock(s.unix_seconds))
            );
            if !view.issues.is_empty() {
                subtitle.push_str(&format!(" · 暂停前有 {} 项需要注意", view.issues.len()));
            }
            (Tone::Paused, "已暂停采集".into(), subtitle)
        }
        Health::Failed => (
            Tone::Unknown,
            "暂时读不到系统数据".into(),
            match last_sample(monitor) {
                Some(s) => format!(
                    "最后一次成功采样：{} · 状态未知，不代表正常",
                    clock(s.unix_seconds)
                ),
                None => "还没有成功采样 · 状态未知，不代表正常".into(),
            },
        ),
        Health::Stale => (
            Tone::Unknown,
            "数据已过时".into(),
            format!(
                "最后采样 {}，应每 {} 更新一次",
                last_sample(monitor).map_or("—".into(), |s| clock(s.unix_seconds)),
                interval_label(monitor.interval())
            ),
        ),
        Health::Issues(severity) => {
            let top = &view.issues[0];
            if view.issues.len() > 1 {
                (
                    Tone::of(severity),
                    format!("有 {} 项需要注意", view.issues.len()),
                    format!("最严重：{} · {}", issue_title(top), sampled_at(view)),
                )
            } else {
                (
                    Tone::of(severity),
                    issue_title(top).into(),
                    duration_line(view, top).unwrap_or_else(|| sampled_at(view)),
                )
            }
        }
        Health::Recovering => {
            let done = view.issues.iter().map(recovery_done).min().unwrap_or(0);
            (
                Tone::Recovering,
                "正在确认恢复".into(),
                format!(
                    "已有 {done}/{} 个正常样本 · {}",
                    monitor.policy().recovery_samples,
                    sampled_at(view)
                ),
            )
        }
        Health::Partial => (
            Tone::Unknown,
            "部分数据暂时读不到".into(),
            format!("其余检查目前无需处理 · {}", sampled_at(view)),
        ),
        Health::Warmup => (
            Tone::Loading,
            "正在准备".into(),
            "内存和磁盘已读取；处理器与交换速率需要再采样一次".into(),
        ),
        Health::Normal => (
            Tone::Ok,
            "目前无需处理".into(),
            format!("内存、处理器、磁盘空间未见持续异常 · {}", sampled_at(view)),
        ),
    }
}

fn issue_for<'a>(view: &'a View, group: Group) -> Option<&'a Issue> {
    view.issues.iter().find(|i| i.group == group)
}

fn issue_status(issue: &Issue) -> (Tone, &'static str) {
    if matches!(issue.recovery, Recovery::Confirming { .. }) {
        return (Tone::Recovering, "确认恢复中");
    }
    let tone = Tone::of(issue.severity);
    let has = |id| issue.rules.iter().any(|r| r.id == id);
    let text = match issue.group {
        Group::Memory if has("memory_pressure") && has("swap_activity") => "压力偏高 · 交换频繁",
        Group::Memory if has("swap_activity") => "交换频繁",
        Group::Memory if issue.severity >= Severity::Critical => "压力严重",
        Group::Memory => "压力偏高",
        Group::Cpu => "持续很高",
        Group::Disk if issue.severity >= Severity::Critical => "空间严重不足",
        Group::Disk => "空间不足",
    };
    (tone, text)
}

fn resource_row(view: &View, resource: Resource) -> Block {
    let monitor = view.monitor;
    let group = match resource {
        Resource::Memory => Group::Memory,
        Resource::Cpu => Group::Cpu,
        Resource::Disk => Group::Disk,
    };
    let shown = shown(view);
    let dim = shown.is_some_and(|(_, old)| old);
    let (mut tone, mut status): (Tone, String) = match view.health {
        Health::Loading => (Tone::Loading, "正在读取".into()),
        Health::Failed => (Tone::Unknown, "未知".into()),
        _ => (Tone::Unknown, "未知".into()),
    };
    let mut value = "—".to_string();
    let mut graphic = None;
    let name = match resource {
        Resource::Memory => "内存",
        Resource::Cpu => "处理器",
        Resource::Disk => "磁盘空间",
    };
    if let Some((latest, _)) = shown {
        let snapshot = &latest.snapshot;
        match resource {
            Resource::Memory => {
                if let Some(b) = &snapshot.memory.breakdown {
                    value = format!(
                        "{} / {} GiB",
                        gib(b.used_bytes),
                        gib(snapshot.memory.total_bytes)
                    );
                }
                (tone, status) = match snapshot.memory.pressure {
                    Pressure::Normal => (Tone::Ok, "压力正常".into()),
                    Pressure::Warning => (Tone::Warning, "压力偏高".into()),
                    Pressure::Critical => (Tone::Critical, "压力严重".into()),
                    Pressure::Unknown => (Tone::Unknown, "压力未知".into()),
                };
            }
            Resource::Cpu => {
                let activity = &snapshot.activity;
                (value, tone, status) = match (activity.status, activity.cpu_busy_percent) {
                    (RateStatus::Ready, Some(cpu)) if cpu >= 90.0 => {
                        (percent(cpu), Tone::Neutral, "很忙，观察中".into())
                    }
                    (RateStatus::Ready, Some(cpu)) => (percent(cpu), Tone::Ok, "正常".into()),
                    (RateStatus::Ready, None) | (RateStatus::Unavailable, _) => {
                        ("—".into(), Tone::Unknown, "未知".into())
                    }
                    _ => ("—".into(), Tone::Loading, "正在建立基线".into()),
                };
                if monitor.history().len() > 1 {
                    graphic = Some(Graphic::Spark(cpu_chart(monitor)));
                }
            }
            Resource::Disk => {
                if let Some(disk) = &snapshot.disk {
                    value = format!("{} GiB 可用", gib(disk.available_bytes));
                    let used = 1.0 - disk.available_bytes as f64 / disk.total_bytes as f64;
                    graphic = Some(Graphic::Bar {
                        fraction: used,
                        tone: Tone::Neutral,
                        label: format!("已用 {:.0}%", used * 100.0),
                    });
                    (tone, status) = (Tone::Ok, "充足".into());
                }
            }
        }
        if let Some(issue) = issue_for(view, group) {
            let (t, s) = issue_status(issue);
            (tone, status) = (t, s.into());
        }
        match view.health {
            Health::Paused => (tone, status) = (Tone::Paused, "已暂停".into()),
            Health::Stale => (tone, status) = (Tone::Unknown, "已过时".into()),
            _ => {}
        }
    } else if let Some(issue) = issue_for(view, group) {
        // A failed attempt keeps the issue: show it, not a reassuring "unknown" alone.
        let (t, s) = issue_status(issue);
        (tone, status) = (t, format!("{s}（此前）"));
    }
    if let Some(Graphic::Bar { tone: bar, .. }) = &mut graphic {
        *bar = if matches!(tone, Tone::Warning | Tone::Critical) {
            tone
        } else {
            Tone::Neutral
        };
    }
    let mut label = format!("{name}，{value}，{status}");
    if let Some(Graphic::Spark(chart)) = &graphic {
        label.push_str(&format!("。{}", chart.summary));
    }
    label.push_str("。查看详情");
    Block::Resource {
        resource,
        tone,
        name: name.into(),
        value,
        status,
        graphic,
        dim,
        label,
    }
}

fn issue_card(view: &View, issue: &Issue, more: usize) -> Block {
    let mut body = Vec::new();
    for rule in &issue.rules {
        body.push(evidence(view, rule));
    }
    if matches!(issue.recovery, Recovery::Confirming { .. }) {
        body.push(recovery_line(issue));
    }
    let mut buttons =
        vec![Button::new("查看处理方法", Command::Go(Route::Handling(issue.group))).primary()];
    if more > 0 {
        buttons.push(Button::new(
            format!("另有 {more} 项 ›"),
            Command::Go(Route::Issues),
        ));
    }
    let tone = if matches!(issue.recovery, Recovery::Confirming { .. }) {
        Tone::Recovering
    } else {
        Tone::of(issue.severity)
    };
    Block::Card {
        tone,
        title: issue_title(issue).into(),
        body,
        buttons,
    }
}

fn home(view: &View, blocks: &mut Vec<Block>) {
    let monitor = view.monitor;
    let (tone, title, subtitle) = headline(view);
    blocks.push(Block::Conclusion {
        tone,
        title,
        subtitle,
    });
    if let Some((at, groups)) = monitor.recovered() {
        if monitor.current().is_some() {
            let names: Vec<&str> = groups
                .iter()
                .map(|g| match g {
                    Group::Memory => "内存",
                    Group::Cpu => "处理器",
                    Group::Disk => "磁盘空间",
                })
                .collect();
            blocks.push(Block::Card {
                tone: Tone::Ok,
                title: format!("{}已恢复正常", names.join("、")),
                body: vec![format!(
                    "连续 {} 个正常样本（截至 {}）",
                    monitor.policy().recovery_samples,
                    clock(at)
                )],
                buttons: Vec::new(),
            });
        }
    }
    for resource in [Resource::Memory, Resource::Cpu, Resource::Disk] {
        blocks.push(resource_row(view, resource));
    }
    match view.health {
        Health::Paused | Health::Loading | Health::Normal | Health::Warmup => {}
        Health::Issues(_) | Health::Recovering => {
            blocks.push(issue_card(view, &view.issues[0], view.issues.len() - 1));
        }
        Health::Failed => {
            let (_, error, count) = monitor.failure().unwrap_or((0, "", 0));
            let mut body = vec![
                format!("读取系统数据时出错（连续 {count} 次）：{error}"),
                "在恢复之前，各项显示为“未知”，不代表正常。Hostwatch 会按间隔自动重试。".into(),
            ];
            let mut buttons = vec![Button::new("重新读取", Command::Retry).primary()];
            if let Some(top) = view.issues.first() {
                body.push(format!(
                    "此前的提示仍保留：{}（{}）",
                    issue_title(top),
                    severity_word(top.severity)
                ));
                buttons.push(Button::new(
                    "查看处理方法",
                    Command::Go(Route::Handling(top.group)),
                ));
            }
            blocks.push(Block::Card {
                tone: Tone::Unknown,
                title: "可以重新读取一次".into(),
                body,
                buttons,
            });
        }
        Health::Stale => blocks.push(Block::Card {
            tone: Tone::Unknown,
            title: "数据没有按时更新".into(),
            body: vec![
                "可能是电脑刚从睡眠中醒来，或采集暂时卡住。旧数据已变淡，不代表现在。".into(),
            ],
            buttons: vec![Button::new("重新读取", Command::Retry).primary()],
        }),
        Health::Partial => {
            let missing = monitor.current().map(unmeasured).unwrap_or_default();
            let failed: Vec<&str> = monitor
                .current()
                .map(|l| {
                    l.snapshot
                        .sources
                        .iter()
                        .filter(|s| !s.ok)
                        .map(|s| s.source)
                        .collect()
                })
                .unwrap_or_default();
            let what = if missing.is_empty() {
                "部分数据".to_string()
            } else {
                missing.join("、")
            };
            let mut body = vec![format!(
                "因此无法判断这一项，显示为“未知”，不代表正常。其他检查目前无需处理。"
            )];
            if !failed.is_empty() {
                body.push(format!("不可用的数据源：{}", failed.join("、")));
            }
            blocks.push(Block::Card {
                tone: Tone::Unknown,
                title: format!("{what}暂时读不到"),
                body,
                buttons: Vec::new(),
            });
        }
    }
}

fn freshness_note(view: &View, sources: &str) -> String {
    match shown(view) {
        Some((latest, old)) => {
            let suffix = match view.health {
                Health::Paused if old => "（已暂停，非当前）",
                Health::Stale if old => "（已过时，非当前）",
                _ => "",
            };
            format!(
                "样本时间 {}{suffix} · 来源：{sources}",
                clock(latest.snapshot.unix_seconds)
            )
        }
        None => format!("暂无当前样本 · 来源：{sources}"),
    }
}

fn tool_feedback(view: &View, tool: Tool, blocks: &mut Vec<Block>) {
    let Some(feedback) = view.monitor.tool_feedback().filter(|f| f.tool == tool) else {
        return;
    };
    blocks.push(match &feedback.result {
        Ok(()) => Block::Card {
            tone: Tone::Neutral,
            title: format!("已请求打开{}", tool.label()),
            body: vec![format!(
                "{} 已交给系统打开。如果没有出现，可以手动打开：{}。",
                clock(feedback.at),
                tool.manual_path()
            )],
            buttons: Vec::new(),
        },
        Err(error) => Block::Card {
            tone: Tone::Critical,
            title: format!("无法打开{}", tool.label()),
            body: vec![
                error.clone(),
                format!("可以手动打开：{}。", tool.manual_path()),
            ],
            buttons: vec![Button::new("重试", Command::Open(tool))],
        },
    });
}

fn related_issue(view: &View, group: Group, buttons: &mut Vec<Button>) {
    if issue_for(view, group).is_some() {
        buttons.insert(
            0,
            Button::new("查看处理方法", Command::Go(Route::Handling(group))).primary(),
        );
    }
}

fn memory_detail(view: &View, blocks: &mut Vec<Block>) {
    let monitor = view.monitor;
    let latest = shown(view).map(|(l, _)| l);
    let memory = latest.map(|l| &l.snapshot.memory);
    let breakdown = memory.and_then(|m| m.breakdown.as_ref());
    blocks.push(match (breakdown, memory) {
        (Some(b), Some(m)) => Block::Big {
            value: gib(b.used_bytes),
            unit: format!("/ {} GiB 已用", gib(m.total_bytes)),
        },
        _ => Block::Big {
            value: "—".into(),
            unit: "已用内存未知".into(),
        },
    });
    blocks.push(Block::Text(
        "“已用”= 应用内存 + 联动内存 + 压缩内存，不含可随时回收的文件缓存。已用多本身很正常；真正要看的是系统给出的“内存压力”和持续的交换读写。压力只有正常 / 偏高 / 严重三个等级，不是百分比。"
            .into(),
    ));
    let total = memory
        .map(|m| m.total_bytes)
        .or_else(|| monitor.latest().map(|l| l.snapshot.memory.total_bytes));
    if let Some(total) = total {
        blocks.push(Block::Graphic(Graphic::Chart(memory_chart(monitor, total))));
    }
    let mut swap = swap_chart(monitor);
    swap.unit = "MiB/s · ≥ 1 需要注意 · ≥ 16 严重".into();
    blocks.push(Block::Graphic(Graphic::Chart(swap)));
    let unknown = || "未知".to_string();
    let mut facts = vec![(
        "内存压力（系统等级）".to_string(),
        memory.map_or_else(unknown, |m| pressure_label(m.pressure).into()),
    )];
    match breakdown {
        Some(b) => facts.extend([
            ("应用内存".into(), format!("{} GiB", gib(b.app_bytes))),
            ("联动内存".into(), format!("{} GiB", gib(b.wired_bytes))),
            (
                "压缩内存".into(),
                format!("{} GiB", gib(b.compressed_bytes)),
            ),
            (
                "文件缓存（不计入已用）".into(),
                format!("{} GiB", gib(b.cached_files_bytes)),
            ),
            ("空闲".into(), format!("{} GiB", gib(b.free_bytes))),
        ]),
        None => facts.push(("内存分项".into(), unknown())),
    }
    let activity = latest.map(|l| &l.snapshot.activity);
    facts.push((
        "交换读取 / 写入".into(),
        match activity {
            Some(a) if a.status == RateStatus::Ready => {
                match (a.swap_in_bytes_per_second, a.swap_out_bytes_per_second) {
                    (Some(i), Some(o)) => format!("{:.1} / {}", i / MIB, rate(o)),
                    _ => unknown(),
                }
            }
            Some(_) => "正在建立基线".into(),
            None => unknown(),
        },
    ));
    facts.push((
        "交换文件已用".into(),
        memory
            .and_then(|m| m.swap.as_ref())
            .map_or_else(unknown, |s| format!("{} GiB", gib(s.used_bytes))),
    ));
    blocks.push(Block::Facts(facts));
    let mut buttons = vec![Button::new(
        "打开活动监视器",
        Command::Open(Tool::ActivityMonitor),
    )];
    related_issue(view, Group::Memory, &mut buttons);
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, Tool::ActivityMonitor, blocks);
    blocks.push(Block::Note(freshness_note(
        view,
        "系统内存统计、交换计数器、内核内存压力等级",
    )));
}

fn cpu_detail(view: &View, blocks: &mut Vec<Block>) {
    let monitor = view.monitor;
    let activity = shown(view).map(|(l, _)| &l.snapshot.activity);
    blocks.push(match activity {
        Some(a) if a.status == RateStatus::Ready && a.cpu_busy_percent.is_some() => Block::Big {
            value: format!("{:.0}", a.cpu_busy_percent.unwrap_or(0.0)),
            unit: "% 使用率".into(),
        },
        Some(a) if a.status != RateStatus::Ready && a.status != RateStatus::Unavailable => {
            Block::Big {
                value: "—".into(),
                unit: "正在建立基线，需要再采样一次".into(),
            }
        }
        _ => Block::Big {
            value: "—".into(),
            unit: "使用率未知".into(),
        },
    });
    blocks.push(Block::Text(
        "处理器使用率是两次采样之间所有核心平均有多忙。短时间升高很正常，连续 2 个样本 ≥ 90% 才会提示。Hostwatch 不提供按进程的处理器排行。"
            .into(),
    ));
    let mut chart = cpu_chart(monitor);
    chart.unit = "% · 虚线 90 为提示线".into();
    let peak = chart
        .segments
        .iter()
        .flatten()
        .map(|p| p.1)
        .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))));
    blocks.push(Block::Graphic(Graphic::Chart(chart)));
    blocks.push(Block::Facts(vec![
        ("图表范围内最高".into(), peak.map_or("—".into(), percent)),
        ("提示条件".into(), "≥ 90%，连续 2 个样本".into()),
        ("按进程查看".into(), "请用活动监视器的“CPU”页".into()),
    ]));
    let mut buttons = vec![Button::new(
        "打开活动监视器",
        Command::Open(Tool::ActivityMonitor),
    )];
    related_issue(view, Group::Cpu, &mut buttons);
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, Tool::ActivityMonitor, blocks);
    blocks.push(Block::Note(freshness_note(
        view,
        "系统处理器负载计数（两次采样之差）",
    )));
}

fn disk_detail(view: &View, blocks: &mut Vec<Block>) {
    let disk = shown(view).and_then(|(l, _)| l.snapshot.disk.as_ref());
    match disk {
        Some(d) => {
            let used = d.total_bytes - d.available_bytes;
            let fraction = used as f64 / d.total_bytes as f64;
            blocks.push(Block::Big {
                value: gib(d.available_bytes),
                unit: "GiB 可用".into(),
            });
            blocks.push(Block::Text(
                "这是启动磁盘（Data 卷）还剩多少空间。空间太少时，系统更新、保存文件和交换都会受影响。这里只显示容量，不显示读写速度或磁盘健康；同一容器中的卷共享空间。"
                    .into(),
            ));
            let tone = match issue_for(view, Group::Disk) {
                Some(issue) => Tone::of(issue.severity),
                None => Tone::Neutral,
            };
            blocks.push(Block::Graphic(Graphic::Bar {
                fraction,
                tone,
                label: format!(
                    "已用 {:.0}%，可用 {:.1}%",
                    fraction * 100.0,
                    100.0 - fraction * 100.0
                ),
            }));
            blocks.push(Block::Facts(vec![
                ("可用".into(), format!("{} GiB", gib(d.available_bytes))),
                ("已用".into(), format!("{} GiB", gib(used))),
                ("总容量".into(), format!("{} GiB", gib(d.total_bytes))),
                ("卷".into(), d.path.clone()),
                ("提示条件".into(), rule_rule("disk_space").into()),
            ]));
        }
        None => {
            blocks.push(Block::Big {
                value: "—".into(),
                unit: "可用空间未知".into(),
            });
            blocks.push(Block::Text(
                "目前读不到启动磁盘的容量，显示为“未知”，不代表空间充足。".into(),
            ));
        }
    }
    blocks.push(Block::Note(
        "容量以 GiB 计（1 GiB ≈ 1.07 GB），“储存空间”设置以 GB 显示。只显示当前值，不保留容量历史。"
            .into(),
    ));
    let mut buttons = vec![Button::new(
        "打开“储存空间”设置",
        Command::Open(Tool::StorageSettings),
    )];
    related_issue(view, Group::Disk, &mut buttons);
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, Tool::StorageSettings, blocks);
    blocks.push(Block::Note(freshness_note(
        view,
        "文件系统容量统计（statfs）",
    )));
}

fn issues(view: &View, blocks: &mut Vec<Block>) {
    if view.issues.is_empty() {
        blocks.push(Block::Text("目前没有需要处理的问题。".into()));
        return;
    }
    let rules: usize = view.issues.iter().map(|i| i.rules.len()).sum();
    blocks.push(Block::Text(format!(
        "共 {} 项问题（涉及 {rules} 条检查规则），按严重程度排列。",
        view.issues.len()
    )));
    for issue in &view.issues {
        let mut body = Vec::new();
        if issue.rules.len() > 1 {
            let names: Vec<&str> = issue.rules.iter().map(|r| rule_name(r.id)).collect();
            body.push(format!("合并提示，包含：{}", names.join("、")));
        }
        for rule in &issue.rules {
            body.push(evidence(view, rule));
        }
        body.push(recovery_line(issue));
        let (tone, _) = issue_status(issue);
        blocks.push(Block::Card {
            tone,
            title: format!("{} · {}", issue_title(issue), severity_word(issue.severity)),
            body,
            buttons: vec![Button::new(
                "查看处理方法",
                Command::Go(Route::Handling(issue.group)),
            )],
        });
    }
}

fn handling(view: &View, group: Group, blocks: &mut Vec<Block>) {
    let Some(issue) = issue_for(view, group) else {
        let recovered = view
            .monitor
            .recovered()
            .is_some_and(|(_, groups)| groups.contains(&group));
        blocks.push(Block::Card {
            tone: Tone::Ok,
            title: if recovered {
                "已恢复正常".into()
            } else {
                "这项提示已解除".into()
            },
            body: vec![format!(
                "Hostwatch 已观察到连续 {} 个正常样本。",
                view.monitor.policy().recovery_samples
            )],
            buttons: vec![Button::new("返回", Command::Back)],
        });
        return;
    };
    if issue.rules.len() > 1 {
        let names: Vec<String> = issue
            .rules
            .iter()
            .map(|r| {
                format!(
                    "{}（{}）",
                    rule_name(r.id),
                    severity_word(r.status.active.unwrap_or(Severity::Warning))
                )
            })
            .collect();
        blocks.push(Block::Note(format!("合并提示，包含：{}", names.join("、"))));
    }
    blocks.push(Block::Heading("为什么提示".into()));
    blocks.push(Block::Text(issue_reason(issue).into()));
    let mut bullets: Vec<String> = issue.rules.iter().map(|r| evidence(view, r)).collect();
    if let Some(line) = duration_line(view, issue) {
        bullets.push(line);
    }
    blocks.push(Block::Bullets(bullets));
    blocks.push(Block::Heading("你可以这样做".into()));
    blocks.push(Block::Steps(
        issue_steps(group).iter().map(|s| s.to_string()).collect(),
    ));
    let tool = group.tool();
    let mut buttons =
        vec![Button::new(format!("打开{}", tool.label()), Command::Open(tool)).primary()];
    let detail = match group {
        Group::Memory => ("查看内存详情", Resource::Memory),
        Group::Cpu => ("查看处理器详情", Resource::Cpu),
        Group::Disk => ("查看磁盘详情", Resource::Disk),
    };
    buttons.push(Button::new(detail.0, Command::Go(Route::Detail(detail.1))));
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, tool, blocks);
    blocks.push(Block::Heading("之后会怎样".into()));
    let required = view.monitor.policy().recovery_samples;
    blocks.push(Block::Text(format!(
        "Hostwatch 会一直按间隔自动采样，不需要点任何按钮；打开工具也不会改变判断。连续 {required} 个正常样本后才确认恢复，读不到的样本不计入。"
    )));
    blocks.push(Block::Progress {
        done: recovery_done(issue),
        total: required,
        label: recovery_line(issue),
    });
    let rules: Vec<String> = issue
        .rules
        .iter()
        .map(|r| format!("{}：{}", rule_name(r.id), rule_rule(r.id)))
        .collect();
    blocks.push(Block::Note(format!(
        "提示条件 — {}。Hostwatch 不会自动清理、删除或结束任何内容，也无法预估能释放多少内存或空间。",
        rules.join("；")
    )));
}

fn processes(view: &View, blocks: &mut Vec<Block>) {
    blocks.push(Block::Text(
        "按常驻内存（RSS）列出可读取的前 5 个进程，只在你点“读取”时扫描。占用大不代表它让电脑变慢或有问题。"
            .into(),
    ));
    let (loading, result) = match view.monitor.processes() {
        Processes::NotRead => (false, None),
        Processes::Loading { previous, .. } => (true, previous.as_ref()),
        Processes::Done(result) => (false, Some(result)),
    };
    let mut button = Button::new(
        if loading {
            "读取中…"
        } else if result.is_some() {
            "刷新"
        } else {
            "读取"
        },
        Command::RefreshProcesses,
    )
    .primary();
    button.enabled = !loading;
    blocks.push(Block::Buttons(vec![button]));
    match (loading, result) {
        (true, _) => blocks.push(Block::Note("正在读取进程信息…".into())),
        (false, None) => blocks.push(Block::Note("尚未读取。".into())),
        _ => {}
    }
    if let Some(result) = result {
        let prefix = if loading { "上次" } else { "" };
        match &result.outcome {
            Ok(scan) => {
                blocks.push(Block::Note(format!("{prefix}读取于 {}", clock(result.at))));
                let largest = scan.top.first().map_or(1, |p| p.resident_bytes.max(1));
                if scan.top.is_empty() {
                    blocks.push(Block::Text("没有读到任何进程的内存数据。".into()));
                } else {
                    blocks.push(Block::Processes(
                        scan.top
                            .iter()
                            .enumerate()
                            .map(|(i, p)| ProcessRow {
                                rank: i + 1,
                                name: display_name(&p.name),
                                size: size(p.resident_bytes),
                                fraction: p.resident_bytes as f64 / largest as f64,
                            })
                            .collect(),
                    ));
                }
                if scan.unreadable > 0 {
                    blocks.push(Block::Note(format!(
                        "另有 {} 个进程（共列出 {} 个）读不到内存数据，通常属于系统或其他用户，未列入排行。",
                        scan.unreadable, scan.listed
                    )));
                }
            }
            Err(error) => blocks.push(Block::Card {
                tone: Tone::Critical,
                title: format!("{prefix}读取失败"),
                body: vec![error.clone(), "可以稍后重试，或打开活动监视器查看。".into()],
                buttons: Vec::new(),
            }),
        }
    }
    blocks.push(Block::Note(
        "只显示单个进程，不合并为应用；RSS 不等于活动监视器的“内存”列，多个进程的数值不能直接相加。"
            .into(),
    ));
    blocks.push(Block::Buttons(vec![Button::new(
        "打开活动监视器",
        Command::Open(Tool::ActivityMonitor),
    )]));
    tool_feedback(view, Tool::ActivityMonitor, blocks);
}

fn settings(view: &View, blocks: &mut Vec<Block>) {
    let monitor = view.monitor;
    blocks.push(Block::Heading("采样间隔".into()));
    let mut presets = INTERVAL_PRESETS.to_vec();
    let custom = !presets.contains(&monitor.interval());
    if custom {
        presets.push(monitor.interval());
        presets.sort_unstable();
    }
    blocks.push(Block::Choices(
        presets
            .into_iter()
            .map(|seconds| {
                let mut title = interval_label(seconds);
                if custom && seconds == monitor.interval() {
                    title.push_str("（启动参数）");
                }
                let mut button = Button::new(title, Command::SetInterval(seconds));
                button.primary = seconds == monitor.interval();
                button
            })
            .collect(),
    ));
    blocks.push(Block::Note(if monitor.paused() {
        "暂停中：继续采集后按所选间隔采样。".into()
    } else {
        "间隔越短越及时，开销也略高。修改后下一个样本按新间隔采集。".into()
    }));
    blocks.push(Block::Heading("采集".into()));
    blocks.push(Block::Buttons(vec![Button::new(
        if monitor.paused() {
            "继续采集"
        } else {
            "暂停采集"
        },
        Command::TogglePause,
    )]));
    blocks.push(Block::Heading("数据说明".into()));
    blocks.push(Block::Facts(vec![
        (
            "内存".into(),
            "“已用”= 应用 + 联动 + 压缩内存，不含文件缓存。“内存压力”是系统给出的等级，比已用量更能反映是否不够用。"
                .into(),
        ),
        (
            "交换读写".into(),
            "内存不够时系统把数据写到磁盘再读回。≥ 1 MiB/s 需要注意，≥ 16 MiB/s 严重；只有交换文件而没有读写不需要处理。"
                .into(),
        ),
        (
            "处理器".into(),
            "两次采样之间所有核心的平均使用率；刚启动或刚继续采集时需要两个样本才能算出。".into(),
        ),
        (
            "磁盘空间".into(),
            "启动磁盘 Data 卷的可用容量，不包括读写速度、温度或磁盘健康。".into(),
        ),
        (
            "进程排行".into(),
            "按需读取单个进程的常驻内存（RSS）前 5 名；部分进程可能因权限读不到。".into(),
        ),
        (
            "提示规则".into(),
            "需要注意要连续 2 个异常样本，严重立即提示；连续 2 个正常样本才确认恢复；读不到的样本既不算异常也不算恢复。阈值是本工具的经验值，不是 Apple 官方标准。"
                .into(),
        ),
    ]));
    match monitor.current() {
        Some(latest) => {
            let sources = &latest.snapshot.sources;
            let failed: Vec<String> = sources
                .iter()
                .filter(|s| !s.ok)
                .map(|s| {
                    format!(
                        "{}（{}）",
                        s.source,
                        s.error.as_deref().unwrap_or("未知错误")
                    )
                })
                .collect();
            let mut line = format!(
                "最新样本 {}：{}/{} 个数据源可用",
                clock(latest.snapshot.unix_seconds),
                sources.len() - failed.len(),
                sources.len()
            );
            if !failed.is_empty() {
                line.push_str(&format!("；不可用：{}", failed.join("、")));
            }
            blocks.push(Block::Note(line));
        }
        None => blocks.push(Block::Note(match monitor.failure() {
            Some((at, error, _)) => format!("{} 采集失败：{error}", clock(at)),
            None => "尚无样本。".into(),
        })),
    }
    blocks.push(Block::Note(
        "Hostwatch 只读取系统统计：不清理、不结束进程、不上传任何数据。".into(),
    ));
    blocks.push(Block::Buttons(vec![Button::new(
        "退出 Hostwatch",
        Command::Quit,
    )]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::advice::tests::{ready, snapshot};
    use crate::model::{Process, ProcessScan, SourceStatus};
    use crate::monitor::tests::{at, normal};

    fn texts(page: &Page) -> Vec<String> {
        let mut out = Vec::new();
        let buttons =
            |out: &mut Vec<String>, b: &[Button]| out.extend(b.iter().map(|b| b.title.clone()));
        for block in &page.blocks {
            match block {
                Block::Conclusion {
                    title, subtitle, ..
                } => out.extend([title.clone(), subtitle.clone()]),
                Block::Header { back, title, badge } => {
                    out.extend([back.clone(), title.clone()]);
                    out.extend(badge.iter().map(|b| b.1.clone()));
                }
                Block::Resource {
                    value,
                    status,
                    label,
                    ..
                } => out.extend([value.clone(), status.clone(), label.clone()]),
                Block::Card {
                    title,
                    body,
                    buttons: b,
                    ..
                } => {
                    out.push(title.clone());
                    out.extend(body.iter().cloned());
                    buttons(&mut out, b);
                }
                Block::Heading(t) | Block::Text(t) | Block::Note(t) => out.push(t.clone()),
                Block::Big { value, unit } => out.push(format!("{value} {unit}")),
                Block::Graphic(Graphic::Chart(c) | Graphic::Spark(c)) => {
                    out.extend([c.summary.clone(), c.note.clone()]);
                    out.extend(c.empty.iter().cloned());
                }
                Block::Graphic(Graphic::Bar { label, .. }) => out.push(label.clone()),
                Block::Facts(f) => out.extend(f.iter().map(|(k, v)| format!("{k}：{v}"))),
                Block::Steps(s) | Block::Bullets(s) => out.extend(s.iter().cloned()),
                Block::Buttons(b) | Block::Choices(b) => buttons(&mut out, b),
                Block::Progress { label, .. } => out.push(label.clone()),
                Block::Processes(rows) => out.extend(
                    rows.iter()
                        .map(|r| format!("{} {} {}", r.rank, r.name, r.size)),
                ),
            }
        }
        out
    }

    fn has(page: &Page, needle: &str) -> bool {
        texts(page).iter().any(|t| t.contains(needle))
    }

    fn commands(page: &Page) -> Vec<Command> {
        let mut out = vec![
            page.footer.pause.command.clone(),
            page.footer.interval.command.clone(),
        ];
        for block in &page.blocks {
            match block {
                Block::Card { buttons, .. } | Block::Buttons(buttons) | Block::Choices(buttons) => {
                    out.extend(buttons.iter().map(|b| b.command.clone()))
                }
                _ => {}
            }
        }
        out
    }

    fn resource(page: &Page, which: Resource) -> (&str, &str, Tone, bool) {
        page.blocks
            .iter()
            .find_map(|b| match b {
                Block::Resource {
                    resource,
                    value,
                    status,
                    tone,
                    dim,
                    ..
                } if *resource == which => Some((value.as_str(), status.as_str(), *tone, *dim)),
                _ => None,
            })
            .unwrap()
    }

    fn home(monitor: &Monitor, now: u64) -> Page {
        page(monitor, Route::Home, None, now)
    }

    #[test]
    fn quiet_home_when_nothing_needs_attention() {
        let mut monitor = Monitor::new(30);
        monitor.record(Ok(normal(0)), 0);
        monitor.record(Ok(normal(30)), 30);
        let page = home(&monitor, 31);
        assert!(
            matches!(&page.blocks[0], Block::Conclusion { tone: Tone::Ok, title, .. } if title == "目前无需处理")
        );
        assert_eq!(page.blocks.len(), 4, "conclusion + three rows, no card");
        assert_eq!(resource(&page, Resource::Cpu).0, "20%");
        assert_eq!(page.footer.interval.title, "每 30 秒采样");
    }

    #[test]
    fn chart_endpoint_and_summary_match_displayed_value() {
        let mut monitor = Monitor::new(5);
        for (t, cpu) in [(0, 10.0), (5, 37.4), (10, 18.2)] {
            let mut s = normal(t);
            s.activity = ready(0.0, cpu);
            monitor.record(Ok(s), t);
        }
        let page = home(&monitor, 10);
        let (value, ..) = resource(&page, Resource::Cpu);
        let chart = cpu_chart(&monitor);
        let last = *chart.segments.last().unwrap().last().unwrap();
        assert_eq!((last.0, percent(last.1)), (10, value.to_string()));
        assert!(chart.summary.contains(&format!("最近 {value}")));
        assert!(chart.summary.contains("最高 37%"));
        let detail = page_for(&monitor, Route::Detail(Resource::Cpu));
        assert!(has(&detail, "18 % 使用率"));
    }

    fn page_for(monitor: &Monitor, route: Route) -> Page {
        page(
            monitor,
            route,
            Some(Route::Home),
            monitor
                .history()
                .samples()
                .last()
                .map_or(0, |s| s.unix_seconds),
        )
    }

    #[test]
    fn history_uses_timestamps_across_interval_changes_and_gaps() {
        let mut monitor = Monitor::new(30);
        for t in [0, 30, 60] {
            monitor.record(Ok(normal(t)), t);
        }
        monitor.set_interval(5, 61);
        for t in [65, 70] {
            monitor.record(Ok(normal(t)), t);
        }
        let chart = cpu_chart(&monitor);
        let times: Vec<u64> = chart.segments.iter().flatten().map(|p| p.0).collect();
        assert_eq!(
            times,
            [0, 30, 60, 65, 70],
            "old samples keep their own timestamps"
        );
        assert!(chart.gaps.is_empty(), "a cadence change is not a gap");
        assert_eq!((chart.t0, chart.t1), (0, 70));
        assert!(chart.note.contains("已有 5 个样本"));

        // Missing samples without recorded evidence: a gap labelled as no data, never sleep.
        monitor.record(Ok(normal(200)), 200);
        // A recorded failure, a pause and measured sleep each explain their own gap.
        monitor.record(Err("x".into()), 205);
        monitor.record(Ok(normal(210)), 210);
        monitor.set_paused(true, 211);
        monitor.set_paused(false, 300);
        monitor.record(Ok(normal(300)), 300);
        let mut woke = normal(5000);
        woke.activity = crate::activity::empty(RateStatus::SleepGap);
        monitor.record(Ok(woke), 5000);
        let labels: Vec<&str> = cpu_chart(&monitor).gaps.iter().map(|g| g.label).collect();
        assert_eq!(labels, ["无数据", "采集失败", "暂停", "睡眠"]);
        // The sleep sample has no CPU rate: absent, not zero, and the line is not joined across.
        let chart = cpu_chart(&monitor);
        assert!(chart.segments.iter().flatten().all(|p| p.0 != 5000));
        assert!(chart
            .segments
            .iter()
            .all(|s| s.windows(2).all(|w| w[1].0 - w[0].0 <= 30)));
    }

    #[test]
    fn missing_values_are_absent_not_zero() {
        let mut monitor = Monitor::new(30);
        let mut first = normal(0);
        first.activity = crate::activity::empty(RateStatus::Warmup);
        monitor.record(Ok(first), 0);
        let page = home(&monitor, 0);
        let (value, status, tone, _) = resource(&page, Resource::Cpu);
        assert_eq!((value, status, tone), ("—", "正在建立基线", Tone::Loading));
        let swap = swap_chart(&monitor);
        assert!(swap.segments.is_empty());
        assert!(swap.empty.unwrap().contains("至少 2 个样本"));
        let mut s = normal(30);
        s.memory.swap = None;
        s.activity.swap_in_bytes_per_second = None;
        monitor.record(Ok(s), 30);
        let detail = page_for(&monitor, Route::Detail(Resource::Memory));
        assert!(has(&detail, "交换文件已用：未知"));
        assert!(has(&detail, "交换读取 / 写入：未知"));
    }

    #[test]
    fn swap_axis_fits_real_values_beyond_demo_range() {
        let mut monitor = Monitor::new(5);
        for t in [0, 5] {
            let mut s = normal(t);
            s.activity = ready(80.0 * MIB, 10.0);
            monitor.record(Ok(s), t);
        }
        let chart = swap_chart(&monitor);
        assert!(chart.y_max >= 80.0);
        assert!(chart.thresholds.iter().any(|t| t.0 == 16.0));
        let mut quiet = Monitor::new(5);
        quiet.record(Ok(normal(0)), 0);
        assert_eq!(swap_chart(&quiet).y_max, 2.0);
        assert_eq!(swap_chart(&quiet).thresholds.len(), 1);
    }

    #[test]
    fn grouped_issue_counts_and_recovery_stay_engine_owned() {
        let mut monitor = Monitor::new(30);
        for t in [0, 30] {
            let mut s = at(snapshot(Pressure::Warning, 4 * GIB), t);
            s.activity = ready(20.0 * MIB, 95.0);
            monitor.record(Ok(s), t);
        }
        let page = home(&monitor, 30);
        assert!(has(&page, "有 3 项需要注意"));
        assert!(has(&page, "另有 2 项 ›"));
        let list = page_for(&monitor, Route::Issues);
        assert!(has(&list, "共 3 项问题（涉及 4 条检查规则）"));
        assert!(has(&list, "合并提示，包含：内存压力、交换读写"));

        let handling = page_for(&monitor, Route::Handling(Group::Memory));
        assert!(has(&handling, "内存不足，电脑可能变慢"));
        assert!(has(&handling, "交换读写合计 20.0 MiB/s（当前"));
        assert!(commands(&handling).contains(&Command::Open(Tool::ActivityMonitor)));
        // Opening the tool does not change the recovery shown.
        let before = handling
            .blocks
            .iter()
            .find(|b| matches!(b, Block::Progress { .. }))
            .cloned();
        monitor.note_open(Tool::ActivityMonitor, Ok(()), 31);
        let after = page_for(&monitor, Route::Handling(Group::Memory));
        assert_eq!(
            before,
            after
                .blocks
                .iter()
                .find(|b| matches!(b, Block::Progress { .. }))
                .cloned()
        );
        assert!(has(&after, "已请求打开活动监视器"));
        assert!(!has(&after, "已恢复"));

        monitor.record(Ok(at(snapshot(Pressure::Normal, 300 * GIB), 60)), 60);
        let handling = page_for(&monitor, Route::Handling(Group::Memory));
        assert!(has(&handling, "正在确认恢复：1/2 个正常样本"));
        // Retained evidence is labelled with its trigger time and not shown as current.
        assert!(has(
            &handling,
            &format!(
                "交换读写：{} 时 交换读写合计 20.0 MiB/s；当前已回到正常范围",
                clock(30)
            )
        ));
        monitor.record(Ok(at(snapshot(Pressure::Normal, 300 * GIB), 90)), 90);
        let page = home(&monitor, 90);
        assert!(has(&page, "内存、处理器、磁盘空间已恢复正常"));
        assert!(has(
            &page_for(&monitor, Route::Handling(Group::Memory)),
            "已恢复正常"
        ));
    }

    #[test]
    fn failure_and_stale_are_not_health() {
        let mut monitor = Monitor::new(30);
        monitor.record(Ok(at(snapshot(Pressure::Critical, 300 * GIB), 0)), 0);
        monitor.record(Err("无法读取物理内存总量".into()), 30);
        let page = home(&monitor, 30);
        assert!(has(&page, "暂时读不到系统数据"));
        assert_eq!(resource(&page, Resource::Memory).0, "—");
        assert_eq!(resource(&page, Resource::Memory).1, "压力严重（此前）");
        assert!(has(&page, "此前的提示仍保留：内存严重不足"));
        assert!(commands(&page).contains(&Command::Retry));
        let handling = page_for(&monitor, Route::Handling(Group::Memory));
        assert!(has(&handling, "本次未能读取，当前状态未知"));

        let mut monitor = Monitor::new(30);
        monitor.record(Ok(normal(0)), 0);
        let page = home(&monitor, 200);
        assert!(has(&page, "数据已过时"));
        let (value, status, _, dim) = resource(&page, Resource::Disk);
        assert!(value != "—" && status == "已过时" && dim);
    }

    #[test]
    fn partial_availability_names_the_unknown_item() {
        let mut monitor = Monitor::new(30);
        let mut s = snapshot(Pressure::Unknown, 300 * GIB);
        s.sources.push(SourceStatus {
            source: "kern.memorystatus_vm_pressure_level",
            ok: false,
            elapsed_micros: 3,
            error: Some("denied".into()),
        });
        monitor.record(Ok(s), 0);
        let page = home(&monitor, 0);
        assert!(has(&page, "部分数据暂时读不到"));
        assert!(has(&page, "内存压力暂时读不到"));
        assert_eq!(resource(&page, Resource::Memory).1, "压力未知");
    }

    #[test]
    fn paused_home_and_settings_keep_interval_consistent() {
        let mut monitor = Monitor::new(7);
        monitor.record(Ok(normal(0)), 0);
        monitor.set_paused(true, 3);
        let page = home(&monitor, 500);
        assert!(has(&page, "已暂停采集"));
        assert_eq!(resource(&page, Resource::Disk).1, "已暂停");
        assert_eq!(page.footer.pause.title, "继续采集");
        assert_eq!(page.footer.interval.title, "每 7 秒采样");
        let settings = page_for(&monitor, Route::Settings);
        let choices: Vec<(String, bool)> = settings
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Choices(c) => Some(c.iter().map(|b| (b.title.clone(), b.primary)).collect()),
                _ => None,
            })
            .unwrap();
        assert_eq!(choices.len(), 6);
        assert!(choices.contains(&("7 秒（启动参数）".into(), true)));
        assert!(choices.contains(&("5 分钟".into(), false)));
        assert!(has(&settings, "暂停中：继续采集后按所选间隔采样"));
        monitor.set_interval(60, 501);
        let page = home(&monitor, 502);
        assert_eq!(page.footer.interval.title, "每 1 分钟采样");
    }

    #[test]
    fn process_page_lifecycle() {
        let mut monitor = Monitor::new(30);
        let page = page_for(&monitor, Route::Processes);
        assert!(has(&page, "尚未读取"));
        assert!(has(&page, "读取"));
        monitor.begin_processes(10);
        let page = page_for(&monitor, Route::Processes);
        assert!(has(&page, "读取中…"));
        let enabled = page.blocks.iter().find_map(|b| match b {
            Block::Buttons(b) if b[0].command == Command::RefreshProcesses => Some(b[0].enabled),
            _ => None,
        });
        assert_eq!(enabled, Some(false));
        monitor.finish_processes(
            Ok(ProcessScan {
                top: vec![Process {
                    pid: 9,
                    name: "evil\nname".into(),
                    resident_bytes: 3 * GIB,
                }],
                listed: 400,
                unreadable: 120,
            }),
            12,
        );
        let page = page_for(&monitor, Route::Processes);
        assert!(has(&page, "1 evilname 3.0 GiB"));
        assert!(has(&page, "另有 120 个进程（共列出 400 个）读不到内存数据"));
        assert!(has(&page, "刷新"));
        monitor.finish_processes(
            Ok(ProcessScan {
                top: vec![],
                listed: 3,
                unreadable: 0,
            }),
            20,
        );
        let page = page_for(&monitor, Route::Processes);
        assert!(has(&page, "没有读到任何进程的内存数据"));
        assert!(
            !has(&page, "读不到内存数据，通常"),
            "fewer results do not imply permission limits"
        );
        monitor.finish_processes(Err("系统没有返回任何进程信息".into()), 30);
        assert!(has(&page_for(&monitor, Route::Processes), "读取失败"));
    }

    #[test]
    fn tool_failure_offers_manual_path_and_retry() {
        let mut monitor = Monitor::new(30);
        monitor.record(Ok(at(snapshot(Pressure::Normal, 4 * GIB), 0)), 0);
        monitor.note_open(Tool::StorageSettings, Err("系统未能打开 x".into()), 5);
        let page = page_for(&monitor, Route::Handling(Group::Disk));
        assert!(has(&page, "无法打开“储存空间”设置"));
        assert!(has(&page, "苹果菜单 › 系统设置 › 通用 › 储存空间"));
        assert!(
            commands(&page)
                .iter()
                .filter(|c| **c == Command::Open(Tool::StorageSettings))
                .count()
                >= 2
        );
    }

    #[test]
    fn memory_copy_matches_collector_definition() {
        let mut monitor = Monitor::new(30);
        let mut s = normal(0);
        s.memory.breakdown = Some(crate::model::MemoryBreakdown {
            used_bytes: 10 * GIB,
            app_bytes: 6 * GIB,
            wired_bytes: 2 * GIB,
            compressed_bytes: 2 * GIB,
            cached_files_bytes: 5 * GIB,
            free_bytes: GIB,
        });
        monitor.record(Ok(s), 0);
        let detail = page_for(&monitor, Route::Detail(Resource::Memory));
        assert!(has(&detail, "10.0 / 16.0 GiB 已用"));
        assert!(has(&detail, "不含可随时回收的文件缓存"));
        assert!(has(&detail, "文件缓存（不计入已用）：5.0 GiB"));
        let chart = memory_chart(&monitor, 16 * GIB);
        assert_eq!(chart.segments[0][0].1, 10.0);
    }

    #[test]
    fn clock_and_span_formatting() {
        assert_eq!(clock(0).len(), 8);
        assert_eq!(span(45), "45 秒");
        assert_eq!(span(600), "约 10 分钟");
        assert_eq!(span(3900), "约 1 小时 5 分钟");
        assert_eq!(interval_label(300), "5 分钟");
        assert_eq!(interval_label(90), "90 秒");
    }
}
