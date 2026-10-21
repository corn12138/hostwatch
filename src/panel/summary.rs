//! Status line, resource rows and the issue card, shared by the home page, details and the
//! status item.

use super::*;

pub(crate) fn sampled_at(view: &View) -> String {
    last_sample(view.monitor).map_or_else(String::new, |s| {
        let at = clock(s.unix_seconds);
        tr!(view.lang, format!("{at} 采样"), format!("sampled {at}"))
    })
}

pub(crate) fn headline(view: &View) -> (Tone, String, String) {
    let (lang, monitor) = (view.lang, view.monitor);
    let last = last_sample(monitor).map_or("—".into(), |s| clock(s.unix_seconds));
    match view.health {
        Health::Loading => (
            Tone::Loading,
            tr!(lang, "正在读取系统数据…", "Reading system data…").into(),
            tr!(
                lang,
                "首次采样通常只需片刻",
                "The first sample takes a moment"
            )
            .into(),
        ),
        Health::Paused => {
            let at = monitor.paused_at().map_or("—".into(), clock);
            let mut subtitle = tr!(
                lang,
                format!("{at} 暂停 · 数据停留在 {last}"),
                format!("Paused at {at} · data from {last}")
            );
            if !view.issues.is_empty() {
                let count = view.issues.len();
                subtitle.push_str(&tr!(
                    lang,
                    format!(" · 暂停前有 {count} 项需要注意"),
                    format!(" · {count} issue(s) before pausing")
                ));
            }
            (
                Tone::Paused,
                tr!(lang, "已暂停采集", "Monitoring paused").into(),
                subtitle,
            )
        }
        Health::Failed => (
            Tone::Unknown,
            tr!(lang, "暂时读不到系统数据", "Cannot read system data").into(),
            match last_sample(monitor) {
                Some(s) => {
                    let at = clock(s.unix_seconds);
                    tr!(
                        lang,
                        format!("最后一次成功采样：{at} · 状态未知，不代表正常"),
                        format!("Last good sample: {at} · state unknown, not normal")
                    )
                }
                None => tr!(
                    lang,
                    "还没有成功采样 · 状态未知，不代表正常",
                    "No good sample yet · state unknown, not normal"
                )
                .into(),
            },
        ),
        Health::Stale => {
            let every = interval_label(lang, monitor.interval());
            (
                Tone::Unknown,
                tr!(lang, "数据已过时", "Data is out of date").into(),
                tr!(
                    lang,
                    format!("最后采样 {last}，应每 {every} 更新一次"),
                    format!("Last sample {last}; should update every {every}")
                ),
            )
        }
        Health::Issues(severity) => {
            let top = &view.issues[0];
            if view.issues.len() > 1 {
                let count = view.issues.len();
                let worst = issue_title(lang, top);
                let at = sampled_at(view);
                (
                    Tone::of(severity),
                    tr!(
                        lang,
                        format!("有 {count} 项需要注意"),
                        format!("{count} issues need attention")
                    ),
                    tr!(
                        lang,
                        format!("最严重：{worst} · {at}"),
                        format!("Most serious: {worst} · {at}")
                    ),
                )
            } else {
                (
                    Tone::of(severity),
                    issue_title(lang, top).into(),
                    duration_line(view, top).unwrap_or_else(|| sampled_at(view)),
                )
            }
        }
        Health::Recovering => {
            let done = view.issues.iter().map(recovery_done).min().unwrap_or(0);
            let required = monitor.policy().recovery_samples;
            let at = sampled_at(view);
            (
                Tone::Recovering,
                tr!(lang, "正在确认恢复", "Confirming recovery").into(),
                tr!(
                    lang,
                    format!("已有 {done}/{required} 个正常样本 · {at}"),
                    format!("{done}/{required} normal samples · {at}")
                ),
            )
        }
        Health::Partial => {
            let at = sampled_at(view);
            (
                Tone::Unknown,
                tr!(lang, "部分数据暂时读不到", "Some data is unavailable").into(),
                tr!(
                    lang,
                    format!("其余检查目前无需处理 · {at}"),
                    format!("Other checks need no action · {at}")
                ),
            )
        }
        Health::Warmup => (
            Tone::Loading,
            tr!(lang, "正在准备", "Getting ready").into(),
            tr!(
                lang,
                "内存和磁盘已读取；处理器与交换速率需要再采样一次",
                "Memory and disk are read; processor and swap rates need one more sample"
            )
            .into(),
        ),
        Health::Normal => {
            let at = sampled_at(view);
            (
                Tone::Ok,
                tr!(lang, "目前无需处理", "Nothing needs attention").into(),
                tr!(
                    lang,
                    format!("各项检查未见持续异常 · {at}"),
                    format!("No ongoing problems found · {at}")
                ),
            )
        }
    }
}

