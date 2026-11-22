//! Notification permission: whether macOS shows the app's notifications, and
//! asking the user.
//!
//! The app shows its notifications through Tauri's notification plugin, which
//! can't tell whether the user allowed them. macOS asks each app only once;
//! after "Don't Allow", only System Settings can change it. Reading the answer
//! lets the settings window say so plainly.
//!
//! Apple's framework only works inside a real `.app`: anywhere else (`tauri
//! dev`, tests) it stops the whole process. So outside an app bundle every
//! answer is [`NotificationPermission::Unknown`].

use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError};
use objc2_user_notifications::{
    UNAlertStyle, UNAuthorizationOptions, UNAuthorizationStatus, UNNotificationSettings,
    UNUserNotificationCenter,
};

/// Whether macOS shows the app's notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPermission {
    /// Shown on screen (as banners or alerts).
    Allowed,
    /// Not shown on screen: the user said no, or chose no banners. At most
    /// they reach Notification Center.
    Blocked,
    /// macOS hasn't asked yet. It will, the first time.
    NotAsked,
    /// Can't tell: not running as an app, or macOS didn't answer in time.
    Unknown,
}

/// How long to wait for macOS to report the setting.
const READ_WAIT: Duration = Duration::from_secs(3);
/// How long to wait for the user to answer macOS's question.
const ASK_WAIT: Duration = Duration::from_secs(120);

/// The current setting. Waits up to 3 seconds for macOS, so call it off the
/// main thread.
pub fn notification_permission() -> NotificationPermission {
    let Some(center) = center() else {
        return NotificationPermission::Unknown;
    };
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        // SAFETY: macOS passes a valid settings object for the whole call.
        let settings = unsafe { settings.as_ref() };
        let _ = tx.send(judge(settings.authorizationStatus(), settings.alertStyle()));
    });
    center.getNotificationSettingsWithCompletionHandler(&block);
    rx.recv_timeout(READ_WAIT)
        .unwrap_or(NotificationPermission::Unknown)
}

/// Asks the user whether the app may show notifications, then returns the
/// setting. macOS only shows its question if it hasn't asked before; then it
/// answers straight away. Waits up to 2 minutes for the user, so call it off
/// the main thread.
pub fn ask_notification_permission() -> NotificationPermission {
    let Some(center) = center() else {
        return NotificationPermission::Unknown;
    };
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |_granted: Bool, _error: *mut NSError| {
        let _ = tx.send(());
    });
    center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &block);
    match rx.recv_timeout(ASK_WAIT) {
        Ok(()) => notification_permission(),
        Err(_) => NotificationPermission::Unknown,
    }
}

/// The notification center, or `None` when not running inside an app bundle.
fn center() -> Option<Retained<UNUserNotificationCenter>> {
    let in_app = NSBundle::mainBundle()
        .bundlePath()
        .to_string()
        .ends_with(".app");
    in_app.then(UNUserNotificationCenter::currentNotificationCenter)
}

/// Turns macOS's settings into our answer.
fn judge(status: UNAuthorizationStatus, style: UNAlertStyle) -> NotificationPermission {
    match status {
        UNAuthorizationStatus::NotDetermined => NotificationPermission::NotAsked,
        UNAuthorizationStatus::Denied => NotificationPermission::Blocked,
        // Allowed, but set to "None": nothing appears on screen.
        _ if style == UNAlertStyle::None => NotificationPermission::Blocked,
        _ => NotificationPermission::Allowed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_become_plain_answers() {
        use NotificationPermission as P;
        use UNAlertStyle as Style;
        use UNAuthorizationStatus as Status;

        assert_eq!(judge(Status::NotDetermined, Style::None), P::NotAsked);
        assert_eq!(judge(Status::Denied, Style::Banner), P::Blocked);
        assert_eq!(judge(Status::Authorized, Style::Banner), P::Allowed);
        assert_eq!(judge(Status::Authorized, Style::Alert), P::Allowed);
        assert_eq!(judge(Status::Authorized, Style::None), P::Blocked);
        assert_eq!(judge(Status::Provisional, Style::None), P::Blocked);
    }

    #[test]
    fn outside_an_app_bundle_the_answer_is_unknown() {
        // A test binary isn't an app: asking Apple's framework here would
        // stop the process, so nothing is asked.
        assert_eq!(notification_permission(), NotificationPermission::Unknown);
        assert_eq!(
            ask_notification_permission(),
            NotificationPermission::Unknown
        );
    }
}
