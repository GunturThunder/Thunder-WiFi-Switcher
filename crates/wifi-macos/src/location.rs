//! Location permission. macOS only gives Wi-Fi network names to apps that
//! have it; the app never reads a location (see PRIVACY.md).

use std::cell::RefCell;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2_core_location::{CLAuthorizationStatus, CLLocationManager};
use thunder_core::Permission;

/// Current permission state. Safe to call from any thread.
pub fn status() -> Permission {
    // SAFETY: creating a manager and reading its status has no preconditions.
    // A fresh manager is used because managers aren't thread-safe.
    if !unsafe { CLLocationManager::locationServicesEnabled_class() } {
        // Location Services is off for the whole Mac.
        return Permission::Denied;
    }
    let manager = unsafe { CLLocationManager::new() };
    from_status(unsafe { manager.authorizationStatus() })
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
    /// The manager that asked for permission. It must stay alive on the main
    /// thread, or the prompt disappears.
    static REQUESTER: RefCell<Option<Retained<CLLocationManager>>> = const { RefCell::new(None) };
}

/// Asks for permission. The prompt appears once the main thread's run loop
/// runs; if the user already answered, nothing is shown.
pub fn request() {
    DispatchQueue::main().exec_async(|| {
        REQUESTER.with(|slot| {
            // SAFETY: on the main thread, which has a run loop.
            let manager = unsafe { CLLocationManager::new() };
            unsafe { manager.requestWhenInUseAuthorization() };
            *slot.borrow_mut() = Some(manager);
        });
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
