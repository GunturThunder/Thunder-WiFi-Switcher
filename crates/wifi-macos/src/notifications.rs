//! Notifications on macOS: showing them, whether macOS lets them through, and
//! asking the user.
//!
//! Everything here uses Apple's current notification framework
//! (`UNUserNotificationCenter`). macOS refuses an app that mixes it with the
//! old one (`NSUserNotificationCenter`, which Tauri's notification plugin
//! uses): "You can't mix modern clients with legacy clients". So on macOS the
//! app shows notifications here, not through the plugin.
//!
//! macOS asks each app only once whether it may show notifications; after
//! "Don't Allow", only System Settings can change it. Reading the answer lets
//! the settings window say so plainly.
//!
//! Apple's framework only works inside a real `.app`: anywhere else (`tauri
//! dev`, tests) it stops the whole process. So outside an app bundle every
//! answer is [`NotificationPermission::Unknown`].

use std::ptr::NonNull;
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAlertStyle, UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
    UNNotification, UNNotificationPresentationOptions, UNNotificationRequest,
    UNNotificationSettings, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
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

/// Shows a notification. `on_error` gets macOS's reason if it refuses it.
/// Returns `false` without doing anything when not running inside an app
/// bundle (`tauri dev`), where Apple's framework can't be used.
pub fn show_notification(title: &str, body: &str, on_error: impl Fn(String) + 'static) -> bool {
    let Some(center) = center() else {
        return false;
    };
    show_while_in_front(&center);

    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    // Each notification needs its own id, or it replaces the previous one.
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = format!("thunderwifi-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(&id),
        &content,
        None,
    );
    let done = RcBlock::new(move |error: *mut NSError| {
        // SAFETY: macOS passes either null or a valid error for the call.
        if let Some(error) = unsafe { error.as_ref() } {
            on_error(error.localizedDescription().to_string());
        }
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
    true
}

define_class!(
    /// Tells macOS to show the app's notifications as banners even while the
    /// app is in front (its settings window is open); by default macOS hides
    /// them then.
    // SAFETY: NSObject has no subclassing requirements, and this class has no
    // fields and doesn't implement `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "ThunderWiFiNotificationPresenter"]
    struct Presenter;

    unsafe impl NSObjectProtocol for Presenter {}

    unsafe impl UNUserNotificationCenterDelegate for Presenter {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }
    }
);

/// Sets the [`Presenter`] as the center's delegate, once. The center keeps
/// only a weak reference, so the presenter lives as long as the app.
fn show_while_in_front(center: &UNUserNotificationCenter) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let this = Presenter::alloc().set_ivars(());
        // SAFETY: NSObject's plain `init`; the subclass has no fields to set up.
        let presenter: Retained<Presenter> = unsafe { msg_send![super(this), init] };
        center.setDelegate(Some(ProtocolObject::from_ref(&*presenter)));
        std::mem::forget(presenter);
    });
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
    fn outside_an_app_bundle_nothing_is_asked_or_shown() {
        // A test binary isn't an app: using Apple's framework here would
        // stop the process, so nothing is asked or shown.
        assert_eq!(notification_permission(), NotificationPermission::Unknown);
        assert_eq!(
            ask_notification_permission(),
            NotificationPermission::Unknown
        );
        assert!(!show_notification("Title", "Body", |_| {}));
    }
}
