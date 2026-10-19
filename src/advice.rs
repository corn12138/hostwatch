use crate::model::{Advice, Pressure, RateStatus, Severity, Snapshot};

pub const GIB: u64 = 1024 * 1024 * 1024;
pub const MIB: f64 = 1024.0 * 1024.0;

/// Product heuristics, not Apple limits. Swap traffic counts both directions.
const SWAP_WARNING_BYTES_PER_SECOND: f64 = MIB;
const SWAP_CRITICAL_BYTES_PER_SECOND: f64 = 16.0 * MIB;
const CPU_WARNING_PERCENT: f64 = 90.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Observation {
    Abnormal(Severity, String),
    Clear,
    /// The rule could not be judged from this sample; never treated as healthy or recovered.
    Unknown,
}

#[derive(Debug)]
pub struct Finding {
    pub id: &'static str,
    pub observation: Observation,
    pub action: &'static str,
}

const MEMORY_ACTION: &str = "打开活动监视器 → 内存，查看高占用应用；先保存工作，再正常退出不需要的应用或浏览器标签。持续升高时再排查具体应用。";
const SWAP_ACTION: &str = "系统正在频繁读写交换文件，这通常会直接导致卡顿：打开活动监视器 → 内存，按「内存」排序，关闭不需要的大型应用或标签页；不要手动删除交换文件。";
const CPU_ACTION: &str = "打开活动监视器 → CPU，按「% CPU」排序找出持续占用的应用；确认无正在进行的工作后再正常退出。短时编译、索引或备份属于正常现象。";
const DISK_ACTION: &str = "打开系统设置 → 通用 → 储存空间，优先审查下载、大型文件和不用的应用；确认备份与用途后再手动删除。不要直接删除系统目录或交换文件。";

pub fn assess(snapshot: &Snapshot) -> Vec<Finding> {
    let memory = &snapshot.memory;
    let activity = &snapshot.activity;
    let swap_used = memory.swap.as_ref().map_or(0, |s| s.used_bytes);
    // RAM caches and historical swap alone are not evidence of present memory pressure.
    let pressure = match memory.pressure {
        Pressure::Critical | Pressure::Warning => Observation::Abnormal(
            if memory.pressure == Pressure::Critical {
                Severity::Critical
            } else {
                Severity::Warning
            },
            format!(
                "内核内存压力 {:?}，交换空间已用 {:.2} GiB",
                memory.pressure,
                swap_used as f64 / GIB as f64
            ),
        ),
        Pressure::Normal => Observation::Clear,
        Pressure::Unknown => Observation::Unknown,
    };
    let ready = activity.status == RateStatus::Ready;
    let swap = match (
        ready,
        activity.swap_in_bytes_per_second,
        activity.swap_out_bytes_per_second,
    ) {
        (true, Some(swap_in), Some(swap_out)) => {
            let traffic = swap_in + swap_out;
            let severity = if traffic >= SWAP_CRITICAL_BYTES_PER_SECOND {
                Some(Severity::Critical)
            } else if traffic >= SWAP_WARNING_BYTES_PER_SECOND {
                Some(Severity::Warning)
            } else {
                None
            };
            match severity {
                Some(severity) => Observation::Abnormal(
                    severity,
                    format!(
                        "交换读写 {:.1} MiB/s（换入 {:.1}，换出 {:.1}）",
                        traffic / MIB,
                        swap_in / MIB,
                        swap_out / MIB
                    ),
                ),
                None => Observation::Clear,
            }
        }
        _ => Observation::Unknown,
    };
    let cpu = match (ready, activity.cpu_busy_percent) {
        (true, Some(busy)) if busy >= CPU_WARNING_PERCENT => {
            Observation::Abnormal(Severity::Warning, format!("CPU 总体占用 {busy:.0}%"))
        }
        (true, Some(_)) => Observation::Clear,
        _ => Observation::Unknown,
    };
    let disk = match &snapshot.disk {
        Some(disk) => {
            let ratio = disk.available_bytes as f64 / disk.total_bytes as f64;
            // Absolute space catches small volumes; percentage catches large ones.
            let severity = if disk.available_bytes < 5 * GIB || ratio < 0.05 {
                Some(Severity::Critical)
            } else if disk.available_bytes < 20 * GIB || ratio < 0.15 {
                Some(Severity::Warning)
            } else {
                None
            };
            match severity {
                Some(severity) => Observation::Abnormal(
                    severity,
                    format!(
                        "{} 可用 {:.2} GiB（{:.1}%）",
                        disk.path,
                        disk.available_bytes as f64 / GIB as f64,
                        ratio * 100.0
                    ),
                ),
                None => Observation::Clear,
            }
        }
        None => Observation::Unknown,
    };
    vec![
        Finding {
            id: "memory_pressure",
            observation: pressure,
            action: MEMORY_ACTION,
        },
        Finding {
            id: "swap_activity",
            observation: swap,
            action: SWAP_ACTION,
        },
        Finding {
            id: "cpu_saturation",
            observation: cpu,
            action: CPU_ACTION,
        },
        Finding {
            id: "disk_space",
            observation: disk,
            action: DISK_ACTION,
        },
    ]
}

