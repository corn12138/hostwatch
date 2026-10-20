use super::*;
use crate::advice::tests::{ready, snapshot};
use crate::model::{Process, ProcessScan, SourceStatus};
use crate::monitor::tests::{at, normal};

fn texts(page: &Page) -> Vec<String> {
    let mut out = Vec::new();
    let buttons =
        |out: &mut Vec<String>, b: &[Button]| out.extend(b.iter().map(|b| b.title.clone()));
    for block in &page.blocks {
        match block {
            Block::Conclusion {
                title, subtitle, ..
            } => out.extend([title.clone(), subtitle.clone()]),
            Block::Header { back, title, badge } => {
                out.extend([back.clone(), title.clone()]);
                out.extend(badge.iter().map(|b| b.1.clone()));
            }
            Block::Resource {
                value,
                status,
                label,
                ..
            } => out.extend([value.clone(), status.clone(), label.clone()]),
            Block::Card {
                title,
                body,
                buttons: b,
                ..
            } => {
                out.push(title.clone());
                out.extend(body.iter().cloned());
                buttons(&mut out, b);
            }
            Block::Heading(t) | Block::Text(t) | Block::Note(t) => out.push(t.clone()),
            Block::Big { value, unit } => out.push(format!("{value} {unit}")),
            Block::Graphic(Graphic::Chart(c) | Graphic::Spark(c)) => {
                out.extend([c.summary.clone(), c.note.clone()]);
                out.extend(c.empty.iter().cloned());
            }
            Block::Graphic(Graphic::Bar { label, .. }) => out.push(label.clone()),
            Block::Facts(f) => out.extend(f.iter().map(|(k, v)| format!("{k}：{v}"))),
            Block::Steps(s) | Block::Bullets(s) => out.extend(s.iter().cloned()),
            Block::Buttons(b) | Block::Choices(b) => buttons(&mut out, b),
            Block::Progress { label, .. } => out.push(label.clone()),
            Block::Processes(rows) => out.extend(
                rows.iter()
                    .map(|r| format!("{} {} {}", r.rank, r.name, r.size)),
            ),
        }
    }
    out
}

fn has(page: &Page, needle: &str) -> bool {
    texts(page).iter().any(|t| t.contains(needle))
}

fn commands(page: &Page) -> Vec<Command> {
    let mut out = vec![
        page.footer.pause.command.clone(),
        page.footer.interval.command.clone(),
    ];
    for block in &page.blocks {
        match block {
            Block::Card { buttons, .. } | Block::Buttons(buttons) | Block::Choices(buttons) => {
                out.extend(buttons.iter().map(|b| b.command.clone()))
            }
            _ => {}
        }
    }
    out
}

fn resource(page: &Page, which: Resource) -> (&str, &str, Tone, bool) {
    page.blocks
        .iter()
        .find_map(|b| match b {
            Block::Resource {
                resource,
                value,
                status,
                tone,
                dim,
                ..
            } if *resource == which => Some((value.as_str(), status.as_str(), *tone, *dim)),
            _ => None,
        })
        .unwrap()
}

fn home(monitor: &Monitor, now: u64) -> Page {
    page(monitor, Route::Home, None, now)
}

#[test]
fn quiet_home_when_nothing_needs_attention() {
    let mut monitor = Monitor::new(30);
    monitor.record(Ok(normal(0)), 0);
    monitor.record(Ok(normal(30)), 30);
    let page = home(&monitor, 31);
    assert!(
        matches!(&page.blocks[0], Block::Conclusion { tone: Tone::Ok, title, .. } if title == "目前无需处理")
    );
    assert_eq!(page.blocks.len(), 4, "conclusion + three rows, no card");
    assert_eq!(resource(&page, Resource::Cpu).0, "20%");
    assert_eq!(page.footer.interval.title, "每 30 秒采样");
}

#[test]
fn chart_endpoint_and_summary_match_displayed_value() {
    let mut monitor = Monitor::new(5);
    for (t, cpu) in [(0, 10.0), (5, 37.4), (10, 18.2)] {
        let mut s = normal(t);
        s.activity = ready(0.0, cpu);
        monitor.record(Ok(s), t);
    }
    let page = home(&monitor, 10);
    let (value, ..) = resource(&page, Resource::Cpu);
    let chart = cpu_chart(&monitor);
    let last = *chart.segments.last().unwrap().last().unwrap();
    assert_eq!((last.0, percent(last.1)), (10, value.to_string()));
    assert!(chart.summary.contains(&format!("最近 {value}")));
    assert!(chart.summary.contains("最高 37%"));
    let detail = page_for(&monitor, Route::Detail(Resource::Cpu));
    assert!(has(&detail, "18 % 使用率"));
}

