//! End-to-end tests for `baml feedback` + `baml auth login` against an
//! in-process mock of the Boundary login gateway and `PostHog` ingestion.
//!
//! The mock records every `PostHog` capture body so tests can assert on the
//! actual events: anonymous continuity (one distinct id across reports),
//! the `$identify` merge on identified feedback, email attribution, and the
//! open-until-synced lifecycle of reports sent while offline.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

use serde_json::Value;

#[derive(Default)]
struct MockState {
    /// Bodies received on `/capture/`, in order.
    captures: Mutex<Vec<Value>>,
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
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{response}",
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
        "/capture/" => {
            let parsed: Value = serde_json::from_str(body).expect("capture body is JSON");
            state.captures.lock().unwrap().push(parsed);
            ("200 OK", r#"{"status":1}"#.into())
        }
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

fn run_baml(
    home: &std::path::Path,
    base: &str,
    args: &[&str],
    stdin: Option<&str>,
) -> (bool, String) {
    run_baml_posthog(home, base, base, args, stdin)
}

/// Like [`run_baml`], but with a separately controllable `PostHog` host so
/// a test can simulate `PostHog` being unreachable while Boundary login still
/// works.
fn run_baml_posthog(
    home: &std::path::Path,
    boundary: &str,
    posthog: &str,
    args: &[&str],
    stdin: Option<&str>,
) -> (bool, String) {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    cmd.args(["--agent-skill-check", "off"])
        .args(args)
        .current_dir(home)
        .env_remove("BOUNDARY_API_KEY")
        .env_remove("BOUNDARY_PROJECT")
        .env("BAML_HOME", home)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BOUNDARY_API_URL", boundary)
        .env("BAML_FEEDBACK_HOST", posthog)
        // Keep ordinary invocation telemetry quiet so /capture/ sees only
        // feedback + identify events.
        .env("DO_NOT_TRACK", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    } else {
        cmd.stdin(std::process::Stdio::null());
    }
    let mut child = cmd.spawn().expect("failed to run baml-cli");
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().expect("failed to wait");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.success(), format!("{stdout}{stderr}"))
}

fn feedback_events(state: &MockState) -> Vec<Value> {
    state
        .captures
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c["event"] == "baml_feedback")
        .cloned()
        .collect()
}

#[test]
fn review_logout_clears_local_login_when_server_revocation_fails() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let (ok, out) = run_baml(home.path(), &base, &["auth", "login", "--no-open"], None);
    assert!(ok, "{out}");
    let anonymous = std::fs::read_to_string(home.path().join("creds.json")).unwrap();

    *state.reject_logout.lock().unwrap() = true;
    let (ok, out) = run_baml(home.path(), &base, &["auth", "logout"], None);
    assert!(ok, "{out}");
    assert_eq!(
        out.trim(),
        r#"Logout local login removed
warning: Boundary could not confirm server-side logout.
The saved credential was removed from this machine, but the server session may still be active.

Boundary request failed with HTTP 500 Internal Server Error"#
    );
    let (ok, out) = run_baml(home.path(), &base, &["auth", "token"], None);
    assert!(!ok, "a revoked local login must not be reusable: {out}");
    assert_eq!(out.trim(), r#"error: not logged in; run `baml auth login`"#);
    assert_eq!(
        std::fs::read_to_string(home.path().join("creds.json")).unwrap(),
        anonymous
    );
}

/// A dead endpoint: a listener that accepts and immediately closes every
/// connection, so requests fail fast and deterministically. (Bind-then-drop
/// would free the port for reuse by a concurrent test.)
fn dead_posthog() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            drop(stream);
        }
    });
    format!("http://{addr}")
}

