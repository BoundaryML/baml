//! The JSON state dump, from live VMs and from snapshot bytes.

#![cfg(not(target_arch = "wasm32"))]

mod common;

use bex_snapshot::{ParkedAt, ThreadInput};
use common::Harness;
use serde_json::Value as Json;

const PROGRAM: &str = r#"
class Node {
    label string
    next Node?
    scores int[]
}

function inner(node: Node, depth: int) -> int {
    let note = "at depth " + depth.to_string();
    baml.io.println(note);
    depth
}

function main() -> int {
    let first = Node { label: "first", next: null, scores: [1, 2, 3] };
    let second = Node { label: "second", next: first, scores: [] };
    first.next = second;
    let lookup: map<string, int> = { "a": 1, "b": 2 };
    inner(first, 4) + lookup["a"]
}
"#;

fn local<'a>(frame: &'a Json, name: &str) -> &'a Json {
    frame["locals"]
        .as_array()
        .expect("locals")
        .iter()
        .find(|local| local["name"] == name)
        .unwrap_or_else(|| panic!("local {name} missing in {frame}"))
}

fn child<'a>(value: &'a Json, key: &str) -> &'a Json {
    value["children"]
        .as_array()
        .unwrap_or_else(|| panic!("no children in {value}"))
        .iter()
        .find(|child| child["key"] == key)
        .map(|child| &child["value"])
        .unwrap_or_else(|| panic!("child {key} missing in {value}"))
}

fn contains_kind(value: &Json, kind: &str) -> bool {
    value["kind"] == kind
        || value["children"]
            .as_array()
            .is_some_and(|children| children.iter().any(|c| contains_kind(&c["value"], kind)))
}

fn check_dump(dump: &Json) {
    assert_eq!(dump["run"], "r-test");
    assert_eq!(dump["segment"], 1);
    assert!(dump["created_ts"].is_u64());

    let thread = &dump["threads"][0];
    assert_eq!(thread["thread"], 1);
    assert_eq!(thread["name"], "main");
    assert_eq!(thread["parked"]["kind"], "sysop");

    // Innermost first.
    let frames = thread["frames"].as_array().expect("frames");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["function"], "user.inner");
    assert_eq!(frames[1]["function"], "user.main");
    assert_eq!(frames[0]["file"], "test.baml");
    assert_eq!(frames[0]["line"], 10, "the println line in `inner`");
    assert_eq!(frames[1]["line"], 19, "the call site in `main`");

    let depth = local(&frames[0], "depth");
    assert_eq!(depth["type"], "int");
    assert_eq!(depth["value"]["kind"], "int");
    assert_eq!(depth["value"]["preview"], "4");
    let note = local(&frames[0], "note");
    assert_eq!(note["value"]["kind"], "string");
    assert_eq!(note["value"]["preview"], "\"at depth 4\"");

    let first = local(&frames[1], "first");
    assert_eq!(first["type"], "Node");
    assert_eq!(first["value"]["kind"], "instance");
    assert_eq!(first["value"]["class"], "Node");
    assert_eq!(child(&first["value"], "label")["preview"], "\"first\"");
    let scores = child(&first["value"], "scores");
    assert_eq!(scores["kind"], "array");
    assert_eq!(child(scores, "[2]")["preview"], "3");
    // first.next.next is `first` again: the cycle is cut.
    assert!(contains_kind(&first["value"], "omitted"), "{first}");

    let lookup = local(&frames[1], "lookup");
    assert_eq!(lookup["value"]["kind"], "map");
    assert_eq!(child(&lookup["value"], "b")["preview"], "2");

    let heap = &dump["heap"];
    assert!(heap["objects"].as_u64().expect("objects") >= 6);
    assert!(heap["bytes"].as_u64().expect("bytes") > 0);
    assert_eq!(heap["by_kind"]["instance"]["count"], 2);
    assert!(
        heap["by_kind"]["string"]["count"]
            .as_u64()
            .expect("strings")
            >= 1
    );
    assert!(heap["by_kind"]["map"]["bytes"].as_u64().expect("map bytes") > 0);
}

#[test]
fn live_dump_and_dump_from_bytes_agree() {
    let harness = Harness::new_unoptimized(PROGRAM);
    let (vm, op, args) = harness.run_to_sysop("user.main", 0);
    let parked = ParkedAt {
        kind: "sysop".to_string(),
        payload: op.clone().into_bytes(),
    };

    let threads = [ThreadInput {
        thread_id: 1,
        parent_thread: None,
        name: "main".to_string(),
        vm: &vm,
        parked: parked.clone(),
        extra_roots: args.clone(),
    }];
    let live = bex_snapshot::state_dump(&vm.heap, &threads, "r-test", 1);
    check_dump(&live);
    assert_eq!(live["threads"][0]["parked"]["detail"], op.as_str());

    let (bytes, _) = harness.write(&vm, parked, args, true, 0).expect("snapshot");
    let from_bytes =
        bex_snapshot::state_dump_from_bytes(&bytes, &harness.program).expect("dump from bytes");
    check_dump(&from_bytes);

    // Same content apart from the timestamp (the header's, for the file).
    assert_eq!(live["threads"], from_bytes["threads"]);
    assert_eq!(live["heap"], from_bytes["heap"]);
    assert_eq!(from_bytes["created_ts"], 1_700_000_000_000u64);
}

#[test]
fn dump_from_bytes_refuses_another_program() {
    let harness = Harness::new_unoptimized(PROGRAM);
    let (vm, _op, args) = harness.run_to_sysop("user.main", 0);
    let (bytes, _) = harness
        .write(&vm, ParkedAt::runnable(), args, false, 0)
        .expect("snapshot");
    let other = common::compile("function main() -> int { 1 }");
    assert!(matches!(
        bex_snapshot::state_dump_from_bytes(&bytes, &other),
        Err(bex_snapshot::SnapshotError::Mismatch(_))
    ));
}
