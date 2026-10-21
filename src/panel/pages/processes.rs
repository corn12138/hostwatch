use super::*;

pub(crate) fn processes(view: &View, blocks: &mut Vec<Block>) {
    let lang = view.lang;
    blocks.push(Block::Text(
        tr!(
            lang,
            "按内存占用列出可读取的前 5 个进程（与活动监视器“内存”列同一口径），只在你点“读取”时扫描。占用大不代表它让电脑变慢或有问题。",
            "The 5 readable processes using the most memory (same figure as Activity Monitor's Memory column), scanned only when you press Read. Using a lot does not mean a process slows the Mac or has a problem."
        )
        .into(),
    ));
    let (loading, result) = match view.monitor.processes() {
        Processes::NotRead => (false, None),
        Processes::Loading { previous, .. } => (true, previous.as_ref()),
        Processes::Done(result) => (false, Some(result)),
    };
    let title = match (loading, result.is_some()) {
        (true, _) => tr!(lang, "读取中…", "Reading…"),
        (false, true) => tr!(lang, "刷新", "Refresh"),
        (false, false) => tr!(lang, "读取", "Read"),
    };
    let mut button = Button::new(title, Command::RefreshProcesses).primary();
    button.enabled = !loading;
    blocks.push(Block::Buttons(vec![button]));
    match (loading, result) {
        (true, _) => blocks.push(Block::Note(
            tr!(lang, "正在读取进程信息…", "Reading processes…").into(),
        )),
        (false, None) => blocks.push(Block::Note(tr!(lang, "尚未读取。", "Not read yet.").into())),
        _ => {}
    }
    if let Some(result) = result {
        match &result.outcome {
            Ok(scan) => {
                let at = clock(result.at);
                blocks.push(Block::Note(match (loading, lang) {
                    (true, Lang::Zh) => format!("上次读取于 {at}"),
                    (false, Lang::Zh) => format!("读取于 {at}"),
                    (true, Lang::En) => format!("Last read at {at}"),
                    (false, Lang::En) => format!("Read at {at}"),
                }));
                let memory =
                    |p: &crate::model::Process| p.footprint_bytes.unwrap_or(p.resident_bytes);
                let largest = scan.top.first().map_or(1, |p| memory(p).max(1));
                if scan.top.is_empty() {
                    blocks.push(Block::Text(
                        tr!(
                            lang,
                            "没有读到任何进程的内存数据。",
                            "No process memory could be read."
                        )
                        .into(),
                    ));
                } else {
                    blocks.push(Block::Processes(
                        scan.top
                            .iter()
                            .enumerate()
                            .map(|(i, p)| ProcessRow {
                                rank: i + 1,
                                name: display_name(lang, &p.name),
                                size: size(memory(p)),
                                fraction: memory(p) as f64 / largest as f64,
                            })
                            .collect(),
                    ));
                }
                if scan.unreadable > 0 {
                    let (unreadable, listed) = (scan.unreadable, scan.listed);
                    blocks.push(Block::Note(tr!(
                        lang,
                        format!("另有 {unreadable} 个进程（共列出 {listed} 个）读不到内存数据，通常属于系统或其他用户，未列入排行。"),
                        format!("{unreadable} of {listed} processes could not be read (usually system or other users' processes) and are not ranked.")
                    )));
                }
            }
            Err(error) => blocks.push(Block::Card {
                tone: Tone::Critical,
                title: match (loading, lang) {
                    (true, Lang::Zh) => "上次读取失败",
                    (false, Lang::Zh) => "读取失败",
                    (true, Lang::En) => "The last read failed",
                    (false, Lang::En) => "Read failed",
                }
                .into(),
                body: vec![
                    error.clone(),
                    tr!(
                        lang,
                        "可以稍后重试，或打开活动监视器查看。",
                        "Try again later, or check Activity Monitor."
                    )
                    .into(),
                ],
                buttons: Vec::new(),
            }),
        }
    }
    blocks.push(Block::Note(
        tr!(
            lang,
            "只显示单个进程，不合并为应用；共享内存会在多个进程中各算一次，数值不能直接相加。",
            "Single processes only, not grouped by app; shared memory counts in each process, so the numbers cannot be added up."
        )
        .into(),
    ));
    blocks.push(Block::Buttons(vec![Button::new(
        open_tool_title(lang, Tool::ActivityMonitor),
        Command::Open(Tool::ActivityMonitor),
    )]));
    tool_feedback(view, Tool::ActivityMonitor, blocks);
}
