#[cfg(any(target_os = "macos", test))]
#[macro_use]
mod lang;
mod activity;
mod advice;
mod alerts;
mod cli;
#[cfg(target_os = "macos")]
mod menubar;
mod model;
#[cfg(any(target_os = "macos", test))]
mod monitor;
#[cfg(any(target_os = "macos", test))]
mod optimize;
#[cfg(any(target_os = "macos", test))]
mod panel;
mod platform;

use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use alerts::{Engine, History, Policy, Sample};
use model::{EventKind, Pressure, RateStatus, Report, ThermalState, SCHEMA_VERSION};

fn main() {
    if let Err(error) = run() {
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe)
        {
            return;
        }
        eprintln!("hostwatch: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        writeln!(io::stdout(), "{}", cli::HELP)?;
        return Ok(());
    }
    let mut options = cli::parse(&args)?;
    // Double-clicking Hostwatch.app passes no arguments; the bundle is the menu-bar app.
    if args.is_empty()
        && std::env::current_exe().is_ok_and(|exe| cli::inside_app_bundle(&exe.to_string_lossy()))
    {
        options.menubar = true;
    }
    if options.menubar {
        #[cfg(target_os = "macos")]
        return Ok(menubar::run(options.interval)?);
        #[cfg(not(target_os = "macos"))]
        return Err("菜单栏仅支持 macOS；Windows 托盘尚未实现。".into());
    }
    let mut collector = platform::collector()?;
    if !options.watch {
        // Rates and CPU need two counter samples; a short baseline keeps snapshot self-contained.
        collector.collect(false)?;
        std::thread::sleep(Duration::from_secs(1));
    }
    let mut engine = Engine::new(Policy::default());
    let mut history = History::new();
    let mut count = 0u64;
    loop {
        let started = Instant::now();
        let snapshot = collector.collect(options.processes)?;
        let advice = advice::evaluate(&snapshot);
        let events = options.watch.then(|| {
            history.push(Sample::from_snapshot(&snapshot));
            engine.observe(snapshot.unix_seconds, &advice::assess(&snapshot))
        });
        let report = Report {
            schema_version: SCHEMA_VERSION,
            snapshot,
            advice,
            events,
        };
        let mut out = io::stdout().lock();
        if options.json {
            writeln!(out, "{}", serde_json::to_string(&report)?)?;
        } else {
            render(&mut out, &report, options.watch.then_some(&history))?;
        }
        out.flush()?;
        count += 1;
        if !options.watch || options.count.is_some_and(|limit| count >= limit) {
            break;
        }
        // Avoid catch-up bursts after slow sampling or machine sleep.
        std::thread::sleep(
            Duration::from_secs(options.interval)
                .saturating_sub(started.elapsed())
                .max(Duration::from_secs(1)),
        );
    }
    Ok(())
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / advice::GIB as f64
}

fn mib(bytes: f64) -> f64 {
    bytes / advice::MIB
}

