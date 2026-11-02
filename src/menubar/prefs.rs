//! Language and notification preferences, stored in the app's user defaults, and the system's
//! languages.

use super::*;
use objc2_foundation::{NSLocale, NSUserDefaults};

const LANGUAGE_KEY: &str = "language";
const NOTIFICATIONS_KEY: &str = "notifications";

pub(super) fn load_choice() -> LanguageChoice {
    autoreleasepool(|_| {
        NSUserDefaults::standardUserDefaults()
            .stringForKey(&NSString::from_str(LANGUAGE_KEY))
            .and_then(|value| LanguageChoice::from_key(&value.to_string()))
            .unwrap_or(LanguageChoice::System)
    })
}

pub(super) fn save_choice(choice: LanguageChoice) {
    autoreleasepool(|_| {
        let value = NSString::from_str(choice.key());
        // SAFETY: an NSString is a valid property-list value for user defaults.
        unsafe {
            NSUserDefaults::standardUserDefaults()
                .setObject_forKey(Some(&value), &NSString::from_str(LANGUAGE_KEY))
        };
    });
}

/// Notifications are on unless the person turned them off.
pub(super) fn load_notifications() -> bool {
    autoreleasepool(|_| {
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = NSString::from_str(NOTIFICATIONS_KEY);
        defaults.objectForKey(&key).is_none() || defaults.boolForKey(&key)
    })
}

pub(super) fn save_notifications(on: bool) {
    autoreleasepool(|_| {
        NSUserDefaults::standardUserDefaults()
            .setBool_forKey(on, &NSString::from_str(NOTIFICATIONS_KEY))
    });
}

/// macOS's preferred languages, most preferred first (for example `zh-Hans-CN`).
pub(super) fn preferred_languages() -> Vec<String> {
    autoreleasepool(|_| {
        NSLocale::preferredLanguages()
            .iter()
            .map(|tag| tag.to_string())
            .collect()
    })
}

pub(super) fn current_ui(choice: LanguageChoice) -> Ui {
    Ui::new(choice, &preferred_languages())
}
