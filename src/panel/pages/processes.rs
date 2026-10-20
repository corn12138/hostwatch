use super::*;

pub(crate) fn processes(view: &View, blocks: &mut Vec<Block>) {
    blocks.push(Block::Text(
        "按常驻内存（RSS）列出可读取的前 5 个进程，只在你点“读取”时扫描。占用大不代表它让电脑变慢或有问题。"
            .into(),
    ));
    let (loading, result) = match view.monitor.processes() {
        Processes::NotRead => (false, None),
        Processes::Loading { previous, .. } => (true, previous.as_ref()),
        Processes::Done(result) => (false, Some(result)),
    };
    let mut button = Button::new(
        if loading {
            "读取中…"
        } else if result.is_some() {
            "刷新"
        } else {
            "读取"
        },
        Command::RefreshProcesses,
    )
    .primary();
    button.enabled = !loading;
    blocks.push(Block::Buttons(vec![button]));
    match (loading, result) {
        (true, _) => blocks.push(Block::Note("正在读取进程信息…".into())),
        (false, None) => blocks.push(Block::Note("尚未读取。".into())),
        _ => {}
    }
    if let Some(result) = result {
        let prefix = if loading { "上次" } else { "" };
        match &result.outcome {
            Ok(scan) => {
                blocks.push(Block::Note(format!("{prefix}读取于 {}", clock(result.at))));
                let largest = scan.top.first().map_or(1, |p| p.resident_bytes.max(1));
                if scan.top.is_empty() {
                    blocks.push(Block::Text("没有读到任何进程的内存数据。".into()));
                } else {
                    blocks.push(Block::Processes(
                        scan.top
                            .iter()
                            .enumerate()
                            .map(|(i, p)| ProcessRow {
                                rank: i + 1,
                                name: display_name(&p.name),
                                size: size(p.resident_bytes),
                                fraction: p.resident_bytes as f64 / largest as f64,
                            })
                            .collect(),
                    ));
                }
                if scan.unreadable > 0 {
                    blocks.push(Block::Note(format!(
                        "另有 {} 个进程（共列出 {} 个）读不到内存数据，通常属于系统或其他用户，未列入排行。",
                        scan.unreadable, scan.listed
                    )));
                }
            }
            Err(error) => blocks.push(Block::Card {
                tone: Tone::Critical,
                title: format!("{prefix}读取失败"),
                body: vec![error.clone(), "可以稍后重试，或打开活动监视器查看。".into()],
                buttons: Vec::new(),
            }),
        }
    }
    blocks.push(Block::Note(
        "只显示单个进程，不合并为应用；RSS 不等于活动监视器的“内存”列，多个进程的数值不能直接相加。"
            .into(),
    ));
    blocks.push(Block::Buttons(vec![Button::new(
        "打开活动监视器",
        Command::Open(Tool::ActivityMonitor),
    )]));
    tool_feedback(view, Tool::ActivityMonitor, blocks);
}
