use super::*;

pub(crate) fn optimize(view: &View, group: Group, blocks: &mut Vec<Block>) {
    let Some(focus) = Focus::for_group(group) else {
        blocks.push(Block::Text(
            "磁盘空间需要你确认哪些文件可以删除，Hostwatch 不会自动删除任何文件。".into(),
        ));
        blocks.push(Block::Buttons(vec![Button::new(
            "打开“储存空间”设置",
            Command::Open(Tool::StorageSettings),
        )]));
        return;
    };
    blocks.push(Block::Text(
        match focus {
            Focus::Memory => "下面是占用内存最多的已打开应用（含它们启动的辅助进程）。退出不用的应用能直接让出内存。",
            Focus::Cpu => "下面是最近 1 秒处理器占用最高的已打开应用（含它们启动的辅助进程）。退出不需要的应用能降低负载和温度。",
        }
        .into(),
    ));
    let state = view
        .monitor
        .optimizer()
        .state()
        .filter(|s| s.group() == group);
    let again = || Button::new("重新分析", Command::Optimize(group));
    match state {
        None | Some(Optimization::Analyzing { .. }) => {
            blocks.push(Block::Note("正在分析已打开的应用…（约 1 秒）".into()));
        }
        Some(Optimization::Failed { error, .. }) => blocks.push(Block::Card {
            tone: Tone::Critical,
            title: "分析失败".into(),
            body: vec![error.clone()],
            buttons: vec![again()],
        }),
        Some(Optimization::Ready { at, candidates, .. }) => {
            blocks.push(Block::Note(format!("分析于 {}", clock(*at))));
            if candidates.is_empty() {
                blocks.push(Block::Text(
                    "没有发现明显占用的已打开应用。负载可能来自系统进程或后台服务，可以在活动监视器中查看。"
                        .into(),
                ));
                blocks.push(Block::Buttons(vec![
                    again(),
                    Button::new("打开活动监视器", Command::Open(Tool::ActivityMonitor)),
                ]));
            } else {
                blocks.push(Block::Checks(
                    candidates
                        .iter()
                        .map(|c| {
                            let amount = match focus {
                                Focus::Memory => size(c.footprint_bytes),
                                Focus::Cpu => format!("{:.0}% 处理器", c.cpu_percent),
                            };
                            let mut detail = format!("{amount} · {} 个进程", c.processes);
                            if c.active {
                                detail.push_str(" · 正在使用");
                            }
                            Check {
                                title: display_name(&c.name),
                                detail,
                                checked: c.selected,
                                command: Command::ToggleApp(c.pid),
                            }
                        })
                        .collect(),
                ));
                let count = candidates.iter().filter(|c| c.selected).count();
                let mut quit = Button::new(
                    if count == 0 {
                        "请先勾选要退出的应用".to_string()
                    } else {
                        format!("退出所选的 {count} 个应用")
                    },
                    Command::QuitSelected,
                )
                .primary();
                quit.enabled = count > 0;
                blocks.push(Block::Buttons(vec![quit, again()]));
            }
        }
        Some(Optimization::Applied { at, outcomes, .. }) => {
            let body = outcomes
                .iter()
                .map(|o| {
                    let state = match o.state {
                        QuitState::Requested => "已请求退出，正在确认…",
                        QuitState::Quit => "已退出",
                        QuitState::StillRunning => "仍在运行（可能在等你保存或确认）",
                        QuitState::NotSent => "未能发送退出请求（应用已关闭或拒绝）",
                    };
                    format!("{}：{state}", display_name(&o.name))
                })
                .collect();
            blocks.push(Block::Card {
                tone: Tone::Neutral,
                title: format!("{} 已请求 {} 个应用退出", clock(*at), outcomes.len()),
                body,
                buttons: vec![again()],
            });
            blocks.push(Block::Text(format!(
                "是否恢复由之后的样本判断：连续 {} 个正常样本后才会显示已恢复。",
                view.monitor.policy().recovery_samples
            )));
        }
    }
    blocks.push(Block::Note(
        "Hostwatch 会像按 ⌘Q 一样请应用正常退出：有未保存内容时应用会先询问你，可以取消。退出应用也会结束它启动的子进程（例如终端里运行的任务），请确认它们可以停止。不会强制结束进程、不会删除文件、不会修改系统设置；访达和系统组件不在列表中。"
            .into(),
    ));
}
