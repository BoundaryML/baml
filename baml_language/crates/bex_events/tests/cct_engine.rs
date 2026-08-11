//! CCT engine correctness fixtures (observability design §5.2/§5.3/§5.6):
//! the two-ring migration fixture, the corrupt-range fixture, the >512
//! recursion fold (unreachable end-to-end while the VM caps frames at 256 —
//! this consumer-level stream is the only way to pin it), and the
//! suspend/resume accounting contract.

use bex_events::ids::{BexCallId, BexThreadId, FunctionId};
use bex_events::prof::cct::{CctEngine, RECURSION_FOLD_DEPTH};
use bex_events::prof::record::{
    FunctionEndStatus, MAX_RECORD_LEN, RawRecord, SuspendReason, ThreadEndStatus,
};

/// Encode a record sequence into one drained-range byte buffer.
fn encode(records: &[RawRecord<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; MAX_RECORD_LEN];
    for rec in records {
        let len = rec.encode(&mut buf);
        out.extend_from_slice(&buf[..len]);
    }
    out
}

fn start_thread(thread: u64, parent: u64, parent_call: u64, ts: u64) -> RawRecord<'static> {
    RawRecord::StartThread {
        flags: 0,
        thread_id: BexThreadId(thread),
        parent_thread_id: BexThreadId(parent),
        parent_call_id: BexCallId(parent_call),
        ts_ticks: ts,
        name: b"",
    }
}

fn call(thread: u64, id: u64, parent: u64, function: u32, ts: u64) -> RawRecord<'static> {
    RawRecord::CallFunction {
        flags: 0,
        thread_id: BexThreadId(thread),
        call_id: BexCallId(id),
        parent_call_id: BexCallId(parent),
        function_id: FunctionId(function),
        call_site: None,
        ts_ticks: ts,
    }
}

fn end(thread: u64, id: u64, status: FunctionEndStatus, ts: u64) -> RawRecord<'static> {
    RawRecord::EndFunction {
        status,
        thread_id: BexThreadId(thread),
        call_id: BexCallId(id),
        ts_ticks: ts,
    }
}

fn end_thread(thread: u64, status: ThreadEndStatus, ts: u64) -> RawRecord<'static> {
    RawRecord::EndThread {
        status,
        thread_id: BexThreadId(thread),
        ts_ticks: ts,
    }
}

fn identity(ticks: u64) -> u64 {
    ticks
}

fn consume(engine: &mut CctEngine, bytes: &[u8]) {
    engine.consume(bytes, &mut identity);
}

/// Counters keyed by function id, independent of node layout, for
/// order-invariance comparisons.
fn totals_by_function(engine: &CctEngine) -> std::collections::BTreeMap<u32, (u64, u64, u64)> {
    let nodes = engine.nodes();
    let mut map = std::collections::BTreeMap::new();
    for i in 0..nodes.len() {
        let entry = map.entry(nodes.function[i]).or_insert((0, 0, 0));
        entry.0 += nodes.enters[i];
        entry.1 += nodes.ends_ok[i] + nodes.ends_err[i] + nodes.ends_cancel[i] + nodes.ends_exit[i];
        entry.2 += nodes.total_ns[i];
    }
    map
}

#[test]
fn nested_calls_build_context_tree_with_exact_counts() {
    let mut engine = CctEngine::new(16);
    let bytes = encode(&[
        start_thread(1, 0, 0, 0),
        call(1, 1, 0, 100, 10),
        call(1, 2, 1, 101, 20),
        end(1, 2, FunctionEndStatus::Ok, 50),
        call(1, 3, 1, 101, 60),
        end(1, 3, FunctionEndStatus::Errored, 90),
        end(1, 1, FunctionEndStatus::Ok, 100),
        end_thread(1, ThreadEndStatus::Completed, 110),
    ]);
    consume(&mut engine, &bytes);

    let nodes = engine.nodes();
    // partition root + fn100 + (fn100→fn101): the two fn101 calls share
    // one context node — cost grows with unique contexts, not calls.
    assert_eq!(nodes.len(), 3);
    let idx_101 = (0..nodes.len())
        .find(|&i| nodes.function[i] == 101)
        .unwrap();
    assert_eq!(nodes.enters[idx_101], 2);
    assert_eq!(nodes.ends_ok[idx_101], 1);
    assert_eq!(nodes.ends_err[idx_101], 1);
    assert_eq!(nodes.total_ns[idx_101], 30 + 30);
    let idx_100 = (0..nodes.len())
        .find(|&i| nodes.function[i] == 100)
        .unwrap();
    assert_eq!(nodes.total_ns[idx_100], 90);
    // Self-time: fn100 owns [10..20]+[50..60]+[90..100]=30; fn101 owns
    // [20..50]+[60..90]=60.
    assert_eq!(nodes.self_ns[idx_100], 30);
    assert_eq!(nodes.self_ns[idx_101], 60);
    let diag = engine.diagnostics();
    assert_eq!(diag.deferred, 0, "single ring never defers");
    assert_eq!(diag.degraded_partitions, 0);
    // The recent ring holds all three completed calls.
    assert_eq!(engine.recent_ring(0).unwrap().len(), 3);
}

