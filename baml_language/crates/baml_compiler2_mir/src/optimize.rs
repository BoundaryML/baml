//! Post-lowering MIR optimization passes.
//!
//! Runs after `MirBuilder::build()` and performs:
//! 1. Dead block elimination (reachability-based)
//! 2. Copy propagation + dead local elimination
//! 3. RPO block reordering

use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(debug_assertions)]
use baml_base::Name;

use crate::{
    BasicBlock, BlockId, CatchRegion, Local, MirFunction, MirFunctionBody, MirFunctionKind,
    Operand, Place, Terminator,
};

/// Run all optimization passes on a MIR function.
pub(crate) fn optimize_function(func: &mut MirFunction) {
    let MirFunctionKind::Bytecode(body) = &mut func.kind else {
        return; // nothing to clean up on builtins
    };
    eliminate_dead_blocks(body);
    merge_passthrough_blocks(body);
    propagate_copies(body, func.arity);
    eliminate_dead_locals(body, func.arity);
    merge_passthrough_blocks(body); // catch blocks emptied by copy-prop / dead-local elim
    reorder_blocks_rpo(body);

    #[cfg(debug_assertions)]
    verify_mir(body, &func.item_ref);
}

/// Run all cleanup phases directly on a `MirFunctionBody`.
///
/// Used for let-binding initializers, which are lowered as bodies without
/// the enclosing `MirFunction` wrapper (arity = 0).
pub(crate) fn optimize_function_body(body: &mut MirFunctionBody) {
    eliminate_dead_blocks(body);
    merge_passthrough_blocks(body);
    propagate_copies(body, 0);
    eliminate_dead_locals(body, 0);
    merge_passthrough_blocks(body); // catch blocks emptied by copy-prop / dead-local elim
    reorder_blocks_rpo(body);

    #[cfg(debug_assertions)]
    verify_mir(
        body,
        &crate::ItemRef::Free {
            package: Name::new("$init_let"),
            namespace: vec![],
            name: Name::new("_"),
        },
    );
}

// ============================================================================
// Phase 1: Dead block elimination
// ============================================================================

/// Phase 1: Remove unreachable blocks via BFS from entry.
fn eliminate_dead_blocks(body: &mut MirFunctionBody) {
    // BFS to find all reachable blocks. Seed with entry AND exception
    // handler blocks — they're reachable at runtime via the exception table
    // even though they have no incoming CFG edges.
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(body.entry);
    reachable.insert(body.entry);
    for region in &body.catch_regions {
        if reachable.insert(region.handler) {
            queue.push_back(region.handler);
        }
    }

    while let Some(block_id) = queue.pop_front() {
        if let Some(term) = &body.blocks[block_id.0].terminator {
            for succ in term.successors() {
                if reachable.insert(succ) {
                    queue.push_back(succ);
                }
            }
        }
    }

    // If all blocks are reachable, nothing to do
    if reachable.len() == body.blocks.len() {
        return;
    }

    // Build old -> new BlockId mapping (only reachable blocks, preserving order)
    let mut old_to_new: Vec<Option<BlockId>> = vec![None; body.blocks.len()];
    let mut new_blocks: Vec<BasicBlock<'_>> = Vec::new();
    for block in &body.blocks {
        if reachable.contains(&block.id) {
            let new_id = BlockId(new_blocks.len());
            old_to_new[block.id.0] = Some(new_id);
            let mut new_block = block.clone();
            new_block.id = new_id;
            new_blocks.push(new_block);
        }
    }

    // Rewrite all BlockId references in terminators
    for block in &mut new_blocks {
        if let Some(term) = &mut block.terminator {
            rewrite_block_ids_in_terminator(term, &old_to_new);
        }
    }

    // Rewrite entry block
    body.entry = old_to_new[body.entry.0].expect("entry block must be reachable");

    rewrite_catch_region_blocks(&mut body.catch_regions, &old_to_new);

    body.blocks = new_blocks;
}

/// Rewrite all `BlockId` references in a terminator using old->new mapping.
fn rewrite_block_ids_in_terminator(term: &mut Terminator, map: &[Option<BlockId>]) {
    let remap = |id: &mut BlockId| {
        *id = map[id.0].expect("successor block must be reachable");
    };

    match term {
        Terminator::Goto { target } => remap(target),
        Terminator::Branch {
            then_block,
            else_block,
            ..
        }
        | Terminator::NarrowBind {
            then_block,
            else_block,
            ..
        } => {
            remap(then_block);
            remap(else_block);
        }
        Terminator::Switch {
            arms, otherwise, ..
        } => {
            for (_, target) in arms {
                remap(target);
            }
            remap(otherwise);
        }
        Terminator::Return => {}
        Terminator::Unreachable => {}
        Terminator::Spawn { resume, .. } => remap(resume),
        Terminator::Call { target, unwind, .. }
        | Terminator::VirtualCall { target, unwind, .. }
        | Terminator::SysOp { target, unwind, .. }
        | Terminator::Await { target, unwind, .. }
        | Terminator::AwaitAny { target, unwind, .. } => {
            remap(target);
            if let Some(u) = unwind {
                remap(u);
            }
        }
        Terminator::Throw { .. } | Terminator::Rethrow { .. } => {}
        Terminator::ThrowIfPanic { otherwise, .. } => remap(otherwise),
        Terminator::ShortCircuit { eval_rhs, join, .. } => {
            remap(eval_rhs);
            remap(join);
        }
    }
}

/// Rewrite `BlockId` references in all catch regions using old->new mapping.
fn rewrite_catch_region_blocks(regions: &mut Vec<CatchRegion>, map: &[Option<BlockId>]) {
    regions.retain_mut(|region| {
        let Some(new_body) = map[region.body_entry.0] else {
            return false; // body block was removed — drop the region
        };
        let Some(new_handler) = map[region.handler.0] else {
            return false; // handler block was removed — drop the region
        };
        region.body_entry = new_body;
        region.handler = new_handler;
        // Remap the handler-body blocks too (drop any that were removed) so the
        // BEP-042 cause-chain extent stays accurate after block renumbering.
        region.handler_body = region
            .handler_body
            .iter()
            .filter_map(|b| map[b.0])
            .collect();
        // Same for the protected body blocks (a removed block was unreachable
        // and had nothing to protect).
        region.body_blocks = region.body_blocks.iter().filter_map(|b| map[b.0]).collect();
        true
    });
}

// ============================================================================
// Phase 1b: Passthrough block merging
// ============================================================================

/// Rewrite all `BlockId` references in a terminator using a `HashMap` redirect map.
fn rewrite_block_ids_in_terminator_with_map(
    term: &mut Terminator,
    map: &HashMap<BlockId, BlockId>,
) {
    let remap = |id: &mut BlockId| {
        if let Some(&new_id) = map.get(id) {
            *id = new_id;
        }
    };

    match term {
        Terminator::Goto { target } => remap(target),
        Terminator::Branch {
            then_block,
            else_block,
            ..
        }
        | Terminator::NarrowBind {
            then_block,
            else_block,
            ..
        } => {
            remap(then_block);
            remap(else_block);
        }
        Terminator::Switch {
            arms, otherwise, ..
        } => {
            for (_, target) in arms {
                remap(target);
            }
            remap(otherwise);
        }
        Terminator::Return => {}
        Terminator::Unreachable => {}
        Terminator::Spawn { resume, .. } => remap(resume),
        Terminator::Call { target, unwind, .. }
        | Terminator::VirtualCall { target, unwind, .. }
        | Terminator::SysOp { target, unwind, .. }
        | Terminator::Await { target, unwind, .. }
        | Terminator::AwaitAny { target, unwind, .. } => {
            remap(target);
            if let Some(u) = unwind {
                remap(u);
            }
        }
        Terminator::Throw { .. } | Terminator::Rethrow { .. } => {}
        Terminator::ThrowIfPanic { otherwise, .. } => remap(otherwise),
        Terminator::ShortCircuit { eval_rhs, join, .. } => {
            remap(eval_rhs);
            remap(join);
        }
    }
}

