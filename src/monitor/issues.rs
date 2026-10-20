//! Related rules presented as one issue, with recovery read from the alert Engine. Clicking a
//! button never starts, advances or completes recovery.

use super::{Monitor, Tool};
use crate::advice::Observation;
use crate::alerts::RuleStatus;
use crate::model::Severity;

/// Related rules are presented together; each keeps its own Engine state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Memory,
    Cpu,
    Disk,
    Thermal,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Memory, Group::Cpu, Group::Disk, Group::Thermal];

    pub fn rules(self) -> &'static [&'static str] {
        match self {
            Group::Memory => &["memory_pressure", "swap_activity"],
            Group::Cpu => &["cpu_saturation"],
            Group::Disk => &["disk_space"],
            Group::Thermal => &["thermal_state"],
        }
    }

    pub fn tool(self) -> Tool {
        match self {
            Group::Disk => Tool::StorageSettings,
            _ => Tool::ActivityMonitor,
        }
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

impl Monitor {
    pub(super) fn active_groups(&self) -> Vec<Group> {
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
}
