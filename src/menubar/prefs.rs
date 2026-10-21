//! The language preference, stored in the app's user defaults, and the system's languages.

use super::*;
use objc2_foundation::{NSLocale, NSUserDefaults};

const LANGUAGE_KEY: &str = "language";

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
