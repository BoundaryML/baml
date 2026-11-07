//! Narrow, visible exceptions for today's compiled backend only. A new backend
//! does not inherit these except for the explicit C5 GC exception.
//! Never allow process-output differences here.
use serde_json::Value;

pub const GC_REASON: &str = "GC scheduling and heap occupancy differ (especially under BAML_GC_STRESS); only successful root baml.gc maintenance tasks are exempt";

// Runtime-v2 CONTRACTS.md C5: collector maintenance counts/context describe
// the collector, not the program. All other records remain strict.
pub fn without_gc(trace: &str, backend: &str) -> String {
    if !matches!(
        backend,
        "compiled" | "gc-stress" | "vm-frames" | "vm-frames-gc-stress" | "runtime-v2" | "v2"
    ) {
        return trace.to_owned();
    }
    trace
        .lines()
        .filter(|line| {
            let Some((_, json)) = line.split_once(' ') else {
                return true;
            };
            let Ok(record) = serde_json::from_str::<Value>(json) else {
                return true;
            };
            let kind = record[0].as_str().unwrap_or_default();
            let data = &record[1];
            let task = &data["thread_id"];
            let root_gc =
                task["parent"].is_null() && task["kind"] == "task" && task["name"] == "baml.gc";
            let context = &data["context"]["ContextCasId"]["value"];
            let metadata = &context["metadata"];
            let valid_context = context["distinct_id"].is_null()
                && metadata.as_object().is_some_and(|m| m.len() == 7)
                && metadata["collection_level"] == "major"
                && matches!(
                    metadata["trigger"].as_str(),
                    Some("automatic" | "shutdown" | "explicit")
                )
                && [
                    "live_slots",
                    "reclaimed_slots",
                    "promoted_to_gen1",
                    "promoted_to_gen2",
                    "live_payload_bytes",
                ]
                .iter()
                .all(|k| metadata[k].as_u64().is_some());
            !(root_gc
                && (kind == "task"
                    || (kind == "span_tree"
                        && data["children"].as_object().is_some_and(|c| c.is_empty()))
                    || (kind == "ThreadAnnouncement" && valid_context)
                    || (kind == "ThreadCompletion"
                        && valid_context
                        && data["payload"]["outcome"] == 1
                        && data["payload"]["panicked"] == false
                        && data["payload"]["usage"]
                            .as_array()
                            .is_some_and(|u| u.is_empty()))))
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gc_exception_does_not_waive_user_tasks_errors_or_new_backends() {
        let record = |name: &str, outcome: u64| {
            format!(
                "1 {}\n",
                json!(["ThreadCompletion", {
                    "thread_id": {"parent": null, "kind": "task", "name": name},
                    "payload": {"outcome": outcome, "panicked": false, "usage": []},
                "context": {"ContextCasId": {"value": {"distinct_id": null, "metadata": {
                    "collection_level": "major", "trigger": "automatic", "live_slots": 1,
                    "reclaimed_slots": 1, "promoted_to_gen1": 0, "promoted_to_gen2": 1, "live_payload_bytes": 10
                }}}}
                }])
            )
        };
        let gc = record("baml.gc", 1);
        assert!(without_gc(&gc, "gc-stress").is_empty());
        assert!(without_gc(&gc, "runtime-v2").is_empty());
        assert!(without_gc(&gc, "v2").is_empty());
        assert_eq!(without_gc(&gc, "unknown-backend"), gc);
        for retained in [
            record("user.task", 1),
            record("baml.gc", 2),
            record("baml.gc", 3),
        ] {
            assert_eq!(without_gc(&retained, "compiled"), retained);
            assert_eq!(without_gc(&retained, "v2"), retained);
        }
    }
}

pub const POLLING_REASON: &str = "readiness polling executes a scheduler-dependent number of successful 1ms sleeps (Cancellation probes.baml:136, RecursiveCancellation probes.baml:167, CancelDuringUnwind records.baml:401, CancelDeep probes.baml:581, Shielded probes.baml:613)";

/// Remove only the harness readiness poll, then rebuild tree signatures so
/// ancestors still check all their other children. Error/cancellation outcomes
/// at the polling call sites are deliberately not waived.
pub fn without_readiness_poll(trace: &str, probe: &str, backend: &str) -> String {
    use std::collections::{BTreeMap, BTreeSet};
    let line = match probe {
        "Cancellation" => 136,
        "RecursiveCancellation" => 167,
        "CancelDuringUnwind" => 401,
        "CancelDeep" => 581,
        "Shielded" => 613,
        _ => return trace.to_owned(),
    };
    if !matches!(
        backend,
        "compiled" | "gc-stress" | "vm-frames" | "vm-frames-gc-stress"
    ) {
        return trace.to_owned();
    }
    fn paths(value: &Value, out: &mut BTreeMap<String, Value>) {
        match value {
            Value::Object(object) => {
                if object.contains_key("callee")
                    && let Some(key) = value["key"].as_str()
                {
                    out.insert(key.to_owned(), value.clone());
                }
                for value in object.values() {
                    paths(value, out);
                }
            }
            Value::Array(array) => {
                for value in array {
                    paths(value, out);
                }
            }
            _ => {}
        }
    }
    fn references(value: &Value, removed: &BTreeSet<String>) -> bool {
        match value {
            Value::Object(object) => {
                value["key"]
                    .as_str()
                    .is_some_and(|key| removed.contains(key))
                    || object.values().any(|v| references(v, removed))
            }
            Value::Array(array) => array.iter().any(|v| references(v, removed)),
            _ => false,
        }
    }
    trace
        .split("--- recording ---\n")
        .map(|recording| {
            let rows: Vec<(u64, Value)> = recording
                .lines()
                .map(|line| {
                    let (count, value) = line.split_once(' ').unwrap();
                    (count.parse().unwrap(), serde_json::from_str(value).unwrap())
                })
                .collect();
            let mut all_paths = BTreeMap::new();
            for (_, row) in &rows {
                paths(row, &mut all_paths);
            }
            let mut removed: BTreeSet<String> = all_paths
                .iter()
                .filter(|(_, p)| {
                    p["location"]["function"] == format!("user.{probe}")
                        && p["location"]["line"] == line
                })
                .map(|(key, _)| key.clone())
                .collect();
            loop {
                let before = removed.len();
                for (key, p) in &all_paths {
                    if p["parent"]
                        .as_str()
                        .is_some_and(|parent| removed.contains(parent))
                    {
                        removed.insert(key.clone());
                    }
                }
                if before == removed.len() {
                    break;
                }
            }
            let mut trees = BTreeMap::new();
            let mut result = BTreeMap::<String, u64>::new();
            let mut hashes = BTreeMap::<String, Option<String>>::new();
            for (count, row) in rows {
                let kind = row[0].as_str().unwrap();
                if kind == "span_tree" {
                    let key = super::telemetry::fingerprint(&row);
                    if references(&row[1]["thread_id"]["path"], &removed) {
                        hashes.insert(key, None);
                    } else {
                        trees.insert(key, (count, row));
                    }
                } else {
                    let success = match kind {
                        "FunctionAnnouncement" => {
                            row[1]["payload"]["inputs_cas_id"].is_null()
                                && row[1]["payload"]["type_args_cas_id"].is_null()
                        }
                        "FunctionCompletion" => {
                            row[1]["payload"]["value_cas_id"].is_null()
                                && row[1]["payload"]["completion_flags"]
                                    .as_u64()
                                    .is_some_and(|f| f & 3 == 1)
                        }
                        "aggregate.entries" => row[2] == "count" || count == 0,
                        "aggregate.sysop_times" => true,
                        _ => false,
                    };
                    if !(success && references(&row, &removed)) {
                        *result.entry(row.to_string()).or_default() += count;
                    }
                }
            }
            while !trees.is_empty() {
                let ready: Vec<String> = trees
                    .iter()
                    .filter(|(_, (_, row))| {
                        row[1]["children"]
                            .as_object()
                            .unwrap()
                            .keys()
                            .all(|k| hashes.contains_key(k))
                    })
                    .map(|(key, _)| key.clone())
                    .collect();
                assert!(!ready.is_empty(), "unresolved subtree signature");
                for key in ready {
                    let (count, mut row) = trees.remove(&key).unwrap();
                    let mut children = BTreeMap::<String, u64>::new();
                    for (old, count) in row[1]["children"].as_object().unwrap() {
                        if let Some(new) = &hashes[old] {
                            *children.entry(new.clone()).or_default() += count.as_u64().unwrap();
                        }
                    }
                    row[1]["children"] = serde_json::to_value(children).unwrap();
                    hashes.insert(key, Some(super::telemetry::fingerprint(&row)));
                    *result.entry(row.to_string()).or_default() += count;
                }
            }
            result
                .into_iter()
                .map(|(row, count)| format!("{count} {row}\n"))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("--- recording ---\n")
}

pub const LOG_REASON: &str = "compiled log observations use a stale/function-entry PC instead of the log statement location; log values, levels, counts and parents still compare";

pub fn without_log_locations(trace: &str, probe: &str, backend: &str) -> String {
    if !matches!(probe, "Logs" | "LogsDeep")
        || !matches!(
            backend,
            "compiled" | "gc-stress" | "vm-frames" | "vm-frames-gc-stress"
        )
    {
        return trace.to_owned();
    }
    trace
        .split("--- recording ---\n")
        .map(|recording| {
            let mut rows = std::collections::BTreeMap::<String, u64>::new();
            for line in recording.lines() {
                let (count, value) = line.split_once(' ').unwrap();
                let mut row: Value = serde_json::from_str(value).unwrap();
                let known_function = match probe {
                    "Logs" => row[1]["payload"]["function_id"] == "user.Logs",
                    _ => matches!(
                        row[1]["payload"]["function_id"].as_str(),
                        Some("user.LogChain" | ".<lambda(LogChain, 0)>" | "user.LogDefer")
                    ),
                };
                if row[0] == "Log" && known_function {
                    row[1]["payload"]["pc"] =
                        Value::String("known compiled log location gap".into());
                }
                *rows.entry(row.to_string()).or_default() += count.parse::<u64>().unwrap();
            }
            rows.into_iter()
                .map(|(row, count)| format!("{count} {row}\n"))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("--- recording ---\n")
}

/// These probes deliberately test *relative* stack depth (see their BAML
/// comments). Native fiber frames and interpreter frames have different byte
/// sizes, hence different absolute populations and overflow evidence. This is
/// an explicit fixture-wide telemetry gap, not a semantic normalization.
pub fn fixture_exception(probe: &str, level: &str, backend: &str) -> Option<&'static str> {
    if level == "off"
        || !matches!(
            backend,
            "compiled" | "gc-stress" | "vm-frames" | "vm-frames-gc-stress"
        )
    {
        return None;
    }
    match probe {
        "FrameLimit" | "NoLeaks" | "CallbackDepth" | "TaskOverflow" => Some(
            "absolute recursion limit differs with native fiber/VM frame size; span populations, tree depths and overflow frame counts differ (relative-depth process assertions remain enforced)",
        ),
        _ => None,
    }
}

pub const TIMEOUT_REASON: &str = "initializer test runner cancels its losing timeout task; whether the sleep reaches the host wait before cancellation changes its sysop count (testing/registry.baml:609)";

/// Only the cancelled test-runner deadline's host-wait count is exempt. Its
/// function counts, cancellation outcome, task tree and all other sysops remain.
pub fn without_timeout_wait(trace: &str, probe: &str, backend: &str) -> String {
    if !matches!(probe, "InitializerTests" | "InitializerTestErrors")
        || !matches!(
            backend,
            "compiled" | "gc-stress" | "vm-frames" | "vm-frames-gc-stress"
        )
    {
        return trace.to_owned();
    }
    trace
        .lines()
        .filter(|line| {
            let Some((_, json)) = line.split_once(' ') else {
                return true;
            };
            let Ok(row) = serde_json::from_str::<Value>(json) else {
                return true;
            };
            let path = &row[1]["node"][0];
            !(row[0] == "aggregate.sysop_times"
                && path["callee"] == "baml.sys.sleep"
                && path["location"]["file"] == "<builtin>/testing/registry.baml"
                && path["location"]["line"] == 609)
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn log_exception_keeps_values_levels_parents_and_new_backends() {
        let record = |pc, value: &str, level, parent| {
            format!(
                "1 {}\n",
                json!(["Log", {
                    "payload": {"function_id": "user.Logs", "pc": pc, "value": value, "level": level, "parent_id": parent}
                }])
            )
        };
        let a = record(1, "hello", 2, 3);
        let b = record(9, "hello", 2, 3);
        assert_eq!(
            without_log_locations(&a, "Logs", "compiled"),
            without_log_locations(&b, "Logs", "compiled")
        );
        assert_eq!(without_log_locations(&a, "Logs", "runtime-v2"), a);
        for changed in [
            record(1, "changed", 2, 3),
            record(1, "hello", 4, 3),
            record(1, "hello", 2, 5),
        ] {
            assert_ne!(
                without_log_locations(&a, "Logs", "compiled"),
                without_log_locations(&changed, "Logs", "compiled")
            );
        }
    }

    #[test]
    fn readiness_exception_keeps_errors_captures_and_other_calls() {
        let record = |kind: &str, line, flags, capture: Value| {
            format!(
                "1 {}\n",
                json!([kind, {
                    "payload": {"completion_flags": flags, "value_cas_id": capture,
                        "node": [{"key": "poll", "callee": "baml.sys.sleep", "location": {
                            "function": "user.RecursiveCancellation", "line": line}}, 0]}
                }])
            )
        };
        let success = record("FunctionCompletion", 167, 1, Value::Null);
        assert!(without_readiness_poll(&success, "RecursiveCancellation", "compiled").is_empty());
        assert_eq!(
            without_readiness_poll(&success, "RecursiveCancellation", "runtime-v2"),
            success
        );
        for kept in [
            record("FunctionCompletion", 167, 2, Value::Null),
            record("FunctionCompletion", 167, 1, json!({"value": 42})),
            record("FunctionCompletion", 169, 1, Value::Null),
        ] {
            assert_eq!(
                without_readiness_poll(&kept, "RecursiveCancellation", "compiled"),
                kept
            );
        }
    }

    #[test]
    fn readiness_exception_is_scoped_to_the_observed_sites() {
        for (probe, line) in [
            ("Cancellation", 136),
            ("RecursiveCancellation", 167),
            ("CancelDuringUnwind", 401),
            ("CancelDeep", 581),
            ("Shielded", 613),
        ] {
            let row = format!(
                "1 {}\n",
                json!(["aggregate.entries", {"node": [{
                "key": "poll", "callee": "baml.sys.sleep",
                "location": {"function": format!("user.{probe}"), "line": line}
            }, 0]}, "count"])
            );
            assert!(without_readiness_poll(&row, probe, "compiled").is_empty());
            for backend in ["runtime-v2", "v2", "v2-gc-stress"] {
                assert_eq!(without_readiness_poll(&row, probe, backend), row);
            }
        }
    }

    #[test]
    fn timeout_exception_is_only_the_known_host_wait() {
        let record = |kind: &str, line| {
            format!(
                "1 {}\n",
                json!([kind, {"node": [{
            "callee": "baml.sys.sleep", "location": {"file": "<builtin>/testing/registry.baml", "line": line}
        }, 0]}])
            )
        };
        let a = record("aggregate.sysop_times", 609);
        assert!(without_timeout_wait(&a, "InitializerTests", "compiled").is_empty());
        assert_eq!(
            without_timeout_wait(&a, "InitializerTests", "runtime-v2"),
            a
        );
        assert_eq!(without_timeout_wait(&a, "OtherProbe", "compiled"), a);
        for retained in [
            record("aggregate.entries", 609),
            record("aggregate.sysop_times", 610),
        ] {
            assert_eq!(
                without_timeout_wait(&retained, "InitializerTests", "compiled"),
                retained
            );
        }
        assert!(fixture_exception("FrameLimit", "high", "runtime-v2").is_none());
        assert!(fixture_exception("FrameLimit", "off", "compiled").is_none());
    }
}
