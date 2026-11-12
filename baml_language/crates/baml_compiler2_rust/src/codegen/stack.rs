//! Stack placement of class literals the escape analysis proved cannot
//! outlive their frame (`escape::EscapeFacts::site_stays_in_frame`).
//!
//! One slot per alias group: every site and local that may hold the same
//! object shares one `StackCell` in the body's frame, so a literal in a loop
//! overwrites the previous iteration's instance in place. That reuse is
//! sound only when nothing still reaches the previous instance: no local
//! live after the site may hold it, or hold an object whose fields or
//! elements hold it (`group_may_reach`), and the group may not be self-linked.
//! Liveness is `live_in_precise`, with the handler's live-in merged at every
//! statement: a throw anywhere in the block keeps those locals alive.
//!
//! A placed object may be held only by other placed objects: the collector
//! finds a stack cell's heap pointers through the conservative scan of the
//! fiber stack, but a heap object's pointer slots are traced precisely, and a
//! stack address there would be marked as a cell. So a group held by any
//! group that is not itself placed stays in the heap.
//!
//! Heap and stack cells share one layout, header included
//! (`BexVm::place_instance`), so field access, calls, natives and telemetry
//! see an ordinary pointer.
//!
//! [`plan`] decides every allocation site of a body and records why each
//! unplaced one stays in the heap ([`Refusal`]), the same decision the
//! emitter acts on; `coverage` reports those verdicts over a program.

use std::collections::{BTreeMap, BTreeSet};

use baml_compiler2_mir::{
    AggregateKind, BlockId, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind,
    Terminator,
    escape::{EscapeFacts, FrameEscape, GroupId, Site},
    memory::{Resources, place_reads, walk_operand_places, walk_rvalue_places},
};

use crate::{
    index::ProgramIndex,
    operands::{walk_statement_operands, walk_terminator_operands},
};

/// Bytes of frame a class's cell may take, header included.
const MAX_CELL_BYTES: usize = 1024;

/// Bytes of frame every slot of a body may take together.
const MAX_FRAME_BYTES: usize = 4096;

/// Why an allocation site stays in the heap: the first check that refused
/// it, in the order [`plan`] applies them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// The body is not compiled: the backend rejected it for this reason.
    NotCompiled(String),
    /// The analysis says the object may outlive the frame.
    Escapes(FrameEscape),
    /// Not a class literal: an array, map or byte array, which v1 keeps in
    /// the heap.
    NotClass,
    /// The literal is stored into a field or element, not a local.
    Destination,
    /// The literal's class is not in the program's object table.
    UnknownClass,
    /// A generic class, or a literal with type arguments.
    Generic,
    /// The class has a finalizer.
    Finalizer,
    /// The cell exceeds [`MAX_CELL_BYTES`].
    Size,
    /// Another class literal of the alias group stays in the heap.
    GroupMember,
    /// The alias group holds literals of two classes.
    MixedClasses,
    /// A local live after the site may still reach the previous instance,
    /// or the group is self-linked, so the slot cannot be reused.
    LiveReach,
    /// Held by an alias group that is not placed.
    HeldByUnplaced,
    /// The body's slots together exceed [`MAX_FRAME_BYTES`].
    Budget,
}

