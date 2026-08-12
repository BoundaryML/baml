//! Streaming replay-harness recorder
//! (thoughts/sam-projects/bridge-generics/streaming/02).
//!
//! For each recording this owns an insta **binary** snapshot under
//! `sdk_tests/fixtures/llm_functions/recordings/`:
//!
//!   * `<name>.snap` / `<name>.snap.sse` — the raw SSE response body, captured
//!     once with `curl` against the real provider (the request is built via the
//!     the fixture's recorder-only request helper, which delegates to the
//!     same native-BAML provider lowering as `$stream`) and served verbatim
//!     by the BAML replay server.
//!
//! Whether the network is hit is decided **only by insta state**: a capture
//! runs when a `.snap.sse` payload is missing or `INSTA_UPDATE=always|force`;
//! otherwise the checked-in payload is validated offline and re-asserted
//! through insta. No bespoke record env var. A real capture/refresh runs under
//! `infisical run --` (it needs `OPENAI_API_KEY`).

use std::{env, path::PathBuf, process::Command};

use baml_tests::engine::{IndexMap, compile_multi_file, run_compiled};
use bex_engine::BexExternalValue;

/// Stable, representative input. Only needs to be deterministic for snapshot
/// stability — the replay server ignores request bodies, so pytest does not
/// share it.
const RECORDING_INPUT: &str = "Seasoned software engineer with 12 years of \
experience. Specializes in Python and Rust. Currently based in Berlin. \
Interests include distributed systems and developer tooling.";

/// `base_url` for record + request-snapshot builds. The openai provider appends
/// `/chat/completions`, giving the real `…/v1/chat/completions` endpoint.
const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

