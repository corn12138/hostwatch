use super::*;

pub(crate) fn home(view: &View, blocks: &mut Vec<Block>) {
    let monitor = view.monitor;
    let (tone, title, subtitle) = headline(view);
    blocks.push(Block::Conclusion {
        tone,
        title,
        subtitle,
    });
    if let Some((at, groups)) = monitor.recovered() {
        if monitor.current().is_some() {
            let names: Vec<&str> = groups
                .iter()
                .map(|g| match g {
                    Group::Memory => "内存",
                    Group::Cpu => "处理器",
                    Group::Disk => "磁盘空间",
                    Group::Thermal => "散热",
                })
                .collect();
            blocks.push(Block::Card {
                tone: Tone::Ok,
                title: format!("{}已恢复正常", names.join("、")),
                body: vec![format!(
                    "连续 {} 个正常样本（截至 {}）",
                    monitor.policy().recovery_samples,
                    clock(at)
                )],
                buttons: Vec::new(),
            });
        }
    }
    for resource in [
        Resource::Memory,
        Resource::Cpu,
        Resource::Disk,
        Resource::Thermal,
    ] {
        blocks.push(resource_row(view, resource));
    }
    match view.health {
        Health::Paused | Health::Loading | Health::Normal | Health::Warmup => {}
        Health::Issues(_) | Health::Recovering => {
            blocks.push(issue_card(view, &view.issues[0], view.issues.len() - 1));
        }
        Health::Failed => {
            let (_, error, count) = monitor.failure().unwrap_or((0, "", 0));
            let mut body = vec![
                format!("读取系统数据时出错（连续 {count} 次）：{error}"),
                "在恢复之前，各项显示为“未知”，不代表正常。Hostwatch 会按间隔自动重试。".into(),
            ];
            let mut buttons = vec![Button::new("重新读取", Command::Retry).primary()];
            if let Some(top) = view.issues.first() {
                body.push(format!(
                    "此前的提示仍保留：{}（{}）",
                    issue_title(top),
                    severity_word(top.severity)
                ));
                buttons.push(Button::new(
                    "查看处理方法",
                    Command::Go(Route::Handling(top.group)),
                ));
            }
            blocks.push(Block::Card {
                tone: Tone::Unknown,
                title: "可以重新读取一次".into(),
                body,
                buttons,
            });
        }
        Health::Stale => blocks.push(Block::Card {
            tone: Tone::Unknown,
            title: "数据没有按时更新".into(),
            body: vec![
                "可能是电脑刚从睡眠中醒来，或采集暂时卡住。旧数据已变淡，不代表现在。".into(),
            ],
            buttons: vec![Button::new("重新读取", Command::Retry).primary()],
        }),
        Health::Partial => {
            let missing = monitor.current().map(unmeasured).unwrap_or_default();
            let failed: Vec<&str> = monitor
                .current()
                .map(|l| {
                    l.snapshot
                        .sources
                        .iter()
                        .filter(|s| !s.ok)
                        .map(|s| s.source)
                        .collect()
                })
                .unwrap_or_default();
            let what = if missing.is_empty() {
                "部分数据".to_string()
            } else {
                missing.join("、")
            };
            let mut body = vec![format!(
                "因此无法判断这一项，显示为“未知”，不代表正常。其他检查目前无需处理。"
            )];
            if !failed.is_empty() {
                body.push(format!("不可用的数据源：{}", failed.join("、")));
            }
            blocks.push(Block::Card {
                tone: Tone::Unknown,
                title: format!("{what}暂时读不到"),
                body,
                buttons: Vec::new(),
            });
        }
    }
}
