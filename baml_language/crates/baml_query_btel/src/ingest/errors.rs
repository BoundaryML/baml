//! Error evidence: raises, their stacks, unwind ends and call links. Each is
//! validated on its own; a malformed record becomes an issue and is skipped,
//! never a reason to reject the file's other evidence.
use btel_recorder::proto;
use prost::Message as _;
use rusqlite::{OptionalExtension as _, params};

use super::{Applier, id, sequence_i64};
use crate::Error;

fn valid_enum(value: i32, max: i32) -> bool {
    (1..=max).contains(&value)
}

impl Applier<'_> {
    pub(super) fn errors(
        &mut self,
        sequence: u64,
        batch: &proto::ErrorBatch,
        tick: &mut impl FnMut(u64) -> Option<i64>,
    ) -> Result<(), Error> {
        for raise in &batch.raises {
            self.raise(sequence, raise, tick)?;
        }
        for link in &batch.call_links {
            self.link(sequence, link)?;
        }
        for end in &batch.unwind_ends {
            self.unwind_end(sequence, end)?;
        }
        Ok(())
    }

    fn invalid_error(&self, sequence: u64, what: &str) -> Result<(), Error> {
        self.issue(
            sequence,
            "error_evidence_invalid",
            None,
            &format!("a malformed {what} was skipped; other evidence in the file was applied"),
        )
    }

    fn raise(
        &mut self,
        sequence: u64,
        raise: &proto::ErrorRaise,
        tick: &mut impl FnMut(u64) -> Option<i64>,
    ) -> Result<(), Error> {
        let origin_ok = match raise.origin_state {
            1 => raise.origin_raise_id.is_none(),
            2 => raise.origin_raise_id.is_some_and(|id| id != 0),
            3 | 4 => raise.origin_raise_id.is_none(),
            _ => false,
        };
        if raise.raise_id == 0
            || raise.thread_id == 0
            || !valid_enum(raise.kind, 8)
            || !origin_ok
            || raise.function_id == Some(0)
            || raise.frames.len() > usize::try_from(raise.frame_count).unwrap_or(usize::MAX)
            // A call path names the stack; explicit frames replace it.
            || raise.call_path_id == Some(0)
            || (raise.call_path_id.is_some() && !raise.frames.is_empty())
        {
            return self.invalid_error(sequence, "error raise");
        }
        let raise_id = id(raise.raise_id);
        let encoded = raise.encode_to_vec();
        let existing: Option<(i64, Option<Vec<u8>>)> = self
            .tx
            .prepare_cached(
                "SELECT defined, evidence FROM error_raise WHERE rec = ?1 AND raise_id = ?2",
            )?
            .query_row(params![self.rec, raise_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if let Some((1, previous)) = existing {
            if previous.as_deref() != Some(encoded.as_slice()) {
                self.tx
                    .prepare_cached(
                        "UPDATE error_raise SET conflict = 1 WHERE rec = ?1 AND raise_id = ?2",
                    )?
                    .execute(params![self.rec, raise_id])?;
                self.issue(
                    sequence,
                    "error_raise_conflict",
                    Some(&raise.raise_id.to_string()),
                    "two different records for one raise; the first is kept",
                )?;
            }
            return Ok(());
        }
        self.references.threads.insert(raise.thread_id);
        self.references.functions.extend(raise.function_id);
        if let Some(path) = raise.call_path_id {
            self.references.paths.insert(path);
            self.raise_paths.insert(path);
        }
        let inherited = (!raise.inherited_frames.is_empty()).then(|| {
            serde_json::Value::Array(
                raise
                    .inherited_frames
                    .iter()
                    .map(|frame| {
                        serde_json::json!({
                            "function": frame.function_name,
                            "file": frame.file,
                            "line": (frame.line != 0).then_some(frame.line),
                        })
                    })
                    .collect(),
            )
            .to_string()
        });
        self.tx
            .prepare_cached(
                "INSERT INTO error_raise (rec, raise_id, defined, sequence, thread_id, raised_ticks,
                   kind, function_id, pc, origin_state, origin_raise_id, previous_raise_id,
                   origin_via, origin_candidates, unresolved_reason, frame_count, inherited_count,
                   inherited, evidence, call_path_id)
                 VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                   ?17, ?18, ?19)
                 ON CONFLICT (rec, raise_id) DO UPDATE SET defined = 1,
                   sequence = excluded.sequence, thread_id = excluded.thread_id,
                   raised_ticks = excluded.raised_ticks, kind = excluded.kind,
                   function_id = excluded.function_id, pc = excluded.pc,
                   origin_state = excluded.origin_state, origin_raise_id = excluded.origin_raise_id,
                   previous_raise_id = excluded.previous_raise_id,
                   origin_via = excluded.origin_via,
                   origin_candidates = excluded.origin_candidates,
                   unresolved_reason = excluded.unresolved_reason,
                   frame_count = excluded.frame_count, inherited_count = excluded.inherited_count,
                   inherited = excluded.inherited, evidence = excluded.evidence,
                   call_path_id = excluded.call_path_id",
            )?
            .execute(params![
                self.rec,
                raise_id,
                sequence_i64(sequence)?,
                id(raise.thread_id),
                tick(raise.raised_at_ticks),
                raise.kind,
                raise.function_id.map(id),
                raise.pc,
                raise.origin_state,
                raise.origin_raise_id.map(id),
                raise.previous_raise_id.filter(|id| *id != 0).map(id),
                (raise.origin_via != 0).then_some(raise.origin_via),
                (raise.origin_state == 3).then_some(raise.origin_candidates),
                (raise.origin_state == 4).then_some(raise.unresolved_reason),
                raise.frame_count,
                raise.inherited_frame_count,
                inherited,
                encoded,
                raise.call_path_id.map(i64::from)
            ])?;
        let mut frame = self.tx.prepare_cached(
            "INSERT OR REPLACE INTO error_frame (rec, raise_id, position, function_id, pc, native)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (position, entry) in raise.frames.iter().enumerate() {
            let function = entry.function_id.filter(|id| *id != 0);
            self.references.functions.extend(function);
            frame.execute(params![
                self.rec,
                raise_id,
                i64::try_from(position).unwrap_or(i64::MAX),
                function.map(id),
                entry.pc,
                entry.native
            ])?;
        }
        self.sites.raises.insert(raise.raise_id);
        Ok(())
    }

    fn link(&mut self, sequence: u64, link: &proto::ErrorCallLink) -> Result<(), Error> {
        if link.raise_id == 0 || link.call_id == 0 || !valid_enum(link.role, 2) {
            return self.invalid_error(sequence, "error call link");
        }
        let inserted = self
            .tx
            .prepare_cached(
                "INSERT OR IGNORE INTO error_link (rec, raise_id, call_id, role, sequence)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?
            .execute(params![
                self.rec,
                id(link.raise_id),
                id(link.call_id),
                link.role,
                sequence_i64(sequence)?
            ])?;
        if inserted == 0 && link.role == 2 {
            // A call completes once; a second raise claiming it is a conflict.
            let owner: Option<Vec<u8>> = self
                .tx
                .prepare_cached(
                    "SELECT raise_id FROM error_link WHERE rec = ?1 AND call_id = ?2 AND role = 2",
                )?
                .query_row(params![self.rec, id(link.call_id)], |r| r.get(0))
                .optional()?;
            if owner.as_deref() != Some(id(link.raise_id).as_slice()) {
                self.issue(
                    sequence,
                    "error_link_conflict",
                    Some(&link.call_id.to_string()),
                    "two raises claim to have failed one call; the first is kept",
                )?;
            }
        }
        Ok(())
    }

    fn unwind_end(&mut self, sequence: u64, end: &proto::ErrorUnwindEnd) -> Result<(), Error> {
        let caught = end.result == 1;
        if end.raise_id == 0
            || !valid_enum(end.result, 4)
            || end.handler_function_id == Some(0)
            || caught != end.handler_pc.is_some()
        {
            return self.invalid_error(sequence, "unwind end");
        }
        let raise_id = id(end.raise_id);
        self.references.functions.extend(end.handler_function_id);
        // An end whose raise is not indexed keeps a placeholder row.
        self.tx
            .prepare_cached(
                "INSERT OR IGNORE INTO error_raise (rec, raise_id, defined) VALUES (?1, ?2, 0)",
            )?
            .execute(params![self.rec, raise_id])?;
        let updated = self
            .tx
            .prepare_cached(
                "UPDATE error_raise SET end_sequence = ?3, end_result = ?4,
                   handler_function_id = ?5, handler_pc = ?6, unwound_frames = ?7
                 WHERE rec = ?1 AND raise_id = ?2 AND end_sequence IS NULL",
            )?
            .execute(params![
                self.rec,
                raise_id,
                sequence_i64(sequence)?,
                end.result,
                end.handler_function_id.map(id),
                end.handler_pc,
                end.unwound_frames
            ])?;
        if updated == 0 {
            let same: bool = self
                .tx
                .prepare_cached(
                    "SELECT end_result = ?3 AND handler_function_id IS ?4 AND handler_pc IS ?5
                       AND unwound_frames = ?6
                     FROM error_raise WHERE rec = ?1 AND raise_id = ?2",
                )?
                .query_row(
                    params![
                        self.rec,
                        raise_id,
                        end.result,
                        end.handler_function_id.map(id),
                        end.handler_pc,
                        end.unwound_frames
                    ],
                    |r| r.get(0),
                )?;
            if !same {
                self.tx
                    .prepare_cached(
                        "UPDATE error_raise SET end_conflict = 1 WHERE rec = ?1 AND raise_id = ?2",
                    )?
                    .execute(params![self.rec, raise_id])?;
                self.issue(
                    sequence,
                    "error_end_conflict",
                    Some(&end.raise_id.to_string()),
                    "a raise ended twice with different evidence; the first is kept",
                )?;
            }
            return Ok(());
        }
        self.sites.ends.insert(end.raise_id);
        Ok(())
    }
}

