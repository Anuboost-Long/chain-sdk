//! Chain Core — see /docs/ARCHITECTURE.md.
//! Coordinates capability calls, normalizes errors, and routes to the
//! native adapter for the current platform. No Tauri or native types are
//! allowed to leak back into the Chain SDK across this boundary.

pub mod agent_server;
pub mod audio_recorder;
pub mod browser;
pub mod dev_launch;
pub mod dev_trace;
pub mod embeddings;
pub mod files;
pub mod http;
pub mod m4a;
mod microphone_processor;
pub mod models;
pub mod platform;
pub mod process_runner;
mod recorder_tracks;
pub mod sherpa;
pub mod speech;
pub mod storage;
pub mod vision;
pub mod window;
