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

use std::collections::{BTreeMap, BTreeSet};

use baml_compiler2_mir::{
    AggregateKind, BlockId, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind,
    Terminator,
    escape::{EscapeFacts, GroupId, Site},
    memory::{Resources, place_reads, walk_operand_places, walk_rvalue_places},
};

use crate::operands::{walk_statement_operands, walk_terminator_operands};

/// Bytes of frame a class's cell may take, header included.
const MAX_CELL_BYTES: usize = 1024;

/// Bytes of frame every slot of a body may take together.
const MAX_FRAME_BYTES: usize = 4096;

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

impl Placement {
    /// The placement of `body`, whose facts are `facts`; `cell` gives a
    /// class object's instance cell bytes when its instances may be placed
    /// (not generic, no finalizer), and `class` resolves a literal's class.
    pub(crate) fn new<'db>(
        body: &MirFunctionBody<'db>,
        facts: &EscapeFacts,
        class: impl Fn(&AggregateKind<'db>) -> Option<usize>,
        cell: impl Fn(usize) -> Option<usize>,
    ) -> Self {
        let mut candidates = Vec::new();
        let mut invalid: BTreeSet<GroupId> = BTreeSet::new();
        // Groups with a class site, each one's class: a group holding
        // literals of two classes has no one cell size.
        let mut classes: BTreeMap<GroupId, usize> = BTreeMap::new();
        for (site, block, statement) in facts.sites() {
            let kind = &body.blocks[block.0].statements[statement].kind;
            let StatementKind::Assign {
                destination,
                value: Rvalue::Aggregate { kind, .. },
            } = kind
            else {
                continue;
            };
            let AggregateKind::Class { .. } = kind else {
                continue;
            };
            let group = facts.alias_group(site);
            let placeable = (|| {
                let Place::Local(destination) = destination else {
                    return None;
                };
                if !facts.site_stays_in_frame(site) {
                    return None;
                }
                let class = class(kind)?;
                let bytes = cell(class)?;
                if bytes > MAX_CELL_BYTES {
                    return None;
                }
                Some(Candidate {
                    site,
                    block: block.0,
                    statement,
                    destination: *destination,
                    class,
                    bytes,
                })
            })();
            match placeable {
                Some(candidate) => {
                    if *classes.entry(group).or_insert(candidate.class) != candidate.class {
                        invalid.insert(group);
                    }
                    candidates.push(candidate);
                }
                // A class site of the group that stays in the heap: the
                // group's objects are not all placed.
                None => {
                    invalid.insert(group);
                }
            }
        }
        if candidates.is_empty() {
            return Self::default();
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
                        invalid.insert(group);
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
            .filter(|group| !invalid.contains(group))
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
            }
        }

        let mut placement = Self::default();
        let mut slot_of: BTreeMap<GroupId, usize> = BTreeMap::new();
        for candidate in &candidates {
            let group = facts.alias_group(candidate.site);
            if !placed.contains(&group) {
                continue;
            }
            let slot = *slot_of.entry(group).or_insert_with(|| {
                placement.slots.push((candidate.class, candidate.bytes));
                placement.slots.len() - 1
            });
            placement
                .sites
                .insert((candidate.block, candidate.statement), slot);
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
            return Self::default();
        }
        placement
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
