//! Location permission. macOS only gives Wi-Fi network names to apps that
//! have it; the app never reads a location (see PRIVACY.md).

use std::cell::RefCell;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2_core_location::{CLAuthorizationStatus, CLLocationManager};
use thunder_core::Permission;

/// Current permission state. Safe to call from any thread, but only accurate
/// after [`start`] and a short run of the main run loop.
pub fn status() -> Permission {
    // SAFETY: class methods with no preconditions.
    if !unsafe { CLLocationManager::locationServicesEnabled_class() } {
        // Location Services is off for the whole Mac.
        return Permission::Denied;
    }
    // The class method answers straight away. The newer instance property
    // reads "not determined" on a fresh manager until macOS delivers the real
    // status to that manager's run loop, which a worker thread never runs.
    #[allow(deprecated)]
    let status = unsafe { CLLocationManager::authorizationStatus_class() };
    from_status(status)
}

fn from_status(status: CLAuthorizationStatus) -> Permission {
    match status {
        CLAuthorizationStatus::AuthorizedAlways | CLAuthorizationStatus::AuthorizedWhenInUse => {
            Permission::Granted
        }
        CLAuthorizationStatus::NotDetermined => Permission::NotDetermined,
        _ => Permission::Denied,
    }
}

thread_local! {
    /// The app's long-lived location manager, on the main thread. macOS tells
    /// a process its permission state asynchronously, through a manager's run
    /// loop; until a manager has heard back, every status check in the process
    /// reads "not determined". It also has to stay alive while the permission
    /// prompt is up, or the prompt disappears.
    static MANAGER: RefCell<Option<Retained<CLLocationManager>>> = const { RefCell::new(None) };
}

/// Runs `f` with the main-thread manager, creating it if needed. Must be
/// called on the main thread.
fn with_manager(f: impl FnOnce(&CLLocationManager)) {
    MANAGER.with(|slot| {
        // SAFETY: on the main thread, which has a run loop.
        let mut slot = slot.borrow_mut();
        let manager = slot.get_or_insert_with(|| unsafe { CLLocationManager::new() });
        f(manager);
    });
}

/// Creates the main-thread manager, so [`status`] becomes accurate once the
/// main run loop has run briefly. Called when the backend is created.
pub fn start() {
    DispatchQueue::main().exec_async(|| with_manager(|_| {}));
}

/// Asks for permission. The prompt appears once the main thread's run loop
/// runs; if the user already answered, nothing is shown.
pub fn request() {
    DispatchQueue::main().exec_async(|| {
        // SAFETY: on the main thread.
        with_manager(|manager| unsafe { manager.requestWhenInUseAuthorization() });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_authorization_status() {
        assert_eq!(
            from_status(CLAuthorizationStatus::NotDetermined),
            Permission::NotDetermined
        );
        assert_eq!(
            from_status(CLAuthorizationStatus::Restricted),
            Permission::Denied
        );
        assert_eq!(
            from_status(CLAuthorizationStatus::Denied),
            Permission::Denied
        );
        assert_eq!(
            from_status(CLAuthorizationStatus::AuthorizedAlways),
            Permission::Granted
        );
        assert_eq!(
            from_status(CLAuthorizationStatus::AuthorizedWhenInUse),
            Permission::Granted
        );
    }
}
