//! Time-series charts built from retained samples. Points keep their own timestamps; gaps are
//! labelled only from recorded evidence.

use super::*;

pub(crate) fn gap_label(lang: Lang, monitor: &Monitor, a: u64, b: u64) -> Option<&'static str> {
    let between = |m: &&Mark| match **m {
        Mark::Failed(t) | Mark::Paused(t) | Mark::Resumed(t) => a < t && t <= b,
        Mark::SleepGap(t) => t == b,
        Mark::Interval(..) => false,
    };
    let marks: Vec<&Mark> = monitor.marks().filter(between).collect();
    if marks
        .iter()
        .any(|m| matches!(m, Mark::Paused(_) | Mark::Resumed(_)))
    {
        return Some(tr!(lang, "暂停", "paused"));
    }
    if marks.iter().any(|m| matches!(m, Mark::SleepGap(_))) {
        return Some(tr!(lang, "睡眠", "sleep"));
    }
    if marks.iter().any(|m| matches!(m, Mark::Failed(_))) {
        return Some(tr!(lang, "采集失败", "failed"));
    }
    let expected = monitor.interval_at(a).max(monitor.interval_at(b));
    // Timer tolerance is 10%; anything well beyond one cadence is missing data.
    (b - a > expected * 3 / 2 + 2).then_some(tr!(lang, "无数据", "no data"))
}

pub(crate) fn series(
    lang: Lang,
    monitor: &Monitor,
    value: impl Fn(&Sample) -> Option<f64>,
) -> (Vec<Vec<(u64, f64)>>, Vec<Gap>) {
    let mut segments: Vec<Vec<(u64, f64)>> = Vec::new();
    let mut gaps = Vec::new();
    let mut run: Vec<(u64, f64)> = Vec::new();
    let mut previous: Option<u64> = None;
    for sample in monitor.history().samples() {
        let t = sample.unix_seconds;
        if let Some(a) = previous {
            if let Some(label) = gap_label(lang, monitor, a, t) {
                gaps.push(Gap {
                    from: a,
                    to: t,
                    label,
                });
                if !run.is_empty() {
                    segments.push(std::mem::take(&mut run));
                }
            }
        }
        match value(sample) {
            Some(v) => run.push((t, v)),
            None => {
                if !run.is_empty() {
                    segments.push(std::mem::take(&mut run));
                }
            }
        }
        previous = Some(t);
    }
    if !run.is_empty() {
        segments.push(run);
    }
    (segments, gaps)
}

pub(crate) fn nice_ceiling(value: f64) -> f64 {
    let mut step = 1.0;
    loop {
        for m in [2.0, 5.0, 10.0] {
            if value <= step * m {
                return step * m;
            }
        }
        step *= 10.0;
    }
}

pub(crate) struct Spec<'a> {
    pub(crate) title: &'a str,
    pub(crate) unit: &'a str,
    pub(crate) format: fn(f64) -> String,
    pub(crate) empty: &'a str,
    pub(crate) tone: Tone,
}

pub(crate) fn chart(
    lang: Lang,
    monitor: &Monitor,
    spec: Spec,
    y_max: f64,
    y_ticks: Vec<(f64, String)>,
    thresholds: Vec<(f64, String)>,
    value: impl Fn(&Sample) -> Option<f64>,
) -> Chart {
    let (segments, gaps) = series(lang, monitor, &value);
    let samples = monitor.history().len();
    let points: Vec<(u64, f64)> = segments.iter().flatten().copied().collect();
    let t0 = monitor
        .history()
        .samples()
        .next()
        .map_or(0, |s| s.unix_seconds);
    let t1 = last_sample(monitor).map_or(0, |s| s.unix_seconds);
    let empty = points.is_empty().then(|| spec.empty.to_string());
    let x_labels = if t1 > t0 {
        let label = if t1 - t0 < 120 { clock } else { hm };
        vec![(t0, label(t0)), (t1, label(t1))]
    } else {
        Vec::new()
    };
    let mut summary = format!("{}{}", spec.title, tr!(lang, "，", ": "));
    let mut note = String::new();
    if let (Some(&(first, _)), Some(&(last, latest))) = (points.first(), points.last()) {
        let min = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        let (from, to, low, high, now) = (
            clock(first),
            clock(last),
            (spec.format)(min),
            (spec.format)(max),
            (spec.format)(latest),
        );
        summary.push_str(&tr!(
            lang,
            format!("{from} 至 {to}，最低 {low}，最高 {high}，最近 {now}（{to}）"),
            format!("{from} to {to}, low {low}, high {high}, latest {now} ({to})")
        ));
        if !gaps.is_empty() {
            let list: Vec<String> = gaps
                .iter()
                .map(|g| format!("{}–{} {}", hm(g.from), hm(g.to), g.label))
                .collect();
            let count = gaps.len();
            summary.push_str(&tr!(
                lang,
                format!("；有 {count} 段空白：{}", list.join("，")),
                format!("; {count} gap(s): {}", list.join(", "))
            ));
        }
        note = if samples < 10 {
            tr!(
                lang,
                format!("历史不足：已有 {samples} 个样本（最多保留 120 个），只显示已有范围。"),
                format!("Short history: {samples} sample(s) so far (up to 120 kept); only that range is shown.")
            )
        } else {
            let (start, end, length) = (hm(t0), hm(t1), span(lang, t1 - t0));
            tr!(
                lang,
                format!("最近 {samples} 个样本，{start}–{end}（{length}）。空白表示没有数据，不按 0 计算。"),
                format!("Last {samples} samples, {start}–{end} ({length}). Blank areas mean no data, not zero.")
            )
        };
    } else {
        summary.push_str(spec.empty);
    }
    Chart {
        title: spec.title.into(),
        unit: spec.unit.into(),
        y_max,
        y_ticks,
        thresholds,
        t0,
        t1,
        segments,
        gaps,
        x_labels,
        summary,
        note,
        empty,
        tone: spec.tone,
    }
}

