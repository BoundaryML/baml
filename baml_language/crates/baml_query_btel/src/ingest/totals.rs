//! Incremental `total_usage`: a node's total is its own usage plus its
//! children's totals. Usage newly charged to a node is added to it and to
//! every ancestor already ingested. Where the walk reaches an ancestor not yet
//! ingested, or a thread whose parent is not yet known, the rest waits in
//! `usage_pending` and resumes when a later transaction brings it.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    Error,
    usage::{self, ModelUsage},
};

/// Where a pending amount resumes: at a node not yet ingested, or above a
/// thread whose parent is not yet known.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Waiting {
    AtNode,
    AboveThread,
}

impl Waiting {
    fn code(self) -> i64 {
        match self {
            Waiting::AtNode => 0,
            Waiting::AboveThread => 1,
        }
    }

    fn from_code(code: i64) -> Self {
        if code == 0 {
            Waiting::AtNode
        } else {
            Waiting::AboveThread
        }
    }
}

struct Node {
    table: &'static str,
    parent: Option<[u8; 8]>,
    total: Vec<ModelUsage>,
    /// A thread placeholder: its parent is not known yet.
    undefined_thread: bool,
}

/// Add `charged` (node id, its newly stored usage) to the totals, and resume
/// what earlier transactions left pending.
pub(super) fn apply(
    tx: &Transaction<'_>,
    rec: i64,
    charged: &[([u8; 8], Vec<ModelUsage>)],
) -> Result<(), Error> {
    let mut pending: BTreeMap<([u8; 8], Waiting), Vec<ModelUsage>> = BTreeMap::new();
    let resumed = {
        let mut select =
            tx.prepare_cached("SELECT node, waiting, usage FROM usage_pending WHERE rec = ?1")?;
        let rows = select.query_map([rec], |r| {
            Ok((
                r.get::<_, [u8; 8]>(0)?,
                Waiting::from_code(r.get(1)?),
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if resumed.is_empty() && charged.is_empty() {
        return Ok(());
    }
    if !resumed.is_empty() {
        tx.prepare_cached("DELETE FROM usage_pending WHERE rec = ?1")?
            .execute([rec])?;
    }
    for (node, waiting, text) in resumed {
        let amount = stored(&text)?;
        match waiting {
            Waiting::AtNode => walk(tx, rec, node, &amount, &mut pending)?,
            Waiting::AboveThread => match lookup(tx, rec, node)? {
                Some(Node {
                    parent: Some(parent),
                    ..
                }) => walk(tx, rec, parent, &amount, &mut pending)?,
                Some(Node {
                    undefined_thread: true,
                    ..
                }) => usage::merge(
                    pending.entry((node, Waiting::AboveThread)).or_default(),
                    &amount,
                ),
                _ => {}
            },
        }
    }
    for (node, amount) in charged {
        walk(tx, rec, *node, amount, &mut pending)?;
    }
    let mut insert = tx.prepare_cached(
        "INSERT INTO usage_pending (rec, node, waiting, usage) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for ((node, waiting), amount) in pending {
        insert.execute(params![rec, node, waiting.code(), usage::to_json(&amount)])?;
    }
    Ok(())
}

/// Add `amount` to `node` and each ancestor up to the first one not yet
/// ingested, or not yet linked to its parent, where the rest waits.
fn walk(
    tx: &Transaction<'_>,
    rec: i64,
    mut node: [u8; 8],
    amount: &[ModelUsage],
    pending: &mut BTreeMap<([u8; 8], Waiting), Vec<ModelUsage>>,
) -> Result<(), Error> {
    loop {
        let Some(mut row) = lookup(tx, rec, node)? else {
            usage::merge(pending.entry((node, Waiting::AtNode)).or_default(), amount);
            return Ok(());
        };
        usage::merge(&mut row.total, amount);
        let key = if row.table == "call" {
            "call_id"
        } else {
            "thread_id"
        };
        tx.prepare_cached(&format!(
            "UPDATE {} SET total_usage = ?3 WHERE rec = ?1 AND {key} = ?2",
            row.table
        ))?
        .execute(params![rec, node, usage::to_json(&row.total)])?;
        match row.parent {
            Some(parent) => node = parent,
            None => {
                if row.undefined_thread {
                    usage::merge(
                        pending.entry((node, Waiting::AboveThread)).or_default(),
                        amount,
                    );
                }
                return Ok(());
            }
        }
    }
}

fn lookup(tx: &Transaction<'_>, rec: i64, node: [u8; 8]) -> Result<Option<Node>, Error> {
    let call = tx
        .prepare_cached("SELECT parent_id, total_usage FROM call WHERE rec = ?1 AND call_id = ?2")?
        .query_row(params![rec, node], |r| {
            Ok((
                r.get::<_, Option<[u8; 8]>>(0)?,
                r.get::<_, Option<String>>(1)?,
            ))
        })
        .optional()?;
    if let Some((parent, total)) = call {
        return Ok(Some(Node {
            table: "call",
            parent,
            total: total
                .as_deref()
                .map(stored)
                .transpose()?
                .unwrap_or_default(),
            undefined_thread: false,
        }));
    }
    let thread = tx
        .prepare_cached(
            "SELECT parent_id, total_usage, defined FROM thread WHERE rec = ?1 AND thread_id = ?2",
        )?
        .query_row(params![rec, node], |r| {
            Ok((
                r.get::<_, Option<[u8; 8]>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })
        .optional()?;
    Ok(thread
        .map(|(parent, total, defined)| -> Result<Node, Error> {
            Ok(Node {
                table: "thread",
                parent,
                total: total
                    .as_deref()
                    .map(stored)
                    .transpose()?
                    .unwrap_or_default(),
                undefined_thread: !defined && parent.is_none(),
            })
        })
        .transpose()?)
}

fn stored(text: &str) -> Result<Vec<ModelUsage>, Error> {
    usage::parse(text).map_err(|e| Error::Io(format!("invalid stored usage: {e}")))
}
