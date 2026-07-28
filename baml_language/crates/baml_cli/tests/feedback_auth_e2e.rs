//! End-to-end tests for `baml feedback` + `baml login` against an
//! in-process mock of `WorkOS` CLI Auth (device flow) and `PostHog` ingestion.
//!
//! The mock records every `PostHog` capture body so tests can assert on the
//! actual events: anonymous continuity (one distinct id across reports),
//! the `$identify` merge on login, and email attribution afterwards.

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
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    cmd.args(args)
        .env("BAML_HOME", home)
        .env("BAML_WORKOS_API_DOMAIN", base)
        .env("BAML_WORKOS_CLIENT_ID", "client_test")
        .env("BAML_POSTHOG_HOST", base)
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

#[test]
fn anonymous_feedback_then_login_backfills() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let creds = home.path().join("creds.json");

    // 1. Anonymous feedback via flag: no session needed, event has no email.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &[
            "feedback",
            "--anonymous",
            "--issue",
            "parser panics on nested unions",
        ],
        None,
    );
    assert!(ok, "{out}");
    assert!(out.contains("Feedback sent anonymously"), "{out}");
    let json = std::fs::read_to_string(&creds).unwrap();
    assert!(json.contains("posthog_distinct_id"), "{json}");

    // 2. Second report reuses the same distinct id (one anonymous person).
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--anonymous", "--issue", "map keys mis-typed"],
        None,
    );
    assert!(ok, "{out}");
    let events = feedback_events(&state);
    assert_eq!(events.len(), 2, "{events:?}");
    let anon_id = events[0]["distinct_id"].as_str().unwrap().to_string();
    assert_eq!(events[1]["distinct_id"], anon_id.as_str(), "{events:?}");
    assert!(events[0]["properties"]["email"].is_null(), "{events:?}");
    assert_eq!(
        events[0]["properties"]["issue"], "parser panics on nested unions",
        "{events:?}"
    );

    // 3. whoami shows the anonymous state.
    let (_, out) = run_baml(home.path(), &base, &["auth", "whoami"], None);
    assert!(out.contains("Anonymous (feedback id"), "{out}");

    // 4. Login: device flow (one pending poll), then $identify merges the
    //    anonymous person into the identified one.
    let (ok, out) = run_baml(home.path(), &base, &["login", "--no-open"], None);
    assert!(ok, "{out}");
    assert!(out.contains("Logged in as user@example.com"), "{out}");
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

    // 5. Feedback while logged in: no prompt, event carries the email and
    //    still the same distinct id (person continuity).
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--issue", "third report"],
        None,
    );
    assert!(ok, "{out}");
    assert!(out.contains("Feedback sent as user@example.com"), "{out}");
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
    assert!(out.contains("Anonymous"), "{out}");
}

#[test]
fn interactive_prompt_anonymous_choice() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    // No mode flag: the prompt appears; choosing 1 reports anonymously.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--issue", "prompted report"],
        Some("1\n"),
    );
    assert!(ok, "{out}");
    assert!(out.contains("How should I report it?"), "{out}");
    assert!(out.contains("Feedback sent anonymously"), "{out}");
    assert_eq!(feedback_events(&state).len(), 1);

    // The anonymous choice sticks: the next report skips the prompt and
    // sends anonymously without any stdin.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--issue", "second report"],
        None,
    );
    assert!(ok, "{out}");
    assert!(!out.contains("How should I report it?"), "{out}");
    assert!(out.contains("Feedback sent anonymously"), "{out}");
    assert_eq!(feedback_events(&state).len(), 2);
}

#[test]
fn interactive_prompt_decline_sends_nothing() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    // The prompt is preceded by a preview of exactly what would be sent.
    // Choosing 3 declines: nothing sent, and the decline is remembered.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--issue", "never mind", "--repro", "class A {}"],
        Some("3\n"),
    );
    assert!(ok, "{out}");
    assert!(out.contains("Here's what will be sent:"), "{out}");
    assert!(out.contains("Issue: never mind"), "{out}");
    assert!(out.contains("Repro: class A {}"), "{out}");
    assert!(out.contains("Nothing sent"), "{out}");
    assert_eq!(feedback_events(&state).len(), 0);
    let creds = std::fs::read_to_string(home.path().join("creds.json")).unwrap();
    assert!(creds.contains("feedback_declined_at"), "{creds}");

    // Within the day-long cooldown: no prompt, nothing sent, still exit 0.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--issue", "still nothing"],
        None,
    );
    assert!(ok, "{out}");
    assert!(!out.contains("How should I report it?"), "{out}");
    assert!(out.contains("declined recently"), "{out}");
    assert_eq!(feedback_events(&state).len(), 0);

    // Explicit --anonymous overrides the cooldown and clears it.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--anonymous", "--issue", "ok fine"],
        None,
    );
    assert!(ok, "{out}");
    assert_eq!(feedback_events(&state).len(), 1);
    let creds = std::fs::read_to_string(home.path().join("creds.json")).unwrap();
    assert!(!creds.contains("feedback_declined_at"), "{creds}");

    // An expired cooldown prompts again: write stale state directly (a
    // 1970 decline, no sticky-anonymous flag) and expect the prompt.
    std::fs::write(
        home.path().join("creds.json"),
        r#"{"feedback_declined_at":1}"#,
    )
    .unwrap();
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--issue", "after cooldown"],
        Some("3\n"),
    );
    assert!(ok, "{out}");
    assert!(
        out.contains("How should I report it?"),
        "prompts again after expiry: {out}"
    );
}

#[test]
fn email_flag_without_login_gives_guidance() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--email", "--issue", "x"],
        None,
    );
    assert!(!ok, "must exit non-zero: {out}");
    assert!(out.contains("Run `baml login`"), "{out}");
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
        &["feedback", "--anonymous", "-"],
        Some(r#"{"issue": "from stdin", "repro": "class A {}"}"#),
    );
    assert!(ok, "{out}");
    let events = feedback_events(&state);
    assert_eq!(events[0]["properties"]["issue"], "from stdin");
    assert_eq!(events[0]["properties"]["repro"], "class A {}");
}

#[test]
fn anonymous_after_login_uses_one_shot_id() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    // Establish an identified session with a merged distinct id.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--anonymous", "--issue", "first"],
        None,
    );
    assert!(ok, "{out}");
    let anon_id = feedback_events(&state)[0]["distinct_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (ok, out) = run_baml(home.path(), &base, &["login", "--no-open"], None);
    assert!(ok, "{out}");

    // --anonymous after login must NOT use the merged persistent id (which
    // would attribute the report to the person anyway) and must not carry
    // the email.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--anonymous", "--issue", "truly anonymous"],
        None,
    );
    assert!(ok, "{out}");
    let events = feedback_events(&state);
    let last = events.last().unwrap();
    assert_ne!(last["distinct_id"], anon_id.as_str(), "{last}");
    assert!(last["properties"]["email"].is_null(), "{last}");

    // The persistent id is untouched for the identified path.
    let creds = std::fs::read_to_string(home.path().join("creds.json")).unwrap();
    assert!(creds.contains(&anon_id), "{creds}");
}

#[test]
fn unknown_payload_fields_are_rejected() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();

    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["feedback", "--anonymous", "-"],
        Some(r#"{"issue": "x", "$set": {"email": "spoof@example.com"}}"#),
    );
    assert!(!ok, "must reject unknown fields: {out}");
    assert!(out.contains("Unknown feedback field"), "{out}");
    assert_eq!(feedback_events(&state).len(), 0, "nothing may be sent");
}
