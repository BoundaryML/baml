//! Stack placement consumes MIR's function-escape facts. No second escape
//! analysis: the slot grouping and backwards reuse check are adapted from
//! escape-stack's placement code. Heap and stack use the very same C struct,
//! including its header, and ordinary pointer-based calls and field accesses.
use super::*;
use crate::operands::{walk_statement_operands, walk_terminator_operands};
use baml_compiler2_mir::{
    Local, escape,
    memory::{Resources, place_reads, walk_operand_places, walk_rvalue_places},
};

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn statement(
        &self,
        emitter: &mut Emitter<'_, '_>,
        statement: &StatementKind<'_>,
    ) -> Option<Gen<String>> {
        let &(group, class) = emitter
            .stack
            .sites
            .get(&(emitter.block, emitter.statement))?;
        let StatementKind::Assign {
            destination,
            value: Rvalue::Aggregate { fields, .. },
        } = statement
        else {
            unreachable!("stack allocation site")
        };
        Some((|| {
            let layout = &emitter.plan.layouts[&class];
            let mut out = String::from("{");
            // Evaluate before overwriting storage, including fields read from
            // an older instance. No allocation or safepoint during this store.
            for (i, value) in fields.iter().enumerate() {
                let value = emitter.coerce(emitter.operand(value)?, layout.fields[i])?;
                write!(&mut out, "let a{i}={value};").unwrap();
            }
            write!(
                &mut out,
                "stack{group}=C{class}{{header:baml_rt_gc::Header{{desc:{},aux:0}},",
                emitter.descriptors[&class]
            )
            .unwrap();
            for i in 0..fields.len() {
                let cast = if layout.fields[i] == Repr::Bool {
                    " as u64"
                } else {
                    ""
                };
                write!(&mut out, "f{i}:a{i}{cast},").unwrap();
            }
            out.push_str("};");
            out.push_str(&emitter.assign(
                destination,
                (
                    format!("(&raw mut stack{group}) as usize"),
                    Repr::Ptr(class),
                ),
            )?);
            out.push('}');
            Ok(out)
        })())
    }
}

#[derive(Default)]
pub(super) struct Placement {
    sites: BTreeMap<(usize, usize), (usize, usize)>,
    groups: BTreeMap<usize, usize>,
    pub(super) leaf_calls: BTreeSet<usize>,
}

impl Placement {
    pub(super) fn new<'db>(
        db: &'db dyn Db,
        root: SourceRoot,
        object: usize,
        body: &MirFunctionBody<'db>,
        opt: OptLevel,
        plan: &Plan<'_>,
        all: &BTreeMap<usize, MirFunction<'db>>,
    ) -> Self {
        if !body.blocks.iter().flat_map(|b| &b.statements).any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign {
                    destination: Place::Local(_),
                    value: Rvalue::Aggregate {
                        kind: AggregateKind::Class { .. },
                        ..
                    },
                }
            )
        }) {
            return Self::default();
        }
        let facts = escape::analyze_function(db, root, body, all[&object].arity, opt);
        let mut placement = Self::default();
        let mut invalid = BTreeSet::new();
        for block in &body.blocks {
            for (i, statement) in block.statements.iter().enumerate() {
                let StatementKind::Assign {
                    destination: Place::Local(local),
                    value:
                        Rvalue::Aggregate {
                            kind:
                                AggregateKind::Class {
                                    class,
                                    type_arg_templates,
                                },
                            fields,
                        },
                } = &statement.kind
                else {
                    continue;
                };
                if !type_arg_templates.is_empty() || !facts.local_stays_in_function(*local) {
                    continue;
                }
                let Some((class, _)) = plan.index.class(db, *class) else {
                    continue;
                };
                let layout = &plan.layouts[&class];
                // Bound the extra frame space, even in a large admitted body.
                if fields.len() != layout.fields.len() || layout.bytes > 1024 {
                    continue;
                }
                let group = facts.alias_group(*local);
                if let Some(previous) = placement.groups.insert(group, class)
                    && previous != class
                {
                    invalid.insert(group);
                }
                placement.sites.insert((block.id.0, i), (group, class));
            }
        }
        if placement.sites.is_empty() {
            return placement;
        }

        let live_in = baml_compiler2_mir::live_in_precise(body);
        for block in &body.blocks {
            let mut live = live_out(body, block, &live_in);
            let exceptional = block.unwind.map(|h| &live_in[h.0]);
            for (i, statement) in block.statements.iter().enumerate().rev() {
                if let Some(&(group, _)) = placement.sites.get(&(block.id.0, i)) {
                    let StatementKind::Assign {
                        destination: Place::Local(destination),
                        ..
                    } = &statement.kind
                    else {
                        unreachable!()
                    };
                    // A live alias may be held indirectly by another local's
                    // fields/elements. Also reject self-linked storage: killing
                    // the destination does not kill its previous contents.
                    if live.iter().enumerate().any(|(local, &live)| {
                        live && facts.may_reach(Local(local), *destination, local == destination.0)
                    }) || facts.may_reach(*destination, *destination, true)
                    {
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
        placement
            .sites
            .retain(|_, (group, _)| !invalid.contains(group));
        placement.groups.retain(|group, _| !invalid.contains(group));
        // Stay within the existing fiber stack budget; oversized functions
        // simply retain their heap allocations.
        if placement
            .groups
            .values()
            .map(|c| plan.layouts[c].bytes)
            .sum::<usize>()
            > 4096
        {
            return Self::default();
        }
        for block in &body.blocks {
            let Some(Terminator::Call {
                callee: Operand::Constant(Constant::Function(function)),
                args,
                ..
            }) = &block.terminator
            else {
                continue;
            };
            if !args.iter().any(|arg| {
                matches!(arg,
                Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))
                if placement.groups.contains_key(&facts.alias_group(*local)))
            }) {
                continue;
            }
            let Some((callee, _)) = plan.index.function(db, *function) else {
                continue;
            };
            // Generic calls reach their monomorphized body, whose observation
            // word and leaf entry are emitted under the specialization's id.
            let callee = plan
                .specialized_calls
                .get(&(object, block.id.0))
                .copied()
                .unwrap_or(callee);
            if all.get(&callee).is_some_and(|mir| small_leaf(mir)) {
                placement.leaf_calls.insert(callee);
            }
        }
        placement
    }

    pub(super) fn declarations(&self, emitter: &Emitter<'_, '_>) -> String {
        let mut out = String::new();
        // The closed-world driver initializes observation words once, before
        // BAML runs. Hoisting this test lets LLVM unswitch the loop and scalar
        // replace its stack objects while keeping observed calls unchanged.
        for callee in &self.leaf_calls {
            writeln!(
                &mut out,
                "let stack_mode{callee}=if OBSERVE {{rt::telemetry::mode(&O{callee})}} else {{0}};"
            )
            .unwrap();
        }
        for (&group, &class) in &self.groups {
            write!(
                &mut out,
                "let mut stack{group}=C{class}{{header:baml_rt_gc::Header{{desc:{},aux:0}},",
                emitter.descriptors[&class]
            )
            .unwrap();
            for (i, repr) in emitter.plan.layouts[&class].fields.iter().enumerate() {
                let cast = if *repr == Repr::Bool { " as u64" } else { "" };
                write!(&mut out, "f{i}:{}{cast},", repr.zero()).unwrap();
            }
            out.push_str("};\n");
        }
        out
    }

    pub(super) fn bytes(&self, plan: &Plan<'_>) -> usize {
        self.groups
            .values()
            .map(|class| plan.layouts[class].bytes)
            .sum()
    }
}

