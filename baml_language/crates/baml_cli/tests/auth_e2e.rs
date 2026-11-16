//! End-to-end tests for `baml cloud auth` against an in-process mock of the
//! Boundary login gateway.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

use serde_json::Value;

#[derive(Default)]
struct MockState {
    /// How many times device approval has been polled.
    auth_polls: Mutex<u32>,
    reject_logout: Mutex<bool>,
    refreshes: Mutex<u32>,
    refreshed_email: Mutex<Option<String>>,
    reject_refresh: Mutex<bool>,
    verified_keys: Mutex<Vec<String>>,
}

fn spawn_mock(state: Arc<MockState>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let base_for_thread = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // One thread per connection so an assert! panic inside respond
            // fails that request visibly instead of killing the accept loop.
            let state = state.clone();
            let base = base_for_thread.clone();
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                // Read until the full content-length body has arrived.
                loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    if let Some(header_end) = text.find("\r\n\r\n") {
                        let content_length = text
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= header_end + 4 + content_length {
                            break;
                        }
                    }
                }
                let req = String::from_utf8_lossy(&buf).into_owned();
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("")
                    .to_string();
                let body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                let (status, response) = respond(&state, &base, &path, &body, &req);
                // This thread closes the socket after one response, so say so.
                // Otherwise the client pools the connection and may send its
                // next request (e.g. refresh right after API-key verification)
                // before seeing the close, failing it with a connection reset.
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{response}",
                        response.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    base
}

fn respond(
    state: &MockState,
    base: &str,
    path: &str,
    body: &str,
    request: &str,
) -> (&'static str, String) {
    match path {
        "/v1/auth/device" => (
            "200 OK",
            format!(
                r#"{{"deviceCode":"dc_1","userCode":"RRGQ-BJVS","verificationUri":"{base}/device","expiresAt":{},"intervalSeconds":1}}"#,
                now() + 300,
            ),
        ),
        "/v1/auth/device/poll" => {
            let mut polls = state.auth_polls.lock().unwrap();
            *polls += 1;
            assert_eq!(
                serde_json::from_str::<Value>(body).unwrap()["deviceCode"],
                "dc_1"
            );
            if *polls >= 2 {
                (
                    "200 OK",
                    serde_json::json!({"status":"approved", "session":session()}).to_string(),
                )
            } else {
                (
                    "200 OK",
                    r#"{"status":"pending","intervalSeconds":1}"#.into(),
                )
            }
        }
        "/v1/auth/verify" => {
            let key = request
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_owned())
                })
                .unwrap_or_default();
            state.verified_keys.lock().unwrap().push(key.clone());
            assert!(body.is_empty(), "auth status uses default verification");
            if key == "Bearer bdry_secret_status_test" {
                ("204 No Content", String::new())
            } else {
                (
                    "401 Unauthorized",
                    r#"{"code":"UNAUTHORIZED","retryable":false}"#.into(),
                )
            }
        }
        "/v1/auth/refresh" => {
            *state.refreshes.lock().unwrap() += 1;
            if *state.reject_refresh.lock().unwrap() {
                return (
                    "401 Unauthorized",
                    r#"{"code":"UNAUTHORIZED","retryable":false}"#.into(),
                );
            }
            assert_eq!(
                serde_json::from_str::<Value>(body).unwrap()["refreshToken"],
                "bdry_session_test"
            );
            let mut session = session();
            if let Some(email) = state.refreshed_email.lock().unwrap().as_ref() {
                session["caller"]["email"] = Value::String(email.clone());
            }
            ("200 OK", session.to_string())
        }
        "/v1/auth/logout" => {
            assert_eq!(
                serde_json::from_str::<Value>(body).unwrap()["refreshToken"],
                "bdry_session_test"
            );
            if *state.reject_logout.lock().unwrap() {
                return ("500 Internal Server Error", "{}".into());
            }
            ("200 OK", "{}".into())
        }
        _ => ("404 Not Found", "{}".into()),
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn session() -> Value {
    serde_json::json!({"accessToken":"bdry_access_test", "refreshToken":"bdry_session_test",
        "expiresAt":now()+300, "caller":{"userId":"user_wos_1","email":"user@example.com"}})
}

fn run_baml(home: &std::path::Path, base: &str, args: &[&str]) -> (bool, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(["--agent-skill-check", "off"])
        .args(args)
        .current_dir(home)
        .env_remove("BOUNDARY_API_KEY")
        .env_remove("BOUNDARY_PROJECT")
        .env("BAML_HOME", home)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BOUNDARY_API_URL", base)
        .env("DO_NOT_TRACK", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("failed to run baml-cli");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.success(), format!("{stdout}{stderr}"))
}

