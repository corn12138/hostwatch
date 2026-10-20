//! Startup-volume capacity, including the figure System Settings and Finder show as available.
//!
//! That figure adds purgeable space, which macOS computes through a system service that takes
//! hundreds of milliseconds. It is asked off the main thread at most every ten minutes; each
//! sample adds the last known purgeable amount to the cheap `statfs` free space.

use std::{
    ffi::CString,
    mem,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use objc2::rc::autoreleasepool;
use objc2_foundation::{
    NSArray, NSNumber, NSString, NSURLVolumeAvailableCapacityForImportantUsageKey, NSURL,
};

use crate::model::Disk;

const PURGEABLE_REFRESH: Duration = Duration::from_secs(600);

#[derive(Default)]
pub struct Purgeable {
    latest: Arc<Mutex<Option<u64>>>,
    worker: Option<JoinHandle<()>>,
    asked: Option<Instant>,
}

impl Purgeable {
    fn current(&mut self, path: &'static str) -> Option<u64> {
        let finished = self.worker.as_ref().map_or(true, JoinHandle::is_finished);
        if finished {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            if self
                .asked
                .map_or(true, |at| at.elapsed() >= PURGEABLE_REFRESH)
            {
                self.asked = Some(Instant::now());
                let latest = Arc::clone(&self.latest);
                self.worker = std::thread::Builder::new()
                    .name("hostwatch-purgeable".into())
                    .spawn(move || {
                        let free = statfs(path).ok().map(|(_, free)| free);
                        if let (Some(important), Some(free)) = (important_available(path), free) {
                            if let Ok(mut latest) = latest.lock() {
                                *latest = Some(important.saturating_sub(free));
                            }
                        }
                    })
                    .ok();
            }
        }
        self.latest.lock().ok().and_then(|latest| *latest)
    }
}

impl Drop for Purgeable {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// (total, free for a normal write) in bytes.
fn statfs(path: &str) -> Result<(u64, u64), String> {
    // APFS volumes share capacity. Report the writable Data volume once, without summing volumes.
    let c_path = CString::new(path).map_err(|e| e.to_string())?;
    // Darwin statfs uses 64-bit block counters to preserve large-volume capacity.
    let mut stats = mem::MaybeUninit::<libc::statfs>::uninit();
    // statfs initializes the output only on success; c_path is NUL-terminated and remains live.
    if unsafe { libc::statfs(c_path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let stats = unsafe { stats.assume_init() };
    let total_bytes = stats
        .f_blocks
        .checked_mul(u64::from(stats.f_bsize))
        .ok_or("磁盘总容量溢出")?;
    let available_bytes = stats
        .f_bavail
        .checked_mul(u64::from(stats.f_bsize))
        .ok_or("磁盘可用容量溢出")?;
    if total_bytes == 0 || available_bytes > total_bytes {
        return Err("磁盘返回无效容量".into());
    }
    Ok((total_bytes, available_bytes))
}

pub fn startup_disk(purgeable: &mut Purgeable) -> Result<Disk, String> {
    // APFS volumes share capacity. Report the writable Data volume once, without summing volumes.
    let path = if std::path::Path::new("/System/Volumes/Data").exists() {
        "/System/Volumes/Data"
    } else {
        "/"
    };
    let (total_bytes, available_bytes) = statfs(path)?;
    Ok(Disk {
        path: path.into(),
        total_bytes,
        available_bytes,
        important_available_bytes: purgeable
            .current(path)
            .map(|extra| (available_bytes + extra).min(total_bytes)),
    })
}

/// Free plus purgeable space, as Finder and System Settings report it; `None` if not reported.
fn important_available(path: &str) -> Option<u64> {
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        // SAFETY: a static Foundation resource-key constant.
        let key = unsafe { NSURLVolumeAvailableCapacityForImportantUsageKey };
        let values = url
            .resourceValuesForKeys_error(&NSArray::from_slice(&[key]))
            .ok()?;
        let number = values.objectForKey(key)?.downcast::<NSNumber>().ok()?;
        u64::try_from(number.longLongValue()).ok()
    })
}
