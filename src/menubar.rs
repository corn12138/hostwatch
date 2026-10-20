//! Native AppKit shell: one `NSStatusItem` with a transient `NSPopover`; no WebView, no helper
//! process.
//!
//! AppKit work stays on the main thread. Sampling is an `NSTimer` in the common run-loop modes, so
//! it keeps firing while the popover or its More menu is open. Popover views are rebuilt from
//! `panel::page` only while the popover is shown (new sample, navigation or command); closing it
//! drops them and monitoring continues. Pausing invalidates the timer; resuming starts a fresh
//! collector so rates get a new baseline. The on-demand process scan runs on one short-lived worker
//! thread that the main thread polls and joins, including at quit. Actions only open read-only
//! system tools (Activity Monitor, Storage settings).

use std::cell::{Cell, OnceCell, RefCell};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use objc2::rc::{autoreleasepool, Retained, Weak};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityAnnouncementKey,
    NSAccessibilityAnnouncementRequestedNotification, NSAccessibilityImageRole,
    NSAccessibilityPostNotificationWithUserInfo, NSAccessibilityPriorityKey,
    NSAccessibilityPriorityLevel, NSApplication, NSApplicationActivationPolicy, NSBezierPath,
    NSButton, NSCellImagePosition, NSColor, NSControl, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSFontAttributeName, NSFontWeight, NSFontWeightMedium,
    NSFontWeightRegular, NSFontWeightSemibold, NSForegroundColorAttributeName, NSImage,
    NSImageScaling, NSImageSymbolConfiguration, NSImageView, NSLineBreakMode, NSMenu, NSMenuItem,
    NSPopover, NSPopoverBehavior, NSPopoverDelegate, NSResponder, NSScreen, NSScrollView,
    NSStatusBar, NSStatusItem, NSStringDrawing, NSTextAlignment, NSTextField,
    NSVariableStatusItemLength, NSView, NSViewController, NSWorkspace,
    NSWorkspaceDidWakeNotification,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSAttributedStringKey, NSDictionary, NSNotification, NSNumber,
    NSObject, NSObjectProtocol, NSPoint, NSRect, NSRectEdge, NSRunLoop, NSRunLoopCommonModes,
    NSSize, NSString, NSTimer, NSURL,
};

use crate::model::ProcessScan;
use crate::monitor::{Health, Monitor, Tool};
use crate::panel::{self, Block, Button, Chart, Command, Footer, Graphic, ProcessRow, Route, Tone};
use crate::platform::{self, Collector};

const WIDTH: f64 = 440.0;
const PAD: f64 = 16.0;
const GAP: f64 = 12.0;
const FOOTER: f64 = 40.0;
const MENU_PROCESSES: isize = 9001;
const MENU_SETTINGS: isize = 9002;
const MENU_QUIT: isize = 9003;

type ScanSlot = Arc<Mutex<Option<Result<ProcessScan, String>>>>;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// ---------------------------------------------------------------- drawing primitives

define_class!(
    // SAFETY: NSView subclass with no extra requirements; `Plain` does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HostwatchPlainView"]
    struct Plain;

    impl Plain {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl Plain {
    /// A non-drawing flipped container, so it gets no backing store of its own.
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

#[derive(Clone, Debug)]
enum Shape {
    /// Rounded group background; a tone adds a status-colored border.
    Card(Option<Tone>),
    Chart(Box<Chart>, bool),
    Bar(f64, Tone),
    Rule,
    Boxes(u32, u32),
}

define_class!(
    // SAFETY: NSView subclass with no extra requirements; `ShapeView` does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HostwatchShapeView"]
    #[ivars = RefCell<Shape>]
    struct ShapeView;

    impl ShapeView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        // SAFETY: signature matches `-[NSView drawRect:]`.
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            draw(&self.ivars().borrow(), self.bounds());
        }
    }
);

impl ShapeView {
    fn new(mtm: MainThreadMarker, shape: Shape, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RefCell::new(shape));
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

struct RootIvars {
    controller: OnceCell<Weak<Controller>>,
}

define_class!(
    // SAFETY: NSView subclass with no extra requirements; `RootView` does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HostwatchRootView"]
    #[ivars = RootIvars]
    struct RootView;

    impl RootView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        // SAFETY: Escape is delivered as `cancelOperation:` through the responder chain.
        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            if let Some(controller) = self.ivars().controller.get().and_then(Weak::load) {
                controller.escape();
            }
        }
    }
);

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

fn tone_color(tone: Tone) -> Retained<NSColor> {
    match tone {
        Tone::Ok | Tone::Recovering => NSColor::systemGreenColor(),
        Tone::Warning => NSColor::systemOrangeColor(),
        Tone::Critical => NSColor::systemRedColor(),
        Tone::Loading => NSColor::systemBlueColor(),
        Tone::Unknown | Tone::Paused | Tone::Neutral => NSColor::secondaryLabelColor(),
    }
}

fn symbol(tone: Tone) -> &'static str {
    match tone {
        Tone::Ok => "checkmark.circle.fill",
        Tone::Warning => "exclamationmark.triangle.fill",
        Tone::Critical => "exclamationmark.octagon.fill",
        Tone::Unknown => "questionmark.circle",
        Tone::Paused => "pause.circle.fill",
        Tone::Recovering => "circle.lefthalf.filled",
        Tone::Loading => "circle.dotted",
        Tone::Neutral => "info.circle",
    }
}

