//! Context evidence is independent of call reconciliation and shared by both
//! ingestion paths. No section or file inherits a previous section's context.
//! Announcements are authoritative even when explicitly unavailable. Function
//! completions restore the frame's entry context; future completions restore
//! the launch context. They are fallbacks, never overrides. Running markers
//! supply timing, not another context capture. A network span has only its
//! announcement's context.
use std::collections::BTreeMap;

use btel_reader::{
    cas::{CasOutcome, CasStore},
    context::{ContextReference, reference},
    discovery::FileStamp,
    value::{Nav, Root, Segment, navigate},
};
use btel_recorder::proto::{self, span_event::Event};
use btel_snapshot::{DecodedObject, DecodedRoot, DecodedValue, SnapshotId};
use rusqlite::{Connection, Transaction, params};

use crate::Error;

#[derive(Default)]
pub(super) struct Contexts(BTreeMap<(u64, u8), (u8, ContextReference)>);

impl Contexts {
    pub(super) fn apply(&mut self, file: &proto::RecordingFile) {
        for thread in file.definitions.iter().flat_map(|defs| &defs.threads) {
            self.unknown(thread.thread_id);
        }
        for section in file.spans.iter().flat_map(|spans| &spans.sections) {
            self.unknown(section.thread_id);
            let context = reference(section);
            for event in &section.events {
                match &event.event {
                    Some(Event::FunctionAnnouncement(entry)) => {
                        self.select(entry.id, 0, 2, context);
                        self.select(entry.id, 1, 2, context);
                    }
                    Some(Event::FunctionCompletion(done) | Event::LateFunctionCompletion(done)) => {
                        self.unknown(done.id);
                        // Completions carry the captured entry context, not the
                        // context in effect when the function returned.
                        self.select(done.id, 0, 1, context);
                    }
                    Some(Event::ThreadAnnouncement(_)) => {
                        self.select(section.thread_id, 0, 2, context);
                        self.select(section.thread_id, 1, 2, context);
                    }
                    Some(Event::ThreadCompletion(_)) => {
                        self.select(section.thread_id, 0, 1, context);
                    }
                    // A request's context is the sending frame's, at its
                    // announcement; its events and completion can be read
                    // on other threads, under other contexts.
                    Some(Event::NetworkAnnouncement(span)) => {
                        self.select(span.id, 0, 2, context);
                        self.select(span.id, 1, 2, context);
                    }
                    Some(Event::NetworkEvent(event)) => self.unknown(event.span_id),
                    Some(Event::NetworkCompletion(done)) => self.unknown(done.span_id),
                    _ => {}
                }
            }
        }
    }

    fn unknown(&mut self, node: u64) {
        for slot in [0, 1] {
            self.select(node, slot, 0, ContextReference::Unavailable);
        }
    }

    fn select(&mut self, node: u64, slot: u8, priority: u8, context: ContextReference) {
        let entry = self.0.entry((node, slot)).or_insert((priority, context));
        if priority > entry.0 {
            *entry = (priority, context);
        }
    }

    pub(super) fn write(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let mut snapshot =
            tx.prepare_cached("INSERT OR IGNORE INTO context_snapshot (cas) VALUES (?1)")?;
        let mut event = tx.prepare_cached(
            "INSERT INTO event_context (rec, node_id, slot, priority, state, cas)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (rec, node_id, slot) DO UPDATE SET
               priority = excluded.priority, state = excluded.state, cas = excluded.cas
             WHERE excluded.priority > event_context.priority",
        )?;
        for (&(node, slot), &(priority, context)) in &self.0 {
            let (state, cas) = match context {
                ContextReference::Unavailable => (0, None),
                ContextReference::Empty => (1, None),
                ContextReference::Snapshot(id) => (2, Some(*id.as_bytes())),
                ContextReference::Invalid => (3, None),
            };
            if let Some(cas) = cas {
                snapshot.execute([cas])?;
            }
            event.execute(params![rec, super::id(node), slot, priority, state, cas])?;
        }
        Ok(())
    }
}

