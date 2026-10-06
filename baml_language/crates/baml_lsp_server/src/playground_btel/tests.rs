use std::{sync::Arc, time::Duration};

use sys_native::SysOpsExt as _;

use super::*;

const SOURCE: &str = r#"
class Customer {
    name string
    age int
}
class Problem {
    reason string
}
function Ask(customer: Customer) -> string {
    if (customer.name == "boom") { throw Problem { reason: "boom" } }
    customer.name
}
function main(name: string) -> string {
    Ask(Customer { name: name, age: 30 })
}
"#;

/// The playground's own engine construction, recording to the project.
fn engine(project: &Path) -> Arc<bex_engine::BexEngine> {
    engine_for(project, SOURCE)
}

/// `engine` for another program, whose `Ask` is captured the same way.
fn engine_for(project: &Path, source: &str) -> Arc<bex_engine::BexEngine> {
    let mut program = baml_test_support::compile_source(source);
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(f) = object
            && f.name.ends_with(".Ask")
        {
            // Capture like an LLM function, without a model.
            f.body_meta = Some(bex_vm_types::FunctionMeta::Llm {
                client: "test".into(),
            });
        }
    }
    Arc::new(
        crate::engine::construct_engine_candidate(
            program,
            Arc::new(sys_native::SysOps::native()),
            baml_lsp::SourceRevision(1),
            Some(project),
        )
        .unwrap()
        .into_engine(),
    )
}

async fn call(engine: &Arc<bex_engine::BexEngine>, name: &str) {
    run(engine, name).await.unwrap();
}

