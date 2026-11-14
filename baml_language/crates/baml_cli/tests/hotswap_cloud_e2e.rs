//! Hot swapping from Boundary Cloud end to end (BEP-087, stage 2): the real
//! `baml-cli cloud hotswaps` against a fake BCS that keeps the contract's
//! state, and a running engine whose agent registers with it.
//!
//! BCS itself is tested in its own repository; the fake here holds only what
//! a publisher and a process can observe.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::{Command, Output},
    sync::{Arc, Mutex},
    time::Duration,
};

use bcs_api::{
    credentials::{Authentication, RequestAuthorization, Target},
    hotswaps::{Swap, SwapFunction},
};
use bex_engine::{BexEngine, BexExternalValue, CallId, EngineConfig, FunctionCallContextBuilder};
use serde_json::{Value, json};
use sys_native::SysOpsExt as _;
use wiremock::{Request, Respond, ResponseTemplate};

const SOURCE: &str = r#"
function helper(x: int) -> int { x + 1 }

/// baml:hotswap=hotswap.no_new_calls
function step(x: int) -> int { helper(x) }

function main() -> int { step(1) }
"#;

const API_KEY: &str = "bdry_secret_hotswap_test";

#[derive(Default)]
struct FakeSwap {
    id: String,
    baseline: String,
    require_approval: bool,
    replaces: Option<String>,
    revision: u64,
    status: String,
    approved_by: Option<String>,
    functions: Vec<Value>,
    matched: Vec<String>,
    outcomes: BTreeMap<String, String>,
    events: Vec<Value>,
    payload: bool,
    enabled: bool,
}

#[derive(Default)]
struct State {
    base: String,
    swaps: Vec<FakeSwap>,
    processes: BTreeSet<String>,
    uploaded: BTreeSet<String>,
    requests: Vec<String>,
}

impl FakeSwap {
    fn event(&mut self, mut event: Value) {
        event["sequence"] = json!(self.events.len() + 1);
        event["at"] = json!("2026-11-13T12:00Z");
        self.events.push(event);
    }

    fn deliverable(&self) -> bool {
        self.payload && self.enabled && (self.approved_by.is_some() || !self.require_approval)
    }

    fn json(&self) -> Value {
        let installed = self.outcomes.values().filter(|s| *s == "installed").count();
        let rejected = self.outcomes.values().filter(|s| *s == "rejected").count();
        json!({
            "id": self.id,
            "status": self.status,
            "revision": self.revision,
            "baseline": self.baseline,
            "query": "SELECT process_id FROM processes",
            "requireApproval": self.require_approval,
            "replaces": self.replaces,
            "functions": self.functions.iter().map(|f| json!({
                "name": f["name"], "rule": f["rule"],
                "oldSource": f["oldSource"], "newSource": f["newSource"],
            })).collect::<Vec<_>>(),
            "publishedBy": "alice",
            "approvedBy": self.approved_by,
            "processes": {
                "matched": self.matched.len(), "skipped": 0,
                "installed": installed, "rejected": rejected,
            },
        })
    }

    fn swap(&self) -> Value {
        json!({
            "id": self.id, "revision": self.revision, "baseline": self.baseline,
            "replaces": self.replaces, "publishedBy": "alice",
            "approvedBy": self.approved_by, "functions": self.functions,
        })
    }
}

impl State {
    fn swap(&mut self, id: &str) -> &mut FakeSwap {
        self.swaps
            .iter_mut()
            .find(|s| s.id == id)
            .expect("a known swap")
    }

    fn notifications(&self, process: &str) -> Vec<Value> {
        self.swaps
            .iter()
            .filter(|swap| swap.matched.iter().any(|p| p == process))
            .filter_map(|swap| {
                let status = if swap.status == "deleted" {
                    "deleted"
                } else if !swap.enabled {
                    "disabled"
                } else if swap.deliverable() {
                    "active"
                } else {
                    return None;
                };
                Some(json!({"type": "hotswap", "id": swap.id, "revision": swap.revision, "status": status}))
            })
            .collect()
    }

