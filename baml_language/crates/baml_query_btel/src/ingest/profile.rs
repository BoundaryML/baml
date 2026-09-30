//! The profiler of each ended process, built once its end is indexed.
//!
//! A process is every recording in this index that names the same process
//! ID (a recording without one is its own process). Its profiler merges the
//! call paths of all those recordings by profiler node, so one function
//! reached the same way from several call sites, threads or engines is one
//! row. Rebuilt whenever one of the process's recordings changes.
use std::collections::hash_map::Entry;

use btel_reader::timing::{Clock, TimingState};
use rusqlite::{Connection, Transaction, params, types::ValueRef};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use crate::{Error, functions};

/// One node's sums over the paths that share it.
#[derive(Default)]
struct Node {
    /// One of its paths, `(rec, call_path_id)`; path IDs start at 1.
    sample: (i64, i64),
    parent: Option<i64>,
    name: Option<String>,
    /// Reached synchronously from its parent: its time is part of the parent's.
    synchronous: bool,
    total_ns: Option<i128>,
    io_self_ns: Option<i128>,
    invocations: Option<u128>,
    errored: Option<u128>,
    cancelled: Option<u128>,
    panicked: Option<u128>,
}

fn add(sum: &mut Option<u128>, value: Option<i64>) {
    *sum = sum
        .zip(value.and_then(|v| u128::try_from(v).ok()))
        .map(|(a, b)| a + b);
}

fn add_ns(sum: &mut Option<i128>, value: Option<i64>) {
    *sum = sum.zip(value).map(|(a, b)| a + i128::from(b));
}

/// Rebuild the profiler of each process in `profile_pending`: nothing until
/// the process's end is indexed. Each process leaves the queue in the
/// transaction that rebuilds it.
pub(super) fn rebuild_pending(conn: &mut Connection) -> Result<(), Error> {
    let processes: Vec<Vec<u8>> = conn
        .prepare("SELECT process_id FROM profile_pending")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for process in &processes {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM profile_pending WHERE process_id = ?1",
            [process],
        )?;
        tx.execute("DELETE FROM profile_node WHERE process_id = ?1", [process])?;
        let ended: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM recording
               WHERE COALESCE(process_id, recording_id) = ?1 AND process_end_status IS NOT NULL)",
            [process],
            |r| r.get(0),
        )?;
        if ended {
            build(&tx, process)?;
        }
        tx.commit()?;
    }
    Ok(())
}

fn build(tx: &Transaction<'_>, process: &[u8]) -> Result<(), Error> {
    let mut nodes: FxHashMap<i64, Node> = FxHashMap::default();
    let recs: Vec<i64> = tx
        .prepare_cached("SELECT rec FROM recording WHERE COALESCE(process_id, recording_id) = ?1")?
        .query_map([process], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for rec in recs {
        add_paths(tx, rec, &mut nodes)?;
    }
    // Every path of a node has the same parent node, edge and function name
    // (the node ID hashes them), so one path of each names it.
    let mut describe = tx.prepare_cached(
        "SELECT parent.node_id, p.edge, IIF(f.state = 2, f.fqn, NULL)
         FROM call_path p
         LEFT JOIN call_path parent
           ON parent.rec = p.rec AND parent.call_path_id = p.parent_call_path_id
         LEFT JOIN function_def f ON f.rec = p.rec AND f.function_id = p.callee_function_id
         WHERE p.rec = ?1 AND p.call_path_id = ?2",
    )?;
    for node in nodes.values_mut() {
        let (rec, path) = node.sample;
        (node.parent, node.synchronous, node.name) = describe.query_row([rec, path], |r| {
            Ok((r.get(0)?, r.get::<_, Option<i64>>(1)? == Some(1), r.get(2)?))
        })?;
    }
    drop(describe);

    // Synchronous children, for self time and total I/O.
    let mut children: FxHashMap<i64, Vec<i64>> = FxHashMap::default();
    for (id, node) in &nodes {
        if let Some(parent) = node.parent
            && node.synchronous
        {
            children.entry(parent).or_default().push(*id);
        }
    }
    let io_totals = io_totals(&nodes, &children);
    let ids: Vec<i64> = nodes.keys().copied().collect();
    // A context entered but never completed, with nothing completed below
    // it, is not a row: it has no invocation to report.
    let mut kept: FxHashSet<i64> = FxHashSet::default();
    for (id, node) in &nodes {
        let ran = node.invocations != Some(0) || node.io_self_ns != Some(0);
        let mut current = Some(*id);
        while ran && let Some(at) = current {
            if !kept.insert(at) {
                break;
            }
            current = nodes.get(&at).and_then(|n| n.parent);
        }
    }
    let ids: Vec<i64> = ids.into_iter().filter(|id| kept.contains(id)).collect();

    let fits = |value: Option<i128>| value.and_then(|v| i64::try_from(v).ok());
    let count = |value: Option<u128>| value.and_then(|v| i64::try_from(v).ok());
    let mut insert = tx.prepare_cached(
        "INSERT INTO profile_node (process_id, node_id, parent_node_id, function_name, total_ns,
           self_ns, io_total_ns, io_self_ns, invocation_count, return_count, error_count,
           panic_error_count, user_error_count, cancel_error_count, missing_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0)",
    )?;
    for id in ids {
        let node = &nodes[&id];
        let children_total = children.get(&id).map_or(Some(0), |kids| {
            kids.iter()
                .try_fold(0_i128, |sum, kid| Some(sum + nodes[kid].total_ns?))
        });
        let self_ns = node
            .total_ns
            .zip(children_total)
            .map(|(total, kids)| (total - kids).max(0));
        let failed = node.errored.zip(node.cancelled).map(|(e, c)| e + c);
        let returned = node
            .invocations
            .zip(failed)
            .and_then(|(n, f)| n.checked_sub(f));
        let user = node
            .errored
            .zip(node.panicked)
            .and_then(|(e, p)| e.checked_sub(p));
        insert.execute(params![
            process,
            id,
            node.parent,
            node.name,
            fits(node.total_ns),
            fits(self_ns),
            fits(io_totals[&id]),
            fits(node.io_self_ns),
            count(node.invocations),
            count(returned),
            count(failed),
            count(node.panicked),
            count(user),
            count(node.cancelled)
        ])?;
    }
    Ok(())
}