/// Merge passthrough blocks: blocks with no statements and a single Goto terminator
/// are eliminated by redirecting all references to them to their target.
fn merge_passthrough_blocks(body: &mut MirFunctionBody) {
    // Step 1: identify passthrough blocks (empty statements + Goto terminator)
    let mut redirect: HashMap<BlockId, BlockId> = HashMap::new();
    for block in &body.blocks {
        if !block.statements.is_empty() {
            continue;
        }
        if let Some(Terminator::Goto { target }) = &block.terminator {
            // Don't redirect the entry block — it must remain as-is
            if block.id != body.entry {
                redirect.insert(block.id, *target);
            }
        }
    }

    if redirect.is_empty() {
        return;
    }

    // Step 2: resolve chains (A→B→C becomes A→C)
    let mut resolved: HashMap<BlockId, BlockId> = HashMap::new();
    for &src in redirect.keys() {
        let mut target = redirect[&src];
        let mut visited = HashSet::new();
        visited.insert(src);
        while let Some(&next) = redirect.get(&target) {
            if !visited.insert(target) {
                break; // cycle detected, stop
            }
            target = next;
        }
        resolved.insert(src, target);
    }

    // Step 3: rewrite all terminators
    for block in &mut body.blocks {
        if let Some(term) = &mut block.terminator {
            rewrite_block_ids_in_terminator_with_map(term, &resolved);
        }
    }

    // Step 4: rewrite catch regions
    for region in &mut body.catch_regions {
        if let Some(&new_body) = resolved.get(&region.body_entry) {
            region.body_entry = new_body;
        }
        if let Some(&new_handler) = resolved.get(&region.handler) {
            region.handler = new_handler;
        }
        for b in &mut region.handler_body {
            if let Some(&new_b) = resolved.get(b) {
                *b = new_b;
            }
        }
        // A redirected passthrough block is empty (no instructions to
        // protect), and remapping it to its target would wrongly extend the
        // protected range over the target's instructions — drop it instead.
        region.body_blocks.retain(|b| !resolved.contains_key(b));
    }

    // Step 5: entry block redirect (shouldn't happen since we excluded it, but be safe)
    if let Some(&new_entry) = resolved.get(&body.entry) {
        body.entry = new_entry;
    }

    // Step 6: re-run dead block elimination to compact
    eliminate_dead_blocks(body);
}

// ============================================================================
// Phase 2a: Copy propagation
// ============================================================================

/// Count uses of each Local across all blocks and catch region error locals.
/// Collect all locals that appear inside a `Place` projection.
///
/// This includes locals used as `Place::Local` bases of field/index projections
/// and locals used as the `index` field of `Place::Index`. These positions are
/// typed as `Local` (not `Operand`), so they cannot be replaced by a `Constant`
/// during copy propagation.
fn collect_place_index_locals(body: &MirFunctionBody<'_>) -> HashSet<Local> {
    fn scan_place(p: &Place, set: &mut HashSet<Local>) {
        match p {
            Place::Local(_) => {}
            Place::Capture(_) => {}
            Place::Field { base, .. } => {
                // The base local of a field projection can't be replaced with a constant.
                if let Place::Local(l) = base.as_ref() {
                    set.insert(*l);
                }
                scan_place(base, set);
            }
            Place::Index { base, index, .. } => {
                set.insert(*index);
                if let Place::Local(l) = base.as_ref() {
                    set.insert(*l);
                }
                scan_place(base, set);
            }
        }
    }

    fn scan_operand(op: &Operand<'_>, set: &mut HashSet<Local>) {
        match op {
            Operand::Copy(p) | Operand::Move(p) => scan_place(p, set),
            Operand::Constant(_) => {}
        }
    }

    fn scan_rvalue(rv: &crate::Rvalue, set: &mut HashSet<Local>) {
        match rv {
            crate::Rvalue::Use(op) => scan_operand(op, set),
            crate::Rvalue::BinaryOp { left, right, .. } => {
                scan_operand(left, set);
                scan_operand(right, set);
            }
            crate::Rvalue::UnaryOp { operand, .. } => scan_operand(operand, set),
            crate::Rvalue::Uint8Array(_) => {}
            crate::Rvalue::Array(_, elems) => {
                for e in elems {
                    scan_operand(e, set);
                }
            }
            crate::Rvalue::Map(_, _, entries) => {
                for (k, v) in entries {
                    scan_operand(k, set);
                    scan_operand(v, set);
                }
            }
            crate::Rvalue::Aggregate { fields, .. } => {
                for f in fields {
                    scan_operand(f, set);
                }
            }
            crate::Rvalue::Discriminant(p) | crate::Rvalue::TypeTag(p) | crate::Rvalue::Len(p) => {
                scan_place(p, set);
            }
            crate::Rvalue::IsType { operand, .. } | crate::Rvalue::IsTypeTag { operand, .. } => {
                scan_operand(operand, set);
            }
            crate::Rvalue::RuntimeIsType {
                operand,
                type_value,
            } => {
                scan_operand(operand, set);
                scan_operand(type_value, set);
            }
            crate::Rvalue::MakeClosure { captures, .. } => {
                for cap in captures {
                    scan_operand(cap, set);
                }
            }
            crate::Rvalue::MakeVirtualFunction { type_args, .. } => {
                for arg in type_args {
                    scan_operand(arg, set);
                }
            }
            crate::Rvalue::MakeBoundMethod { receiver, .. }
            | crate::Rvalue::MakeVirtualBoundMethod { receiver, .. }
            | crate::Rvalue::VirtualFieldAccess { receiver, .. } => {
                scan_operand(receiver, set);
            }
            crate::Rvalue::MakeGenericFunctionFromValue { value, .. } => {
                scan_operand(value, set);
            }
            crate::Rvalue::LoadType(_)
            | crate::Rvalue::CurrentPackage(_)
            | crate::Rvalue::MakeGenericFunction { .. } => {
                // LoadType takes no local operands.
            }
        }
    }

    let mut set = HashSet::new();

    for block in &body.blocks {
        for stmt in &block.statements {
            match &stmt.kind {
                crate::StatementKind::Assign { destination, value } => {
                    scan_place(destination, &mut set);
                    scan_rvalue(value, &mut set);
                }
                crate::StatementKind::VirtualFieldStore {
                    receiver, value, ..
                } => {
                    scan_operand(receiver, &mut set);
                    scan_operand(value, &mut set);
                }
                crate::StatementKind::Intrinsic { args, .. } => {
                    for arg in args {
                        scan_operand(arg, &mut set);
                    }
                }
                crate::StatementKind::Drop(p) => scan_place(p, &mut set),
                // Exhaustive for the same reason the substitution walk is: a
                // projected operand missed here lets copy propagation pick a
                // constant for a local that `apply_subst_to_place_locals` then
                // declines to write into the `Local`-typed position, while the
                // defining assignment is dropped regardless — leaving the
                // projection pointing at a local nothing defines.
                crate::StatementKind::FreshCell(_) | crate::StatementKind::Nop => {}
            }
        }
        if let Some(term) = &block.terminator {
            match term {
                Terminator::Call {
                    callee,
                    args,
                    runtime_id,
                    destination,
                    ..
                } => {
                    scan_operand(callee, &mut set);
                    for a in args {
                        scan_operand(a, &mut set);
                    }
                    if let Some(runtime_id) = runtime_id {
                        scan_operand(runtime_id, &mut set);
                    }
                    scan_place(destination, &mut set);
                }
                Terminator::VirtualCall {
                    args,
                    runtime_id,
                    destination,
                    ..
                } => {
                    // No callee operand: the method is resolved at runtime from
                    // `iface` (a type template, not a value local).
                    for a in args {
                        scan_operand(a, &mut set);
                    }
                    if let Some(runtime_id) = runtime_id {
                        scan_operand(runtime_id, &mut set);
                    }
                    scan_place(destination, &mut set);
                }
                Terminator::SysOp {
                    callee,
                    args,
                    runtime_id,
                    destination,
                    ..
                } => {
                    scan_operand(callee, &mut set);
                    for a in args {
                        scan_operand(a, &mut set);
                    }
                    if let Some(runtime_id) = runtime_id {
                        scan_operand(runtime_id, &mut set);
                    }
                    scan_place(destination, &mut set);
                }
                Terminator::Spawn {
                    closure,
                    name,
                    config,
                    future,
                    ..
                } => {
                    scan_operand(closure, &mut set);
                    scan_operand(name, &mut set);
                    if let Some(config) = config {
                        scan_operand(config, &mut set);
                    }
                    scan_place(future, &mut set);
                }
                Terminator::Branch { condition, .. } => scan_operand(condition, &mut set),
                Terminator::NarrowBind {
                    source,
                    destination,
                    ..
                } => {
                    scan_operand(source, &mut set);
                    scan_place(&Place::Local(*destination), &mut set);
                }
                Terminator::Switch { discriminant, .. } => {
                    scan_operand(discriminant, &mut set);
                }
                Terminator::Throw { value }
                | Terminator::Rethrow { value }
                | Terminator::ThrowIfPanic { value, .. } => {
                    scan_operand(value, &mut set);
                }
                Terminator::Await {
                    future,
                    destination,
                    ..
                } => {
                    // The awaited place can be a `Place::Index` (e.g.
                    // `await xs[_i]`): its index local is typed `Local` in the
                    // bytecode and cannot be rewritten to a constant.
                    scan_place(future, &mut set);
                    scan_place(destination, &mut set);
                }
                Terminator::AwaitAny {
                    futures,
                    destination,
                    ..
                } => {
                    scan_operand(futures, &mut set);
                    scan_place(destination, &mut set);
                }
                Terminator::ShortCircuit {
                    operand,
                    destination,
                    ..
                } => {
                    scan_operand(operand, &mut set);
                    scan_place(destination, &mut set);
                }
                Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
            }
        }
    }

    set
}