    fn deliverable_to(&self, process: &str) -> Vec<Value> {
        self.swaps
            .iter()
            .filter(|swap| swap.deliverable() && swap.matched.iter().any(|p| p == process))
            .map(FakeSwap::swap)
            .collect()
    }
}

struct Fake(Arc<Mutex<State>>);

impl Respond for Fake {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut state = self.0.lock().unwrap();
        let method = request.method.as_str().to_string();
        let path = request.url.path().to_string();
        state.requests.push(format!("{method} {path}"));
        assert_eq!(
            request
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok()),
            Some(format!("Bearer {API_KEY}").as_str()),
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let ok = |value: Value| ResponseTemplate::new(200).set_body_json(value);
        match (method.as_str(), segments.as_slice()) {
            ("POST", ["v1", "hotswaps"]) => {
                let id = format!("hs_{}", state.swaps.len() + 1);
                let matched: Vec<String> = state.processes.iter().cloned().collect();
                let mut swap = FakeSwap {
                    id: id.clone(),
                    baseline: body["baseline"].as_str().unwrap().into(),
                    require_approval: body["requireApproval"].as_bool().unwrap(),
                    replaces: body["replaces"].as_str().map(str::to_string),
                    revision: 1,
                    status: "matching".into(),
                    matched,
                    enabled: true,
                    ..FakeSwap::default()
                };
                swap.event(json!({"status": "matching"}));
                let count = swap.matched.len();
                swap.event(json!({"status": "matched", "baseline": swap.baseline, "processes": count, "skipped": 0}));
                swap.status = "matched".into();
                let json = swap.json();
                state.swaps.push(swap);
                ResponseTemplate::new(201).set_body_json(json)
            }
            ("GET", ["v1", "hotswaps"]) => ok(
                json!({"hotswaps": state.swaps.iter().filter(|s| s.status != "deleted").map(FakeSwap::json).collect::<Vec<_>>()}),
            ),
            ("GET", ["v1", "hotswaps", id]) => ok(state.swap(id).json()),
            ("DELETE", ["v1", "hotswaps", id]) => {
                let swap = state.swap(id);
                swap.status = "deleted".into();
                swap.revision += 1;
                swap.event(json!({"status": "deleted", "by": "alice"}));
                ok(swap.json())
            }
            ("GET", ["v1", "hotswaps", id, "events"]) => {
                let after: usize = request
                    .url
                    .query_pairs()
                    .find(|(k, _)| k == "after")
                    .and_then(|(_, v)| v.parse().ok())
                    .unwrap_or(0);
                let lines: String = state
                    .swap(id)
                    .events
                    .iter()
                    .skip(after)
                    .map(|event| format!("{event}\n"))
                    .collect();
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/x-ndjson")
                    .set_body_string(lines)
            }
            ("POST", ["v1", "hotswaps", id, "uploads:prepare"]) => {
                let base = state.base.clone();
                let files: Vec<Value> = body["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|file| {
                        let sha = file["sha256"].as_str().unwrap();
                        if state.uploaded.contains(sha) {
                            json!({"sha256": sha, "disposition": "available"})
                        } else {
                            json!({"sha256": sha, "disposition": "upload",
                                   "uploadUrl": format!("/v1/hotswaps/{id}/sources/{sha}")})
                        }
                    })
                    .collect();
                ok(json!({ "files": files }))
            }
            ("PUT", ["v1", "hotswaps", _, "sources", sha]) => {
                state.uploaded.insert((*sha).to_string());
                ResponseTemplate::new(204)
            }
            ("POST", ["v1", "hotswaps", id, "payload"]) => {
                let swap = state.swap(id);
                swap.functions = body["functions"].as_array().unwrap().clone();
                swap.payload = true;
                swap.revision += 1;
                if swap.require_approval {
                    swap.status = "pending_approval".into();
                    swap.event(json!({"status": "pending_approval"}));
                } else {
                    swap.status = "approved".into();
                    swap.event(json!({"status": "approved"}));
                }
                ok(swap.json())
            }
            ("POST", ["v1", "hotswaps", id, "approve"]) => {
                let swap = state.swap(id);
                swap.approved_by = Some("bob".into());
                swap.status = "approved".into();
                swap.revision += 1;
                swap.event(json!({"status": "approved", "by": "bob"}));
                ok(swap.json())
            }
            ("POST", ["v1", "processes"]) => {
                let process = body["processId"].as_str().unwrap().to_string();
                state.processes.insert(process.clone());
                ResponseTemplate::new(201).set_body_json(json!({
                    "swaps": state.deliverable_to(&process),
                    "notifications": [],
                    "holdSeconds": 1,
                }))
            }
            ("POST", ["v1", "processes", process, "notifications", "wait"])
                if !state.processes.contains(*process) =>
            {
                ResponseTemplate::new(404)
                    .set_body_json(json!({"code": "NOT_FOUND", "retryable": false}))
            }
            ("POST", ["v1", "processes", process, "notifications", "wait"]) => {
                let notifications = state.notifications(process);
                let installed: BTreeSet<(String, u64)> = body["installed"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| {
                        (
                            i["id"].as_str().unwrap().into(),
                            i["revision"].as_u64().unwrap(),
                        )
                    })
                    .collect();
                let news = notifications.iter().any(|n| {
                    !installed.contains(&(
                        n["id"].as_str().unwrap().into(),
                        n["revision"].as_u64().unwrap(),
                    ))
                });
                let response = ok(json!({"notifications": notifications, "holdSeconds": 1}));
                if news {
                    response
                } else {
                    response.set_delay(Duration::from_millis(100))
                }
            }
            ("POST", ["v1", "processes", _, "hotswaps", "fetch"]) => {
                let ids: Vec<String> = body["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap().to_string())
                    .collect();
                let swaps: Vec<Value> = state
                    .swaps
                    .iter()
                    .filter(|swap| ids.contains(&swap.id) && swap.deliverable())
                    .map(FakeSwap::swap)
                    .collect();
                ok(json!({ "swaps": swaps }))
            }
            ("POST", ["v1", "processes", process, "hotswaps", id, "status"]) => {
                let process = (*process).to_string();
                let swap = state.swap(id);
                let status = body["status"].as_str().unwrap().to_string();
                swap.outcomes.insert(process.clone(), status.clone());
                swap.event(json!({"process": process, "status": status, "reason": body["reason"]}));
                if swap.outcomes.len() == swap.matched.len()
                    && matches!(status.as_str(), "installed" | "rejected")
                {
                    let installed = swap.outcomes.values().filter(|s| *s == "installed").count();
                    let rejected = swap.outcomes.len() - installed;
                    swap.status = if rejected == 0 {
                        "installed"
                    } else {
                        "rejected"
                    }
                    .into();
                    let status = swap.status.clone();
                    swap.event(
                        json!({"status": status, "installed": installed, "rejected": rejected}),
                    );
                }
                ok(json!({ "notifications": [] }))
            }
            _ => ResponseTemplate::new(404)
                .set_body_json(json!({"code": "NOT_FOUND", "retryable": false})),
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn baml(dir: &Path, home: &Path, url: &str, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    command
        .args(args)
        .current_dir(dir)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("BAML_HOME", home)
        .env("BOUNDARY_API_KEY", API_KEY)
        .env("BOUNDARY_API_URL", url)
        .env_remove("BOUNDARY_PROJECT")
        .env_remove("BAML_BUILD_CACHE");
    command
}

fn text(output: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

async fn step(engine: &Arc<BexEngine>) -> i64 {
    let context = FunctionCallContextBuilder::new(CallId::next()).build();
    match engine
        .call_function("step", vec![BexExternalValue::Int(1)], context, true)
        .await
        .unwrap()
    {
        BexExternalValue::Int(value) => value,
        other => panic!("step returned {other:?}"),
    }
}

/// The engine as a process built from `commit` runs it, with its agent.
fn process(url: &str, commit: &str) -> (Arc<BexEngine>, bex_hotswap::agent::Agent) {
    let build = Arc::new(bex_hotswap::HotswapBuild {
        sources: [("main.baml".to_string(), SOURCE.to_string())].into(),
        commit: Some(commit.to_string()),
        config: bex_hotswap::HotswapConfig::default(),
    });
    let program = baml_tests::engine::compile_source(SOURCE);
    let engine = Arc::new(
        BexEngine::new_with_config(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            EngineConfig {
                runtime_compiler: Some(bex_project::runtime_compiler()),
                hotswap_baseline: Some(build.baseline()),
                hotswap_marked: Some(build.marked()),
                ..EngineConfig::default()
            },
        )
        .unwrap(),
    );
    let endpoint = bcs_api::Endpoint::parse(url).unwrap();
    let link = bex_hotswap::agent::CloudLink {
        client: bcs_api::Client::new(endpoint.clone()).unwrap(),
        authorization: RequestAuthorization {
            authentication: Authentication::shared(
                endpoint.as_str(),
                bcs_api::Secret::new(API_KEY.to_string()),
            ),
            target: Some(Target::default()),
        },
    };
    let agent = bex_hotswap::agent::start(
        Arc::clone(&engine),
        build,
        link,
        bex_hotswap::agent::AgentOptions {
            process_id: "p_91".into(),
            baml_version: "test".into(),
            host: "test".into(),
            lapse: None,
            retry: Some(Duration::from_millis(100)),
        },
    )
    .unwrap();
    assert!(agent.wait_ready(Duration::from_secs(10)), "registered");
    (engine, agent)
}

/// Register, create, approve, install: the whole path a swap takes, and
/// `list`, `show` and `delete` after it.
#[tokio::test(flavor = "multi_thread")]
async fn create_then_approve_installs_on_the_registered_process() {
    let server = wiremock::MockServer::start().await;
    let state = Arc::new(Mutex::new(State {
        base: server.uri(),
        ..State::default()
    }));
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(Fake(Arc::clone(&state)))
        .mount(&server)
        .await;
    let url = server.uri();

    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        "[update]\nauto_check = false\n",
    )
    .unwrap();
    let dir = repo.path();
    std::fs::write(dir.join("baml.toml"), "[package]\nname = \"app\"\n").unwrap();
    std::fs::write(dir.join("main.baml"), SOURCE).unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-qm", "build"]);
    let commit = git(dir, &["rev-parse", "HEAD"]);

    let (engine, agent) = {
        let (url, commit) = (url.clone(), commit.clone());
        tokio::task::spawn_blocking(move || process(&url, &commit))
            .await
            .unwrap()
    };
    assert_eq!(step(&engine).await, 2);
    assert_eq!(state.lock().unwrap().processes.len(), 1);

    // An unmarked function's change is refused before anything uploads.
    std::fs::write(
        dir.join("main.baml"),
        SOURCE.replace("{ x + 1 }", "{ x + 2 }"),
    )
    .unwrap();
    let refused = baml(
        dir,
        home.path(),
        &url,
        &[
            "cloud",
            "hotswaps",
            "create",
            "SELECT process_id FROM processes",
        ],
    )
    .output()
    .unwrap();
    // A refused swap is deleted, so nothing waits for approval.
    assert_eq!(
        state
            .lock()
            .unwrap()
            .swaps
            .last()
            .map(|s| s.status.clone())
            .as_deref(),
        Some("deleted")
    );
    let (_, stderr) = text(&refused);
    assert!(!refused.status.success());
    assert!(
        stderr
            .contains("`helper` changed, and only swappable functions can change without a deploy"),
        "{stderr}"
    );

    // A marked function's change goes out once approved.
    std::fs::write(
        dir.join("main.baml"),
        SOURCE.replace("{ helper(x) }", "{ helper(x) * 10 }"),
    )
    .unwrap();
    let mut create = baml(
        dir,
        home.path(),
        &url,
        &[
            "cloud",
            "hotswaps",
            "create",
            "SELECT process_id FROM processes",
        ],
    );
    create
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let create = create.spawn().unwrap();
    // Approve once the swap waits for it.
    let id = loop {
        let pending = state
            .lock()
            .unwrap()
            .swaps
            .iter()
            .find(|swap| swap.status == "pending_approval")
            .map(|swap| swap.id.clone());
        if let Some(id) = pending {
            break id;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let approved = baml(
        dir,
        home.path(),
        &url,
        &["cloud", "hotswaps", "update", &id, "--approve"],
    )
    .output()
    .unwrap();
    assert!(approved.status.success(), "{:?}", text(&approved));
    let created = tokio::task::spawn_blocking(move || create.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    let (stdout, stderr) = text(&created);
    assert!(created.status.success(), "{stdout}\n{stderr}");
    for line in [
        &format!("Created {id}"),
        "Matched 1 processes",
        "Changed step, rule hotswap.no_new_calls",
        "Uploaded 1 file",
        "Waiting for approval",
        "Approved by bob",
        "Installed 1/1 processes",
    ] {
        assert!(stderr.contains(line), "missing {line:?} in:\n{stderr}");
    }
    assert_eq!(
        stdout.split_whitespace().collect::<Vec<_>>(),
        [&id, "installed", "step", "1/1"]
    );
    assert_eq!(step(&engine).await, 20);

    // The process reported its outcome; JSON output follows the contract.
    let listed = baml(
        dir,
        home.path(),
        &url,
        &["cloud", "hotswaps", "list", "--output-format", "json"],
    )
    .output()
    .unwrap();
    let rows: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(
        rows,
        json!([{"id": id, "status": "installed", "functions": ["step"],
                "processes": {"matched": 1, "installed": 1, "rejected": 0}}])
    );
    let shown = baml(
        dir,
        home.path(),
        &url,
        &["cloud", "hotswaps", "show", &id, "--events"],
    )
    .output()
    .unwrap();
    let (stdout, _) = text(&shown);
    assert!(
        stdout.contains("step (rule: hotswap.no_new_calls)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("+function step(x: int) -> int { helper(x) * 10 }"),
        "{stdout}"
    );
    assert!(stdout.contains("process p_91  installed"), "{stdout}");

    // Deleting it returns the process to the code as built.
    let deleted = baml(
        dir,
        home.path(),
        &url,
        &["cloud", "hotswaps", "delete", &id],
    )
    .output()
    .unwrap();
    assert!(deleted.status.success(), "{:?}", text(&deleted));
    for _ in 0..100 {
        if step(&engine).await == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(step(&engine).await, 2);
    agent.stop();
}

/// The rule runs in the process, and its rejection reaches the publisher.
#[tokio::test(flavor = "multi_thread")]
async fn a_rule_rejection_reaches_the_publisher() {
    let server = wiremock::MockServer::start().await;
    let state = Arc::new(Mutex::new(State {
        base: server.uri(),
        ..State::default()
    }));
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(Fake(Arc::clone(&state)))
        .mount(&server)
        .await;
    let url = server.uri();
    let commit = "3f2a91c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6";
    let (engine, agent) = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || process(&url, commit))
            .await
            .unwrap()
    };
    // Published straight into the fake, as a CLI with no git would.
    {
        let mut state = state.lock().unwrap();
        let old =
            "/// baml:hotswap=hotswap.no_new_calls\nfunction step(x: int) -> int { helper(x) }";
        let function = SwapFunction {
            name: "step".into(),
            file: "main.baml".into(),
            rule: "hotswap.no_new_calls".into(),
            old_source: old.into(),
            new_source: old.replace("helper(x)", "main()"),
        };
        let mut swap = FakeSwap {
            id: "hs_9".into(),
            baseline: commit.into(),
            revision: 3,
            status: "approved".into(),
            approved_by: Some("bob".into()),
            functions: vec![serde_json::to_value(&function).unwrap()],
            matched: vec!["p_91".into()],
            payload: true,
            enabled: true,
            ..FakeSwap::default()
        };
        swap.event(json!({"status": "approved", "by": "bob"}));
        let _: Swap = serde_json::from_value(swap.swap()).unwrap();
        state.swaps.push(swap);
    }
    for _ in 0..100 {
        if state.lock().unwrap().swaps[0].outcomes.contains_key("p_91") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let state = state.lock().unwrap();
    let swap = &state.swaps[0];
    assert_eq!(swap.outcomes["p_91"], "rejected");
    let reason = swap
        .events
        .iter()
        .find(|event| event["process"] == "p_91")
        .unwrap()["reason"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        reason,
        "step: hotswap.no_new_calls: the replacement calls `main`, which the original does not"
    );
    drop(state);
    assert_eq!(step(&engine).await, 2);
    agent.stop();
}

/// `baml run` registers a process that marks functions with BCS, unless
/// `--no-hotswap` or `[hotswap] enabled = false` turns swapping off.
#[tokio::test(flavor = "multi_thread")]
async fn no_hotswap_keeps_a_process_from_registering() {
    let server = wiremock::MockServer::start().await;
    let state = Arc::new(Mutex::new(State {
        base: server.uri(),
        ..State::default()
    }));
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(Fake(Arc::clone(&state)))
        .mount(&server)
        .await;
    let url = server.uri();
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        "[update]\nauto_check = false\n",
    )
    .unwrap();
    let dir = repo.path().to_path_buf();
    std::fs::write(dir.join("baml.toml"), "[package]\nname = \"app\"\n").unwrap();
    std::fs::write(dir.join("main.baml"), SOURCE).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "build"]);

    let registered = |state: &Arc<Mutex<State>>| {
        state
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|request| *request == "POST /v1/processes")
            .count()
    };
    let run = |args: &'static [&'static str]| {
        let (dir, home, url) = (dir.clone(), home.path().to_path_buf(), url.clone());
        tokio::task::spawn_blocking(move || baml(&dir, &home, &url, args).output().unwrap())
    };
    let output = run(&["run", "--no-hotswap", "main"]).await.unwrap();
    assert_eq!(text(&output).0.trim(), "2", "{:?}", text(&output));
    assert_eq!(registered(&state), 0);

    // With a startup wait, the process registers before it serves.
    let output = run(&["run", "--hotswap-startup-wait", "5s", "main"])
        .await
        .unwrap();
    assert_eq!(text(&output).0.trim(), "2", "{:?}", text(&output));
    assert_eq!(registered(&state), 1);

    std::fs::write(
        dir.join("baml.toml"),
        "[package]\nname = \"app\"\n\n[hotswap]\nenabled = false\nstartup_wait = \"5s\"\n",
    )
    .unwrap();
    let output = run(&["run", "main"]).await.unwrap();
    assert_eq!(text(&output).0.trim(), "2", "{:?}", text(&output));
    assert_eq!(registered(&state), 1);
}

/// A process registers again once BCS no longer knows it, such as after its
/// database was reset.
#[tokio::test(flavor = "multi_thread")]
async fn a_forgotten_process_registers_again() {
    let server = wiremock::MockServer::start().await;
    let state = Arc::new(Mutex::new(State {
        base: server.uri(),
        ..State::default()
    }));
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(Fake(Arc::clone(&state)))
        .mount(&server)
        .await;
    let url = server.uri();
    let (_engine, agent) = tokio::task::spawn_blocking(move || {
        process(&url, "3f2a91c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6")
    })
    .await
    .unwrap();
    let registrations = |state: &Arc<Mutex<State>>| {
        state
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|request| *request == "POST /v1/processes")
            .count()
    };
    assert_eq!(registrations(&state), 1);
    state.lock().unwrap().processes.clear();
    for _ in 0..100 {
        if registrations(&state) == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(registrations(&state), 2);
    assert!(state.lock().unwrap().processes.contains("p_91"));
    tokio::task::spawn_blocking(move || agent.stop()).await.unwrap();
}
