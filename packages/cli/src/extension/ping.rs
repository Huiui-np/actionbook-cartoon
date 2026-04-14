use std::time::Instant;

use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::bridge::BRIDGE_PORT;

pub const COMMAND_NAME: &str = "extension ping";

pub async fn execute() -> ActionResult {
    let port: u16 = std::env::var("ACTIONBOOK_EXTENSION_BRIDGE_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(BRIDGE_PORT);

    let start = Instant::now();
    match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
        Ok(_) => {
            let rtt_ms = start.elapsed().as_millis() as u64;
            ActionResult::ok(json!({
                "bridge": "listening",
                "rtt_ms": rtt_ms,
            }))
        }
        Err(_) => ActionResult::ok(json!({
            "bridge": "not_listening",
            "rtt_ms": serde_json::Value::Null,
        })),
    }
}
