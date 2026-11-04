//! The folder-size scan on the AppKit side: a cancellable background job whose progress the
//! main thread copies into the page while the popover is open.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use super::*;
use crate::space::{self, Report};

pub(super) struct SpaceJob {
    pub(super) job: Job<Result<Report, String>>,
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) progress: Arc<AtomicU64>,
}

impl Controller {
    pub(super) fn start_space(&self, root: String, now: u64) {
        let root = PathBuf::from(root);
        let guarded = space::guarded(self.ivars().monitor.borrow().space().home());
        if !self
            .ivars()
            .monitor
            .borrow_mut()
            .space_mut()
            .begin(root.clone(), now)
        {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU64::new(0));
        let (stop, count) = (Arc::clone(&cancel), Arc::clone(&progress));
        let job = Job::spawn("hostwatch-space", move || {
            space::scan(&root, &guarded, space::BUDGET, &stop, &count)
        });
        match job {
            Ok(job) => {
                *self.ivars().space.borrow_mut() = Some(SpaceJob {
                    job,
                    cancel,
                    progress,
                });
                self.poll_jobs();
            }
            Err(error) => self.ivars().monitor.borrow_mut().space_mut().finish(
                Err(tr!(
                    self.ui().lang,
                    format!("无法开始统计：{error}"),
                    format!("Could not start the scan: {error}")
                )),
                now,
            ),
        }
    }

    pub(super) fn cancel_space(&self) {
        if let Some(job) = self.ivars().space.borrow().as_ref() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Applies a finished scan, or copies progress into the page; returns whether to redraw.
    pub(super) fn collect_space(&self, wait: bool, now: u64) -> bool {
        let done = self
            .ivars()
            .space
            .borrow()
            .as_ref()
            .map(|j| wait || j.job.done());
        match done {
            None => false,
            Some(true) => {
                let Some(job) = self.ivars().space.borrow_mut().take() else {
                    return false;
                };
                let result = job.job.finish().unwrap_or_else(|| {
                    Err(tr!(
                        self.ui().lang,
                        "统计时出错，未得到结果",
                        "The scan failed without a result"
                    )
                    .into())
                });
                self.ivars()
                    .monitor
                    .borrow_mut()
                    .space_mut()
                    .finish(result, now);
                true
            }
            Some(false) => {
                let items = self
                    .ivars()
                    .space
                    .borrow()
                    .as_ref()
                    .map_or(0, |j| j.progress.load(Ordering::Relaxed));
                // Redraw about once a second, not on every 0.1 s poll.
                let ticks = self.ivars().space_ticks.get().wrapping_add(1);
                self.ivars().space_ticks.set(ticks);
                self.ivars()
                    .monitor
                    .borrow_mut()
                    .space_mut()
                    .progress(items);
                ticks % 10 == 0
            }
        }
    }
}

/// Opens a folder in Finder; never opens a file.
pub(super) fn reveal(lang: Lang, path: &str) -> Result<(), String> {
    if !std::path::Path::new(path).is_dir() {
        return Err(tr!(
            lang,
            format!("{path} 不是文件夹"),
            format!("{path} is not a folder")
        ));
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err(tr!(
            lang,
            format!("访达未能打开 {path}"),
            format!("Finder could not open {path}")
        ))
    }
}
