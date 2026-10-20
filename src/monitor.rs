//! Platform-neutral state behind the menu-bar popover.
//!
//! Holds the latest sample, the alert `Engine`, bounded `History`, evidence for gaps between
//! samples, on-demand process results and tool-launch feedback. AppKit renders pages built from
//! this state by `panel`, so every transition is testable without a window server.

use std::collections::VecDeque;

use crate::advice::{self, Finding, Observation};
use crate::alerts::{Engine, History, Policy, RuleStatus, Sample};
use crate::model::{ProcessScan, RateStatus, Severity, Snapshot};

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

/// Related rules are presented together; each keeps its own Engine state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Memory,
    Cpu,
    Disk,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Memory, Group::Cpu, Group::Disk];

    pub fn rules(self) -> &'static [&'static str] {
        match self {
            Group::Memory => &["memory_pressure", "swap_activity"],
            Group::Cpu => &["cpu_saturation"],
            Group::Disk => &["disk_space"],
        }
    }

    pub fn tool(self) -> Tool {
        match self {
            Group::Disk => Tool::StorageSettings,
            _ => Tool::ActivityMonitor,
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

/// A rule's reading in the newest collection attempt.
#[derive(Clone, Debug, PartialEq)]
pub enum Current {
    Abnormal(Severity, String),
    Clear,
    Unknown,
    /// The newest attempt failed: nothing is known about now.
    NoSample,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveRule {
    pub id: &'static str,
    pub status: RuleStatus,
    pub current: Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// At least one active rule is still abnormal now.
    Ongoing,
    /// No active rule is abnormal now, but at least one cannot be judged; streaks do not move.
    Undetermined,
    /// Every active rule read normal; the Engine needs `required` consecutive normal samples.
    Confirming { clear: u32, required: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub group: Group,
    pub severity: Severity,
    /// Active rules only, in `Group::rules` order.
    pub rules: Vec<ActiveRule>,
    pub recovery: Recovery,
    /// Earliest first-abnormal sample among the active rules.
    pub since: Option<u64>,
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

    fn active_groups(&self) -> Vec<Group> {
        Group::ALL
            .into_iter()
            .filter(|g| {
                g.rules().iter().any(|id| {
                    self.engine
                        .status(id)
                        .is_some_and(|status| status.active.is_some())
                })
            })
            .collect()
    }

    /// Active issues, most severe first.
    pub fn issues(&self) -> Vec<Issue> {
        let policy = self.engine.policy();
        let mut issues: Vec<Issue> = Group::ALL
            .into_iter()
            .filter_map(|group| {
                let rules: Vec<ActiveRule> = group
                    .rules()
                    .iter()
                    .filter_map(|id| {
                        let status = self.engine.status(id)?;
                        status.active?;
                        let current = match self.current().map(|l| l.observation(id)) {
                            None => Current::NoSample,
                            Some(Some(Observation::Abnormal(severity, evidence))) => {
                                Current::Abnormal(*severity, evidence.clone())
                            }
                            Some(Some(Observation::Clear)) => Current::Clear,
                            Some(_) => Current::Unknown,
                        };
                        Some(ActiveRule {
                            id,
                            status,
                            current,
                        })
                    })
                    .collect();
                let severity = rules.iter().filter_map(|r| r.status.active).max()?;
                let recovery = if rules
                    .iter()
                    .any(|r| matches!(r.current, Current::Abnormal(..)))
                {
                    Recovery::Ongoing
                } else if rules.iter().all(|r| r.current == Current::Clear) {
                    Recovery::Confirming {
                        clear: rules
                            .iter()
                            .map(|r| r.status.clear_streak)
                            .min()
                            .unwrap_or(0),
                        required: policy.recovery_samples,
                    }
                } else {
                    Recovery::Undetermined
                };
                let since = rules.iter().filter_map(|r| r.status.episode_start).min();
                Some(Issue {
                    group,
                    severity,
                    rules,
                    recovery,
                    since,
                })
            })
            .collect();
        issues.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.group.cmp(&b.group)));
        issues
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

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::advice::tests::{ready, snapshot};
    use crate::advice::{GIB, MIB};
    use crate::model::{Pressure, Process, SourceStatus};

    pub fn at(mut s: Snapshot, t: u64) -> Snapshot {
        s.unix_seconds = t;
        s
    }

    pub fn normal(t: u64) -> Snapshot {
        at(snapshot(Pressure::Normal, 300 * GIB), t)
    }

    #[test]
    fn loading_then_warmup_then_normal() {
        let mut monitor = Monitor::new(30);
        assert_eq!(monitor.health(0), Health::Loading);
        let mut first = normal(0);
        first.activity = crate::activity::empty(RateStatus::Warmup);
        monitor.record(Ok(first), 0);
        assert_eq!(monitor.health(0), Health::Warmup);
        monitor.record(Ok(normal(30)), 30);
        assert_eq!(monitor.health(30), Health::Normal);
    }

    #[test]
    fn failure_retains_alerts_and_hides_current_values() {
        let mut monitor = Monitor::new(30);
        monitor.record(Ok(at(snapshot(Pressure::Critical, 300 * GIB), 0)), 0);
        monitor.record(Err("无法读取物理内存总量".into()), 30);
        monitor.record(Err("无法读取物理内存总量".into()), 60);
        assert_eq!(monitor.health(60), Health::Failed);
        assert!(monitor.current().is_none());
        assert_eq!(monitor.failure().unwrap().2, 2);
        let issues = monitor.issues();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Critical);
        assert_eq!(issues[0].rules[0].current, Current::NoSample);
        assert_eq!(issues[0].recovery, Recovery::Undetermined);
        // Consecutive failures are one piece of gap evidence.
        assert_eq!(
            monitor
                .marks()
                .filter(|m| matches!(m, Mark::Failed(_)))
                .count(),
            1
        );
        monitor.record(Ok(normal(90)), 90);
        assert_eq!(monitor.health(90), Health::Recovering);
    }

    #[test]
    fn grouped_memory_issue_keeps_rule_evidence_and_highest_severity() {
        let mut monitor = Monitor::new(30);
        for t in [0, 30] {
            let mut s = at(snapshot(Pressure::Warning, 300 * GIB), t);
            s.activity = ready(20.0 * MIB, 20.0);
            monitor.record(Ok(s), t);
        }
        let issues = monitor.issues();
        assert_eq!(issues.len(), 1, "pressure and swap are one presented issue");
        let memory = &issues[0];
        assert_eq!(memory.group, Group::Memory);
        assert_eq!(memory.severity, Severity::Critical);
        assert_eq!(memory.rules.len(), 2);
        assert_eq!(memory.since, Some(0));
        assert_eq!(monitor.health(30), Health::Issues(Severity::Critical));

        // Swap recovers first; the group stays active until pressure also recovers.
        for t in [60, 90] {
            monitor.record(Ok(at(snapshot(Pressure::Warning, 300 * GIB), t)), t);
        }
        let memory = &monitor.issues()[0];
        assert_eq!(memory.rules.len(), 1);
        assert_eq!(memory.rules[0].id, "memory_pressure");
        assert_eq!(memory.severity, Severity::Warning);
        assert!(monitor.recovered().is_none());
        for t in [120, 150] {
            monitor.record(Ok(normal(t)), t);
            if t == 120 {
                assert_eq!(
                    monitor.issues()[0].recovery,
                    Recovery::Confirming {
                        clear: 1,
                        required: 2
                    }
                );
            }
        }
        assert!(monitor.issues().is_empty());
        assert_eq!(monitor.recovered(), Some((150, &[Group::Memory][..])));
        monitor.record(Ok(normal(180)), 180);
        assert!(monitor.recovered().is_none());
    }

    #[test]
    fn retained_evidence_differs_from_current_reading() {
        let mut monitor = Monitor::new(30);
        let mut s = at(snapshot(Pressure::Normal, 300 * GIB), 0);
        s.activity = ready(20.0 * MIB, 20.0);
        monitor.record(Ok(s), 0);
        monitor.record(Ok(normal(30)), 30);
        let rule = &monitor.issues()[0].rules[0];
        assert_eq!(rule.current, Current::Clear);
        let (when, severity, evidence) = rule.status.last_abnormal.clone().unwrap();
        assert_eq!((when, severity), (0, Severity::Critical));
        assert!(evidence.contains("20.0 MiB/s"));
    }

    #[test]
    fn tool_feedback_never_moves_recovery() {
        let mut monitor = Monitor::new(30);
        monitor.record(Ok(at(snapshot(Pressure::Critical, 300 * GIB), 0)), 0);
        let before = monitor.issues();
        monitor.note_open(Tool::ActivityMonitor, Ok(()), 5);
        monitor.note_open(Tool::ActivityMonitor, Err("x".into()), 6);
        assert_eq!(monitor.issues(), before);
        assert_eq!(monitor.health(6), Health::Issues(Severity::Critical));
        assert_eq!(monitor.tool_feedback().unwrap().result, Err("x".into()));
    }

    #[test]
    fn partial_sources_are_not_reported_healthy() {
        let mut monitor = Monitor::new(30);
        let mut s = snapshot(Pressure::Unknown, 300 * GIB);
        s.sources.push(SourceStatus {
            source: "kern.memorystatus_vm_pressure_level",
            ok: false,
            elapsed_micros: 3,
            error: Some("denied".into()),
        });
        monitor.record(Ok(s), 0);
        assert_eq!(monitor.health(0), Health::Partial);
        assert_eq!(unmeasured(monitor.current().unwrap()), ["内存压力"]);
    }

    #[test]
    fn pause_interval_and_resume_state() {
        let mut monitor = Monitor::new(30);
        monitor.record(Ok(normal(0)), 0);
        monitor.set_paused(true, 10);
        assert_eq!(
            monitor.health(1000),
            Health::Paused,
            "paused is never stale"
        );
        assert!(monitor.set_interval(5, 20));
        assert!(monitor.paused(), "changing the interval does not resume");
        assert!(!monitor.set_interval(4, 20));
        assert!(!monitor.set_interval(5, 20));
        assert!(monitor.set_interval(3600, 21));
        assert!(
            monitor.set_interval(7, 22),
            "custom CLI values are accepted"
        );
        monitor.set_paused(false, 100);
        assert_eq!(monitor.interval(), 7);
        assert!(!monitor.stale(120));
        assert!(monitor.stale(127));
        assert_eq!(monitor.interval_at(5), 30);
        assert_eq!(monitor.interval_at(100), 7);
    }

    #[test]
    fn stale_after_missed_samples_and_cleared_by_next_attempt() {
        let mut monitor = Monitor::new(10);
        monitor.record(Ok(normal(0)), 0);
        assert_eq!(monitor.health(35), Health::Normal);
        assert_eq!(monitor.health(36), Health::Stale);
        monitor.record(Ok(normal(400)), 400);
        assert_eq!(monitor.health(400), Health::Normal);
    }

    #[test]
    fn sleep_gap_is_recorded_as_evidence() {
        let mut monitor = Monitor::new(30);
        monitor.record(Ok(normal(0)), 0);
        let mut woke = normal(4000);
        woke.activity = crate::activity::empty(RateStatus::SleepGap);
        monitor.record(Ok(woke), 4000);
        assert!(monitor.marks().any(|m| *m == Mark::SleepGap(4000)));
        // Rates are pending again, not normal.
        assert_eq!(monitor.health(4000), Health::Warmup);
    }

    #[test]
    fn process_reads_do_not_overlap_and_keep_previous_result() {
        let mut monitor = Monitor::new(30);
        assert!(matches!(monitor.processes(), Processes::NotRead));
        assert!(monitor.begin_processes(1));
        assert!(!monitor.begin_processes(2));
        let scan = ProcessScan {
            top: vec![Process {
                pid: 1,
                name: "a".into(),
                resident_bytes: 1,
            }],
            listed: 3,
            unreadable: 2,
        };
        monitor.finish_processes(Ok(scan), 3);
        assert!(monitor.begin_processes(4));
        match monitor.processes() {
            Processes::Loading {
                since: 4,
                previous: Some(previous),
            } => assert_eq!(previous.at, 3),
            other => panic!("{other:?}"),
        }
        monitor.finish_processes(Err("系统没有返回任何进程信息".into()), 5);
        assert!(matches!(
            monitor.processes(),
            Processes::Done(ProcessResult {
                at: 5,
                outcome: Err(_)
            })
        ));
    }

    #[test]
    fn marks_are_pruned_with_history() {
        let mut monitor = Monitor::new(5);
        for t in 0..300u64 {
            if t % 3 == 0 {
                monitor.record(Err("x".into()), t * 5);
            } else {
                monitor.record(Ok(normal(t * 5)), t * 5);
            }
        }
        let oldest = monitor.history().samples().next().unwrap().unix_seconds;
        assert!(monitor.marks().all(|m| m.at() >= oldest));
        assert!(monitor.marks().count() <= crate::alerts::HISTORY_CAPACITY + 1);
        assert_eq!(monitor.interval_at(oldest), 5);
    }
}
