//! Keep-awake capability — see /agent-docs/capabilities/keep-awake/CONTRACT.md.
//! One power assertion for the whole app: IOKit's IOPMAssertionCreateWithName
//! on macOS. The kernel releases an assertion when its process exits,
//! crash included, so nothing can outlive the app.

use std::sync::Mutex;

#[derive(Debug)]
pub enum KeepAwakeError {
    InvalidArgument(String),
    Unsupported(String),
    Other(String),
}

impl KeepAwakeError {
    pub fn message(&self) -> &str {
        match self {
            KeepAwakeError::InvalidArgument(m) | KeepAwakeError::Unsupported(m) | KeepAwakeError::Other(m) => m,
        }
    }
}

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::{c_char, c_void, CString};

    pub type AssertionId = u32;
    type CFStringRef = *const c_void;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_IOPM_ASSERTION_LEVEL_ON: u32 = 255;
    const K_IO_RETURN_SUCCESS: i32 = 0;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(alloc: *const c_void, text: *const c_char, encoding: u32) -> CFStringRef;
        fn CFRelease(value: *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(kind: CFStringRef, level: u32, name: CFStringRef, id: *mut AssertionId) -> i32;
        fn IOPMAssertionRelease(id: AssertionId) -> i32;
    }

    fn cf_string(text: &str) -> CFStringRef {
        let text = CString::new(text.replace('\0', "")).expect("NULs removed");
        unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
    }

    pub fn create(display: bool, reason: &str) -> Result<AssertionId, String> {
        // The display assertion also keeps the system from idle-sleeping.
        let kind = cf_string(if display { "PreventUserIdleDisplaySleep" } else { "PreventUserIdleSystemSleep" });
        let name = cf_string(reason);
        let mut id: AssertionId = 0;
        let result = unsafe { IOPMAssertionCreateWithName(kind, K_IOPM_ASSERTION_LEVEL_ON, name, &mut id) };
        unsafe {
            CFRelease(kind);
            CFRelease(name);
        }
        if result == K_IO_RETURN_SUCCESS {
            Ok(id)
        } else {
            Err(format!("IOPMAssertionCreateWithName failed (IOReturn {result:#x})"))
        }
    }

    pub fn release(id: AssertionId) {
        unsafe { IOPMAssertionRelease(id) };
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Held {
    pub reason: String,
    pub display: bool,
}

#[cfg(target_os = "macos")]
static HELD: Mutex<Option<(native::AssertionId, Held)>> = Mutex::new(None);

/// Holds the app's one assertion, replacing any previous one (so a new
/// reason or display choice takes effect).
#[cfg(target_os = "macos")]
pub fn start(reason: &str, display: bool) -> Result<(), KeepAwakeError> {
    if reason.trim().is_empty() {
        return Err(KeepAwakeError::InvalidArgument("a reason is required — the OS shows it to the user".to_string()));
    }
    let mut held = HELD.lock().expect("keep-awake mutex poisoned");
    // Create the new one before releasing the old, so there's no gap.
    let id = native::create(display, reason).map_err(KeepAwakeError::Other)?;
    if let Some((old, _)) = held.replace((id, Held { reason: reason.to_string(), display })) {
        native::release(old);
    }
    Ok(())
}

/// Idempotent.
#[cfg(target_os = "macos")]
pub fn stop() {
    if let Some((id, _)) = HELD.lock().expect("keep-awake mutex poisoned").take() {
        native::release(id);
    }
}

#[cfg(target_os = "macos")]
pub fn held() -> Option<Held> {
    HELD.lock().expect("keep-awake mutex poisoned").as_ref().map(|(_, held)| held.clone())
}

#[cfg(not(target_os = "macos"))]
pub fn start(_reason: &str, _display: bool) -> Result<(), KeepAwakeError> {
    Err(KeepAwakeError::Unsupported("keeping the computer awake isn't implemented on this platform yet".to_string()))
}

#[cfg(not(target_os = "macos"))]
pub fn stop() {}

#[cfg(not(target_os = "macos"))]
pub fn held() -> Option<Held> {
    None
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// `pmset -g assertions` lists every assertion with its name.
    fn listed(reason: &str) -> bool {
        let output = std::process::Command::new("/usr/bin/pmset").args(["-g", "assertions"]).output().unwrap();
        String::from_utf8_lossy(&output.stdout).contains(reason)
    }

    #[test]
    fn holds_one_named_assertion_until_stopped() {
        let first = format!("chain-test-{}", crate::files::generate_id());
        start(&first, true).unwrap();
        assert!(listed(&first), "pmset doesn't list {first}");
        assert_eq!(held(), Some(Held { reason: first.clone(), display: true }));

        let second = format!("chain-test-{}", crate::files::generate_id());
        start(&second, false).unwrap();
        assert!(listed(&second));
        assert!(!listed(&first), "the replaced assertion is still held");

        stop();
        stop();
        assert!(!listed(&second));
        assert_eq!(held(), None);
        assert!(matches!(start("  ", true), Err(KeepAwakeError::InvalidArgument(_))));
    }
}
