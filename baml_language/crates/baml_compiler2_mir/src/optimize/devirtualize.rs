//! Finding the class of an interface receiver where the code is written.
//!
//! An interface call or field read resolves, at run time, from the class of
//! its receiver. Where that class is known at the site, the inliner can
//! resolve it once, at compile time: the call becomes the implementation's
//! body and the field read a plain slot read.
//!
//! The class is known when the receiver's last definition before the site,
//! followed back through copies along the one way into each block, is a
//! class literal. Where two ways meet first (a callee that returns one class
//! from one `return` and another from the next), and each way gives a class,
//! the short run of blocks from the meeting point to the site is copied once
//! per way, so that each copy has one.
//!
//! Which implementation a class has for an interface is decided by
//! [`resolve_impl`], the checker's own selection. A class that has one can
//! never get another: coherence allows one implementation per class and
//! interface, whoever declares it and whenever it is loaded. Nothing here
//! assumes an interface has no other implementors.

use std::collections::HashMap;

use baml_base::Name;
use baml_compiler2_hir::{
    item_data::{function_data, function_has_body, impl_block_data, interface_data},
    loc::{DeclRef, FunctionLoc},
};
use baml_compiler2_hir_ty::{
    extern_loc::ClassRef,
    impls::{ResolvedImplFacts, resolve_impl},
    layout,
};
use baml_type::interned::{InferInterface, Ty};

use super::inline::{defines, terminator_defines};
use crate::{
    AggregateKind, BlockId, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind,
    Terminator, TyTemplate, TyTemplateInterface,
};

/// The most statements and terminators copied to give each way into a join
/// its own receiver class, per extra way.
const MAX_SPLIT_SIZE: usize = 16;

/// The most ways into a join that are given a copy each.
const MAX_SPLIT_WAYS: usize = 4;

/// How far back a receiver is followed, in definitions and blocks.
const MAX_TRACE: usize = 64;

/// A class a value is known to be an instance of, with its type arguments.
pub(super) struct KnownClass<'db> {
    pub(super) class: ClassRef<'db>,
    pub(super) args: Vec<TyTemplate>,
}

/// Where following a receiver back ended.
enum Trace<'db> {
    Known(KnownClass<'db>),
    /// At the start of `chain[0]`, which more than one block leads into,
    /// still looking for `local`. `chain` runs from there to the site.
    Join {
        local: Local,
        chain: Vec<BlockId>,
    },
    Unknown,
}

/// Each block's predecessors, without repeats. A block's handler is a
/// successor of it: a throw anywhere in the block continues there.
fn predecessors(body: &MirFunctionBody<'_>) -> Vec<Vec<BlockId>> {
    let mut predecessors: Vec<Vec<BlockId>> = vec![Vec::new(); body.blocks.len()];
    for block in &body.blocks {
        let successors = block
            .terminator
            .iter()
            .flat_map(Terminator::successors)
            .chain(block.unwind);
        for successor in successors {
            if !predecessors[successor.0].contains(&block.id) {
                predecessors[successor.0].push(block.id);
            }
        }
    }
    predecessors
}

/// Follow `local` back from just before statement `end` of `block` to the
/// class literal it was built by.
fn trace<'db>(
    body: &MirFunctionBody<'db>,
    predecessors: &[Vec<BlockId>],
    (mut block, mut end): (BlockId, usize),
    mut local: Local,
) -> Trace<'db> {
    let mut chain = vec![block];
    for _ in 0..MAX_TRACE {
        let statements = &body.blocks[block.0].statements[..end];
        if let Some(index) = statements
            .iter()
            .rposition(|statement| defines(&statement.kind, local))
        {
            let StatementKind::Assign {
                destination: Place::Local(_),
                value,
            } = &statements[index].kind
            else {
                return Trace::Unknown;
            };
            match value {
                Rvalue::Use(
                    Operand::Copy(Place::Local(source)) | Operand::Move(Place::Local(source)),
                ) => {
                    local = *source;
                    end = index;
                    continue;
                }
                Rvalue::Aggregate {
                    kind:
                        AggregateKind::Class {
                            class,
                            type_arg_templates,
                            ..
                        },
                    ..
                } if type_arg_templates.iter().all(TyTemplate::is_fully_concrete) => {
                    return Trace::Known(KnownClass {
                        class: *class,
                        args: type_arg_templates.clone(),
                    });
                }
                _ => return Trace::Unknown,
            }
        }
        match predecessors[block.0].as_slice() {
            [only] if !chain.contains(only) => {
                let terminator = body.blocks[only.0].terminator.as_ref();
                if terminator.is_some_and(|terminator| terminator_defines(terminator, local)) {
                    return Trace::Unknown;
                }
                block = *only;
                end = body.blocks[block.0].statements.len();
                chain.push(block);
            }
            [] | [_] => return Trace::Unknown,
            _ => {
                chain.reverse();
                return Trace::Join { local, chain };
            }
        }
    }
    Trace::Unknown
}

