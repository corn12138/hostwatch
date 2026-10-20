//! What each control does. Commands change monitor state; none of them touches recovery.

use super::*;

impl Controller {
    pub(super) fn handle(&self, command: Command) {
        let now = now_unix();
        match command {
            Command::Go(route) => {
                let mut routes = self.ivars().routes.borrow_mut();
                if let Some(i) = routes.iter().position(|r| *r == route) {
                    routes.truncate(i + 1);
                } else {
                    routes.push(route);
                }
            }
            Command::Back => {
                let mut routes = self.ivars().routes.borrow_mut();
                if routes.len() > 1 {
                    routes.pop();
                }
            }
            Command::TogglePause => {
                let paused = {
                    let mut monitor = self.ivars().monitor.borrow_mut();
                    let paused = !monitor.paused();
                    monitor.set_paused(paused, now);
                    paused
                };
                if !paused {
                    // Fresh baseline: a rate spanning the whole pause would not describe "now".
                    match platform::collector() {
                        Ok(collector) => *self.ivars().collector.borrow_mut() = collector,
                        Err(error) => self.ivars().monitor.borrow_mut().record(Err(error), now),
                    }
                    self.schedule();
                    self.sample();
                } else {
                    self.schedule();
                }
                self.refresh_status();
            }
            Command::SetInterval(seconds) => {
                let changed = self.ivars().monitor.borrow_mut().set_interval(seconds, now);
                if changed {
                    self.schedule();
                }
            }
            Command::Open(tool) => {
                let result = open_tool(tool);
                self.ivars()
                    .monitor
                    .borrow_mut()
                    .note_open(tool, result, now);
            }
            Command::Retry => {
                self.schedule();
                self.sample();
                return;
            }
            Command::RefreshProcesses => self.start_scan(now),
            Command::More => {
                self.show_more();
                return;
            }
            Command::Quit => {
                self.quit();
                return;
            }
        }
        self.render(false);
    }

    pub(super) fn start_scan(&self, now: u64) {
        if !self.ivars().monitor.borrow_mut().begin_processes(now) {
            return;
        }
        let slot: ScanSlot = Arc::new(Mutex::new(None));
        let output = Arc::clone(&slot);
        let worker = std::thread::Builder::new()
            .name("hostwatch-processes".into())
            .spawn(move || {
                let result = platform::scan_processes();
                if let Ok(mut out) = output.lock() {
                    *out = Some(result);
                }
            });
        match worker {
            Ok(handle) => {
                *self.ivars().scan.borrow_mut() = Some((handle, slot));
                *self.ivars().poll_timer.borrow_mut() = Some(self.new_timer(0.1, sel!(pollScan:)));
            }
            Err(error) => self
                .ivars()
                .monitor
                .borrow_mut()
                .finish_processes(Err(format!("无法启动读取：{error}")), now),
        }
    }

    /// Takes a finished scan result; with `wait`, blocks until the worker ends (quit).
    pub(super) fn collect_scan(&self, wait: bool) {
        let finished = match self.ivars().scan.borrow().as_ref() {
            Some((handle, slot)) => {
                wait || handle.is_finished() || slot.lock().map_or(true, |s| s.is_some())
            }
            None => false,
        };
        if !finished {
            return;
        }
        let Some((handle, slot)) = self.ivars().scan.borrow_mut().take() else {
            return;
        };
        let _ = handle.join();
        if let Some(timer) = self.ivars().poll_timer.borrow_mut().take() {
            timer.invalidate();
        }
        let result = slot
            .lock()
            .ok()
            .and_then(|mut s| s.take())
            .unwrap_or_else(|| Err("读取进程时出错，未得到结果".into()));
        self.ivars()
            .monitor
            .borrow_mut()
            .finish_processes(result, now_unix());
        if !wait {
            self.render(false);
        }
    }

    pub(super) fn show_more(&self) {
        let mtm = self.mtm();
        let Some(button) = self.ivars().more_button.borrow().clone() else {
            return;
        };
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        for (title, tag, key) in [
            ("进程内存排行", MENU_PROCESSES, ""),
            ("设置与数据说明", MENU_SETTINGS, ","),
            ("", 0, ""),
            ("退出 Hostwatch", MENU_QUIT, "q"),
        ] {
            if tag == 0 {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
                continue;
            }
            // SAFETY: `menuPerform:` is defined on Controller, which outlives the menu.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(sel!(menuPerform:)),
                    &NSString::from_str(key),
                )
            };
            unsafe { item.setTarget(Some(self)) };
            item.setTag(tag);
            menu.addItem(&item);
        }
        let bounds = button.bounds();
        menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(0.0, bounds.size.height + 4.0),
            Some(&button),
        );
    }

    pub(super) fn quit(&self) {
        for timer in [
            &self.ivars().timer,
            &self.ivars().ui_timer,
            &self.ivars().poll_timer,
        ] {
            if let Some(timer) = timer.borrow_mut().take() {
                timer.invalidate();
            }
        }
        self.collect_scan(true);
        if let Some(popover) = self.ivars().popover.get() {
            popover.close();
        }
        if let Some(item) = self.ivars().status_item.get() {
            NSStatusBar::systemStatusBar().removeStatusItem(item);
        }
        NSApplication::sharedApplication(self.mtm()).terminate(None);
    }
}

/// particular pane is visible or a problem is fixed.
pub(super) fn open_tool(tool: Tool) -> Result<(), String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let url = match tool {
        Tool::ActivityMonitor => workspace
            .URLForApplicationWithBundleIdentifier(ns_string!("com.apple.ActivityMonitor"))
            .ok_or("未找到活动监视器（com.apple.ActivityMonitor）")?,
        Tool::StorageSettings => {
            NSURL::URLWithString(ns_string!(STORAGE_SETTINGS_URL)).ok_or("储存空间设置地址无效")?
        }
    };
    if workspace.openURL(&url) {
        Ok(())
    } else {
        Err(format!(
            "系统未能打开 {}",
            url.absoluteString()
                .map_or_else(String::new, |s| s.to_string())
        ))
    }
}

/// System Settings → General → Storage (macOS 13+).
pub(super) const STORAGE_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.settings.Storage";
