use std::collections::{BTreeMap, VecDeque};

use crate::advice::{Finding, Observation};
use crate::model::{Event, EventKind, Severity, Snapshot};

pub const HISTORY_CAPACITY: usize = 120;

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// Consecutive abnormal samples before a warning is raised.
    pub warning_samples: u32,
    /// Critical states (e.g. kernel pressure critical) are raised on the first sample.
    pub critical_samples: u32,
    /// Consecutive clear samples before recovery.
    pub recovery_samples: u32,
    /// After a raise, the same rule is not raised again at the same or lower severity.
    pub cooldown_seconds: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            warning_samples: 2,
            critical_samples: 1,
            recovery_samples: 2,
            cooldown_seconds: 600,
        }
    }
}

#[derive(Default)]
struct RuleState {
    abnormal_streak: u32,
    clear_streak: u32,
    /// Severity currently considered active, if any.
    active: Option<Severity>,
    /// Highest severity announced during the current activation.
    announced: Option<Severity>,
    last_raised: Option<(u64, Severity)>,
    /// First abnormal sample of the streak that led to (or is building toward) activation.
    episode_start: Option<u64>,
    /// Most recent abnormal observation while abnormal or active: (time, severity, evidence).
    last_abnormal: Option<(u64, Severity, String)>,
}

/// Read-only view of one rule's sustained state, for presentation.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleStatus {
    pub active: Option<Severity>,
    pub abnormal_streak: u32,
    pub clear_streak: u32,
    pub episode_start: Option<u64>,
    pub last_abnormal: Option<(u64, Severity, String)>,
}

/// Turns per-sample findings into raise / escalate / recover transitions.
pub struct Engine {
    policy: Policy,
    rules: BTreeMap<&'static str, RuleState>,
}

impl Engine {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            rules: BTreeMap::new(),
        }
    }

    pub fn observe(&mut self, unix_seconds: u64, findings: &[Finding]) -> Vec<Event> {
        findings
            .iter()
            .filter_map(|finding| self.observe_one(unix_seconds, finding))
            .collect()
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    pub fn status(&self, id: &str) -> Option<RuleStatus> {
        self.rules.get(id).map(|state| RuleStatus {
            active: state.active,
            abnormal_streak: state.abnormal_streak,
            clear_streak: state.clear_streak,
            episode_start: state.episode_start,
            last_abnormal: state.last_abnormal.clone(),
        })
    }

    fn observe_one(&mut self, now: u64, finding: &Finding) -> Option<Event> {
        let policy = self.policy;
        let state = self.rules.entry(finding.id).or_default();
        let event = |kind, severity, evidence: String, streak| Event {
            id: finding.id,
            kind,
            severity,
            evidence,
            action: finding.action,
            streak,
        };
        match &finding.observation {
            // Missing data neither confirms nor clears a problem, and does not break a streak.
            Observation::Unknown => None,
            Observation::Clear => {
                state.abnormal_streak = 0;
                state.clear_streak += 1;
                let Some(severity) = state.active else {
                    state.episode_start = None;
                    state.last_abnormal = None;
                    return None;
                };
                if state.clear_streak < policy.recovery_samples {
                    return None;
                }
                state.active = None;
                state.episode_start = None;
                state.last_abnormal = None;
                state.announced?;
                state.announced = None;
                Some(event(
                    EventKind::Recovered,
                    severity,
                    format!("连续 {} 个有效样本恢复正常", state.clear_streak),
                    state.clear_streak,
                ))
            }
            Observation::Abnormal(severity, evidence) => {
                let severity = *severity;
                state.clear_streak = 0;
                if state.abnormal_streak == 0 && state.active.is_none() {
                    state.episode_start = Some(now);
                }
                state.abnormal_streak += 1;
                state.last_abnormal = Some((now, severity, evidence.clone()));
                let required = if severity >= Severity::Critical {
                    policy.critical_samples
                } else {
                    policy.warning_samples
                };
                if state.abnormal_streak < required {
                    return None;
                }
                state.active = Some(state.active.map_or(severity, |s| s.max(severity)));
                if state.announced.is_some_and(|a| severity <= a) {
                    return None;
                }
                let kind = if state.announced.is_some() {
                    EventKind::Escalated
                } else {
                    // Flap protection: a fresh activation inside the cooldown is not re-announced
                    // unless it is more severe than what was announced last time.
                    if let Some((at, previous)) = state.last_raised {
                        if now.saturating_sub(at) < policy.cooldown_seconds && severity <= previous
                        {
                            return None;
                        }
                    }
                    EventKind::Raised
                };
                state.announced = Some(severity);
                state.last_raised = Some((now, severity));
                Some(event(
                    kind,
                    severity,
                    format!("{evidence}（连续 {} 个样本）", state.abnormal_streak),
                    state.abnormal_streak,
                ))
            }
        }
    }
}