pub(crate) fn issue_for<'a>(view: &'a View, group: Group) -> Option<&'a Issue> {
    view.issues.iter().find(|i| i.group == group)
}

pub(crate) fn issue_status(lang: Lang, issue: &Issue) -> (Tone, &'static str) {
    if matches!(issue.recovery, Recovery::Confirming { .. }) {
        return (Tone::Recovering, tr!(lang, "确认恢复中", "Recovering"));
    }
    let tone = Tone::of(issue.severity);
    let has = |id| issue.rules.iter().any(|r| r.id == id);
    let critical = issue.severity >= Severity::Critical;
    let text = match issue.group {
        Group::Memory if has("memory_pressure") && has("swap_activity") => {
            tr!(lang, "压力偏高 · 交换频繁", "Pressure · swapping")
        }
        Group::Memory if has("swap_activity") => tr!(lang, "交换频繁", "Swapping"),
        Group::Memory if critical => tr!(lang, "压力严重", "Critical pressure"),
        Group::Memory => tr!(lang, "压力偏高", "High pressure"),
        Group::Cpu => tr!(lang, "持续很高", "Stays high"),
        Group::Disk if critical => tr!(lang, "空间严重不足", "Critically low"),
        Group::Disk => tr!(lang, "空间不足", "Low"),
        Group::Thermal if critical => tr!(lang, "过热", "Overheating"),
        Group::Thermal => tr!(lang, "偏热", "Hot"),
    };
    (tone, text)
}