/// Count definition sites (assignments) for each local across all blocks.
///
/// A local is "single-definition" if it appears as an assignment destination
/// in exactly one statement. Locals defined in multiple branches (e.g., a temp
/// that is assigned in both arms of an if-else) have a count > 1 and must not
/// be constant-propagated.
fn count_local_defs(body: &MirFunctionBody<'_>) -> Vec<usize> {
    let mut defs = vec![0usize; body.locals.len()];

    for block in &body.blocks {
        for stmt in &block.statements {
            if let crate::StatementKind::Assign { destination, .. } = &stmt.kind {
                // Capture-rooted places have no local definition to count.
                if let Some(local) = destination.base_local() {
                    defs[local.0] += 1;
                }
            }
        }
        // Terminator destinations also count as definitions.
        if let Some(dest) = match &block.terminator {
            Some(Terminator::NarrowBind { destination, .. }) => Some(*destination),
            Some(
                Terminator::Call { destination, .. }
                | Terminator::VirtualCall { destination, .. }
                | Terminator::SysOp { destination, .. }
                | Terminator::Await { destination, .. }
                | Terminator::AwaitAny { destination, .. }
                | Terminator::ShortCircuit { destination, .. },
            ) => destination.base_local(),
            Some(Terminator::Spawn { future, .. }) => future.base_local(),
            _ => None,
        } {
            defs[dest.0] += 1;
        }
    }

    defs
}

fn count_local_uses(body: &MirFunctionBody<'_>) -> Vec<usize> {
    let mut uses = vec![0usize; body.locals.len()];

    for block in &body.blocks {
        for stmt in &block.statements {
            count_in_statement(stmt, &mut uses);
        }
        if let Some(term) = &block.terminator {
            count_in_terminator(term, &mut uses);
        }
    }

    // Count uses in catch region error locals (VM writes into these slots).
    for (_, local) in body.unwind_error_locals() {
        uses[local.0] += 1;
    }

    // The VM also materializes the caught error's `baml.errors.Context` into the
    // context (second-binding) slot at unwind time, and the BEP-042 cause-chain
    // pre-walk reads it from an *enclosing* handler — a use the static analysis
    // can't see. Keep it alive even when the `ctx` binding looks dead.
    for region in &body.catch_regions {
        if let Some(ctx_local) = region.stack_trace_local {
            uses[ctx_local.0] += 1;
        }
    }

    uses
}

fn count_in_place(p: &Place, uses: &mut [usize]) {
    let mut cur = p;
    loop {
        match cur {
            Place::Local(l) => {
                uses[l.0] += 1;
                break;
            }
            Place::Capture(_) => {
                // Captures have no local base — nothing to count.
                break;
            }
            Place::Field { base, .. } => cur = base,
            Place::Index { base, index, .. } => {
                uses[index.0] += 1;
                cur = base;
            }
        }
    }
}

fn count_in_operand(op: &Operand<'_>, uses: &mut [usize]) {
    match op {
        Operand::Copy(p) | Operand::Move(p) => count_in_place(p, uses),
        Operand::Constant(_) => {}
    }
}

fn count_in_rvalue(rv: &crate::Rvalue, uses: &mut [usize]) {
    match rv {
        crate::Rvalue::Use(op) => count_in_operand(op, uses),
        crate::Rvalue::BinaryOp { left, right, .. } => {
            count_in_operand(left, uses);
            count_in_operand(right, uses);
        }
        crate::Rvalue::UnaryOp { operand, .. } => count_in_operand(operand, uses),
        crate::Rvalue::Uint8Array(_) => {}
        crate::Rvalue::Array(_, elems) => {
            for e in elems {
                count_in_operand(e, uses);
            }
        }
        crate::Rvalue::Map(_, _, entries) => {
            for (k, v) in entries {
                count_in_operand(k, uses);
                count_in_operand(v, uses);
            }
        }
        crate::Rvalue::Aggregate { fields, .. } => {
            for f in fields {
                count_in_operand(f, uses);
            }
        }
        crate::Rvalue::Discriminant(p) => count_in_place(p, uses),
        crate::Rvalue::TypeTag(p) => count_in_place(p, uses),
        crate::Rvalue::Len(p) => count_in_place(p, uses),
        crate::Rvalue::IsType { operand, .. } | crate::Rvalue::IsTypeTag { operand, .. } => {
            count_in_operand(operand, uses);
        }
        crate::Rvalue::RuntimeIsType {
            operand,
            type_value,
        } => {
            count_in_operand(operand, uses);
            count_in_operand(type_value, uses);
        }
        crate::Rvalue::MakeClosure { captures, .. } => {
            for cap in captures {
                count_in_operand(cap, uses);
            }
        }
        crate::Rvalue::MakeVirtualFunction { type_args, .. } => {
            for arg in type_args {
                count_in_operand(arg, uses);
            }
        }
        crate::Rvalue::MakeBoundMethod { receiver, .. }
        | crate::Rvalue::MakeVirtualBoundMethod { receiver, .. }
        | crate::Rvalue::VirtualFieldAccess { receiver, .. } => {
            count_in_operand(receiver, uses);
        }
        crate::Rvalue::MakeGenericFunctionFromValue { value, .. } => {
            count_in_operand(value, uses);
        }
        crate::Rvalue::LoadType(_)
        | crate::Rvalue::CurrentPackage(_)
        | crate::Rvalue::MakeGenericFunction { .. } => {
            // No local operands.
        }
    }
}