/// The class of the instance `receiver` holds just before statement `at.1`
/// of block `at.0` (the block's length for its terminator), when the code
/// says which.
///
/// May copy blocks to get there (see the module docs), in which case the
/// copies' size is added to `growth`. The site itself stays where it is.
pub(super) fn class_of<'db>(
    body: &mut MirFunctionBody<'db>,
    at: (BlockId, usize),
    receiver: &Operand<'db>,
    growth: &mut usize,
    budget: usize,
) -> Option<KnownClass<'db>> {
    let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = receiver else {
        return None;
    };
    let preds = predecessors(body);
    match trace(body, &preds, at, *local) {
        Trace::Known(class) => Some(class),
        Trace::Unknown => None,
        Trace::Join {
            local: joined,
            chain,
        } => {
            let (size, own) = split(body, &preds, joined, &chain, budget.saturating_sub(*growth))?;
            *growth += size;
            // The site's own way may now build the instance under a local of
            // its own.
            let local = if *local == joined {
                own.unwrap_or(joined)
            } else {
                *local
            };
            let preds = predecessors(body);
            match trace(body, &preds, at, local) {
                Trace::Known(class) => Some(class),
                Trace::Join { .. } | Trace::Unknown => None,
            }
        }
    }
}

/// Give each way into `chain[0]` its own copy of `chain`, when following
/// `local` back along each way ends at a class literal. Returns the size of
/// what was copied and, when each way got a local of its own for `local`,
/// the one of the way that kept `chain` itself. `None` when nothing was
/// copied.
fn split(
    body: &mut MirFunctionBody<'_>,
    preds: &[Vec<BlockId>],
    local: Local,
    chain: &[BlockId],
    budget: usize,
) -> Option<(usize, Option<Local>)> {
    let join = chain[0];
    let ways = preds[join.0].clone();
    let size: usize = chain
        .iter()
        .map(|block| body.blocks[block.0].statements.len() + 1)
        .sum();
    let copied = size * (ways.len() - 1);
    if ways.len() > MAX_SPLIT_WAYS
        || size > MAX_SPLIT_SIZE
        || copied > budget
        // The VM enters a handler with its error slots, and the entry has
        // no predecessor to redirect.
        || chain
            .iter()
            .any(|block| *block == body.entry || body.blocks[block.0].landing.is_some())
        // A way in from the chain itself is a loop around it.
        || ways.iter().any(|way| chain.contains(way))
    {
        return None;
    }
    // Every way must end knowing the class, or the copy buys nothing.
    for way in &ways {
        let block = &body.blocks[way.0];
        let terminator = block.terminator.as_ref()?;
        if terminator_defines(terminator, local)
            || !matches!(
                trace(body, preds, (*way, block.statements.len()), local),
                Trace::Known(_)
            )
        {
            return None;
        }
    }

    // When each way builds the instance itself and nothing outside the ways
    // and the chain looks at `local`, each way gets a local of its own for
    // it. One local built as two classes can not be replaced by its fields;
    // two locals built as one class each can.
    let own_local = renamable(body, local, chain, &ways);

    let mut copies: Vec<Vec<BlockId>> = vec![chain.to_vec()];
    for way in &ways[1..] {
        let renumbered: HashMap<BlockId, BlockId> = chain
            .iter()
            .enumerate()
            .map(|(offset, block)| (*block, BlockId(body.blocks.len() + offset)))
            .collect();
        for block in chain {
            let mut copy = body.blocks[block.0].clone();
            copy.id = renumbered[block];
            if let Some(terminator) = &mut copy.terminator {
                super::rewrite_block_ids_in_terminator_with_map(terminator, &renumbered);
            }
            body.blocks.push(copy);
        }
        let entered = HashMap::from([(join, renumbered[&join])]);
        if let Some(terminator) = &mut body.blocks[way.0].terminator {
            super::rewrite_block_ids_in_terminator_with_map(terminator, &entered);
        }
        copies.push(chain.iter().map(|block| renumbered[block]).collect());
    }

    let mut kept_own = None;
    if let Some(built_at) = own_local {
        for ((way, built), copy) in ways.iter().zip(built_at).zip(&copies) {
            let own = Local(body.locals.len());
            kept_own.get_or_insert(own);
            let decl = body.locals[local.0].clone();
            body.locals.push(decl);
            let mut renamed: Vec<Option<Local>> = (0..body.locals.len())
                .map(|index| Some(Local(index)))
                .collect();
            renamed[local.0] = Some(own);
            let block = &mut body.blocks[way.0];
            for statement in &mut block.statements[built..] {
                super::rewrite_locals_in_statement(statement, &renamed);
            }
            if let Some(terminator) = &mut block.terminator {
                super::rewrite_locals_in_terminator(terminator, &renamed);
            }
            for block in copy {
                let block = &mut body.blocks[block.0];
                for statement in &mut block.statements {
                    super::rewrite_locals_in_statement(statement, &renamed);
                }
                if let Some(terminator) = &mut block.terminator {
                    super::rewrite_locals_in_terminator(terminator, &renamed);
                }
            }
        }
    }
    Some((copied, kept_own))
}

