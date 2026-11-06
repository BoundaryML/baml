//! Semantic BTEL comparison. Files/batches and cross-task scheduling order are
//! transport details; record multiplicity and all non-clock payloads are not.
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use btel_reader::{cas::CasStore, value};
use prost::Message;
use serde_json::{Value, json};

#[allow(dead_code, clippy::all)]
mod wire {
    include!(concat!(env!("OUT_DIR"), "/baml.btel.recording.v2.rs"));
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Node {
    parent: u64,
    path: u32,
    task: bool,
    name: Option<String>,
    manual: bool,
}

#[derive(Default)]
struct Names {
    functions: BTreeMap<u64, Value>,
    metadata: BTreeMap<u64, Value>,
    paths: BTreeMap<u64, Value>,
    nodes: BTreeMap<u64, Node>,
    raises: BTreeMap<u64, Value>,
    captures: std::cell::RefCell<BTreeMap<[u8; 16], Value>>,
    path_names: std::cell::RefCell<BTreeMap<u64, Value>>,
    node_names: std::cell::RefCell<HashMap<u64, u64>>,
    node_patterns: std::cell::RefCell<HashMap<Node, u64>>,
    node_labels: std::cell::RefCell<HashMap<u64, Value>>,
}

fn number(v: &Value, key: &str) -> u64 {
    v[key].as_u64().unwrap_or(0)
}

impl Names {
    fn function(&self, id: u64) -> Value {
        if id == 0 {
            return Value::Null;
        }
        self.functions
            .get(&id)
            .unwrap_or_else(|| panic!("unresolved function {id}"))
            .clone()
    }

    fn location(&self, function: u64, pc: u64) -> Value {
        if let Some(metadata) = self.metadata.get(&function) {
            let map = &metadata["source_map"];
            if let Some(pcs) = map["pc"].as_array() {
                let index = pcs.partition_point(|p| p.as_u64().unwrap() <= pc);
                if index > 0 && pc < number(map, "code_bytes") {
                    let i = index - 1;
                    return json!({"function": self.function(function), "file": metadata["source_file"],
                        "line": map["line"][i], "start": map["start"][i], "end": map["end"][i]});
                }
            }
        }
        json!({"function": self.function(function), "unmapped_pc": pc})
    }

    fn path(&self, id: u64, _depth: usize) -> Value {
        if id == 0 {
            return Value::Null;
        }
        let mut cache = self.path_names.borrow_mut();
        let mut pending = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut cursor = id;
        while cursor != 0 && !cache.contains_key(&cursor) {
            assert!(seen.insert(cursor), "cyclic call path");
            let p = self
                .paths
                .get(&cursor)
                .unwrap_or_else(|| panic!("unresolved path {cursor}"));
            pending.push(cursor);
            cursor = number(p, "parent_call_path_id");
        }
        for id in pending.into_iter().rev() {
            let p = &self.paths[&id];
            let parent = cache
                .get(&number(p, "parent_call_path_id"))
                .map(|v| v["key"].clone());
            let descriptor = json!({"parent": parent, "callee": self.function(number(p, "callee_function_id")),
                "caller": self.function(number(p, "visible_caller_function_id")), "location": self.location(number(p, "visible_caller_function_id"), number(p, "caller_pc")), "edge": p["edge"]});
            let mut result = descriptor.clone();
            result["key"] = json!(fingerprint(&descriptor));
            cache.insert(id, result);
        }
        cache[&id].clone()
    }