const MISSING_KEY_HINT: &str = r#"
recording mode is active (a recordings/*.snap.sse payload is missing, or
INSTA_UPDATE forces an update), but OPENAI_API_KEY is not set — the live
SSE capture can't run.

maybe we should be running with Infisical?

  infisical run -- cargo nextest run -p sdk_test_llm_recordings

see sdk_tests/fixtures/llm_functions/recordings/README.md"#;

/// Snapshot basenames. Each `<name>` maps to a `lorem.*` parent function,
/// wired explicitly in the per-recording test functions below
/// (`replay_extract_string` ← `stream_e2e_extract`,
/// `replay_extract_doc` ← `stream_e2e_extract_doc`). Used here to compute
/// record-mode (any `<name>.snap.sse` missing).
const RECORDING_NAMES: &[&str] = &["replay_extract_string", "replay_extract_doc"];

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures_baml_src() -> PathBuf {
    manifest_dir().join("../../fixtures/llm_functions/baml_src")
}

fn recordings_dir() -> PathBuf {
    manifest_dir().join("../../fixtures/llm_functions/recordings")
}

// ---------------------------------------------------------------------------
// insta settings — snapshots live in the fixtures dir, not next to this test.
// ---------------------------------------------------------------------------

fn settings() -> insta::Settings {
    let mut s = insta::Settings::clone_current();
    // Resolved relative to THIS file's directory (insta runtime) →
    // sdk_tests/fixtures/llm_functions/recordings/.
    s.set_snapshot_path("../../../fixtures/llm_functions/recordings");
    s.set_prepend_module_to_snapshot(false); // no `recordings__` filename prefix
    s.add_filter(r"sk-[A-Za-z0-9_-]+", "[REDACTED-KEY]"); // belt-and-braces
    s
}

// ---------------------------------------------------------------------------
// Fixture compile + companion invocation
// ---------------------------------------------------------------------------

fn read_fixture_files() -> Vec<(String, String)> {
    let root = fixtures_baml_src();
    let mut out = Vec::new();
    collect_baml(&root, &root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!out.is_empty(), "no .baml files under {}", root.display());
    out
}

fn collect_baml(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).expect("read_dir baml_src") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_baml(root, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("baml") {
            let rel = path
                .strip_prefix(root)
                .expect("strip prefix")
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path).expect("read .baml");
            out.push((rel, content));
        }
    }
}

struct RequestParts {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: String,
}

/// Build the streaming request via a fixture helper backed by the provider's
/// native-BAML request lowering (the exact `{method,url,headers,body}` the
/// runtime would send, with `stream: true`).
/// Sets the replay client's env-resolved options first; `api_key` is the real
/// key, since the only caller is the curl capture.
async fn build_request(function: &str, api_key: &str) -> RequestParts {
    let files = read_fixture_files();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();
    let program = compile_multi_file(&refs);

    // `env.VAR` in the client options desugars to `baml.env.get_or_panic`,
    // read when the client is instantiated inside the builder — so set them
    // before running. (Process-global; nextest isolates each test in its own
    // process. Under plain `cargo test` these would race across threads.)
    env::set_var("BAML_REPLAY_BASE_URL", OPENAI_BASE_URL);
    env::set_var("BAML_REPLAY_API_KEY", api_key);

    let entry = function.to_string();
    let mut args: IndexMap<&str, BexExternalValue> = IndexMap::new();
    args.insert("text", BexExternalValue::String(RECORDING_INPUT.into()));
    let out = run_compiled(program, &entry, args, false).await;
    let value = out
        .result
        .unwrap_or_else(|e| panic!("{entry} failed: {e:?}"));
    extract_request(value)
}

fn unwrap_union(v: &BexExternalValue) -> &BexExternalValue {
    match v {
        BexExternalValue::Union { value, .. } => unwrap_union(value),
        other => other,
    }
}

fn as_string(v: &BexExternalValue) -> String {
    match unwrap_union(v) {
        BexExternalValue::String(s) => s.to_string(),
        BexExternalValue::Null => String::new(),
        other => panic!("expected string, got {other:?}"),
    }
}

fn extract_request(v: BexExternalValue) -> RequestParts {
    let fields = match unwrap_union(&v) {
        BexExternalValue::Instance { fields, .. } => fields,
        other => panic!("expected Request instance, got {other:?}"),
    };
    let get = |k: &str| {
        fields
            .get(k)
            .unwrap_or_else(|| panic!("Request missing `{k}`"))
    };
    let headers = match unwrap_union(get("headers")) {
        BexExternalValue::Map { entries, .. } => entries
            .iter()
            .map(|(k, v)| (k.clone(), as_string(v)))
            .collect(),
        other => panic!("expected headers map, got {other:?}"),
    };
    RequestParts {
        method: as_string(get("method")),
        url: as_string(get("url")),
        headers,
        body: as_string(get("body")),
    }
}

// ---------------------------------------------------------------------------
// Record-mode decision + SSE capture
// ---------------------------------------------------------------------------

fn insta_force_update() -> bool {
    matches!(
        env::var("INSTA_UPDATE").as_deref(),
        Ok("always") | Ok("force")
    )
}

fn record_mode() -> bool {
    insta_force_update()
        || RECORDING_NAMES
            .iter()
            .any(|name| !recordings_dir().join(format!("{name}.snap.sse")).exists())
}

fn openai_key() -> Option<String> {
    env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty())
}

/// Capture the raw SSE bytes for `name` by curl-ing the real provider (needs a
/// key — panics with the infisical hint otherwise).
async fn capture_sse(function: &str) -> Vec<u8> {
    let key = openai_key().unwrap_or_else(|| panic!("{MISSING_KEY_HINT}"));
    let req = build_request(function, &key).await;
    capture_via_curl(&req)
}

fn capture_via_curl(req: &RequestParts) -> Vec<u8> {
    let mut cmd = Command::new("curl");
    cmd.arg("--silent")
        .arg("--no-buffer")
        .arg("--fail-with-body")
        .arg("-X")
        .arg(&req.method)
        .arg(&req.url);
    for (k, v) in &req.headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg("--data").arg(&req.body);
    let out = cmd.output().expect("spawn curl");
    assert!(
        out.status.success(),
        "curl failed ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

/// Well-formedness checks applied to a recording in BOTH record and validate
/// modes: non-empty, several `data:` lines, properly terminated, no leaked key.
///
/// The openai client speaks the Responses API, so a recording ends with a
/// terminal `response.completed` / `response.incomplete` / `response.failed`
/// event. There is no `data: [DONE]` sentinel — that was Chat Completions,
/// which no client speaks any more.
fn validate_sse(bytes: &[u8]) {
    assert!(!bytes.is_empty(), "empty SSE recording");
    let text = String::from_utf8_lossy(bytes);
    let data_lines = text.lines().filter(|l| l.starts_with("data:")).count();
    assert!(
        data_lines > 3,
        "expected >3 `data:` lines, got {data_lines}"
    );
    let tail = text.trim_end();
    let terminated = tail.contains("\"response.completed\"")
        || tail.contains("\"response.incomplete\"")
        || tail.contains("\"response.failed\"");
    assert!(
        terminated,
        "SSE recording must end with a terminal Responses-API event \
         (response.completed / response.incomplete / response.failed)"
    );
    assert!(
        !text.contains("sk-"),
        "SSE recording contains an `sk-` substring (possible leaked key)"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The missing-key failure is a clearly-named test, not a buried panic. Passes
/// in every non-record state (keyless CI with healthy snapshots) and whenever a
/// key is available — preserving the keyless-CI guarantee.
#[test]
fn openai_api_key_available_when_recording() {
    if record_mode() && openai_key().is_none() {
        panic!("{MISSING_KEY_HINT}");
    }
}

#[tokio::test]
async fn recording_request_uses_stream_provider_lowering() {
    let request = build_request(
        "lorem.stream_e2e_extract_recording_request",
        "test-recording-key",
    )
    .await;
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "https://api.openai.com/v1/responses");
    assert!(
        request.body.contains("\"stream\":true"),
        "recording request enables streaming: {}",
        request.body
    );
    assert!(
        request.body.contains("\"input\":"),
        "recording request contains Responses input: {}",
        request.body
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_snapshot_string() {
    let bytes = obtain_sse(
        "replay_extract_string",
        "lorem.stream_e2e_extract_recording_request",
    )
    .await;
    validate_sse(&bytes);
    settings().bind(|| insta::assert_binary_snapshot!("replay_extract_string.sse", bytes.clone()));
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_snapshot_doc() {
    let bytes = obtain_sse(
        "replay_extract_doc",
        "lorem.stream_e2e_extract_doc_recording_request",
    )
    .await;
    validate_sse(&bytes);
    settings().bind(|| insta::assert_binary_snapshot!("replay_extract_doc.sse", bytes.clone()));
}

/// Capture when forced/missing, else validate the checked-in payload offline.
async fn obtain_sse(name: &str, function: &str) -> Vec<u8> {
    let payload = recordings_dir().join(format!("{name}.snap.sse"));
    if insta_force_update() || !payload.exists() {
        capture_sse(function).await
    } else {
        std::fs::read(&payload).unwrap_or_else(|e| panic!("read {}: {e}", payload.display()))
    }
}