#[test]
fn anonymous_by_default_then_identified_feedback_backfills() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let creds = home.path().join("creds.json");

    // 1. No flags, no prompt: sends anonymously from the get-go, with a
    //    preview of what goes out and the login hint.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &[
            "feedback",
            "--title",
            "Issue (parser): panics on nested unions",
        ],
        None,
    );
    assert!(ok, "{out}");
    assert!(out.contains("reporting to Boundary anonymously:"), "{out}");
    assert!(!out.contains("How should I report"), "no prompt: {out}");
    assert!(out.contains("sent anonymously"), "{out}");
    assert!(out.contains("run `baml auth login`"), "{out}");
    let json = std::fs::read_to_string(&creds).unwrap();
    assert!(json.contains("posthog_distinct_id"), "{json}");

    // 2. Second report reuses the same distinct id (one anonymous person)
    //    and carries a report_id for later server-side joining.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &[
            "feedback",
            "--title",
            "Issue (types): map keys mis-typed",
            "--description",
            "Minimum repro: map<int,int>",
        ],
        None,
    );
    assert!(ok, "{out}");
    let events = feedback_events(&state);
    assert_eq!(events.len(), 2, "{events:?}");
    let anon_id = events[0]["distinct_id"].as_str().unwrap().to_string();
    assert_eq!(events[1]["distinct_id"], anon_id.as_str(), "{events:?}");
    assert!(events[0]["properties"]["email"].is_null(), "{events:?}");
    assert_eq!(
        events[0]["properties"]["title"], "Issue (parser): panics on nested unions",
        "{events:?}"
    );
    assert_eq!(
        events[1]["properties"]["description"], "Minimum repro: map<int,int>",
        "{events:?}"
    );
    assert!(
        events[1]["properties"]["report_id"].is_string(),
        "{events:?}"
    );

    // 3. Status reports no login.
    let (_, out) = run_baml(home.path(), &base, &["auth", "status"], None);
    assert_eq!(
        out,
        format!(
            r#"User: Not logged in
Project: Not selected (outside a BAML project)
Endpoint: {base}
"#
        )
    );

    // 4. Login saves the protected session; it does not identify feedback.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "login", "--no-open"], None);
    assert!(ok, "{out}");
    assert!(out.contains("logged in as user@example.com"), "{out}");
    let mut login_files: Vec<_> = std::fs::read_dir(home.path().join("login").join("cache"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    let login_file = login_files
        .iter()
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .expect("login must persist a JSON credential file")
        .clone();
    let mut expected_files = vec![login_file.clone()];
    #[cfg(windows)]
    expected_files.push(login_file.with_extension("lock"));
    login_files.sort();
    expected_files.sort();
    assert_eq!(login_files, expected_files);
    let login: Value = serde_json::from_slice(&std::fs::read(&login_file).unwrap()).unwrap();
    assert_eq!(
        login,
        serde_json::json!({
            "refreshToken": session()["refreshToken"],
            "caller": session()["caller"],
        })
    );
    assert!(
        !state
            .captures
            .lock()
            .unwrap()
            .iter()
            .any(|c| c["event"] == "$identify")
    );
    let json = std::fs::read_to_string(&creds).unwrap();
    assert!(
        !json.contains("bdry_session_") && !json.contains("user@example.com"),
        "{json}"
    );

    // 5. Identified feedback associates the earlier anonymous reports.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--title", "Issue (cli): third report"],
        None,
    );
    assert!(ok, "{out}");
    assert!(out.contains("sent as user@example.com"), "{out}");
    let identify = state
        .captures
        .lock()
        .unwrap()
        .iter()
        .find(|c| c["event"] == "$identify")
        .cloned()
        .expect("identified feedback must send an $identify event");
    assert_eq!(identify["distinct_id"], "user_wos_1", "{identify}");
    assert_eq!(
        identify["properties"]["$anon_distinct_id"],
        anon_id.as_str(),
        "{identify}"
    );
    assert_eq!(
        identify["properties"]["$set"]["email"], "user@example.com",
        "{identify}"
    );

    let events = feedback_events(&state);
    assert_eq!(events.len(), 3, "{events:?}");
    assert_eq!(events[2]["distinct_id"], anon_id.as_str(), "{events:?}");
    assert_eq!(
        events[2]["properties"]["email"], "user@example.com",
        "{events:?}"
    );

    // 6. Logout keeps the distinct id; status reports no login.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "logout"], None);
    assert!(ok, "{out}");
    assert!(!login_file.exists());
    let json = std::fs::read_to_string(&creds).unwrap();
    assert!(
        json.contains(&anon_id),
        "distinct id must survive logout: {json}"
    );
    assert!(!json.contains("access_token"), "{json}");
    let (_, out) = run_baml(home.path(), &base, &["auth", "status"], None);
    assert_eq!(
        out,
        format!(
            r#"User: Not logged in
Project: Not selected (outside a BAML project)
Endpoint: {base}
"#
        )
    );

    // 7. `baml login` no longer exists at the top level.
    let (ok, out) = run_baml(home.path(), &base, &["login"], None);
    assert!(!ok, "top-level login must be gone: {out}");
}