#[test]
fn review_logout_clears_local_login_when_server_revocation_fails() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let (ok, out) = run_baml(home.path(), &base, &["cloud", "auth", "login", "--no-open"]);
    assert!(ok, "{out}");

    *state.reject_logout.lock().unwrap() = true;
    let (ok, out) = run_baml(home.path(), &base, &["cloud", "auth", "logout"]);
    assert!(ok, "{out}");
    assert_eq!(
        out.trim(),
        r#"Logout local login removed
warning: Boundary could not confirm server-side logout.
The saved credential was removed from this machine, but the server session may still be active.

Boundary request failed with HTTP 500 Internal Server Error"#
    );
    let (ok, out) = run_baml(home.path(), &base, &["cloud", "auth", "token"]);
    assert!(!ok, "a revoked local login must not be reusable: {out}");
    assert_eq!(
        out.trim(),
        r#"error: not logged in; run `baml cloud auth login`"#
    );
}

fn status_command(
    home: &std::path::Path,
    directory: &std::path::Path,
    base: &str,
    env: &[(&str, &str)],
) -> std::process::Output {
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(["--agent-skill-check", "off", "cloud", "auth", "status"])
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .current_dir(directory)
        .env("BAML_HOME", home)
        .env("BOUNDARY_API_URL", base)
        .env("DO_NOT_TRACK", "1")
        .env_remove("BOUNDARY_API_KEY")
        .env_remove("BOUNDARY_PROJECT")
        .envs(env.iter().copied())
        .output()
        .unwrap()
}

#[test]
fn status_verifies_both_credentials_and_shows_the_project_override() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let (ok, output) = run_baml(home.path(), &base, &["cloud", "auth", "login", "--no-open"]);
    assert!(ok, "{output}");
    *state.refreshed_email.lock().unwrap() = Some("verified@example.com".into());
    let output = status_command(
        home.path(),
        home.path(),
        &base,
        &[
            ("BOUNDARY_API_KEY", "bdry_secret_status_test"),
            ("BOUNDARY_PROJECT", "acme/app"),
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            r#"User: verified@example.com (verified)
Credential overridden by: BOUNDARY_API_KEY (verified)
Project: acme/app (BOUNDARY_PROJECT)
Endpoint: {base}
"#
        ),
        "auth status stderr: {}; refresh requests: {}",
        String::from_utf8_lossy(&output.stderr),
        *state.refreshes.lock().unwrap()
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(*state.refreshes.lock().unwrap(), 1);
    assert_eq!(
        *state.verified_keys.lock().unwrap(),
        ["Bearer bdry_secret_status_test"]
    );
    let output = status_command(home.path(), home.path(), &base, &[]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            r#"User: verified@example.com (verified)
Project: Not selected (outside a BAML project)
Endpoint: {base}
"#
        )
    );
    assert_eq!(*state.refreshes.lock().unwrap(), 2);
    let output = status_command(
        home.path(),
        home.path(),
        &base,
        &[("BOUNDARY_API_KEY", "invalid-status-key")],
    );
    assert!(!output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            r#"error: Boundary rejected BOUNDARY_API_KEY (401).
  Endpoint: {base}
  Code: UNAUTHORIZED

  To continue, choose one:
    • Replace BOUNDARY_API_KEY with a valid API key.
    • Unset BOUNDARY_API_KEY to use your saved Boundary login.

  Authentication command failed.
"#
        )
    );
}

#[test]
fn status_reports_api_key_only_and_manifest_project_and_rejects_bad_keys() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("baml_src")).unwrap();
    std::fs::write(
        project.path().join("baml.toml"),
        "[boundary]\nproject = \"acme/app\"\n",
    )
    .unwrap();
    let output = status_command(
        home.path(),
        project.path(),
        &base,
        &[("BOUNDARY_API_KEY", "bdry_secret_status_test")],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            r#"User: Not logged in
Credential: BOUNDARY_API_KEY (verified)
Project: acme/app (baml.toml)
Endpoint: {base}
"#
        )
    );
    assert_eq!(*state.refreshes.lock().unwrap(), 0);
    let output = status_command(
        home.path(),
        project.path(),
        &base,
        &[("BOUNDARY_API_KEY", "invalid-status-key")],
    );
    assert!(!output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            r#"error: Boundary rejected BOUNDARY_API_KEY (401).
  Endpoint: {base}
  Code: UNAUTHORIZED

  To continue, choose one:
    • Replace BOUNDARY_API_KEY with a valid API key.
    • Unset BOUNDARY_API_KEY, then run `baml cloud auth login`.

  Authentication command failed.
"#
        )
    );
}

#[test]
fn status_rejects_revoked_login_and_whoami_is_removed() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let (ok, output) = run_baml(home.path(), &base, &["cloud", "auth", "login", "--no-open"]);
    assert!(ok, "{output}");
    *state.reject_refresh.lock().unwrap() = true;
    let output = status_command(home.path(), home.path(), &base, &[]);
    assert!(!output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            r#"error: Boundary rejected your saved login (401).
  Endpoint: {base}
  Code: UNAUTHORIZED

  To continue, choose one:
    • Run `baml cloud auth login` again and approve the new code in your browser.

  Authentication command failed.
"#
        )
    );
    let (ok, _) = run_baml(home.path(), &base, &["cloud", "auth", "whoami"]);
    assert!(!ok);
}
