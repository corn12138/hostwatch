//! A short in-memory record of how much memory and processor each open app used, so an issue can
//! say which app was rising before it started. Nothing is written to disk.
//!
//! The menu-bar app adds a point at most every `EVERY` seconds; `CAPACITY` keeps about ten minutes.
//! Each point's processor figure covers the time since the previous point.

use std::collections::VecDeque;

use crate::optimize::Focus;

const MIB: u64 = 1024 * 1024;

pub const EVERY: u64 = 30;
pub const CAPACITY: usize = 21;
/// How far before an issue started to look.
pub const LOOKBACK: u64 = 300;
/// Rises smaller than this are ordinary fluctuation.
pub const MEMORY_RISE: u64 = 256 * MIB;
/// Average processor use, percent of one core, worth naming.
pub const CPU_BUSY: f64 = 25.0;
const NAMED: usize = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct AppUse {
    /// Bundle identifier, or the name when there is none; matches an app across points.
    pub key: String,
    pub name: String,
    pub footprint: u64,
    /// `None` on the first point, which has nothing to measure from.
    pub cpu_percent: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub at: u64,
    pub apps: Vec<AppUse>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Rise {
    /// `from` is `None` when the app was not open at the start of the window.
    Memory {
        name: String,
        from: Option<u64>,
        to: u64,
    },
    Cpu {
        name: String,
        percent: f64,
    },
}

/// What the trail says about the minutes before an issue started.
#[derive(Clone, Debug, PartialEq)]
pub enum Lead {
    /// Fewer than two points around the start.
    NotEnough,
    /// No open app rose enough to name, between `from` and `to`.
    Quiet { from: u64, to: u64 },
    Rose {
        from: u64,
        to: u64,
        rises: Vec<Rise>,
    },
}

#[derive(Default)]
pub struct Trail {
    points: VecDeque<Point>,
}

impl Trail {
    pub fn due(&self, now: u64) -> bool {
        self.points
            .back()
            .map_or(true, |p| now.saturating_sub(p.at) >= EVERY)
    }

    pub fn push(&mut self, point: Point) {
        if self.points.back().is_some_and(|p| p.at >= point.at) {
            return;
        }
        self.points.push_back(point);
        while self.points.len() > CAPACITY {
            self.points.pop_front();
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Compares the window from up to `LOOKBACK` seconds before `start` to the first point at or
    /// after it (or the newest point when the issue started after it).
    pub fn lead(&self, focus: Focus, start: u64) -> Lead {
        let floor = start.saturating_sub(LOOKBACK);
        let Some(base) = self.points.iter().find(|p| p.at >= floor && p.at <= start) else {
            return Lead::NotEnough;
        };
        let Some(end) = self
            .points
            .iter()
            .find(|p| p.at >= start && p.at > base.at)
            .or_else(|| self.points.back().filter(|p| p.at > base.at))
        else {
            return Lead::NotEnough;
        };
        let (from, to) = (base.at, end.at);
        let mut rises: Vec<(f64, Rise)> = match focus {
            Focus::Memory => end
                .apps
                .iter()
                .filter_map(|app| {
                    let before = base.apps.iter().find(|b| b.key == app.key);
                    let was = before.map_or(0, |b| b.footprint);
                    let grew = app.footprint.saturating_sub(was);
                    (grew >= MEMORY_RISE).then(|| {
                        (
                            grew as f64,
                            Rise::Memory {
                                name: app.name.clone(),
                                from: before.map(|b| b.footprint),
                                to: app.footprint,
                            },
                        )
                    })
                })
                .collect(),
            Focus::Cpu => {
                // Each point's figure covers the interval before it, so the window starts after base.
                let window: Vec<&Point> = self
                    .points
                    .iter()
                    .filter(|p| p.at > from && p.at <= to)
                    .collect();
                let measured = window
                    .iter()
                    .filter(|p| p.apps.iter().any(|a| a.cpu_percent.is_some()))
                    .count();
                if measured == 0 {
                    return Lead::NotEnough;
                }
                let mut keys: Vec<(&str, &str)> = Vec::new();
                for app in window.iter().flat_map(|p| &p.apps) {
                    if !keys.iter().any(|(k, _)| *k == app.key) {
                        keys.push((&app.key, &app.name));
                    }
                }
                keys.into_iter()
                    .filter_map(|(key, name)| {
                        // An app missing from a point used nothing then.
                        let total: f64 = window
                            .iter()
                            .flat_map(|p| p.apps.iter().filter(|a| a.key == key))
                            .filter_map(|a| a.cpu_percent)
                            .sum();
                        let percent = total / measured as f64;
                        (percent >= CPU_BUSY).then(|| {
                            (
                                percent,
                                Rise::Cpu {
                                    name: name.to_string(),
                                    percent,
                                },
                            )
                        })
                    })
                    .collect()
            }
        };
        if rises.is_empty() {
            return Lead::Quiet { from, to };
        }
        rises.sort_by(|a, b| b.0.total_cmp(&a.0));
        Lead::Rose {
            from,
            to,
            rises: rises.into_iter().take(NAMED).map(|(_, r)| r).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::advice::GIB;

    fn app(key: &str, footprint: u64, cpu: Option<f64>) -> AppUse {
        AppUse {
            key: key.into(),
            name: key.to_uppercase(),
            footprint,
            cpu_percent: cpu,
        }
    }

    fn point(at: u64, apps: Vec<AppUse>) -> Point {
        Point { at, apps }
    }

    #[test]
    fn records_at_most_every_interval_and_keeps_ten_minutes() {
        let mut trail = Trail::default();
        assert!(trail.due(0));
        trail.push(point(0, vec![]));
        assert!(!trail.due(EVERY - 1));
        assert!(trail.due(EVERY));
        trail.push(point(0, vec![]));
        assert_eq!(trail.len(), 1, "a point at the same time is ignored");
        for i in 1..100 {
            trail.push(point(i * EVERY, vec![]));
        }
        assert_eq!(trail.len(), CAPACITY);
    }

    #[test]
    fn memory_names_the_apps_that_grew_before_the_start() {
        let mut trail = Trail::default();
        trail.push(point(0, vec![app("old", 5 * GIB, None)]));
        trail.push(point(
            100,
            vec![app("browser", GIB, None), app("editor", GIB, None)],
        ));
        trail.push(point(
            130,
            vec![
                app("browser", 2 * GIB, Some(10.0)),
                app("editor", GIB, None),
            ],
        ));
        trail.push(point(
            400,
            vec![
                app("browser", 3 * GIB, Some(10.0)),
                app("editor", GIB + 100 * MIB, None),
                app("new", GIB, None),
            ],
        ));
        // Started at 390: the window runs from the first point within 5 minutes (100) to 400.
        assert_eq!(
            trail.lead(Focus::Memory, 390),
            Lead::Rose {
                from: 100,
                to: 400,
                rises: vec![
                    Rise::Memory {
                        name: "BROWSER".into(),
                        from: Some(GIB),
                        to: 3 * GIB
                    },
                    Rise::Memory {
                        name: "NEW".into(),
                        from: None,
                        to: GIB
                    },
                ],
            }
        );
        // Nothing within 5 minutes before the start.
        assert_eq!(trail.lead(Focus::Memory, 2000), Lead::NotEnough);
        // A single point is not a comparison.
        let mut one = Trail::default();
        one.push(point(0, vec![app("browser", GIB, None)]));
        assert_eq!(one.lead(Focus::Memory, 10), Lead::NotEnough);
    }

    #[test]
    fn processor_averages_the_window_and_ignores_brief_spikes() {
        let mut trail = Trail::default();
        trail.push(point(0, vec![app("encoder", 0, None)]));
        trail.push(point(
            30,
            vec![app("encoder", 0, Some(180.0)), app("spike", 0, Some(60.0))],
        ));
        trail.push(point(60, vec![app("encoder", 0, Some(160.0))]));
        trail.push(point(90, vec![app("encoder", 0, Some(170.0))]));
        match trail.lead(Focus::Cpu, 90) {
            Lead::Rose { from, to, rises } => {
                assert_eq!((from, to), (0, 90));
                // spike: 60 / 3 points = 20%, below the floor.
                assert_eq!(rises.len(), 1);
                assert!(
                    matches!(&rises[0], Rise::Cpu { name, percent } if name == "ENCODER" && (*percent - 170.0).abs() < 1e-9)
                );
            }
            other => panic!("{other:?}"),
        }
        let mut quiet = Trail::default();
        quiet.push(point(0, vec![app("a", 0, None)]));
        quiet.push(point(30, vec![app("a", 0, Some(5.0))]));
        assert_eq!(quiet.lead(Focus::Cpu, 30), Lead::Quiet { from: 0, to: 30 });
        // Only the unmeasured first point inside the window: no average to give.
        let mut first = Trail::default();
        first.push(point(0, vec![app("a", 0, None)]));
        first.push(point(30, vec![app("a", 0, None)]));
        assert_eq!(first.lead(Focus::Cpu, 30), Lead::NotEnough);
    }
}
