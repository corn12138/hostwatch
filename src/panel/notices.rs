//! System notifications for issues the Engine just raised or escalated.
//!
//! Only Engine transitions are announced, so its two-sample rule, ten-minute cooldown and
//! escalation logic also govern notifications; recovery is shown in the panel, not announced.
//! Disk space changes slowly and is visible in Storage settings, so only critical disk space
//! interrupts.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub group: Group,
    pub severity: Severity,
    pub title: String,
    pub body: String,
}

/// Notifications for the newest sample; empty when the person turned them off.
pub fn notices(monitor: &Monitor, now: u64, ui: Ui) -> Vec<Notice> {
    if !monitor.integrations().notifications || monitor.raised().is_empty() {
        return Vec::new();
    }
    let view = View {
        monitor,
        health: monitor.health(now),
        issues: monitor.issues(),
        lang: ui.lang,
        ui,
    };
    let lang = view.lang;
    view.issues
        .iter()
        .filter(|issue| monitor.raised().contains(&issue.group))
        .filter(|issue| issue.group != Group::Disk || issue.severity >= Severity::Critical)
        .map(|issue| {
            let mut lines: Vec<String> = issue.rules.iter().map(|r| evidence(&view, r)).collect();
            lines.push(
                match issue.group {
                    Group::Disk => tr!(lang, "点按查看处理方法。", "Click to see what to do."),
                    _ => tr!(
                        lang,
                        "点按查看处理方法，或用“一键优化”退出最占资源的应用。",
                        "Click to see what to do, or use Optimize to quit the heaviest apps."
                    ),
                }
                .into(),
            );
            Notice {
                group: issue.group,
                severity: issue.severity,
                title: issue_title(lang, issue).into(),
                body: lines.join("\n"),
            }
        })
        .collect()
}
