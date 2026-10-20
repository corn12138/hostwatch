//! One-click optimization: which open apps to ask to quit, and what happened to each request.
//!
//! The only action is a normal quit request to apps the person opened (like ⌘Q): apps may ask to
//! save or decline, nothing is force-killed, no files or settings change. Helper processes count
//! toward the app that started them. Whether the alert then recovers is still decided by the
//! alert Engine from later samples.

use std::collections::HashMap;

use crate::monitor::Group;

/// Apps that keep the desktop working or are Hostwatch itself; never offered.
const PROTECTED: &[&str] = &[
    "com.apple.finder",
    "com.apple.dock",
    "com.apple.loginwindow",
    "com.apple.systemuiserver",
    "com.apple.controlcenter",
    "com.apple.WindowManager",
    "io.github.corn12138.hostwatch",
];

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
/// Offered at all / preselected; product heuristics.
const MEMORY_FLOOR: u64 = 200 * MIB;
const MEMORY_PRESELECT: u64 = GIB;
const CPU_FLOOR: f64 = 5.0;
const CPU_PRESELECT: f64 = 25.0;
const MAX_ROWS: usize = 5;
const MAX_PRESELECTED: usize = 3;
/// Length of the CPU measurement window.
pub const CPU_WINDOW_MS: u64 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Memory,
    Cpu,
}

impl Focus {
    /// The resource whose biggest users are offered for an issue; disk space has no safe quit.
    pub fn for_group(group: Group) -> Option<Self> {
        match group {
            Group::Memory => Some(Focus::Memory),
            Group::Cpu | Group::Thermal => Some(Focus::Cpu),
            Group::Disk => None,
        }
    }
}

/// A regular app the person can see in the Dock.
#[derive(Clone, Debug, PartialEq)]
pub struct RunningApp {
    pub pid: u32,
    pub name: String,
    pub bundle_id: Option<String>,
    pub active: bool,
}

/// One process at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcSample {
    pub pid: u32,
    pub ppid: u32,
    pub footprint: u64,
    pub cpu_ns: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub pid: u32,
    pub name: String,
    pub bundle_id: Option<String>,
    /// The app and every process it started.
    pub footprint_bytes: u64,
    /// Percent of one core over the measurement window, as Activity Monitor counts it.
    pub cpu_percent: f64,
    pub processes: usize,
    /// The app in front, probably in use: shown but never preselected.
    pub active: bool,
    pub selected: bool,
}