/// Compact per-sample summary kept for trends; bounded to `HISTORY_CAPACITY`.
#[derive(Clone, Debug)]
pub struct Sample {
    pub unix_seconds: u64,
    pub memory_used_bytes: Option<u64>,
    pub swap_traffic_bytes_per_second: Option<f64>,
    pub cpu_busy_percent: Option<f64>,
    pub disk_available_bytes: Option<u64>,
    pub cpu_celsius: Option<f64>,
}

impl Sample {
    pub fn from_snapshot(snapshot: &Snapshot) -> Self {
        let activity = &snapshot.activity;
        Self {
            unix_seconds: snapshot.unix_seconds,
            memory_used_bytes: snapshot.memory.breakdown.as_ref().map(|b| b.used_bytes),
            swap_traffic_bytes_per_second: activity
                .swap_in_bytes_per_second
                .zip(activity.swap_out_bytes_per_second)
                .map(|(a, b)| a + b),
            cpu_busy_percent: activity.cpu_busy_percent,
            disk_available_bytes: snapshot.disk.as_ref().map(|d| d.usable_bytes()),
            cpu_celsius: snapshot.thermal.as_ref().and_then(|t| t.cpu_celsius),
        }
    }
}

pub struct History {
    samples: VecDeque<Sample>,
}

pub struct Trend {
    pub samples: usize,
    pub span_seconds: u64,
    pub peak_cpu_percent: Option<f64>,
    pub peak_swap_bytes_per_second: Option<f64>,
    pub peak_memory_used_bytes: Option<u64>,
    pub min_disk_available_bytes: Option<u64>,
}

fn peak(values: impl Iterator<Item = f64>) -> Option<f64> {
    values.fold(None, |acc: Option<f64>, v| {
        Some(acc.map_or(v, |a| a.max(v)))
    })
}

