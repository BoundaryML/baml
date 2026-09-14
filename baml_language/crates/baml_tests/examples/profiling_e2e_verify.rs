//! Durable verification for the packed profiling e2e run: every execution of
//! the process stream must be complete, healthy, and semantically consistent
//! with the workload's expected call/context population.

use std::{error::Error, io, path::Path};

use bex_events::prof::{
    backend::{
        CounterHealth, DataState, EdgeKind, ExecutionHealthSnapshot, ExecutionProfile,
        ExecutionReader, ExecutionStatus, IndexState, Plane, RoleMask, StreamReader, ValueState,
        list_streams, segment_path,
    },
    record::FunctionEndStatus,
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

    let store_root = Path::new(&store_root);
    // One process ⇒ one stream (streams spec §3/§9).
    let streams = list_streams(store_root)?;
    assert_eq!(
        streams.len(),
        1,
        "the packed single-process run must produce exactly one stream"
    );
    let stream_reader = StreamReader::open(store_root, streams[0])?;
    assert!(
        !stream_reader.alive,
        "the packed process exited; its stream must read dead"
    );
    assert!(
        stream_reader.index_gaps.is_empty(),
        "index gaps: {:?}",
        stream_reader.index_gaps
    );
    assert!(
        stream_reader.header.is_some(),
        "the stream header (wall-clock zero) must be durable"
    );
    let meta_segments = stream_reader.high_water.meta;
    let data_segments = stream_reader.high_water.data;
    let summaries = stream_reader.executions();
    assert_eq!(
        summaries.len(),
        2,
        "packed execution should produce workload and output-serialization executions"
    );

    let mut profiles = Vec::with_capacity(summaries.len());
    for summary in &summaries {
        let reader = stream_reader.execution(summary.id)?;
        let profile = reader.load()?;
        let counts = assert_execution_integrity(&reader, &profile)?;
        profiles.push((summary.id, profile, counts));
    }

    let workload_index = profiles.iter().position(|(_, profile, counts)| {
        u64::try_from(profile.contexts.len()).ok() == Some(expected_contexts)
            && counts.invocations.iter().sum::<u64>() == expected_total_calls
    });
    let Some(workload_index) = workload_index else {
        let observed = profiles
            .iter()
            .map(|(id, profile, counts)| {
                format!(
                    "{}: contexts={}, counts={counts:?}, health={:?}",
                    id.encode(),
                    profile.contexts.len(),
                    profile.summary.health,
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(io::Error::other(format!(
            "workload profiling execution not found; observed {observed}"
        ))
        .into());
    };
    assert_eq!(
        profiles
            .iter()
            .filter(|(_, profile, counts)| {
                u64::try_from(profile.contexts.len()).ok() == Some(expected_contexts)
                    && counts.invocations.iter().sum::<u64>() == expected_total_calls
            })
            .count(),
        1,
        "workload profiling execution is not unique"
    );
    let serialization_index = 1_usize
        .checked_sub(workload_index)
        .expect("two execution indexes");
    let (execution_id, profile, counts) = &profiles[workload_index];
    let (_, serialization, serialization_counts) = &profiles[serialization_index];

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

    assert_eq!(serialization.contexts.len(), 3);
    assert_eq!(serialization_counts.contexts, [1, 2, 0]);
    assert_eq!(serialization_counts.invocations, [1, 2, 0]);
    assert_eq!(serialization_counts.completed, 3);
    assert_eq!(serialization_counts.await_count, 0);

    // Optional strict layout gate for runs whose publish interval exceeded
    // the run duration (streams spec §9: meta, data, meta).
    if std::env::var_os("PROFILING_E2E_EXPECT_MINIMAL_SEGMENTS").is_some() {
        assert_eq!(
            (meta_segments, data_segments),
            (2, 1),
            "expected the minimal meta,data,meta stream layout"
        );
    }
    // The layout is O(bytes), never O(executions): a single fast process must
    // not produce more than a handful of segments per plane.
    for sequence in 1..=data_segments {
        assert!(
            segment_path(store_root, streams[0], Plane::Data, sequence).is_file(),
            "data plane must be contiguous"
        );
    }

    let transport_loss = profiles
        .iter()
        .map(|(_, profile, _)| {
            profile
                .summary
                .health
                .map_or(0, |health| health.structural_transport_exceeded)
        })
        .sum::<u64>();

    println!(
        "{}",
        json!({
            "execution_id": execution_id.encode(),
            "stream_count": streams.len(),
            "durable_executions": profiles.len(),
            "contexts": profile.contexts.len(),
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
            "selected_spans": profile.spans.len(),
            "threads": profile.threads.len(),
            "serialization": {
                "contexts": serialization.contexts.len(),
                "invocations": serialization_counts.invocations.iter().sum::<u64>(),
                "selected_spans": serialization.spans.len(),
            },
            "transport_loss": transport_loss,
            "meta_segments": meta_segments,
            "data_segments": data_segments,
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

fn assert_execution_integrity(
    reader: &ExecutionReader,
    profile: &ExecutionProfile,
) -> Result<RunCounts, Box<dyn Error>> {
    assert_eq!(profile.summary.status, ExecutionStatus::Succeeded);
    assert_eq!(profile.summary.index_state, IndexState::Complete);
    assert_eq!(profile.data_state, DataState::Complete);
    assert_eq!(
        profile.summary.health,
        Some(ExecutionHealthSnapshot::default()),
        "the packed run must be loss-free"
    );
    assert_eq!(profile.cct_health, CounterHealth::default());
    assert!(profile.overflow.is_empty(), "unexpected CCT overflow");
    assert!(profile.errors.is_empty(), "unexpected error evidence");
    assert!(
        profile.summary.started_unix_ns.is_some(),
        "wall clock must be durable"
    );
    assert!(
        profile.thread_issues.is_empty(),
        "thread lineage must be loss-free: {:?}",
        profile.thread_issues
    );
    // Every thread has durable start AND end facts in a loss-free run.
    for (thread_ref, thread) in &profile.threads {
        assert!(thread.start.is_some(), "thread start for {thread_ref:?}");
        assert!(thread.end.is_some(), "thread end for {thread_ref:?}");
    }

    let mut counts = RunCounts {
        contexts: [0; 3],
        invocations: [0; 3],
        completed: 0,
        await_count: 0,
    };
    let mut selected = 0_u64;
    for context in profile.contexts.values() {
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

    assert_eq!(
        profile.spans.len(),
        1,
        "only the root span should be selected"
    );
    let span = profile
        .spans
        .values()
        .next()
        .expect("one selected root span");
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
    reader: &ExecutionReader,
    occurrence: bex_events::prof::backend::ValueOccurrence,
) -> Result<(), Box<dyn Error>> {
    let ValueState::Available { cid, .. } = occurrence.state else {
        return Err(io::Error::other("selected root value was lost").into());
    };
    let object = reader.read_value(cid)?;
    assert_eq!(object.cid, cid);
    Ok(())
}
