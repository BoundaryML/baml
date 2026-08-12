//! End-to-end tests for `baml feedback` + `baml auth login` against an
//! in-process mock of `WorkOS` CLI Auth (device flow) and `PostHog` ingestion.
//!
//! The mock records every `PostHog` capture body so tests can assert on the
//! actual events: anonymous continuity (one distinct id across reports),
//! the `$identify` merge on login, email attribution afterwards, and the
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
    /// How many times `/user_management/authenticate` has been polled.
    auth_polls: Mutex<u32>,
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
                let (status, response) = respond(&state, &base, &path, &body);
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

fn respond(state: &MockState, base: &str, path: &str, body: &str) -> (&'static str, String) {
    match path {
        "/capture/" => {
            let parsed: Value = serde_json::from_str(body).expect("capture body is JSON");
            state.captures.lock().unwrap().push(parsed);
            ("200 OK", r#"{"status":1}"#.into())
        }
        "/user_management/authorize/device" => (
            "200 OK",
            format!(
                r#"{{"device_code":"dc_1","user_code":"RRGQ-BJVS","verification_uri":"{base}/device","interval":1}}"#
            ),
        ),
        "/user_management/authenticate" => {
            let mut polls = state.auth_polls.lock().unwrap();
            *polls += 1;
            if body.contains("refresh_token") || *polls >= 2 {
                (
                    "200 OK",
                    r#"{"access_token":"at_1","refresh_token":"rt_1","expires_in":3600,"user":{"id":"user_wos_1","email":"user@example.com"}}"#.into(),
                )
            } else {
                (
                    "400 Bad Request",
                    r#"{"error":"authorization_pending"}"#.into(),
                )
            }
        }
        _ => ("404 Not Found", "{}".into()),
    }
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
/// a test can simulate `PostHog` being unreachable while `WorkOS` still
/// works.
fn run_baml_posthog(
    home: &std::path::Path,
    workos: &str,
    posthog: &str,
    args: &[&str],
    stdin: Option<&str>,
) -> (bool, String) {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    cmd.args(args)
        .env("BAML_HOME", home)
        .env("BAML_WORKOS_API_DOMAIN", workos)
        .env("BAML_WORKOS_CLIENT_ID", "client_test")
        .env("BAML_POSTHOG_HOST", posthog)
        // Keep ordinary invocation telemetry quiet so /capture/ sees only
        // feedback + identify events.
        .env("BAML_TELEMETRY_DISABLED", "1")
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
fn anonymous_by_default_then_login_backfills() {
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

    // 3. whoami shows the anonymous state.
    let (_, out) = run_baml(home.path(), &base, &["auth", "whoami"], None);
    assert!(out.contains("anonymous (feedback id"), "{out}");

    // 4. `baml auth login`: device flow (one pending poll), then $identify
    //    merges the anonymous person into the identified one.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "login", "--no-open"], None);
    assert!(ok, "{out}");
    assert!(out.contains("logged in as user@example.com"), "{out}");
    let identify = state
        .captures
        .lock()
        .unwrap()
        .iter()
        .find(|c| c["event"] == "$identify")
        .cloned()
        .expect("an $identify event must be sent on login");
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

    // 5. Feedback while logged in: event carries the email and still the
    //    same distinct id (person continuity).
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--title", "Issue (cli): third report"],
        None,
    );
    assert!(ok, "{out}");
    assert!(out.contains("sent as user@example.com"), "{out}");
    let events = feedback_events(&state);
    assert_eq!(events.len(), 3, "{events:?}");
    assert_eq!(events[2]["distinct_id"], anon_id.as_str(), "{events:?}");
    assert_eq!(
        events[2]["properties"]["email"], "user@example.com",
        "{events:?}"
    );

    // 6. Logout keeps the distinct id; whoami returns to anonymous.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "logout"], None);
    assert!(ok, "{out}");
    let json = std::fs::read_to_string(&creds).unwrap();
    assert!(
        json.contains(&anon_id),
        "distinct id must survive logout: {json}"
    );
    assert!(!json.contains("access_token"), "{json}");
    let (_, out) = run_baml(home.path(), &base, &["auth", "whoami"], None);
    assert!(out.contains("anonymous"), "{out}");

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
