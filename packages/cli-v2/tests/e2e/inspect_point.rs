//! E2E tests for `browser inspect-point` command (§10.11).
//!
//! Tests are strict per api-reference.md §10.11.
//!
//! **Expected to FAIL until implementation lands:**
//! - `inspect_point_json_happy_path`
//! - `inspect_point_text_happy_path`
//! - `inspect_point_with_parent_depth`
//! - `inspect_point_no_element`
//!
//! **Expected to PASS against stub (error paths handled before command logic):**
//! - `inspect_point_session_not_found_json` / `_text`
//! - `inspect_point_tab_not_found_json`
//! - `inspect_point_missing_session_arg` / `_missing_tab_arg`
//! - `inspect_point_invalid_coords`

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};

const URL_A: &str = "https://actionbook.dev";

// ── Helpers ───────────────────────────────────────────────────────────

/// Start a headless session, navigate to url, return (session_id, tab_id).
fn start_session(url: &str) -> (String, String) {
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            url,
        ],
        30,
    );
    assert_success(&out, "start session");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    // Ensure navigation completes (CI may be slow)
    let goto_out = headless_json(
        &["browser", "goto", url, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&goto_out, "goto for session setup");

    (sid, tid)
}

/// Close a session.
fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

/// Assert full §2.4 meta structure.
fn assert_meta(v: &serde_json::Value) {
    assert!(
        v["meta"]["duration_ms"].is_number(),
        "meta.duration_ms must be a number"
    );
    assert!(
        v["meta"]["warnings"].is_array(),
        "meta.warnings must be an array"
    );
    assert!(
        v["meta"]["pagination"].is_null(),
        "meta.pagination must be null"
    );
    assert!(
        v["meta"]["truncated"].is_boolean(),
        "meta.truncated must be a boolean"
    );
}

/// Assert full §3.1 error envelope.
fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
    assert_eq!(v["ok"], false, "ok must be false on error");
    assert!(v["data"].is_null(), "data must be null on failure");
    assert_eq!(v["error"]["code"], expected_code);
    assert!(
        v["error"]["message"].is_string(),
        "error.message must be a string"
    );
    assert!(
        v["error"]["retryable"].is_boolean(),
        "error.retryable must be a boolean"
    );
    assert!(
        v["error"]["details"].is_object() || v["error"]["details"].is_null(),
        "error.details must be object or null"
    );
    assert_meta(v);
}

// ===========================================================================
// Group 1: Happy path
// ===========================================================================

#[test]
fn inspect_point_json_happy_path() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // Use coordinates near the center of the page — should hit some element
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "inspect-point json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.inspect-point");
    assert!(v["error"].is_null());
    assert_meta(&v);

    // context — tab-level, including url per §2.5
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be a string"
    );

    // §10.11 data contract
    assert!(v["data"]["point"].is_object(), "data.point must be object");
    assert_eq!(v["data"]["point"]["x"], 100.0);
    assert_eq!(v["data"]["point"]["y"], 100.0);

    assert!(
        v["data"]["element"].is_object(),
        "data.element must be object"
    );
    assert!(
        v["data"]["element"]["role"].is_string(),
        "element.role must be a string"
    );
    assert!(
        v["data"]["element"]["name"].is_string(),
        "element.name must be a string"
    );
    assert!(
        v["data"]["element"]["selector"].is_string(),
        "element.selector must be a string (ref)"
    );

    assert!(
        v["data"]["parents"].is_array(),
        "data.parents must be an array"
    );

    // No screenshot_path per mcfeng's direction
    assert!(
        v["data"]["screenshot_path"].is_null() || v["data"].get("screenshot_path").is_none(),
        "screenshot_path should not be present"
    );

    close_session(&sid);
}

#[test]
fn inspect_point_text_happy_path() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    let out = headless(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "inspect-point text");
    let text = stdout_str(&out);

    // §2.5: header is `[sid tid] <url>`
    let header_line = text.lines().next().unwrap_or("");
    assert!(
        header_line.starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: got {header_line}"
    );

    // §10.11: body contains role line, selector line, point line
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.len() >= 3,
        "text must have header + role + selector + point: got {text:.300}"
    );

    // Should have a "point: x,y" line
    let has_point_line = lines.iter().any(|l| l.starts_with("point: "));
    assert!(
        has_point_line,
        "must contain 'point: x,y' line: got {text:.300}"
    );

    close_session(&sid);
}

#[test]
fn inspect_point_with_parent_depth() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--parent-depth",
            "2",
        ],
        10,
    );
    assert_success(&out, "inspect-point with parent-depth");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["data"]["parents"].is_array(), "parents must be an array");
    let parents = v["data"]["parents"].as_array().unwrap();
    // With --parent-depth 2, should have up to 2 parent entries
    assert!(
        parents.len() <= 2,
        "parents should have at most 2 entries: got {}",
        parents.len()
    );
    // Each parent should have role, name, selector
    for (i, parent) in parents.iter().enumerate() {
        assert!(
            parent["role"].is_string(),
            "parents[{i}].role must be a string"
        );
        assert!(
            parent["name"].is_string(),
            "parents[{i}].name must be a string"
        );
        assert!(
            parent["selector"].is_string(),
            "parents[{i}].selector must be a string"
        );
    }

    close_session(&sid);
}

#[test]
fn inspect_point_no_element() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // Use coordinates far outside the viewport — should return null element
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "-9999,-9999",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "inspect-point no element");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["point"]["x"], -9999.0);
    assert_eq!(v["data"]["point"]["y"], -9999.0);
    // element should be null when no element at coordinates
    assert!(
        v["data"]["element"].is_null(),
        "element must be null when no element at coordinates: got {:?}",
        v["data"]["element"]
    );

    close_session(&sid);
}

// ===========================================================================
// Group 2: Error paths
// ===========================================================================

#[test]
fn inspect_point_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            "nonexistent",
            "--tab",
            "t0",
        ],
        10,
    );
    assert_failure(&out, "inspect-point session not found");
    let v = parse_json(&out);
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null on SESSION_NOT_FOUND"
    );
}

#[test]
fn inspect_point_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            "nonexistent",
            "--tab",
            "t0",
        ],
        10,
    );
    assert_failure(&out, "inspect-point session not found text");
    let text = stdout_str(&out);
    assert!(
        text.contains("SESSION_NOT_FOUND"),
        "text must contain SESSION_NOT_FOUND: got {text:.200}"
    );
}

#[test]
fn inspect_point_tab_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session(URL_A);

    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab",
        ],
        10,
    );
    assert_failure(&out, "inspect-point tab not found");
    let v = parse_json(&out);
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_null(),
        "tab_id must be null on TAB_NOT_FOUND"
    );

    close_session(&sid);
}

#[test]
fn inspect_point_missing_session_arg() {
    if skip() {
        return;
    }
    // Missing --session should fail at clap level
    let out = headless_json(&["browser", "inspect-point", "100,100", "--tab", "t0"], 10);
    assert_failure(&out, "inspect-point missing --session");
}

#[test]
fn inspect_point_missing_tab_arg() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            "any-sid",
        ],
        10,
    );
    assert_failure(&out, "inspect-point missing --tab");
}

#[test]
fn inspect_point_invalid_coords() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // Invalid coordinate format — should fail with INVALID_ARGUMENT or similar
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "not-valid",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "inspect-point invalid coords");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);

    close_session(&sid);
}
