//! Platform-neutral state behind the menu-bar popover.
//!
//! Holds the latest sample, the alert `Engine`, bounded `History`, evidence for gaps between
//! samples, on-demand process results and tool-launch feedback. AppKit renders pages built from
//! this state by `panel`, so every transition is testable without a window server. Issue grouping
//! and Engine-owned recovery live in `issues`.

mod issues;
#[cfg(test)]
pub mod tests;

use std::collections::VecDeque;

use crate::advice::{self, Finding, Observation};
use crate::alerts::{Engine, History, Policy, Sample};
use crate::model::{ProcessScan, RateStatus, Severity, Snapshot};
pub use issues::{ActiveRule, Current, Group, Issue, Recovery};

pub const INTERVAL_PRESETS: [u64; 5] = [5, 10, 30, 60, 300];
pub const INTERVAL_RANGE: std::ops::RangeInclusive<u64> = 5..=3600;

/// Read-only system tools that handling steps point to. Opening one never changes system state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    ActivityMonitor,
    StorageSettings,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::ActivityMonitor => "活动监视器",
            Tool::StorageSettings => "“储存空间”设置",
        }
    }

    pub fn manual_path(self) -> &'static str {
        match self {
            Tool::ActivityMonitor => "访达 › 应用程序 › 实用工具 › 活动监视器",
            Tool::StorageSettings => "苹果菜单 › 系统设置 › 通用 › 储存空间",
        }
    }
}

/// Recorded evidence about time between samples. Only these explain a gap; a long timestamp
/// difference on its own is shown as "no data", never as sleep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A collection attempt failed.
    Failed(u64),
    Paused(u64),
    Resumed(u64),
    /// The sample at this time measured wall time running far ahead of monotonic time.
    SleepGap(u64),
    /// Sampling cadence in effect from this time.
    Interval(u64, u64),
}

impl Mark {
    fn at(self) -> u64 {
        match self {
            Mark::Failed(t)
            | Mark::Paused(t)
            | Mark::Resumed(t)
            | Mark::SleepGap(t)
            | Mark::Interval(t, _) => t,
        }
    }
}

pub struct Latest {
    pub snapshot: Snapshot,
    pub findings: Vec<Finding>,
}

