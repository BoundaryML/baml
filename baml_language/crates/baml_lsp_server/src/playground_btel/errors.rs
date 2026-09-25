//! Throw entries from recorded raises. One entry per occurrence (a fresh
//! raise), with the raises proven to pass it along as its propagation. A
//! raise whose origin is ambiguous or unresolved gets its own entry that says
//! so; it is never folded into an occurrence it might not belong to.
use std::collections::HashMap;

use serde_json::{Value, json};

/// Where the raise came from, in the UI's existing vocabulary.
fn source(kind: &str) -> &'static str {
    match kind {
        "native_boundary" => "native_call",
        "host_boundary" => "engine_call",
        "await" | "await_cancelled" => "future_resume",
        _ => "bytecode",
    }
}

fn site(raise: &[Value]) -> Value {
    json!({
        "state": raise[10],
        "file": raise[11],
        "line": raise[12],
        "start": raise[13],
        "end": raise[14],
    })
}

/// Root-to-throw function names from a raise's frames (stored innermost first).
fn stack(frames: Option<&Vec<&Vec<Value>>>) -> Vec<Value> {
    frames
        .map(|frames| {
            frames
                .iter()
                .rev()
                .map(|frame| match (&frame[2], frame[3].as_i64()) {
                    (Value::String(fqn), _) => Value::from(fqn.as_str()),
                    (_, Some(1)) => Value::from("<native>"),
                    _ => Value::from("<unknown function>"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn step(raise: &[Value]) -> Value {
    json!({
        "raiseId": raise[0],
        "kind": raise[2],
        "fqn": raise[8],
        "threadId": raise[1],
        "site": site(raise),
        "result": raise[15],
        "handlerFqn": raise[16],
    })
}

pub(super) fn occurrences(
    raises: &[Vec<Value>],
    frames: &[Vec<Value>],
    calls: &[Value],
) -> Vec<Value> {
    let mut stacks: HashMap<&str, Vec<&Vec<Value>>> = HashMap::new();
    for frame in frames {
        if let Some(raise) = frame[0].as_str() {
            stacks.entry(raise).or_default().push(frame);
        }
    }
    let path_of: HashMap<&str, &Value> = calls
        .iter()
        .filter_map(|call| Some((call["callId"].as_str()?, &call["callPathId"])))
        .collect();
    let mut propagation: HashMap<&str, Vec<Value>> = HashMap::new();
    for raise in raises {
        if raise[4] == "proven"
            && let Some(occurrence) = raise[3].as_str()
        {
            propagation.entry(occurrence).or_default().push(step(raise));
        }
    }
    let fresh: std::collections::HashSet<&str> = raises
        .iter()
        .filter(|raise| raise[4] == "fresh")
        .filter_map(|raise| raise[0].as_str())
        .collect();
    let mut entries = Vec::new();
    for raise in raises {
        let (Some(id), Some(origin)) = (raise[0].as_str(), raise[4].as_str()) else {
            continue;
        };
        // A proven raise joins its occurrence, when that occurrence is here.
        if origin == "proven" && raise[3].as_str().is_some_and(|o| fresh.contains(o)) {
            continue;
        }
        let key = if origin == "proven" {
            raise[3].as_str().unwrap_or(id)
        } else {
            id
        };
        // Without a proven occurrence no call can be attributed to it.
        let linked = origin == "fresh" || origin == "proven";
        let failed: Vec<&Value> = calls
            .iter()
            .filter(|call| linked && call["errorOccurrenceId"].as_str() == Some(key))
            .map(|call| &call["callId"])
            .collect();
        // The value any failed call captured for this occurrence.
        let valued = calls.iter().find(|call| {
            linked
                && call["errorOccurrenceId"].as_str() == Some(key)
                && call["errorState"] == "available"
        });
        let kind = raise[2].as_str().unwrap_or("");
        let inherited: Vec<Value> = raise[20]
            .as_str()
            .and_then(|text| serde_json::from_str::<Vec<Value>>(text).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|frame| frame["function"].clone())
            .collect();
        entries.push(json!({
            "errorId": key,
            "grain": "throw",
            "throwCallId": raise[9],
            "throwThreadId": raise[1],
            "throwCallPathId": raise[9].as_str().and_then(|call| path_of.get(call).copied()),
            "throwFqn": raise[8],
            "throwSiteFile": raise[11],
            "throwSiteLine": raise[12],
            "throwSiteStart": raise[13],
            "throwSiteEnd": raise[14],
            "throwSiteState": raise[10],
            // `fresh` starts an occurrence; anything else passed one along.
            "kind": if origin == "fresh" { "fresh" } else { "rethrow" },
            "raiseKind": kind,
            "source": source(kind),
            "originState": origin,
            "originVia": raise[5],
            "originCandidates": raise[6],
            "unresolvedReason": raise[7],
            "unwindResult": raise[15],
            "handlerFqn": raise[16],
            "propagation": if origin == "fresh" {
                propagation.remove(id).unwrap_or_default()
            } else {
                Vec::new()
            },
            "failedCallIds": failed,
            "valueState": valued.map_or(Value::from("not_captured"), |call| call["errorState"].clone()),
            "valueCid": valued.map_or(Value::Null, |call| call["errorCid"].clone()),
            "value": valued.map_or(Value::Null, |call| call["error"].clone()),
            "stackComplete": raise[19] == "complete",
            "stack": stack(stacks.get(id)),
            "inheritedStack": inherited,
        }));
    }
    entries
}
