//! Recorded PCs to source sites, after each applied batch. A site depends on
//! the function's recorded definition, which can arrive in a later file, so
//! rows are (re)resolved whenever they or their function's definition change.
//! Only the recording's own maps are used, never today's source.
use std::collections::{HashMap, HashSet};

use btel_reader::source_map::{SiteState, SourceMap};
use rusqlite::{OptionalExtension as _, Transaction, params};

use super::id;
use crate::Error;

/// Rows touched by one batch.
#[derive(Default)]
pub(super) struct Pending {
    pub(super) paths: HashSet<u32>,
    /// Functions whose metadata was indexed in this batch.
    pub(super) functions: HashSet<u64>,
    /// Functions whose metadata turned out conflicted in this batch: every
    /// site in them changes, resolved or not.
    pub(super) conflicted: HashSet<u64>,
    /// Newly defined raises: their site and stack frames.
    pub(super) raises: HashSet<u64>,
    /// Raises whose unwind end arrived: their handler site.
    pub(super) ends: HashSet<u64>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.paths.is_empty()
            && self.functions.is_empty()
            && self.conflicted.is_empty()
            && self.raises.is_empty()
            && self.ends.is_empty()
    }
}

/// What a site needs from its function's indexed definition.
pub(super) struct Source {
    pub(super) state: i64,
    pub(super) conflict: bool,
    pub(super) file: Option<String>,
    pub(super) map_state: Option<i64>,
    pub(super) map: Option<SourceMap>,
}

#[derive(Clone)]
pub(super) struct Site {
    pub(super) state: &'static str,
    pub(super) file: Option<String>,
    pub(super) line: Option<i64>,
    pub(super) start: Option<i64>,
    pub(super) end: Option<i64>,
}

impl Site {
    pub(super) fn missing(state: SiteState) -> Self {
        Self {
            state: state.label(),
            file: None,
            line: None,
            start: None,
            end: None,
        }
    }
}

/// The site of `pc` in a function whose definition is `source` (`None`:
/// not indexed at all). Callers handle a site with no function recorded.
pub(super) fn site_in(source: Option<&Source>, pc: Option<i64>) -> Site {
    let Some(source) = source else {
        return Site::missing(SiteState::FunctionUnresolved);
    };
    let state = match (source.state, source.conflict, source.map_state) {
        (0, ..) => SiteState::FunctionUnresolved,
        (1, ..) => SiteState::FunctionUnavailable,
        (_, true, _) => SiteState::FunctionConflicted,
        (_, _, None) => SiteState::NoSourceMap,
        (_, _, Some(2)) => SiteState::InvalidSourceMap,
        _ if source.file.is_none() => SiteState::NoSourceFile,
        _ => SiteState::Resolved,
    };
    if state != SiteState::Resolved {
        return Site::missing(state);
    }
    let Some(map) = &source.map else {
        return Site::missing(SiteState::InvalidSourceMap);
    };
    let Some(pc) = pc else {
        return Site::missing(SiteState::NoPc);
    };
    let resolved = u32::try_from(pc)
        .map_err(|_| SiteState::PcOutOfRange)
        .and_then(|pc| map.resolve(pc));
    match resolved {
        Ok(site) => Site {
            state: SiteState::Resolved.label(),
            file: source.file.clone(),
            line: Some(i64::from(site.line)),
            start: Some(i64::from(site.start)),
            end: Some(i64::from(site.end)),
        },
        Err(state) => Site::missing(state),
    }
}

struct Resolver<'t> {
    tx: &'t Transaction<'t>,
    rec: i64,
    sources: HashMap<Vec<u8>, Option<Source>>,
}

