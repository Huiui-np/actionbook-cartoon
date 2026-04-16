use std::collections::VecDeque;
use std::time::{Duration, Instant};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const POLL_INTERVAL_MS: u64 = 100;
/// Strict idle: zero in-flight requests for this long.
const STRICT_IDLE_QUIET_MS: u64 = 500;
/// Relaxed idle: fewer than RELAXED_MAX_REQUESTS new requests in the sliding
/// window, sustained for this long.  Used when a page has persistent
/// background activity (analytics pings, health-checks, etc.) that would
/// otherwise prevent the strict condition from ever being satisfied.
const RELAXED_IDLE_QUIET_MS: u64 = 3_000;
/// Sliding-window length for the relaxed-idle request-rate check.
const RELAXED_WINDOW_MS: u64 = 10_000;
/// Max new requests allowed inside the sliding window to qualify as relaxed idle.
const RELAXED_MAX_REQUESTS: usize = 5;

/// Wait for network activity to become idle
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser wait network-idle --session s1 --tab t1 --timeout 10000")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Timeout in milliseconds (default 30000)
    #[arg(long)]
    pub timeout: Option<u64>,
}

pub const COMMAND_NAME: &str = "browser wait network-idle";

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
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let timeout_ms = cmd.timeout.unwrap_or(DEFAULT_TIMEOUT_MS);
    let start = Instant::now();

    // Resolve the CDP flat-session ID.  `attach()` already called `Network.enable`
    // on this session, so `tab_net_pending` tracks ALL requests since tab attachment —
    // including those that started before this `wait network-idle` call.
    let cdp_session_id = match cdp.get_cdp_session_id(&target_id).await {
        Some(sid) => sid,
        None => {
            return ActionResult::fatal(
                "INTERNAL_ERROR",
                format!("no CDP session for target '{target_id}'"),
            );
        }
    };

    // JS guard: readyState must be complete and DOM-attached <img> elements loaded.
    // This ensures the page itself has finished parsing, independent of XHR/fetch traffic.
    let js = r#"(function() {
        if (document.readyState !== 'complete') { return { ready: false, unloaded_imgs: 1 }; }
        var imgs = Array.prototype.slice.call(document.querySelectorAll('img'));
        var unloaded = imgs.filter(function(i) { return !i.complete; }).length;
        return { ready: true, unloaded_imgs: unloaded };
    })()"#;

    // --- idle tracking state ---
    // Timestamps of request-start events within the relaxed sliding window.
    // Each time `pending` increases we push one entry per new request.
    let mut request_events: VecDeque<Instant> = VecDeque::new();
    let mut prev_pending: i64 = 0;
    // When the idle condition first becomes true, record when it started so we
    // can enforce the required quiet window before declaring success.
    let mut quiet_start: Option<Instant> = None;
    // Whether the current quiet window is running in relaxed mode.
    let mut quiet_is_relaxed = false;

    loop {
        // Read the live in-flight counter maintained by reader_loop.
        let pending = cdp.network_pending(&cdp_session_id).await;

        // Track new request starts: any increase in `pending` means at least that
        // many requests were initiated since the last poll.
        if pending > prev_pending {
            let new_reqs = (pending - prev_pending) as usize;
            let now = Instant::now();
            for _ in 0..new_reqs {
                request_events.push_back(now);
            }
        }
        prev_pending = pending;

        // Evict events that have aged out of the sliding window.
        let window_cutoff = Instant::now() - Duration::from_millis(RELAXED_WINDOW_MS);
        while request_events.front().is_some_and(|t| *t < window_cutoff) {
            request_events.pop_front();
        }
        let recent_requests = request_events.len();

        // JS fallback: readyState + DOM-attached img.complete.
        let js_idle = cdp
            .execute_on_tab(
                &target_id,
                "Runtime.evaluate",
                json!({ "expression": js, "returnByValue": true }),
            )
            .await
            .ok()
            .and_then(|v| v.pointer("/result/result/value").cloned())
            .map(|rv| {
                let ready = rv.get("ready").and_then(|v| v.as_bool()).unwrap_or(false);
                let unloaded = rv
                    .get("unloaded_imgs")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                ready && unloaded == 0
            })
            .unwrap_or(false);

        // Determine which idle mode (if any) applies this tick.
        let strict_idle = pending == 0 && js_idle;
        // Relaxed idle: page is done (js_idle) and request rate is low enough.
        // We still require pending == 0 OR the long-running connection is clearly
        // background traffic (rate < threshold).
        let relaxed_idle = js_idle && recent_requests < RELAXED_MAX_REQUESTS;

        if strict_idle || relaxed_idle {
            let required_quiet = if strict_idle {
                STRICT_IDLE_QUIET_MS
            } else {
                RELAXED_IDLE_QUIET_MS
            };
            // If we just switched from relaxed→strict (or newly entered idle),
            // reset the quiet window so strict mode gets a fresh 500 ms run.
            let mode_changed = quiet_start.is_some() && quiet_is_relaxed && strict_idle;
            if quiet_start.is_none() || mode_changed {
                quiet_start = Some(Instant::now());
                quiet_is_relaxed = !strict_idle;
            }
            let quiet_elapsed_ms = quiet_start.unwrap().elapsed().as_millis() as u64;
            if quiet_elapsed_ms >= required_quiet {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                let url = navigation::get_tab_url(&cdp, &target_id).await;
                let title = navigation::get_tab_title(&cdp, &target_id).await;
                let mode = if strict_idle { "strict" } else { "relaxed" };
                return ActionResult::ok(json!({
                    "kind": "network-idle",
                    "satisfied": true,
                    "mode": mode,
                    "elapsed_ms": elapsed_ms,
                    "observed_value": {
                        "idle": true,
                        "pending": pending,
                        "recent_requests_10s": recent_requests,
                    },
                    "__ctx_url": url,
                    "__ctx_title": title,
                }));
            }
        } else {
            // Not idle — reset the quiet window entirely.
            quiet_start = None;
            quiet_is_relaxed = false;
        }

        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed >= timeout_ms {
            return ActionResult::fatal_with_hint(
                "TIMEOUT",
                format!("network did not become idle within {}ms", timeout_ms),
                "check that the page has finished loading or increase --timeout",
            );
        }

        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}
