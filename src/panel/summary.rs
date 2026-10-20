//! Status line, resource rows and the issue card, shared by the home page, details and the
//! status item.

use super::*;

pub(crate) fn sampled_at(view: &View) -> String {
    last_sample(view.monitor)
        .map_or_else(String::new, |s| format!("{} 采样", clock(s.unix_seconds)))
}

pub(crate) fn headline(view: &View) -> (Tone, String, String) {
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
            format!("各项检查未见持续异常 · {}", sampled_at(view)),
        ),
    }
}

pub(crate) fn issue_for<'a>(view: &'a View, group: Group) -> Option<&'a Issue> {
    view.issues.iter().find(|i| i.group == group)
}

pub(crate) fn issue_status(issue: &Issue) -> (Tone, &'static str) {
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
        Group::Thermal if issue.severity >= Severity::Critical => "过热",
        Group::Thermal => "偏热",
    };
    (tone, text)
}

pub(crate) fn resource_row(view: &View, resource: Resource) -> Block {
    let monitor = view.monitor;
    let group = match resource {
        Resource::Memory => Group::Memory,
        Resource::Cpu => Group::Cpu,
        Resource::Disk => Group::Disk,
        Resource::Thermal => Group::Thermal,
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
        Resource::Thermal => "温度",
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
                    value = format!("{} 可用", gb(disk.usable_bytes()));
                    let used = 1.0 - disk.usable_bytes() as f64 / disk.total_bytes as f64;
                    graphic = Some(Graphic::Bar {
                        fraction: used,
                        tone: Tone::Neutral,
                        label: format!("已用 {:.0}%", used * 100.0),
                    });
                    (tone, status) = (Tone::Ok, "充足".into());
                }
            }
            Resource::Thermal => {
                if let Some(thermal) = &snapshot.thermal {
                    let state = thermal_label(thermal.state);
                    value = thermal
                        .cpu_celsius
                        .map_or_else(|| state.to_string(), celsius_label);
                    (tone, status) = match thermal.state {
                        Some(ThermalState::Nominal) => (Tone::Ok, "正常".into()),
                        // Fair is the system's normal warm state, not an alert.
                        Some(ThermalState::Fair) => (Tone::Neutral, "偏热".into()),
                        Some(ThermalState::Serious) => (Tone::Warning, "偏高".into()),
                        Some(ThermalState::Critical) => (Tone::Critical, "严重".into()),
                        None => (Tone::Unknown, "未知".into()),
                    };
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

pub(crate) fn issue_card(view: &View, issue: &Issue, more: usize) -> Block {
    let mut body = Vec::new();
    for rule in &issue.rules {
        body.push(evidence(view, rule));
    }
    if matches!(issue.recovery, Recovery::Confirming { .. }) {
        body.push(recovery_line(issue));
    }
    let mut buttons = Vec::new();
    if Focus::for_group(issue.group).is_some() {
        buttons.push(Button::new("一键优化", Command::Optimize(issue.group)).primary());
        buttons.push(Button::new(
            "处理方法",
            Command::Go(Route::Handling(issue.group)),
        ));
    } else {
        buttons
            .push(Button::new("查看处理方法", Command::Go(Route::Handling(issue.group))).primary());
    }
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