impl Resolver<'_> {
    fn source(&mut self, function: &[u8]) -> Result<Option<&Source>, Error> {
        if !self.sources.contains_key(function) {
            let source = self
                .tx
                .prepare_cached(
                    "SELECT state, conflict, source_file, source_map_state, source_map
                     FROM function_def WHERE rec = ?1 AND function_id = ?2",
                )?
                .query_row(params![self.rec, function], |r| {
                    let blob: Option<Vec<u8>> = r.get(4)?;
                    Ok(Source {
                        state: r.get(0)?,
                        conflict: r.get::<_, i64>(1)? != 0,
                        file: r.get(2)?,
                        map_state: r.get(3)?,
                        map: blob.as_deref().and_then(SourceMap::decode),
                    })
                })
                .optional()?;
            self.sources.insert(function.to_vec(), source);
        }
        Ok(self.sources[function].as_ref())
    }

    /// `absent` is the state when no function was recorded at all.
    fn site(
        &mut self,
        function: Option<&[u8]>,
        pc: Option<i64>,
        absent: SiteState,
    ) -> Result<Site, Error> {
        let Some(function) = function else {
            return Ok(Site::missing(absent));
        };
        Ok(site_in(self.source(function)?, pc))
    }
}

fn ids(
    tx: &Transaction<'_>,
    sql: &str,
    rec: i64,
    functions: &HashSet<u64>,
    into: &mut HashSet<Vec<u8>>,
) -> Result<(), Error> {
    let mut statement = tx.prepare_cached(sql)?;
    for function in functions {
        let rows = statement.query_map(params![rec, id(*function)], |r| r.get::<_, Vec<u8>>(0))?;
        for row in rows {
            into.insert(row?);
        }
    }
    Ok(())
}

/// An unwind end's result, handler function and handler PC.
type EndRow = (Option<i64>, Option<Vec<u8>>, Option<i64>);

