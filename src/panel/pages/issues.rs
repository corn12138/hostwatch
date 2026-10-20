use super::*;

pub(crate) fn issues(view: &View, blocks: &mut Vec<Block>) {
    if view.issues.is_empty() {
        blocks.push(Block::Text("目前没有需要处理的问题。".into()));
        return;
    }
    let rules: usize = view.issues.iter().map(|i| i.rules.len()).sum();
    blocks.push(Block::Text(format!(
        "共 {} 项问题（涉及 {rules} 条检查规则），按严重程度排列。",
        view.issues.len()
    )));
    for issue in &view.issues {
        let mut body = Vec::new();
        if issue.rules.len() > 1 {
            let names: Vec<&str> = issue.rules.iter().map(|r| rule_name(r.id)).collect();
            body.push(format!("合并提示，包含：{}", names.join("、")));
        }
        for rule in &issue.rules {
            body.push(evidence(view, rule));
        }
        body.push(recovery_line(issue));
        let (tone, _) = issue_status(issue);
        blocks.push(Block::Card {
            tone,
            title: format!("{} · {}", issue_title(issue), severity_word(issue.severity)),
            body,
            buttons: vec![Button::new(
                "查看处理方法",
                Command::Go(Route::Handling(issue.group)),
            )],
        });
    }
}

pub(crate) fn handling(view: &View, group: Group, blocks: &mut Vec<Block>) {
    let Some(issue) = issue_for(view, group) else {
        let recovered = view
            .monitor
            .recovered()
            .is_some_and(|(_, groups)| groups.contains(&group));
        blocks.push(Block::Card {
            tone: Tone::Ok,
            title: if recovered {
                "已恢复正常".into()
            } else {
                "这项提示已解除".into()
            },
            body: vec![format!(
                "Hostwatch 已观察到连续 {} 个正常样本。",
                view.monitor.policy().recovery_samples
            )],
            buttons: vec![Button::new("返回", Command::Back)],
        });
        return;
    };
    if issue.rules.len() > 1 {
        let names: Vec<String> = issue
            .rules
            .iter()
            .map(|r| {
                format!(
                    "{}（{}）",
                    rule_name(r.id),
                    severity_word(r.status.active.unwrap_or(Severity::Warning))
                )
            })
            .collect();
        blocks.push(Block::Note(format!("合并提示，包含：{}", names.join("、"))));
    }
    blocks.push(Block::Heading("为什么提示".into()));
    blocks.push(Block::Text(issue_reason(issue).into()));
    let mut bullets: Vec<String> = issue.rules.iter().map(|r| evidence(view, r)).collect();
    if let Some(line) = duration_line(view, issue) {
        bullets.push(line);
    }
    blocks.push(Block::Bullets(bullets));
    blocks.push(Block::Heading("你可以这样做".into()));
    blocks.push(Block::Steps(
        issue_steps(group).iter().map(|s| s.to_string()).collect(),
    ));
    let tool = group.tool();
    let mut buttons = Vec::new();
    if Focus::for_group(group).is_some() {
        buttons.push(Button::new("一键优化", Command::Optimize(group)).primary());
        buttons.push(Button::new(
            format!("打开{}", tool.label()),
            Command::Open(tool),
        ));
    } else {
        buttons.push(Button::new(format!("打开{}", tool.label()), Command::Open(tool)).primary());
    }
    let detail = match group {
        Group::Memory => ("查看内存详情", Resource::Memory),
        Group::Cpu => ("查看处理器详情", Resource::Cpu),
        Group::Disk => ("查看磁盘详情", Resource::Disk),
        Group::Thermal => ("查看温度详情", Resource::Thermal),
    };
    buttons.push(Button::new(detail.0, Command::Go(Route::Detail(detail.1))));
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, tool, blocks);
    blocks.push(Block::Heading("之后会怎样".into()));
    let required = view.monitor.policy().recovery_samples;
    blocks.push(Block::Text(format!(
        "Hostwatch 会一直按间隔自动采样，不需要点任何按钮；打开工具也不会改变判断。连续 {required} 个正常样本后才确认恢复，读不到的样本不计入。"
    )));
    blocks.push(Block::Progress {
        done: recovery_done(issue),
        total: required,
        label: recovery_line(issue),
    });
    let rules: Vec<String> = issue
        .rules
        .iter()
        .map(|r| format!("{}：{}", rule_name(r.id), rule_rule(r.id)))
        .collect();
    blocks.push(Block::Note(format!(
        "提示条件 — {}。Hostwatch 不会自动清理、删除或结束任何内容，也无法预估能释放多少内存或空间。",
        rules.join("；")
    )));
}