/// Advice for the current sample alone; `watch` additionally applies sustained-state rules.
pub fn evaluate(snapshot: &Snapshot) -> Vec<Advice> {
    let findings = assess(snapshot);
    let mut advice: Vec<Advice> = findings
        .iter()
        .filter_map(|f| match &f.observation {
            Observation::Abnormal(severity, evidence) => Some(Advice {
                id: f.id,
                severity: *severity,
                evidence: evidence.clone(),
                action: f.action,
            }),
            _ => None,
        })
        .collect();
    let observed = |id| findings.iter().find(|f| f.id == id).map(|f| &f.observation);
    if observed("memory_pressure") == Some(&Observation::Unknown) {
        advice.push(Advice {
            id: "memory_unknown",
            severity: Severity::Info,
            evidence: "未能取得内核内存压力，不能判断是否存在内存瓶颈。".into(),
            action: "打开活动监视器 → 内存，人工确认内存压力曲线。",
        });
    }
    // Health is claimed only for the core signals that were actually measured.
    let core_clear = observed("memory_pressure") == Some(&Observation::Clear)
        && observed("disk_space") == Some(&Observation::Clear);
    if core_clear && advice.is_empty() {
        let unmeasured: Vec<&str> = ["swap_activity", "cpu_saturation"]
            .into_iter()
            .filter(|id| observed(id) == Some(&Observation::Unknown))
            .map(|id| {
                if id == "swap_activity" {
                    "交换读写"
                } else {
                    "CPU"
                }
            })
            .collect();
        let mut evidence =
            String::from("当前样本未见内存压力、交换频繁、CPU 饱和或启动卷空间不足。");
        if !unmeasured.is_empty() {
            evidence = format!(
                "当前样本未见内存压力或启动卷空间不足；{}本次未能测得。",
                unmeasured.join("、")
            );
        }
        advice.push(Advice {
            id: "no_observed_pressure",
            severity: Severity::Info,
            evidence,
            action: "若仍卡顿，在卡顿发生时再次采样或运行 watch 观察持续情况；当前工具尚未测量磁盘 I/O、温度或单个应用无响应，无需为了降低内存数字而清缓存。",
        });
    }
    advice
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::activity;
    use crate::model::{Activity, Disk, Memory, Swap};

    pub fn snapshot(pressure: Pressure, free: u64) -> Snapshot {
        Snapshot {
            unix_seconds: 0,
            platform: "test",
            memory: Memory {
                total_bytes: 16 * GIB,
                breakdown: None,
                swap: Some(Swap {
                    total_bytes: 12 * GIB,
                    used_bytes: 10 * GIB,
                }),
                pressure,
            },
            activity: ready(0.0, 20.0),
            disk: Some(Disk {
                path: "/data".into(),
                total_bytes: 1000 * GIB,
                available_bytes: free,
            }),
            processes_requested: false,
            top_processes: vec![],
            sources: vec![],
            limitations: vec![],
        }
    }

    pub fn ready(swap_bytes_per_second: f64, cpu: f64) -> Activity {
        Activity {
            status: RateStatus::Ready,
            interval_seconds: Some(30.0),
            swap_in_bytes_per_second: Some(0.0),
            swap_out_bytes_per_second: Some(swap_bytes_per_second),
            page_in_bytes_per_second: Some(0.0),
            cpu_busy_percent: Some(cpu),
        }
    }

    #[test]
    fn high_ram_and_historical_swap_do_not_trigger_cleanup() {
        let advice = evaluate(&snapshot(Pressure::Normal, 300 * GIB));
        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].id, "no_observed_pressure");
    }

    #[test]
    fn unknown_or_missing_metrics_never_report_healthy() {
        let mut sample = snapshot(Pressure::Unknown, 300 * GIB);
        assert!(evaluate(&sample)
            .iter()
            .all(|a| a.id != "no_observed_pressure"));
        sample.memory.pressure = Pressure::Normal;
        sample.disk = None;
        assert!(evaluate(&sample)
            .iter()
            .all(|a| a.id != "no_observed_pressure"));
    }

    #[test]
    fn warmup_rates_are_disclosed_not_assumed_clear() {
        let mut sample = snapshot(Pressure::Normal, 300 * GIB);
        sample.activity = activity::empty(RateStatus::Warmup);
        let advice = evaluate(&sample);
        assert_eq!(advice.len(), 1);
        assert!(advice[0].evidence.contains("未能测得"));
        assert!(assess(&sample)
            .iter()
            .filter(|f| f.id == "swap_activity" || f.id == "cpu_saturation")
            .all(|f| f.observation == Observation::Unknown));
    }

    #[test]
    fn swap_traffic_and_cpu_saturation_surface() {
        let mut sample = snapshot(Pressure::Normal, 300 * GIB);
        sample.activity = ready(2.0 * MIB, 95.0);
        let ids: Vec<_> = evaluate(&sample).iter().map(|a| a.id).collect();
        assert_eq!(ids, ["swap_activity", "cpu_saturation"]);
        sample.activity = ready(20.0 * MIB, 10.0);
        assert_eq!(evaluate(&sample)[0].severity, Severity::Critical);
    }

    #[test]
    fn pressure_and_disk_problems_both_surface() {
        let advice = evaluate(&snapshot(Pressure::Critical, 4 * GIB));
        assert_eq!(advice.len(), 2);
        assert!(advice.iter().all(|a| a.severity == Severity::Critical));
    }

    #[test]
    fn disk_percentage_boundaries_are_strict() {
        assert_eq!(
            evaluate(&snapshot(Pressure::Normal, 50 * GIB))[0].severity,
            Severity::Warning
        );
        assert_eq!(
            evaluate(&snapshot(Pressure::Normal, 150 * GIB))[0].id,
            "no_observed_pressure"
        );
        assert_eq!(
            evaluate(&snapshot(Pressure::Normal, 49 * GIB))[0].severity,
            Severity::Critical
        );
    }

    #[test]
    fn absolute_space_protects_small_volumes() {
        let mut sample = snapshot(Pressure::Normal, 19 * GIB);
        sample.disk.as_mut().unwrap().total_bytes = 100 * GIB;
        assert_eq!(evaluate(&sample)[0].severity, Severity::Warning);
        sample.disk.as_mut().unwrap().available_bytes = 4 * GIB;
        sample.disk.as_mut().unwrap().total_bytes = 20 * GIB;
        assert_eq!(evaluate(&sample)[0].severity, Severity::Critical);
    }
}
