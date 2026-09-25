//! Offline recordings produced by the real engine → processor → publisher →
//! file sink path. Capture uses the AI-function policy on ordinary bytecode
//! functions (as the engine's own telemetry tests do), so no model is called.
#![allow(dead_code, unreachable_pub, clippy::print_stderr)]
use std::{path::Path, sync::Arc, time::Duration};

use baml_query_btel::{Index, IndexOptions, QueryRequest, QueryResult};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_recorder::RecordingConfig;
use sys_native::SysOpsExt;

pub const SOURCE: &str = r#"
class Item {
    name string
    price float
}
class Customer {
    name string
    age int
    tags string[]
}
class Order {
    customer Customer
    items Item[]
    total float
    note string?
}
class Problem {
    code int
    reason string
}
class Node {
    name string
    next Node?
}
enum Tier {
    Basic
    Premium
}

function Leaf(n: int) -> int { n + 1 }

function Classify(customer: Customer, threshold: int = 25) -> Tier {
    if (customer.age >= threshold) { Tier.Premium } else { Tier.Basic }
}

function Extract(customer: Customer, count: int) -> Order {
    let items: Item[] = [];
    let i = 0;
    while (i < count) {
        items.push(Item { name: customer.name + "-item", price: 1.5 });
        i = i + 1;
    }
    Order { customer: customer, items: items, total: 1.5, note: null }
}

function Link(node: Node) -> Node { node }

function Validate(customer: Customer) -> int throws Problem {
    if (customer.age < 25) { throw Problem { code: 42, reason: "too young" } }
    customer.age
}

function main(i: int, name: string) -> int {
    let customer = Customer { name: name, age: 20 + i, tags: ["vip", name] };
    let order = Extract(customer, i + 1);
    let child = spawn { Leaf(i) };
    let j = 0;
    let sum = 0;
    while (j < 10) {
        sum = sum + Leaf(j);
        j = j + 1;
    }
    let tier = Classify(customer);
    let node = Node { name: name, next: null };
    node.next = node;
    let linked = Link(node);
    let checked = Validate(customer) catch (e) { Problem => 0 };
    sum + (await child) + checked
}

function crash(i: int) -> int {
    let customer = Customer { name: "crash", age: i, tags: [] };
    Validate(customer)
}
"#;

/// Functions whose inputs/outputs/errors are captured.
pub const CAPTURED: &[&str] = &["Extract", "Classify", "Validate", "Link"];

pub fn program() -> bex_vm_types::Program {
    let mut program = baml_db::testing::compile_source(SOURCE);
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(f) = object
            && f.name
                .rsplit('.')
                .next()
                .is_some_and(|n| CAPTURED.contains(&n))
        {
            f.body_meta = Some(bex_vm_types::FunctionMeta::Llm {
                client: "fixture".into(),
            });
        }
    }
    program
}

pub fn engine(project: &Path, config: RecordingConfig) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new_with_telemetry_recording(
            program(),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files(project, config),
        )
        .expect("engine"),
    )
}

pub async fn run_main(engine: &Arc<BexEngine>, i: i64, name: &str) -> BexExternalValue {
    engine
        .call_function(
            "main",
            vec![
                BexExternalValue::Int(i),
                BexExternalValue::String(name.into()),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("main")
}

/// Expected `main` result: sum of Leaf(0..10) = 55, Leaf(i) = i + 1, and
/// Validate returns the age (20 + i) or 0 when it throws (age < 25).
pub fn expected_main(i: i64) -> i64 {
    let age = 20 + i;
    55 + i + 1 + if age < 25 { 0 } else { age }
}

/// One recording with the given runs, flushed and shut down.
pub async fn record(project: &Path, runs: &[(i64, &str)]) {
    let engine = engine(project, RecordingConfig::default());
    for (i, name) in runs {
        assert_eq!(
            run_main(&engine, *i, name).await,
            BexExternalValue::Int(expected_main(*i))
        );
    }
    // One uncaught throw: an errored execution with a captured error.
    let failed = engine
        .call_function(
            "crash",
            vec![BexExternalValue::Int(3)],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(failed.is_err(), "crash must throw");
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .expect("shutdown");
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
}

pub fn index(project: &Path) -> Index {
    Index::for_project(project, IndexOptions::default()).expect("open index")
}

pub fn sql(index: &mut Index, text: &str) -> QueryResult {
    index
        .refresh_and_query(&QueryRequest {
            sql: text.to_owned(),
            ..QueryRequest::default()
        })
        .unwrap_or_else(|e| panic!("{text}: {e}"))
}

pub fn column<'a>(result: &'a QueryResult, name: &str) -> Vec<&'a serde_json::Value> {
    let at = result
        .columns
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("no column {name}"));
    result.rows.iter().map(|row| &row[at]).collect()
}

/// Record `calls` (entry function, int argument) of a custom program whose
/// `captured` functions retain and capture like LLM functions. Returns each
/// call's result; the engine is shut down so every file is published.
pub async fn record_program(
    project: &Path,
    source: &str,
    captured: &[&str],
    calls: &[(&str, i64)],
) -> Vec<Result<BexExternalValue, String>> {
    let mut program = baml_db::testing::compile_source(source);
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(f) = object
            && f.name
                .rsplit('.')
                .next()
                .is_some_and(|n| captured.contains(&n))
        {
            f.body_meta = Some(bex_vm_types::FunctionMeta::Llm {
                client: "fixture".into(),
            });
        }
    }
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files(project, RecordingConfig::default()),
        )
        .expect("engine"),
    );
    let mut results = Vec::new();
    for (name, arg) in calls {
        let result = engine
            .call_function(
                name,
                vec![BexExternalValue::Int(*arg)],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await;
        results.push(result.map_err(|e| format!("{e:?}")));
    }
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .expect("shutdown");
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    results
}