pub(crate) fn resource_row(view: &View, resource: Resource) -> Block {
    let (lang, monitor) = (view.lang, view.monitor);
    let unknown = || tr!(lang, "未知", "Unknown").to_string();
    let group = match resource {
        Resource::Memory => Group::Memory,
        Resource::Cpu => Group::Cpu,
        Resource::Disk => Group::Disk,
        Resource::Thermal => Group::Thermal,
    };
    let shown = shown(view);
    let dim = shown.is_some_and(|(_, old)| old);
    let (mut tone, mut status): (Tone, String) = match view.health {
        Health::Loading => (Tone::Loading, tr!(lang, "正在读取", "Reading").into()),
        _ => (Tone::Unknown, unknown()),
    };
    let mut value = "—".to_string();
    let mut graphic = None;
    let name = Route::Detail(resource).title(lang);
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
                let (t, s) = match snapshot.memory.pressure {
                    Pressure::Normal => (Tone::Ok, tr!(lang, "压力正常", "Pressure normal")),
                    Pressure::Warning => (Tone::Warning, tr!(lang, "压力偏高", "High pressure")),
                    Pressure::Critical => {
                        (Tone::Critical, tr!(lang, "压力严重", "Critical pressure"))
                    }
                    Pressure::Unknown => (Tone::Unknown, tr!(lang, "压力未知", "Pressure unknown")),
                };
                (tone, status) = (t, s.into());
            }
            Resource::Cpu => {
                let activity = &snapshot.activity;
                (value, tone, status) = match (activity.status, activity.cpu_busy_percent) {
                    (RateStatus::Ready, Some(cpu)) if cpu >= 90.0 => (
                        percent(cpu),
                        Tone::Neutral,
                        tr!(lang, "很忙，观察中", "Busy, watching").into(),
                    ),
                    (RateStatus::Ready, Some(cpu)) => {
                        (percent(cpu), Tone::Ok, tr!(lang, "正常", "Normal").into())
                    }
                    (RateStatus::Ready, None) | (RateStatus::Unavailable, _) => {
                        ("—".into(), Tone::Unknown, unknown())
                    }
                    _ => (
                        "—".into(),
                        Tone::Loading,
                        tr!(lang, "正在建立基线", "Building baseline").into(),
                    ),
                };
                if monitor.history().len() > 1 {
                    graphic = Some(Graphic::Spark(cpu_chart(lang, monitor)));
                }
            }
            Resource::Disk => {
                if let Some(disk) = &snapshot.disk {
                    let free = gb(disk.usable_bytes());
                    value = tr!(lang, format!("{free} 可用"), format!("{free} free"));
                    let used = 1.0 - disk.usable_bytes() as f64 / disk.total_bytes as f64;
                    let share = used * 100.0;
                    graphic = Some(Graphic::Bar {
                        fraction: used,
                        tone: Tone::Neutral,
                        label: tr!(
                            lang,
                            format!("已用 {share:.0}%"),
                            format!("{share:.0}% used")
                        ),
                    });
                    (tone, status) = (Tone::Ok, tr!(lang, "充足", "Plenty").into());
                }
            }
            Resource::Thermal => {
                if let Some(thermal) = &snapshot.thermal {
                    let state = thermal_label(lang, thermal.state);
                    value = thermal
                        .cpu_celsius
                        .map_or_else(|| state.to_string(), celsius_label);
                    tone = match thermal.state {
                        Some(ThermalState::Nominal) => Tone::Ok,
                        // Fair is the system's normal warm state, not an alert.
                        Some(ThermalState::Fair) => Tone::Neutral,
                        Some(ThermalState::Serious) => Tone::Warning,
                        Some(ThermalState::Critical) => Tone::Critical,
                        None => Tone::Unknown,
                    };
                    status = state.into();
                }
            }
        }
        if let Some(issue) = issue_for(view, group) {
            let (t, s) = issue_status(lang, issue);
            (tone, status) = (t, s.into());
        }
        match view.health {
            Health::Paused => (tone, status) = (Tone::Paused, tr!(lang, "已暂停", "Paused").into()),
            Health::Stale => {
                (tone, status) = (Tone::Unknown, tr!(lang, "已过时", "Out of date").into())
            }
            _ => {}
        }
    } else if let Some(issue) = issue_for(view, group) {
        // A failed attempt keeps the issue: show it, not a reassuring "unknown" alone.
        let (t, s) = issue_status(lang, issue);
        (tone, status) = (
            t,
            tr!(lang, format!("{s}（此前）"), format!("{s} (earlier)")),
        );
    }
    if let Some(Graphic::Bar { tone: bar, .. }) = &mut graphic {
        *bar = if matches!(tone, Tone::Warning | Tone::Critical) {
            tone
        } else {
            Tone::Neutral
        };
    }
    let mut label = tr!(
        lang,
        format!("{name}，{value}，{status}"),
        format!("{name}, {value}, {status}")
    );
    if let Some(Graphic::Spark(chart)) = &graphic {
        label.push_str(&tr!(
            lang,
            format!("。{}", chart.summary),
            format!(". {}", chart.summary)
        ));
    }
    label.push_str(tr!(lang, "。查看详情", ". Show details"));
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
    let lang = view.lang;
    let mut body = Vec::new();
    for rule in &issue.rules {
        body.push(evidence(view, rule));
    }
    if matches!(issue.recovery, Recovery::Confirming { .. }) {
        body.push(recovery_line(lang, issue));
    }
    let handling = Command::Go(Route::Handling(issue.group));
    let mut buttons = Vec::new();
    if Focus::for_group(issue.group).is_some() {
        buttons.push(
            Button::new(
                tr!(lang, "一键优化", "Optimize"),
                Command::Optimize(issue.group),
            )
            .primary(),
        );
        buttons.push(Button::new(tr!(lang, "处理方法", "What to do"), handling));
    } else {
        buttons.push(Button::new(tr!(lang, "查看处理方法", "What to do"), handling).primary());
    }
    if more > 0 {
        buttons.push(Button::new(
            tr!(lang, format!("另有 {more} 项 ›"), format!("{more} more ›")),
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
        title: issue_title(lang, issue).into(),
        body,
        buttons,
    }
}
