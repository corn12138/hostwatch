//! App lifecycle: status item, popover, sampling and UI timers, wake handling.

use super::*;

pub(super) type ScanJob = Job<Result<ProcessScan, String>>;
pub(super) type PlanJob = Job<Result<Vec<Candidate>, String>>;

pub(super) struct Ivars {
    pub(super) monitor: RefCell<Monitor>,
    pub(super) collector: RefCell<Box<dyn Collector>>,
    pub(super) status_item: OnceCell<Retained<NSStatusItem>>,
    pub(super) popover: OnceCell<Retained<NSPopover>>,
    pub(super) root: OnceCell<Retained<RootView>>,
    pub(super) scroll: OnceCell<Retained<NSScrollView>>,
    pub(super) timer: RefCell<Option<Retained<NSTimer>>>,
    pub(super) ui_timer: RefCell<Option<Retained<NSTimer>>>,
    pub(super) poll_timer: RefCell<Option<Retained<NSTimer>>>,
    pub(super) routes: RefCell<Vec<Route>>,
    /// Commands of the rendered page; a control's tag is its index + 1.
    pub(super) commands: RefCell<Vec<Command>>,
    pub(super) more_button: RefCell<Option<Retained<NSButton>>>,
    pub(super) status: RefCell<Option<(Tone, String)>>,
    pub(super) rendered: Cell<Option<(Health, Route)>>,
    pub(super) closed_at: Cell<Option<Instant>>,
    pub(super) scan: RefCell<Option<ScanJob>>,
    pub(super) plan: RefCell<Option<PlanJob>>,
    /// Checks left before pending quit requests are reported as still running.
    pub(super) quit_checks: Cell<u32>,
    /// Interface language, re-resolved whenever the popover opens.
    pub(super) ui: Cell<Ui>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and `Controller` does not implement Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "HostwatchController"]
    #[ivars = Ivars]
    pub(super) struct Controller;

    impl Controller {
        // SAFETY: NSTimer calls its selector with the timer as the only argument.
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            self.sample();
        }

        // SAFETY: as above; runs only while the popover is shown.
        #[unsafe(method(uiTick:))]
        fn ui_tick(&self, _timer: &NSTimer) {
            let health = self.ivars().monitor.borrow().health(now_unix());
            if self.ivars().rendered.get().map(|r| r.0) != Some(health) {
                self.render(false);
                self.refresh_status();
            }
        }

        // SAFETY: as above; runs only while a background job is in flight.
        #[unsafe(method(pollJobs:))]
        fn poll_jobs_tick(&self, _timer: &NSTimer) {
            self.collect_jobs(false);
        }

        // SAFETY: as above; a one-shot timer a few seconds after quit requests.
        #[unsafe(method(verifyQuit:))]
        fn verify_quit(&self, _timer: &NSTimer) {
            self.verify_quit_requests();
        }

        // SAFETY: controls send their action with themselves as the argument.
        #[unsafe(method(perform:))]
        fn perform(&self, sender: &NSControl) {
            let command = usize::try_from(sender.tag() - 1)
                .ok()
                .and_then(|i| self.ivars().commands.borrow().get(i).cloned());
            if let Some(command) = command {
                self.handle(command);
            }
        }

        // SAFETY: menu items send their action with themselves as the argument.
        #[unsafe(method(menuPerform:))]
        fn menu_perform(&self, item: &NSMenuItem) {
            match item.tag() {
                MENU_PROCESSES => self.handle(Command::Go(Route::Processes)),
                MENU_SETTINGS => self.handle(Command::Go(Route::Settings)),
                MENU_QUIT => self.handle(Command::Quit),
                _ => {}
            }
        }

        // SAFETY: the status bar button sends its action with itself as the argument.
        #[unsafe(method(toggle:))]
        fn toggle(&self, _sender: Option<&AnyObject>) {
            self.toggle_popover();
        }

        // SAFETY: notification observers receive the NSNotification.
        #[unsafe(method(didWake:))]
        fn did_wake(&self, _note: &NSNotification) {
            // Sample now so a wake does not leave old values up for a whole interval.
            if !self.ivars().monitor.borrow().paused() {
                self.sample();
                self.schedule();
            }
        }
    }

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSPopoverDelegate for Controller {
        // SAFETY: signature matches `-[NSPopoverDelegate popoverDidClose:]`.
        #[unsafe(method(popoverDidClose:))]
        fn popover_did_close(&self, _notification: &NSNotification) {
            self.closed();
        }
    }
);

