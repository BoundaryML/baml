//! End-to-end test of the anonymous-first auth lifecycle against an
//! in-process mock speaking the real auth.md shapes served by `WorkOS`
//! `AuthKit` (nested `identity.assertion` / `claim.token`, claim ceremony via
//! attempt + user-code completion, jwt-bearer exchange).

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn b64url(s: &str) -> String {
    // Minimal base64url (no padding) for building mock JWTs.
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bytes = s.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(CHARS[(n >> 18) as usize & 63] as char);
        out.push(CHARS[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(CHARS[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(CHARS[n as usize & 63] as char);
        }
    }
    out
}

fn mock_jwt(sub: &str, email: Option<&str>) -> String {
    let header = b64url(r#"{"alg":"none"}"#);
    let payload = match email {
        Some(email) => b64url(&format!(r#"{{"sub":"{sub}","email":"{email}"}}"#)),
        None => b64url(&format!(r#"{{"sub":"{sub}"}}"#)),
    };
    format!("{header}.{payload}.sig")
}

#[derive(Default)]
struct MockState {
    claimed: AtomicBool,
}

fn spawn_mock(state: Arc<MockState>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let base_for_thread = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // One thread per connection: a panic from an assert! inside
            // `respond` then kills only that connection (the client sees a
            // reset and errors) instead of the whole accept loop, which
            // would mask the assertion message behind a hang.
            let state = state.clone();
            let base = base_for_thread.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 16384];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
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
    let anon_jwt = mock_jwt("agent_reg_test1", None);
    let claimed_jwt = mock_jwt("agent_reg_test1", Some("user@example.com"));
    match path {
        "/.well-known/oauth-authorization-server" => (
            "200 OK",
            format!(
                r#"{{"token_endpoint":"{base}/oauth2/token","agent_auth":{{"identity_endpoint":"{base}/agent/identity","claim_endpoint":"{base}/agent/identity/claim"}}}}"#
            ),
        ),
        "/agent/identity" => (
            "200 OK",
            format!(
                r#"{{"identity":{{"assertion":"{anon_jwt}","refresh_token":{{"value":"irt_1"}}}},"claim":{{"token":"ct_anon_1"}}}}"#
            ),
        ),
        "/agent/identity/claim/complete" => {
            assert!(body.contains("ct_anon_1"), "must send claim_token: {body}");
            if body.contains("WXYZ-7890") {
                state.claimed.store(true, Ordering::SeqCst);
                (
                    "200 OK",
                    format!(
                        r#"{{"identity":{{"assertion":"{claimed_jwt}","refresh_token":{{"value":"irt_2"}}}}}}"#
                    ),
                )
            } else {
                (
                    "400 Bad Request",
                    r#"{"code":"invalid_user_code","message":"wrong code"}"#.into(),
                )
            }
        }
        "/agent/identity/claim" => {
            assert!(body.contains("ct_anon_1"), "must send claim_token: {body}");
            (
                "200 OK",
                format!(r#"{{"attempt":{{"verification_uri":"{base}/claim-page"}}}}"#),
            )
        }
        "/oauth2/token" => {
            assert!(body.contains("jwt-bearer"), "unexpected grant: {body}");
            let (token, email_part) = if state.claimed.load(Ordering::SeqCst) {
                ("at_claimed", r#","user":{"email":"user@example.com"}"#)
            } else {
                ("at_anon", "")
            };
            (
                "200 OK",
                format!(r#"{{"access_token":"{token}","expires_in":300{email_part}}}"#),
            )
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
        .env("BAML_WORKOS_AUTHKIT_DOMAIN", base)
        .env("BAML_WORKOS_API_DOMAIN", base)
        .env("BAML_WORKOS_CLIENT_ID", "client_test")
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

#[test]
fn anonymous_first_lifecycle_with_claim_ceremony() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state);
    let home = tempfile::tempdir().unwrap();
    let creds = home.path().join("creds.json");

    // 1. Anonymous start: instant project (the registration id), no prompts.
    let (ok, out) = run_baml(home.path(), &base, &["login"], None);
    assert!(ok, "{out}");
    assert!(
        out.contains("Started anonymous project agent_reg_test1"),
        "{out}"
    );
    let json = std::fs::read_to_string(&creds).unwrap();
    assert!(json.contains(r#""status": "anonymous""#), "{json}");
    assert!(json.contains("ct_anon_1"), "{json}");

    // 2. Idempotent re-run.
    let (ok, out) = run_baml(home.path(), &base, &["login"], None);
    assert!(ok, "{out}");
    assert!(out.contains("Already started"), "{out}");

    // 3. whoami reports the anonymous project.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "whoami"], None);
    assert!(ok, "{out}");
    assert!(out.contains("Anonymous (project agent_reg_test1)"), "{out}");

    // 4. Claim ceremony: wrong code first (retry prompt), then the right one.
    let (ok, out) = run_baml(
        home.path(),
        &base,
        &["auth", "login", "--no-open", "--email", "user@example.com"],
        Some("WRONG-0000\nWXYZ-7890\n"),
    );
    assert!(ok, "{out}");
    assert!(
        out.contains("didn't match"),
        "wrong code should re-prompt: {out}"
    );
    assert!(
        out.contains("Project agent_reg_test1 and its data now belong to your account"),
        "{out}"
    );
    let json = std::fs::read_to_string(&creds).unwrap();
    assert!(json.contains(r#""status": "claimed""#), "{json}");
    assert!(
        !json.contains("claim_token"),
        "claim_token must be dropped: {json}"
    );

    // 5. whoami reports the claimed identity.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "whoami"], None);
    assert!(ok, "{out}");
    assert!(out.contains("Logged in as user@example.com"), "{out}");

    // 6. token prints the current access token.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "token"], None);
    assert!(ok, "{out}");
    assert!(out.contains("at_claimed"), "{out}");

    // 7. logout removes the credentials file; whoami reports logged out.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "logout"], None);
    assert!(ok, "{out}");
    assert!(out.contains("Logged out"), "{out}");
    assert!(!creds.exists(), "creds.json must be removed on logout");
    let (ok, out) = run_baml(home.path(), &base, &["auth", "whoami"], None);
    assert!(!ok, "whoami should exit non-zero when logged out: {out}");
    assert!(out.contains("Not logged in"), "{out}");
}

#[test]
fn playground_claim_detected_on_reexchange() {
    let state = Arc::new(MockState::default());
    let base = spawn_mock(state.clone());
    let home = tempfile::tempdir().unwrap();
    let creds = home.path().join("creds.json");

    // Seed an anonymous session whose cached token is expired.
    let jwt = mock_jwt("agent_reg_test1", None);
    std::fs::write(
        &creds,
        format!(
            r#"{{"status":"anonymous","project_id":"agent_reg_test1","assertion":"{jwt}","assertion_refresh_token":"irt_1","claim_token":"ct_anon_1","access_token":"stale","expires_at":1,"refresh_token":null,"user_email":null}}"#
        ),
    )
    .unwrap();

    // Simulate the playground having completed the claim server-side.
    state.claimed.store(true, Ordering::SeqCst);

    // whoami re-exchanges the assertion; the user-bound response flips status.
    let (ok, out) = run_baml(home.path(), &base, &["auth", "whoami"], None);
    assert!(ok, "{out}");
    assert!(out.contains("Logged in as user@example.com"), "{out}");
    let json = std::fs::read_to_string(&creds).unwrap();
    assert!(json.contains(r#""status": "claimed""#), "{json}");
    assert!(!json.contains("claim_token"), "{json}");
}
