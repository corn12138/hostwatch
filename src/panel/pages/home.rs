use super::*;

pub(crate) fn home(view: &View, blocks: &mut Vec<Block>) {
    let (lang, monitor) = (view.lang, view.monitor);
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
                    Group::Memory => tr!(lang, "内存", "Memory"),
                    Group::Cpu => tr!(lang, "处理器", "Processor"),
                    Group::Disk => tr!(lang, "磁盘空间", "Disk space"),
                    Group::Thermal => tr!(lang, "散热", "Cooling"),
                })
                .collect();
            let required = monitor.policy().recovery_samples;
            let until = clock(at);
            blocks.push(Block::Card {
                tone: Tone::Ok,
                title: tr!(
                    lang,
                    format!("{}已恢复正常", names.join("、")),
                    format!("{} back to normal", names.join(", "))
                ),
                body: vec![tr!(
                    lang,
                    format!("连续 {required} 个正常样本（截至 {until}）"),
                    format!("{required} normal samples in a row (as of {until})")
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
    let retry = || Button::new(tr!(lang, "重新读取", "Read again"), Command::Retry).primary();
    match view.health {
        Health::Paused | Health::Loading | Health::Normal | Health::Warmup => {}
        Health::Issues(_) | Health::Recovering => {
            blocks.push(issue_card(view, &view.issues[0], view.issues.len() - 1));
        }
        Health::Failed => {
            let (_, error, count) = monitor.failure().unwrap_or((0, "", 0));
            let mut body = vec![
                tr!(
                    lang,
                    format!("读取系统数据时出错（连续 {count} 次）：{error}"),
                    format!("Reading system data failed ({count} in a row): {error}")
                ),
                tr!(
                    lang,
                    "在恢复之前，各项显示为“未知”，不代表正常。Hostwatch 会按间隔自动重试。",
                    "Until it recovers, every item shows “Unknown”, which does not mean normal. Hostwatch retries automatically at each interval."
                )
                .into(),
            ];
            let mut buttons = vec![retry()];
            if let Some(top) = view.issues.first() {
                let (title, word) = (issue_title(lang, top), severity_word(lang, top.severity));
                body.push(tr!(
                    lang,
                    format!("此前的提示仍保留：{title}（{word}）"),
                    format!("The earlier alert still stands: {title} ({word})")
                ));
                buttons.push(Button::new(
                    tr!(lang, "查看处理方法", "What to do"),
                    Command::Go(Route::Handling(top.group)),
                ));
            }
            blocks.push(Block::Card {
                tone: Tone::Unknown,
                title: tr!(lang, "可以重新读取一次", "You can read again").into(),
                body,
                buttons,
            });
        }
        Health::Stale => blocks.push(Block::Card {
            tone: Tone::Unknown,
            title: tr!(lang, "数据没有按时更新", "Data did not update on time").into(),
            body: vec![tr!(
                lang,
                "可能是电脑刚从睡眠中醒来，或采集暂时卡住。旧数据已变淡，不代表现在。",
                "The Mac may have just woken from sleep, or sampling stalled. Old values are dimmed and do not describe now."
            )
            .into()],
            buttons: vec![retry()],
        }),
        Health::Partial => {
            let missing: Vec<&str> = monitor
                .current()
                .map(unmeasured)
                .unwrap_or_default()
                .into_iter()
                .map(|id| rule_name(lang, id))
                .collect();
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
            let title = match (missing.is_empty(), lang) {
                (true, Lang::Zh) => "部分数据暂时读不到".to_string(),
                (true, Lang::En) => "Some data is unavailable".to_string(),
                (false, Lang::Zh) => format!("{}暂时读不到", missing.join("、")),
                (false, Lang::En) => format!("{} is unavailable", missing.join(", ")),
            };
            let mut body = vec![tr!(
                lang,
                "因此无法判断这一项，显示为“未知”，不代表正常。其他检查目前无需处理。",
                "So it cannot be judged and shows “Unknown”, which does not mean normal. Other checks need no action."
            )
            .to_string()];
            if !failed.is_empty() {
                body.push(tr!(
                    lang,
                    format!("不可用的数据源：{}", failed.join("、")),
                    format!("Unavailable sources: {}", failed.join(", "))
                ));
            }
            blocks.push(Block::Card {
                tone: Tone::Unknown,
                title,
                body,
                buttons: Vec::new(),
            });
        }
    }
}
