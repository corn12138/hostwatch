use std::time::Duration;

use crate::model::{Activity, RateStatus};

/// Cumulative kernel counters captured in one sample. Page counts use `page_size` bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counters {
    pub page_size: u64,
    pub swap_ins: u64,
    pub swap_outs: u64,
    pub page_ins: u64,
    /// user, system, idle, nice ticks summed across CPUs; 32-bit kernel counters that may wrap.
    pub cpu_ticks: Option<[u32; 4]>,
}

/// A counter sample with both clocks. `monotonic` must exclude sleep (Rust's `Instant` on macOS).
#[derive(Clone, Copy, Debug)]
pub struct Stamp {
    pub monotonic: Duration,
    pub wall: Duration,
}

/// Extra wall time beyond monotonic time that marks a sleep or clock jump.
const SLEEP_TOLERANCE: Duration = Duration::from_secs(5);

pub fn empty(status: RateStatus) -> Activity {
    Activity {
        status,
        interval_seconds: None,
        swap_in_bytes_per_second: None,
        swap_out_bytes_per_second: None,
        page_in_bytes_per_second: None,
        cpu_busy_percent: None,
    }
}

pub fn derive(previous: Option<(Stamp, Counters)>, current: Option<(Stamp, Counters)>) -> Activity {
    let (Some((before, old)), Some((now, new))) = (previous, current) else {
        return empty(if current.is_some() {
            RateStatus::Warmup
        } else {
            RateStatus::Unavailable
        });
    };
    let Some(elapsed) = now.monotonic.checked_sub(before.monotonic) else {
        return empty(RateStatus::CounterReset);
    };
    let wall = now.wall.saturating_sub(before.wall);
    // Rates spanning sleep would be averaged over time the machine was not running.
    if wall > elapsed + SLEEP_TOLERANCE.max(elapsed / 10) || now.wall < before.wall {
        return empty(RateStatus::SleepGap);
    }
    if new.page_size != old.page_size
        || new.swap_ins < old.swap_ins
        || new.swap_outs < old.swap_outs
        || new.page_ins < old.page_ins
    {
        return empty(RateStatus::CounterReset);
    }
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return empty(RateStatus::Warmup);
    }
    let rate = |a: u64, b: u64| ((b - a) * new.page_size) as f64 / seconds;
    Activity {
        status: RateStatus::Ready,
        interval_seconds: Some(seconds),
        swap_in_bytes_per_second: Some(rate(old.swap_ins, new.swap_ins)),
        swap_out_bytes_per_second: Some(rate(old.swap_outs, new.swap_outs)),
        page_in_bytes_per_second: Some(rate(old.page_ins, new.page_ins)),
        cpu_busy_percent: match (old.cpu_ticks, new.cpu_ticks) {
            (Some(a), Some(b)) => cpu_busy(a, b),
            _ => None,
        },
    }
}

fn cpu_busy(old: [u32; 4], new: [u32; 4]) -> Option<f64> {
    // Tick counters are 32-bit and wrap on long uptimes; a single wrap is recovered by wrapping_sub.
    let delta: Vec<u64> = old
        .iter()
        .zip(new)
        .map(|(a, b)| u64::from(b.wrapping_sub(*a)))
        .collect();
    let total: u64 = delta.iter().sum();
    if total == 0 {
        return None;
    }
    let idle = delta[2];
    Some((total - idle) as f64 / total as f64 * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(mono: u64, wall: u64) -> Stamp {
        Stamp {
            monotonic: Duration::from_secs(mono),
            wall: Duration::from_secs(1_000_000 + wall),
        }
    }

    fn counters(swap_outs: u64, ticks: [u32; 4]) -> Counters {
        Counters {
            page_size: 16384,
            swap_ins: 0,
            swap_outs,
            page_ins: 0,
            cpu_ticks: Some(ticks),
        }
    }

    #[test]
    fn first_sample_is_warmup_and_missing_source_is_unavailable() {
        let c = counters(0, [0; 4]);
        assert_eq!(
            derive(None, Some((stamp(0, 0), c))).status,
            RateStatus::Warmup
        );
        assert_eq!(
            derive(Some((stamp(0, 0), c)), None).status,
            RateStatus::Unavailable
        );
    }

    #[test]
    fn rates_use_monotonic_interval() {
        let a = (stamp(0, 0), counters(0, [0, 0, 0, 0]));
        let b = (stamp(10, 10), counters(640, [30, 20, 50, 0]));
        let activity = derive(Some(a), Some(b));
        assert_eq!(activity.status, RateStatus::Ready);
        assert_eq!(
            activity.swap_out_bytes_per_second,
            Some(640.0 * 16384.0 / 10.0)
        );
        assert_eq!(activity.cpu_busy_percent, Some(50.0));
    }

    #[test]
    fn sleep_gap_and_counter_reset_produce_no_rates() {
        let a = (stamp(0, 0), counters(100, [0; 4]));
        let slept = derive(Some(a), Some((stamp(30, 3600), counters(200, [0; 4]))));
        assert_eq!(slept.status, RateStatus::SleepGap);
        assert!(slept.swap_out_bytes_per_second.is_none());
        let reset = derive(Some(a), Some((stamp(30, 30), counters(5, [0; 4]))));
        assert_eq!(reset.status, RateStatus::CounterReset);
        let clock_back = (stamp(0, 60), counters(100, [0; 4]));
        let backwards = derive(
            Some(clock_back),
            Some((stamp(30, 0), counters(200, [0; 4]))),
        );
        assert_eq!(backwards.status, RateStatus::SleepGap);
    }

    #[test]
    fn cpu_tick_wrap_is_recovered() {
        let a = (stamp(0, 0), counters(0, [u32::MAX - 9, 0, u32::MAX - 9, 0]));
        let b = (stamp(1, 1), counters(0, [10, 0, 10, 0]));
        assert_eq!(derive(Some(a), Some(b)).cpu_busy_percent, Some(50.0));
    }
}
