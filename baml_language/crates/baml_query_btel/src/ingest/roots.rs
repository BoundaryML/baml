//! `thread.root_id`: the root future each thread runs under. A thread defined
//! without a parent is its own root. A spawned thread has the root of the
//! thread that spawned it: its parent when that is a thread, else the thread
//! of the call it names. Both ingest paths resolve it here, after their
//! writes, from the rows they stored, so they agree whatever order the
//! evidence arrived in: a thread waits, without a root, until its spawner's
//! rows and root are indexed.

use rusqlite::Transaction;

use crate::Error;

/// The root a waiting thread can take now, or NULL.
const ROOT: &str = "CASE
    WHEN defined = 0 THEN NULL
    WHEN parent_id IS NULL THEN thread_id
    ELSE COALESCE(
      (SELECT p.root_id FROM thread p
        WHERE p.rec = thread.rec AND p.thread_id = thread.parent_id),
      (SELECT p.root_id FROM call c
        JOIN thread p ON p.rec = c.rec AND p.thread_id = c.thread_id
        WHERE c.rec = thread.rec AND c.call_id = thread.parent_id))
  END";

/// Give every thread of `rec` whose chain of spawners now reaches a root that
/// root. Each pass reads only the threads still waiting and resolves at
/// least one more level of spawn nesting; a root is never changed once set.
pub(super) fn resolve(tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
    let mut update = tx.prepare_cached(&format!(
        "UPDATE thread INDEXED BY thread_unrooted SET root_id = {ROOT}
         WHERE rec = ?1 AND root_id IS NULL AND {ROOT} IS NOT NULL"
    ))?;
    while update.execute([rec])? > 0 {}
    Ok(())
}