impl Refusal {
    /// The coarse cause, for tallies: the escape reason's kind without its
    /// callee name, and the backend's checks by name.
    pub fn bucket(&self) -> &'static str {
        match self {
            Self::NotCompiled(_) => "not compiled",
            Self::Escapes(escape) => match escape {
                FrameEscape::Returned => "escapes: returned",
                FrameEscape::Shared => "escapes: shared",
                FrameEscape::Global => "escapes: global",
                FrameEscape::Captured => "escapes: captured",
                FrameEscape::Thrown => "escapes: thrown",
                FrameEscape::Spawned => "escapes: spawned",
                FrameEscape::KeepingCall(_) => "escapes: keeping call",
                FrameEscape::Native(_) => "escapes: native",
                FrameEscape::Finalizer => "escapes: finalizer",
                FrameEscape::Parameter => "escapes: parameter",
                FrameEscape::Generic => "escapes: generic",
            },
            Self::NotClass => "not a class literal",
            Self::Destination => "stored into a field or element",
            Self::UnknownClass => "class not in the program",
            Self::Generic => "generic class",
            Self::Finalizer => "class with a finalizer",
            Self::Size => "cell over the size limit",
            Self::GroupMember => "another literal of the group stays in the heap",
            Self::MixedClasses => "group holds two classes",
            Self::LiveReach => "previous instance reachable at the site",
            Self::HeldByUnplaced => "held by an unplaced group",
            Self::Budget => "frame over the slot budget",
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotCompiled(reason) => write!(f, "not compiled: {reason}"),
            Self::Escapes(FrameEscape::KeepingCall(callee)) => {
                write!(f, "escapes: kept by call to {callee}")
            }
            Self::Escapes(FrameEscape::Native(callee)) => {
                write!(f, "escapes: passed to native {callee}")
            }
            _ => f.write_str(self.bucket()),
        }
    }
}

/// The decision for one allocation site: its slot, or why it stays in the
/// heap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub site: Site,
    pub outcome: Result<usize, Refusal>,
}

/// The stack slots of one body and the sites that fill them.
#[derive(Default, Debug)]
pub(crate) struct Placement {
    /// `(block, statement)` of each placed site, to its slot.
    sites: BTreeMap<(usize, usize), usize>,
    /// Per slot: the class object it holds instances of, and its cell bytes
    /// (the layout's size, header included; the runtime rounds it to the
    /// heap's size class).
    slots: Vec<(usize, usize)>,
}

/// A class literal assigned to a local: a candidate site.
struct Candidate {
    site: Site,
    block: usize,
    statement: usize,
    destination: Local,
    class: usize,
    bytes: usize,
}

/// The placement of `body` against the classes of `index`, with the verdict
/// of every allocation site in statement order.
pub(crate) fn plan<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex<'_>,
    body: &MirFunctionBody<'db>,
    facts: &EscapeFacts,
) -> (Placement, Vec<Verdict>) {
    Placement::plan(
        body,
        facts,
        |kind| match kind {
            AggregateKind::Class {
                class,
                type_arg_templates,
            } if type_arg_templates.is_empty() => index
                .class(db, *class)
                .map(|(object, _)| object)
                .ok_or(Refusal::UnknownClass),
            _ => Err(Refusal::Generic),
        },
        |object| index.instance_cell(object),
    )
}

