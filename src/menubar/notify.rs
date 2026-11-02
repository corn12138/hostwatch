//! macOS notifications and the login item. Both need an app bundle: outside Hostwatch.app every
//! call here is skipped and the state reads as unavailable.
//!
//! Notification callbacks may arrive on any thread. The permission answer goes into an atomic the
//! main thread reads; a click is handed to the main thread with `performSelectorOnMainThread`.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};

use block2::{DynBlock, RcBlock};
use objc2::runtime::Bool;
use objc2::AllocAnyThread;
use objc2_foundation::{NSBundle, NSError};
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationInterruptionLevel, UNNotificationPresentationOptions, UNNotificationRequest,
    UNNotificationResponse, UNNotificationSettings, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

use super::*;
use crate::monitor::{LoginItem, NoticeAccess};

const ID_PREFIX: &str = "hostwatch.";
/// No answer from macOS yet.
const UNKNOWN: isize = -1;

thread_local! {
    /// Main thread only: where clicked notifications are delivered.
    static TARGET: RefCell<Option<Weak<Controller>>> = const { RefCell::new(None) };
}

/// The group of the most recently clicked notification, waiting for the main thread.
static CLICKED: Mutex<Option<Group>> = Mutex::new(None);

pub(super) fn in_app_bundle() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
        && NSBundle::mainBundle()
            .bundlePath()
            .to_string()
            .ends_with(".app")
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and `NoticeDelegate` does not implement Drop.
    #[unsafe(super = NSObject)]
    #[name = "HostwatchNoticeDelegate"]
    pub(super) struct NoticeDelegate;

    impl NoticeDelegate {
        // SAFETY: `performSelectorOnMainThread:withObject:waitUntilDone:` passes one object.
        #[unsafe(method(deliverClick:))]
        fn deliver_click(&self, _arg: Option<&AnyObject>) {
            let Some(group) = CLICKED.lock().ok().and_then(|mut g| g.take()) else {
                return;
            };
            let controller = TARGET.with(|t| t.borrow().as_ref().and_then(Weak::load));
            if let Some(controller) = controller {
                controller.show_issue(group);
            }
        }
    }

    unsafe impl NSObjectProtocol for NoticeDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NoticeDelegate {
        // SAFETY: signature matches the protocol method.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            // A menu-bar app counts as "in front" while its popover is key; still show the banner.
            handler.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List,));
        }

        // SAFETY: signature matches the protocol method.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &DynBlock<dyn Fn()>,
        ) {
            let id = response.notification().request().identifier().to_string();
            if let Some(group) = id.strip_prefix(ID_PREFIX).and_then(Group::from_key) {
                if let Ok(mut clicked) = CLICKED.lock() {
                    *clicked = Some(group);
                }
                // SAFETY: `deliverClick:` takes one optional object; `self` outlives the call.
                unsafe {
                    let _: () = msg_send![
                        self,
                        performSelectorOnMainThread: sel!(deliverClick:),
                        withObject: Option::<&AnyObject>::None,
                        waitUntilDone: false
                    ];
                }
            }
            handler.call(());
        }
    }
);

/// Notification permission and delivery for Hostwatch.app.
pub(super) struct Notifier {
    /// Kept alive here: the center holds its delegate weakly.
    _delegate: Retained<NoticeDelegate>,
    status: Arc<AtomicIsize>,
}

impl Notifier {
    /// `None` outside an app bundle, where the notification center is unavailable.
    pub(super) fn new(controller: &Retained<Controller>) -> Option<Self> {
        if !in_app_bundle() {
            return None;
        }
        TARGET.with(|t| *t.borrow_mut() = Some(Weak::from_retained(controller)));
        let delegate = NoticeDelegate::alloc().set_ivars(());
        // SAFETY: plain NSObject init.
        let delegate: Retained<NoticeDelegate> = unsafe { msg_send![super(delegate), init] };
        UNUserNotificationCenter::currentNotificationCenter()
            .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let notifier = Self {
            _delegate: delegate,
            status: Arc::new(AtomicIsize::new(UNKNOWN)),
        };
        notifier.refresh();
        Some(notifier)
    }

    /// Asks macOS once; later calls return the stored answer without a prompt.
    pub(super) fn authorize(&self) {
        let status = Arc::clone(&self.status);
        let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            let value = if granted.as_bool() {
                UNAuthorizationStatus::Authorized
            } else {
                UNAuthorizationStatus::Denied
            };
            status.store(value.0, Ordering::Relaxed);
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert,
                &block,
            );
    }

    /// Re-reads permission in the background; the answer shows on the next render.
    pub(super) fn refresh(&self) {
        let status = Arc::clone(&self.status);
        let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            // SAFETY: the center passes a valid settings object for the duration of the call.
            let value = unsafe { settings.as_ref() }.authorizationStatus();
            status.store(value.0, Ordering::Relaxed);
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .getNotificationSettingsWithCompletionHandler(&block);
    }

    pub(super) fn access(&self) -> NoticeAccess {
        match UNAuthorizationStatus(self.status.load(Ordering::Relaxed)) {
            UNAuthorizationStatus::NotDetermined => NoticeAccess::NotAsked,
            UNAuthorizationStatus::Denied => NoticeAccess::Denied,
            UNAuthorizationStatus(UNKNOWN) => NoticeAccess::Unknown,
            _ => NoticeAccess::Allowed,
        }
    }

    /// One notification per group: a newer one for the same group replaces the older.
    pub(super) fn post(&self, notice: &Notice) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&notice.title));
        content.setBody(&NSString::from_str(&notice.body));
        content.setThreadIdentifier(&NSString::from_str(notice.group.key()));
        content.setInterruptionLevel(UNNotificationInterruptionLevel::Active);
        let id = NSString::from_str(&format!("{ID_PREFIX}{}", notice.group.key()));
        let request =
            UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, None);
    }
}

pub(super) fn login_item() -> LoginItem {
    if !in_app_bundle() {
        return LoginItem::Unavailable;
    }
    // SAFETY: plain status read of this app's own service.
    match unsafe { SMAppService::mainAppService().status() } {
        SMAppServiceStatus::Enabled => LoginItem::On,
        SMAppServiceStatus::RequiresApproval => LoginItem::NeedsApproval,
        _ => LoginItem::Off,
    }
}

pub(super) fn set_login_item(on: bool) -> Result<(), String> {
    // SAFETY: registers or unregisters this app's own login item.
    let result = unsafe {
        let service = SMAppService::mainAppService();
        if on {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    result.map_err(|error| error.localizedDescription().to_string())
}