pub(super) fn resolve(
    conn: &mut Connection,
    cas: &CasStore,
    options: &super::RefreshOptions,
    facts_changed: bool,
) -> Result<(), Error> {
    resolve_with(conn, cas, options, facts_changed, |id| cas.load(id))
}

fn resolve_with(
    conn: &mut Connection,
    cas: &CasStore,
    options: &super::RefreshOptions,
    facts_changed: bool,
    mut load: impl FnMut(SnapshotId) -> btel_reader::cas::CasLoad,
) -> Result<(), Error> {
    if facts_changed {
        conn.execute(
            "DELETE FROM context_snapshot WHERE NOT EXISTS
             (SELECT 1 FROM event_context WHERE event_context.cas = context_snapshot.cas)",
            [],
        )?;
    }
    let mut after = Vec::<u8>::new();
    loop {
        let pending = conn
            .prepare(
                "SELECT cas, failed_stamp FROM context_snapshot
                 WHERE state = 0 AND cas > ?1
                 ORDER BY cas LIMIT ?2",
            )?
            .query_map(
                params![after, super::saturating(options.batch_files.max(1) as u64)],
                |row| {
                    Ok((
                        row.get::<_, [u8; 16]>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if pending.is_empty() {
            break;
        }
        let mut hydrated = Vec::new();
        let mut bytes = 0u64;
        for (id, failed_stamp) in pending {
            after = id.to_vec();
            let snapshot_id = SnapshotId::from_bytes(id);
            let stamp = std::fs::metadata(cas.path(snapshot_id))
                .ok()
                .map(|metadata| fingerprint(FileStamp::of(&metadata)));
            if !options.verify_contents && stamp.is_some() && stamp == failed_stamp {
                continue;
            }
            let loaded = load(snapshot_id);
            bytes = bytes.saturating_add(loaded.bytes_read);
            let (state, identity, failed_stamp) = match loaded.outcome {
                CasOutcome::Available(snapshot) => {
                    let identity = identity(&snapshot);
                    (
                        if identity.is_ok() { 1 } else { 2 },
                        identity.ok().flatten().map(str::to_owned),
                        None,
                    )
                }
                CasOutcome::Corrupt(_) | CasOutcome::TooLarge { .. } => (0, None, stamp),
                // Missing and transiently unreadable files remain retryable.
                CasOutcome::Missing | CasOutcome::Unreadable(_) => (0, None, None),
            };
            hydrated.push((id, state, identity, failed_stamp));
            if bytes >= options.batch_bytes {
                break;
            }
        }
        if hydrated.is_empty() {
            continue;
        }
        let tx = conn.transaction()?;
        for (id, state, identity, failed_stamp) in hydrated {
            tx.execute(
                "UPDATE context_snapshot
                 SET state = ?2, distinct_id = ?3, failed_stamp = ?4 WHERE cas = ?1",
                params![id, state, identity, failed_stamp],
            )?;
        }
        tx.commit()?;
    }
    Ok(())
}

fn fingerprint(stamp: FileStamp) -> Vec<u8> {
    [
        stamp.size.to_be_bytes().as_slice(),
        stamp.modified_ns.to_be_bytes().as_slice(),
        stamp.device.to_be_bytes().as_slice(),
        stamp.inode.to_be_bytes().as_slice(),
    ]
    .concat()
}

fn identity(snapshot: &btel_snapshot::DecodedSnapshot) -> Result<Option<&str>, ()> {
    let DecodedRoot::Value(DecodedValue::Object(root)) = snapshot.root else {
        return Err(());
    };
    if !matches!(snapshot.object(root), DecodedObject::Map { .. }) {
        return Err(());
    }
    let Nav::Value(DecodedValue::Object(metadata)) = navigate(
        snapshot,
        Root::Value,
        None,
        &[Segment::Key("metadata".into())],
    ) else {
        return Err(());
    };
    let DecodedObject::Map { entries, .. } = snapshot.object(*metadata) else {
        return Err(());
    };
    if !entries.iter().all(|(_, value)| {
        matches!(
            value,
            DecodedValue::String(_)
                | DecodedValue::Int(_)
                | DecodedValue::Float(_)
                | DecodedValue::Bool(_)
        )
    }) {
        return Err(());
    }
    match navigate(
        snapshot,
        Root::Value,
        None,
        &[Segment::Key("distinct_id".into())],
    ) {
        Nav::Value(DecodedValue::String(value)) => Ok(Some(value.as_ref())),
        Nav::Value(DecodedValue::Null) => Ok(None),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use btel_reader::cas::CasLimits;
    use btel_snapshot::{Limits, SnapshotPool};
    use btel_types::context::{Context, ContextPatch};

    use super::*;

    #[test]
    fn unchanged_failed_blobs_are_skipped_and_repairs_recover() {
        let pool = SnapshotPool::new(1, Limits::default());
        let snapshot = btel_snapshot::context::capture(&Context::default(), &pool).unwrap();
        let mut valid = Vec::new();
        snapshot.write_blob(&mut valid).unwrap();
        for (oversized, remove) in [(false, false), (true, false), (false, true), (true, true)] {
            let directory = tempfile::tempdir().unwrap();
            let cas = CasStore::new(
                directory.path().to_owned(),
                CasLimits {
                    max_blob_bytes: valid.len() as u64,
                    ..Default::default()
                },
            );
            let path = cas.path(snapshot.id());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let invalid = vec![0; if oversized { valid.len() + 1 } else { 1 }];
            std::fs::write(&path, invalid).unwrap();
            let mut conn = Connection::open_in_memory().unwrap();
            conn.execute_batch(crate::schema::DDL).unwrap();
            conn.execute(
                "INSERT INTO context_snapshot (cas) VALUES (?1)",
                [snapshot.id().as_bytes()],
            )
            .unwrap();
            let options = super::super::RefreshOptions::default();
            let mut refresh = |options: &super::super::RefreshOptions| {
                let mut loads = 0;
                resolve_with(&mut conn, &cas, options, false, |id| {
                    loads += 1;
                    cas.load(id)
                })
                .unwrap();
                loads
            };
            assert_eq!(refresh(&options), 1);
            assert_eq!(refresh(&options), 0);
            assert_eq!(
                refresh(&super::super::RefreshOptions {
                    verify_contents: true,
                    ..options.clone()
                }),
                1
            );
            assert_eq!(refresh(&options), 0);
            if remove {
                std::fs::remove_file(&path).unwrap();
                assert_eq!(refresh(&options), 1);
            }
            std::fs::write(&path, &valid).unwrap();
            assert_eq!(refresh(&options), 1);
            assert_eq!(refresh(&options), 0);
            let state: i64 = conn
                .query_row("SELECT state FROM context_snapshot", [], |row| row.get(0))
                .unwrap();
            assert_eq!(state, 1);
        }
    }

    #[test]
    fn bounded_groups_do_not_omit_pending_identities() {
        let directory = tempfile::tempdir().unwrap();
        let cas = CasStore::new(directory.path().to_owned(), CasLimits::default());
        let pool = SnapshotPool::new(1, Limits::default());
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::DDL).unwrap();
        for i in 0..5 {
            let context = Context::default().with_patch(&ContextPatch {
                distinct_id: Some(i.to_string()),
                ..Default::default()
            });
            let snapshot = btel_snapshot::context::capture(&context, &pool).unwrap();
            let path = cas.path(snapshot.id());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            snapshot
                .write_blob(&mut std::fs::File::create(path).unwrap())
                .unwrap();
            conn.execute(
                "INSERT INTO context_snapshot (cas) VALUES (?1)",
                [snapshot.id().as_bytes()],
            )
            .unwrap();
        }
        resolve(
            &mut conn,
            &cas,
            &super::super::RefreshOptions {
                batch_files: 2,
                batch_bytes: 1,
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let resolved: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT distinct_id) FROM context_snapshot WHERE state = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved, 5);
    }
}