fn count_in_statement(stmt: &crate::Statement, uses: &mut [usize]) {
    match &stmt.kind {
        crate::StatementKind::Assign { destination, value } => {
            // Count the destination place (for field/index projections)
            // but NOT for plain Local — that's a def, not a use.
            // Place::Capture is a store through a cell — the capture index is
            // not a local, so no local use to count here.
            if !matches!(destination, Place::Local(_) | Place::Capture(_)) {
                count_in_place(destination, uses);
            }
            count_in_rvalue(value, uses);
        }
        crate::StatementKind::VirtualFieldStore {
            receiver, value, ..
        } => {
            count_in_operand(receiver, uses);
            count_in_operand(value, uses);
        }
        crate::StatementKind::Drop(p) => count_in_place(p, uses),
        crate::StatementKind::FreshCell(l) => {
            uses[l.0] += 1;
        }
        crate::StatementKind::Nop => {}
        crate::StatementKind::Intrinsic { args, .. } => {
            for arg in args {
                count_in_operand(arg, uses);
            }
        }
    }
}

fn count_in_terminator(term: &Terminator<'_>, uses: &mut [usize]) {
    // For terminator destination places (Call::destination, Await::destination,
    // SysOp::destination): these are writes, so don't count plain Local
    // destinations. But if the destination is a projection (Field/Index), the
    // base local IS being read (partial update), so count it.
    let count_dest_place = |p: &Place, uses: &mut [usize]| {
        if !matches!(p, Place::Local(_) | Place::Capture(_)) {
            count_in_place(p, uses);
        }
    };

    match term {
        Terminator::Branch { condition, .. } => count_in_operand(condition, uses),
        Terminator::NarrowBind { source, .. } => count_in_operand(source, uses),
        Terminator::Switch { discriminant, .. } => count_in_operand(discriminant, uses),
        Terminator::Call {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            count_in_operand(callee, uses);
            for arg in args {
                count_in_operand(arg, uses);
            }
            if let Some(runtime_id) = runtime_id {
                count_in_operand(runtime_id, uses);
            }
            count_dest_place(destination, uses);
        }
        Terminator::VirtualCall {
            args,
            runtime_id,
            destination,
            ..
        } => {
            // No callee operand — the method is resolved at runtime from `iface`.
            for arg in args {
                count_in_operand(arg, uses);
            }
            if let Some(runtime_id) = runtime_id {
                count_in_operand(runtime_id, uses);
            }
            count_dest_place(destination, uses);
        }
        Terminator::SysOp {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            count_in_operand(callee, uses);
            for arg in args {
                count_in_operand(arg, uses);
            }
            if let Some(runtime_id) = runtime_id {
                count_in_operand(runtime_id, uses);
            }
            count_dest_place(destination, uses);
        }
        Terminator::Spawn {
            closure,
            name,
            config,
            future,
            ..
        } => {
            count_in_operand(closure, uses);
            count_in_operand(name, uses);
            if let Some(config) = config {
                count_in_operand(config, uses);
            }
            count_dest_place(future, uses);
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            // future is a READ — the future handle being consumed
            count_in_place(future, uses);
            // destination is a write
            count_dest_place(destination, uses);
        }
        Terminator::AwaitAny {
            futures,
            destination,
            ..
        } => {
            // futures is a READ — the array of handles being consumed
            count_in_operand(futures, uses);
            // destination is a write (the winning index)
            count_dest_place(destination, uses);
        }
        Terminator::Throw { value }
        | Terminator::Rethrow { value }
        | Terminator::ThrowIfPanic { value, .. } => {
            count_in_operand(value, uses);
        }
        Terminator::ShortCircuit {
            operand,
            destination,
            ..
        } => {
            count_in_operand(operand, uses);
            count_dest_place(destination, uses);
        }
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
    }
}

/// Phase 2a: Propagate trivial copies and single-use constants.
fn propagate_copies(body: &mut MirFunctionBody, arity: usize) {
    // Build substitution map: Local -> replacement Operand
    let uses = count_local_uses(body);
    let defs = count_local_defs(body);
    // Locals used as the `index` field of a `Place::Index` cannot be replaced
    // with constants — that field is typed `Local`, not `Operand`. Collect them
    // so we can exclude them from constant inlining below.
    let used_as_place_index = collect_place_index_locals(body);
    let mut subst: HashMap<Local, Operand<'_>> = HashMap::new();

    // Scan for copy-of-param: `_X = copy _Y` where Y is a param (1..=arity)
    // and single-use constants: `_X = const V` where X is used exactly once.
    //
    // SAFETY: Only propagate unnamed locals (compiler temporaries from
    // lower_to_operand / builder.temp()). Named locals (user variables from
    // AstStmt::Let) can be reassigned via AstStmt::Assign or AstStmt::AssignOp,
    // making propagation unsound.
    //
    // We additionally require defs[dest] == 1 to guard against phi-like temps
    // that are assigned in multiple branches (e.g., the result temp of an
    // if-else used directly as an arithmetic operand). Such temps have a
    // single use-site but two definition-sites; propagating the last-seen
    // constant would silently use the wrong branch value.
    for block in &body.blocks {
        for stmt in &block.statements {
            if let crate::StatementKind::Assign {
                destination: Place::Local(dest),
                value: crate::Rvalue::Use(operand),
            } = &stmt.kind
            {
                // Skip named locals — they may be reassigned
                if body.locals[dest.0].name.is_some() {
                    continue;
                }

                // Captured locals need a stable slot so emit can wrap them in a
                // cell and closure construction can pass that cell pointer.
                if body.locals[dest.0].is_captured {
                    continue;
                }

                // Skip locals with multiple definition sites (phi-like).
                if defs[dest.0] != 1 {
                    continue;
                }

                match operand {
                    Operand::Copy(Place::Local(src))
                        if src.0 >= 1 && src.0 <= arity && !used_as_place_index.contains(dest) =>
                    {
                        // Copy of param — substitute. Skip locals that appear
                        // as a Place::Index index, since removing the copy would
                        // leave the destination Place referencing a dead local.
                        subst.insert(*dest, Operand::Copy(Place::Local(*src)));
                    }
                    Operand::Constant(c)
                        if uses[dest.0] == 1 && !used_as_place_index.contains(dest) =>
                    {
                        // Single-use, single-definition constant — inline. Skip
                        // locals that appear as a Place::Index index, since that
                        // position can only hold a Local, not a Constant.
                        subst.insert(*dest, Operand::Constant(c.clone()));
                    }
                    _ => {}
                }
            }
        }
    }

    if subst.is_empty() {
        return;
    }

    // Resolve transitive chains: if _3 -> copy _1 and _4 -> copy _3,
    // follow the chain so _4 -> copy _1 directly.
    let keys: Vec<Local> = subst.keys().copied().collect();
    for key in keys {
        let mut resolved = subst[&key].clone();
        loop {
            if let Operand::Copy(Place::Local(src)) = &resolved {
                if let Some(next) = subst.get(src) {
                    resolved = next.clone();
                    continue;
                }
            }
            break;
        }
        subst.insert(key, resolved);
    }

    // Apply substitutions to all operands across all blocks
    for block in &mut body.blocks {
        for stmt in &mut block.statements {
            apply_subst_to_statement(stmt, &subst);
        }
        if let Some(term) = &mut block.terminator {
            apply_subst_to_terminator(term, &subst);
        }
    }

    // Remove the dead assignment statements (where dest is in subst)
    for block in &mut body.blocks {
        block.statements.retain(|stmt| {
            if let crate::StatementKind::Assign {
                destination: Place::Local(dest),
                ..
            } = &stmt.kind
            {
                !subst.contains_key(dest)
            } else {
                true
            }
        });
    }
}