fn page_for(monitor: &Monitor, route: Route) -> Page {
    page(
        monitor,
        route,
        Some(Route::Home),
        monitor
            .history()
            .samples()
            .last()
            .map_or(0, |s| s.unix_seconds),
    )
}

#[test]
fn history_uses_timestamps_across_interval_changes_and_gaps() {
    let mut monitor = Monitor::new(30);
    for t in [0, 30, 60] {
        monitor.record(Ok(normal(t)), t);
    }
    monitor.set_interval(5, 61);
    for t in [65, 70] {
        monitor.record(Ok(normal(t)), t);
    }
    let chart = cpu_chart(&monitor);
    let times: Vec<u64> = chart.segments.iter().flatten().map(|p| p.0).collect();
    assert_eq!(
        times,
        [0, 30, 60, 65, 70],
        "old samples keep their own timestamps"
    );
    assert!(chart.gaps.is_empty(), "a cadence change is not a gap");
    assert_eq!((chart.t0, chart.t1), (0, 70));
    assert!(chart.note.contains("已有 5 个样本"));

    // Missing samples without recorded evidence: a gap labelled as no data, never sleep.
    monitor.record(Ok(normal(200)), 200);
    // A recorded failure, a pause and measured sleep each explain their own gap.
    monitor.record(Err("x".into()), 205);
    monitor.record(Ok(normal(210)), 210);
    monitor.set_paused(true, 211);
    monitor.set_paused(false, 300);
    monitor.record(Ok(normal(300)), 300);
    let mut woke = normal(5000);
    woke.activity = crate::activity::empty(RateStatus::SleepGap);
    monitor.record(Ok(woke), 5000);
    let labels: Vec<&str> = cpu_chart(&monitor).gaps.iter().map(|g| g.label).collect();
    assert_eq!(labels, ["无数据", "采集失败", "暂停", "睡眠"]);
    // The sleep sample has no CPU rate: absent, not zero, and the line is not joined across.
    let chart = cpu_chart(&monitor);
    assert!(chart.segments.iter().flatten().all(|p| p.0 != 5000));
    assert!(chart
        .segments
        .iter()
        .all(|s| s.windows(2).all(|w| w[1].0 - w[0].0 <= 30)));
}

#[test]
fn missing_values_are_absent_not_zero() {
    let mut monitor = Monitor::new(30);
    let mut first = normal(0);
    first.activity = crate::activity::empty(RateStatus::Warmup);
    monitor.record(Ok(first), 0);
    let page = home(&monitor, 0);
    let (value, status, tone, _) = resource(&page, Resource::Cpu);
    assert_eq!((value, status, tone), ("—", "正在建立基线", Tone::Loading));
    let swap = swap_chart(&monitor);
    assert!(swap.segments.is_empty());
    assert!(swap.empty.unwrap().contains("至少 2 个样本"));
    let mut s = normal(30);
    s.memory.swap = None;
    s.activity.swap_in_bytes_per_second = None;
    monitor.record(Ok(s), 30);
    let detail = page_for(&monitor, Route::Detail(Resource::Memory));
    assert!(has(&detail, "交换文件已用：未知"));
    assert!(has(&detail, "交换读取 / 写入：未知"));
}

#[test]
fn swap_axis_fits_real_values_beyond_demo_range() {
    let mut monitor = Monitor::new(5);
    for t in [0, 5] {
        let mut s = normal(t);
        s.activity = ready(80.0 * MIB, 10.0);
        monitor.record(Ok(s), t);
    }
    let chart = swap_chart(&monitor);
    assert!(chart.y_max >= 80.0);
    assert!(chart.thresholds.iter().any(|t| t.0 == 16.0));
    let mut quiet = Monitor::new(5);
    quiet.record(Ok(normal(0)), 0);
    assert_eq!(swap_chart(&quiet).y_max, 2.0);
    assert_eq!(swap_chart(&quiet).thresholds.len(), 1);
}