#[test]
fn email_flag_without_login_gives_guidance() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--email", "--title", "x"],
        None,
    );
    assert!(!ok, "must exit non-zero: {out}");
    assert!(out.contains("run `baml auth login`"), "{out}");
    assert_eq!(feedback_events(&state).len(), 0, "nothing may be sent");
}

#[test]
fn json_stdin_payload() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "-"],
        Some(r#"{"title": "Issue (stdin): piped", "description": "from a pipe"}"#),
    );
    assert!(ok, "{out}");
    let events = feedback_events(&state);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        events[0]["properties"]["title"], "Issue (stdin): piped",
        "{events:?}"
    );
    assert_eq!(
        events[0]["properties"]["description"], "from a pipe",
        "{events:?}"
    );
}

#[test]
fn anonymous_after_login_uses_one_shot_id() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    // Establish an anonymous id, then log in (merging it).
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--title", "Issue (x): first"],
        None,
    );
    assert!(ok, "{out}");
    let (ok, out) = run_baml(home.path(), &base, &["auth", "login", "--no-open"], None);
    assert!(ok, "{out}");

    // --anonymous while logged in: a fresh, unpersisted id.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--anonymous", "--title", "Issue (x): hush"],
        None,
    );
    assert!(ok, "{out}");
    let events = feedback_events(&state);
    assert_eq!(events.len(), 2, "{events:?}");
    let stored_id = events[0]["distinct_id"].as_str().unwrap();
    let one_shot = events[1]["distinct_id"].as_str().unwrap();
    assert_ne!(stored_id, one_shot, "{events:?}");
    assert!(events[1]["properties"]["email"].is_null(), "{events:?}");
    let (ok, out) = run_baml(home.path(), &base, &["auth", "logout"], None);
    assert!(ok, "{out}");
}

#[test]
fn unknown_payload_fields_are_rejected() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    for payload in [
        r#"{"title": "x", "$set": {"email": "spoof@example.com"}}"#,
        r#"{"title": "x", "issue": "old flag name"}"#,
    ] {
        let (ok, out) = run_baml(home.path(), &base, &["feedback", "-"], Some(payload));
        assert!(!ok, "{out}");
        assert!(out.contains("unknown feedback field"), "{out}");
    }
    assert_eq!(feedback_events(&state).len(), 0, "nothing may be sent");
}

#[test]
fn offline_send_saves_open_then_syncs() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let dead = dead_posthog();

    // PostHog unreachable: the send still exits 0 and records the report.
    let (ok, out) = run_baml_posthog(
        home.path(),
        &base,
        &dead,
        &["feedback", "--title", "Issue (net): sent offline"],
        None,
    );
    assert!(ok, "an offline send must not fail: {out}");
    assert!(out.contains("saved locally"), "{out}");
    let store = std::fs::read_to_string(home.path().join("feedback.json")).unwrap();
    assert!(store.contains("\"open\""), "{store}");
    assert_eq!(feedback_events(&state).len(), 0);

    // Any later invocation against a reachable PostHog syncs it.
    let (ok, out) = run_baml(home.path(), &base, &["feedback", "list"], None);
    assert!(ok, "{out}");
    assert!(out.contains("anonymous"), "synced status: {out}");
    let events = feedback_events(&state);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        events[0]["properties"]["title"], "Issue (net): sent offline",
        "{events:?}"
    );
    let store = std::fs::read_to_string(home.path().join("feedback.json")).unwrap();
    assert!(!store.contains("\"open\""), "{store}");
}

