//! Sleep and wake notifications, so the engine can pause while the Mac sleeps.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};

/// A change in the Mac's power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    /// The Mac is about to sleep.
    Sleep,
    /// The Mac woke up.
    Wake,
}

/// Calls a function when the Mac sleeps or wakes, until dropped.
///
/// Notifications arrive on the main thread, so the app must be running its
/// main run loop (the Tauri app always is).
pub struct SleepWatcher {
    center: Retained<NSNotificationCenter>,
    observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl SleepWatcher {
    /// Starts watching. `on_event` runs on the main thread, so keep it short
    /// (for example, send the event to the worker thread).
    pub fn new(on_event: impl Fn(PowerEvent) + Send + Sync + Clone + 'static) -> Self {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let observers = [
            // SAFETY: the extern statics are valid notification names.
            (
                unsafe { NSWorkspaceWillSleepNotification },
                PowerEvent::Sleep,
            ),
            (unsafe { NSWorkspaceDidWakeNotification }, PowerEvent::Wake),
        ]
        .into_iter()
        .map(|(name, event)| {
            let on_event = on_event.clone();
            let block = RcBlock::new(move |_: NonNull<NSNotification>| on_event(event));
            // SAFETY: the block is retained by the notification center until
            // the observer is removed in `drop`.
            unsafe {
                center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
            }
        })
        .collect();
        Self { center, observers }
    }
}

impl Drop for SleepWatcher {
    fn drop(&mut self) {
        for observer in &self.observers {
            // SAFETY: each observer was returned by this center.
            unsafe { self.center.removeObserver(observer.as_ref()) };
        }
    }
}

impl std::fmt::Debug for SleepWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SleepWatcher")
            .field("observers", &self.observers.len())
            .finish()
    }
}