impl Latest {
    pub fn observation(&self, id: &str) -> Option<&Observation> {
        self.findings
            .iter()
            .find(|f| f.id == id)
            .map(|f| &f.observation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// No collection attempt yet.
    Loading,
    Paused,
    /// The newest collection attempt failed as a whole.
    Failed,
    /// Sampling is running but no attempt happened for several intervals.
    Stale,
    Issues(Severity),
    /// Every active issue currently reads normal and awaits Engine confirmation.
    Recovering,
    /// Some sources or rules could not be judged.
    Partial,
    /// First sample after start or resume: rates need one more sample.
    Warmup,
    Normal,
}

#[derive(Debug)]
pub struct ProcessResult {
    pub at: u64,
    pub outcome: Result<ProcessScan, String>,
}

#[derive(Debug)]
pub enum Processes {
    NotRead,
    Loading {
        since: u64,
        previous: Option<ProcessResult>,
    },
    Done(ProcessResult),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolFeedback {
    pub tool: Tool,
    pub at: u64,
    pub result: Result<(), String>,
}

pub struct Monitor {
    interval: u64,
    paused: bool,
    paused_at: Option<u64>,
    engine: Engine,
    history: History,
    marks: VecDeque<Mark>,
    /// Latest successful sample; kept after a failure only for its timestamp and old values,
    /// which are never presented as current.
    latest: Option<Latest>,
    failure: Option<(u64, String)>,
    consecutive_failures: u32,
    /// Most recent collection attempt, interval change or resume; staleness is measured from it.
    cadence_from: Option<u64>,
    /// Groups whose last active rule recovered in the newest sample.
    recovered: Option<(u64, Vec<Group>)>,
    processes: Processes,
    tool: Option<ToolFeedback>,
}

impl Monitor {
    pub fn new(interval: u64) -> Self {
        let mut marks = VecDeque::new();
        marks.push_back(Mark::Interval(0, interval));
        Self {
            interval,
            paused: false,
            paused_at: None,
            engine: Engine::new(Policy::default()),
            history: History::new(),
            marks,
            latest: None,
            failure: None,
            consecutive_failures: 0,
            cadence_from: None,
            recovered: None,
            processes: Processes::NotRead,
            tool: None,
        }
    }

    pub fn interval(&self) -> u64 {
        self.interval
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn paused_at(&self) -> Option<u64> {
        self.paused_at
    }

    pub fn policy(&self) -> Policy {
        self.engine.policy()
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn marks(&self) -> impl Iterator<Item = &Mark> {
        self.marks.iter()
    }

    pub fn latest(&self) -> Option<&Latest> {
        self.latest.as_ref()
    }

    pub fn failure(&self) -> Option<(u64, &str, u32)> {
        self.failure
            .as_ref()
            .map(|(at, error)| (*at, error.as_str(), self.consecutive_failures))
    }

    /// The latest sample when it describes the newest attempt.
    pub fn current(&self) -> Option<&Latest> {
        self.latest.as_ref().filter(|_| self.failure.is_none())
    }

    pub fn recovered(&self) -> Option<(u64, &[Group])> {
        self.recovered.as_ref().map(|(at, g)| (*at, g.as_slice()))
    }

    pub fn processes(&self) -> &Processes {
        &self.processes
    }

    pub fn tool_feedback(&self) -> Option<&ToolFeedback> {
        self.tool.as_ref()
    }

    pub fn record(&mut self, result: Result<Snapshot, String>, now: u64) {
        self.cadence_from = Some(now);
        self.recovered = None;
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                // The engine sees nothing: a failed sample neither confirms nor clears alerts.
                if !matches!(self.marks.back(), Some(Mark::Failed(_))) {
                    self.marks.push_back(Mark::Failed(now));
                }
                self.failure = Some((now, error));
                self.consecutive_failures += 1;
                return;
            }
        };
        self.failure = None;
        self.consecutive_failures = 0;
        let at = snapshot.unix_seconds;
        if snapshot.activity.status == RateStatus::SleepGap {
            self.marks.push_back(Mark::SleepGap(at));
        }
        self.history.push(Sample::from_snapshot(&snapshot));
        self.prune_marks();
        let before = self.active_groups();
        let findings = advice::assess(&snapshot);
        self.engine.observe(at, &findings);
        let after = self.active_groups();
        let recovered: Vec<Group> = before.into_iter().filter(|g| !after.contains(g)).collect();
        if !recovered.is_empty() {
            self.recovered = Some((at, recovered));
        }
        self.latest = Some(Latest { snapshot, findings });
    }

    /// Returns whether sampling should be (re)scheduled; changing it while paused does not resume.
    pub fn set_interval(&mut self, seconds: u64, now: u64) -> bool {
        if !INTERVAL_RANGE.contains(&seconds) || seconds == self.interval {
            return false;
        }
        self.interval = seconds;
        self.marks.push_back(Mark::Interval(now, seconds));
        if !self.paused {
            self.cadence_from = Some(now);
        }
        true
    }

    pub fn set_paused(&mut self, paused: bool, now: u64) {
        if paused == self.paused {
            return;
        }
        self.paused = paused;
        if paused {
            self.paused_at = Some(now);
            self.marks.push_back(Mark::Paused(now));
        } else {
            self.paused_at = None;
            self.marks.push_back(Mark::Resumed(now));
            self.cadence_from = Some(now);
        }
    }

    /// Records the outcome of opening a system tool. It is not a sample: health, alerts and
    /// recovery are untouched.
    pub fn note_open(&mut self, tool: Tool, result: Result<(), String>, now: u64) {
        self.tool = Some(ToolFeedback {
            tool,
            at: now,
            result,
        });
    }

    pub fn clear_tool_feedback(&mut self) {
        self.tool = None;
    }

    /// Starts an on-demand process read; refuses to overlap a read already in flight.
    pub fn begin_processes(&mut self, now: u64) -> bool {
        let previous = match std::mem::replace(&mut self.processes, Processes::NotRead) {
            Processes::Loading { since, previous } => {
                self.processes = Processes::Loading { since, previous };
                return false;
            }
            Processes::Done(result) => Some(result),
            Processes::NotRead => None,
        };
        self.processes = Processes::Loading {
            since: now,
            previous,
        };
        true
    }

    pub fn finish_processes(&mut self, outcome: Result<ProcessScan, String>, now: u64) {
        self.processes = Processes::Done(ProcessResult { at: now, outcome });
    }
    /// Whether sampling has fallen behind its schedule, e.g. after the machine slept.
    pub fn stale(&self, now: u64) -> bool {
        !self.paused
            && self
                .cadence_from
                .is_some_and(|from| now.saturating_sub(from) > 3 * self.interval + 5)
    }

    pub fn health(&self, now: u64) -> Health {
        if self.paused {
            return Health::Paused;
        }
        if self.cadence_from.is_none() {
            return Health::Loading;
        }
        if self.failure.is_some() {
            return Health::Failed;
        }
        if self.stale(now) {
            return Health::Stale;
        }
        let issues = self.issues();
        if let Some(top) = issues.first() {
            if issues
                .iter()
                .all(|i| matches!(i.recovery, Recovery::Confirming { .. }))
            {
                return Health::Recovering;
            }
            return Health::Issues(top.severity);
        }
        let Some(latest) = self.current() else {
            return Health::Loading;
        };
        if latest.snapshot.sources.iter().any(|s| !s.ok) || !unmeasured(latest).is_empty() {
            return Health::Partial;
        }
        if latest.snapshot.activity.status != RateStatus::Ready {
            return Health::Warmup;
        }
        Health::Normal
    }

    /// Cadence in effect at `t`.
    pub fn interval_at(&self, t: u64) -> u64 {
        self.marks
            .iter()
            .filter_map(|m| match *m {
                Mark::Interval(at, seconds) if at <= t => Some(seconds),
                _ => None,
            })
            .last()
            .unwrap_or(self.interval)
    }

    /// Drops marks older than the oldest retained sample, keeping the cadence in effect then.
    fn prune_marks(&mut self) {
        let Some(oldest) = self.history.samples().next().map(|s| s.unix_seconds) else {
            return;
        };
        let base = self.interval_at(oldest);
        let before = self.marks.len();
        self.marks.retain(|m| m.at() >= oldest);
        if self.marks.len() != before {
            self.marks.push_front(Mark::Interval(oldest, base));
        }
    }
}

/// Rule-level measurements the newest sample could not judge. While rates are warming up, the
/// rate-based rules are expected to be unknown and are reported as warmup instead.
pub fn unmeasured(latest: &Latest) -> Vec<&'static str> {
    let rates_pending = latest.snapshot.activity.status != RateStatus::Ready;
    latest
        .findings
        .iter()
        .filter(|f| f.observation == Observation::Unknown)
        .filter(|f| !(rates_pending && matches!(f.id, "swap_activity" | "cpu_saturation")))
        .map(|f| match f.id {
            "memory_pressure" => "内存压力",
            "swap_activity" => "交换读写",
            "cpu_saturation" => "处理器使用率",
            "disk_space" => "磁盘空间",
            other => other,
        })
        .collect()
}