/// §5.2: the two-ring migration fixture. The same logical thread's records
/// split at an await point; the post-await range drains FIRST. Counters
/// must match the in-order run exactly, via defer + replay.
#[test]
fn two_ring_migration_defers_and_replays_to_identical_counters() {
    let pre_await: Vec<RawRecord<'static>> = vec![
        start_thread(1, 0, 0, 0),
        call(1, 1, 0, 100, 10),
        call(1, 2, 1, 101, 20),
    ];
    let post_await: Vec<RawRecord<'static>> = vec![
        call(1, 3, 2, 102, 30),
        end(1, 3, FunctionEndStatus::Ok, 40),
        end(1, 2, FunctionEndStatus::Ok, 50),
        end(1, 1, FunctionEndStatus::Ok, 60),
        end_thread(1, ThreadEndStatus::Completed, 70),
    ];

    // In-order reference.
    let mut reference = CctEngine::new(16);
    let mut all = pre_await.clone();
    all.extend(post_await.iter().cloned());
    consume(&mut reference, &encode(&all));

    // Migrated: post-await ring drains first.
    let mut migrated = CctEngine::new(16);
    consume(&mut migrated, &encode(&post_await));
    let diag_mid = migrated.diagnostics();
    assert!(diag_mid.deferred > 0, "cross-ring records must defer");
    consume(&mut migrated, &encode(&pre_await));

    assert_eq!(
        totals_by_function(&reference),
        totals_by_function(&migrated)
    );
    let diag = migrated.diagnostics();
    assert!(
        diag.replayed >= diag_mid.deferred,
        "deferred records replayed"
    );
    assert_eq!(
        diag.synthesized_parents, 0,
        "no timeout on a healthy stream"
    );
    assert_eq!(diag.degraded_partitions, 0);
}

/// §5.2 resync: a deferral surviving DEFER_MAX_SWEEPS synthesizes the
/// missing parent as the unattributable node, replays dependents, and
/// degrades the partition — never a wedge, never a silent drop.
#[test]
fn defer_timeout_synthesizes_unattributable_parent_and_degrades() {
    let mut engine = CctEngine::new(16);
    // A child call whose parent call 99 never arrives.
    let bytes = encode(&[
        start_thread(1, 0, 0, 0),
        call(1, 2, 99, 101, 20),
        end(1, 2, FunctionEndStatus::Ok, 40),
    ]);
    consume(&mut engine, &bytes);
    assert!(engine.diagnostics().deferred > 0);

    for _ in 0..bex_events::prof::cct::DEFER_MAX_SWEEPS + 1 {
        engine.sweep_tick(&mut identity);
    }
    let diag = engine.diagnostics();
    assert!(diag.synthesized_parents > 0, "timeout must synthesize");
    assert_eq!(diag.degraded_partitions, 1, "partition visibly degraded");
    // The child applied under the synthesized unattributable parent.
    let totals = totals_by_function(&engine);
    assert_eq!(totals.get(&101).map(|t| (t.0, t.1)), Some((1, 1)));
    // The synthesized parent is a function-0 node with an open enter.
    assert!(totals.get(&0).is_some_and(|t| t.0 >= 1));
}