#[test]
fn sync_honors_forced_anonymity_after_login() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let dead = dead_posthog();

    // Establish the persistent distinct id online, then log in.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--title", "Issue (x): baseline"],
        None,
    );
    assert!(ok, "{out}");
    let (ok, out) = run_baml(home.path(), &base, &["auth", "login", "--no-open"], None);
    assert!(ok, "{out}");

    // An explicitly anonymous report filed while PostHog is unreachable...
    let (ok, out) = run_baml_posthog(
        home.path(),
        &base,
        &dead,
        &["feedback", "--anonymous", "--title", "Issue (x): hush"],
        None,
    );
    assert!(ok, "{out}");

    // ...must stay anonymous when a later run syncs it, despite the login:
    // no email property, and a distinct id different from the merged one.
    let (ok, out) = run_baml(home.path(), &base, &["feedback", "status"], None);
    assert!(ok, "{out}");
    let events = feedback_events(&state);
    assert_eq!(events.len(), 2, "{events:?}");
    let baseline_id = events[0]["distinct_id"].as_str().unwrap();
    let synced = &events[1];
    assert_eq!(synced["properties"]["title"], "Issue (x): hush", "{synced}");
    assert!(synced["properties"]["email"].is_null(), "{synced}");
    assert_ne!(synced["distinct_id"].as_str().unwrap(), baseline_id);
    let store = std::fs::read_to_string(home.path().join("feedback.json")).unwrap();
    assert!(store.contains("\"anonymous\""), "{store}");
    assert!(!store.contains("\"open\""), "{store}");
    let (ok, out) = run_baml(home.path(), &base, &["auth", "logout"], None);
    assert!(ok, "{out}");
}

#[test]
fn attached_files_ship_and_survive_offline_sync() {
    use base64::Engine as _;

    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let dead = dead_posthog();
    let attachment = home.path().join("repro.baml");
    std::fs::write(&attachment, "class A { x int }").unwrap();

    // Online send with --files: the event carries the encoded attachment.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &[
            "feedback",
            "--title",
            "Issue (types): with repro file",
            "--files",
            attachment.to_str().unwrap(),
        ],
        None,
    );
    assert!(ok, "{out}");
    assert!(out.contains("Files: repro.baml (17 bytes)"), "{out}");
    let events = feedback_events(&state);
    let files = events[0]["properties"]["files"].as_array().unwrap();
    assert_eq!(files[0]["name"], "repro.baml");
    assert_eq!(files[0]["mime"], "text/plain");
    assert!(!files[0]["content_base64"].as_str().unwrap().is_empty());
    // Delivered: the local record keeps metadata, not content.
    let store = std::fs::read_to_string(home.path().join("feedback.json")).unwrap();
    assert!(store.contains("repro.baml"), "{store}");
    assert!(!store.contains("content_base64"), "{store}");

    // Offline send with a file: content is retained in the open record...
    let (ok, out) = run_baml_posthog(
        home.path(),
        &base,
        &dead,
        &[
            "feedback",
            "--title",
            "Issue (types): offline with file",
            "--files",
            attachment.to_str().unwrap(),
        ],
        None,
    );
    assert!(ok, "{out}");
    let store = std::fs::read_to_string(home.path().join("feedback.json")).unwrap();
    assert!(store.contains("content_base64"), "{store}");

    // ...and the deferred delivery ships it intact.
    let (ok, out) = run_baml(home.path(), &base, &["feedback", "status"], None);
    assert!(ok, "{out}");
    let events = feedback_events(&state);
    assert_eq!(events.len(), 2, "{events:?}");
    let synced_files = events[1]["properties"]["files"].as_array().unwrap();
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(synced_files[0]["content_base64"].as_str().unwrap())
            .unwrap(),
        b"class A { x int }"
    );
    let store = std::fs::read_to_string(home.path().join("feedback.json")).unwrap();
    assert!(!store.contains("content_base64"), "{store}");
    assert!(!store.contains("\"open\""), "{store}");
}