impl Controller {
    pub(super) fn new(
        mtm: MainThreadMarker,
        collector: Box<dyn Collector>,
        interval: u64,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            monitor: RefCell::new(Monitor::new(interval)),
            collector: RefCell::new(collector),
            status_item: OnceCell::new(),
            popover: OnceCell::new(),
            root: OnceCell::new(),
            scroll: OnceCell::new(),
            timer: RefCell::new(None),
            ui_timer: RefCell::new(None),
            poll_timer: RefCell::new(None),
            routes: RefCell::new(vec![Route::Home]),
            commands: RefCell::new(Vec::new()),
            more_button: RefCell::new(None),
            status: RefCell::new(None),
            rendered: Cell::new(None),
            closed_at: Cell::new(None),
            scan: RefCell::new(None),
            plan: RefCell::new(None),
            quit_checks: Cell::new(0),
            ui: Cell::new(prefs::current_ui(prefs::load_choice())),
        });
        unsafe { msg_send![super(this), init] }
    }

    pub(super) fn install(this: &Retained<Self>) {
        let mtm = this.mtm();
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            // SAFETY: `toggle:` is defined on Controller, which lives for the whole app.
            unsafe {
                button.setTarget(Some(this));
                button.setAction(Some(sel!(toggle:)));
            }
            button.setImagePosition(NSCellImagePosition::ImageLeading);
        }
        let _ = this.ivars().status_item.set(item);

        let root = RootView::alloc(mtm).set_ivars(RootIvars {
            controller: OnceCell::new(),
        });
        let root: Retained<RootView> =
            unsafe { msg_send![super(root), initWithFrame: rect(0.0, 0.0, WIDTH, 200.0)] };
        let _ = root.ivars().controller.set(Weak::from_retained(this));
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, WIDTH, 160.0));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        root.addSubview(&scroll);
        let controller = NSViewController::new(mtm);
        controller.setView(&root);
        let popover = NSPopover::new(mtm);
        popover.setBehavior(NSPopoverBehavior::Transient);
        popover.setAnimates(false);
        popover.setContentViewController(Some(&controller));
        popover.setDelegate(Some(ProtocolObject::from_ref(&**this)));
        let _ = this.ivars().popover.set(popover);
        let _ = this.ivars().root.set(root);
        let _ = this.ivars().scroll.set(scroll);

        // SAFETY: `didWake:` takes the notification; the observer outlives the app run loop.
        unsafe {
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .addObserver_selector_name_object(
                    this,
                    sel!(didWake:),
                    Some(NSWorkspaceDidWakeNotification),
                    None,
                );
        }
        this.refresh_status();
        this.sample();
        this.schedule();
    }

    pub(super) fn sample(&self) {
        autoreleasepool(|_| {
            let result = self.ivars().collector.borrow_mut().collect(false);
            self.ivars().monitor.borrow_mut().record(result, now_unix());
            self.refresh_status();
            self.render(false);
        });
    }

    pub(super) fn refresh_status(&self) {
        let (tone, title) =
            panel::conclusion(&self.ivars().monitor.borrow(), now_unix(), self.ui());
        let status = (tone, title);
        if self.ivars().status.borrow().as_ref() == Some(&status) {
            return;
        }
        let mtm = self.mtm();
        let Some(button) = self.ivars().status_item.get().and_then(|i| i.button(mtm)) else {
            return;
        };
        let (tone, title) = &status;
        let name = match tone {
            Tone::Ok | Tone::Neutral => "gauge.with.dots.needle.33percent",
            Tone::Warning => "exclamationmark.triangle",
            Tone::Critical => "exclamationmark.octagon",
            Tone::Unknown => "questionmark.circle",
            Tone::Paused => "pause.circle",
            Tone::Recovering => "circle.lefthalf.filled",
            Tone::Loading => "circle.dotted",
        };
        let label = NSString::from_str(&tr!(
            self.ui().lang,
            format!("Hostwatch：{title}"),
            format!("Hostwatch: {title}")
        ));
        match NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(name),
            Some(&label),
        ) {
            Some(image) => {
                image.setTemplate(true);
                button.setImage(Some(&image));
                button.setTitle(ns_string!(""));
            }
            None => button.setTitle(ns_string!("HW")),
        }
        button.setToolTip(Some(&label));
        button.setAccessibilityLabel(Some(&label));
        let changed = self.ivars().status.borrow().is_some();
        *self.ivars().status.borrow_mut() = Some(status.clone());
        if changed && self.shown() {
            self.announce(&status.1);
        }
    }

    pub(super) fn announce(&self, text: &str) {
        let Some(root) = self.ivars().root.get() else {
            return;
        };
        let message = NSString::from_str(text);
        let priority = NSNumber::new_isize(NSAccessibilityPriorityLevel::High.0);
        let values: [&AnyObject; 2] = [message.as_ref(), priority.as_ref()];
        // SAFETY: the keys are AppKit's static accessibility user-info keys.
        let keys = unsafe { [NSAccessibilityAnnouncementKey, NSAccessibilityPriorityKey] };
        let info = NSDictionary::from_slices(&keys, &values);
        // SAFETY: `root` is a live view; the user info uses the documented keys and value types.
        unsafe {
            NSAccessibilityPostNotificationWithUserInfo(
                root,
                NSAccessibilityAnnouncementRequestedNotification,
                Some(&info),
            )
        };
    }

    pub(super) fn new_timer(
        &self,
        seconds: f64,
        selector: objc2::runtime::Sel,
    ) -> Retained<NSTimer> {
        self.add_timer(seconds, selector, true)
    }

    pub(super) fn once(&self, seconds: f64, selector: objc2::runtime::Sel) {
        self.add_timer(seconds, selector, false);
    }

    fn add_timer(
        &self,
        seconds: f64,
        selector: objc2::runtime::Sel,
        repeats: bool,
    ) -> Retained<NSTimer> {
        // SAFETY: the selector takes one object argument; the timer retains `self` until invalidated.
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                seconds, self, selector, None, repeats,
            )
        };
        // Lets macOS coalesce wakeups with other timers.
        timer.setTolerance(seconds * 0.1);
        // Common modes include event tracking, so an open popover or menu does not stop it.
        // SAFETY: the main run loop is used from the main thread; the mode is a system constant.
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        timer
    }

    /// (Re)starts the sampling timer for the current interval, or stops it while paused.
    pub(super) fn schedule(&self) {
        if let Some(timer) = self.ivars().timer.borrow_mut().take() {
            timer.invalidate();
        }
        let (paused, interval) = {
            let monitor = self.ivars().monitor.borrow();
            (monitor.paused(), monitor.interval())
        };
        if !paused {
            *self.ivars().timer.borrow_mut() = Some(self.new_timer(interval as f64, sel!(tick:)));
        }
    }

    pub(super) fn ui(&self) -> Ui {
        self.ivars().ui.get()
    }

    pub(super) fn shown(&self) -> bool {
        self.ivars().popover.get().is_some_and(|p| p.isShown())
    }

    pub(super) fn toggle_popover(&self) {
        let mtm = self.mtm();
        let Some(popover) = self.ivars().popover.get() else {
            return;
        };
        if popover.isShown() {
            popover.close();
            return;
        }
        // The click on the status item that dismissed a transient popover must not reopen it.
        if self
            .ivars()
            .closed_at
            .get()
            .is_some_and(|at| at.elapsed() < Duration::from_millis(300))
        {
            return;
        }
        *self.ivars().routes.borrow_mut() = vec![Route::Home];
        // A system language change takes effect the next time the panel opens.
        let ui = prefs::current_ui(self.ui().choice);
        if ui != self.ui() {
            self.ivars().ui.set(ui);
            self.refresh_status();
        }
        self.ivars().monitor.borrow_mut().clear_tool_feedback();
        self.render(true);
        let Some(button) = self.ivars().status_item.get().and_then(|i| i.button(mtm)) else {
            return;
        };
        #[allow(deprecated)]
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        popover.showRelativeToRect_ofView_preferredEdge(button.bounds(), &button, NSRectEdge::MinY);
        if let (Some(root), Some(window)) = (
            self.ivars().root.get(),
            self.ivars().root.get().and_then(|r| r.window()),
        ) {
            window.makeFirstResponder(Some(root));
        }
        *self.ivars().ui_timer.borrow_mut() = Some(self.new_timer(5.0, sel!(uiTick:)));
    }

    pub(super) fn closed(&self) {
        if let Some(timer) = self.ivars().ui_timer.borrow_mut().take() {
            timer.invalidate();
        }
        self.ivars().closed_at.set(Some(Instant::now()));
        self.ivars().rendered.set(None);
        self.ivars().commands.borrow_mut().clear();
        *self.ivars().more_button.borrow_mut() = None;
        if let Some(scroll) = self.ivars().scroll.get() {
            scroll.setDocumentView(None);
        }
        if let Some(root) = self.ivars().root.get() {
            for view in root.subviews().iter() {
                if Some(&*view) != self.ivars().scroll.get().map(|s| s.as_ref() as &NSView) {
                    view.removeFromSuperview();
                }
            }
        }
    }

    pub(super) fn escape(&self) {
        let depth = self.ivars().routes.borrow().len();
        if depth > 1 {
            self.handle(Command::Back);
        } else if let Some(popover) = self.ivars().popover.get() {
            popover.close();
        }
    }
}
