use super::*;

pub(crate) fn memory_detail(view: &View, blocks: &mut Vec<Block>) {
    let monitor = view.monitor;
    let latest = shown(view).map(|(l, _)| l);
    let memory = latest.map(|l| &l.snapshot.memory);
    let breakdown = memory.and_then(|m| m.breakdown.as_ref());
    blocks.push(match (breakdown, memory) {
        (Some(b), Some(m)) => Block::Big {
            value: gib(b.used_bytes),
            unit: format!("/ {} GiB 已用", gib(m.total_bytes)),
        },
        _ => Block::Big {
            value: "—".into(),
            unit: "已用内存未知".into(),
        },
    });
    blocks.push(Block::Text(
        "“已用”= 应用内存 + 联动内存 + 压缩内存，不含可随时回收的文件缓存。已用多本身很正常；真正要看的是系统给出的“内存压力”和持续的交换读写。压力只有正常 / 偏高 / 严重三个等级，不是百分比。"
            .into(),
    ));
    let total = memory
        .map(|m| m.total_bytes)
        .or_else(|| monitor.latest().map(|l| l.snapshot.memory.total_bytes));
    if let Some(total) = total {
        blocks.push(Block::Graphic(Graphic::Chart(memory_chart(monitor, total))));
    }
    let mut swap = swap_chart(monitor);
    swap.unit = "MiB/s · ≥ 1 需要注意 · ≥ 16 严重".into();
    blocks.push(Block::Graphic(Graphic::Chart(swap)));
    let unknown = || "未知".to_string();
    let mut facts = vec![(
        "内存压力（系统等级）".to_string(),
        memory.map_or_else(unknown, |m| pressure_label(m.pressure).into()),
    )];
    match breakdown {
        Some(b) => facts.extend([
            ("应用内存".into(), format!("{} GiB", gib(b.app_bytes))),
            ("联动内存".into(), format!("{} GiB", gib(b.wired_bytes))),
            (
                "压缩内存".into(),
                format!("{} GiB", gib(b.compressed_bytes)),
            ),
            (
                "文件缓存（不计入已用）".into(),
                format!("{} GiB", gib(b.cached_files_bytes)),
            ),
            ("空闲".into(), format!("{} GiB", gib(b.free_bytes))),
        ]),
        None => facts.push(("内存分项".into(), unknown())),
    }
    let activity = latest.map(|l| &l.snapshot.activity);
    facts.push((
        "交换读取 / 写入".into(),
        match activity {
            Some(a) if a.status == RateStatus::Ready => {
                match (a.swap_in_bytes_per_second, a.swap_out_bytes_per_second) {
                    (Some(i), Some(o)) => format!("{:.1} / {}", i / MIB, rate(o)),
                    _ => unknown(),
                }
            }
            Some(_) => "正在建立基线".into(),
            None => unknown(),
        },
    ));
    facts.push((
        "交换文件已用".into(),
        memory
            .and_then(|m| m.swap.as_ref())
            .map_or_else(unknown, |s| format!("{} GiB", gib(s.used_bytes))),
    ));
    blocks.push(Block::Facts(facts));
    let mut buttons = vec![Button::new(
        "打开活动监视器",
        Command::Open(Tool::ActivityMonitor),
    )];
    related_issue(view, Group::Memory, &mut buttons);
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, Tool::ActivityMonitor, blocks);
    blocks.push(Block::Note(freshness_note(
        view,
        "系统内存统计、交换计数器、内核内存压力等级",
    )));
}