fn small_leaf(mir: &MirFunction<'_>) -> bool {
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        return false;
    };
    mir.arity > 0
        && body.locals.len() <= 32
        && body.blocks.len() <= 16
        && body.blocks.iter().all(|b| {
            matches!(
                b.terminator,
                Some(
                    Terminator::Goto { .. }
                        | Terminator::Return
                        | Terminator::Branch { .. }
                        | Terminator::ShortCircuit { .. }
                        | Terminator::Throw { .. }
                        | Terminator::Rethrow { .. }
                )
            )
        })
}

impl Emitter<'_, '_> {
    pub(super) fn stack_leaf_call(&self, callee: usize, arguments: &str, observed: &str) -> String {
        if self.stack.leaf_calls.contains(&callee) {
            format!(
                "if stack_mode{callee}==0 {{f{callee}_body::<OBSERVE>(cx,{arguments})}} else {{{observed}}}"
            )
        } else {
            observed.to_owned()
        }
    }
}

// From escape-stack's storage reuse check. A call overwrites a local result
// on its normal edge; the previous result stays live on exceptional edges.
fn live_out(
    body: &MirFunctionBody<'_>,
    block: &baml_compiler2_mir::BasicBlock<'_>,
    live_in: &[Vec<bool>],
) -> Vec<bool> {
    let mut live = vec![false; body.locals.len()];
    if let Some(term) = &block.terminator {
        for successor in term.successors() {
            for (live, &into) in live.iter_mut().zip(&live_in[successor.0]) {
                *live |= into
            }
        }
        let mut reads = Resources::default();
        if let Some((destination, _)) = crate::analysis::continuation(term) {
            match destination {
                Place::Local(local) => live[local.0] = false,
                other => place_reads(other, &mut reads),
            }
        }
        walk_terminator_operands(term, &mut |operand| {
            walk_operand_places(operand, &mut |place| place_reads(place, &mut reads));
        });
        match term {
            Terminator::Return => live[0] = true,
            Terminator::NarrowBind { destination, .. } => live[destination.0] = true,
            _ => {}
        }
        for local in reads.locals {
            live[local.0] = true
        }
    }
    if let Some(handler) = block.unwind {
        for (live, &into) in live.iter_mut().zip(&live_in[handler.0]) {
            *live |= into
        }
    }
    live
}

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
        _ => walk_statement_operands(statement, &mut |operand| {
            walk_operand_places(operand, &mut |place| place_reads(place, &mut reads));
        }),
    }
    for local in reads.locals {
        live[local.0] = true
    }
}
