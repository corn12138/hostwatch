//! Pages of the menu-bar popover as plain data. `menubar` turns `Block`s into AppKit views and
//! sends `Command`s back; all copy, numbers and chart geometry come from here so text, charts
//! and accessibility summaries share one source.
//!
//! Layout: `format` (units and times), `chart` (time series and gaps), `reading` (shared
//! plain-language readings of rules and issues), `summary` (status line, resource rows and the
//! issue card shared by several pages) and `pages` (one module per page).

mod chart;
mod format;
mod pages;
mod reading;
mod summary;
#[cfg(test)]
mod tests;

use crate::advice::{GB, GIB, MIB};
use crate::alerts::Sample;
pub use crate::lang::{Lang, LanguageChoice, Ui};
use crate::model::{Pressure, RateStatus, Severity, ThermalState};
use crate::monitor::{
    unmeasured, ActiveRule, Current, Group, Health, Issue, Latest, Mark, Monitor, Processes,
    Recovery, Tool, INTERVAL_PRESETS,
};
use crate::optimize::{Focus, Optimization, QuitState};

pub use chart::{cpu_chart, memory_chart, swap_chart, temperature_chart};
use format::*;
pub use format::{clock, interval_label, percent, rate, span};
pub(crate) use format::{tool_label, tool_path};
use reading::*;
use summary::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Memory,
    Cpu,
    Disk,
    Thermal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Home,
    Detail(Resource),
    Issues,
    Handling(Group),
    Processes,
    Settings,
    /// Ask the apps behind an issue to quit normally.
    Optimize(Group),
}

impl Route {
    pub fn title(self, lang: Lang) -> &'static str {
        match self {
            Route::Home => tr!(lang, "概览", "Overview"),
            Route::Detail(Resource::Memory) => tr!(lang, "内存", "Memory"),
            Route::Detail(Resource::Cpu) => tr!(lang, "处理器", "Processor"),
            Route::Detail(Resource::Disk) => tr!(lang, "磁盘空间", "Disk space"),
            Route::Detail(Resource::Thermal) => tr!(lang, "温度", "Temperature"),
            Route::Optimize(_) => tr!(lang, "一键优化", "Optimize"),
            Route::Issues => tr!(lang, "问题列表", "Issues"),
            Route::Handling(_) => tr!(lang, "处理方法", "What to do"),
            Route::Processes => tr!(lang, "进程排行", "Processes"),
            Route::Settings => tr!(lang, "设置", "Settings"),
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
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Tone::Ok => tr!(lang, "正常", "Normal"),
            Tone::Warning => tr!(lang, "需要注意", "Needs attention"),
            Tone::Critical => tr!(lang, "严重", "Critical"),
            Tone::Unknown => tr!(lang, "未知", "Unknown"),
            Tone::Paused => tr!(lang, "已暂停", "Paused"),
            Tone::Recovering => tr!(lang, "正在确认恢复", "Confirming recovery"),
            Tone::Loading => tr!(lang, "正在读取", "Reading"),
            Tone::Neutral => tr!(lang, "提示", "Note"),
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
    /// Analyse which open apps drive an issue, then show them for confirmation.
    Optimize(Group),
    /// Select or clear one app in the optimization list.
    ToggleApp(u32),
    /// Ask the selected apps to quit normally.
    QuitSelected,
    SetLanguage(LanguageChoice),
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

/// One checkbox row.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub title: String,
    pub detail: String,
    pub checked: bool,
    pub command: Command,
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
    Checks(Vec<Check>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Footer {
    /// Monitoring is paused, so the pause button resumes.
    pub paused: bool,
    pub pause: Button,
    pub interval: Button,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub blocks: Vec<Block>,
    pub footer: Footer,
}

pub fn page(monitor: &Monitor, route: Route, back: Option<Route>, now: u64, ui: Ui) -> Page {
    let view = View {
        monitor,
        health: monitor.health(now),
        issues: monitor.issues(),
        lang: ui.lang,
        ui,
    };
    let lang = ui.lang;
    let mut blocks = Vec::new();
    if let Some(back) = back {
        blocks.push(Block::Header {
            back: back.title(lang).into(),
            title: match route {
                Route::Processes => tr!(lang, "占用内存最多的进程", "Top memory users").into(),
                Route::Settings => tr!(lang, "设置与数据说明", "Settings and data notes").into(),
                Route::Issues => tr!(lang, "需要注意的问题", "Issues needing attention").into(),
                Route::Handling(group) => view
                    .issues
                    .iter()
                    .find(|i| i.group == group)
                    .map_or(tr!(lang, "处理方法", "What to do"), |i| {
                        issue_title(lang, i)
                    })
                    .into(),
                other => other.title(lang).into(),
            },
            badge: badge(&view, route),
        });
    }
    match route {
        Route::Home => pages::home(&view, &mut blocks),
        Route::Detail(Resource::Memory) => pages::memory_detail(&view, &mut blocks),
        Route::Detail(Resource::Cpu) => pages::cpu_detail(&view, &mut blocks),
        Route::Detail(Resource::Disk) => pages::disk_detail(&view, &mut blocks),
        Route::Detail(Resource::Thermal) => pages::thermal_detail(&view, &mut blocks),
        Route::Optimize(group) => pages::optimize(&view, group, &mut blocks),
        Route::Issues => pages::issues(&view, &mut blocks),
        Route::Handling(group) => pages::handling(&view, group, &mut blocks),
        Route::Processes => pages::processes(&view, &mut blocks),
        Route::Settings => pages::settings(&view, &mut blocks),
    }
    let every = interval_label(lang, monitor.interval());
    Page {
        blocks,
        footer: Footer {
            paused: monitor.paused(),
            pause: Button::new(
                match (monitor.paused(), lang) {
                    (true, Lang::Zh) => "继续采集",
                    (false, Lang::Zh) => "暂停采集",
                    (true, Lang::En) => "Resume",
                    (false, Lang::En) => "Pause",
                },
                Command::TogglePause,
            ),
            interval: Button::new(
                tr!(lang, format!("每 {every}采样"), format!("Every {every}")),
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
                (
                    Tone::Recovering,
                    tr!(view.lang, "正在确认恢复", "Confirming recovery").to_string(),
                )
            } else {
                (
                    Tone::of(i.severity),
                    severity_word(view.lang, i.severity).to_string(),
                )
            }
        }),
        _ => None,
    }
}

/// One plain sentence for the status item, the home title and accessibility.
pub fn conclusion(monitor: &Monitor, now: u64, ui: Ui) -> (Tone, String) {
    let view = View {
        monitor,
        health: monitor.health(now),
        issues: monitor.issues(),
        lang: ui.lang,
        ui,
    };
    let (tone, title, _) = headline(&view);
    (tone, title)
}
