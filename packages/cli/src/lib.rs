pub const BUILD_VERSION: &str = env!("BUILD_VERSION");

/// Minimum extension protocol version required to connect to the daemon.
/// Extensions reporting a lower version will be rejected with version_mismatch.
pub const EXTENSION_PROTOCOL_MIN_VERSION: &str = "0.4.0";

pub mod action;
pub mod action_result;
pub mod api;
pub mod api_response;
pub mod browser;
pub mod cli;
pub mod commands;
pub mod config;
pub mod daemon;
pub mod error;
pub mod extension;
pub mod formatter;
pub mod output;
pub mod setup;
pub mod types;
pub mod utils;
