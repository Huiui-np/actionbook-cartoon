use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::bridge::BridgeListenerStatus;
use crate::daemon::registry::SharedRegistry;

/// Query extension bridge status.
#[derive(Args, Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cmd {}

pub const COMMAND_NAME: &str = "extension status";

pub async fn execute_daemon(_cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let bridge_arc = {
        let reg = registry.lock().await;
        reg.bridge_state().cloned()
    };

    let (bridge, extension_connected) = match bridge_arc {
        Some(state) => {
            let state = state.lock().await;
            let bridge_str = match state.listener_status() {
                BridgeListenerStatus::Listening => "listening",
                BridgeListenerStatus::Failed => "failed",
                BridgeListenerStatus::Binding => "not_listening",
            };
            (bridge_str, state.is_extension_connected())
        }
        None => ("not_listening", false),
    };

    ActionResult::ok(json!({
        "bridge": bridge,
        "extension_connected": extension_connected,
    }))
}
