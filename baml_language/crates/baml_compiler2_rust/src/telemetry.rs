//! Which functions record telemetry, and the table that names them.
//!
//! Logging every call would put back the calls inlining removes, which is
//! where native code's speed comes from. So the backend partitions the
//! program's functions when it compiles them (the Notion "telemetry" design):
//!
//! - an **observed** function is emitted `#[inline(never)]` and runs the
//!   VM's producer sequence around its body (`bex_aot::telemetry::enter` /
//!   `Frame::exit`): a timing completion per call at `medium`, a span at
//!   `high`;
//! - every other function is **folded**: no instrumentation, free to be
//!   inlined, and its time belongs to its nearest observed ancestor, as a
//!   `trace.hidden` helper's does on the VM. The table marks it `inlined`.
//!
//! A function is observed when its body loops or its MIR is larger than
//! [`OBSERVED_STATEMENTS`]: work whose cost dominates the producer's (about
//! 30 ns), and so worth a record. A folded entry point still has its run
//! recorded, as the root thread span the host's call opens.
//! [`Observe::All`] observes every function: what `high` means for a
//! program compiled to be profiled. (The design's other two rules, an
//! explicit trace policy and reaching a sys op, await or LLM call, select
//! nothing yet: the backend admits none of those.)
//!
//! Every call site that can reach BAML code names itself before the call
//! (`bex_aot::telemetry::site`): a call path is keyed, as on the VM, by the
//! physical caller and its site, an index into the caller's source map
//! (`PC_COORDINATE_COMPILED_SITE`).

use baml_base::{SourceFile, Span};
use baml_compiler2_mir::{BlockId, Terminator};
use proc_macro2::TokenStream;
use quote::quote;
use rustc_hash::FxHashMap;

use crate::{
    Rejection,
    function::{Candidate, FnId},
    structure::Stmt,
};

/// The MIR size above which a function without a loop is observed: about a
/// screenful of straight-line code, where the producer's cost is no longer
/// the larger part of a call.
pub(crate) const OBSERVED_STATEMENTS: usize = 64;

/// Which functions the program observes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Observe {
    /// Functions that loop and large functions; the rest are folded.
    #[default]
    Default,
    /// Every function, to profile the program.
    All,
}

/// One BAML function in the table: generic instances share their
/// declaration's entry.
#[derive(Debug)]
struct Entry {
    fqn: String,
    observed: bool,
    file: String,
    file_id: u32,
    start: u32,
    end: u32,
    /// Call sites by site index: their source span and one-based line.
    sites: Vec<(u32, u32, u32)>,
}

/// The program's telemetry table, and where each candidate's sites are.
#[derive(Debug, Default)]
pub(crate) struct Plan<'db> {
    entries: Vec<Entry>,
    index: FxHashMap<FnId<'db>, u32>,
    /// Per candidate, the site index of each call terminator, by block.
    sites: FxHashMap<FnId<'db>, FxHashMap<BlockId, u32>>,
}