/// Where each way builds `local`, when `local` can be a different local on
/// each way: every definition of it is the last one in a way, a class
/// literal, and every other mention of it is after that in the same way or
/// in the chain.
fn renamable(
    body: &MirFunctionBody<'_>,
    local: Local,
    chain: &[BlockId],
    ways: &[BlockId],
) -> Option<Vec<usize>> {
    if body.locals[local.0].is_captured || local.0 == 0 {
        return None;
    }
    let mut mentions = vec![0usize; body.locals.len()];
    let mut built_at = Vec::with_capacity(ways.len());
    for way in ways {
        let block = &body.blocks[way.0];
        let built = block
            .statements
            .iter()
            .rposition(|statement| defines(&statement.kind, local))?;
        if !matches!(
            &block.statements[built].kind,
            StatementKind::Assign {
                destination: Place::Local(_),
                value: Rvalue::Aggregate {
                    kind: AggregateKind::Class { .. },
                    ..
                },
            }
        ) {
            return None;
        }
        for statement in &block.statements[built..] {
            super::count_in_statement(statement, &mut mentions);
        }
        if let Some(terminator) = &block.terminator {
            super::count_in_terminator(terminator, &mut mentions);
        }
        built_at.push(built);
    }
    for block in chain {
        let block = &body.blocks[block.0];
        for statement in &block.statements {
            super::count_in_statement(statement, &mut mentions);
        }
        if let Some(terminator) = &block.terminator {
            super::count_in_terminator(terminator, &mut mentions);
        }
    }
    (super::count_local_defs(body)[local.0] == ways.len()
        && super::count_local_uses(body)[local.0] == mentions[local.0])
        .then_some(built_at)
}

/// The checker's view of `class` and `interface`, when both are whole types.
fn goal<'db>(
    db: &'db dyn crate::Db,
    class: &KnownClass<'db>,
    interface: &TyTemplateInterface,
) -> Option<(Ty, InferInterface)> {
    let plain = |template: &TyTemplate| {
        template
            .is_fully_concrete()
            .then(|| template.to_display_ty())
    };
    let concrete = baml_type::Ty::Class(
        layout::class_head(db, class.class),
        class.args.iter().map(plain).collect::<Option<_>>()?,
    );
    let interface = InferInterface::new(
        interface.name.clone(),
        interface
            .generics
            .iter()
            .map(|generic| plain(generic).map(|ty| Ty::from_plain(&ty)))
            .collect::<Option<_>>()?,
        interface
            .associated_types
            .iter()
            .map(|(name, binding)| Some((name.clone(), Ty::from_plain(&plain(binding)?))))
            .collect::<Option<_>>()?,
    );
    Some((Ty::from_plain(&concrete), interface))
}

/// The function that runs for `interface`'s `method` on an instance of
/// `class`: what the class's implementation provides, or the interface's
/// default body when it provides none. `None` when the class has no
/// implementation the checker can select from source.
pub(super) fn method<'db>(
    db: &'db dyn crate::Db,
    class: &KnownClass<'db>,
    interface: &TyTemplateInterface,
    method: &str,
) -> Option<FunctionLoc<'db>> {
    let (concrete, goal) = goal(db, class, interface)?;
    let resolved = resolve_impl(db, &concrete, &goal)?;
    let name = Name::new(method);
    match resolved.provided_method(db, &name) {
        Some(DeclRef::Source(function)) => Some(function),
        Some(DeclRef::External(_)) => None,
        None => {
            let DeclRef::Source(declaring) = layout::interface_ref_of(db, &interface.name)? else {
                return None;
            };
            interface_data(db, declaring)
                .methods
                .iter()
                .copied()
                .find(|&declared| {
                    function_data(db, declared).name == name && function_has_body(db, declared)
                })
        }
    }
}

/// The slot of `class` that `interface`'s field `field` is linked to: the
/// class field its `implements` block names for it, or the class field of
/// the same name.
pub(super) fn field_slot<'db>(
    db: &'db dyn crate::Db,
    class: &KnownClass<'db>,
    interface: &TyTemplateInterface,
    field: &Name,
) -> Option<usize> {
    let (concrete, goal) = goal(db, class, interface)?;
    let resolved = resolve_impl(db, &concrete, &goal)?;
    let ResolvedImplFacts::Source { block, .. } = resolved.facts else {
        return None;
    };
    let block = impl_block_data(db, block);
    let class_field = block
        .field_links
        .iter()
        .find(|link| link.interface_field == *field)
        .map_or(field, |link| &link.class_field);
    let slot = layout::class_field_index(db, class.class, class_field)?;
    usize::try_from(slot).ok()
}
