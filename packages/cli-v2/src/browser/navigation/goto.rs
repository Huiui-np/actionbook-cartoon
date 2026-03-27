use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::{cdp_navigate, ensure_scheme};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;
use crate::types::TabId;

/// Navigate to URL
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Target URL
    pub url: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.goto";

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
    let final_url = ensure_scheme(&cmd.url);

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
        let parsed_tab: TabId = match cmd.tab.parse() {
            Ok(t) => t,
            Err(e) => {
                return ActionResult::fatal("INVALID_ARGUMENT", format!("invalid tab id: {e}"));
            }
        };
        let tab = match entry.tabs.iter().find(|t| t.id == parsed_tab) {
            Some(t) => t,
            None => {
                return ActionResult::fatal(
                    "TAB_NOT_FOUND",
                    format!("tab '{}' not found", cmd.tab),
                );
            }
        };
        if tab.target_id.is_empty() {
            None
        } else {
            Some(format!(
                "ws://127.0.0.1:{}/devtools/page/{}",
                entry.cdp_port, tab.target_id
            ))
        }
    };

    if let Some(ref ws) = ws_url {
        let _ = cdp_navigate(ws, &final_url).await;
    }

    {
        let mut reg = registry.lock().await;
        if let Some(entry) = reg.get_mut(&cmd.session)
            && let Ok(parsed_tab) = cmd.tab.parse::<TabId>()
            && let Some(tab) = entry.tabs.iter_mut().find(|t| t.id == parsed_tab)
        {
            tab.url.clone_from(&final_url);
        }
    }

    ActionResult::ok(json!({
        "kind": "goto",
        "to_url": final_url,
    }))
}
