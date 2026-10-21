use super::*;

pub(crate) fn optimize(view: &View, group: Group, blocks: &mut Vec<Block>) {
    let lang = view.lang;
    let Some(focus) = Focus::for_group(group) else {
        blocks.push(Block::Text(
            tr!(
                lang,
                "磁盘空间需要你确认哪些文件可以删除，Hostwatch 不会自动删除任何文件。",
                "Only you can decide which files to delete; Hostwatch never deletes files."
            )
            .into(),
        ));
        blocks.push(Block::Buttons(vec![Button::new(
            open_tool_title(lang, Tool::StorageSettings),
            Command::Open(Tool::StorageSettings),
        )]));
        return;
    };
    blocks.push(Block::Text(
        match (focus, lang) {
            (Focus::Memory, Lang::Zh) => "下面是占用内存最多的已打开应用（含它们启动的辅助进程）。退出不用的应用能直接让出内存。",
            (Focus::Memory, Lang::En) => "These open apps use the most memory (including helper processes they started). Quitting ones you do not need frees memory directly.",
            (Focus::Cpu, Lang::Zh) => "下面是最近 1 秒处理器占用最高的已打开应用（含它们启动的辅助进程）。退出不需要的应用能降低负载和温度。",
            (Focus::Cpu, Lang::En) => "These open apps used the most processor time in the last second (including helper processes they started). Quitting ones you do not need lowers load and heat.",
        }
        .into(),
    ));
    let state = view
        .monitor
        .optimizer()
        .state()
        .filter(|s| s.group() == group);
    let again = || {
        Button::new(
            tr!(lang, "重新分析", "Analyze again"),
            Command::Optimize(group),
        )
    };
    match state {
        None | Some(Optimization::Analyzing { .. }) => {
            blocks.push(Block::Note(
                tr!(
                    lang,
                    "正在分析已打开的应用…（约 1 秒）",
                    "Analyzing open apps… (about 1 second)"
                )
                .into(),
            ));
        }
        Some(Optimization::Failed { error, .. }) => blocks.push(Block::Card {
            tone: Tone::Critical,
            title: tr!(lang, "分析失败", "Analysis failed").into(),
            body: vec![error.clone()],
            buttons: vec![again()],
        }),
        Some(Optimization::Ready { at, candidates, .. }) => {
            let at = clock(*at);
            blocks.push(Block::Note(tr!(
                lang,
                format!("分析于 {at}"),
                format!("Analyzed at {at}")
            )));
            if candidates.is_empty() {
                blocks.push(Block::Text(
                    tr!(
                        lang,
                        "没有发现明显占用的已打开应用。负载可能来自系统进程或后台服务，可以在活动监视器中查看。",
                        "No open app stands out. The load may come from system processes or background services; check Activity Monitor."
                    )
                    .into(),
                ));
                blocks.push(Block::Buttons(vec![
                    again(),
                    Button::new(
                        open_tool_title(lang, Tool::ActivityMonitor),
                        Command::Open(Tool::ActivityMonitor),
                    ),
                ]));
            } else {
                blocks.push(Block::Checks(
                    candidates
                        .iter()
                        .map(|c| {
                            let amount = match focus {
                                Focus::Memory => size(c.footprint_bytes),
                                Focus::Cpu => tr!(
                                    lang,
                                    format!("{:.0}% 处理器", c.cpu_percent),
                                    format!("{:.0}% CPU", c.cpu_percent)
                                ),
                            };
                            let processes = c.processes;
                            let mut detail = tr!(
                                lang,
                                format!("{amount} · {processes} 个进程"),
                                format!("{amount} · {processes} process(es)")
                            );
                            if c.active {
                                detail.push_str(tr!(lang, " · 正在使用", " · in use"));
                            }
                            Check {
                                title: display_name(lang, &c.name),
                                detail,
                                checked: c.selected,
                                command: Command::ToggleApp(c.pid),
                            }
                        })
                        .collect(),
                ));
                let count = candidates.iter().filter(|c| c.selected).count();
                let title = match (count, lang) {
                    (0, Lang::Zh) => "请先勾选要退出的应用".to_string(),
                    (0, Lang::En) => "Select apps to quit".to_string(),
                    (n, Lang::Zh) => format!("退出所选的 {n} 个应用"),
                    (n, Lang::En) => format!("Quit {n} selected app(s)"),
                };
                let mut quit = Button::new(title, Command::QuitSelected).primary();
                quit.enabled = count > 0;
                blocks.push(Block::Buttons(vec![quit, again()]));
            }
        }
        Some(Optimization::Applied { at, outcomes, .. }) => {
            let body = outcomes
                .iter()
                .map(|o| {
                    let state = match o.state {
                        QuitState::Requested => {
                            tr!(lang, "已请求退出，正在确认…", "asked to quit, checking…")
                        }
                        QuitState::Quit => tr!(lang, "已退出", "quit"),
                        QuitState::StillRunning => tr!(
                            lang,
                            "仍在运行（可能在等你保存或确认）",
                            "still running (may be waiting for you to save or confirm)"
                        ),
                        QuitState::NotSent => tr!(
                            lang,
                            "未能发送退出请求（应用已关闭或拒绝）",
                            "quit request not delivered (app gone or refused)"
                        ),
                    };
                    let name = display_name(lang, &o.name);
                    tr!(lang, format!("{name}：{state}"), format!("{name}: {state}"))
                })
                .collect();
            let (at, count) = (clock(*at), outcomes.len());
            blocks.push(Block::Card {
                tone: Tone::Neutral,
                title: tr!(
                    lang,
                    format!("{at} 已请求 {count} 个应用退出"),
                    format!("Asked {count} app(s) to quit at {at}")
                ),
                body,
                buttons: vec![again()],
            });
            let required = view.monitor.policy().recovery_samples;
            blocks.push(Block::Text(tr!(
                lang,
                format!("是否恢复由之后的样本判断：连续 {required} 个正常样本后才会显示已恢复。"),
                format!("Later samples decide recovery: it shows as recovered after {required} normal samples in a row.")
            )));
        }
    }
    blocks.push(Block::Note(
        tr!(
            lang,
            "Hostwatch 会像按 ⌘Q 一样请应用正常退出：有未保存内容时应用会先询问你，可以取消。退出应用也会结束它启动的子进程（例如终端里运行的任务），请确认它们可以停止。不会强制结束进程、不会删除文件、不会修改系统设置；访达和系统组件不在列表中。",
            "Hostwatch asks apps to quit normally, like ⌘Q: an app with unsaved work asks you first, and you can cancel. Quitting an app also ends processes it started (such as tasks running in Terminal), so make sure they can stop. Nothing is force-quit, no files are deleted and no settings change; Finder and system components are never listed."
        )
        .into(),
    ));
}
