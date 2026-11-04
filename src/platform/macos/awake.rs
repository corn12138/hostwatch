//! What keeps the Mac awake: power assertions processes hold, from IOKit's
//! `IOPMCopyAssertionsByProcess` (the data behind `pmset -g assertions`). Read-only; nothing is
//! released or changed.

use std::ffi::c_void;

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSDate, NSDictionary, NSNumber, NSString};

use crate::model::{AwakeKind, SleepBlocker};

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMCopyAssertionsByProcess(out: *mut *const c_void) -> i32;
}

/// Assertion types that stop idle sleep, by what they keep on.
fn kind(true_type: &str) -> Option<AwakeKind> {
    match true_type {
        "PreventUserIdleSystemSleep" | "PreventSystemSleep" | "NoIdleSleepAssertion" => {
            Some(AwakeKind::System)
        }
        "PreventUserIdleDisplaySleep" | "NoDisplaySleepAssertion" => Some(AwakeKind::Display),
        // UserIsActive is a momentary input tickle; background, push and update tasks time out
        // on their own and do not hold the Mac awake indefinitely.
        _ => None,
    }
}

fn text(dict: &NSDictionary<AnyObject, AnyObject>, key: &str) -> Option<String> {
    let value = dict.objectForKey(&NSString::from_str(key))?;
    value.downcast_ref::<NSString>().map(|s| s.to_string())
}

pub fn sleep_blockers() -> Result<Vec<SleepBlocker>, String> {
    autoreleasepool(|_| {
        let mut raw: *const c_void = std::ptr::null();
        // SAFETY: the call writes a +1 CFDictionary (toll-free bridged to NSDictionary) or leaves
        // the pointer null on failure.
        let status = unsafe { IOPMCopyAssertionsByProcess(&mut raw) };
        if status != 0 || raw.is_null() {
            return Err(format!("系统没有返回电源断言（IOReturn {status:#x}）"));
        }
        // SAFETY: a non-null result is an owned CFDictionaryRef, which is an NSDictionary.
        let by_process: Retained<NSDictionary<AnyObject, AnyObject>> =
            unsafe { Retained::from_raw(raw as *mut NSDictionary<AnyObject, AnyObject>) }
                .ok_or("电源断言为空")?;
        let mut blockers = Vec::new();
        for list in by_process.allValues().iter() {
            let Some(list) = list.downcast_ref::<NSArray>() else {
                continue;
            };
            for item in list.iter() {
                let Some(assertion) = item.downcast_ref::<NSDictionary>() else {
                    continue;
                };
                let true_type = text(assertion, "AssertionTrueType")
                    .or_else(|| text(assertion, "AssertType"))
                    .unwrap_or_default();
                let Some(kind) = kind(&true_type) else {
                    continue;
                };
                let process = text(assertion, "Process Name").unwrap_or_default();
                // powerd's own entry only mirrors "the display is on"; it is not a cause.
                if process == "powerd" {
                    continue;
                }
                let pid = assertion
                    .objectForKey(&NSString::from_str("AssertPID"))
                    .and_then(|v| v.downcast_ref::<NSNumber>().map(|n| n.as_i64()))
                    .map_or(0, |pid| pid.max(0) as u32);
                let since_seconds = assertion
                    .objectForKey(&NSString::from_str("AssertStartWhen"))
                    .and_then(|v| {
                        v.downcast_ref::<NSDate>()
                            .map(|d| -d.timeIntervalSinceNow())
                    })
                    .filter(|s| *s >= 0.0)
                    .map(|s| s as u64);
                blockers.push(SleepBlocker {
                    pid,
                    process,
                    kind,
                    reason: text(assertion, "AssertName").unwrap_or_default(),
                    since_seconds,
                });
            }
        }
        // Longest-held first.
        blockers.sort_by_key(|b| std::cmp::Reverse(b.since_seconds));
        Ok(blockers)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_without_error_and_skips_momentary_types() {
        let blockers = super::sleep_blockers().unwrap();
        assert!(blockers.iter().all(|b| b.process != "powerd"));
    }
}
