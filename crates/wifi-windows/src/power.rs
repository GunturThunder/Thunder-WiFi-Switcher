//! Sleep and resume notifications, so the engine can pause while the PC sleeps.

use std::ffi::c_void;
use std::ptr;

use thunder_core::{Error, Result};
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
use windows::Win32::System::Power::{
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, PowerRegisterSuspendResumeNotification,
    PowerUnregisterSuspendResumeNotification,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND,
};

/// A change in the PC's power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    /// The PC is about to sleep.
    Sleep,
    /// The PC woke up.
    Wake,
}

type Callback = Box<dyn Fn(PowerEvent) + Send + Sync>;

/// Calls a function when the PC sleeps or wakes, until dropped. Needs no
/// window or message loop: Windows calls back on its own thread.
pub struct PowerWatcher {
    registration: *mut c_void,
    callback: *mut Callback,
    // Windows keeps a pointer to the parameters while registered.
    _parameters: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
}

// SAFETY: the callback is Send + Sync, and the registration handle may be
// used (unregistered) from any thread.
unsafe impl Send for PowerWatcher {}
// SAFETY: as above; nothing is mutated after creation.
unsafe impl Sync for PowerWatcher {}

unsafe extern "system" fn on_power(
    context: *const c_void,
    kind: u32,
    _setting: *const c_void,
) -> u32 {
    // SAFETY: the context is the boxed callback, alive until unregistered.
    let Some(callback) = (unsafe { (context as *const Callback).as_ref() }) else {
        return ERROR_SUCCESS.0;
    };
    match kind {
        PBT_APMSUSPEND => callback(PowerEvent::Sleep),
        // Sent on every resume (PBT_APMRESUMESUSPEND only follows when a user
        // is present), so one Wake per resume.
        PBT_APMRESUMEAUTOMATIC => callback(PowerEvent::Wake),
        _ => {}
    }
    ERROR_SUCCESS.0
}

impl PowerWatcher {
    /// Starts watching. `on_event` runs on a Windows thread; keep it short.
    pub fn new(on_event: impl Fn(PowerEvent) + Send + Sync + 'static) -> Result<Self> {
        let callback: *mut Callback = Box::into_raw(Box::new(Box::new(on_event)));
        let mut parameters = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(on_power),
            Context: callback as *mut c_void,
        });
        let mut registration: *mut c_void = ptr::null_mut();
        // SAFETY: with DEVICE_NOTIFY_CALLBACK the "recipient" is a pointer to
        // the subscribe parameters, which live as long as the watcher.
        let result = unsafe {
            PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK,
                HANDLE(&mut *parameters as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS as *mut c_void),
                &mut registration,
            )
        };
        if result != ERROR_SUCCESS {
            // SAFETY: never handed to Windows successfully.
            drop(unsafe { Box::from_raw(callback) });
            return Err(Error::Os(format!(
                "couldn't watch for sleep and wake (error {})",
                result.0
            )));
        }
        Ok(Self {
            registration,
            callback,
            _parameters: parameters,
        })
    }
}

impl Drop for PowerWatcher {
    fn drop(&mut self) {
        // SAFETY: unregister first, then free the callback it pointed to.
        unsafe {
            let _ =
                PowerUnregisterSuspendResumeNotification(HPOWERNOTIFY(self.registration as isize));
            drop(Box::from_raw(self.callback));
        }
    }
}

impl std::fmt::Debug for PowerWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PowerWatcher").finish_non_exhaustive()
    }
}