impl Placement {
    /// The placement of `body`, whose facts are `facts`; `class` resolves a
    /// literal's class object and `cell` gives a class object's instance cell
    /// bytes, each refusing with the cause.
    fn plan<'db>(
        body: &MirFunctionBody<'db>,
        facts: &EscapeFacts,
        class: impl Fn(&AggregateKind<'db>) -> Result<usize, Refusal>,
        cell: impl Fn(usize) -> Result<usize, Refusal>,
    ) -> (Self, Vec<Verdict>) {
        let mut candidates = Vec::new();
        let mut verdicts = Vec::new();
        // The first cause a group's literals stay in the heap, once one is
        // known for every candidate of the group.
        let mut invalid: BTreeMap<GroupId, Refusal> = BTreeMap::new();
        // Groups with a class site, each one's class: a group holding
        // literals of two classes has no one cell size.
        let mut classes: BTreeMap<GroupId, usize> = BTreeMap::new();
        for (site, block, statement) in facts.sites() {
            let kind = &body.blocks[block.0].statements[statement].kind;
            let (destination, literal) = match kind {
                StatementKind::Assign {
                    destination,
                    value: Rvalue::Aggregate { kind, .. },
                } if matches!(kind, AggregateKind::Class { .. }) => (destination, Some(kind)),
                StatementKind::Assign { destination, .. } => (destination, None),
                _ => unreachable!("an allocation site is an assignment"),
            };
            let group = facts.alias_group(site);
            let placeable = (|| {
                if let Some(escape) = facts.site_escape(site) {
                    return Err(Refusal::Escapes(escape.clone()));
                }
                let Some(kind) = literal else {
                    return Err(Refusal::NotClass);
                };
                let Place::Local(destination) = destination else {
                    return Err(Refusal::Destination);
                };
                let class = class(kind)?;
                let bytes = cell(class)?;
                if bytes > MAX_CELL_BYTES {
                    return Err(Refusal::Size);
                }
                Ok(Candidate {
                    site,
                    block: block.0,
                    statement,
                    destination: *destination,
                    class,
                    bytes,
                })
            })();
            match placeable {
                Ok(candidate) => {
                    if *classes.entry(group).or_insert(candidate.class) != candidate.class {
                        invalid.entry(group).or_insert(Refusal::MixedClasses);
                    }
                    candidates.push(candidate);
                }
                Err(refusal) => {
                    // A class site of the group that stays in the heap: the
                    // group's objects are not all placed.
                    if literal.is_some() {
                        invalid.entry(group).or_insert(Refusal::GroupMember);
                    }
                    verdicts.push(Verdict {
                        site,
                        outcome: Err(refusal),
                    });
                }
            }
        }
        if candidates.is_empty() {
            verdicts.sort_by_key(|verdict| verdict.site);
            return (Self::default(), verdicts);
        }

        // Holders: every group reachable from a local or site, with what
        // its fields and elements hold.
        let mut holders: BTreeMap<GroupId, BTreeSet<GroupId>> = BTreeMap::new();
        let roots = (0..body.locals.len())
            .filter_map(|local| facts.local_group(Local(local)))
            .chain(facts.sites().map(|(site, _, _)| facts.alias_group(site)));
        let mut seen = BTreeSet::new();
        for root in roots {
            let mut group = root;
            while seen.insert(group) {
                let Some(contents) = facts.contents_group(group) else {
                    break;
                };
                holders.entry(contents).or_default().insert(group);
                group = contents;
            }
        }

        let live_in = baml_compiler2_mir::live_in_precise(body);
        for block in &body.blocks {
            let mut live = live_out(body, block, &live_in);
            let exceptional = block.unwind.map(|handler| &live_in[handler.0]);
            for (i, statement) in block.statements.iter().enumerate().rev() {
                if let Some(candidate) = candidates
                    .iter()
                    .find(|c| c.block == block.id.0 && c.statement == i)
                {
                    let group = facts.alias_group(candidate.site);
                    let destination = candidate.destination;
                    // A live alias may hold the previous instance directly or
                    // through another object. The destination itself takes
                    // the new instance, so only its contents matter; and a
                    // self-linked group keeps its previous instance alive
                    // through the new one.
                    let reaches = live.iter().enumerate().any(|(local, &live)| {
                        live && facts.local_group(Local(local)).is_some_and(|from| {
                            if local == destination.0 {
                                strictly_reaches(facts, from, group)
                            } else {
                                facts.group_may_reach(from, group)
                            }
                        })
                    });
                    if reaches || strictly_reaches(facts, group, group) {
                        invalid.entry(group).or_insert(Refusal::LiveReach);
                    }
                }
                transfer(&statement.kind, &mut live);
                if let Some(exceptional) = exceptional {
                    for (live, exceptional) in live.iter_mut().zip(exceptional) {
                        *live |= *exceptional;
                    }
                }
            }
        }

        // A placed group is held by placed groups only.
        let mut placed: BTreeSet<GroupId> = classes
            .keys()
            .filter(|group| !invalid.contains_key(group))
            .copied()
            .collect();
        loop {
            let unplaced: Vec<GroupId> = placed
                .iter()
                .filter(|group| {
                    holders
                        .get(group)
                        .is_some_and(|held_by| held_by.iter().any(|h| !placed.contains(h)))
                })
                .copied()
                .collect();
            if unplaced.is_empty() {
                break;
            }
            for group in unplaced {
                placed.remove(&group);
                invalid.insert(group, Refusal::HeldByUnplaced);
            }
        }

        let mut placement = Self::default();
        let mut slot_of: BTreeMap<GroupId, usize> = BTreeMap::new();
        let mut outcomes = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            let group = facts.alias_group(candidate.site);
            if let Some(refusal) = invalid.get(&group) {
                outcomes.push((candidate.site, Err(refusal.clone())));
                continue;
            }
            let slot = *slot_of.entry(group).or_insert_with(|| {
                placement.slots.push((candidate.class, candidate.bytes));
                placement.slots.len() - 1
            });
            placement
                .sites
                .insert((candidate.block, candidate.statement), slot);
            outcomes.push((candidate.site, Ok(slot)));
        }
        // Stay within the fiber stack budget: an oversized body keeps its
        // heap allocations.
        if placement
            .slots
            .iter()
            .map(|&(_, bytes)| bytes)
            .sum::<usize>()
            > MAX_FRAME_BYTES
        {
            placement = Self::default();
            for (_, outcome) in &mut outcomes {
                if outcome.is_ok() {
                    *outcome = Err(Refusal::Budget);
                }
            }
        }
        verdicts.extend(
            outcomes
                .into_iter()
                .map(|(site, outcome)| Verdict { site, outcome }),
        );
        verdicts.sort_by_key(|verdict| verdict.site);
        (placement, verdicts)
    }

    /// The slot the literal at `statement` of `block` fills, if placed.
    pub(crate) fn slot_at(&self, block: BlockId, statement: usize) -> Option<usize> {
        self.sites.get(&(block.0, statement)).copied()
    }

    /// Per slot, its class object and cell bytes.
    pub(crate) fn slots(&self) -> &[(usize, usize)] {
        &self.slots
    }
}

