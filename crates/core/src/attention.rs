//! Attention capability — see /agent-docs/capabilities/attention/CONTRACT.md.
//! System notifications whose clicks come back to the app with the app's
//! own id. On macOS that's UNUserNotificationCenter through
//! swift/ChainAttention.swift, which only works in an .app bundle (not under
//! `chain dev`). Focus and the Dock bounce need no code here: Tauri's window
//! already has both.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    Granted,
    Denied,
    NotDetermined,
    /// No notification system to ask: an unbundled `chain dev` binary, or a
    /// platform without an implementation yet.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum Notified {
    Shown,
    /// The user said no (now or earlier, in System Settings).
    Denied,
    Unavailable,
    /// The system refused, with its own reason — not the user's answer.
    Failed { message: String },
}

pub type OnClick = Box<dyn Fn(String) + Send + Sync>;

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::sync::mpsc;

    use super::*;

    type Click = extern "C" fn(*mut c_void, *const c_char);
    type Done = extern "C" fn(*mut c_void, i32, *const c_char);

    extern "C" {
        fn chain_attention_is_bundled() -> bool;
        fn chain_attention_start(context: *mut c_void, on_click: Click);
        fn chain_attention_permission(context: *mut c_void, done: Done);
        fn chain_attention_notify(id: *const c_char, title: *const c_char, body: *const c_char, context: *mut c_void, done: Done);
    }

    extern "C" fn forward_click(context: *mut c_void, id: *const c_char) {
        let on_click = unsafe { &*context.cast::<OnClick>() };
        on_click(unsafe { CStr::from_ptr(id) }.to_string_lossy().into_owned());
    }

    type Reply = (i32, String);

    /// `context` is a leaked `mpsc::Sender<Reply>`; Swift calls back once.
    extern "C" fn forward_done(context: *mut c_void, status: i32, message: *const c_char) {
        let sender = unsafe { Box::from_raw(context.cast::<mpsc::Sender<Reply>>()) };
        let _ = sender.send((status, unsafe { CStr::from_ptr(message) }.to_string_lossy().into_owned()));
    }

    fn wait_for(call: impl FnOnce(*mut c_void, Done)) -> Reply {
        let (sender, receiver) = mpsc::channel();
        call(Box::into_raw(Box::new(sender)).cast(), forward_done);
        receiver.recv().unwrap_or((-1, String::new()))
    }

    pub fn is_bundled() -> bool {
        unsafe { chain_attention_is_bundled() }
    }

    /// Once, at launch. The handler lives for the rest of the process.
    pub fn start(on_click: OnClick) {
        let context = Box::into_raw(Box::new(on_click));
        unsafe { chain_attention_start(context.cast(), forward_click) };
    }

    pub fn permission() -> Permission {
        match wait_for(|context, done| unsafe { chain_attention_permission(context, done) }).0 {
            0 => Permission::NotDetermined,
            1 => Permission::Denied,
            2 => Permission::Granted,
            _ => Permission::Unavailable,
        }
    }

    /// Blocks until shown — the first call waits for the user to answer
    /// the permission prompt. Call off the main thread.
    pub fn notify(id: &str, title: &str, body: &str) -> Notified {
        let text = |s: &str| CString::new(s.replace('\0', "")).expect("NULs removed");
        let (id, title, body) = (text(id), text(title), text(body));
        match wait_for(|context, done| unsafe {
            chain_attention_notify(id.as_ptr(), title.as_ptr(), body.as_ptr(), context, done)
        }) {
            (0, _) => Notified::Shown,
            (1, _) => Notified::Denied,
            (-1, _) => Notified::Unavailable,
            (_, message) => Notified::Failed { message },
        }
    }
}

#[cfg(target_os = "macos")]
pub use native::{is_bundled, notify, permission, start};

#[cfg(not(target_os = "macos"))]
pub fn is_bundled() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn start(_on_click: OnClick) {}

#[cfg(not(target_os = "macos"))]
pub fn permission() -> Permission {
    Permission::Unavailable
}

#[cfg(not(target_os = "macos"))]
pub fn notify(_id: &str, _title: &str, _body: &str) -> Notified {
    Notified::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    // `cargo test` runs a bare binary, which is exactly the case that must
    // not touch UNUserNotificationCenter (it would abort the process).
    #[test]
    fn an_unbundled_process_reports_unavailable_instead_of_crashing() {
        assert!(!is_bundled());
        start(Box::new(|_| {}));
        assert_eq!(permission(), Permission::Unavailable);
        assert_eq!(notify("run-1", "Title", "Body"), Notified::Unavailable);
    }
}
