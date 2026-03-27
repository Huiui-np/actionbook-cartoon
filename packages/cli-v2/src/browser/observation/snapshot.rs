use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::{cdp_get_ax_tree, resolve_tab_ws_url};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Capture accessibility snapshot
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.snapshot";

pub fn context(cmd: &Cmd, _result: &ActionResult) -> Option<ResponseContext> {
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let ws_url = {
        let reg = registry.lock().await;
        let entry = match reg.get(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                );
            }
        };
        match resolve_tab_ws_url(&cmd.tab, entry) {
            Ok(url) => url,
            Err(err) => return err,
        }
    };

    match cdp_get_ax_tree(&ws_url).await {
        Ok(snapshot) => ActionResult::ok(json!({ "snapshot": snapshot })),
        Err(e) => ActionResult::fatal("INTERNAL_ERROR", e.to_string()),
    }
}
