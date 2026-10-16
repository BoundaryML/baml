//! The cost of the host boundary: decode an encoded `CallFunctionArgs`, land
//! its arguments on the engine heap, run a trivial function, and encode the
//! result. Every workload is checked to succeed before it is timed, so a
//! refusal can never masquerade as a fast path.
//!
//! Run with: cargo bench -p bridge_cffi --bench boundary
#![expect(clippy::disallowed_methods)]

use std::collections::HashMap;

use baml_type::TypeName;
use bex_project::RuntimeTy;
use bridge_cffi::baml_bridge::cffi::{
    BamlOutboundResult, BamlTy, CallFunctionArgs, InboundClassValue, InboundListValue,
    InboundMapEntry, InboundMapValue, InboundValue, InvocationOptions, baml_outbound_result,
    call_function_args::CallTarget, inbound_map_entry::Key, inbound_value::Value,
};
use divan::{Bencher, black_box};
use prost::Message;

const SOURCE: &str = r#"
class Point { x float  y float }
class Shape { name string  points Point[]  tags map<string, int> }

function Nop() -> int { 0 }
function Scalars(a: int, b: float, c: string, d: bool) -> int { a }
function Ints(xs: int[]) -> int { 0 }
function Strings(xs: string[]) -> int { 0 }
function Counts(m: map<string, int>) -> int { 0 }
function Shapes(shapes: Shape[]) -> int { 0 }
function Open(v: unknown) -> int { 0 }
function Json(v: baml.json.json) -> int { 0 }
function Either(v: int | string | Point) -> int { 0 }
"#;

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping boundary in debug/test profile.");
        return;
    }
    if std::env::var_os("DIVAN_MAX_TIME").is_none() {
        // SAFETY: single-threaded here at the very top of main, before divan
        // reads its args/env. No other thread can observe the environment.
        unsafe { std::env::set_var("DIVAN_MAX_TIME", "3") };
    }
    let mut files = HashMap::new();
    files.insert("bench.baml".to_string(), SOURCE.to_string());
    bridge_cffi::initialize_runtime(".", files).expect("benchmark runtime");
    for (name, template) in workloads() {
        let bytes = encode(&template);
        let request = bridge_cffi::decode_invocation_request(&bytes)
            .unwrap_or_else(|e| panic!("{name}: request refused: {e}"));
        let outbound = bridge_cffi::get_tokio_runtime()
            .unwrap()
            .block_on(bridge_cffi::execute_invocation(request));
        let result = BamlOutboundResult::decode(outbound.as_slice()).unwrap();
        match result.result {
            Some(baml_outbound_result::Result::Ok(_)) => {}
            other => panic!("{name}: the call did not succeed: {other:?}"),
        }
    }
    divan::main();
}

fn encode(template: &CallFunctionArgs) -> Vec<u8> {
    let mut args = template.clone();
    args.call_id = bridge_cffi::new_function_call_id();
    assert_ne!(args.call_id, 0, "call allocation failed");
    args.encode_to_vec()
}

/// Time the whole boundary for one request shape: decode, land, run, encode.
fn bench_call(bencher: Bencher, template: &CallFunctionArgs) {
    let rt = bridge_cffi::get_tokio_runtime().expect("tokio runtime");
    bencher
        .with_inputs(|| encode(template))
        .bench_values(|bytes| {
            let request = bridge_cffi::decode_invocation_request(&bytes).expect("decode");
            black_box(rt.block_on(bridge_cffi::execute_invocation(request)))
        });
}

// ---------------------------------------------------------------------------
// Wire builders. Every list, map and class node states its type, which both
// the lenient and the exact engine accept.
// ---------------------------------------------------------------------------

fn stated(ty: &RuntimeTy) -> Option<BamlTy> {
    Some(bridge_ctypes::runtime_ty_to_proto_ty(ty))
}
fn int(v: i64) -> InboundValue {
    InboundValue {
        value: Some(Value::IntValue(v)),
        ..Default::default()
    }
}
fn float(v: f64) -> InboundValue {
    InboundValue {
        value: Some(Value::FloatValue(v)),
        ..Default::default()
    }
}
fn string(v: &str) -> InboundValue {
    InboundValue {
        value: Some(Value::StringValue(v.to_string())),
        ..Default::default()
    }
}
fn boolean(v: bool) -> InboundValue {
    InboundValue {
        value: Some(Value::BoolValue(v)),
        ..Default::default()
    }
}
fn list(element: &RuntimeTy, values: Vec<InboundValue>) -> InboundValue {
    InboundValue {
        value_type: stated(&RuntimeTy::List(Box::new(element.clone()))),
        value: Some(Value::ListValue(InboundListValue { values })),
    }
}
fn entry(key: &str, value: InboundValue) -> InboundMapEntry {
    InboundMapEntry {
        key: Some(Key::StringKey(key.to_string())),
        value: Some(value),
    }
}
fn map(value_ty: &RuntimeTy, entries: Vec<InboundMapEntry>) -> InboundValue {
    InboundValue {
        value_type: stated(&RuntimeTy::Map {
            key: Box::new(RuntimeTy::String),
            value: Box::new(value_ty.clone()),
        }),
        value: Some(Value::MapValue(InboundMapValue { entries })),
    }
}
fn class(ty: &RuntimeTy, fields: Vec<InboundMapEntry>) -> InboundValue {
    InboundValue {
        value_type: stated(ty),
        value: Some(Value::ClassValue(InboundClassValue { fields })),
    }
}
fn class_ty(name: &str) -> RuntimeTy {
    RuntimeTy::Class(TypeName::from_dotted_path(name), Box::new([]))
}
fn request(name: &str, kwargs: Vec<InboundMapEntry>) -> CallFunctionArgs {
    CallFunctionArgs {
        kwargs,
        call_target: Some(CallTarget::FunctionName(name.into())),
        invocation: Some(InvocationOptions::default()),
        ..Default::default()
    }
}