/// A corrupt range degrades every live partition (§5.2) but aggregation
/// keeps going.
#[test]
fn corrupt_range_degrades_partitions_but_keeps_aggregating() {
    let mut engine = CctEngine::new(16);
    let mut bytes = encode(&[start_thread(1, 0, 0, 0), call(1, 1, 0, 100, 10)]);
    bytes.push(0xEE); // unknown tag: rest of range is unrecoverable
    consume(&mut engine, &bytes);
    assert_eq!(engine.diagnostics().degraded_partitions, 1);
    assert_eq!(
        engine.diagnostics().corrupt_ranges,
        1,
        "corruption is counted so it can persist as evidence"
    );

    // Later ranges still aggregate.
    consume(
        &mut engine,
        &encode(&[end(1, 1, FunctionEndStatus::Ok, 30)]),
    );
    let totals = totals_by_function(&engine);
    assert_eq!(totals.get(&100).map(|t| t.1), Some(1));
}

/// Structural-exhaustion shed: dropped-record counts degrade every live
/// partition and accumulate in diagnostics, exactly like corruption — a
/// drop is declared loss, never a silent gap.
#[test]
fn structural_shed_degrades_and_counts() {
    let mut engine = CctEngine::new(16);
    consume(
        &mut engine,
        &encode(&[start_thread(1, 0, 0, 0), call(1, 1, 0, 100, 10)]),
    );
    assert_eq!(engine.diagnostics().degraded_partitions, 0);

    engine.note_structural_shed(42);
    engine.note_structural_shed(8);
    let diag = engine.diagnostics();
    assert_eq!(diag.shed_records, 50);
    assert_eq!(diag.degraded_partitions, 1, "shed coarsens live partitions");

    // Aggregation continues after the shed (lower bounds, not a wedge).
    consume(
        &mut engine,
        &encode(&[end(1, 1, FunctionEndStatus::Ok, 30)]),
    );
    let totals = totals_by_function(&engine);
    assert_eq!(totals.get(&100).map(|t| t.1), Some(1));
}

/// §5.3: suspend/resume splits self vs awaiting, and the self-contained
/// resume reconstructs the parked window even when the suspend record is
/// missing (cross-ring).
#[test]
fn suspend_resume_attributes_awaiting_time() {
    let suspend = RawRecord::SuspendThread {
        reason: SuspendReason::SysOp,
        thread_id: BexThreadId(1),
        suspend_seq: 1,
        ts_ticks: 30,
    };
    let resume = RawRecord::ResumeThread {
        flags: 0,
        thread_id: BexThreadId(1),
        suspend_seq: 1,
        suspend_ts_ticks: 30,
        ts_ticks: 80,
    };
    let base = [
        start_thread(1, 0, 0, 0),
        call(1, 1, 0, 100, 10),
        suspend,
        resume,
        end(1, 1, FunctionEndStatus::Ok, 100),
        end_thread(1, ThreadEndStatus::Completed, 110),
    ];
    let mut engine = CctEngine::new(16);
    consume(&mut engine, &encode(&base));
    let nodes = engine.nodes();
    let idx = (0..nodes.len())
        .find(|&i| nodes.function[i] == 100)
        .unwrap();
    assert_eq!(
        nodes.await_ns[idx], 50,
        "parked window [30..80] is awaiting"
    );
    assert_eq!(nodes.self_ns[idx], 20 + 20, "running windows are self");
    assert_eq!(nodes.total_ns[idx], 90);

    // Same stream WITHOUT the suspend record (lost to ring split): the
    // self-contained resume reconstructs the same attribution.
    let without_suspend = [
        start_thread(1, 0, 0, 0),
        call(1, 1, 0, 100, 10),
        resume,
        end(1, 1, FunctionEndStatus::Ok, 100),
        end_thread(1, ThreadEndStatus::Completed, 110),
    ];
    let mut engine2 = CctEngine::new(16);
    consume(&mut engine2, &encode(&without_suspend));
    let nodes2 = engine2.nodes();
    let idx2 = (0..nodes2.len())
        .find(|&i| nodes2.function[i] == 100)
        .unwrap();
    assert_eq!(
        nodes2.await_ns[idx2], 50,
        "resume alone reconstructs the park"
    );
    assert_eq!(nodes2.self_ns[idx2], 40);
}