/// Ranks open apps by the resource behind the alert.
pub fn plan(
    focus: Focus,
    apps: &[RunningApp],
    before: &[ProcSample],
    after: &[ProcSample],
    elapsed_ns: u64,
    own_pid: u32,
) -> Vec<Candidate> {
    let offered: HashMap<u32, &RunningApp> = apps
        .iter()
        .filter(|app| app.pid != own_pid)
        .filter(|app| {
            app.bundle_id
                .as_deref()
                .map_or(true, |id| !PROTECTED.contains(&id))
        })
        .map(|app| (app.pid, app))
        .collect();
    let parent: HashMap<u32, u32> = after.iter().map(|p| (p.pid, p.ppid)).collect();
    let earlier: HashMap<u32, u64> = before.iter().map(|p| (p.pid, p.cpu_ns)).collect();
    let owner = |mut pid: u32| {
        // Parent chains are short; the bound guards against a pid reused into a cycle.
        for _ in 0..32 {
            if offered.contains_key(&pid) {
                return Some(pid);
            }
            match parent.get(&pid) {
                Some(&ppid) if ppid > 1 && ppid != pid => pid = ppid,
                _ => return None,
            }
        }
        None
    };
    let mut totals: HashMap<u32, (u64, u64, usize)> = HashMap::new();
    for process in after {
        let Some(app) = owner(process.pid) else {
            continue;
        };
        let cpu = earlier
            .get(&process.pid)
            .map_or(0, |&start| process.cpu_ns.saturating_sub(start));
        let entry = totals.entry(app).or_default();
        entry.0 += process.footprint;
        entry.1 += cpu;
        entry.2 += 1;
    }
    let elapsed = elapsed_ns.max(1) as f64;
    let mut candidates: Vec<Candidate> = totals
        .into_iter()
        .map(|(pid, (footprint, cpu, processes))| {
            let app = offered[&pid];
            Candidate {
                pid,
                name: app.name.clone(),
                bundle_id: app.bundle_id.clone(),
                footprint_bytes: footprint,
                cpu_percent: cpu as f64 / elapsed * 100.0,
                processes,
                active: app.active,
                selected: false,
            }
        })
        .filter(|c| match focus {
            Focus::Memory => c.footprint_bytes >= MEMORY_FLOOR,
            Focus::Cpu => c.cpu_percent >= CPU_FLOOR,
        })
        .collect();
    candidates.sort_by(|a, b| match focus {
        Focus::Memory => b.footprint_bytes.cmp(&a.footprint_bytes),
        Focus::Cpu => b.cpu_percent.total_cmp(&a.cpu_percent),
    });
    candidates.truncate(MAX_ROWS);
    let mut preselected = 0;
    for candidate in &mut candidates {
        let heavy = match focus {
            Focus::Memory => candidate.footprint_bytes >= MEMORY_PRESELECT,
            Focus::Cpu => candidate.cpu_percent >= CPU_PRESELECT,
        };
        if heavy && !candidate.active && preselected < MAX_PRESELECTED {
            candidate.selected = true;
            preselected += 1;
        }
    }
    candidates
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitState {
    /// Request delivered; the app has not quit yet.
    Requested,
    /// The system did not deliver the request (the app is gone or refused).
    NotSent,
    Quit,
    /// Still running when checked; usually waiting for the person to save or confirm.
    StillRunning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuitOutcome {
    pub pid: u32,
    pub name: String,
    pub state: QuitState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Optimization {
    Analyzing {
        group: Group,
        since: u64,
    },
    Ready {
        group: Group,
        at: u64,
        candidates: Vec<Candidate>,
    },
    Failed {
        group: Group,
        at: u64,
        error: String,
    },
    Applied {
        group: Group,
        at: u64,
        outcomes: Vec<QuitOutcome>,
    },
}

impl Optimization {
    pub fn group(&self) -> Group {
        match self {
            Optimization::Analyzing { group, .. }
            | Optimization::Ready { group, .. }
            | Optimization::Failed { group, .. }
            | Optimization::Applied { group, .. } => *group,
        }
    }
}

#[derive(Default)]
pub struct Optimizer {
    state: Option<Optimization>,
}

impl Optimizer {
    pub fn state(&self) -> Option<&Optimization> {
        self.state.as_ref()
    }

    /// Starts an analysis; refuses to overlap one already running.
    pub fn begin(&mut self, group: Group, now: u64) -> bool {
        if matches!(self.state, Some(Optimization::Analyzing { .. })) {
            return false;
        }
        self.state = Some(Optimization::Analyzing { group, since: now });
        true
    }

    pub fn finish(&mut self, result: Result<Vec<Candidate>, String>, now: u64) {
        let Some(Optimization::Analyzing { group, .. }) = self.state else {
            return;
        };
        self.state = Some(match result {
            Ok(candidates) => Optimization::Ready {
                group,
                at: now,
                candidates,
            },
            Err(error) => Optimization::Failed {
                group,
                at: now,
                error,
            },
        });
    }

    pub fn toggle(&mut self, pid: u32) {
        if let Some(Optimization::Ready { candidates, .. }) = &mut self.state {
            if let Some(candidate) = candidates.iter_mut().find(|c| c.pid == pid) {
                candidate.selected = !candidate.selected;
            }
        }
    }

    pub fn selected(&self) -> Vec<Candidate> {
        match &self.state {
            Some(Optimization::Ready { candidates, .. }) => {
                candidates.iter().filter(|c| c.selected).cloned().collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn applied(&mut self, outcomes: Vec<QuitOutcome>, now: u64) {
        if let Some(group) = self.state.as_ref().map(Optimization::group) {
            self.state = Some(Optimization::Applied {
                group,
                at: now,
                outcomes,
            });
        }
    }

    /// Pids whose quit request is still pending a check.
    pub fn pending(&self) -> Vec<u32> {
        match &self.state {
            Some(Optimization::Applied { outcomes, .. }) => outcomes
                .iter()
                .filter(|o| o.state == QuitState::Requested)
                .map(|o| o.pid)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Records which pending apps have quit; on the final check the rest are still running.
    pub fn verified(&mut self, quit: impl Fn(u32) -> bool, last: bool) {
        if let Some(Optimization::Applied { outcomes, .. }) = &mut self.state {
            for outcome in outcomes
                .iter_mut()
                .filter(|o| o.state == QuitState::Requested)
            {
                if quit(outcome.pid) {
                    outcome.state = QuitState::Quit;
                } else if last {
                    outcome.state = QuitState::StillRunning;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(pid: u32, name: &str, bundle: &str, active: bool) -> RunningApp {
        RunningApp {
            pid,
            name: name.into(),
            bundle_id: Some(bundle.into()),
            active,
        }
    }

    fn proc(pid: u32, ppid: u32, footprint: u64, cpu_ns: u64) -> ProcSample {
        ProcSample {
            pid,
            ppid,
            footprint,
            cpu_ns,
        }
    }

    const SECOND: u64 = 1_000_000_000;

    #[test]
    fn helpers_count_toward_their_app_and_protected_apps_are_never_offered() {
        let apps = [
            app(100, "Browser", "com.example.browser", false),
            app(200, "Editor", "com.example.editor", true),
            app(300, "访达", "com.apple.finder", false),
            app(400, "Hostwatch", "io.github.corn12138.hostwatch", false),
        ];
        let after = [
            proc(1, 0, 50 * MIB, 0),
            proc(100, 1, GIB, 0),
            proc(101, 100, GIB, 0),
            proc(102, 101, GIB / 2, 0),
            proc(200, 1, 2 * GIB, 0),
            proc(300, 1, 4 * GIB, 0),
            proc(400, 1, 4 * GIB, 0),
            // Started by launchd, not by an app: not attributed to anyone.
            proc(500, 1, 8 * GIB, 0),
        ];
        let plan = plan(Focus::Memory, &apps, &after, &after, SECOND, 400);
        let names: Vec<&str> = plan.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Browser", "Editor"]);
        assert_eq!(plan[0].footprint_bytes, 2 * GIB + GIB / 2);
        assert_eq!(plan[0].processes, 3);
        assert!(plan[0].selected);
        assert!(!plan[1].selected, "the app in front is never preselected");
    }

    #[test]
    fn cpu_is_measured_over_the_window_like_activity_monitor() {
        let apps = [
            app(100, "Encoder", "com.example.encoder", false),
            app(200, "Idle", "com.example.idle", false),
        ];
        let before = [proc(100, 1, 0, 0), proc(101, 100, 0, 0), proc(200, 1, 0, 5)];
        let after = [
            proc(100, 1, 0, SECOND),
            proc(101, 100, 0, SECOND / 2),
            proc(200, 1, 0, 5 + SECOND / 100),
            // Appeared during the window: no baseline, contributes no CPU.
            proc(102, 100, 0, 9 * SECOND),
        ];
        let plan = plan(Focus::Cpu, &apps, &before, &after, SECOND, 0);
        assert_eq!(plan.len(), 1, "1% is below the floor");
        assert!((plan[0].cpu_percent - 150.0).abs() < 1e-9);
        assert_eq!(plan[0].processes, 3);
        assert!(plan[0].selected);
    }

    #[test]
    fn at_most_three_are_preselected() {
        let apps: Vec<RunningApp> = (1..=7)
            .map(|i| {
                app(
                    i * 10,
                    &format!("App{i}"),
                    &format!("com.example.{i}"),
                    false,
                )
            })
            .collect();
        let after: Vec<ProcSample> = (1..=7)
            .map(|i| proc(i * 10, 1, i as u64 * GIB, 0))
            .collect();
        let plan = plan(Focus::Memory, &apps, &after, &after, SECOND, 0);
        assert_eq!(plan.len(), MAX_ROWS);
        assert_eq!(plan[0].name, "App7");
        assert_eq!(plan.iter().filter(|c| c.selected).count(), 3);
    }

    #[test]
    fn lifecycle_toggles_applies_and_verifies() {
        let mut optimizer = Optimizer::default();
        assert!(optimizer.begin(Group::Memory, 1));
        assert!(
            !optimizer.begin(Group::Memory, 2),
            "no overlapping analysis"
        );
        let candidate = |pid, selected| Candidate {
            pid,
            name: format!("A{pid}"),
            bundle_id: None,
            footprint_bytes: GIB,
            cpu_percent: 0.0,
            processes: 1,
            active: false,
            selected,
        };
        optimizer.finish(Ok(vec![candidate(1, true), candidate(2, false)]), 3);
        optimizer.toggle(2);
        optimizer.toggle(1);
        let selected: Vec<u32> = optimizer.selected().iter().map(|c| c.pid).collect();
        assert_eq!(selected, [2]);
        optimizer.applied(
            vec![
                QuitOutcome {
                    pid: 2,
                    name: "A2".into(),
                    state: QuitState::Requested,
                },
                QuitOutcome {
                    pid: 3,
                    name: "A3".into(),
                    state: QuitState::NotSent,
                },
            ],
            4,
        );
        assert_eq!(optimizer.pending(), [2]);
        optimizer.verified(|_| false, false);
        assert_eq!(
            optimizer.pending(),
            [2],
            "slow apps stay pending until the last check"
        );
        optimizer.verified(|_| false, true);
        assert!(optimizer.pending().is_empty());
        match optimizer.state() {
            Some(Optimization::Applied { outcomes, .. }) => {
                assert_eq!(outcomes[0].state, QuitState::StillRunning);
                assert_eq!(outcomes[1].state, QuitState::NotSent);
            }
            other => panic!("{other:?}"),
        }
        let mut failed = Optimizer::default();
        failed.begin(Group::Cpu, 1);
        failed.finish(Err("x".into()), 2);
        assert!(matches!(failed.state(), Some(Optimization::Failed { .. })));
    }
}
