//! Chain Core — see /docs/ARCHITECTURE.md.
//! Coordinates capability calls, normalizes errors, and routes to the
//! native adapter for the current platform. No Tauri or native types are
//! allowed to leak back into the Chain SDK across this boundary.

pub mod agent_server;
pub mod attention;
pub mod audio_recorder;
pub mod browser;
pub mod dev_launch;
pub mod dev_trace;
pub mod embeddings;
pub mod files;
pub mod folders;
pub mod http;
pub mod keep_awake;
pub mod m4a;
mod microphone_processor;
pub mod models;
pub mod page_zoom;
pub mod pdf;
pub mod platform;
pub mod ports;
pub mod process_runner;
pub mod process_tree;
mod recorder_tracks;
pub mod share;
pub mod sherpa;
mod sound;
pub mod speech;
pub mod storage;
pub mod terminal;
pub mod vision;
pub mod window;
