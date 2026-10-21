use super::*;

pub(crate) fn issues(view: &View, blocks: &mut Vec<Block>) {
    let lang = view.lang;
    if view.issues.is_empty() {
        blocks.push(Block::Text(
            tr!(
                lang,
                "目前没有需要处理的问题。",
                "Nothing needs attention right now."
            )
            .into(),
        ));
        return;
    }
    let count = view.issues.len();
    let rules: usize = view.issues.iter().map(|i| i.rules.len()).sum();
    blocks.push(Block::Text(tr!(
        lang,
        format!("共 {count} 项问题（涉及 {rules} 条检查规则），按严重程度排列。"),
        format!("{count} issue(s) from {rules} check rule(s), most serious first.")
    )));
    for issue in &view.issues {
        let mut body = Vec::new();
        if issue.rules.len() > 1 {
            let names: Vec<&str> = issue.rules.iter().map(|r| rule_name(lang, r.id)).collect();
            body.push(tr!(
                lang,
                format!("合并提示，包含：{}", names.join("、")),
                format!("Combined alert: {}", names.join(", "))
            ));
        }
        for rule in &issue.rules {
            body.push(evidence(view, rule));
        }
        body.push(recovery_line(lang, issue));
        let (tone, _) = issue_status(lang, issue);
        blocks.push(Block::Card {
            tone,
            title: format!(
                "{} · {}",
                issue_title(lang, issue),
                severity_word(lang, issue.severity)
            ),
            body,
            buttons: vec![Button::new(
                tr!(lang, "查看处理方法", "What to do"),
                Command::Go(Route::Handling(issue.group)),
            )],
        });
    }
}

pub(crate) fn handling(view: &View, group: Group, blocks: &mut Vec<Block>) {
    let lang = view.lang;
    let required = view.monitor.policy().recovery_samples;
    let Some(issue) = issue_for(view, group) else {
        let recovered = view
            .monitor
            .recovered()
            .is_some_and(|(_, groups)| groups.contains(&group));
        blocks.push(Block::Card {
            tone: Tone::Ok,
            title: if recovered {
                tr!(lang, "已恢复正常", "Back to normal").into()
            } else {
                tr!(lang, "这项提示已解除", "This alert has cleared").into()
            },
            body: vec![tr!(
                lang,
                format!("Hostwatch 已观察到连续 {required} 个正常样本。"),
                format!("Hostwatch saw {required} normal samples in a row.")
            )],
            buttons: vec![Button::new(tr!(lang, "返回", "Back"), Command::Back)],
        });
        return;
    };
    if issue.rules.len() > 1 {
        let names: Vec<String> = issue
            .rules
            .iter()
            .map(|r| {
                let name = rule_name(lang, r.id);
                let word = severity_word(lang, r.status.active.unwrap_or(Severity::Warning));
                tr!(
                    lang,
                    format!("{name}（{word}）"),
                    format!("{name} ({word})")
                )
            })
            .collect();
        blocks.push(Block::Note(tr!(
            lang,
            format!("合并提示，包含：{}", names.join("、")),
            format!("Combined alert: {}", names.join(", "))
        )));
    }
    blocks.push(Block::Heading(tr!(lang, "为什么提示", "Why").into()));
    blocks.push(Block::Text(issue_reason(lang, issue).into()));
    let mut bullets: Vec<String> = issue.rules.iter().map(|r| evidence(view, r)).collect();
    if let Some(line) = duration_line(view, issue) {
        bullets.push(line);
    }
    blocks.push(Block::Bullets(bullets));
    blocks.push(Block::Heading(
        tr!(lang, "你可以这样做", "What you can do").into(),
    ));
    blocks.push(Block::Steps(
        issue_steps(lang, group)
            .iter()
            .map(|s| s.to_string())
            .collect(),
    ));
    let tool = group.tool();
    let open = Button::new(open_tool_title(lang, tool), Command::Open(tool));
    let mut buttons = Vec::new();
    if Focus::for_group(group).is_some() {
        buttons.push(
            Button::new(tr!(lang, "一键优化", "Optimize"), Command::Optimize(group)).primary(),
        );
        buttons.push(open);
    } else {
        buttons.push(open.primary());
    }
    let (detail, resource) = match group {
        Group::Memory => (
            tr!(lang, "查看内存详情", "Memory details"),
            Resource::Memory,
        ),
        Group::Cpu => (
            tr!(lang, "查看处理器详情", "Processor details"),
            Resource::Cpu,
        ),
        Group::Disk => (tr!(lang, "查看磁盘详情", "Disk details"), Resource::Disk),
        Group::Thermal => (
            tr!(lang, "查看温度详情", "Temperature details"),
            Resource::Thermal,
        ),
    };
    buttons.push(Button::new(detail, Command::Go(Route::Detail(resource))));
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, tool, blocks);
    blocks.push(Block::Heading(
        tr!(lang, "之后会怎样", "What happens next").into(),
    ));
    blocks.push(Block::Text(tr!(
        lang,
        format!("Hostwatch 会一直按间隔自动采样，不需要点任何按钮；打开工具也不会改变判断。连续 {required} 个正常样本后才确认恢复，读不到的样本不计入。"),
        format!("Hostwatch keeps sampling at its interval; you do not need to press anything, and opening a tool does not change the verdict. Recovery is confirmed after {required} normal samples in a row; unreadable samples do not count.")
    )));
    blocks.push(Block::Progress {
        done: recovery_done(issue),
        total: required,
        label: recovery_line(lang, issue),
    });
    let rules: Vec<String> = issue
        .rules
        .iter()
        .map(|r| {
            let (name, rule) = (rule_name(lang, r.id), rule_rule(lang, r.id));
            tr!(lang, format!("{name}：{rule}"), format!("{name}: {rule}"))
        })
        .collect();
    blocks.push(Block::Note(tr!(
        lang,
        format!("提示条件 — {}。Hostwatch 不会自动清理、删除或结束任何内容，也无法预估能释放多少内存或空间。", rules.join("；")),
        format!("Alert rules — {}. Hostwatch never cleans up, deletes or quits anything on its own, and cannot predict how much memory or space will be freed.", rules.join("; "))
    )));
}