fn apply_subst_to_operand<'db>(op: &mut Operand<'db>, subst: &HashMap<Local, Operand<'db>>) {
    match op {
        Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) => {
            // Plain local — replace the entire operand if in subst
            if let Some(new_op) = subst.get(l).cloned() {
                *op = new_op;
            }
        }
        Operand::Copy(p) | Operand::Move(p) => {
            // Projection (Field/Index) — substitute locals within the place.
            // e.g. `copy _5.0` where `_5 -> copy _1` becomes `copy _1.0`.
            apply_subst_to_place_locals(p, subst);
        }
        Operand::Constant(_) => {}
    }
}

/// Substitute all Local references within a place using the subst map.
/// Only substitutes when the substitution maps to a plain-local operand
/// (i.e., `copy _Y`), since projecting through a constant is not meaningful.
fn apply_subst_to_place_locals(p: &mut Place, subst: &HashMap<Local, Operand<'_>>) {
    match p {
        Place::Local(l) => {
            // Substitute bare local if it maps to another local
            if let Some(Operand::Copy(Place::Local(new_l)) | Operand::Move(Place::Local(new_l))) =
                subst.get(l)
            {
                *l = *new_l;
            }
        }
        Place::Capture(_) => {
            // Captures are indexed into the closure's capture array — no local to substitute.
        }
        Place::Field { base, .. } => {
            apply_subst_to_place_locals(base, subst);
        }
        Place::Index { base, index, .. } => {
            // Substitute the index local if it maps to a plain local
            if let Some(Operand::Copy(Place::Local(new_l)) | Operand::Move(Place::Local(new_l))) =
                subst.get(index).cloned()
            {
                *index = new_l;
            }
            apply_subst_to_place_locals(base, subst);
        }
    }
}

fn apply_subst_to_rvalue<'db>(rv: &mut crate::Rvalue<'db>, subst: &HashMap<Local, Operand<'db>>) {
    match rv {
        crate::Rvalue::Use(op) => apply_subst_to_operand(op, subst),
        crate::Rvalue::BinaryOp { left, right, .. } => {
            apply_subst_to_operand(left, subst);
            apply_subst_to_operand(right, subst);
        }
        crate::Rvalue::UnaryOp { operand, .. } => apply_subst_to_operand(operand, subst),
        crate::Rvalue::Uint8Array(_) => {}
        crate::Rvalue::Array(_, elems) => {
            for e in elems {
                apply_subst_to_operand(e, subst);
            }
        }
        crate::Rvalue::Map(_, _, entries) => {
            for (k, v) in entries {
                apply_subst_to_operand(k, subst);
                apply_subst_to_operand(v, subst);
            }
        }
        crate::Rvalue::Aggregate { fields, .. } => {
            for f in fields {
                apply_subst_to_operand(f, subst);
            }
        }
        crate::Rvalue::Discriminant(p) | crate::Rvalue::TypeTag(p) | crate::Rvalue::Len(p) => {
            apply_subst_to_place_locals(p, subst);
        }
        crate::Rvalue::IsType { operand, .. } | crate::Rvalue::IsTypeTag { operand, .. } => {
            apply_subst_to_operand(operand, subst);
        }
        crate::Rvalue::RuntimeIsType {
            operand,
            type_value,
        } => {
            apply_subst_to_operand(operand, subst);
            apply_subst_to_operand(type_value, subst);
        }
        crate::Rvalue::MakeClosure { captures, .. } => {
            for cap in captures {
                apply_subst_to_operand(cap, subst);
            }
        }
        crate::Rvalue::MakeVirtualFunction { type_args, .. } => {
            for arg in type_args {
                apply_subst_to_operand(arg, subst);
            }
        }
        crate::Rvalue::MakeBoundMethod { receiver, .. }
        | crate::Rvalue::MakeVirtualBoundMethod { receiver, .. }
        | crate::Rvalue::VirtualFieldAccess { receiver, .. } => {
            apply_subst_to_operand(receiver, subst);
        }
        crate::Rvalue::MakeGenericFunctionFromValue { value, .. } => {
            apply_subst_to_operand(value, subst);
        }
        crate::Rvalue::LoadType(_)
        | crate::Rvalue::CurrentPackage(_)
        | crate::Rvalue::MakeGenericFunction { .. } => {
            // No local operands — nothing to substitute.
        }
    }
}

fn apply_subst_to_statement<'db>(
    stmt: &mut crate::Statement<'db>,
    subst: &HashMap<Local, Operand<'db>>,
) {
    match &mut stmt.kind {
        crate::StatementKind::Assign { value, .. } => {
            apply_subst_to_rvalue(value, subst);
        }
        crate::StatementKind::Intrinsic { args, .. } => {
            for arg in args {
                apply_subst_to_operand(arg, subst);
            }
        }
        crate::StatementKind::VirtualFieldStore {
            receiver, value, ..
        } => {
            apply_subst_to_operand(receiver, subst);
            apply_subst_to_operand(value, subst);
        }
        // Deliberately exhaustive over operand-carrying kinds: a statement whose
        // operands are missed here keeps referring to a local that copy
        // propagation has already retired, and the emitter then loads a slot
        // nothing ever stored to.
        crate::StatementKind::Drop(_)
        | crate::StatementKind::FreshCell(_)
        | crate::StatementKind::Nop => {}
    }
}

fn apply_subst_to_terminator<'db>(
    term: &mut Terminator<'db>,
    subst: &HashMap<Local, Operand<'db>>,
) {
    match term {
        Terminator::Branch { condition, .. } => apply_subst_to_operand(condition, subst),
        Terminator::NarrowBind { source, .. } => apply_subst_to_operand(source, subst),
        Terminator::Switch { discriminant, .. } => apply_subst_to_operand(discriminant, subst),
        Terminator::Call {
            callee,
            args,
            runtime_id,
            ..
        } => {
            apply_subst_to_operand(callee, subst);
            for arg in args {
                apply_subst_to_operand(arg, subst);
            }
            if let Some(runtime_id) = runtime_id {
                apply_subst_to_operand(runtime_id, subst);
            }
        }
        Terminator::SysOp {
            callee,
            args,
            runtime_id,
            ..
        } => {
            apply_subst_to_operand(callee, subst);
            for arg in args {
                apply_subst_to_operand(arg, subst);
            }
            if let Some(runtime_id) = runtime_id {
                apply_subst_to_operand(runtime_id, subst);
            }
        }
        Terminator::VirtualCall {
            args, runtime_id, ..
        } => {
            // No callee operand — only the value args are substituted.
            for arg in args {
                apply_subst_to_operand(arg, subst);
            }
            if let Some(runtime_id) = runtime_id {
                apply_subst_to_operand(runtime_id, subst);
            }
        }
        Terminator::Spawn {
            closure,
            name,
            config,
            ..
        } => {
            apply_subst_to_operand(closure, subst);
            apply_subst_to_operand(name, subst);
            if let Some(config) = config {
                apply_subst_to_operand(config, subst);
            }
        }
        Terminator::Throw { value }
        | Terminator::Rethrow { value }
        | Terminator::ThrowIfPanic { value, .. } => {
            apply_subst_to_operand(value, subst);
        }
        Terminator::ShortCircuit { operand, .. } => {
            apply_subst_to_operand(operand, subst);
        }
        // `future` is the awaited operand (a `Place::Local`); it must be
        // substituted like any other read so a propagated copy-of-param
        // (`_tmp = copy param; await _tmp`) doesn't leave the await pointing at
        // a local whose defining copy was just dead-eliminated. `destination`
        // is a write target and is intentionally left untouched.
        Terminator::Await { future, .. } => {
            apply_subst_to_place_locals(future, subst);
        }
        // `futures` is the awaited array operand (a read); substitute it like
        // any other read so a propagated copy isn't left dangling after dead-
        // code elimination. `destination` (the index write) is left untouched.
        Terminator::AwaitAny { futures, .. } => {
            apply_subst_to_operand(futures, subst);
        }
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
    }
}

