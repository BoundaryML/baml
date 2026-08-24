use std::{error::Error, fs, io, path::Path};

use bex_events::{
    ids::BoundaryId,
    prof::{
        backend::{
            BoundaryEndStatus, BoundaryHealthSnapshot, CounterHealth, DurableRunReader, EdgeKind,
            ProfileRun, RoleMask, ValueState,
        },
        record::FunctionEndStatus,
    },
};
use serde_json::json;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let store_root = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing store root"))?;
    let scenario = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing scenario"))?;
    let tasks = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing task count"))?
        .parse::<u64>()?;
    let iterations = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing iteration count"))?
        .parse::<u64>()?;
    if arguments.next().is_some() || !matches!(scenario.as_str(), "baseline" | "stress") {
        return Err(io::Error::other(
            "usage: profiling_e2e_verify STORE_ROOT baseline|stress TASKS ITERATIONS",
        )
        .into());
    }

    let expected_kernel_calls = tasks
        .checked_mul(iterations)
        .ok_or("kernel call count overflow")?;
    let expected_root_calls = 1;
    let (
        expected_contexts,
        expected_total_calls,
        expected_call_edges,
        expected_spawn_edges,
        expected_awaits_min,
        expected_awaits_max,
        expected_nonroot_contexts,
    ) = if scenario == "baseline" {
        let total = 1_u64
            .checked_add(tasks.checked_mul(2).ok_or("call count overflow")?)
            .and_then(|value| value.checked_add(expected_kernel_calls))
            .ok_or("call count overflow")?;
        (
            1_u64
                .checked_add(tasks.checked_mul(3).ok_or("context count overflow")?)
                .ok_or("context count overflow")?,
            total,
            total - 1,
            0,
            0,
            0,
            tasks * 3,
        )
    } else {
        (
            6,
            1_u64
                .checked_add(tasks.checked_mul(4).ok_or("call count overflow")?)
                .and_then(|value| value.checked_add(expected_kernel_calls))
                .ok_or("call count overflow")?,
            tasks
                .checked_mul(3)
                .ok_or("call edge count overflow")?
                .checked_add(expected_kernel_calls)
                .ok_or("call edge count overflow")?,
            tasks,
            tasks,
            tasks.checked_mul(2).ok_or("await count overflow")?,
            5,
        )
    };

    let boundary_ids = boundaries(Path::new(&store_root))?;
    // The JSON argument/output helpers run as suppressed internal roots, so a
    // packed invocation publishes exactly one durable run: the workload.
    assert_eq!(
        boundary_ids.len(),
        1,
        "packed execution should produce exactly one (workload) boundary"
    );
    let mut runs = Vec::with_capacity(boundary_ids.len());
    for boundary_id in boundary_ids {
        let reader = DurableRunReader::open(&store_root, boundary_id)?;
        let run = reader.load()?;
        let counts = assert_run_integrity(&reader, &run)?;
        runs.push((boundary_id, run, counts));
    }

    let workload_index = runs.iter().position(|(_, run, counts)| {
        u64::try_from(run.contexts.len()).ok() == Some(expected_contexts)
            && counts.invocations.iter().sum::<u64>() == expected_total_calls
    });
    let Some(workload_index) = workload_index else {
        let observed = runs
            .iter()
            .map(|(boundary_id, run, counts)| {
                format!(
                    "{}: contexts={}, counts={counts:?}, health={:?}",
                    boundary_id.to_wire_string(),
                    run.contexts.len(),
                    run.terminal_health,
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(io::Error::other(format!(
            "workload profiling boundary not found; observed {observed}"
        ))
        .into());
    };
    assert_eq!(
        runs.iter()
            .filter(|(_, run, counts)| {
                u64::try_from(run.contexts.len()).ok() == Some(expected_contexts)
                    && counts.invocations.iter().sum::<u64>() == expected_total_calls
            })
            .count(),
        1,
        "workload profiling boundary is not unique"
    );
    let (boundary_id, run, counts) = &runs[workload_index];

    assert_eq!(counts.invocations[0], expected_root_calls);
    assert_eq!(counts.invocations[1], expected_call_edges);
    assert_eq!(counts.invocations[2], expected_spawn_edges);
    assert_eq!(counts.completed, expected_total_calls);
    assert!(
        (expected_awaits_min..=expected_awaits_max).contains(&counts.await_count),
        "await count {} outside semantic bounds {expected_awaits_min}..={expected_awaits_max}",
        counts.await_count,
    );
    assert_eq!(counts.contexts[0], 1);
    assert_eq!(
        counts.contexts[1] + counts.contexts[2],
        expected_nonroot_contexts
    );

    let workload_end = run.end.as_ref().expect("integrity checked sealed run");

    println!(
        "{}",
        json!({
            "boundary_id": boundary_id.to_wire_string(),
            "durable_runs": runs.len(),
            "contexts": run.contexts.len(),
            "context_counts": {
                "root": counts.contexts[0],
                "call": counts.contexts[1],
                "spawn": counts.contexts[2],
            },
            "invocations": {
                "total": expected_total_calls,
                "root": counts.invocations[0],
                "call": counts.invocations[1],
                "spawn": counts.invocations[2],
            },
            "await_count": counts.await_count,
            "selected_spans": run.spans.len(),
            "transport_loss": run.terminal_health.structural_transport_exceeded,
            "cct_segments": workload_end.fence.cct.segment_count,
            "evidence_segments": workload_end.fence.evidence.segment_count,
        })
    );
    Ok(())
}

#[derive(Debug)]
struct RunCounts {
    contexts: [u64; 3],
    invocations: [u64; 3],
    completed: u64,
    await_count: u64,
}

fn assert_run_integrity(
    reader: &DurableRunReader,
    run: &ProfileRun,
) -> Result<RunCounts, Box<dyn Error>> {
    let end = run
        .end
        .as_ref()
        .ok_or_else(|| io::Error::other("durable run is incomplete"))?;
    assert_eq!(end.end.status, BoundaryEndStatus::Succeeded);
    assert_eq!(run.terminal_health, BoundaryHealthSnapshot::default());
    assert_eq!(run.cct_health, CounterHealth::default());
    assert!(run.overflow.is_empty(), "unexpected CCT overflow");
    assert!(run.errors.is_empty(), "unexpected error evidence");

    let mut counts = RunCounts {
        contexts: [0; 3],
        invocations: [0; 3],
        completed: 0,
        await_count: 0,
    };
    let mut selected = 0_u64;
    for context in run.contexts.values() {
        let tuple = context
            .tuple
            .ok_or_else(|| io::Error::other("context definition missing"))?;
        let edge = match tuple.edge_kind {
            EdgeKind::Root => 0,
            EdgeKind::Call => 1,
            EdgeKind::Spawn => 2,
        };
        counts.contexts[edge] = counts.contexts[edge]
            .checked_add(1)
            .ok_or("context count overflow")?;
        counts.invocations[edge] = counts.invocations[edge]
            .checked_add(context.counters.invocations_started)
            .ok_or("invocation count overflow")?;
        selected = selected
            .checked_add(context.counters.spans_selected)
            .ok_or("selected count overflow")?;
        counts.completed = counts
            .completed
            .checked_add(context.counters.completed_ok)
            .ok_or("completed count overflow")?;
        counts.await_count = counts
            .await_count
            .checked_add(context.counters.await_count)
            .ok_or("await count overflow")?;
        assert_eq!(
            context.counters.completed_ok, context.counters.invocations_started,
            "context has an incomplete invocation"
        );
        assert_eq!(context.counters.completed_error, 0);
        assert_eq!(context.counters.completed_cancelled, 0);
        assert_eq!(context.counters.completed_exit, 0);
    }
    assert_eq!(selected, 1);

    assert_eq!(run.spans.len(), 1, "only the root span should be selected");
    let span = run.spans.values().next().expect("one selected root span");
    let start = span.start.ok_or("selected root has no start")?;
    let finish = span.end.ok_or("selected root has no end")?;
    assert_eq!(start.edge_kind, EdgeKind::Root);
    assert!(start.selection_reasons.root());
    assert!(!start.selection_reasons.llm());
    assert!(!start.selection_reasons.manual());
    assert_eq!(start.roles, RoleMask::ALL);
    assert_eq!(finish.status, FunctionEndStatus::Ok);
    assert!(span.terminal_error.is_none());
    assert_available(reader, span.input.ok_or("root input evidence missing")?)?;
    assert_available(reader, span.output.ok_or("root output evidence missing")?)?;
    Ok(counts)
}

fn assert_available(
    reader: &DurableRunReader,
    occurrence: bex_events::prof::backend::ValueOccurrence,
) -> Result<(), Box<dyn Error>> {
    let ValueState::Available { cid, .. } = occurrence.state else {
        return Err(io::Error::other("selected root value was lost").into());
    };
    let object = reader.read_value(cid)?;
    assert_eq!(object.cid, cid);
    Ok(())
}

fn boundaries(store_root: &Path) -> Result<Vec<BoundaryId>, Box<dyn Error>> {
    let mut directories = fs::read_dir(store_root.join("runs"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    directories.sort_by_key(fs::DirEntry::file_name);
    directories
        .into_iter()
        .map(|entry| {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| io::Error::other("run directory is not UTF-8"))?;
            Ok(BoundaryId::from_bytes(decode_boundary_hex(&name)?))
        })
        .collect()
}

fn decode_boundary_hex(value: &str) -> Result<[u8; 16], Box<dyn Error>> {
    if value.len() != 32 {
        return Err(io::Error::other("invalid boundary directory length").into());
    }
    let mut output = [0_u8; 16];
    for (index, slot) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *slot = (hex_nibble(value.as_bytes()[offset])? << 4)
            | hex_nibble(value.as_bytes()[offset + 1])?;
    }
    Ok(output)
}

fn hex_nibble(value: u8) -> Result<u8, Box<dyn Error>> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(io::Error::other("invalid boundary directory hex").into()),
    }
}
