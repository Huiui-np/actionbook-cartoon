use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::{NetworkRequestsFilter, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// List tracked network requests for a tab.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser network requests --session s1 --tab t1
  actionbook browser network requests --filter /api/ --session s1 --tab t1
  actionbook browser network requests --type xhr,fetch --session s1 --tab t1
  actionbook browser network requests --method POST --session s1 --tab t1
  actionbook browser network requests --status 2xx --session s1 --tab t1
  actionbook browser network requests --clear --session s1 --tab t1
  actionbook browser network requests --dump --out /tmp/requests --session s1 --tab t1

Lists all network requests captured since the tab was attached (or since last --clear).
Requests are captured automatically — no setup required.
Use --filter for URL substring, --type for resource type (comma-separated),
--method for HTTP method, --status for status code (200, 2xx, 400-499).
Use --clear to reset the request buffer and return {cleared: true}.
Use --dump --out <dir> to write all matched requests (with response bodies) to <dir>/requests.json.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Filter by URL substring
    #[arg(long)]
    pub filter: Option<String>,
    /// Filter by resource type (comma-separated, e.g. xhr,fetch)
    #[arg(long = "type")]
    #[serde(rename = "resource_type")]
    pub resource_type: Option<String>,
    /// Filter by HTTP method (case-insensitive, e.g. POST)
    #[arg(long)]
    pub method: Option<String>,
    /// Filter by status code: exact (200), class (2xx), or range (400-499)
    #[arg(long)]
    pub status: Option<String>,
    /// Clear request buffer after retrieval (returns {cleared: true, count: N})
    #[arg(long)]
    pub clear: bool,
    /// Dump matched requests (with response bodies) to <--out>/requests.json
    #[arg(long)]
    #[serde(default)]
    pub dump: bool,
    /// Output directory for --dump (required when --dump is set)
    #[arg(long)]
    pub out: Option<String>,
}

pub const COMMAND_NAME: &str = "browser network requests";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("__ctx_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            data.get("__ctx_title")
                .and_then(|v| v.as_str())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    if cmd.dump && cmd.out.is_none() {
        return ActionResult::fatal("INVALID_ARGUMENT", "--out <dir> is required with --dump");
    }

    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let cdp_session_id = match cdp.get_cdp_session_id(&target_id).await {
        Some(sid) => sid,
        None => {
            return ActionResult::fatal(
                "INTERNAL_ERROR",
                format!("no CDP session for target '{target_id}'"),
            );
        }
    };

    if cmd.clear {
        let count = cdp.clear_network_requests(&cdp_session_id).await;
        let url = navigation::get_tab_url(&cdp, &target_id).await;
        let title = navigation::get_tab_title(&cdp, &target_id).await;
        return ActionResult::ok(json!({
            "cleared": true,
            "count": count,
            "__ctx_url": url,
            "__ctx_title": title,
        }));
    }

    let filter = NetworkRequestsFilter {
        url_substring: cmd.filter.clone(),
        resource_types: cmd.resource_type.clone(),
        method: cmd.method.clone(),
        status: cmd.status.clone(),
    };

    let total = cdp.network_requests_total(&cdp_session_id).await;
    let matched = cdp.network_requests(&cdp_session_id, &filter).await;
    let filtered = matched.len();

    if cmd.dump {
        let out_dir = cmd.out.as_deref().unwrap();
        if let Err(e) = std::fs::create_dir_all(out_dir) {
            return ActionResult::fatal(
                "IO_ERROR",
                format!("failed to create output directory '{out_dir}': {e}"),
            );
        }

        let mut dump_requests: Vec<Value> = Vec::with_capacity(matched.len());
        for req in &matched {
            let (response_body, body_error): (Value, Value) = match cdp
                .execute_on_tab(
                    &target_id,
                    "Network.getResponseBody",
                    json!({ "requestId": req.request_id }),
                )
                .await
            {
                Ok(resp) => {
                    let body = resp
                        .pointer("/result/body")
                        .and_then(|v| v.as_str())
                        .map(|s| Value::String(s.to_string()))
                        .unwrap_or(Value::Null);
                    (body, Value::Null)
                }
                Err(e) => (Value::Null, Value::String(e.to_string())),
            };

            dump_requests.push(json!({
                "request_id": req.request_id,
                "url": req.url,
                "method": req.method,
                "resource_type": req.resource_type,
                "timestamp": req.timestamp_ms,
                "status": req.status,
                "mime_type": req.mime_type,
                "request_headers": req.request_headers,
                "response_headers": req.response_headers,
                "response_body": response_body,
                "body_error": body_error,
            }));
        }

        let dump_path = std::path::Path::new(out_dir).join("requests.json");
        let dump_json = json!({ "requests": dump_requests });
        let dump_bytes = match serde_json::to_vec_pretty(&dump_json) {
            Ok(b) => b,
            Err(e) => {
                return ActionResult::fatal(
                    "INTERNAL_ERROR",
                    format!("failed to serialize dump: {e}"),
                );
            }
        };
        if let Err(e) = std::fs::write(&dump_path, &dump_bytes) {
            return ActionResult::fatal(
                "IO_ERROR",
                format!("failed to write '{}': {e}", dump_path.display()),
            );
        }

        let url = navigation::get_tab_url(&cdp, &target_id).await;
        let title = navigation::get_tab_title(&cdp, &target_id).await;
        return ActionResult::ok(json!({
            "dump": {
                "path": dump_path.to_string_lossy(),
                "count": filtered,
            },
            "__ctx_url": url,
            "__ctx_title": title,
        }));
    }

    let requests: Vec<Value> = matched
        .into_iter()
        .map(|req| {
            json!({
                "request_id": req.request_id,
                "url": req.url,
                "method": req.method,
                "resource_type": req.resource_type,
                "timestamp": req.timestamp_ms,
                "status": req.status,
                "mime_type": req.mime_type,
                "request_headers": req.request_headers,
                "response_headers": req.response_headers,
            })
        })
        .collect();

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    ActionResult::ok(json!({
        "requests": requests,
        "total": total,
        "filtered": filtered,
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}