// ============================================================================
// Phase 2b: Dead local elimination with renumbering
// ============================================================================

/// Phase 2b: Remove dead locals and renumber densely.
fn eliminate_dead_locals(body: &mut MirFunctionBody, arity: usize) {
    let mut uses = count_local_uses(body);

    // Force-alive: terminator destination locals can't be removed because
    // the terminator has side effects (Call, Await, SysOp, Spawn).
    // Even if the destination local has 0 read-uses, we must keep it.
    for block in &body.blocks {
        if let Some(term) = &block.terminator {
            let dest_local = match term {
                Terminator::Call { destination, .. } => destination.base_local(),
                Terminator::VirtualCall { destination, .. } => destination.base_local(),
                Terminator::Await { destination, .. } => destination.base_local(),
                Terminator::AwaitAny { destination, .. } => destination.base_local(),
                Terminator::SysOp { destination, .. } => destination.base_local(),
                Terminator::NarrowBind { destination, .. } => Some(*destination),
                // ShortCircuit is side-effect-free (pure control flow), so its
                // destination can be dead-eliminated like any other local.
                _ => None,
            };
            if let Some(l) = dest_local {
                uses[l.0] = uses[l.0].max(1); // ensure it survives
            }
        }
    }

    // Determine which locals to keep: _0 (return) + params + any with uses > 0
    let mut old_to_new: Vec<Option<Local>> = vec![None; body.locals.len()];
    let mut new_locals: Vec<crate::LocalDecl> = Vec::new();

    for (i, local_decl) in body.locals.iter().enumerate() {
        let keep = i == 0 || i <= arity || uses[i] > 0;
        if keep {
            let new_id = Local(new_locals.len());
            old_to_new[i] = Some(new_id);
            new_locals.push(local_decl.clone());
        }
    }

    // If nothing was removed, skip rewriting
    if new_locals.len() == body.locals.len() {
        return;
    }

    // Scrub dead Assign statements: remove assignments whose destination is
    // a dead plain-Local. All Rvalue variants are pure (no side effects), so
    // this is always safe. This prevents rewrite_locals_in_statement from
    // encountering a dead local (old_to_new = None) and panicking.
    for block in &mut body.blocks {
        block.statements.retain(|stmt| {
            if let crate::StatementKind::Assign {
                destination: Place::Local(l),
                ..
            } = &stmt.kind
            {
                old_to_new[l.0].is_some() // keep only if local survived
            } else {
                true
            }
        });
    }

    // Replace ShortCircuit terminators whose destination is dead with Goto
    // to the join block. The now-unreachable eval_rhs block will be cleaned
    // up by eliminate_dead_blocks.
    for block in &mut body.blocks {
        if let Some(Terminator::ShortCircuit {
            destination: Place::Local(l),
            join,
            ..
        }) = &block.terminator
        {
            if old_to_new[l.0].is_none() {
                block.terminator = Some(Terminator::Goto { target: *join });
            }
        }
    }

    // Rewrite all Local references
    for block in &mut body.blocks {
        for stmt in &mut block.statements {
            rewrite_locals_in_statement(stmt, &old_to_new);
        }
        if let Some(term) = &mut block.terminator {
            rewrite_locals_in_terminator(term, &old_to_new);
        }
    }

    // Rewrite catch_regions error + context locals. Both the first (`e`) and
    // second (`ctx`/`st`) catch bindings have a payload local the VM writes
    // into; if the context local isn't renumbered alongside the error local,
    // the emitter computes a stale `stack_trace_slot` and the binding reads an
    // uninitialized (Null) slot — see BEP-042 baml.errors.Context nested-catch bug.
    for region in &mut body.catch_regions {
        if let Some(new_local) = old_to_new[region.error_local.0] {
            region.error_local = new_local;
        }
        if let Some(st_local) = region.stack_trace_local
            && let Some(new_local) = old_to_new[st_local.0]
        {
            region.stack_trace_local = Some(new_local);
        }
    }

    body.locals = new_locals;
}

fn remap_local(l: &mut Local, map: &[Option<Local>]) {
    *l = map[l.0].expect("local must have been kept");
}

fn remap_place(p: &mut Place, map: &[Option<Local>]) {
    match p {
        Place::Local(l) => remap_local(l, map),
        Place::Capture(_) => {
            // Capture indices index into the closure's captures array — no local to remap.
        }
        Place::Field { base, .. } => remap_place(base, map),
        Place::Index { base, index, .. } => {
            remap_local(index, map);
            remap_place(base, map);
        }
    }
}

fn remap_operand(op: &mut Operand, map: &[Option<Local>]) {
    match op {
        Operand::Copy(p) | Operand::Move(p) => remap_place(p, map),
        Operand::Constant(_) => {}
    }
}

fn remap_rvalue(rv: &mut crate::Rvalue, map: &[Option<Local>]) {
    match rv {
        crate::Rvalue::Use(op) => remap_operand(op, map),
        crate::Rvalue::BinaryOp { left, right, .. } => {
            remap_operand(left, map);
            remap_operand(right, map);
        }
        crate::Rvalue::UnaryOp { operand, .. } => remap_operand(operand, map),
        crate::Rvalue::Uint8Array(_) => {}
        crate::Rvalue::Array(_, elems) => {
            for e in elems {
                remap_operand(e, map);
            }
        }
        crate::Rvalue::Map(_, _, entries) => {
            for (k, v) in entries {
                remap_operand(k, map);
                remap_operand(v, map);
            }
        }
        crate::Rvalue::Aggregate { fields, .. } => {
            for f in fields {
                remap_operand(f, map);
            }
        }
        crate::Rvalue::Discriminant(p) | crate::Rvalue::TypeTag(p) | crate::Rvalue::Len(p) => {
            remap_place(p, map);
        }
        crate::Rvalue::IsType { operand, .. } | crate::Rvalue::IsTypeTag { operand, .. } => {
            remap_operand(operand, map);
        }
        crate::Rvalue::RuntimeIsType {
            operand,
            type_value,
        } => {
            remap_operand(operand, map);
            remap_operand(type_value, map);
        }
        crate::Rvalue::MakeClosure { captures, .. } => {
            for cap in captures {
                remap_operand(cap, map);
            }
        }
        crate::Rvalue::MakeVirtualFunction { type_args, .. } => {
            for arg in type_args {
                remap_operand(arg, map);
            }
        }
        crate::Rvalue::MakeBoundMethod { receiver, .. }
        | crate::Rvalue::MakeVirtualBoundMethod { receiver, .. }
        | crate::Rvalue::VirtualFieldAccess { receiver, .. } => {
            remap_operand(receiver, map);
        }
        crate::Rvalue::MakeGenericFunctionFromValue { value, .. } => {
            remap_operand(value, map);
        }
        crate::Rvalue::LoadType(_)
        | crate::Rvalue::CurrentPackage(_)
        | crate::Rvalue::MakeGenericFunction { .. } => {
            // No local operands — nothing to remap.
        }
    }
}

