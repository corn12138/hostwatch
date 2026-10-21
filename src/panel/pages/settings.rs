use super::*;

pub(crate) fn settings(view: &View, blocks: &mut Vec<Block>) {
    let (lang, monitor) = (view.lang, view.monitor);
    blocks.push(Block::Heading(
        tr!(lang, "采样间隔", "Sampling interval").into(),
    ));
    let mut presets = INTERVAL_PRESETS.to_vec();
    let custom = !presets.contains(&monitor.interval());
    if custom {
        presets.push(monitor.interval());
        presets.sort_unstable();
    }
    blocks.push(Block::Choices(
        presets
            .into_iter()
            .map(|seconds| {
                let mut title = interval_label(lang, seconds);
                if custom && seconds == monitor.interval() {
                    title.push_str(tr!(lang, "（启动参数）", " (launch option)"));
                }
                let mut button = Button::new(title, Command::SetInterval(seconds));
                button.primary = seconds == monitor.interval();
                button
            })
            .collect(),
    ));
    blocks.push(Block::Note(
        if monitor.paused() {
            tr!(
                lang,
                "暂停中：继续采集后按所选间隔采样。",
                "Paused: sampling uses this interval after you resume."
            )
        } else {
            tr!(
                lang,
                "间隔越短越及时，开销也略高。修改后下一个样本按新间隔采集。",
                "Shorter intervals react faster and cost slightly more. The next sample uses the new interval."
            )
        }
        .into(),
    ));
    blocks.push(Block::Heading(tr!(lang, "采集", "Monitoring").into()));
    blocks.push(Block::Buttons(vec![Button::new(
        match (monitor.paused(), lang) {
            (true, Lang::Zh) => "继续采集",
            (false, Lang::Zh) => "暂停采集",
            (true, Lang::En) => "Resume",
            (false, Lang::En) => "Pause",
        },
        Command::TogglePause,
    )]));
    // Each option is written in its own language so it stays recognizable either way.
    blocks.push(Block::Heading(
        tr!(lang, "语言 · Language", "Language · 语言").into(),
    ));
    let ui = view.ui;
    blocks.push(Block::Choices(
        LanguageChoice::ALL
            .into_iter()
            .map(|choice| {
                let title = match choice {
                    LanguageChoice::System => tr!(
                        lang,
                        format!("跟随系统（{}）", tr!(ui.system, "中文", "English")),
                        format!("Follow system ({})", tr!(ui.system, "中文", "English"))
                    ),
                    LanguageChoice::Zh => "中文".to_string(),
                    LanguageChoice::En => "English".to_string(),
                };
                let mut button = Button::new(title, Command::SetLanguage(choice));
                button.primary = choice == ui.choice;
                button
            })
            .collect(),
    ));
    blocks.push(Block::Note(
        tr!(
            lang,
            "跟随系统时，按 macOS 首选语言中第一个中文或英文显示，其他语言显示英文。",
            "Follow system uses the first Chinese or English entry in macOS's preferred languages; anything else shows English."
        )
        .into(),
    ));
    blocks.push(Block::Heading(
        tr!(lang, "数据说明", "About the data").into(),
    ));
    let pairs: [(&str, &str, &str, &str); 8] = [
        (
            "内存",
            "“已用”= 应用 + 联动 + 压缩内存，不含文件缓存。“内存压力”是系统给出的等级，比已用量更能反映是否不够用。",
            "Memory",
            "“Used” = app + wired + compressed memory, without file cache. “Memory pressure” is macOS's own level and shows shortage better than the amount used.",
        ),
        (
            "交换读写",
            "内存不够时系统把数据写到磁盘再读回。≥ 1 MiB/s 需要注意，≥ 16 MiB/s 严重；只有交换文件而没有读写不需要处理。",
            "Swap activity",
            "When memory runs short, macOS writes data to disk and reads it back. ≥ 1 MiB/s needs attention, ≥ 16 MiB/s is critical; a swap file without activity needs no action.",
        ),
        (
            "处理器",
            "两次采样之间所有核心的平均使用率；刚启动或刚继续采集时需要两个样本才能算出。",
            "Processor",
            "Average use of all cores between two samples; right after starting or resuming it takes two samples.",
        ),
        (
            "磁盘空间",
            "启动磁盘 Data 卷的可用容量，按 10 亿字节为 1 GB，与“储存空间”设置一样包含系统可自动释放的空间；不包括读写速度或磁盘健康。",
            "Disk space",
            "Free space on the startup Data volume, with 1 GB = 1,000,000,000 bytes and, like Storage settings, including space macOS can free by itself. Not disk speed or health.",
        ),
        (
            "温度",
            "macOS 的散热等级（正常 / 偏热 / 偏高 / 严重）决定是否提示；芯片温度来自传感器，仅供参考。",
            "Temperature",
            "macOS's cooling level (normal / warm / hot / critical) decides alerts; chip temperatures come from sensors and are for reference.",
        ),
        (
            "一键优化",
            "只会请你打开的应用像按 ⌘Q 一样正常退出：应用会先询问是否保存，不强制结束、不删除文件、不改系统设置。",
            "Optimize",
            "Only asks apps you opened to quit normally, like ⌘Q: they can ask you to save first. Nothing is force-quit, no files are deleted, no settings change.",
        ),
        (
            "进程排行",
            "按需读取单个进程的内存占用前 5 名（与活动监视器“内存”列同口径）；部分进程可能因权限读不到。",
            "Processes",
            "On request, the top 5 processes by memory (same figure as Activity Monitor's Memory column); some processes may be unreadable due to permissions.",
        ),
        (
            "提示规则",
            "需要注意要连续 2 个异常样本，严重立即提示；连续 2 个正常样本才确认恢复；读不到的样本既不算异常也不算恢复。阈值是本工具的经验值，不是 Apple 官方标准。",
            "Alert rules",
            "“Needs attention” takes 2 abnormal samples in a row, critical alerts at once; recovery takes 2 normal samples in a row; unreadable samples count as neither. Thresholds are Hostwatch's own, not Apple standards.",
        ),
    ];
    blocks.push(Block::Facts(
        pairs
            .iter()
            .map(|(zh_key, zh, en_key, en)| {
                (
                    tr!(lang, zh_key, en_key).to_string(),
                    tr!(lang, zh, en).to_string(),
                )
            })
            .collect(),
    ));
    match monitor.current() {
        Some(latest) => {
            let sources = &latest.snapshot.sources;
            let unknown_error = tr!(lang, "未知错误", "unknown error");
            let failed: Vec<String> = sources
                .iter()
                .filter(|s| !s.ok)
                .map(|s| {
                    let error = s.error.as_deref().unwrap_or(unknown_error);
                    tr!(
                        lang,
                        format!("{}（{error}）", s.source),
                        format!("{} ({error})", s.source)
                    )
                })
                .collect();
            let (at, ok, total) = (
                clock(latest.snapshot.unix_seconds),
                sources.len() - failed.len(),
                sources.len(),
            );
            let mut line = tr!(
                lang,
                format!("最新样本 {at}：{ok}/{total} 个数据源可用"),
                format!("Latest sample {at}: {ok}/{total} data sources available")
            );
            if !failed.is_empty() {
                line.push_str(&tr!(
                    lang,
                    format!("；不可用：{}", failed.join("、")),
                    format!("; unavailable: {}", failed.join(", "))
                ));
            }
            blocks.push(Block::Note(line));
        }
        None => blocks.push(Block::Note(match monitor.failure() {
            Some((at, error, _)) => {
                let at = clock(at);
                tr!(
                    lang,
                    format!("{at} 采集失败：{error}"),
                    format!("Sampling failed at {at}: {error}")
                )
            }
            None => tr!(lang, "尚无样本。", "No sample yet.").into(),
        })),
    }
    blocks.push(Block::Note(
        tr!(
            lang,
            "Hostwatch 只读取系统统计、不上传任何数据；只有你在“一键优化”中确认后，才会请所选应用正常退出。",
            "Hostwatch only reads system statistics and uploads nothing; it asks apps to quit only after you confirm in Optimize."
        )
        .into(),
    ));
    blocks.push(Block::Buttons(vec![Button::new(
        tr!(lang, "退出 Hostwatch", "Quit Hostwatch"),
        Command::Quit,
    )]));
}