impl History {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }

    pub fn push(&mut self, sample: Sample) {
        if self.samples.len() == HISTORY_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Retained samples, oldest first, each with its own timestamp.
    pub fn samples(&self) -> impl DoubleEndedIterator<Item = &Sample> + ExactSizeIterator + '_ {
        self.samples.iter()
    }

    pub fn trend(&self) -> Trend {
        let s = &self.samples;
        Trend {
            samples: s.len(),
            span_seconds: match (s.front(), s.back()) {
                (Some(a), Some(b)) => b.unix_seconds.saturating_sub(a.unix_seconds),
                _ => 0,
            },
            peak_cpu_percent: peak(s.iter().filter_map(|x| x.cpu_busy_percent)),
            peak_swap_bytes_per_second: peak(
                s.iter().filter_map(|x| x.swap_traffic_bytes_per_second),
            ),
            peak_memory_used_bytes: s.iter().filter_map(|x| x.memory_used_bytes).max(),
            min_disk_available_bytes: s.iter().filter_map(|x| x.disk_available_bytes).min(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(observation: Observation) -> Finding {
        Finding {
            id: "rule",
            observation,
            action: "act",
        }
    }

    fn warn() -> Observation {
        Observation::Abnormal(Severity::Warning, "w".into())
    }

    fn crit() -> Observation {
        Observation::Abnormal(Severity::Critical, "c".into())
    }

    fn run(engine: &mut Engine, steps: &[(u64, Observation)]) -> Vec<(u64, EventKind, Severity)> {
        steps
            .iter()
            .flat_map(|(t, o)| {
                engine
                    .observe(*t, &[finding(o.clone())])
                    .into_iter()
                    .map(move |e| (*t, e.kind, e.severity))
            })
            .collect()
    }

    #[test]
    fn warning_needs_two_samples_and_does_not_repeat() {
        let mut engine = Engine::new(Policy::default());
        let events = run(
            &mut engine,
            &[(0, warn()), (30, warn()), (60, warn()), (90, warn())],
        );
        assert_eq!(events, [(30, EventKind::Raised, Severity::Warning)]);
    }

    #[test]
    fn critical_is_immediate_and_escalation_is_announced() {
        let mut engine = Engine::new(Policy::default());
        assert_eq!(
            run(&mut engine, &[(0, crit())]),
            [(0, EventKind::Raised, Severity::Critical)]
        );
        let mut engine = Engine::new(Policy::default());
        let events = run(
            &mut engine,
            &[
                (0, warn()),
                (30, warn()),
                (60, crit()),
                (90, warn()),
                (120, crit()),
            ],
        );
        assert_eq!(
            events,
            [
                (30, EventKind::Raised, Severity::Warning),
                (60, EventKind::Escalated, Severity::Critical),
            ]
        );
    }

    #[test]
    fn unknown_samples_never_recover_or_raise() {
        let mut engine = Engine::new(Policy::default());
        let events = run(
            &mut engine,
            &[
                (0, warn()),
                (30, Observation::Unknown),
                (60, warn()),
                (90, Observation::Unknown),
                (120, Observation::Unknown),
                (150, Observation::Clear),
                (180, Observation::Unknown),
                (210, Observation::Clear),
            ],
        );
        assert_eq!(
            events,
            [
                (60, EventKind::Raised, Severity::Warning),
                (210, EventKind::Recovered, Severity::Warning),
            ]
        );
    }

    #[test]
    fn cooldown_suppresses_flapping_then_rearms() {
        let mut engine = Engine::new(Policy::default());
        let flap = [
            (0, warn()),
            (30, warn()),
            (60, Observation::Clear),
            (90, Observation::Clear),
            (120, warn()),
            (150, warn()),
            (180, Observation::Clear),
            (210, Observation::Clear),
            (700, warn()),
            (730, warn()),
        ];
        let events = run(&mut engine, &flap);
        assert_eq!(
            events,
            [
                (30, EventKind::Raised, Severity::Warning),
                (90, EventKind::Recovered, Severity::Warning),
                (730, EventKind::Raised, Severity::Warning),
            ]
        );
    }

    #[test]
    fn single_clear_sample_does_not_recover() {
        let mut engine = Engine::new(Policy::default());
        let events = run(
            &mut engine,
            &[(0, crit()), (30, Observation::Clear), (60, crit())],
        );
        assert_eq!(events, [(0, EventKind::Raised, Severity::Critical)]);
    }

    #[test]
    fn status_tracks_episode_evidence_and_recovery_progress() {
        let mut engine = Engine::new(Policy::default());
        run(&mut engine, &[(0, Observation::Clear), (10, warn())]);
        let pending = engine.status("rule").unwrap();
        assert_eq!((pending.active, pending.episode_start), (None, Some(10)));
        run(&mut engine, &[(20, warn()), (30, crit())]);
        let active = engine.status("rule").unwrap();
        assert_eq!(active.active, Some(Severity::Critical));
        assert_eq!(active.episode_start, Some(10));
        assert_eq!(
            active.last_abnormal,
            Some((30, Severity::Critical, "c".into()))
        );
        // Unknown neither advances recovery nor drops the triggering evidence.
        run(
            &mut engine,
            &[(40, Observation::Clear), (50, Observation::Unknown)],
        );
        let recovering = engine.status("rule").unwrap();
        assert_eq!(recovering.clear_streak, 1);
        assert_eq!(recovering.active, Some(Severity::Critical));
        assert_eq!(recovering.last_abnormal.unwrap().0, 30);
        run(&mut engine, &[(60, Observation::Clear)]);
        let recovered = engine.status("rule").unwrap();
        assert_eq!(recovered.active, None);
        assert_eq!(recovered.last_abnormal, None);
        assert_eq!(recovered.episode_start, None);
    }

    #[test]
    fn history_is_bounded() {
        let mut history = History::new();
        for t in 0..(HISTORY_CAPACITY as u64 + 50) {
            history.push(Sample {
                unix_seconds: t,
                memory_used_bytes: Some(t),
                swap_traffic_bytes_per_second: None,
                cpu_busy_percent: Some(t as f64),
                disk_available_bytes: Some(1000 - t),
                cpu_celsius: None,
            });
        }
        assert_eq!(history.len(), HISTORY_CAPACITY);
        let trend = history.trend();
        assert_eq!(trend.span_seconds, HISTORY_CAPACITY as u64 - 1);
        assert_eq!(trend.peak_cpu_percent, Some(169.0));
        assert_eq!(trend.min_disk_available_bytes, Some(1000 - 169));
        assert!(trend.peak_swap_bytes_per_second.is_none());
    }
}