fn rewrite_locals_in_statement(stmt: &mut crate::Statement, map: &[Option<Local>]) {
    match &mut stmt.kind {
        crate::StatementKind::Assign { destination, value } => {
            remap_place(destination, map);
            remap_rvalue(value, map);
        }
        crate::StatementKind::VirtualFieldStore {
            receiver, value, ..
        } => {
            remap_operand(receiver, map);
            remap_operand(value, map);
        }
        crate::StatementKind::Drop(p) => remap_place(p, map),
        crate::StatementKind::FreshCell(l) => remap_local(l, map),
        crate::StatementKind::Nop => {}
        crate::StatementKind::Intrinsic { args, .. } => {
            for arg in args {
                remap_operand(arg, map);
            }
        }
    }
}

fn rewrite_locals_in_terminator(term: &mut Terminator, map: &[Option<Local>]) {
    match term {
        Terminator::Branch { condition, .. } => remap_operand(condition, map),
        Terminator::NarrowBind {
            source,
            destination,
            ..
        } => {
            remap_operand(source, map);
            remap_local(destination, map);
        }
        Terminator::Switch { discriminant, .. } => remap_operand(discriminant, map),
        Terminator::Call {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            remap_operand(callee, map);
            for arg in args {
                remap_operand(arg, map);
            }
            if let Some(runtime_id) = runtime_id {
                remap_operand(runtime_id, map);
            }
            remap_place(destination, map);
        }
        Terminator::SysOp {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            remap_operand(callee, map);
            for arg in args {
                remap_operand(arg, map);
            }
            if let Some(runtime_id) = runtime_id {
                remap_operand(runtime_id, map);
            }
            remap_place(destination, map);
        }
        Terminator::VirtualCall {
            args,
            runtime_id,
            destination,
            ..
        } => {
            // No callee operand — the method is resolved at runtime from `iface`.
            for arg in args {
                remap_operand(arg, map);
            }
            if let Some(runtime_id) = runtime_id {
                remap_operand(runtime_id, map);
            }
            remap_place(destination, map);
        }
        Terminator::Spawn {
            closure,
            name,
            config,
            future,
            ..
        } => {
            remap_operand(closure, map);
            remap_operand(name, map);
            if let Some(config) = config {
                remap_operand(config, map);
            }
            remap_place(future, map);
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            remap_place(future, map);
            remap_place(destination, map);
        }
        Terminator::AwaitAny {
            futures,
            destination,
            ..
        } => {
            remap_operand(futures, map);
            remap_place(destination, map);
        }
        Terminator::Throw { value }
        | Terminator::Rethrow { value }
        | Terminator::ThrowIfPanic { value, .. } => {
            remap_operand(value, map);
        }
        Terminator::ShortCircuit {
            operand,
            destination,
            ..
        } => {
            remap_operand(operand, map);
            remap_place(destination, map);
        }
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
    }
}

// ============================================================================
// Phase 4: Post-optimization MIR validation (debug only)
// ============================================================================

/// Verify MIR structural invariants after optimization.
///
/// Debug-only — catches invariant drift between lowering, optimization, and
/// downstream consumers. Modeled after V1's `verifier.rs`.
#[cfg(debug_assertions)]
fn verify_mir(body: &MirFunctionBody<'_>, name: &crate::ItemRef) {
    let num_blocks = body.blocks.len();
    let num_locals = body.locals.len();

    // 1. Block ID / index density: block.id must equal its position.
    //    (Same as V1 verifier.rs:20-28)
    for (idx, block) in body.blocks.iter().enumerate() {
        assert!(
            block.id == BlockId(idx),
            "block id/index mismatch in {}: block.id={:?}, index=bb{}",
            name,
            block.id,
            idx,
        );
    }

    // 2. Every block must be terminated.
    for block in &body.blocks {
        assert!(
            block.terminator.is_some(),
            "unterminated block {:?} in MIR function {}",
            block.id,
            name,
        );
    }

    // 3. All BlockId references in terminators must be in-range.
    for block in &body.blocks {
        if let Some(term) = &block.terminator {
            for succ in term.successors() {
                assert!(
                    succ.0 < num_blocks,
                    "dangling BlockId {:?} in terminator of {:?} in MIR function {}",
                    succ,
                    block.id,
                    name,
                );
            }
        }
    }

    // 4. All Local references must be in-range.
    //    Walk every Place/Operand in statements and terminators.
    let check_local = |l: Local, ctx: &str| {
        assert!(
            l.0 < num_locals,
            "dangling Local {l} in {ctx} of MIR function {name}",
        );
    };

    let check_place = |p: &Place, ctx: &str| {
        let mut cur = p;
        loop {
            match cur {
                Place::Local(l) => {
                    check_local(*l, ctx);
                    break;
                }
                Place::Capture(_) => {
                    // Capture index — no local to check.
                    break;
                }
                Place::Field { base, .. } => cur = base,
                Place::Index { base, index, .. } => {
                    check_local(*index, ctx);
                    cur = base;
                }
            }
        }
    };

    let check_operand = |op: &Operand<'_>, ctx: &str| match op {
        Operand::Copy(p) | Operand::Move(p) => check_place(p, ctx),
        Operand::Constant(_) => {}
    };

    for block in &body.blocks {
        let blk = format!("{:?}", block.id);
        for stmt in &block.statements {
            match &stmt.kind {
                crate::StatementKind::Assign { destination, value } => {
                    check_place(destination, &blk);
                    match value {
                        crate::Rvalue::Use(op) => check_operand(op, &blk),
                        crate::Rvalue::BinaryOp { left, right, .. } => {
                            check_operand(left, &blk);
                            check_operand(right, &blk);
                        }
                        crate::Rvalue::UnaryOp { operand, .. } => check_operand(operand, &blk),
                        crate::Rvalue::Uint8Array(_) => {}
                        crate::Rvalue::Array(_, elems) => {
                            for e in elems {
                                check_operand(e, &blk);
                            }
                        }
                        crate::Rvalue::Map(_, _, entries) => {
                            for (k, v) in entries {
                                check_operand(k, &blk);
                                check_operand(v, &blk);
                            }
                        }
                        crate::Rvalue::Aggregate { fields, .. } => {
                            for f in fields {
                                check_operand(f, &blk);
                            }
                        }
                        crate::Rvalue::Discriminant(p)
                        | crate::Rvalue::TypeTag(p)
                        | crate::Rvalue::Len(p) => check_place(p, &blk),
                        crate::Rvalue::IsType { operand, .. }
                        | crate::Rvalue::IsTypeTag { operand, .. } => {
                            check_operand(operand, &blk);
                        }
                        crate::Rvalue::RuntimeIsType {
                            operand,
                            type_value,
                        } => {
                            check_operand(operand, &blk);
                            check_operand(type_value, &blk);
                        }
                        crate::Rvalue::MakeClosure { captures, .. } => {
                            for cap in captures {
                                check_operand(cap, &blk);
                            }
                        }
                        crate::Rvalue::MakeVirtualFunction { type_args, .. } => {
                            for arg in type_args {
                                check_operand(arg, &blk);
                            }
                        }
                        crate::Rvalue::MakeBoundMethod { receiver, .. }
                        | crate::Rvalue::MakeVirtualBoundMethod { receiver, .. }
                        | crate::Rvalue::VirtualFieldAccess { receiver, .. } => {
                            check_operand(receiver, &blk);
                        }
                        crate::Rvalue::MakeGenericFunctionFromValue { value, .. } => {
                            check_operand(value, &blk);
                        }
                        crate::Rvalue::LoadType(_)
                        | crate::Rvalue::CurrentPackage(_)
                        | crate::Rvalue::MakeGenericFunction { .. } => {
                            // LoadType takes no local operands — nothing to check.
                        }
                    }
                }
                crate::StatementKind::VirtualFieldStore {
                    receiver, value, ..
                } => {
                    check_operand(receiver, &blk);
                    check_operand(value, &blk);
                }
                crate::StatementKind::Intrinsic { args, .. } => {
                    for arg in args {
                        check_operand(arg, &blk);
                    }
                }
                crate::StatementKind::Drop(p) => check_place(p, &blk),
                crate::StatementKind::FreshCell(l) => check_local(*l, &blk),
                // Exhaustive rather than wildcarded: an operand-carrying
                // statement kind that skips this check loses the one cheap
                // tripwire for a reference to a retired local.
                crate::StatementKind::Nop => {}
            }
        }
    }

    // 4b. Also check Local references in terminators.
    for block in &body.blocks {
        let blk = format!("{:?}", block.id);
        if let Some(term) = &block.terminator {
            match term {
                Terminator::Branch { condition, .. } => check_operand(condition, &blk),
                Terminator::NarrowBind {
                    source,
                    destination,
                    ..
                } => {
                    check_operand(source, &blk);
                    check_local(*destination, &blk);
                }
                Terminator::Switch { discriminant, .. } => check_operand(discriminant, &blk),
                Terminator::Call {
                    callee,
                    args,
                    runtime_id,
                    destination,
                    ..
                } => {
                    check_operand(callee, &blk);
                    for a in args {
                        check_operand(a, &blk);
                    }
                    if let Some(runtime_id) = runtime_id {
                        check_operand(runtime_id, &blk);
                    }
                    check_place(destination, &blk);
                }
                Terminator::VirtualCall {
                    args,
                    runtime_id,
                    destination,
                    ..
                } => {
                    // No callee operand — the method is resolved at runtime from `iface`.
                    for a in args {
                        check_operand(a, &blk);
                    }
                    if let Some(runtime_id) = runtime_id {
                        check_operand(runtime_id, &blk);
                    }
                    check_place(destination, &blk);
                }
                Terminator::SysOp {
                    callee,
                    args,
                    runtime_id,
                    destination,
                    ..
                } => {
                    check_operand(callee, &blk);
                    for a in args {
                        check_operand(a, &blk);
                    }
                    if let Some(runtime_id) = runtime_id {
                        check_operand(runtime_id, &blk);
                    }
                    check_place(destination, &blk);
                }
                Terminator::Spawn {
                    closure,
                    name,
                    config,
                    future,
                    ..
                } => {
                    check_operand(closure, &blk);
                    check_operand(name, &blk);
                    if let Some(config) = config {
                        check_operand(config, &blk);
                    }
                    check_place(future, &blk);
                }
                Terminator::Await {
                    future,
                    destination,
                    ..
                } => {
                    check_place(future, &blk);
                    check_place(destination, &blk);
                }
                Terminator::AwaitAny {
                    futures,
                    destination,
                    ..
                } => {
                    check_operand(futures, &blk);
                    check_place(destination, &blk);
                }
                Terminator::Throw { value }
                | Terminator::Rethrow { value }
                | Terminator::ThrowIfPanic { value, .. } => {
                    check_operand(value, &blk);
                }
                Terminator::ShortCircuit {
                    operand,
                    destination,
                    ..
                } => {
                    check_operand(operand, &blk);
                    check_place(destination, &blk);
                }
                Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
            }
        }
    }

    // 5. Exhaustive switches must have Unreachable otherwise block.
    //    (Same as V1 verifier.rs:72-88)
    for block in &body.blocks {
        if let Some(Terminator::Switch {
            otherwise,
            exhaustive,
            ..
        }) = &block.terminator
        {
            if *exhaustive {
                let otherwise_block = &body.blocks[otherwise.0];
                let is_unreachable = otherwise_block.statements.is_empty()
                    && matches!(otherwise_block.terminator, Some(Terminator::Unreachable));
                assert!(
                    is_unreachable,
                    "exhaustive switch in {:?} has non-unreachable default block {:?} in MIR function {}",
                    block.id, otherwise, name,
                );
            }
        }
    }

    // 7. catch_regions: block IDs and locals must be valid.
    for (i, region) in body.catch_regions.iter().enumerate() {
        assert!(
            region.body_entry.0 < num_blocks,
            "dangling body_entry {:?} in catch_region[{i}] of MIR function {name}",
            region.body_entry,
        );
        assert!(
            region.handler.0 < num_blocks,
            "dangling handler {:?} in catch_region[{i}] of MIR function {name}",
            region.handler,
        );
        for b in &region.body_blocks {
            assert!(
                b.0 < num_blocks,
                "dangling body block {b:?} in catch_region[{i}] of MIR function {name}",
            );
        }
        assert!(
            region.error_local.0 < num_locals,
            "dangling error_local {} in catch_region[{i}] of MIR function {name}",
            region.error_local,
        );
    }

    // 9. Entry block must be valid.
    assert!(
        body.entry.0 < num_blocks,
        "entry block {:?} out of range in MIR function {}",
        body.entry,
        name,
    );
}

