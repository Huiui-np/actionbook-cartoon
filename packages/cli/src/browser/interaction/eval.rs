use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::browser::observation::logs_console::ENSURE_LOG_CAPTURE_JS;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Evaluate JavaScript
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser eval \"document.title\" --session s1 --tab t1
  actionbook browser eval \"window.scrollY\" --session s1 --tab t1
  actionbook browser eval \"document.querySelectorAll('a').length\" --session s1 --tab t1

Evaluates a JavaScript expression in the page context and returns the result.
The expression is evaluated via Runtime.evaluate with returnByValue.

By default each eval runs in an isolated scope so that let/const declarations do
not leak across calls on the same tab.  Single-expression await works transparently
(e.g. 'await fetch(url).then(r => r.json())').

Note: Multi-statement expressions that contain 'await' (e.g.
'let x = await Promise.resolve(42); x + 1') are not supported under the default
isolated mode — use --no-isolate or wrap the body in an explicit async arrow:
  actionbook browser eval \"(async () => { let x = await f(); return x + 1; })()\" ...")]
pub struct Cmd {
    /// JavaScript expression
    pub expression: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Disable scope isolation (allow let/const to persist across evals on the same tab)
    #[arg(long)]
    #[serde(default)]
    pub no_isolate: bool,
}

pub const COMMAND_NAME: &str = "browser eval";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("post_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            data.get("post_title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Install log capture hook before eval so console.* calls in the expression are captured.
    let _ = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": ENSURE_LOG_CAPTURE_JS, "returnByValue": true }),
        )
        .await;

    // Capture pre-execution page context for diagnostics.
    let pre_url = navigation::get_tab_url(&cdp, &target_id).await;
    let pre_origin = navigation::get_tab_origin(&cdp, &target_id).await;
    let pre_ready_state = navigation::get_tab_ready_state(&cdp, &target_id).await;

    // Build the expression to send to CDP.
    // By default, isolate scope so let/const don't leak across evals:
    //
    // - Expressions without top-level `await`: wrap with a regular function + eval().
    //   eval() preserves the completion value of multi-statement programs and
    //   scopes let/const to the function, preventing leakage.
    //
    // - Expressions with top-level `await`: embed directly in an async function body.
    //   eval() cannot inherit async context in this Chrome version (eval'd strings
    //   are parsed as Scripts, where await is invalid). The async IIFE makes await
    //   syntactically valid while still isolating let/const to the function scope.
    //   awaitPromise: true (already set) resolves the returned Promise.
    //
    // With --no-isolate, pass the expression directly (old behavior).
    let expression = if cmd.no_isolate {
        cmd.expression.clone()
    } else {
        // Detect top-level `await` anywhere in the expression (not just at start).
        // e.g. `(await Promise.resolve(42)) + 1` has await after `(`.
        // Sync expressions work fine inside async functions too (awaitPromise unwraps).
        let has_await = cmd.expression.contains("await ") || cmd.expression.contains("await(");
        if has_await {
            format!("(async function(){{ return (\n{}\n); }})()", cmd.expression)
        } else {
            let escaped = serde_json::to_string(&cmd.expression).unwrap_or_default();
            format!("(function(){{ return eval({}); }})()", escaped)
        }
    };

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            let details = json!({
                "stage": "eval",
                "pre_url": pre_url,
                "pre_origin": pre_origin,
                "pre_readyState": pre_ready_state,
                "error_type": "CdpError",
            });
            return ActionResult::fatal_with_details("EVAL_FAILED", e.to_string(), "", details);
        }
    };

    // Extract value from CDP response
    if let Some(result) = resp.get("result").and_then(|r| r.get("result")) {
        if let Some(exc) = resp.get("result").and_then(|r| r.get("exceptionDetails")) {
            // Prefer exception.description (e.g. "Error: boom-eval"), fall back to text
            let emsg = exc
                .pointer("/exception/description")
                .and_then(|v| v.as_str())
                .or_else(|| exc.get("text").and_then(|v| v.as_str()))
                .unwrap_or("expression error");

            let error_type = exc
                .pointer("/exception/className")
                .and_then(|v| v.as_str())
                .unwrap_or("Error")
                .to_string();

            let details = json!({
                "stage": "eval",
                "pre_url": pre_url,
                "pre_origin": pre_origin,
                "pre_readyState": pre_ready_state,
                "error_type": error_type,
            });

            return ActionResult::fatal_with_details("EVAL_FAILED", emsg.to_string(), "", details);
        }

        let js_type = result
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("undefined")
            .to_string();

        // Return the typed value as-is from CDP (number, bool, string, etc.)
        let value = result.get("value").cloned().unwrap_or(json!(null));

        let preview = result
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                if value.is_string() {
                    value.as_str().unwrap().to_string()
                } else {
                    value.to_string()
                }
            });

        let post_url = navigation::get_tab_url(&cdp, &target_id).await;
        let post_title = navigation::get_tab_title(&cdp, &target_id).await;

        ActionResult::ok(json!({
            "value": value,
            "type": js_type,
            "preview": preview,
            "pre_url": pre_url,
            "pre_origin": pre_origin,
            "pre_readyState": pre_ready_state,
            "post_url": post_url,
            "post_title": post_title,
        }))
    } else {
        let details = json!({
            "stage": "eval",
            "pre_url": pre_url,
            "pre_origin": pre_origin,
            "pre_readyState": pre_ready_state,
            "error_type": "CdpError",
        });
        ActionResult::fatal_with_details("EVAL_FAILED", "no result in CDP response", "", details)
    }
}