/// Callers listed per raise path; with the raising frame, `MAX_ERROR_FRAMES`.
const MAX_PATH_CALLERS: usize = 63;

/// List the callers of raise paths whose walk is now possible: every path
/// from the raising one up to the thread's first frame is defined. Runs when
/// this batch named new raise paths or defined call paths; each path is
/// built once, however many raises name it.
pub(super) fn build_raise_paths(
    tx: &rusqlite::Transaction<'_>,
    rec: i64,
    new_paths: &std::collections::HashSet<u32>,
    touched_paths: bool,
) -> Result<(), Error> {
    for path in new_paths {
        tx.prepare_cached(
            "INSERT OR IGNORE INTO raise_path (rec, call_path_id, built) VALUES (?1, ?2, 0)",
        )?
        .execute(params![rec, i64::from(*path)])?;
    }
    if new_paths.is_empty() && !touched_paths {
        return Ok(());
    }
    let unbuilt: Vec<i64> = tx
        .prepare_cached(
            "SELECT call_path_id FROM raise_path INDEXED BY raise_path_unbuilt
             WHERE rec = ?1 AND built = 0",
        )?
        .query_map([rec], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut step = tx.prepare_cached(
        "SELECT defined, parent_call_path_id, caller_function_id IS NOT NULL, edge
         FROM call_path WHERE rec = ?1 AND call_path_id = ?2",
    )?;
    for path in unbuilt {
        // Each frame is the caller named by a path; the walk ends at a path
        // without a caller (a thread's first frame) or at a spawn edge.
        let mut frames = Vec::new();
        let mut current = path;
        let complete = loop {
            let row: Option<(i64, Option<i64>, bool, Option<i64>)> = step
                .query_row(params![rec, current], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .optional()?;
            let Some((1, parent, has_caller, edge)) = row else {
                break false;
            };
            if !has_caller || edge != Some(1) {
                break true;
            }
            frames.push(current);
            match parent {
                Some(parent) if parent != 0 && frames.len() < MAX_PATH_CALLERS => {
                    current = parent;
                }
                _ => break true,
            }
        };
        if !complete {
            continue;
        }
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO raise_path_frame (rec, call_path_id, position, frame_path_id)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (index, frame) in frames.iter().enumerate() {
            insert.execute(params![
                rec,
                path,
                i64::try_from(index + 1).unwrap_or(i64::MAX),
                frame
            ])?;
        }
        tx.prepare_cached("UPDATE raise_path SET built = 1 WHERE rec = ?1 AND call_path_id = ?2")?
            .execute(params![rec, path])?;
    }
    Ok(())
}