fn point(i: usize) -> InboundValue {
    class(
        &class_ty("user.Point"),
        vec![
            entry("x", float(i as f64)),
            entry("y", float(i as f64 * 0.5)),
        ],
    )
}
fn shape(i: usize) -> InboundValue {
    class(
        &class_ty("user.Shape"),
        vec![
            entry("name", string(&format!("shape-{i}"))),
            entry(
                "points",
                list(&class_ty("user.Point"), (0..10).map(point).collect()),
            ),
            entry(
                "tags",
                map(
                    &RuntimeTy::Int,
                    (0..5)
                        .map(|t| entry(&format!("tag{t}"), int(t as i64)))
                        .collect(),
                ),
            ),
        ],
    )
}

fn workloads() -> Vec<(&'static str, CallFunctionArgs)> {
    vec![
        ("nop", request("Nop", vec![])),
        (
            "scalars",
            request(
                "Scalars",
                vec![
                    entry("a", int(1)),
                    entry("b", float(2.5)),
                    entry("c", string("three")),
                    entry("d", boolean(true)),
                ],
            ),
        ),
        (
            "ints_1k",
            request(
                "Ints",
                vec![entry(
                    "xs",
                    list(&RuntimeTy::Int, (0..1000).map(int).collect()),
                )],
            ),
        ),
        (
            "strings_1k",
            request(
                "Strings",
                vec![entry(
                    "xs",
                    list(
                        &RuntimeTy::String,
                        (0..1000).map(|i| string(&format!("s{i}"))).collect(),
                    ),
                )],
            ),
        ),
        (
            "map_1k",
            request(
                "Counts",
                vec![entry(
                    "m",
                    map(
                        &RuntimeTy::Int,
                        (0..1000).map(|i| entry(&format!("k{i}"), int(i))).collect(),
                    ),
                )],
            ),
        ),
        (
            "shapes_100",
            request(
                "Shapes",
                vec![entry(
                    "shapes",
                    list(&class_ty("user.Shape"), (0..100).map(shape).collect()),
                )],
            ),
        ),
        (
            "open_ints_1k",
            request(
                "Open",
                vec![entry(
                    "v",
                    list(&RuntimeTy::Int, (0..1000).map(int).collect()),
                )],
            ),
        ),
        ("union_point", request("Either", vec![entry("v", point(7))])),
        (
            "json_ints_1k",
            request(
                "Json",
                vec![entry(
                    "v",
                    list(
                        &RuntimeTy::TypeAlias(TypeName::from_dotted_path("baml.json.json")),
                        (0..1000).map(int).collect(),
                    ),
                )],
            ),
        ),
    ]
}

fn workload(name: &str) -> CallFunctionArgs {
    workloads()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, w)| w)
        .unwrap_or_else(|| panic!("no workload {name}"))
}

#[divan::bench]
fn nop(bencher: Bencher) {
    bench_call(bencher, &workload("nop"));
}
#[divan::bench]
fn scalars(bencher: Bencher) {
    bench_call(bencher, &workload("scalars"));
}
#[divan::bench]
fn ints_1k(bencher: Bencher) {
    bench_call(bencher, &workload("ints_1k"));
}
#[divan::bench]
fn strings_1k(bencher: Bencher) {
    bench_call(bencher, &workload("strings_1k"));
}
#[divan::bench]
fn map_1k(bencher: Bencher) {
    bench_call(bencher, &workload("map_1k"));
}
#[divan::bench]
fn shapes_100(bencher: Bencher) {
    bench_call(bencher, &workload("shapes_100"));
}
#[divan::bench]
fn open_ints_1k(bencher: Bencher) {
    bench_call(bencher, &workload("open_ints_1k"));
}
#[divan::bench]
fn union_point(bencher: Bencher) {
    bench_call(bencher, &workload("union_point"));
}
#[divan::bench]
fn json_ints_1k(bencher: Bencher) {
    bench_call(bencher, &workload("json_ints_1k"));
}