pub(super) fn resolve(tx: &Transaction<'_>, rec: i64, pending: &Pending) -> Result<(), Error> {
    if pending.is_empty() {
        return Ok(());
    }
    let mut resolver = Resolver {
        tx,
        rec,
        sources: HashMap::new(),
    };

    // Call and spawn sites, in the caller's map.
    let mut paths: HashSet<i64> = pending.paths.iter().map(|p| i64::from(*p)).collect();
    {
        let mut unsited = tx.prepare_cached(
            "SELECT call_path_id FROM call_path INDEXED BY call_path_unsited
             WHERE rec = ?1 AND caller_function_id = ?2 AND site_state != 'resolved'",
        )?;
        for function in &pending.functions {
            for path in unsited.query_map(params![rec, id(*function)], |r| r.get::<_, i64>(0))? {
                paths.insert(path?);
            }
        }
        let mut every = tx.prepare_cached(
            "SELECT call_path_id FROM call_path WHERE rec = ?1 AND caller_function_id = ?2",
        )?;
        for function in &pending.conflicted {
            for path in every.query_map(params![rec, id(*function)], |r| r.get::<_, i64>(0))? {
                paths.insert(path?);
            }
        }
    }
    let functions: HashSet<u64> = pending
        .functions
        .union(&pending.conflicted)
        .copied()
        .collect();
    for path in paths {
        let row: Option<(i64, Option<Vec<u8>>, Option<i64>)> = tx
            .prepare_cached(
                "SELECT defined, caller_function_id, caller_pc FROM call_path
                 WHERE rec = ?1 AND call_path_id = ?2",
            )?
            .query_row(params![rec, path], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?;
        let Some((1, caller, pc)) = row else {
            continue;
        };
        let site = resolver.site(caller.as_deref(), pc, SiteState::NoCaller)?;
        tx.prepare_cached(
            "UPDATE call_path SET site_state = ?3, site_file = ?4, site_line = ?5,
               site_start = ?6, site_end = ?7
             WHERE rec = ?1 AND call_path_id = ?2",
        )?
        .execute(params![
            rec, path, site.state, site.file, site.line, site.start, site.end
        ])?;
    }

    // Raise sites and their stacks.
    let mut raises: HashSet<Vec<u8>> = pending.raises.iter().map(|r| id(*r).to_vec()).collect();
    ids(
        tx,
        "SELECT raise_id FROM error_raise INDEXED BY error_raise_by_function
         WHERE rec = ?1 AND function_id = ?2",
        rec,
        &functions,
        &mut raises,
    )?;
    for raise in &raises {
        let row: Option<(i64, Option<Vec<u8>>, Option<i64>)> = tx
            .prepare_cached(
                "SELECT defined, function_id, pc FROM error_raise WHERE rec = ?1 AND raise_id = ?2",
            )?
            .query_row(params![rec, raise], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?;
        let Some((1, function, pc)) = row else {
            continue;
        };
        let absent = if pc.is_some() {
            SiteState::NoFunction
        } else {
            SiteState::NoPc
        };
        let site = resolver.site(function.as_deref(), pc, absent)?;
        tx.prepare_cached(
            "UPDATE error_raise SET site_state = ?3, site_file = ?4, site_line = ?5,
               site_start = ?6, site_end = ?7
             WHERE rec = ?1 AND raise_id = ?2",
        )?
        .execute(params![
            rec, raise, site.state, site.file, site.line, site.start, site.end
        ])?;
    }

    // Stack frames: of new raises, and naming newly defined functions.
    let mut frames: HashSet<(Vec<u8>, i64)> = HashSet::new();
    {
        let mut of_raise =
            tx.prepare_cached("SELECT position FROM error_frame WHERE rec = ?1 AND raise_id = ?2")?;
        for raise in &pending.raises {
            for position in of_raise.query_map(params![rec, id(*raise)], |r| r.get::<_, i64>(0))? {
                frames.insert((id(*raise).to_vec(), position?));
            }
        }
        let mut of_function = tx.prepare_cached(
            "SELECT raise_id, position FROM error_frame INDEXED BY error_frame_by_function
             WHERE rec = ?1 AND function_id = ?2",
        )?;
        for function in &functions {
            for row in of_function.query_map(params![rec, id(*function)], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?))
            })? {
                frames.insert(row?);
            }
        }
    }
    for (raise, position) in frames {
        let row: Option<(Option<Vec<u8>>, Option<i64>, i64)> = tx
            .prepare_cached(
                "SELECT function_id, pc, native FROM error_frame
                 WHERE rec = ?1 AND raise_id = ?2 AND position = ?3",
            )?
            .query_row(params![rec, raise, position], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?;
        let Some((function, pc, native)) = row else {
            continue;
        };
        let site = if native == 1 {
            // Native code has no BAML source; its caller frame has the site.
            Site::missing(SiteState::NoPc)
        } else {
            resolver.site(function.as_deref(), pc, SiteState::NoFunction)?
        };
        tx.prepare_cached(
            "UPDATE error_frame SET site_state = ?4, site_file = ?5, site_line = ?6,
               site_start = ?7, site_end = ?8
             WHERE rec = ?1 AND raise_id = ?2 AND position = ?3",
        )?
        .execute(params![
            rec, raise, position, site.state, site.file, site.line, site.start, site.end
        ])?;
    }

    // Handler sites, once an end names the catching frame.
    let mut ends: HashSet<Vec<u8>> = pending.ends.iter().map(|r| id(*r).to_vec()).collect();
    ids(
        tx,
        "SELECT raise_id FROM error_raise INDEXED BY error_raise_by_handler
         WHERE rec = ?1 AND handler_function_id = ?2",
        rec,
        &functions,
        &mut ends,
    )?;
    for raise in ends {
        let row: Option<EndRow> = tx
            .prepare_cached(
                "SELECT end_result, handler_function_id, handler_pc FROM error_raise
                 WHERE rec = ?1 AND raise_id = ?2",
            )?
            .query_row(params![rec, raise], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?;
        let Some((Some(_), function, pc)) = row else {
            continue;
        };
        let site = if function.is_none() && pc.is_none() {
            None
        } else {
            Some(resolver.site(function.as_deref(), pc, SiteState::NoFunction)?)
        };
        tx.prepare_cached(
            "UPDATE error_raise SET handler_site_state = ?3, handler_site_file = ?4,
               handler_site_line = ?5, handler_site_start = ?6, handler_site_end = ?7
             WHERE rec = ?1 AND raise_id = ?2",
        )?
        .execute(params![
            rec,
            raise,
            site.as_ref().map(|s| s.state),
            site.as_ref().and_then(|s| s.file.clone()),
            site.as_ref().and_then(|s| s.line),
            site.as_ref().and_then(|s| s.start),
            site.as_ref().and_then(|s| s.end)
        ])?;
    }
    Ok(())
}
