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
    assert_eq!(unmeasured(monitor.current().unwrap()), ["memory_pressure"]);
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
            footprint_bytes: None,
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