    fn node_rep(&self, id: u64) -> u64 {
        if id == 0 {
            return 0;
        }
        let mut cache = self.node_names.borrow_mut();
        if let Some(rep) = cache.get(&id) {
            return *rep;
        }
        let mut pending = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut cursor = id;
        while cursor != 0 && !cache.contains_key(&cursor) {
            assert!(seen.insert(cursor), "cyclic span parents");
            let n = self
                .nodes
                .get(&cursor)
                .unwrap_or_else(|| panic!("unresolved node {cursor}"));
            pending.push(cursor);
            cursor = n.parent;
        }
        let mut patterns = self.node_patterns.borrow_mut();
        let mut labels = self.node_labels.borrow_mut();
        for id in pending.into_iter().rev() {
            let mut n = self.nodes[&id].clone();
            n.parent = cache.get(&n.parent).copied().unwrap_or(0);
            let rep = *patterns.entry(n.clone()).or_insert_with(|| {
                let parent = labels.get(&n.parent).map(|v| v["key"].clone());
                let descriptor = json!({"parent": parent, "kind": if n.task { "task" } else { "span" },
                    "path": self.path(u64::from(n.path), 0), "name": n.name,
                    "manual": if n.task { Value::Null } else { json!(n.manual) }});
                let mut result = descriptor.clone();
                result["key"] = json!(fingerprint(&descriptor));
                labels.insert(id, result);
                id
            });
            cache.insert(id, rep);
        }
        cache[&id]
    }

    fn node(&self, id: u64, _depth: usize) -> Value {
        if id == 0 {
            return Value::Null;
        }
        let rep = self.node_rep(id);
        self.node_labels.borrow()[&rep].clone()
    }

    // Bottom-up unordered tree signatures preserve sibling distribution without
    // recursive Rust/JSON stacks, including stack-overflow probes.
    fn tree(
        &self,
        id: u64,
        children: &BTreeMap<u64, Vec<u64>>,
        counts: &mut BTreeMap<String, u64>,
        _depth: usize,
    ) -> String {
        let mut pending = vec![(id, false)];
        let mut active = std::collections::HashSet::new();
        let mut hashes = HashMap::<u64, usize>::new();
        let mut patterns = BTreeMap::<(u64, Vec<(usize, u64)>), usize>::new();
        let mut signatures = Vec::<(String, String, u64)>::new();
        while let Some((node, visited)) = pending.pop() {
            if !visited {
                assert!(active.insert(node), "cyclic span tree");
                pending.push((node, true));
                pending.extend(
                    children
                        .get(&node)
                        .into_iter()
                        .flatten()
                        .map(|child| (*child, false)),
                );
                continue;
            }
            let mut descendants = BTreeMap::<usize, u64>::new();
            for child in children.get(&node).into_iter().flatten() {
                *descendants
                    .entry(hashes.remove(child).unwrap())
                    .or_default() += 1;
            }
            let key = (
                self.node_rep(node),
                descendants.into_iter().collect::<Vec<_>>(),
            );
            let signature = *patterns.entry(key.clone()).or_insert_with(|| {
                let mut descendants = BTreeMap::<String, u64>::new();
                for (id, count) in &key.1 {
                    *descendants.entry(signatures[*id].0.clone()).or_default() += count;
                }
                let descriptor = json!(["span_tree", {"thread_id": self.node(node, 0), "children": descendants}]);
                let index = signatures.len();
                signatures.push((fingerprint(&descriptor), descriptor.to_string(), 0));
                index
            });
            signatures[signature].2 += 1;
            hashes.insert(node, signature);
            active.remove(&node);
        }
        let root = signatures[hashes[&id]].0.clone();
        for (_, row, count) in signatures {
            *counts.entry(row).or_default() += count;
        }
        root
    }

    fn raise(&self, id: u64) -> Value {
        if id == 0 {
            return Value::Null;
        }
        let r = self
            .raises
            .get(&id)
            .unwrap_or_else(|| panic!("unresolved raise {id}"));
        json!([
            self.node(number(r, "thread_id"), 0),
            self.function(number(r, "function_id")),
            self.location(number(r, "function_id"), number(r, "pc")),
            r["kind"]
        ])
    }

