//! Native AppKit shell: one `NSStatusItem` with a transient `NSPopover`; no WebView, no helper
//! process.
//!
//! AppKit work stays on the main thread. Sampling is an `NSTimer` in the common run-loop modes, so
//! it keeps firing while the popover or its More menu is open. Popover views are rebuilt from
//! `panel::page` only while the popover is shown (new sample, navigation or command); closing it
//! drops them and monitoring continues. Pausing invalidates the timer; resuming starts a fresh
//! collector so rates get a new baseline. The on-demand process scan and the optimization analysis
//! run as short background jobs that the main thread polls and joins, including at quit. Actions
//! open read-only system tools or, after the person confirms, ask chosen apps to quit normally.
//!
//! Layout: `controller` (app lifecycle, timers, popover), `commands` (what each control does),
//! `render` (page → view tree), `kit`, `blocks` and `rows` (a small reusable view kit: labels,
//! buttons, cards, block layouts, grouped rows, footer), `views` (custom NSView classes) and `draw` (native drawing).

mod blocks;
mod commands;
mod controller;
mod draw;
mod jobs;
mod kit;
mod optimizer;
mod prefs;
mod render;
mod rows;
mod views;

use std::cell::{Cell, OnceCell, RefCell};
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
    NSPopover, NSPopoverBehavior, NSPopoverDelegate, NSResponder, NSRunningApplication, NSScreen,
    NSScrollView, NSStatusBar, NSStatusItem, NSStringDrawing, NSTextAlignment, NSTextField,
    NSVariableStatusItemLength, NSView, NSViewController, NSWorkspace,
    NSWorkspaceDidWakeNotification,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSAttributedStringKey, NSDictionary, NSNotification, NSNumber,
    NSObject, NSObjectProtocol, NSPoint, NSRect, NSRectEdge, NSRunLoop, NSRunLoopCommonModes,
    NSSize, NSString, NSTimer, NSURL,
};

use crate::lang::{Lang, LanguageChoice, Ui};
use crate::model::ProcessScan;
use crate::monitor::{Group, Health, Monitor, Tool};
use crate::optimize::{self, Candidate, QuitOutcome, QuitState, RunningApp};
use crate::panel::{self, Block, Button, Chart, Command, Footer, Graphic, ProcessRow, Route, Tone};
use crate::platform::{self, Collector};

use controller::Controller;
use draw::*;
use jobs::Job;
use kit::Kit;
use views::*;

pub(super) const WIDTH: f64 = 440.0;
pub(super) const PAD: f64 = 16.0;
pub(super) const GAP: f64 = 12.0;
pub(super) const FOOTER: f64 = 40.0;
pub(super) const MENU_PROCESSES: isize = 9001;
pub(super) const MENU_SETTINGS: isize = 9002;
pub(super) const MENU_QUIT: isize = 9003;

pub(super) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

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