fn text_attributes(
    size: f64,
    color: &NSColor,
) -> Retained<NSDictionary<NSAttributedStringKey, AnyObject>> {
    let font = NSFont::systemFontOfSize(size);
    let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
    // SAFETY: AppKit string attribute keys are immutable static strings.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    NSDictionary::from_slices(&keys, &values)
}

/// Draws `text` with its left, center or right edge at `x` (`align` = 0, 1, 2).
fn draw_text(text: &str, x: f64, y: f64, size: f64, color: &NSColor, align: u8) {
    let string = NSString::from_str(text);
    let attributes = text_attributes(size, color);
    // SAFETY: the attribute dictionary holds an NSFont and an NSColor under the matching keys.
    unsafe {
        let width = string.sizeWithAttributes(Some(&attributes)).width;
        let x = match align {
            0 => x,
            1 => x - width / 2.0,
            _ => x - width,
        };
        string.drawAtPoint_withAttributes(NSPoint::new(x, y), Some(&attributes));
    }
}

fn dashed(path: &NSBezierPath) {
    let pattern = [4.0f64, 3.0];
    // SAFETY: the pattern pointer is valid for `count` elements during the call.
    unsafe { path.setLineDash_count_phase(pattern.as_ptr(), 2, 0.0) };
}

fn draw(shape: &Shape, bounds: NSRect) {
    let (w, h) = (bounds.size.width, bounds.size.height);
    match shape {
        Shape::Card(tone) => {
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.5, 0.5, w - 1.0, h - 1.0),
                8.0,
                8.0,
            );
            NSColor::controlBackgroundColor()
                .colorWithAlphaComponent(0.7)
                .setFill();
            path.fill();
            match tone {
                Some(Tone::Neutral) | None => NSColor::separatorColor().setStroke(),
                Some(tone) => tone_color(*tone).setStroke(),
            }
            path.setLineWidth(1.0);
            path.stroke();
        }
        Shape::Rule => {
            NSColor::separatorColor().setFill();
            NSBezierPath::fillRect(rect(0.0, (h / 2.0).floor(), w, 1.0));
        }
        Shape::Bar(fraction, tone) => {
            let radius = h / 2.0;
            NSColor::quaternaryLabelColor().setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.0, 0.0, w, h),
                radius,
                radius,
            )
            .fill();
            match tone {
                Tone::Neutral => NSColor::secondaryLabelColor().setFill(),
                tone => tone_color(*tone).setFill(),
            }
            let filled = (w * fraction.clamp(0.0, 1.0)).max(if *fraction > 0.0 { h } else { 0.0 });
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.0, 0.0, filled, h),
                radius,
                radius,
            )
            .fill();
        }
        Shape::Boxes(done, total) => {
            for i in 0..*total {
                let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                    rect(f64::from(i) * 24.0 + 0.5, 0.5, 19.0, h - 1.0),
                    2.0,
                    2.0,
                );
                if i < *done {
                    NSColor::systemGreenColor().setFill();
                    path.fill();
                } else {
                    NSColor::tertiaryLabelColor().setStroke();
                    path.stroke();
                }
            }
        }
        Shape::Chart(chart, compact) => draw_chart(chart, *compact, w, h),
    }
}

fn draw_chart(chart: &Chart, compact: bool, w: f64, h: f64) {
    let (left, right, top, bottom) = if compact {
        (1.0, 3.0, 2.0, 2.0)
    } else {
        (28.0, 6.0, 6.0, 16.0)
    };
    let (pw, ph) = (w - left - right, h - top - bottom);
    let secondary = NSColor::secondaryLabelColor();
    if let Some(empty) = &chart.empty {
        if !compact {
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.5, 0.5, w - 1.0, h - 1.0),
                6.0,
                6.0,
            );
            dashed(&path);
            NSColor::tertiaryLabelColor().setStroke();
            path.stroke();
            draw_text(empty, w / 2.0, h / 2.0 - 8.0, 11.0, &secondary, 1);
        }
        return;
    }
    let span = chart.t1.saturating_sub(chart.t0) as f64;
    let x = |t: u64| {
        if span <= 0.0 {
            left + pw / 2.0
        } else {
            left + (t.saturating_sub(chart.t0) as f64 / span) * pw
        }
    };
    let y = |v: f64| top + ph - (v / chart.y_max).clamp(0.0, 1.0) * ph;
    for gap in &chart.gaps {
        let (x0, x1) = (x(gap.from), x(gap.to));
        NSColor::quaternaryLabelColor()
            .colorWithAlphaComponent(0.35)
            .setFill();
        NSBezierPath::fillRect(rect(x0, top, (x1 - x0).max(1.0), ph));
        if !compact && x1 - x0 >= 26.0 {
            draw_text(gap.label, (x0 + x1) / 2.0, top + 2.0, 10.0, &secondary, 1);
        }
    }
    if !compact {
        for (value, label) in &chart.y_ticks {
            NSColor::separatorColor().setFill();
            NSBezierPath::fillRect(rect(left, y(*value).floor(), pw, 1.0));
            draw_text(label, left - 4.0, y(*value) - 7.0, 10.0, &secondary, 2);
        }
        for (i, (t, label)) in chart.x_labels.iter().enumerate() {
            let align = if i == 0 { 0 } else { 2 };
            draw_text(label, x(*t), h - 13.0, 10.0, &secondary, align);
        }
    }
    for (value, label) in &chart.thresholds {
        if compact || *value > chart.y_max {
            continue;
        }
        let path = NSBezierPath::bezierPath();
        path.moveToPoint(NSPoint::new(left, y(*value)));
        path.lineToPoint(NSPoint::new(left + pw, y(*value)));
        path.setLineWidth(1.0);
        dashed(&path);
        let color = if *value >= 16.0 {
            NSColor::systemRedColor()
        } else {
            NSColor::systemOrangeColor()
        };
        color.setStroke();
        path.stroke();
        if !compact {
            draw_text(label, left + pw - 2.0, y(*value) - 13.0, 10.0, &color, 2);
        }
    }
    let line = if chart.title.starts_with("交换") {
        NSColor::systemOrangeColor()
    } else {
        NSColor::systemBlueColor()
    };
    line.setStroke();
    line.setFill();
    for segment in &chart.segments {
        if segment.len() == 1 {
            let (t, v) = segment[0];
            NSBezierPath::bezierPathWithOvalInRect(rect(x(t) - 2.0, y(v) - 2.0, 4.0, 4.0)).fill();
            continue;
        }
        let path = NSBezierPath::bezierPath();
        for (i, (t, v)) in segment.iter().enumerate() {
            let point = NSPoint::new(x(*t), y(*v));
            if i == 0 {
                path.moveToPoint(point);
            } else {
                path.lineToPoint(point);
            }
        }
        path.setLineWidth(if compact { 1.2 } else { 1.6 });
        path.stroke();
    }
}