/// Add one recording's call paths to their nodes: each path's two aggregate
/// rows and sysop time, in nanoseconds by its thread's clock. A million
/// paths is an ordinary recording, so this reads four rows per path and
/// converts ticks here instead of in SQL.
fn add_paths(
    tx: &Transaction<'_>,
    rec: i64,
    nodes: &mut FxHashMap<i64, Node>,
) -> Result<(), Error> {
    let mut clocks: FxHashMap<Vec<u8>, Result<Clock, TimingState>> = FxHashMap::default();
    let mut epochs = tx.prepare_cached(
        "SELECT e.epoch_id, e.multiplier, e.shift, s.status, 1 + e.conflict
         FROM epoch e
         LEFT JOIN epoch_state s ON s.rec = e.rec AND s.epoch_id = e.epoch_id
         WHERE e.rec = ?1 AND e.defined = 1",
    )?;
    let mut rows = epochs.query([rec])?;
    while let Some(row) = rows.next()? {
        let clock = functions::epoch_clock(row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?);
        clocks.insert(row.get(0)?, clock);
    }
    drop(rows);
    // A thread without a defined epoch has no conversion.
    let unknown = functions::epoch_clock(None, None, None, 0);

    let mut paths = tx.prepare_cached(
        "SELECT p.call_path_id, p.node_id, t.epoch_id,
           n.call_count, x.call_count, n.errored_calls, x.errored_calls,
           n.cancelled_calls, x.cancelled_calls, n.panicked_calls, x.panicked_calls,
           n.duration_ticks, io.total_ticks,
           n.rec IS NOT NULL, x.rec IS NOT NULL, io.rec IS NOT NULL
         FROM call_path p
         LEFT JOIN aggregate n ON n.rec = p.rec AND n.node = p.call_path_id * 2
         LEFT JOIN aggregate x ON x.rec = p.rec AND x.node = p.call_path_id * 2 + 1
         LEFT JOIN sysop io ON io.rec = p.rec AND io.call_path_id = p.call_path_id
         LEFT JOIN thread t ON t.rec = p.rec AND t.thread_id = p.thread_id
         WHERE p.rec = ?1 AND p.node_id IS NOT NULL",
    )?;
    let mut rows = paths.query([rec])?;
    while let Some(row) = rows.next()? {
        let node = nodes.entry(row.get(1)?).or_insert_with(|| Node {
            sample: (rec, 0),
            invocations: Some(0),
            errored: Some(0),
            cancelled: Some(0),
            panicked: Some(0),
            total_ns: Some(0),
            io_self_ns: Some(0),
            ..Node::default()
        });
        if node.sample.1 == 0 {
            node.sample = (rec, row.get(0)?);
        }
        let clock = match row.get_ref(2)? {
            ValueRef::Blob(epoch) => clocks.get(epoch).copied().unwrap_or(unknown),
            _ => unknown,
        };
        let (normal, reentry, io): (bool, bool, bool) = (row.get(13)?, row.get(14)?, row.get(15)?);
        if normal {
            add(&mut node.invocations, row.get(3)?);
            add(&mut node.errored, row.get(5)?);
            add(&mut node.cancelled, row.get(7)?);
            add(&mut node.panicked, row.get(9)?);
            // Re-entries run inside the normal invocation's time.
            add_ns(&mut node.total_ns, functions::total_ns(clock, row.get(11)?));
        }
        if reentry {
            add(&mut node.invocations, row.get(4)?);
            add(&mut node.errored, row.get(6)?);
            add(&mut node.cancelled, row.get(8)?);
            add(&mut node.panicked, row.get(10)?);
        }
        if io {
            add_ns(
                &mut node.io_self_ns,
                functions::total_ns(clock, row.get(12)?),
            );
        }
    }
    Ok(())
}

/// Each node's own sysop time plus its synchronous children's. A loop, not
/// recursion: the tree is as deep as the recorded program's calls.
fn io_totals(
    nodes: &FxHashMap<i64, Node>,
    children: &FxHashMap<i64, Vec<i64>>,
) -> FxHashMap<i64, Option<i128>> {
    let mut totals: FxHashMap<i64, Option<i128>> =
        FxHashMap::with_capacity_and_hasher(nodes.len(), FxBuildHasher);
    for &start in nodes.keys() {
        let mut stack = vec![(start, false)];
        while let Some((id, visited)) = stack.pop() {
            let kids = children.get(&id).map_or(&[][..], Vec::as_slice);
            if visited {
                let total = kids.iter().fold(nodes[&id].io_self_ns, |sum, kid| {
                    sum.zip(totals.get(kid).copied().flatten())
                        .map(|(a, b)| a + b)
                });
                totals.insert(id, total);
            } else if let Entry::Vacant(entry) = totals.entry(id) {
                // Marked before its children, so even a node ID collision
                // that made a cycle ends.
                entry.insert(None);
                stack.push((id, true));
                stack.extend(kids.iter().map(|kid| (*kid, false)));
            }
        }
    }
    totals
}