async fn run(
    engine: &Arc<bex_engine::BexEngine>,
    name: &str,
) -> Result<bex_engine::BexExternalValue, bex_engine::EngineError> {
    engine
        .call_function(
            "main",
            vec![bex_engine::BexExternalValue::String(name.into())],
            bex_project::FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recorded_playground_runs_appear_on_refresh_with_captured_values() {
    let workspace = tempfile::tempdir().unwrap();
    let project = workspace.path().to_owned();
    let telemetry = PlaygroundTelemetry::new(Arc::from(vec![project.clone()]));
    let project_text = project.display().to_string();

    // Nothing has run: an empty list that says so.
    let (executions, missing) = telemetry.list_executions(&project_text).unwrap();
    assert!(executions.is_empty() && missing);

    let engine = engine(&project);
    // GC has its own root span; it must not become a playground run.
    engine
        .collect_garbage(bex_heap::CollectionLevel::Major)
        .await;
    call(&engine, "ann").await;
    // The default recording flushes on a deadline; poll like the UI does.
    let listed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (executions, missing) = telemetry.list_executions(&project_text).unwrap();
            let gc_recorded = telemetry
                .with_index(&project_text, |index| {
                    super::run(
                        index,
                        "SELECT COUNT(*) FROM spans WHERE span_name = 'baml.gc'",
                        vec![],
                    )
                })
                .unwrap()
                .is_some_and(|result| result.rows[0][0].as_i64().unwrap() > 0);
            if !missing && !executions.is_empty() && gc_recorded {
                return executions;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the run appears after its file is published");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["entryFqn"], "user.main");
    assert_eq!(listed[0]["status"], "succeeded");

    // A second run becomes visible on a later refresh of the same index.
    call(&engine, "bob").await;
    engine
        .collect_garbage(bex_heap::CollectionLevel::Major)
        .await;
    let listed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (executions, _) = telemetry.list_executions(&project_text).unwrap();
            if executions.len() == 2 {
                return executions;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the newly published run appears on refresh");
    let newest = listed[0]["executionId"].as_str().unwrap().to_owned();

    // Recorded locations are verified only against the program built now.
    let built = engine.source_snapshot_id();
    assert!(built.is_some());
    let opened = telemetry
        .open_execution(&project_text, &newest, built)
        .unwrap();
    assert_eq!(opened["execution"]["sourceState"], "verified");
    let edited = telemetry
        .open_execution(&project_text, &newest, Some([0; 32]))
        .unwrap();
    assert_eq!(edited["execution"]["sourceState"], "stale");
    let unbuilt = telemetry
        .open_execution(&project_text, &newest, None)
        .unwrap();
    assert_eq!(unbuilt["execution"]["sourceState"], "unverified");
    assert_eq!(opened["execution"]["executionId"], newest);
    assert_eq!(opened["execution"]["callsRetained"], 1);
    assert_eq!(opened["execution"]["threadsTotal"], 1);
    let calls = opened["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["fqn"], "user.Ask");
    assert_eq!(calls[0]["argsState"], "available");
    let args: Value = serde_json::from_str(calls[0]["args"].as_str().unwrap()).unwrap();
    assert_eq!(args["customer"]["name"], "bob");
    assert_eq!(calls[0]["output"], "\"bob\"");
    assert_eq!(calls[0]["outputState"], "available");
    assert_eq!(calls[0]["errorState"], "not_applicable");
    assert!(calls[0]["callPathId"].is_string());

    // Timeline lanes come from the recording.
    let threads = opened["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(
        threads[0]["threadId"], newest,
        "the root call is the execution"
    );
    assert_eq!(threads[0]["kind"], "root");
    assert_eq!(threads[0]["endStatus"], "completed");
    assert_eq!(threads[0]["name"], Value::Null);
    assert_eq!(calls[0]["threadId"], newest);
    assert_eq!(
        calls[0]["parentCallId"],
        Value::Null,
        "Ask ran on the root call"
    );
    // The playground's own process is still running: no profiler yet.
    assert!(opened["callPaths"].as_array().unwrap().is_empty());
    assert!(opened["errors"].as_array().unwrap().is_empty());

    // A run whose retained call throws: the call is evidence of an error,
    // not a throw occurrence, and says so.
    assert!(run(&engine, "boom").await.is_err());
    let failed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (executions, _) = telemetry.list_executions(&project_text).unwrap();
            if let Some(failed) = executions.iter().find(|e| e["status"] == "failed") {
                return failed["executionId"].as_str().unwrap().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the failed run appears");
    let opened = telemetry
        .open_execution(&project_text, &failed, None)
        .unwrap();
    assert_eq!(opened["execution"]["totalErrors"], Value::Null);
    // The failed call's error, with the stack its context recorded.
    let errors = opened["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let error = &errors[0];
    assert_eq!(error["grain"], "errored_call");
    assert_eq!(error["kind"], "throw");
    assert_eq!(error["callId"], opened["calls"][0]["callId"]);
    assert_eq!(error["throwFqn"], "user.Ask");
    assert_eq!(error["throwSiteFile"], "test.baml");
    assert_eq!(error["throwSiteLine"], 10);
    assert_eq!(error["stack"], json!(["user.main", "user.Ask"]));
    assert_eq!(error["stackComplete"], true);
    let value: Value = serde_json::from_str(error["value"].as_str().unwrap()).unwrap();
    assert_eq!(value["reason"], "boom");
    engine.shutdown().await;
    let (executions, _) = telemetry.list_executions(&project_text).unwrap();
    assert_eq!(executions.len(), 3, "shutdown GC is not another run");
    assert!(executions.iter().all(|e| e["entryFqn"] == "user.main"));

    // Outside the served workspace: refused.
    let elsewhere = tempfile::tempdir().unwrap();
    assert!(
        telemetry
            .list_executions(&elsewhere.path().display().to_string())
            .is_err()
    );
}

/// An HTTP request is recorded as a network span under the call that made
/// it. The playground has no view for those yet: they stay out of its span
/// tree, and the execution opens as it would without them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn network_spans_stay_out_of_the_span_tree() {
    const FETCHING: &str = r#"
class Pong {
    server: baml.http.Server,
    function handle(self, req: baml.http.ServerRequest) -> baml.http.Response {
        baml.http.Response.new(200, { "content-type": "text/plain" }, "pong".to_utf8())
    }
}
function Ask(url: string) -> string {
    baml.http.fetch(url).text()
}
function main(name: string) -> string {
    let pong = Pong { server: baml.http.Server.bind("127.0.0.1:0") };
    let task = spawn { pong.server.serve(pong.handle) };
    let reply = Ask("http://" + pong.server.addr + "/" + name);
    task.cancel();
    reply
}
"#;
    let workspace = tempfile::tempdir().unwrap();
    let project = workspace.path().to_owned();
    let telemetry = PlaygroundTelemetry::new(Arc::from(vec![project.clone()]));
    let project_text = project.display().to_string();
    let engine = engine_for(&project, FETCHING);
    assert_eq!(
        run(&engine, "ann").await.unwrap(),
        bex_engine::BexExternalValue::String("pong".into())
    );
    engine.shutdown().await;
    // The server's handler runs as a root call of its own.
    let (executions, _) = telemetry.list_executions(&project_text).unwrap();
    let main = executions
        .iter()
        .find(|execution| execution["entryFqn"] == "user.main")
        .unwrap_or_else(|| panic!("{executions:#?}"));
    assert_eq!(main["status"], "succeeded");
    let execution = main["executionId"].as_str().unwrap().to_owned();

    // The request is recorded, under the call that made it.
    let mut index = Index::for_project(&project, IndexOptions::default()).unwrap();
    let network = index
        .refresh_and_query(&QueryRequest {
            sql: "SELECT span_id, parent_span_id, is_complete FROM span_announcements
                  WHERE span_type = 'network_span'"
                .into(),
            ..QueryRequest::default()
        })
        .unwrap()
        .rows;
    assert_eq!(network.len(), 1, "{network:#?}");
    assert_eq!(network[0][2], 1);

    let opened = telemetry
        .open_execution(&project_text, &execution, None)
        .unwrap();
    let calls = opened["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1, "{calls:#?}");
    assert_eq!(calls[0]["fqn"], "user.Ask");
    assert_eq!(calls[0]["callId"], network[0][1]);
    assert_eq!(calls[0]["status"], "ok");
    assert_eq!(calls[0]["output"], "\"pong\"");
    assert_eq!(opened["execution"]["callsRetained"], 1);
    // The root call and the server's future: no lane for the request.
    let threads = opened["threads"].as_array().unwrap();
    assert!(
        threads
            .iter()
            .all(|thread| thread["threadId"] != network[0][0]),
        "{threads:#?}"
    );
    assert!(opened["errors"].as_array().unwrap().is_empty());
}
