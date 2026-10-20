//! One-click optimization on the AppKit side: list the person's open apps, measure them in the
//! background, then send normal quit requests (like ⌘Q) to the apps the person confirmed.

use super::*;

/// Regular apps (the ones with a Dock icon) that are still running.
fn running_apps() -> Vec<RunningApp> {
    autoreleasepool(|_| {
        NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter(|app| {
                app.activationPolicy() == NSApplicationActivationPolicy::Regular
                    && !app.isTerminated()
                    && app.processIdentifier() > 0
            })
            .map(|app| RunningApp {
                pid: app.processIdentifier() as u32,
                name: app
                    .localizedName()
                    .map_or_else(String::new, |n| n.to_string()),
                bundle_id: app.bundleIdentifier().map(|id| id.to_string()),
                active: app.isActive(),
            })
            .collect()
    })
}

fn samples(table: &platform::ProcTable) -> Vec<optimize::ProcSample> {
    table
        .readable
        .iter()
        .map(|p| optimize::ProcSample {
            pid: p.pid,
            ppid: p.ppid,
            footprint: p.footprint,
            cpu_ns: p.cpu_ns,
        })
        .collect()
}

const QUIT_CHECKS: u32 = 4;
const QUIT_CHECK_SECONDS: f64 = 5.0;

/// Whether the process is gone; `kill` with signal 0 only checks existence.
fn exited(pid: u32) -> bool {
    // SAFETY: signal 0 sends nothing.
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

impl Controller {
    pub(super) fn start_plan(&self, group: Group, now: u64) {
        let Some(focus) = optimize::Focus::for_group(group) else {
            return;
        };
        if !self
            .ivars()
            .monitor
            .borrow_mut()
            .optimizer_mut()
            .begin(group, now)
        {
            return;
        }
        let apps = running_apps();
        let own = std::process::id();
        let job = Job::spawn("hostwatch-optimize", move || {
            let before = platform::process_table()?;
            let started = Instant::now();
            let after = if focus == optimize::Focus::Cpu {
                std::thread::sleep(Duration::from_millis(optimize::CPU_WINDOW_MS));
                platform::process_table()?
            } else {
                platform::ProcTable {
                    readable: before.readable.clone(),
                    listed: before.listed,
                }
            };
            let elapsed = started.elapsed().as_nanos() as u64;
            Ok(optimize::plan(
                focus,
                &apps,
                &samples(&before),
                &samples(&after),
                elapsed,
                own,
            ))
        });
        match job {
            Ok(job) => {
                *self.ivars().plan.borrow_mut() = Some(job);
                self.poll_jobs();
            }
            Err(error) => self
                .ivars()
                .monitor
                .borrow_mut()
                .optimizer_mut()
                .finish(Err(format!("无法开始分析：{error}")), now),
        }
    }

    pub(super) fn quit_selected(&self, now: u64) {
        let selected = self.ivars().monitor.borrow().optimizer().selected();
        if selected.is_empty() {
            return;
        }
        let outcomes: Vec<QuitOutcome> = autoreleasepool(|_| {
            selected
                .iter()
                .map(|candidate| {
                    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(
                        candidate.pid as libc::pid_t,
                    );
                    // A reused pid must not receive a request meant for another app.
                    let same = app.as_ref().is_some_and(|app| {
                        app.bundleIdentifier().map(|id| id.to_string()) == candidate.bundle_id
                            && !app.isTerminated()
                    });
                    let state = match app {
                        Some(app) if same && app.terminate() => QuitState::Requested,
                        _ => QuitState::NotSent,
                    };
                    QuitOutcome {
                        pid: candidate.pid,
                        name: candidate.name.clone(),
                        state,
                    }
                })
                .collect()
        });
        self.ivars()
            .monitor
            .borrow_mut()
            .optimizer_mut()
            .applied(outcomes, now);
        // Apps may save state or ask the person first; check back a few times, not continuously.
        self.ivars().quit_checks.set(QUIT_CHECKS);
        self.once(QUIT_CHECK_SECONDS, sel!(verifyQuit:));
    }

    pub(super) fn verify_quit_requests(&self) {
        if self
            .ivars()
            .monitor
            .borrow()
            .optimizer()
            .pending()
            .is_empty()
        {
            return;
        }
        let left = self.ivars().quit_checks.get().saturating_sub(1);
        self.ivars().quit_checks.set(left);
        self.ivars()
            .monitor
            .borrow_mut()
            .optimizer_mut()
            .verified(exited, left == 0);
        if left > 0
            && !self
                .ivars()
                .monitor
                .borrow()
                .optimizer()
                .pending()
                .is_empty()
        {
            self.once(QUIT_CHECK_SECONDS, sel!(verifyQuit:));
        }
        self.render(false);
    }
}
