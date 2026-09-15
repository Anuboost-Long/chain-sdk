//! Chain Core — see /docs/ARCHITECTURE.md.
//! Coordinates capability calls, normalizes errors, and routes to the
//! native adapter for the current platform. No Tauri or native types are
//! allowed to leak back into the Chain SDK across this boundary.

pub mod platform;
pub mod storage;