fn render(out: &mut impl Write, report: &Report, history: Option<&History>) -> io::Result<()> {
    let snapshot = &report.snapshot;
    let memory = &snapshot.memory;
    let pressure = match memory.pressure {
        Pressure::Normal => "正常",
        Pressure::Warning => "偏高",
        Pressure::Critical => "严重",
        Pressure::Unknown => "未知",
    };
    writeln!(
        out,
        "\nHostwatch | {} | Unix 时间 {}",
        snapshot.platform, snapshot.unix_seconds
    )?;
    for event in report.events.iter().flatten() {
        let label = match event.kind {
            EventKind::Raised => "新告警",
            EventKind::Escalated => "告警升级",
            EventKind::Recovered => "已恢复",
        };
        writeln!(
            out,
            ">> {label} [{:?}] {}：{}",
            event.severity, event.id, event.evidence
        )?;
        if event.kind != EventKind::Recovered {
            writeln!(out, "   建议：{}", event.action)?;
        }
    }
    match &memory.breakdown {
        Some(b) => writeln!(
            out,
            "内存：已用 {:.2} / {:.2} GiB（应用 {:.2}，联动 {:.2}，压缩 {:.2}）| 缓存文件 {:.2} | 空闲 {:.2}",
            gib(b.used_bytes),
            gib(memory.total_bytes),
            gib(b.app_bytes),
            gib(b.wired_bytes),
            gib(b.compressed_bytes),
            gib(b.cached_files_bytes),
            gib(b.free_bytes)
        )?,
        None => writeln!(out, "内存：总量 {:.2} GiB，分项不可用", gib(memory.total_bytes))?,
    }
    let swap = memory.swap.as_ref().map_or("不可用".to_string(), |s| {
        format!("{:.2} / {:.2} GiB", gib(s.used_bytes), gib(s.total_bytes))
    });
    writeln!(out, "内存压力：{pressure} | 交换空间：{swap}")?;
    let activity = &snapshot.activity;
    match activity.status {
        RateStatus::Ready => {
            let cpu = activity
                .cpu_busy_percent
                .map_or("不可用".into(), |c| format!("{c:.0}%"));
            writeln!(
                out,
                "活动（{:.0} 秒）：CPU {cpu} | 换入 {:.1} MiB/s | 换出 {:.1} MiB/s | 页面读入 {:.1} MiB/s",
                activity.interval_seconds.unwrap_or(0.0),
                mib(activity.swap_in_bytes_per_second.unwrap_or(0.0)),
                mib(activity.swap_out_bytes_per_second.unwrap_or(0.0)),
                mib(activity.page_in_bytes_per_second.unwrap_or(0.0)),
            )?
        }
        RateStatus::Warmup => writeln!(out, "活动：首个样本，下一次采样后计算速率")?,
        RateStatus::SleepGap => writeln!(out, "活动：检测到睡眠或时钟跳变，本次不计算速率")?,
        RateStatus::CounterReset => writeln!(out, "活动：计数器重置，本次不计算速率")?,
        RateStatus::Unavailable => writeln!(out, "活动：计数器不可用")?,
    }
    if let Some(disk) = &snapshot.disk {
        let gb = |bytes: u64| bytes as f64 / advice::GB as f64;
        writeln!(
            out,
            "磁盘 {}：可用 {:.1} / {:.1} GB（{:.1}% 可用{}）",
            disk.path,
            gb(disk.usable_bytes()),
            gb(disk.total_bytes),
            disk.usable_bytes() as f64 / disk.total_bytes as f64 * 100.0,
            if disk.important_available_bytes.is_some() {
                "，含系统可自动释放的空间"
            } else {
                ""
            }
        )?;
    } else {
        writeln!(out, "磁盘：数据不可用")?;
    }
    if let Some(thermal) = &snapshot.thermal {
        let state = match thermal.state {
            Some(ThermalState::Nominal) => "正常",
            Some(ThermalState::Fair) => "偏热",
            Some(ThermalState::Serious) => "偏高（可能降速）",
            Some(ThermalState::Critical) => "严重（降速）",
            None => "未知",
        };
        match (thermal.cpu_celsius, thermal.cpu_max_celsius) {
            (Some(mean), Some(max)) => {
                writeln!(out, "散热：{state} | 芯片 {mean:.0}°C（最高 {max:.0}°C）")?
            }
            _ => writeln!(out, "散热：{state}")?,
        }
    }
    if let Some(history) = history.filter(|h| h.len() > 1) {
        let t = history.trend();
        let fmt = |v: Option<String>| v.unwrap_or_else(|| "—".into());
        writeln!(
            out,
            "近 {} 次 / {} 秒：CPU 峰值 {} | 交换读写峰值 {} | 内存已用峰值 {} | 磁盘最少可用 {}",
            t.samples,
            t.span_seconds,
            fmt(t.peak_cpu_percent.map(|v| format!("{v:.0}%"))),
            fmt(t
                .peak_swap_bytes_per_second
                .map(|v| format!("{:.1} MiB/s", mib(v)))),
            fmt(t
                .peak_memory_used_bytes
                .map(|v| format!("{:.2} GiB", gib(v)))),
            fmt(t
                .min_disk_available_bytes
                .map(|v| format!("{:.2} GiB", gib(v)))),
        )?;
    }
    if snapshot.processes_requested {
        writeln!(out, "进程 RSS 前五名（可读范围，非活动监视器内存足迹）：")?;
        for process in &snapshot.top_processes {
            // Process names are external data; escape control characters before terminal output.
            writeln!(
                out,
                "  {:>6}  {:>7.1} MiB  {}",
                process.pid,
                process.resident_bytes as f64 / 1024.0 / 1024.0,
                process.name.escape_debug()
            )?;
        }
        if snapshot.top_processes.is_empty() {
            writeln!(out, "  未取得可读进程数据")?;
        }
    }
    for item in &report.advice {
        writeln!(
            out,
            "[{:?}] {}\n  建议：{}",
            item.severity, item.evidence, item.action
        )?;
    }
    for limitation in &snapshot.limitations {
        writeln!(out, "采集说明：{limitation}")?;
    }
    Ok(())
}