    fn normalize(&self, v: &Value, cas: &CasStore) -> Value {
        match v {
            Value::Array(a) => Value::Array(a.iter().map(|v| self.normalize(v, cas)).collect()),
            Value::Object(o) => {
                let mut out = serde_json::Map::new();
                for (k, v) in o {
                    // Only clock/transport identity is discarded. In particular,
                    // PCs, outcomes, frame counts, capture limits and flags stay.
                    if k.ends_with("_ticks") || k.ends_with("_unix_ns") || k == "clock_epoch_id" {
                        continue;
                    }
                    let id = v.as_u64().unwrap_or(0);
                    let normalized = if k.ends_with("_cas_id") || k == "ContextCasId" {
                        capture(v, cas, &self.captures)
                    } else if v.is_null() {
                        Value::Null
                    } else {
                        match k.as_str() {
                            "function_id"
                            | "handler_function_id"
                            | "visible_caller_function_id"
                            | "callee_function_id" => self.function(id),
                            "call_path_id" | "parent_call_path_id" | "spawn_call_path_id" => {
                                self.path(id, 0)
                            }
                            "pc" | "handler_pc" => {
                                let field = if k == "handler_pc" {
                                    "handler_function_id"
                                } else {
                                    "function_id"
                                };
                                self.location(o.get(field).and_then(Value::as_u64).unwrap_or(0), id)
                            }
                            "node" => json!([self.path(id >> 1, 0), id & 1]),
                            "id" | "thread_id" | "parent_id" | "call_id" | "span_id" => {
                                self.node(id, 0)
                            }
                            "raise_id" | "origin_raise_id" | "previous_raise_id" => self.raise(id),
                            _ => self.normalize(v, cas),
                        }
                    };
                    out.insert(k.clone(), normalized);
                }
                Value::Object(out)
            }
            _ => v.clone(),
        }
    }
}

pub(super) fn fingerprint(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

fn capture(
    v: &Value,
    cas: &CasStore,
    cache: &std::cell::RefCell<BTreeMap<[u8; 16], Value>>,
) -> Value {
    if v.is_null() {
        return Value::Null;
    }
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&number(v, "low").to_le_bytes());
    bytes[8..].copy_from_slice(&number(v, "high").to_le_bytes());
    if let Some(value) = cache.borrow().get(&bytes) {
        return value.clone();
    }
    let id = btel_snapshot::CasId::from_bytes(bytes);
    let snapshot = cas
        .load(id)
        .snapshot
        .expect("captured CAS must be readable");
    // A content digest covers exact bytes, types, aliases and cycles, including
    // media that the human-readable renderer intentionally summarizes. CAS IDs
    // are content identities, independent of runtime IDs/heap addresses.
    let mut pending = snapshot.children.clone();
    let mut visited = std::collections::BTreeSet::new();
    while let Some(child) = pending.pop() {
        if !visited.insert(child) {
            continue;
        }
        assert!(visited.len() <= 100_000, "capture graph too large");
        pending.extend(
            cas.load(child)
                .snapshot
                .expect("capture child must be readable")
                .children
                .iter()
                .copied(),
        );
    }
    let limits = value::RenderLimits::default();
    let rendered = match &snapshot.root {
        btel_snapshot::DecodedRoot::FunctionArgs { .. } => {
            value::render_arguments(cas, &snapshot, None, &limits)
        }
        _ => match value::navigate(
            cas,
            &snapshot,
            value::Root::Value,
            None,
            &[],
            limits.max_blobs,
        ) {
            value::Nav::Value(found) => value::render_value(cas, &found, &limits),
            other => panic!("cannot navigate capture: {other:?}"),
        },
    };
    assert!(!rendered.cut, "comparison render limit exceeded");
    // Producer truncation is evidence, retained in JSON; missing CAS is not.
    if let Some(value::Unavailable::Blob(code)) = rendered.incomplete {
        panic!("capture: {code}");
    }
    let result = json!({"content_digest": bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(), "value": rendered.json});
    cache.borrow_mut().insert(bytes, result.clone());
    result
}

/// Two passes: index compact identities, then fold wire events before rendering.
/// A recursive high-level recording can contain millions of identical spans;
/// retaining JSON for each event would dwarf the packed program's memory.
pub fn read(home: &Path) -> String {
    use wire::span_event::Event;
    let root = home.join("btel");
    let layout = btel_reader::layout::SourceLayout {
        recordings: root.join("recordings"),
        cas: root.join("cas"),
        root,
    };
    let discovery = btel_reader::discovery::discover(&layout).unwrap();
    let cas = CasStore::new(layout.cas.clone(), Default::default());
    let mut recordings = Vec::new();
    for recording in discovery.recordings {
        assert_eq!(recording.partial_files, 0, "incomplete recording");
        let mut names = Names::default();
        let mut tasks = BTreeMap::new();
        let mut ended = false;
        let mut header = None;
        for (index, file) in recording.files.iter().enumerate() {
            assert_eq!(file.sequence, index as u64 + 1, "missing recording file");
            assert!(!ended, "file after recording end");
            btel_reader::file::read_file(&file.path, recording.id, file.sequence, 256 << 20)
                .unwrap();
            let file = decode(&file.path);
            if index == 0 {
                let h = file.header.as_ref().expect("recording header");
                header = Some(
                    json!({"format_major": h.format_major, "format_minor": h.format_minor,
                    "baml_version": h.baml_version, "host": h.host, "initial_context_cas_id": h.initial_context_cas_id}),
                );
            }
            if let Some(defs) = file.definitions {
                for fun in defs.functions {
                    if let Some(wire::function_definition::Resolution::Metadata(metadata)) =
                        fun.resolution
                    {
                        names.functions.insert(fun.function_id, json!(metadata.fqn));
                        names
                            .metadata
                            .insert(fun.function_id, serde_json::to_value(metadata).unwrap());
                    } else {
                        names
                            .functions
                            .entry(fun.function_id)
                            .or_insert_with(|| json!({"metadata_unavailable": true}));
                    }
                }
                for path in defs.call_paths {
                    names.paths.insert(
                        u64::from(path.call_path_id),
                        serde_json::to_value(path).unwrap(),
                    );
                }
                for t in defs.threads {
                    names.nodes.insert(
                        t.thread_id,
                        Node {
                            parent: t.parent_id.unwrap_or(0),
                            path: t.spawn_call_path_id,
                            task: true,
                            name: t.name.clone(),
                            manual: false,
                        },
                    );
                    tasks.insert(t.thread_id, serde_json::to_value(t).unwrap());
                }
            }
            if let Some(spans) = file.spans {
                for section in spans.sections {
                    for event in section.events {
                        match event.event.expect("span event") {
                            Event::FunctionAnnouncement(e) => {
                                names.nodes.insert(
                                    e.id,
                                    Node {
                                        parent: e.parent_id,
                                        path: e.call_path_id,
                                        task: false,
                                        name: None,
                                        manual: e.manual,
                                    },
                                );
                            }
                            Event::FunctionCompletion(e) | Event::LateFunctionCompletion(e) => {
                                names.nodes.entry(e.id).or_insert(Node {
                                    parent: e.parent_id,
                                    path: (e.node >> 1) as u32,
                                    task: false,
                                    name: None,
                                    manual: e.completion_flags & 32 != 0,
                                });
                            }
                            Event::NetworkAnnouncement(e) => {
                                names.nodes.insert(
                                    e.id,
                                    Node {
                                        parent: e.parent_id,
                                        path: e.call_path_id,
                                        task: false,
                                        name: None,
                                        manual: false,
                                    },
                                );
                            }
                            Event::ThreadAnnouncement(_)
                            | Event::ThreadCompletion(_)
                            | Event::ThreadRunning(_)
                            | Event::Log(_)
                            | Event::NetworkEvent(_)
                            | Event::NetworkCompletion(_) => {}
                        }
                    }
                }
            }
            if let Some(errors) = file.errors {
                for raise in errors.raises {
                    names
                        .raises
                        .insert(raise.raise_id, serde_json::to_value(raise).unwrap());
                }
            }
            ended = file.end.is_some();
        }
        assert!(ended, "recording missing final seal");
        let mut counts = BTreeMap::<String, u64>::new();
        add(
            &names,
            &cas,
            &mut counts,
            "header",
            header.expect("recording header"),
            1,
        );
        let mut children = BTreeMap::<u64, Vec<u64>>::new();
        for (id, node) in &names.nodes {
            children.entry(node.parent).or_default().push(*id);
        }
        for root in children.get(&0).into_iter().flatten() {
            names.tree(*root, &children, &mut counts, 0);
        }
        assert_eq!(
            names.node_names.borrow().len(),
            names.nodes.len(),
            "unrooted span nodes"
        );
        drop(children);
        names.nodes.clear();
        for task in tasks.into_values() {
            add(&names, &cas, &mut counts, "task", task, 1);
        }

        let mut events = HashMap::<Vec<u8>, u64>::new();
        for file in &recording.files {
            let file = decode(&file.path);
            if let Some(spans) = file.spans {
                for section in spans.sections {
                    let thread = names.node_rep(section.thread_id);
                    for mut event in section.events {
                        fold_event(&names, &mut event);
                        let section = wire::ThreadSection {
                            thread_id: thread,
                            events: vec![event],
                            context: section.context,
                        };
                        *events.entry(section.encode_to_vec()).or_default() += 1;
                    }
                }
            }
            if let Some(aggregates) = file.aggregates {
                for entry in aggregates.entries {
                    add(
                        &names,
                        &cas,
                        &mut counts,
                        "aggregate.entries",
                        serde_json::to_value(entry).unwrap(),
                        1,
                    );
                }
                for entry in aggregates.sysop_times {
                    add(
                        &names,
                        &cas,
                        &mut counts,
                        "aggregate.sysop_times",
                        serde_json::to_value(entry).unwrap(),
                        1,
                    );
                }
            }
            if let Some(errors) = file.errors {
                let value = serde_json::to_value(errors).unwrap();
                for (kind, entries) in value.as_object().unwrap() {
                    for entry in entries.as_array().unwrap() {
                        add(
                            &names,
                            &cas,
                            &mut counts,
                            &format!("error.{kind}"),
                            entry.clone(),
                            1,
                        );
                    }
                }
            }
            if let Some(end) = file.end {
                add(
                    &names,
                    &cas,
                    &mut counts,
                    "end",
                    serde_json::to_value(end).unwrap(),
                    1,
                );
            }
        }
        for (encoded, count) in events {
            let section = wire::ThreadSection::decode(encoded.as_slice()).unwrap();
            let section = serde_json::to_value(section).unwrap();
            let (kind, payload) = section["events"][0]["event"]
                .as_object()
                .unwrap()
                .iter()
                .next()
                .unwrap();
            add(
                &names,
                &cas,
                &mut counts,
                kind,
                json!({"thread_id": section["thread_id"], "context": section["context"], "payload": payload}),
                count,
            );
        }
        recordings.push(
            counts
                .into_iter()
                .map(|(row, count)| format!("{count} {row}\n"))
                .collect::<String>(),
        );
    }
    recordings.sort();
    recordings.join("--- recording ---\n")
}

fn decode(path: &Path) -> wire::RecordingFile {
    wire::RecordingFile::decode(std::fs::read(path).unwrap().as_slice()).unwrap()
}

fn fold_event(names: &Names, event: &mut wire::SpanEvent) {
    use wire::span_event::Event;
    match event.event.as_mut().unwrap() {
        Event::ThreadAnnouncement(_) => {}
        Event::ThreadCompletion(e) => e.completed_at_ticks = 0,
        Event::ThreadRunning(e) => e.at_ticks = 0,
        Event::FunctionAnnouncement(e) => {
            e.id = names.node_rep(e.id);
            e.parent_id = names.node_rep(e.parent_id);
            e.entered_at_ticks = 0;
        }
        Event::FunctionCompletion(e) | Event::LateFunctionCompletion(e) => {
            e.id = names.node_rep(e.id);
            e.parent_id = names.node_rep(e.parent_id);
            e.entered_at_ticks = 0;
            e.exited_at_ticks = 0;
            e.self_await_ticks = 0;
        }
        Event::Log(e) => {
            e.parent_id = names.node_rep(e.parent_id);
            e.at_ticks = 0;
        }
        Event::NetworkAnnouncement(e) => {
            e.id = names.node_rep(e.id);
            e.parent_id = names.node_rep(e.parent_id);
            e.started_at_ticks = 0;
        }
        Event::NetworkEvent(e) => {
            e.span_id = names.node_rep(e.span_id);
            e.at_ticks = 0;
        }
        Event::NetworkCompletion(e) => {
            e.span_id = names.node_rep(e.span_id);
            e.completed_at_ticks = 0;
        }
    }
}

fn add(
    names: &Names,
    cas: &CasStore,
    counts: &mut BTreeMap<String, u64>,
    kind: &str,
    record: Value,
    multiplicity: u64,
) {
    let mut normalized = names.normalize(&record, cas);
    if kind == "aggregate.entries" {
        let count = number(&normalized, "count");
        let outcomes = normalized
            .as_object_mut()
            .unwrap()
            .remove("outcomes")
            .unwrap();
        normalized.as_object_mut().unwrap().remove("count");
        *counts
            .entry(json!([kind, normalized, "count"]).to_string())
            .or_default() += count;
        if outcomes.is_null() {
            *counts
                .entry(json!([kind, normalized, "unknown_outcomes"]).to_string())
                .or_default() += count;
        } else {
            for key in ["errored", "cancelled", "panicked"] {
                let v = &outcomes[key];
                *counts
                    .entry(
                        json!([
                            kind,
                            normalized,
                            key,
                            if v.is_null() { "unknown" } else { "known" }
                        ])
                        .to_string(),
                    )
                    .or_default() += v.as_u64().unwrap_or(0);
            }
        }
    } else if kind == "aggregate.sysop_times" {
        let count = number(&normalized, "sysops");
        normalized.as_object_mut().unwrap().remove("sysops");
        *counts
            .entry(json!([kind, normalized]).to_string())
            .or_default() += count;
    } else {
        *counts
            .entry(json!([kind, normalized]).to_string())
            .or_default() += multiplicity;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(offset: u64) -> Names {
        Names {
            functions: [(1 + offset, json!("main")), (2 + offset, json!("child"))].into(),
            paths: [(1 + offset, json!({"parent_call_path_id": 0, "callee_function_id": 1 + offset, "caller_pc": 0, "edge": 1})),
                (2 + offset, json!({"parent_call_path_id": 1 + offset, "callee_function_id": 2 + offset, "caller_pc": 12, "edge": 1}))].into(),
            nodes: [(1 + offset, Node { parent: 0, path: 0, task: true, name: Some("root".into()), manual: false }),
                (2 + offset, Node { parent: 1 + offset, path: (2 + offset) as u32, task: false, name: None, manual: false })].into(),
            ..Default::default()
        }
    }

    #[test]
    fn sealed_files_are_required_and_end_outcomes_are_compared() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(read(home.path()), "");
        let directory = home.path().join("btel/recordings").join("07".repeat(16));
        std::fs::create_dir_all(&directory).unwrap();
        let mut file = wire::RecordingFile {
            header: Some(wire::RecordingHeader {
                format_major: 2,
                format_minor: 13,
                recording_id: vec![7; 16],
                ..Default::default()
            }),
            sequence: 1,
            end: Some(wire::RecordingEnd {
                process_end: Some(wire::ProcessEnd {
                    status: 1,
                    at_unix_ns: 123,
                }),
            }),
            ..Default::default()
        };
        let path = directory.join("00000000000000000001.btel");
        std::fs::write(&path, file.encode_to_vec()).unwrap();
        let success = read(home.path());
        file.end
            .as_mut()
            .unwrap()
            .process_end
            .as_mut()
            .unwrap()
            .status = 2;
        std::fs::write(&path, file.encode_to_vec()).unwrap();
        assert_ne!(success, read(home.path()));
        file.end = None;
        std::fs::write(&path, file.encode_to_vec()).unwrap();
        assert!(std::panic::catch_unwind(|| read(home.path())).is_err());
    }

    #[test]
    fn wire_folding_keeps_outcomes_and_captures() {
        let n = names(0);
        let event = wire::span_event::Event::FunctionCompletion(wire::FunctionCompletion {
            id: 2,
            parent_id: 1,
            node: 4,
            entered_at_ticks: 10,
            exited_at_ticks: 20,
            completion_flags: 1,
            value_cas_id: Some(wire::CasId { low: 42, high: 0 }),
            ..Default::default()
        });
        let mut a = wire::SpanEvent { event: Some(event) };
        let mut b = a.clone();
        if let Some(wire::span_event::Event::FunctionCompletion(e)) = &mut b.event {
            e.exited_at_ticks = 99;
        }
        fold_event(&n, &mut a);
        fold_event(&n, &mut b);
        assert_eq!(a, b);
        if let Some(wire::span_event::Event::FunctionCompletion(e)) = &mut b.event {
            e.completion_flags = 2;
        }
        assert_ne!(a, b);
        if let Some(wire::span_event::Event::FunctionCompletion(e)) = &mut b.event {
            e.completion_flags = 1;
            e.value_cas_id = None;
        }
        assert_ne!(a, b);
    }

    #[test]
    fn ids_and_clocks_are_not_semantics() {
        let temp = tempfile::tempdir().unwrap();
        let cas = CasStore::new(temp.path().into(), Default::default());
        let a = json!({"id": 2, "parent_id": 1, "node": 4, "exited_at_ticks": 123, "completion_flags": 1});
        let b = json!({"id": 102, "parent_id": 101, "node": 204, "exited_at_ticks": 987, "completion_flags": 1});
        assert_eq!(names(0).normalize(&a, &cas), names(100).normalize(&b, &cas));
    }

    #[test]
    fn equivalent_path_ids_preserve_sibling_multiplicity() {
        let mut n = names(0);
        n.paths.insert(3, n.paths[&2].clone());
        n.nodes.insert(
            3,
            Node {
                parent: 1,
                path: 3,
                task: false,
                name: None,
                manual: false,
            },
        );
        let mut rows = BTreeMap::new();
        n.tree(1, &[(1, vec![2, 3])].into(), &mut rows, 0);
        let root = rows
            .keys()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .find(|r| r[1]["thread_id"]["kind"] == "task")
            .unwrap();
        assert_eq!(
            root[1]["children"]
                .as_object()
                .unwrap()
                .values()
                .map(|v| v.as_u64().unwrap())
                .sum::<u64>(),
            2
        );
    }

    #[test]
    fn sibling_distribution_is_part_of_the_span_tree() {
        let mut n = names(0);
        for (id, parent) in [(3, 1), (4, 2), (5, 2)] {
            n.nodes.insert(
                id,
                Node {
                    parent,
                    path: 2,
                    task: false,
                    name: None,
                    manual: false,
                },
            );
        }
        let mut a = BTreeMap::new();
        n.tree(1, &[(1, vec![2, 3]), (2, vec![4, 5])].into(), &mut a, 0);
        n.nodes.get_mut(&5).unwrap().parent = 3;
        n.node_names.borrow_mut().clear();
        n.node_patterns.borrow_mut().clear();
        n.node_labels.borrow_mut().clear();
        let mut b = BTreeMap::new();
        n.tree(
            1,
            &[(1, vec![2, 3]), (2, vec![4]), (3, vec![5])].into(),
            &mut b,
            0,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn paths_outcomes_counts_and_captured_data_remain_visible() {
        let temp = tempfile::tempdir().unwrap();
        let cas = CasStore::new(temp.path().into(), Default::default());
        let names = names(0);
        let a = json!({"node": 4, "count": 3, "outcomes": {"errored": 1}, "payload": {"value": "hello"}});
        for b in [
            json!({"node": 2, "count": 3, "outcomes": {"errored": 1}, "payload": {"value": "hello"}}),
            json!({"node": 4, "count": 4, "outcomes": {"errored": 1}, "payload": {"value": "hello"}}),
            json!({"node": 4, "count": 3, "outcomes": {"errored": 0}, "payload": {"value": "hello"}}),
            json!({"node": 4, "count": 3, "outcomes": {"errored": 1}, "payload": {"value": "bye"}}),
        ] {
            assert_ne!(names.normalize(&a, &cas), names.normalize(&b, &cas));
        }
    }
}
