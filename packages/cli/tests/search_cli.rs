//! Integration tests for `actionbook search`.
//!
//! Each test spins up a local `wiremock` HTTP server, programs it with the
//! exact wire format that
//! `action-api-server/apps/server/src/app/api/search/route.ts` returns, and
//! runs the real CLI binary via `assert_cmd` with `ACTIONBOOK_API_URL` pointing
//! at the mock. This exercises the full
//! `clap → reqwest → response parsing → formatter → stdout/stderr` pipeline.

use assert_cmd::Command;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Server-shape fixture for `/api/search?q=github` — one site, one group, one
/// action. Matches `route.ts:44-57` byte-for-byte (including `source_type`).
fn single_site_fixture() -> Value {
    json!({
        "success": true,
        "data": [
            {
                "name": "github",
                "description": "GitHub REST API",
                "groups": [
                    {
                        "name": "issues",
                        "actions": [
                            {
                                "name": "list_GET",
                                "method": "GET",
                                "path": "/issues",
                                "source_type": "captured",
                                "summary": "List issues"
                            }
                        ]
                    }
                ]
            }
        ]
    })
}

#[tokio::test]
async fn search_prints_results_against_mock_server() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("q", "github"))
        .respond_with(ResponseTemplate::new(200).set_body_json(single_site_fixture()))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["search", "github"])
        .output()
        .expect("run actionbook search");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(stdout.contains("github"), "stdout:\n{stdout}");
    assert!(stdout.contains("issues"), "stdout:\n{stdout}");
    assert!(stdout.contains("list_GET"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("1 actions from 1 sites"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Run actionbook manual <SITE> [GROUP] [ACTION] for full details."),
        "stdout:\n{stdout}"
    );
}

#[tokio::test]
async fn search_json_mode_outputs_raw_data() {
    let mock = MockServer::start().await;
    let fixture = single_site_fixture();
    let expected_data = fixture.get("data").expect("fixture has data field").clone();
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("q", "github"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["--json", "search", "github"])
        .output()
        .expect("run actionbook --json search");

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
async fn search_empty_results_prints_no_results() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("q", "zzznope"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": []
        })))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["search", "zzznope"])
        .output()
        .expect("run actionbook search");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success even for empty results\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("No results found."), "stdout:\n{stdout}");
}

#[tokio::test]
async fn search_server_400_renders_error() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("q", "anything"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "success": false,
            "error": {
                "code": "BAD_REQUEST",
                "message": "Missing required parameter: q"
            }
        })))
        .mount(&mock)
        .await;

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_API_URL", mock.uri())
        .args(["search", "anything"])
        .output()
        .expect("run actionbook search");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected non-zero exit on 400\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("Missing required parameter: q"),
        "stderr should surface server error message\nstderr:\n{stderr}"
    );
}