#[test]
fn grouped_issue_counts_and_recovery_stay_engine_owned() {
    let mut monitor = Monitor::new(30);
    for t in [0, 30] {
        let mut s = at(snapshot(Pressure::Warning, 4 * GIB), t);
        s.activity = ready(20.0 * MIB, 95.0);
        monitor.record(Ok(s), t);
    }
    let page = home(&monitor, 30);
    assert!(has(&page, "有 3 项需要注意"));
    assert!(has(&page, "另有 2 项 ›"));
    let list = page_for(&monitor, Route::Issues);
    assert!(has(&list, "共 3 项问题（涉及 4 条检查规则）"));
    assert!(has(&list, "合并提示，包含：内存压力、交换读写"));

    let handling = page_for(&monitor, Route::Handling(Group::Memory));
    assert!(has(&handling, "内存不足，电脑可能变慢"));
    assert!(has(&handling, "交换读写合计 20.0 MiB/s（当前"));
    assert!(commands(&handling).contains(&Command::Open(Tool::ActivityMonitor)));
    // Opening the tool does not change the recovery shown.
    let before = handling
        .blocks
        .iter()
        .find(|b| matches!(b, Block::Progress { .. }))
        .cloned();
    monitor.note_open(Tool::ActivityMonitor, Ok(()), 31);
    let after = page_for(&monitor, Route::Handling(Group::Memory));
    assert_eq!(
        before,
        after
            .blocks
            .iter()
            .find(|b| matches!(b, Block::Progress { .. }))
            .cloned()
    );
    assert!(has(&after, "已请求打开活动监视器"));
    assert!(!has(&after, "已恢复"));

    monitor.record(Ok(at(snapshot(Pressure::Normal, 300 * GIB), 60)), 60);
    let handling = page_for(&monitor, Route::Handling(Group::Memory));
    assert!(has(&handling, "正在确认恢复：1/2 个正常样本"));
    // Retained evidence is labelled with its trigger time and not shown as current.
    assert!(has(
        &handling,
        &format!(
            "交换读写：{} 时 交换读写合计 20.0 MiB/s；当前已回到正常范围",
            clock(30)
        )
    ));
    monitor.record(Ok(at(snapshot(Pressure::Normal, 300 * GIB), 90)), 90);
    let page = home(&monitor, 90);
    assert!(has(&page, "内存、处理器、磁盘空间已恢复正常"));
    assert!(has(
        &page_for(&monitor, Route::Handling(Group::Memory)),
        "已恢复正常"
    ));
}

#[test]
fn failure_and_stale_are_not_health() {
    let mut monitor = Monitor::new(30);
    monitor.record(Ok(at(snapshot(Pressure::Critical, 300 * GIB), 0)), 0);
    monitor.record(Err("无法读取物理内存总量".into()), 30);
    let page = home(&monitor, 30);
    assert!(has(&page, "暂时读不到系统数据"));
    assert_eq!(resource(&page, Resource::Memory).0, "—");
    assert_eq!(resource(&page, Resource::Memory).1, "压力严重（此前）");
    assert!(has(&page, "此前的提示仍保留：内存严重不足"));
    assert!(commands(&page).contains(&Command::Retry));
    let handling = page_for(&monitor, Route::Handling(Group::Memory));
    assert!(has(&handling, "本次未能读取，当前状态未知"));

    let mut monitor = Monitor::new(30);
    monitor.record(Ok(normal(0)), 0);
    let page = home(&monitor, 200);
    assert!(has(&page, "数据已过时"));
    let (value, status, _, dim) = resource(&page, Resource::Disk);
    assert!(value != "—" && status == "已过时" && dim);
}

#[test]
fn partial_availability_names_the_unknown_item() {
    let mut monitor = Monitor::new(30);
    let mut s = snapshot(Pressure::Unknown, 300 * GIB);
    s.sources.push(SourceStatus {
        source: "kern.memorystatus_vm_pressure_level",
        ok: false,
        elapsed_micros: 3,
        error: Some("denied".into()),
    });
    monitor.record(Ok(s), 0);
    let page = home(&monitor, 0);
    assert!(has(&page, "部分数据暂时读不到"));
    assert!(has(&page, "内存压力暂时读不到"));
    assert_eq!(resource(&page, Resource::Memory).1, "压力未知");
}

