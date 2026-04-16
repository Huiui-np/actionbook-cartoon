//! Integration tests for `actionbook manual`.
//!
//! Each test spins up a local `wiremock` HTTP server, programs it with the
//! exact wire format that
//! `action-api-server/apps/server/src/app/api/manual/route.ts` returns at each
//! depth (L1 site / L2 group / L3 action), and runs the real CLI binary via
//! `assert_cmd` with `ACTIONBOOK_API_URL` pointing at the mock.

use assert_cmd::Command;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// L1 payload — mirrors `route.ts:45-61`. Actions are full objects (not bare
/// strings), which exercises the `SiteAction::Detailed` untagged-enum variant
/// against the server's real shape rather than the synthetic string fixtures
/// used in the unit tests.
fn level1_fixture() -> Value {
    json!({
        "success": true,
        "data": {
            "name": "github",
            "description": "GitHub REST API available at `https://api.github.com`.",
            "authentication": null,
            "groups": [
                {
                    "name": "issues",
                    "base_url": "https://api.github.com",
                    "actions": [
                        {
                            "name": "list_GET",
                            "method": "GET",
                            "path": "/issues",
                            "source_type": "captured",
                            "base_url": "https://api.github.com",
                            "summary": "List issues"
                        },
                        {
                            "name": "create_POST",
                            "method": "POST",
                            "path": "/issues",
                            "source_type": "captured",
                            "base_url": "https://api.github.com",
                            "summary": "Create an issue"
                        }
                    ]
                },
                {
                    "name": "repos",
                    "base_url": "https://api.github.com",
                    "actions": [
                        {
                            "name": "get_GET",
                            "method": "GET",
                            "path": "/repos/{owner}/{repo}",
                            "source_type": "captured",
                            "base_url": "https://api.github.com",
                            "summary": "Get a repository"
                        }
                    ]
                }
            ]
        }
    })
}

/// L2 payload — mirrors `route.ts:73-84`.
fn level2_fixture() -> Value {
    json!({
        "success": true,
        "data": {
            "group": "issues",
            "base_url": "https://api.github.com",
            "actions": [
                {
                    "name": "list_GET",
                    "method": "GET",
                    "path": "/issues",
                    "source_type": "captured",
                    "base_url": "https://api.github.com",
                    "summary": "List issues"
                },
                {
                    "name": "create_POST",
                    "method": "POST",
                    "path": "/issues",
                    "source_type": "captured",
                    "base_url": "https://api.github.com",
                    "summary": "Create an issue"
                }
            ]
        }
    })
}

/// L3 payload — mirrors `route.ts:106-119`. `parameters`, `requestBody`,
/// `responses`, `authentication` always empty/null from this server.
fn level3_fixture() -> Value {
    json!({
        "success": true,
        "data": {
            "site": "github",
            "group": "issues",
            "action": "list_GET",
            "method": "GET",
            "path": "/issues",
            "source_type": "captured",
            "base_url": "https://api.github.com",
            "description": "List issues assigned to the authenticated user across all visible repositories.",
            "parameters": [],
            "requestBody": null,
            "responses": [],
            "authentication": null
        }
    })
}

#[tokio::test]
async fn manual_level1_site_overview() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/manual"))
        .and(query_param("site", "github"))
        .respond_with(ResponseTemplate::new(200).set_body_json(level1_fixture()))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["manual", "github"])
        .output()
        .expect("run actionbook manual");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(stdout.contains("=== github"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("Base URL:  https://api.github.com"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("2 groups, 3 actions total"),
        "stdout:\n{stdout}"
    );
    assert!(stdout.contains("issues"), "stdout:\n{stdout}");
    assert!(stdout.contains("repos"), "stdout:\n{stdout}");
    assert!(stdout.contains("list_GET"), "stdout:\n{stdout}");
    assert!(stdout.contains("create_POST"), "stdout:\n{stdout}");
    assert!(stdout.contains("get_GET"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("Run actionbook manual <SITE> [GROUP] [ACTION] for full details."),
        "stdout:\n{stdout}"
    );
}

#[tokio::test]
async fn manual_level2_group_overview() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/manual"))
        .and(query_param("site", "github"))
        .and(query_param("group", "issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(level2_fixture()))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["manual", "github", "issues"])
        .output()
        .expect("run actionbook manual");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(stdout.contains("site:      github"), "stdout:\n{stdout}");
    assert!(stdout.contains("group:     issues"), "stdout:\n{stdout}");
    assert!(stdout.contains("actions:   2"), "stdout:\n{stdout}");
    assert!(stdout.contains("list_GET"), "stdout:\n{stdout}");
    assert!(stdout.contains("GET"), "stdout:\n{stdout}");
    assert!(stdout.contains("/issues"), "stdout:\n{stdout}");
    assert!(stdout.contains("List issues"), "stdout:\n{stdout}");
    assert!(stdout.contains("create_POST"), "stdout:\n{stdout}");
    assert!(stdout.contains("POST"), "stdout:\n{stdout}");
}

#[tokio::test]
async fn manual_level3_action_detail() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/manual"))
        .and(query_param("site", "github"))
        .and(query_param("group", "issues"))
        .and(query_param("action", "list_GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(level3_fixture()))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["manual", "github", "issues", "list_GET"])
        .output()
        .expect("run actionbook manual");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(stdout.contains("=== list_GET"), "stdout:\n{stdout}");
    assert!(stdout.contains("site:      github"), "stdout:\n{stdout}");
    assert!(stdout.contains("method:    GET"), "stdout:\n{stdout}");
    assert!(stdout.contains("path:      /issues"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("base_url:  https://api.github.com"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("List issues assigned to the authenticated user"),
        "stdout:\n{stdout}"
    );
}

#[tokio::test]
async fn manual_level3_json_mode() {
    let mock = MockServer::start().await;
    let fixture = level3_fixture();
    let expected_data = fixture.get("data").expect("fixture has data field").clone();
    Mock::given(method("GET"))
        .and(path("/api/manual"))
        .and(query_param("site", "github"))
        .and(query_param("group", "issues"))
        .and(query_param("action", "list_GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["--json", "manual", "github", "issues", "list_GET"])
        .output()
        .expect("run actionbook --json manual");

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let parsed: Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON in --json mode");
    assert_eq!(
        parsed, expected_data,
        "--json output should be the unwrapped data field"
    );
}

#[tokio::test]
async fn manual_404_group_renders_available_hint() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/manual"))
        .and(query_param("site", "github"))
        .and(query_param("group", "unknown"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "success": false,
            "error": {
                "code": "NOT_FOUND",
                "message": "Group \"unknown\" not found in site \"github\".",
                "available": ["issues", "repos"]
            }
        })))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["manual", "github", "unknown"])
        .output()
        .expect("run actionbook manual");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected non-zero exit on 404\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("Group \"unknown\" not found in site \"github\"."),
        "stderr should surface server message\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("Available: issues, repos"),
        "stderr should surface available hint\nstderr:\n{stderr}"
    );
}

#[tokio::test]
async fn manual_no_args_exits_nonzero() {
    // No mock server needed — the command should fail before reaching the network.
    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", "http://127.0.0.1:1")
        .args(["manual"])
        .output()
        .expect("run actionbook manual");

    assert!(
        !output.status.success(),
        "expected non-zero exit when site is missing\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
