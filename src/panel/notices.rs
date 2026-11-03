//! System notifications for issues the Engine just raised or escalated.
//!
//! Only Engine transitions are announced, so its two-sample rule, ten-minute cooldown and
//! escalation logic also govern notifications; recovery is shown in the panel, not announced.
//! Disk space changes slowly and is visible in Storage settings, so only critical disk space
//! interrupts. Memory, processor and heat notifications also name the open apps using the most,
//! measured the same way as the Optimize page.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub group: Group,
    pub severity: Severity,
    pub lang: Lang,
    pub title: String,
    /// Evidence, then the apps using the most (once known).
    pub lines: Vec<String>,
    pub hint: String,
}

/// Apps named in a notification; the panel's Optimize page lists more.
const NAMED_APPS: usize = 2;

impl Notice {
    pub fn body(&self) -> String {
        let mut lines = self.lines.clone();
        lines.push(self.hint.clone());
        lines.join("\n")
    }

    /// Which resource to rank open apps by; disk space has no app to name.
    pub fn focus(&self) -> Option<Focus> {
        Focus::for_group(self.group)
    }

    /// Names the open apps using the most of the resource behind the issue, as measured by the
    /// Optimize analysis. Nothing is added when no open app stands out.
    pub fn attribute(&mut self, candidates: &[Candidate]) {
        let Some(focus) = self.focus() else {
            return;
        };
        let lang = self.lang;
        let named: Vec<String> = candidates
            .iter()
            .take(NAMED_APPS)
            .map(|c| {
                let amount = match focus {
                    Focus::Memory => size(c.footprint_bytes),
                    Focus::Cpu => tr!(
                        lang,
                        format!("{:.0}% 处理器", c.cpu_percent),
                        format!("{:.0}% CPU", c.cpu_percent)
                    ),
                };
                let name = display_name(lang, &c.name);
                tr!(
                    lang,
                    format!("{name}（{amount}）"),
                    format!("{name} ({amount})")
                )
            })
            .collect();
        if named.is_empty() {
            return;
        }
        self.lines.push(match focus {
            Focus::Memory => tr!(
                lang,
                format!("占用内存最多：{}", named.join("、")),
                format!("Using the most memory: {}", named.join(", "))
            ),
            Focus::Cpu => tr!(
                lang,
                format!("处理器占用最高：{}", named.join("、")),
                format!("Using the most processor: {}", named.join(", "))
            ),
        });
    }
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
            let hint = match issue.group {
                Group::Disk => tr!(lang, "点按查看处理方法。", "Click to see what to do."),
                _ => tr!(
                    lang,
                    "点按查看处理方法，或用“一键优化”退出最占资源的应用。",
                    "Click to see what to do, or use Optimize to quit the heaviest apps."
                ),
            };
            Notice {
                group: issue.group,
                severity: issue.severity,
                lang,
                title: issue_title(lang, issue).into(),
                lines: issue.rules.iter().map(|r| evidence(&view, r)).collect(),
                hint: hint.into(),
            }
        })
        .collect()
}
