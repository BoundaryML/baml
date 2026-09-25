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
    let mut program = baml_db::testing::compile_source(SOURCE);
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
    call(&engine, "ann").await;
    // The default recording flushes on a deadline; poll like the UI does.
    let listed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (executions, missing) = telemetry.list_executions(&project_text).unwrap();
            if !missing && !executions.is_empty() {
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
    assert_eq!(listed[0]["callsRetained"], 1);

    // A second run becomes visible on a later refresh of the same index.
    call(&engine, "bob").await;
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

    // Timeline lanes and calling contexts come from the recording.
    let threads = opened["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(
        threads[0]["threadId"], newest,
        "the root thread is the execution"
    );
    assert_eq!(threads[0]["kind"], "root");
    assert_eq!(threads[0]["endStatus"], "completed");
    assert_eq!(
        threads[0]["name"],
        Value::Null,
        "thread names are not recorded"
    );
    let paths = opened["callPaths"].as_array().unwrap();
    let ask = paths
        .iter()
        .find(|path| path["fqn"] == "user.Ask")
        .expect("Ask's calling context");
    assert_eq!(ask["completedCalls"], 1);
    assert_eq!(ask["callsStarted"], Value::Null, "starts are not recorded");
    assert_eq!(ask["completedOk"], 1);
    assert_eq!(ask["completedError"], 0);
    assert_eq!(ask["completedCancelled"], 0);
    assert_eq!(ask["outcomeState"], "recorded");
    assert_eq!(ask["edgeKind"], "call");
    assert_eq!(ask["callPathId"], calls[0]["callPathId"]);
    // The call expression in main, from the recording's source map.
    assert_eq!(ask["callSiteState"], "resolved");
    assert_eq!(ask["callSiteFile"], "test.baml");
    assert_eq!(ask["callSiteLine"], 14);
    let (start, end) = (
        usize::try_from(ask["callSiteStart"].as_u64().unwrap()).unwrap(),
        usize::try_from(ask["callSiteEnd"].as_u64().unwrap()).unwrap(),
    );
    assert_eq!(&SOURCE[start..end], "Ask(Customer { name: name, age: 30 })");
    assert_eq!(calls[0]["callSiteLine"], 14);
    let main = paths
        .iter()
        .find(|path| path["fqn"] == "user.main")
        .unwrap();
    assert_eq!(main["edgeKind"], "root");
    assert_eq!(main["completedOk"], 1, "timing-only invocation counted");
    assert_eq!(main["depth"], 0);
    assert_eq!(ask["parentCallPathId"], main["callPathId"]);
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
    for path in opened["callPaths"].as_array().unwrap() {
        assert_eq!(path["completedOk"], 0);
        assert_eq!(path["completedError"], 1);
        assert_eq!(path["completedCancelled"], 0);
        assert_eq!(path["outcomeState"], "recorded");
    }
    // One recorded throw: its origin, stack and the calls it failed. The
    // failed call is not listed a second time as an errored call.
    let errors = opened["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let error = &errors[0];
    assert_eq!(error["grain"], "throw");
    assert_eq!(error["kind"], "fresh");
    assert_eq!(error["raiseKind"], "throw");
    assert_eq!(error["originState"], "fresh");
    assert_eq!(error["throwFqn"], "user.Ask");
    assert_eq!(error["throwCallId"], opened["calls"][0]["callId"]);
    assert_eq!(error["throwCallPathId"], opened["calls"][0]["callPathId"]);
    assert_eq!(error["throwSiteFile"], "test.baml");
    assert_eq!(error["throwSiteLine"], 10);
    let (start, end) = (
        usize::try_from(error["throwSiteStart"].as_u64().unwrap()).unwrap(),
        usize::try_from(error["throwSiteEnd"].as_u64().unwrap()).unwrap(),
    );
    assert_eq!(&SOURCE[start..end], "throw Problem { reason: \"boom\" }");
    assert_eq!(error["unwindResult"], "unhandled");
    assert_eq!(error["stack"], json!(["user.main", "user.Ask"]));
    assert_eq!(error["stackComplete"], true);
    assert_eq!(
        error["failedCallIds"],
        json!([opened["calls"][0]["callId"]])
    );
    let value: Value = serde_json::from_str(error["value"].as_str().unwrap()).unwrap();
    assert_eq!(value["reason"], "boom");
    engine.shutdown().await;

    // Outside the served workspace: refused.
    let elsewhere = tempfile::tempdir().unwrap();
    assert!(
        telemetry
            .list_executions(&elsewhere.path().display().to_string())
            .is_err()
    );
}