#[test]
fn disable_blocks_sends_until_enable() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    let (ok, out) = run_baml(home.path(), &base, &["feedback", "disable"], None);
    assert!(ok, "{out}");
    assert!(out.contains("feedback disabled"), "{out}");

    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--title", "Issue (x): blocked"],
        None,
    );
    assert!(!ok, "disabled send must exit non-zero: {out}");
    assert!(out.contains("baml feedback enable"), "{out}");
    assert_eq!(feedback_events(&state).len(), 0, "nothing may be sent");

    let (ok, out) = run_baml(home.path(), &base, &["feedback", "enable"], None);
    assert!(ok, "{out}");
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--title", "Issue (x): unblocked"],
        None,
    );
    assert!(ok, "{out}");
    assert_eq!(feedback_events(&state).len(), 1);
}

#[test]
fn status_list_view_read_the_local_store() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state);
    let home = tempfile::tempdir().unwrap();

    let (ok, out) = run_baml(
        home.path(),
        &base,
        &[
            "feedback",
            "--title",
            "Issue (viewer): first",
            "--description",
            "Minimum repro: class A {}",
        ],
        None,
    );
    assert!(ok, "{out}");

    // status: enabled + the report with its delivery state.
    let (ok, out) = run_baml(home.path(), &base, &["feedback", "status"], None);
    assert!(ok, "{out}");
    assert!(out.contains("Status: Enabled"), "{out}");
    assert!(out.contains("[anonymous]"), "{out}");
    assert!(out.contains("Issue (viewer): first"), "{out}");

    // list --json: machine-readable records.
    let (ok, out) = run_baml(home.path(), &base, &["feedback", "list", "--json"], None);
    assert!(ok, "{out}");
    // run_baml appends stderr (the direct-binary-use warning) after stdout;
    // parse just the JSON array span.
    let json_span = &out[out.find('[').unwrap()..=out.rfind(']').unwrap()];
    let records: Value = serde_json::from_str(json_span).expect("list --json is JSON");
    let records = records.as_array().unwrap();
    assert_eq!(records.len(), 1, "{records:?}");
    let id = records[0]["id"].as_str().unwrap().to_string();
    assert_eq!(records[0]["status"], "anonymous", "{records:?}");

    // list --status filters.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "list", "--status", "open"],
        None,
    );
    assert!(ok, "{out}");
    assert!(out.contains("no matching reports"), "{out}");

    // view renders the full record; unknown ids error.
    let (ok, out) = run_baml(home.path(), &base, &["feedback", "view", &id], None);
    assert!(ok, "{out}");
    assert!(out.contains("Issue (viewer): first"), "{out}");
    assert!(out.contains("Minimum repro: class A {}"), "{out}");
    let (ok, out) = run_baml(home.path(), &base, &["feedback", "view", "zzzzzzzz"], None);
    assert!(!ok, "{out}");
    assert!(out.contains("no report with id"), "{out}");
}

fn status_command(
    home: &std::path::Path,
    directory: &std::path::Path,
    base: &str,
    env: &[(&str, &str)],
) -> std::process::Output {
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(["--agent-skill-check", "off", "auth", "status"])
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
    let (ok, output) = run_baml(home.path(), &base, &["auth", "login", "--no-open"], None);
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
    • Unset BOUNDARY_API_KEY, then run `baml auth login`.

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
    let (ok, output) = run_baml(home.path(), &base, &["auth", "login", "--no-open"], None);
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
    • Run `baml auth login` again and approve the new code in your browser.

  Authentication command failed.
"#
        )
    );
    let (ok, _) = run_baml(home.path(), &base, &["auth", "whoami"], None);
    assert!(!ok);
}

#[test]
fn invalid_boundary_url_rejects_anonymous_feedback() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let (ok, out) = run_baml_posthog(
        home.path(),
        "not a URL",
        &base,
        &["feedback", "--anonymous", "--title", "Invalid endpoint"],
        None,
    );
    assert!(!ok, "{out}");
    assert_eq!(
        out.trim(),
        r#"error: Invalid BOUNDARY_API_URL

caused by:
    0: relative URL without a base"#
    );
    assert_eq!(feedback_events(&state).len(), 0);
}