/// §5.6: the >512 recursion fold, reachable only via a synthetic stream
/// (the VM caps frames at 256 — see the deep/ workload note).
#[test]
fn deep_recursion_folds_past_threshold_with_exact_counts() {
    let depth = u64::from(RECURSION_FOLD_DEPTH) + 100;
    let mut records = vec![start_thread(1, 0, 0, 0)];
    for level in 0..depth {
        // Mutual recursion a<->b: function id alternates.
        records.push(call(
            1,
            level + 1,
            level,
            100 + u32::try_from(level % 2).unwrap(),
            level,
        ));
    }
    for level in (0..depth).rev() {
        records.push(end(
            1,
            level + 1,
            FunctionEndStatus::Ok,
            depth + (depth - level),
        ));
    }
    records.push(end_thread(1, ThreadEndStatus::Completed, 3 * depth));

    let mut engine = CctEngine::new(16);
    consume(&mut engine, &encode(&records));

    let diag = engine.diagnostics();
    assert!(
        diag.folded_frames > 0,
        "fold engages past {RECURSION_FOLD_DEPTH}"
    );
    assert!(
        engine.nodes().len() <= usize::from(RECURSION_FOLD_DEPTH) + 8,
        "node table bounded by the fold: {}",
        engine.nodes().len()
    );
    // Counts stay exact through the fold.
    let totals = totals_by_function(&engine);
    let total_enters: u64 = totals.values().map(|t| t.0).sum();
    let total_ends: u64 = totals.values().map(|t| t.1).sum();
    assert_eq!(total_enters, depth);
    assert_eq!(total_ends, depth);
    assert_eq!(diag.deferred, 0);
}

/// §5.5: 10k equivalent workers share ONE spawn edge and one child subtree.
#[test]
fn equivalent_spawns_share_edge_and_subtree() {
    let workers: u64 = 100;
    let mut records = vec![
        start_thread(1, 0, 0, 0),
        call(1, 1, 0, 100, 1), // the spawning call
    ];
    for w in 0..workers {
        let tid = 2 + w;
        records.push(start_thread(tid, 1, 1, 10 + w));
        records.push(call(tid, 1, 0, 200, 10 + w));
        records.push(end(tid, 1, FunctionEndStatus::Ok, 20 + w));
        records.push(end_thread(tid, ThreadEndStatus::Completed, 21 + w));
    }
    records.push(end(1, 1, FunctionEndStatus::Ok, 500));
    records.push(end_thread(1, ThreadEndStatus::Completed, 501));

    let mut engine = CctEngine::new(16);
    consume(&mut engine, &encode(&records));

    let edges = engine.spawn_edges();
    assert_eq!(edges.len(), 1, "equivalent spawns share one edge");
    assert_eq!(edges.counters[0].spawned, workers);
    assert_eq!(edges.counters[0].completed, workers);
    assert_eq!(edges.counters[0].live, 0);
    // One shared child subtree: exactly one node for fn 200.
    let nodes = engine.nodes();
    let fn200_nodes = (0..nodes.len())
        .filter(|&i| nodes.function[i] == 200)
        .count();
    assert_eq!(fn200_nodes, 1, "one shared child subtree");
    let idx = (0..nodes.len())
        .find(|&i| nodes.function[i] == 200)
        .unwrap();
    assert_eq!(nodes.enters[idx], workers);
}

/// Window dirty-sets (§6.3): only touched nodes appear per window.
#[test]
fn windows_carry_only_dirty_nodes() {
    let mut engine = CctEngine::new(16);
    consume(
        &mut engine,
        &encode(&[
            start_thread(1, 0, 0, 0),
            call(1, 1, 0, 100, 10),
            end(1, 1, FunctionEndStatus::Ok, 20),
        ]),
    );
    let w0 = engine.take_window();
    assert!(!w0.dirty_nodes.is_empty());
    assert_eq!(
        w0.births.len(),
        engine.nodes().len(),
        "all births in window 0"
    );

    // Nothing happened: the next window is empty (idle ⇒ ~0 rows).
    let w1 = engine.take_window();
    assert!(w1.dirty_nodes.is_empty(), "idle window has no dirty rows");
    assert!(w1.births.is_empty());

    // A new call dirties exactly the touched path.
    consume(
        &mut engine,
        &encode(&[call(1, 2, 0, 100, 30), end(1, 2, FunctionEndStatus::Ok, 40)]),
    );
    let w2 = engine.take_window();
    assert!(!w2.dirty_nodes.is_empty());
    assert!(w2.births.is_empty(), "no new contexts, no new births");
}

