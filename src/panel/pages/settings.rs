use super::*;

pub(crate) fn settings(view: &View, blocks: &mut Vec<Block>) {
    let monitor = view.monitor;
    blocks.push(Block::Heading("采样间隔".into()));
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
                let mut title = interval_label(seconds);
                if custom && seconds == monitor.interval() {
                    title.push_str("（启动参数）");
                }
                let mut button = Button::new(title, Command::SetInterval(seconds));
                button.primary = seconds == monitor.interval();
                button
            })
            .collect(),
    ));
    blocks.push(Block::Note(if monitor.paused() {
        "暂停中：继续采集后按所选间隔采样。".into()
    } else {
        "间隔越短越及时，开销也略高。修改后下一个样本按新间隔采集。".into()
    }));
    blocks.push(Block::Heading("采集".into()));
    blocks.push(Block::Buttons(vec![Button::new(
        if monitor.paused() {
            "继续采集"
        } else {
            "暂停采集"
        },
        Command::TogglePause,
    )]));
    blocks.push(Block::Heading("数据说明".into()));
    blocks.push(Block::Facts(vec![
        (
            "内存".into(),
            "“已用”= 应用 + 联动 + 压缩内存，不含文件缓存。“内存压力”是系统给出的等级，比已用量更能反映是否不够用。"
                .into(),
        ),
        (
            "交换读写".into(),
            "内存不够时系统把数据写到磁盘再读回。≥ 1 MiB/s 需要注意，≥ 16 MiB/s 严重；只有交换文件而没有读写不需要处理。"
                .into(),
        ),
        (
            "处理器".into(),
            "两次采样之间所有核心的平均使用率；刚启动或刚继续采集时需要两个样本才能算出。".into(),
        ),
        (
            "磁盘空间".into(),
            "启动磁盘 Data 卷的可用容量，按 10 亿字节为 1 GB，与“储存空间”设置一样包含系统可自动释放的空间；不包括读写速度或磁盘健康。".into(),
        ),
        (
            "温度".into(),
            "macOS 的散热等级（正常 / 偏热 / 偏高 / 严重）决定是否提示；芯片温度来自传感器，仅供参考。".into(),
        ),
        (
            "一键优化".into(),
            "只会请你打开的应用像按 ⌘Q 一样正常退出：应用会先询问是否保存，不强制结束、不删除文件、不改系统设置。".into(),
        ),
        (
            "进程排行".into(),
            "按需读取单个进程的内存占用前 5 名（与活动监视器“内存”列同口径）；部分进程可能因权限读不到。".into(),
        ),
        (
            "提示规则".into(),
            "需要注意要连续 2 个异常样本，严重立即提示；连续 2 个正常样本才确认恢复；读不到的样本既不算异常也不算恢复。阈值是本工具的经验值，不是 Apple 官方标准。"
                .into(),
        ),
    ]));
    match monitor.current() {
        Some(latest) => {
            let sources = &latest.snapshot.sources;
            let failed: Vec<String> = sources
                .iter()
                .filter(|s| !s.ok)
                .map(|s| {
                    format!(
                        "{}（{}）",
                        s.source,
                        s.error.as_deref().unwrap_or("未知错误")
                    )
                })
                .collect();
            let mut line = format!(
                "最新样本 {}：{}/{} 个数据源可用",
                clock(latest.snapshot.unix_seconds),
                sources.len() - failed.len(),
                sources.len()
            );
            if !failed.is_empty() {
                line.push_str(&format!("；不可用：{}", failed.join("、")));
            }
            blocks.push(Block::Note(line));
        }
        None => blocks.push(Block::Note(match monitor.failure() {
            Some((at, error, _)) => format!("{} 采集失败：{error}", clock(at)),
            None => "尚无样本。".into(),
        })),
    }
    blocks.push(Block::Note(
        "Hostwatch 只读取系统统计：不清理、不结束进程、不上传任何数据。".into(),
    ));
    blocks.push(Block::Buttons(vec![Button::new(
        "退出 Hostwatch",
        Command::Quit,
    )]));
}