pub(crate) fn cpu_detail(view: &View, blocks: &mut Vec<Block>) {
    let monitor = view.monitor;
    let activity = shown(view).map(|(l, _)| &l.snapshot.activity);
    blocks.push(match activity {
        Some(a) if a.status == RateStatus::Ready && a.cpu_busy_percent.is_some() => Block::Big {
            value: format!("{:.0}", a.cpu_busy_percent.unwrap_or(0.0)),
            unit: "% 使用率".into(),
        },
        Some(a) if a.status != RateStatus::Ready && a.status != RateStatus::Unavailable => {
            Block::Big {
                value: "—".into(),
                unit: "正在建立基线，需要再采样一次".into(),
            }
        }
        _ => Block::Big {
            value: "—".into(),
            unit: "使用率未知".into(),
        },
    });
    blocks.push(Block::Text(
        "处理器使用率是两次采样之间所有核心平均有多忙。短时间升高很正常，连续 2 个样本 ≥ 90% 才会提示。Hostwatch 不提供按进程的处理器排行。"
            .into(),
    ));
    let mut chart = cpu_chart(monitor);
    chart.unit = "% · 虚线 90 为提示线".into();
    let peak = chart
        .segments
        .iter()
        .flatten()
        .map(|p| p.1)
        .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))));
    blocks.push(Block::Graphic(Graphic::Chart(chart)));
    blocks.push(Block::Facts(vec![
        ("图表范围内最高".into(), peak.map_or("—".into(), percent)),
        ("提示条件".into(), "≥ 90%，连续 2 个样本".into()),
        ("按进程查看".into(), "请用活动监视器的“CPU”页".into()),
    ]));
    let mut buttons = vec![Button::new(
        "打开活动监视器",
        Command::Open(Tool::ActivityMonitor),
    )];
    related_issue(view, Group::Cpu, &mut buttons);
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, Tool::ActivityMonitor, blocks);
    blocks.push(Block::Note(freshness_note(
        view,
        "系统处理器负载计数（两次采样之差）",
    )));
}

pub(crate) fn disk_detail(view: &View, blocks: &mut Vec<Block>) {
    let disk = shown(view).and_then(|(l, _)| l.snapshot.disk.as_ref());
    match disk {
        Some(d) => {
            let used = d.total_bytes - d.available_bytes;
            let fraction = used as f64 / d.total_bytes as f64;
            blocks.push(Block::Big {
                value: gib(d.available_bytes),
                unit: "GiB 可用".into(),
            });
            blocks.push(Block::Text(
                "这是启动磁盘（Data 卷）还剩多少空间。空间太少时，系统更新、保存文件和交换都会受影响。这里只显示容量，不显示读写速度或磁盘健康；同一容器中的卷共享空间。"
                    .into(),
            ));
            let tone = match issue_for(view, Group::Disk) {
                Some(issue) => Tone::of(issue.severity),
                None => Tone::Neutral,
            };
            blocks.push(Block::Graphic(Graphic::Bar {
                fraction,
                tone,
                label: format!(
                    "已用 {:.0}%，可用 {:.1}%",
                    fraction * 100.0,
                    100.0 - fraction * 100.0
                ),
            }));
            blocks.push(Block::Facts(vec![
                ("可用".into(), format!("{} GiB", gib(d.available_bytes))),
                ("已用".into(), format!("{} GiB", gib(used))),
                ("总容量".into(), format!("{} GiB", gib(d.total_bytes))),
                ("卷".into(), d.path.clone()),
                ("提示条件".into(), rule_rule("disk_space").into()),
            ]));
        }
        None => {
            blocks.push(Block::Big {
                value: "—".into(),
                unit: "可用空间未知".into(),
            });
            blocks.push(Block::Text(
                "目前读不到启动磁盘的容量，显示为“未知”，不代表空间充足。".into(),
            ));
        }
    }
    blocks.push(Block::Note(
        "容量以 GiB 计（1 GiB ≈ 1.07 GB），“储存空间”设置以 GB 显示。只显示当前值，不保留容量历史。"
            .into(),
    ));
    let mut buttons = vec![Button::new(
        "打开“储存空间”设置",
        Command::Open(Tool::StorageSettings),
    )];
    related_issue(view, Group::Disk, &mut buttons);
    blocks.push(Block::Buttons(buttons));
    tool_feedback(view, Tool::StorageSettings, blocks);
    blocks.push(Block::Note(freshness_note(
        view,
        "文件系统容量统计（statfs）",
    )));
}