/// §6.3 delta discipline: `flush_window` emits true deltas (current −
/// last-flushed), births before first reference, hist rows only for
/// windows with closes, and idle windows cost zero rows.
#[test]
fn flush_window_emits_true_deltas() {
    let mut engine = CctEngine::new(16);
    consume(
        &mut engine,
        &encode(&[
            start_thread(1, 0, 0, 0),
            call(1, 1, 0, 100, 10),
            end(1, 1, FunctionEndStatus::Ok, 30),
        ]),
    );
    let w0 = engine.flush_window();
    assert!(!w0.birth_rows.is_empty(), "births in the first window");
    let fn100_birth = w0.birth_rows.iter().find(|b| b.function_id == 100).unwrap();
    assert_eq!(fn100_birth.logical_thread_id, 1, "birth carries its thread");
    let d0 = w0
        .delta_rows
        .iter()
        .find(|d| d.node_id == fn100_birth.node_id)
        .expect("delta row for the touched node");
    assert_eq!((d0.enters, d0.ends_ok, d0.total_ns), (1, 1, 20));
    assert_eq!(w0.hist_rows.len(), 1, "one close = one hist row set");

    // Idle window: zero rows of any kind.
    let w1 = engine.flush_window();
    assert!(w1.birth_rows.is_empty());
    assert!(w1.delta_rows.is_empty());
    assert!(w1.hist_rows.is_empty());

    // Another call: the next window carries ONLY the delta (1 enter, not 2).
    consume(
        &mut engine,
        &encode(&[call(1, 2, 0, 100, 40), end(1, 2, FunctionEndStatus::Ok, 45)]),
    );
    let w2 = engine.flush_window();
    assert!(w2.birth_rows.is_empty(), "no new contexts");
    let d2 = w2
        .delta_rows
        .iter()
        .find(|d| d.node_id == fn100_birth.node_id)
        .unwrap();
    assert_eq!(
        (d2.enters, d2.ends_ok, d2.total_ns),
        (1, 1, 5),
        "true delta"
    );
}

/// §6.1 session-epoch rotation: fresh node table, live stacks re-interned
/// by path, open calls keep aggregating; totals split across epochs.
#[test]
fn epoch_rotation_reinterns_live_stacks() {
    let mut engine = CctEngine::new(16);
    consume(
        &mut engine,
        &encode(&[
            start_thread(1, 0, 0, 0),
            call(1, 1, 0, 100, 10),
            call(1, 2, 1, 101, 20), // still open at rotation
        ]),
    );
    let nodes_before = engine.nodes().len();
    assert_eq!(nodes_before, 3);

    engine.rotate_epoch(16);
    // Fresh table: pseudo-root + the re-interned open path (fn100, fn101).
    assert_eq!(engine.nodes().len(), 3, "open path re-interned");
    let t = totals_by_function(&engine);
    assert_eq!(t.get(&100).map(|x| x.0), Some(0), "counters restart");

    // The open call closes in the NEW epoch: end lands on the re-interned
    // node with correct per-epoch attribution.
    consume(
        &mut engine,
        &encode(&[
            end(1, 2, FunctionEndStatus::Ok, 50),
            end(1, 1, FunctionEndStatus::Ok, 60),
            end_thread(1, ThreadEndStatus::Completed, 70),
        ]),
    );
    let t = totals_by_function(&engine);
    // Ends counted in this epoch; enters stayed in the previous one (the
    // carry-over checkpoint holds them) — the documented boundary shape.
    assert_eq!(t.get(&101).map(|x| (x.0, x.1)), Some((0, 1)));
    assert_eq!(t.get(&100).map(|x| (x.0, x.1)), Some((0, 1)));
    assert_eq!(
        engine.diagnostics().deferred,
        0,
        "no defers across rotation"
    );
}