pub(crate) fn rate_empty(lang: Lang, monitor: &Monitor) -> &'static str {
    if monitor.history().len() < 2 {
        tr!(
            lang,
            "还没有历史数据：需要至少 2 个样本才能计算速率",
            "No history yet: rates need at least 2 samples"
        )
    } else {
        unavailable(lang)
    }
}

fn unavailable(lang: Lang) -> &'static str {
    tr!(
        lang,
        "数据暂不可用（显示为未知，不是 0）",
        "Data unavailable (shown as unknown, not zero)"
    )
}

fn reading_first(lang: Lang) -> &'static str {
    tr!(lang, "正在读取，还没有历史数据", "Reading; no history yet")
}

pub fn cpu_chart(lang: Lang, monitor: &Monitor) -> Chart {
    chart(
        lang,
        monitor,
        Spec {
            title: tr!(lang, "处理器使用率", "Processor usage"),
            unit: "%",
            format: percent,
            empty: rate_empty(lang, monitor),
            tone: Tone::Neutral,
        },
        100.0,
        vec![
            (0.0, "0".into()),
            (50.0, "50".into()),
            (100.0, "100".into()),
        ],
        vec![(90.0, "90".into())],
        |s| s.cpu_busy_percent,
    )
}

pub fn memory_chart(lang: Lang, monitor: &Monitor, total: u64) -> Chart {
    let total_gib = total as f64 / GIB as f64;
    chart(
        lang,
        monitor,
        Spec {
            title: tr!(lang, "已用内存", "Memory used"),
            unit: "GiB",
            format: |v| format!("{v:.1} GiB"),
            empty: if monitor.history().len() == 0 {
                reading_first(lang)
            } else {
                unavailable(lang)
            },
            tone: Tone::Neutral,
        },
        total_gib,
        vec![(0.0, "0".into()), (total_gib, format!("{total_gib:.0}"))],
        Vec::new(),
        |s| s.memory_used_bytes.map(|b| b as f64 / GIB as f64),
    )
}

pub fn swap_chart(lang: Lang, monitor: &Monitor) -> Chart {
    let peak = monitor
        .history()
        .samples()
        .filter_map(|s| s.swap_traffic_bytes_per_second)
        .fold(0.0, f64::max)
        / MIB;
    let y_max = nice_ceiling((peak * 1.2).max(2.0));
    let thresholds = [
        (1.0, tr!(lang, "1 需要注意", "1 attention")),
        (16.0, tr!(lang, "16 严重", "16 critical")),
    ]
    .into_iter()
    .filter(|(v, _)| *v <= y_max)
    .map(|(v, l)| (v, l.to_string()))
    .collect();
    chart(
        lang,
        monitor,
        Spec {
            title: tr!(lang, "交换读写合计", "Swap reads + writes"),
            unit: "MiB/s",
            format: |v| format!("{v:.1} MiB/s"),
            empty: rate_empty(lang, monitor),
            tone: Tone::Neutral,
        },
        y_max,
        vec![(0.0, "0".into()), (y_max, format!("{y_max:.0}"))],
        thresholds,
        |s| s.swap_traffic_bytes_per_second.map(|v| v / MIB),
    )
}

pub fn temperature_chart(lang: Lang, monitor: &Monitor) -> Chart {
    let peak = monitor
        .history()
        .samples()
        .filter_map(|s| s.cpu_celsius)
        .fold(0.0, f64::max);
    let y_max = if peak > 100.0 {
        nice_ceiling(peak)
    } else {
        100.0
    };
    chart(
        lang,
        monitor,
        Spec {
            title: tr!(lang, "芯片温度", "Chip temperature"),
            unit: "°C",
            format: celsius_label,
            empty: if monitor.history().len() == 0 {
                reading_first(lang)
            } else {
                tr!(
                    lang,
                    "本机读不到芯片温度（显示为未知，不是 0）",
                    "This Mac does not report chip temperature (unknown, not zero)"
                )
            },
            tone: Tone::Neutral,
        },
        y_max,
        vec![
            (0.0, "0".into()),
            (50.0, "50".into()),
            (y_max, format!("{y_max:.0}")),
        ],
        Vec::new(),
        |s| s.cpu_celsius,
    )
}