impl<'db> Plan<'db> {
    pub(crate) fn new(
        db: &'db dyn baml_compiler2_mir::Db,
        candidates: &[Candidate<'db>],
        observe: Observe,
    ) -> Result<Self, Rejection> {
        let mut plan = Self::default();
        let mut by_fqn: FxHashMap<String, u32> = FxHashMap::default();
        for candidate in candidates {
            let fqn = match &candidate.id {
                FnId::Declared(_) => crate::link_name(db, candidate.id.root().loc),
                FnId::Lambda { .. } => candidate.link_name.clone(),
            };
            let observed = observe == Observe::All
                || loops(&candidate.structured)
                || statements(candidate) > OBSERVED_STATEMENTS;
            let file = crate::function_file(db, candidate.id.root().loc);
            let index = match by_fqn.get(&fqn) {
                Some(&index) => {
                    // Another instance of a generic function: observed if
                    // any instance is.
                    plan.entries[index as usize].observed |= observed;
                    index
                }
                None => {
                    let index = u32::try_from(plan.entries.len())
                        .map_err(|_| Rejection::unsupported("more than 2^32 functions"))?;
                    let (start, end) = candidate.mir.span.map_or((0, 0), |span| {
                        (span.range.start().into(), span.range.end().into())
                    });
                    plan.entries.push(Entry {
                        fqn: fqn.clone(),
                        observed,
                        file: file.relative_path(db),
                        file_id: file.file_id(db).as_u32(),
                        start,
                        end,
                        sites: Vec::new(),
                    });
                    by_fqn.insert(fqn, index);
                    index
                }
            };
            plan.index.insert(candidate.id.clone(), index);
            // Every call terminator is a site, numbered by its span within
            // the BAML function, so all instances agree.
            let entry = &mut plan.entries[index as usize];
            let mut sites = FxHashMap::default();
            for block in &candidate.body.blocks {
                if !matches!(
                    block.terminator,
                    Some(Terminator::Call { .. } | Terminator::VirtualCall { .. })
                ) {
                    continue;
                }
                let (start, end, line) = block
                    .terminator_span
                    .map_or((0, 0, 0), |span| site_source(db, file, span));
                let site = match entry
                    .sites
                    .iter()
                    .position(|site| *site == (start, end, line))
                {
                    Some(site) => site,
                    None => {
                        entry.sites.push((start, end, line));
                        entry.sites.len() - 1
                    }
                };
                let site = u32::try_from(site)
                    .map_err(|_| Rejection::unsupported("more than 2^32 call sites"))?;
                sites.insert(block.id, site);
            }
            plan.sites.insert(candidate.id.clone(), sites);
        }
        Ok(plan)
    }

    /// The candidate's index in the table, and whether it is observed.
    pub(crate) fn function(&self, id: &FnId<'db>) -> Result<(u32, bool), Rejection> {
        let index = *self
            .index
            .get(id)
            .ok_or_else(|| Rejection::invalid("function missing from the telemetry table"))?;
        Ok((index, self.entries[index as usize].observed))
    }

    /// The site index of the call terminator ending `block`.
    pub(crate) fn site(&self, id: &FnId<'db>, block: BlockId) -> Result<u32, Rejection> {
        self.sites
            .get(id)
            .and_then(|sites| sites.get(&block))
            .copied()
            .ok_or_else(|| Rejection::invalid(format!("{block} is not a call site")))
    }

    /// The table, as `pub static BAML_TELEMETRY`: by function index, what
    /// `bex_aot::telemetry::start` publishes as the recording's functions.
    pub(crate) fn render(&self) -> TokenStream {
        let entries = self.entries.iter().map(|entry| {
            let Entry {
                fqn,
                observed,
                file,
                file_id,
                start,
                end,
                sites,
            } = entry;
            let sites = sites.iter().map(|(start, end, line)| {
                quote! { bex_aot::telemetry::Site { start: #start, end: #end, line: #line } }
            });
            quote! {
                bex_aot::telemetry::Function {
                    fqn: #fqn,
                    observed: #observed,
                    file: #file,
                    file_id: #file_id,
                    start: #start,
                    end: #end,
                    sites: &[#(#sites),*],
                }
            }
        });
        quote! {
            /// Every compiled BAML function, by telemetry index: the table
            /// `bex_aot::telemetry::start` records.
            pub static BAML_TELEMETRY: &[bex_aot::telemetry::Function] = &[#(#entries),*];
        }
    }
}

/// Whether the structured body has a loop.
fn loops(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Loop { .. } => true,
        Stmt::Block { body, .. } => loops(body),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => loops(then_branch) || loops(else_branch),
        Stmt::Switch { arms, .. } => arms.iter().any(|(_, body)| loops(body)),
        _ => false,
    })
}

/// The MIR statements and terminators of the candidate's body.
fn statements(candidate: &Candidate<'_>) -> usize {
    candidate
        .body
        .blocks
        .iter()
        .map(|block| block.statements.len() + 1)
        .sum()
}

/// A site's byte range and the one-based line it starts on.
fn site_source(db: &dyn baml_compiler2_mir::Db, file: SourceFile, span: Span) -> (u32, u32, u32) {
    let start: u32 = span.range.start().into();
    let end: u32 = span.range.end().into();
    let text = file.text(db);
    let before = text.get(..start as usize).unwrap_or(text);
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    (start, end, u32::try_from(line).unwrap_or(u32::MAX))
}