/// Whether an object of `from` may hold, through fields and elements at any
/// depth, an object of `to`: `group_may_reach` without identity.
fn strictly_reaches(facts: &EscapeFacts, from: GroupId, to: GroupId) -> bool {
    facts
        .contents_group(from)
        .is_some_and(|contents| facts.group_may_reach(contents, to))
}

/// The locals live when `block`'s terminator runs: live into every
/// successor and the handler, read by the terminator, minus a call's
/// destination, which the call overwrites on its normal edge only.
fn live_out(
    body: &MirFunctionBody<'_>,
    block: &baml_compiler2_mir::BasicBlock<'_>,
    live_in: &[Vec<bool>],
) -> Vec<bool> {
    let mut live = vec![false; body.locals.len()];
    if let Some(terminator) = &block.terminator {
        for successor in terminator.successors() {
            for (live, &into) in live.iter_mut().zip(&live_in[successor.0]) {
                *live |= into;
            }
        }
        let mut reads = Resources::default();
        if let Some((destination, _)) = crate::analysis::continuation(terminator) {
            match destination {
                Place::Local(local) => live[local.0] = false,
                other => place_reads(other, &mut reads),
            }
        }
        walk_terminator_operands(terminator, &mut |operand| {
            walk_operand_places(operand, &mut |place| place_reads(place, &mut reads));
        });
        match terminator {
            Terminator::Return => live[0] = true,
            Terminator::NarrowBind { destination, .. } => live[destination.0] = true,
            _ => {}
        }
        for local in reads.locals {
            live[local.0] = true;
        }
    }
    if let Some(handler) = block.unwind {
        for (live, &into) in live.iter_mut().zip(&live_in[handler.0]) {
            *live |= into;
        }
    }
    live
}

/// Liveness through one statement, backwards.
fn transfer(statement: &StatementKind<'_>, live: &mut [bool]) {
    let mut reads = Resources::default();
    match statement {
        StatementKind::Assign { destination, value } => {
            match destination {
                Place::Local(local) => live[local.0] = false,
                other => place_reads(other, &mut reads),
            }
            walk_rvalue_places(value, &mut |place| place_reads(place, &mut reads));
        }
        StatementKind::Drop(place) => place_reads(place, &mut reads),
        StatementKind::FreshCell { local, .. } => live[local.0] = true,
        _ => walk_statement_operands(statement, &mut |operand: &Operand<'_>| {
            walk_operand_places(operand, &mut |place| place_reads(place, &mut reads));
        }),
    }
    for local in reads.locals {
        live[local.0] = true;
    }
}