#[test]
fn paused_home_and_settings_keep_interval_consistent() {
    let mut monitor = Monitor::new(7);
    monitor.record(Ok(normal(0)), 0);
    monitor.set_paused(true, 3);
    let page = home(&monitor, 500);
    assert!(has(&page, "已暂停采集"));
    assert_eq!(resource(&page, Resource::Disk).1, "已暂停");
    assert_eq!(page.footer.pause.title, "继续采集");
    assert_eq!(page.footer.interval.title, "每 7 秒采样");
    let settings = page_for(&monitor, Route::Settings);
    let choices: Vec<(String, bool)> = settings
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Choices(c) => Some(c.iter().map(|b| (b.title.clone(), b.primary)).collect()),
            _ => None,
        })
        .unwrap();
    assert_eq!(choices.len(), 6);
    assert!(choices.contains(&("7 秒（启动参数）".into(), true)));
    assert!(choices.contains(&("5 分钟".into(), false)));
    assert!(has(&settings, "暂停中：继续采集后按所选间隔采样"));
    monitor.set_interval(60, 501);
    let page = home(&monitor, 502);
    assert_eq!(page.footer.interval.title, "每 1 分钟采样");
}

#[test]
fn process_page_lifecycle() {
    let mut monitor = Monitor::new(30);
    let page = page_for(&monitor, Route::Processes);
    assert!(has(&page, "尚未读取"));
    assert!(has(&page, "读取"));
    monitor.begin_processes(10);
    let page = page_for(&monitor, Route::Processes);
    assert!(has(&page, "读取中…"));
    let enabled = page.blocks.iter().find_map(|b| match b {
        Block::Buttons(b) if b[0].command == Command::RefreshProcesses => Some(b[0].enabled),
        _ => None,
    });
    assert_eq!(enabled, Some(false));
    monitor.finish_processes(
        Ok(ProcessScan {
            top: vec![Process {
                pid: 9,
                name: "evil\nname".into(),
                resident_bytes: 3 * GIB,
            }],
            listed: 400,
            unreadable: 120,
        }),
        12,
    );
    let page = page_for(&monitor, Route::Processes);
    assert!(has(&page, "1 evilname 3.0 GiB"));
    assert!(has(&page, "另有 120 个进程（共列出 400 个）读不到内存数据"));
    assert!(has(&page, "刷新"));
    monitor.finish_processes(
        Ok(ProcessScan {
            top: vec![],
            listed: 3,
            unreadable: 0,
        }),
        20,
    );
    let page = page_for(&monitor, Route::Processes);
    assert!(has(&page, "没有读到任何进程的内存数据"));
    assert!(
        !has(&page, "读不到内存数据，通常"),
        "fewer results do not imply permission limits"
    );
    monitor.finish_processes(Err("系统没有返回任何进程信息".into()), 30);
    assert!(has(&page_for(&monitor, Route::Processes), "读取失败"));
}

#[test]
fn tool_failure_offers_manual_path_and_retry() {
    let mut monitor = Monitor::new(30);
    monitor.record(Ok(at(snapshot(Pressure::Normal, 4 * GIB), 0)), 0);
    monitor.note_open(Tool::StorageSettings, Err("系统未能打开 x".into()), 5);
    let page = page_for(&monitor, Route::Handling(Group::Disk));
    assert!(has(&page, "无法打开“储存空间”设置"));
    assert!(has(&page, "苹果菜单 › 系统设置 › 通用 › 储存空间"));
    assert!(
        commands(&page)
            .iter()
            .filter(|c| **c == Command::Open(Tool::StorageSettings))
            .count()
            >= 2
    );
}

#[test]
fn memory_copy_matches_collector_definition() {
    let mut monitor = Monitor::new(30);
    let mut s = normal(0);
    s.memory.breakdown = Some(crate::model::MemoryBreakdown {
        used_bytes: 10 * GIB,
        app_bytes: 6 * GIB,
        wired_bytes: 2 * GIB,
        compressed_bytes: 2 * GIB,
        cached_files_bytes: 5 * GIB,
        free_bytes: GIB,
    });
    monitor.record(Ok(s), 0);
    let detail = page_for(&monitor, Route::Detail(Resource::Memory));
    assert!(has(&detail, "10.0 / 16.0 GiB 已用"));
    assert!(has(&detail, "不含可随时回收的文件缓存"));
    assert!(has(&detail, "文件缓存（不计入已用）：5.0 GiB"));
    let chart = memory_chart(&monitor, 16 * GIB);
    assert_eq!(chart.segments[0][0].1, 10.0);
}

#[test]
fn clock_and_span_formatting() {
    assert_eq!(clock(0).len(), 8);
    assert_eq!(span(45), "45 秒");
    assert_eq!(span(600), "约 10 分钟");
    assert_eq!(span(3900), "约 1 小时 5 分钟");
    assert_eq!(interval_label(300), "5 分钟");
    assert_eq!(interval_label(90), "90 秒");
}
