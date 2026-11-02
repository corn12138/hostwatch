//! What the system lets Hostwatch do outside the popover: post notifications and open at login.
//! AppKit reads the real state and stores it here so pages stay testable.

/// Whether macOS lets Hostwatch post notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeAccess {
    /// Not running as Hostwatch.app (e.g. `hostwatch menubar` from a terminal): macOS only
    /// delivers notifications for app bundles.
    Unavailable,
    /// The answer from macOS has not arrived yet.
    Unknown,
    NotAsked,
    Allowed,
    Denied,
}

/// Whether Hostwatch opens when you log in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginItem {
    /// Not running as Hostwatch.app.
    Unavailable,
    Off,
    On,
    /// Registered, but switched off in System Settings › General › Login Items.
    NeedsApproval,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Integrations {
    /// The person's choice; posting also needs `access` to be `Allowed`.
    pub notifications: bool,
    pub access: NoticeAccess,
    pub login: LoginItem,
    /// Why the last change of the login item failed, until the next change.
    pub login_error: Option<String>,
}

impl Default for Integrations {
    fn default() -> Self {
        Self {
            notifications: true,
            access: NoticeAccess::Unavailable,
            login: LoginItem::Unavailable,
            login_error: None,
        }
    }
}