// ---------------------------------------------------------------- controller

struct Ivars {
    monitor: RefCell<Monitor>,
    collector: RefCell<Box<dyn Collector>>,
    status_item: OnceCell<Retained<NSStatusItem>>,
    popover: OnceCell<Retained<NSPopover>>,
    root: OnceCell<Retained<RootView>>,
    scroll: OnceCell<Retained<NSScrollView>>,
    timer: RefCell<Option<Retained<NSTimer>>>,
    ui_timer: RefCell<Option<Retained<NSTimer>>>,
    poll_timer: RefCell<Option<Retained<NSTimer>>>,
    routes: RefCell<Vec<Route>>,
    /// Commands of the rendered page; a control's tag is its index + 1.
    commands: RefCell<Vec<Command>>,
    more_button: RefCell<Option<Retained<NSButton>>>,
    status: RefCell<Option<(Tone, String)>>,
    rendered: Cell<Option<(Health, Route)>>,
    closed_at: Cell<Option<Instant>>,
    scan: RefCell<Option<(JoinHandle<()>, ScanSlot)>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and `Controller` does not implement Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "HostwatchController"]
    #[ivars = Ivars]
    struct Controller;

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

        // SAFETY: as above; runs only while a process scan is in flight.
        #[unsafe(method(pollScan:))]
        fn poll_scan(&self, _timer: &NSTimer) {
            self.collect_scan(false);
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
    fn new(mtm: MainThreadMarker, collector: Box<dyn Collector>, interval: u64) -> Retained<Self> {
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
        });
        unsafe { msg_send![super(this), init] }
    }

    fn install(this: &Retained<Self>) {
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

    fn sample(&self) {
        autoreleasepool(|_| {
            let result = self.ivars().collector.borrow_mut().collect(false);
            self.ivars().monitor.borrow_mut().record(result, now_unix());
            self.refresh_status();
            self.render(false);
        });
    }

    fn refresh_status(&self) {
        let (tone, title) = panel::conclusion(&self.ivars().monitor.borrow(), now_unix());
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
        let label = NSString::from_str(&format!("Hostwatch：{title}"));
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

    fn announce(&self, text: &str) {
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

    fn new_timer(&self, seconds: f64, selector: objc2::runtime::Sel) -> Retained<NSTimer> {
        // SAFETY: the selector takes one object argument; the timer retains `self` until invalidated.
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                seconds, self, selector, None, true,
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
    fn schedule(&self) {
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

    fn shown(&self) -> bool {
        self.ivars().popover.get().is_some_and(|p| p.isShown())
    }

    fn toggle_popover(&self) {
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

    fn closed(&self) {
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

    fn escape(&self) {
        let depth = self.ivars().routes.borrow().len();
        if depth > 1 {
            self.handle(Command::Back);
        } else if let Some(popover) = self.ivars().popover.get() {
            popover.close();
        }
    }

    fn handle(&self, command: Command) {
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

    fn start_scan(&self, now: u64) {
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
    fn collect_scan(&self, wait: bool) {
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

    fn show_more(&self) {
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

    fn quit(&self) {
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

    // ------------------------------------------------------------ rendering

    fn render(&self, force: bool) {
        if !force && !self.shown() {
            return;
        }
        let (Some(root), Some(scroll), Some(popover)) = (
            self.ivars().root.get(),
            self.ivars().scroll.get(),
            self.ivars().popover.get(),
        ) else {
            return;
        };
        autoreleasepool(|_| {
            let mtm = self.mtm();
            let now = now_unix();
            let (route, back) = {
                let routes = self.ivars().routes.borrow();
                let route = *routes.last().unwrap_or(&Route::Home);
                let back = routes.len().checked_sub(2).map(|i| routes[i]);
                (route, back)
            };
            let (page, health) = {
                let monitor = self.ivars().monitor.borrow();
                (panel::page(&monitor, route, back, now), monitor.health(now))
            };
            let focused = self.focused_command(root);
            let same_route = self.ivars().rendered.get().map(|r| r.1) == Some(route);
            let offset = scroll.contentView().bounds().origin;
            self.ivars().commands.borrow_mut().clear();

            let doc = Plain::new(mtm, rect(0.0, 0.0, WIDTH, 10.0));
            let mut y = 14.0;
            let mut blocks = page.blocks.iter().peekable();
            while let Some(block) = blocks.next() {
                let height = if let Block::Resource { .. } = block {
                    let mut rows = vec![block];
                    while let Some(next @ Block::Resource { .. }) = blocks.peek() {
                        rows.push(next);
                        blocks.next();
                    }
                    self.resource_group(&doc, &rows, y)
                } else {
                    self.block(&doc, block, PAD, y, WIDTH - 2.0 * PAD)
                };
                y += height + GAP;
            }
            let content = y - GAP + 14.0;
            doc.setFrameSize(NSSize::new(WIDTH, content));

            let screen = NSScreen::mainScreen(mtm).map_or(800.0, |s| s.visibleFrame().size.height);
            let visible = content.min((screen - 40.0 - FOOTER).max(240.0));
            scroll.setFrame(rect(0.0, 0.0, WIDTH, visible));
            scroll.setDocumentView(Some(&doc));
            if same_route {
                doc.scrollPoint(offset);
            } else {
                doc.scrollPoint(NSPoint::new(0.0, 0.0));
            }
            for view in root.subviews().iter() {
                if &*view != scroll.as_ref() as &NSView {
                    view.removeFromSuperview();
                }
            }
            let footer = self.footer(&page.footer, visible);
            root.addSubview(&footer);
            let size = NSSize::new(WIDTH, visible + FOOTER);
            root.setFrameSize(size);
            popover.setContentSize(size);
            self.restore_focus(root, focused);
            self.ivars().rendered.set(Some((health, route)));
        });
    }

    fn focused_command(&self, root: &RootView) -> Option<Command> {
        let responder = root.window()?.firstResponder()?;
        let control = responder.downcast_ref::<NSControl>()?;
        let index = usize::try_from(control.tag() - 1).ok()?;
        self.ivars().commands.borrow().get(index).cloned()
    }

    fn restore_focus(&self, root: &RootView, command: Option<Command>) {
        let Some(window) = root.window() else {
            return;
        };
        let index =
            command.and_then(|c| self.ivars().commands.borrow().iter().position(|x| *x == c));
        match index.and_then(|i| root.viewWithTag(i as isize + 1)) {
            Some(view) => {
                window.makeFirstResponder(Some(&view));
            }
            None => {
                if window.firstResponder().is_none() {
                    window.makeFirstResponder(Some(root));
                }
            }
        }
    }

    fn tag(&self, command: &Command) -> isize {
        let mut commands = self.ivars().commands.borrow_mut();
        commands.push(command.clone());
        commands.len() as isize
    }

    fn label(
        &self,
        text: &str,
        size: f64,
        weight: NSFontWeight,
        color: &NSColor,
        width: f64,
        wrap: bool,
    ) -> (Retained<NSTextField>, f64) {
        let mtm = self.mtm();
        let string = NSString::from_str(text);
        let field = if wrap {
            NSTextField::wrappingLabelWithString(&string, mtm)
        } else {
            NSTextField::labelWithString(&string, mtm)
        };
        field.setFont(Some(&NSFont::systemFontOfSize_weight(size, weight)));
        field.setTextColor(Some(color));
        field.setSelectable(false);
        if wrap {
            field.setPreferredMaxLayoutWidth(width);
        } else {
            field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        }
        let height = field
            .sizeThatFits(NSSize::new(width, 10_000.0))
            .height
            .ceil();
        (field, height)
    }

    fn put(&self, parent: &NSView, view: &NSView, x: f64, y: f64, w: f64, h: f64) {
        view.setFrame(rect(x, y, w, h));
        parent.addSubview(view);
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        parent: &NSView,
        text: &str,
        size: f64,
        weight: NSFontWeight,
        color: &NSColor,
        x: f64,
        y: f64,
        w: f64,
    ) -> f64 {
        let (field, h) = self.label(text, size, weight, color, w, true);
        self.put(parent, &field, x, y, w, h);
        h
    }

    fn icon(&self, parent: &NSView, tone: Tone, x: f64, y: f64, size: f64) {
        let mtm = self.mtm();
        let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol(tone)),
            Some(&NSString::from_str(tone.label())),
        ) else {
            return;
        };
        let configuration =
            NSImageSymbolConfiguration::configurationWithPointSize_weight(size, unsafe {
                NSFontWeightRegular
            });
        let view = NSImageView::imageViewWithImage(&image, mtm);
        view.setSymbolConfiguration(Some(&configuration));
        view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        view.setContentTintColor(Some(&tone_color(tone)));
        self.put(parent, &view, x, y, size + 2.0, size + 2.0);
    }

    fn button(&self, button: &Button) -> Retained<NSButton> {
        let mtm = self.mtm();
        // SAFETY: `perform:` is defined on Controller, which lives for the whole app.
        let control = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(&button.title),
                Some(self),
                Some(sel!(perform:)),
                mtm,
            )
        };
        control.setTag(self.tag(&button.command));
        control.setEnabled(button.enabled);
        if button.primary {
            control.setBezelColor(Some(&NSColor::controlAccentColor()));
        }
        control.sizeToFit();
        control
    }

    /// Lays buttons out left to right, wrapping when they do not fit.
    fn buttons(&self, parent: &NSView, buttons: &[Button], x: f64, y: f64, w: f64) -> f64 {
        let (mut cx, mut cy, mut row) = (x, y, 0.0f64);
        for button in buttons {
            let control = self.button(button);
            let size = control.frame().size;
            if cx > x && cx + size.width > x + w {
                cx = x;
                cy += row + 6.0;
                row = 0.0;
            }
            self.put(parent, &control, cx, cy, size.width.min(w), size.height);
            cx += size.width + 8.0;
            row = row.max(size.height);
        }
        cy + row - y
    }

    fn graphic_view(&self, graphic: &Graphic, frame: NSRect) -> Retained<ShapeView> {
        let mtm = self.mtm();
        let (shape, label) = match graphic {
            Graphic::Chart(chart) => (
                Shape::Chart(Box::new(chart.clone()), false),
                chart.summary.clone(),
            ),
            Graphic::Spark(chart) => (
                Shape::Chart(Box::new(chart.clone()), true),
                chart.summary.clone(),
            ),
            Graphic::Bar {
                fraction,
                tone,
                label,
            } => (Shape::Bar(*fraction, *tone), label.clone()),
        };
        let view = ShapeView::new(mtm, shape, frame);
        view.setAccessibilityElement(true);
        // SAFETY: static AppKit accessibility role constant.
        view.setAccessibilityRole(Some(unsafe { NSAccessibilityImageRole }));
        view.setAccessibilityLabel(Some(&NSString::from_str(&label)));
        view
    }

    fn resource_group(&self, parent: &NSView, rows: &[&Block], y: f64) -> f64 {
        const ROW: f64 = 52.0;
        let mtm = self.mtm();
        let w = WIDTH - 2.0 * PAD;
        let card = ShapeView::new(
            mtm,
            Shape::Card(None),
            rect(PAD, y, w, ROW * rows.len() as f64),
        );
        for (i, row) in rows.iter().enumerate() {
            let Block::Resource {
                resource,
                tone,
                name,
                value,
                status,
                graphic,
                dim,
                label,
            } = row
            else {
                continue;
            };
            let top = ROW * i as f64;
            if i > 0 {
                self.put(
                    &card,
                    &ShapeView::new(mtm, Shape::Rule, rect(0.0, 0.0, 0.0, 0.0)),
                    12.0,
                    top - 0.5,
                    w - 24.0,
                    1.0,
                );
            }
            let content = Plain::new(mtm, rect(0.0, top, w, ROW));
            let primary = NSColor::labelColor();
            let secondary = NSColor::secondaryLabelColor();
            let (name_field, nh) = self.label(
                name,
                13.0,
                unsafe { NSFontWeightSemibold },
                &primary,
                72.0,
                false,
            );
            self.put(&content, &name_field, 12.0, (ROW - nh) / 2.0, 72.0, nh);
            let (value_field, vh) = self.label(
                value,
                15.0,
                unsafe { NSFontWeightSemibold },
                &primary,
                150.0,
                false,
            );
            value_field.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
                15.0,
                unsafe { NSFontWeightSemibold },
            )));
            let value_width = value_field
                .sizeThatFits(NSSize::new(150.0, 40.0))
                .width
                .ceil()
                .min(150.0);
            self.put(
                &content,
                &value_field,
                84.0,
                (ROW - vh) / 2.0,
                value_width,
                vh,
            );
            let gx = 84.0 + value_width + 10.0;
            match graphic {
                Some(g @ Graphic::Spark(_)) => {
                    let view = self.graphic_view(g, rect(gx, (ROW - 24.0) / 2.0, 96.0, 24.0));
                    view.setAccessibilityElement(false);
                    content.addSubview(&view);
                }
                Some(g @ Graphic::Bar { .. }) => {
                    let view = self.graphic_view(g, rect(gx, (ROW - 6.0) / 2.0, 80.0, 6.0));
                    view.setAccessibilityElement(false);
                    content.addSubview(&view);
                }
                _ => {}
            }
            let (status_field, sh) = self.label(
                status,
                12.0,
                unsafe { NSFontWeightMedium },
                &tone_color(*tone),
                150.0,
                false,
            );
            if matches!(tone, Tone::Neutral | Tone::Unknown | Tone::Paused) {
                status_field.setTextColor(Some(&secondary));
            }
            status_field.setAlignment(NSTextAlignment::Right);
            let sw = status_field
                .sizeThatFits(NSSize::new(150.0, 40.0))
                .width
                .ceil()
                .min(150.0);
            let sx = w - 24.0 - sw;
            self.put(&content, &status_field, sx, (ROW - sh) / 2.0, sw, sh);
            self.icon(&content, *tone, sx - 17.0, (ROW - 14.0) / 2.0, 12.0);
            let (chevron, ch) = self.label(
                "›",
                15.0,
                unsafe { NSFontWeightRegular },
                &secondary,
                12.0,
                false,
            );
            self.put(&content, &chevron, w - 18.0, (ROW - ch) / 2.0, 10.0, ch);
            if *dim {
                content.setAlphaValue(0.5);
            }
            card.addSubview(&content);
            let route = Command::Go(Route::Detail(*resource));
            // SAFETY: `perform:` is defined on Controller, which lives for the whole app.
            let overlay = unsafe {
                NSButton::buttonWithTitle_target_action(
                    ns_string!(""),
                    Some(self),
                    Some(sel!(perform:)),
                    mtm,
                )
            };
            overlay.setBordered(false);
            overlay.setTag(self.tag(&route));
            overlay.setAccessibilityLabel(Some(&NSString::from_str(label)));
            self.put(&card, &overlay, 0.0, top, w, ROW);
        }
        parent.addSubview(&card);
        ROW * rows.len() as f64
    }

    #[allow(clippy::too_many_arguments)]
    fn card(
        &self,
        parent: &NSView,
        tone: Option<Tone>,
        title: &str,
        body: &[String],
        buttons: &[Button],
        x: f64,
        y: f64,
        w: f64,
    ) -> f64 {
        let mtm = self.mtm();
        let card = ShapeView::new(mtm, Shape::Card(tone), rect(x, y, w, 10.0));
        let inner = w - 24.0;
        let mut cy = 12.0;
        let tx = if let Some(tone) = tone.filter(|t| *t != Tone::Neutral) {
            self.icon(&card, tone, 12.0, cy, 14.0);
            32.0
        } else {
            12.0
        };
        let label = NSColor::labelColor();
        cy += self.text(
            &card,
            title,
            14.0,
            unsafe { NSFontWeightSemibold },
            &label,
            tx,
            cy,
            inner - (tx - 12.0),
        );
        for line in body {
            cy += 4.0;
            cy += self.text(
                &card,
                line,
                12.0,
                unsafe { NSFontWeightRegular },
                &NSColor::secondaryLabelColor(),
                12.0,
                cy,
                inner,
            );
        }
        if !buttons.is_empty() {
            cy += 10.0;
            cy += self.buttons(&card, buttons, 12.0, cy, inner);
        }
        cy += 12.0;
        card.setFrameSize(NSSize::new(w, cy));
        parent.addSubview(&card);
        cy
    }

    fn block(&self, parent: &NSView, block: &Block, x: f64, y: f64, w: f64) -> f64 {
        let mtm = self.mtm();
        let label = NSColor::labelColor();
        let secondary = NSColor::secondaryLabelColor();
        let regular = unsafe { NSFontWeightRegular };
        let semibold = unsafe { NSFontWeightSemibold };
        match block {
            Block::Conclusion {
                tone,
                title,
                subtitle,
            } => {
                self.icon(parent, *tone, x, y + 2.0, 28.0);
                let tx = x + 40.0;
                let th = self.text(parent, title, 17.0, semibold, &label, tx, y, w - 40.0);
                let sh = self.text(
                    parent,
                    subtitle,
                    12.0,
                    regular,
                    &secondary,
                    tx,
                    y + th + 2.0,
                    w - 40.0,
                );
                (th + 2.0 + sh).max(32.0)
            }
            Block::Header { back, title, badge } => {
                let back = Button {
                    title: format!("‹ {back}"),
                    command: Command::Back,
                    primary: false,
                    enabled: true,
                };
                let control = self.button(&back);
                control.setBordered(false);
                control.setContentTintColor(Some(&NSColor::controlAccentColor()));
                control.sizeToFit();
                let bw = control.frame().size.width;
                self.put(parent, &control, x - 4.0, y, bw, 22.0);
                let mut badge_w = 0.0;
                if let Some((tone, text)) = badge {
                    let (field, h) = self.label(
                        text,
                        12.0,
                        unsafe { NSFontWeightMedium },
                        &tone_color(*tone),
                        140.0,
                        false,
                    );
                    if matches!(tone, Tone::Neutral | Tone::Unknown | Tone::Paused) {
                        field.setTextColor(Some(&secondary));
                    }
                    let fw = field
                        .sizeThatFits(NSSize::new(140.0, 30.0))
                        .width
                        .ceil()
                        .min(140.0);
                    self.put(parent, &field, x + w - fw, y + (22.0 - h) / 2.0, fw, h);
                    self.icon(parent, *tone, x + w - fw - 17.0, y + 4.0, 12.0);
                    badge_w = fw + 22.0;
                }
                let tx = x + bw + 4.0;
                let (field, h) =
                    self.label(title, 15.0, semibold, &label, w - bw - badge_w - 8.0, false);
                self.put(
                    parent,
                    &field,
                    tx,
                    y + (22.0 - h) / 2.0,
                    w - bw - badge_w - 8.0,
                    h,
                );
                22.0
            }
            Block::Card {
                tone,
                title,
                body,
                buttons,
            } => self.card(parent, Some(*tone), title, body, buttons, x, y, w),
            Block::Heading(text) => self.text(parent, text, 12.0, semibold, &secondary, x, y, w),
            Block::Text(text) => self.text(parent, text, 12.5, regular, &label, x, y, w),
            Block::Note(text) => self.text(parent, text, 11.0, regular, &secondary, x, y, w),
            Block::Big { value, unit } => {
                let (big, bh) = self.label(value, 26.0, semibold, &label, w, false);
                big.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
                    26.0, semibold,
                )));
                let bw = big.sizeThatFits(NSSize::new(w, 60.0)).width.ceil();
                self.put(parent, &big, x, y, bw, bh);
                let (small, sh) = self.label(unit, 13.0, regular, &secondary, w - bw - 6.0, false);
                self.put(
                    parent,
                    &small,
                    x + bw + 6.0,
                    y + bh - sh - 4.0,
                    w - bw - 6.0,
                    sh,
                );
                bh
            }
            Block::Graphic(graphic) => match graphic {
                Graphic::Chart(chart) => self.chart_card(parent, chart, x, y, w),
                Graphic::Bar { label: text, .. } => {
                    let view = self.graphic_view(graphic, rect(x, y, w, 8.0));
                    parent.addSubview(&view);
                    let h = self.text(parent, text, 11.0, regular, &secondary, x, y + 12.0, w);
                    12.0 + h
                }
                Graphic::Spark(_) => {
                    let view = self.graphic_view(graphic, rect(x, y, 96.0, 24.0));
                    parent.addSubview(&view);
                    24.0
                }
            },
            Block::Facts(facts) => {
                let card = ShapeView::new(mtm, Shape::Card(None), rect(x, y, w, 10.0));
                let inner = w - 24.0;
                let mut cy = 4.0;
                for (i, (key, value)) in facts.iter().enumerate() {
                    if i > 0 {
                        self.put(
                            &card,
                            &ShapeView::new(mtm, Shape::Rule, rect(0.0, 0.0, 0.0, 0.0)),
                            12.0,
                            cy,
                            inner,
                            1.0,
                        );
                    }
                    cy += 6.0;
                    let (vfield, _) = self.label(
                        value,
                        12.0,
                        unsafe { NSFontWeightMedium },
                        &label,
                        inner,
                        false,
                    );
                    let vw = vfield
                        .sizeThatFits(NSSize::new(10_000.0, 30.0))
                        .width
                        .ceil();
                    if vw <= inner * 0.55 {
                        let (kfield, kh) =
                            self.label(key, 12.0, regular, &secondary, inner - vw - 8.0, false);
                        self.put(&card, &kfield, 12.0, cy, inner - vw - 8.0, kh);
                        vfield.setAlignment(NSTextAlignment::Right);
                        self.put(&card, &vfield, 12.0 + inner - vw, cy, vw, kh);
                        cy += kh;
                    } else {
                        cy += self.text(
                            &card,
                            key,
                            12.0,
                            unsafe { NSFontWeightMedium },
                            &label,
                            12.0,
                            cy,
                            inner,
                        );
                        cy += 2.0;
                        cy += self.text(&card, value, 12.0, regular, &secondary, 12.0, cy, inner);
                    }
                    cy += 6.0;
                }
                cy += 4.0;
                card.setFrameSize(NSSize::new(w, cy));
                parent.addSubview(&card);
                cy
            }
            Block::Steps(steps) | Block::Bullets(steps) => {
                let numbered = matches!(block, Block::Steps(_));
                let mut cy = 0.0;
                for (i, step) in steps.iter().enumerate() {
                    let mark = if numbered {
                        format!("{}.", i + 1)
                    } else {
                        "•".into()
                    };
                    let (field, _) = self.label(&mark, 12.5, semibold, &secondary, 18.0, false);
                    self.put(parent, &field, x, y + cy, 18.0, 17.0);
                    cy += self.text(
                        parent,
                        step,
                        12.5,
                        regular,
                        &label,
                        x + 20.0,
                        y + cy,
                        w - 20.0,
                    ) + 4.0;
                }
                cy - 4.0
            }
            Block::Buttons(buttons) => self.buttons(parent, buttons, x, y, w),
            Block::Choices(choices) => {
                let mut cy = 0.0;
                for choice in choices {
                    // SAFETY: `perform:` is defined on Controller, which lives for the whole app.
                    let radio = unsafe {
                        NSButton::radioButtonWithTitle_target_action(
                            &NSString::from_str(&choice.title),
                            Some(self),
                            Some(sel!(perform:)),
                            mtm,
                        )
                    };
                    radio.setTag(self.tag(&choice.command));
                    radio.setState(if choice.primary {
                        NSControlStateValueOn
                    } else {
                        NSControlStateValueOff
                    });
                    radio.sizeToFit();
                    let size = radio.frame().size;
                    self.put(parent, &radio, x, y + cy, size.width, size.height);
                    cy += size.height + 4.0;
                }
                cy - 4.0
            }
            Block::Progress {
                done,
                total,
                label: text,
            } => {
                let boxes = ShapeView::new(
                    mtm,
                    Shape::Boxes(*done, *total),
                    rect(x, y + 3.0, 24.0 * f64::from(*total), 10.0),
                );
                boxes.setAccessibilityElement(true);
                boxes.setAccessibilityLabel(Some(&NSString::from_str(text)));
                parent.addSubview(&boxes);
                let tx = x + 24.0 * f64::from(*total) + 6.0;
                self.text(parent, text, 12.0, regular, &secondary, tx, y, w - (tx - x))
                    .max(16.0)
            }
            Block::Processes(rows) => self.process_rows(parent, rows, x, y, w),
            Block::Resource { .. } => 0.0,
        }
    }

    fn chart_card(&self, parent: &NSView, chart: &Chart, x: f64, y: f64, w: f64) -> f64 {
        let mtm = self.mtm();
        let card = ShapeView::new(mtm, Shape::Card(None), rect(x, y, w, 10.0));
        let inner = w - 24.0;
        let (title, th) = self.label(
            &chart.title,
            12.0,
            unsafe { NSFontWeightSemibold },
            &NSColor::labelColor(),
            inner / 2.0,
            false,
        );
        self.put(&card, &title, 12.0, 10.0, inner / 2.0, th);
        let (unit, uh) = self.label(
            &chart.unit,
            11.0,
            unsafe { NSFontWeightRegular },
            &NSColor::secondaryLabelColor(),
            inner / 2.0,
            false,
        );
        unit.setAlignment(NSTextAlignment::Right);
        self.put(
            &card,
            &unit,
            12.0 + inner / 2.0,
            10.0 + (th - uh),
            inner / 2.0,
            uh,
        );
        let mut cy = 10.0 + th + 6.0;
        let plot = self.graphic_view(&Graphic::Chart(chart.clone()), rect(12.0, cy, inner, 104.0));
        card.addSubview(&plot);
        cy += 104.0 + 4.0;
        if !chart.note.is_empty() {
            cy += self.text(
                &card,
                &chart.note,
                11.0,
                unsafe { NSFontWeightRegular },
                &NSColor::secondaryLabelColor(),
                12.0,
                cy,
                inner,
            );
        }
        cy += 10.0;
        card.setFrameSize(NSSize::new(w, cy));
        parent.addSubview(&card);
        cy
    }

    fn process_rows(&self, parent: &NSView, rows: &[ProcessRow], x: f64, y: f64, w: f64) -> f64 {
        const ROW: f64 = 30.0;
        let mtm = self.mtm();
        let card = ShapeView::new(
            mtm,
            Shape::Card(None),
            rect(x, y, w, ROW * rows.len() as f64),
        );
        let label = NSColor::labelColor();
        let secondary = NSColor::secondaryLabelColor();
        for (i, row) in rows.iter().enumerate() {
            let top = ROW * i as f64;
            if i > 0 {
                self.put(
                    &card,
                    &ShapeView::new(mtm, Shape::Rule, rect(0.0, 0.0, 0.0, 0.0)),
                    12.0,
                    top,
                    w - 24.0,
                    1.0,
                );
            }
            let (rank, h) = self.label(
                &row.rank.to_string(),
                12.0,
                unsafe { NSFontWeightRegular },
                &secondary,
                16.0,
                false,
            );
            self.put(&card, &rank, 12.0, top + (ROW - h) / 2.0, 16.0, h);
            let name_w = w - 32.0 - 150.0;
            let (name, h) = self.label(
                &row.name,
                12.5,
                unsafe { NSFontWeightRegular },
                &label,
                name_w,
                false,
            );
            self.put(&card, &name, 32.0, top + (ROW - h) / 2.0, name_w, h);
            let bar = ShapeView::new(
                mtm,
                Shape::Bar(row.fraction, Tone::Neutral),
                rect(w - 150.0, top + (ROW - 4.0) / 2.0, 64.0, 4.0),
            );
            card.addSubview(&bar);
            let (size, h) = self.label(
                &row.size,
                12.0,
                unsafe { NSFontWeightSemibold },
                &label,
                70.0,
                false,
            );
            size.setAlignment(NSTextAlignment::Right);
            self.put(&card, &size, w - 82.0, top + (ROW - h) / 2.0, 70.0, h);
        }
        parent.addSubview(&card);
        ROW * rows.len() as f64
    }

    fn footer(&self, footer: &Footer, top: f64) -> Retained<Plain> {
        let mtm = self.mtm();
        let view = Plain::new(mtm, rect(0.0, top, WIDTH, FOOTER));
        self.put(
            &view,
            &ShapeView::new(mtm, Shape::Rule, rect(0.0, 0.0, 0.0, 0.0)),
            0.0,
            0.0,
            WIDTH,
            1.0,
        );
        let small = |control: &NSButton| {
            control.setBordered(false);
            control.setFont(Some(&NSFont::systemFontOfSize(12.0)));
            control.sizeToFit();
        };
        let pause = self.button(&footer.pause);
        let paused =
            footer.pause.command == Command::TogglePause && footer.pause.title.starts_with("继续");
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(if paused { "play.fill" } else { "pause.fill" }),
            None,
        ) {
            pause.setImage(Some(&image));
            pause.setImagePosition(NSCellImagePosition::ImageLeading);
        }
        small(&pause);
        let pw = pause.frame().size.width;
        self.put(&view, &pause, PAD - 4.0, (FOOTER - 20.0) / 2.0, pw, 20.0);
        let interval = self.button(&footer.interval);
        small(&interval);
        interval.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        let iw = interval.frame().size.width;
        self.put(
            &view,
            &interval,
            PAD + pw + 8.0,
            (FOOTER - 20.0) / 2.0,
            iw,
            20.0,
        );
        let more = self.button(&Button {
            title: "更多".into(),
            command: Command::More,
            primary: false,
            enabled: true,
        });
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            ns_string!("chevron.down"),
            None,
        ) {
            more.setImage(Some(&image));
            more.setImagePosition(NSCellImagePosition::ImageTrailing);
        }
        small(&more);
        let mw = more.frame().size.width;
        self.put(
            &view,
            &more,
            WIDTH - PAD - mw + 4.0,
            (FOOTER - 20.0) / 2.0,
            mw,
            20.0,
        );
        *self.ivars().more_button.borrow_mut() = Some(more);
        view
    }
}

/// Opens a read-only system tool. `Ok` means the system accepted the request, not that a
/// particular pane is visible or a problem is fixed.
fn open_tool(tool: Tool) -> Result<(), String> {
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
const STORAGE_SETTINGS_URL: &str = "x-apple.systempreferences:com.apple.settings.Storage";

pub fn run(interval: u64) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("菜单栏必须在主线程启动")?;
    let collector = platform::collector()?;
    let app = NSApplication::sharedApplication(mtm);
    // Menu-bar only: no Dock icon, no main window.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let controller = Controller::new(mtm, collector, interval);
    Controller::install(&controller);
    app.run();
    Ok(())
}