// ============================================================================
// Phase 3: RPO block reordering
// ============================================================================

/// Phase 3: Reorder blocks into reverse-post-order.
///
/// After dead block elimination (Phase 1), blocks are densely packed but their
/// IDs still follow allocation order rather than execution order. RPO reordering
/// ensures that `bb0 → bb1 → bb2 → ...` corresponds to typical execution flow,
/// making the MIR output much more readable.
fn reorder_blocks_rpo(body: &mut MirFunctionBody) {
    let num_blocks = body.blocks.len();
    if num_blocks <= 1 {
        return;
    }

    // Compute RPO via iterative DFS. Seed with entry AND exception handler
    // blocks (reachable at runtime via exception table, not CFG edges).
    let mut visited = vec![false; num_blocks];
    let mut post_order: Vec<BlockId> = Vec::with_capacity(num_blocks);
    let mut stack: Vec<(BlockId, bool)> = vec![(body.entry, false)];
    for region in &body.catch_regions {
        stack.push((region.handler, false));
    }

    while let Some((block_id, processed)) = stack.pop() {
        if processed {
            post_order.push(block_id);
            continue;
        }
        if visited[block_id.0] {
            continue;
        }
        visited[block_id.0] = true;
        stack.push((block_id, true)); // push for post-order recording

        if let Some(term) = &body.blocks[block_id.0].terminator {
            // Push successors in reverse order so first successor is visited first
            let succs = term.successors();
            for &succ in succs.iter().rev() {
                if !visited[succ.0] {
                    stack.push((succ, false));
                }
            }
        }
    }

    // Reverse post_order to get RPO
    post_order.reverse();

    // Check if already in RPO order (optimization: skip rewriting if nothing changes)
    let already_ordered = post_order.iter().enumerate().all(|(i, b)| b.0 == i);
    if already_ordered && post_order.len() == num_blocks {
        return;
    }

    // Build old -> new mapping
    let mut old_to_new: Vec<Option<BlockId>> = vec![None; num_blocks];
    for (new_idx, &old_id) in post_order.iter().enumerate() {
        old_to_new[old_id.0] = Some(BlockId(new_idx));
    }

    // Reorder blocks and rewrite internal BlockId references
    let mut new_blocks: Vec<BasicBlock<'_>> = Vec::with_capacity(post_order.len());
    for &old_id in &post_order {
        let mut block = body.blocks[old_id.0].clone();
        block.id = old_to_new[old_id.0].unwrap();
        if let Some(term) = &mut block.terminator {
            rewrite_block_ids_in_terminator(term, &old_to_new);
        }
        new_blocks.push(block);
    }

    // Rewrite entry
    body.entry = old_to_new[body.entry.0].expect("entry must be in RPO");

    rewrite_catch_region_blocks(&mut body.catch_regions, &old_to_new);

    body.blocks = new_blocks;
}
