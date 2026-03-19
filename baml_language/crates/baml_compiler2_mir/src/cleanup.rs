//! Post-lowering cleanup pass for MIR functions.
//!
//! Runs after `MirBuilder::build()` and performs:
//! 1. Dead block elimination (reachability-based)
//! 2. Copy propagation + dead local elimination
//! 3. RPO block reordering (Phase 3)

use std::collections::{HashMap, HashSet, VecDeque};

use baml_base::Name;

use crate::{
    BasicBlock, BlockId, Local, MirFunction, MirFunctionBody, MirFunctionKind, Operand, Place,
    Terminator,
};

/// Run all cleanup phases on a MIR function.
pub(crate) fn cleanup_function(func: &mut MirFunction) {
    let MirFunctionKind::Bytecode(body) = &mut func.kind else {
        return; // nothing to clean up on builtins
    };
    eliminate_dead_blocks(body);
    propagate_copies(body, func.arity);
    eliminate_dead_locals(body, func.arity);
    reorder_blocks_rpo(body);

    #[cfg(debug_assertions)]
    verify_mir(body, &func.item_ref);
}

/// Run all cleanup phases directly on a `MirFunctionBody`.
///
/// Used for let-binding initializers, which are lowered as bodies without
/// the enclosing `MirFunction` wrapper (arity = 0).
pub(crate) fn cleanup_function_body(body: &mut MirFunctionBody) {
    eliminate_dead_blocks(body);
    propagate_copies(body, 0);
    eliminate_dead_locals(body, 0);
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
    // BFS to find all reachable blocks
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(body.entry);
    reachable.insert(body.entry);

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
    let mut new_blocks: Vec<BasicBlock> = Vec::new();
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

    // Rewrite unwind_error_locals keys
    let old_unwind = std::mem::take(&mut body.unwind_error_locals);
    for (old_block, local) in old_unwind {
        if let Some(new_block) = old_to_new[old_block.0] {
            body.unwind_error_locals.insert(new_block, local);
        }
    }

    body.blocks = new_blocks;
}

/// Rewrite all BlockId references in a terminator using old->new mapping.
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
        Terminator::Call { target, unwind, .. } => {
            remap(target);
            if let Some(u) = unwind {
                remap(u);
            }
        }
        Terminator::Unreachable => {}
        Terminator::DispatchFuture { resume, .. } => remap(resume),
        Terminator::Await { target, unwind, .. } => {
            remap(target);
            if let Some(u) = unwind {
                remap(u);
            }
        }
        Terminator::Throw { .. } => {}
    }
}

// ============================================================================
// Phase 2a: Copy propagation
// ============================================================================

/// Count uses of each Local across all blocks and unwind_error_locals.
fn count_local_uses(body: &MirFunctionBody) -> Vec<usize> {
    let mut uses = vec![0usize; body.locals.len()];

    for block in &body.blocks {
        for stmt in &block.statements {
            count_in_statement(stmt, &mut uses);
        }
        if let Some(term) = &block.terminator {
            count_in_terminator(term, &mut uses);
        }
    }

    // Count uses in unwind_error_locals values
    for (_, local) in &body.unwind_error_locals {
        uses[local.0] += 1;
    }

    uses
}

fn count_in_place(p: &Place, uses: &mut Vec<usize>) {
    let mut cur = p;
    loop {
        match cur {
            Place::Local(l) => {
                uses[l.0] += 1;
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

fn count_in_operand(op: &Operand, uses: &mut Vec<usize>) {
    match op {
        Operand::Copy(p) | Operand::Move(p) => count_in_place(p, uses),
        Operand::Constant(_) => {}
    }
}

fn count_in_rvalue(rv: &crate::Rvalue, uses: &mut Vec<usize>) {
    match rv {
        crate::Rvalue::Use(op) => count_in_operand(op, uses),
        crate::Rvalue::BinaryOp { left, right, .. } => {
            count_in_operand(left, uses);
            count_in_operand(right, uses);
        }
        crate::Rvalue::UnaryOp { operand, .. } => count_in_operand(operand, uses),
        crate::Rvalue::Array(elems) => {
            for e in elems {
                count_in_operand(e, uses);
            }
        }
        crate::Rvalue::Map(entries) => {
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
        crate::Rvalue::IsType { operand, .. } => count_in_operand(operand, uses),
    }
}

fn count_in_statement(stmt: &crate::Statement, uses: &mut Vec<usize>) {
    match &stmt.kind {
        crate::StatementKind::Assign { destination, value } => {
            // Count the destination place (for field/index projections)
            // but NOT for plain Local — that's a def, not a use
            if !matches!(destination, Place::Local(_)) {
                count_in_place(destination, uses);
            }
            count_in_rvalue(value, uses);
        }
        crate::StatementKind::Drop(p) => count_in_place(p, uses),
        crate::StatementKind::Unwatch(l) => {
            uses[l.0] += 1;
        }
        crate::StatementKind::WatchOptions { local, filter } => {
            uses[local.0] += 1;
            count_in_operand(filter, uses);
        }
        crate::StatementKind::WatchNotify(l) => {
            uses[l.0] += 1;
        }
        crate::StatementKind::Assert(op) => count_in_operand(op, uses),
        crate::StatementKind::VizEnter(_)
        | crate::StatementKind::VizExit(_)
        | crate::StatementKind::NotifyBlock { .. }
        | crate::StatementKind::Nop => {}
    }
}

fn count_in_terminator(term: &Terminator, uses: &mut Vec<usize>) {
    // For terminator destination places (Call::destination, Await::destination,
    // DispatchFuture::future): these are writes, so don't count plain Local
    // destinations. But if the destination is a projection (Field/Index), the
    // base local IS being read (partial update), so count it.
    let count_dest_place = |p: &Place, uses: &mut Vec<usize>| {
        if !matches!(p, Place::Local(_)) {
            count_in_place(p, uses);
        }
    };

    match term {
        Terminator::Branch { condition, .. } => count_in_operand(condition, uses),
        Terminator::Switch { discriminant, .. } => count_in_operand(discriminant, uses),
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        } => {
            count_in_operand(callee, uses);
            for arg in args {
                count_in_operand(arg, uses);
            }
            count_dest_place(destination, uses);
        }
        Terminator::DispatchFuture {
            callee,
            args,
            future,
            ..
        } => {
            count_in_operand(callee, uses);
            for arg in args {
                count_in_operand(arg, uses);
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
        Terminator::Throw { value } => count_in_operand(value, uses),
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
    }
}

/// Phase 2a: Propagate trivial copies and single-use constants.
fn propagate_copies(body: &mut MirFunctionBody, arity: usize) {
    // Build substitution map: Local -> replacement Operand
    let uses = count_local_uses(body);
    let mut subst: HashMap<Local, Operand> = HashMap::new();

    // Scan for copy-of-param: `_X = copy _Y` where Y is a param (1..=arity)
    // and single-use constants: `_X = const V` where X is used exactly once.
    //
    // SAFETY: Only propagate unnamed locals (compiler temporaries from
    // lower_to_operand / builder.temp()). Named locals (user variables from
    // AstStmt::Let) can be reassigned via AstStmt::Assign or AstStmt::AssignOp,
    // making propagation unsound. Unnamed temps are always fresh single-definition
    // locals, so this is safe.
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

                match operand {
                    Operand::Copy(Place::Local(src)) if src.0 >= 1 && src.0 <= arity => {
                        // Copy of param — substitute
                        subst.insert(*dest, Operand::Copy(Place::Local(*src)));
                    }
                    Operand::Constant(c) if uses[dest.0] == 1 => {
                        // Single-use constant — inline
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
    let keys: Vec<Local> = subst.keys().cloned().collect();
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

fn apply_subst_to_operand(op: &mut Operand, subst: &HashMap<Local, Operand>) {
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
fn apply_subst_to_place_locals(p: &mut Place, subst: &HashMap<Local, Operand>) {
    match p {
        Place::Local(l) => {
            // Substitute bare local if it maps to another local
            if let Some(Operand::Copy(Place::Local(new_l)))
            | Some(Operand::Move(Place::Local(new_l))) = subst.get(l)
            {
                *l = *new_l;
            }
        }
        Place::Field { base, .. } => {
            apply_subst_to_place_locals(base, subst);
        }
        Place::Index { base, index, .. } => {
            // Substitute the index local if it maps to a plain local
            if let Some(Operand::Copy(Place::Local(new_l)))
            | Some(Operand::Move(Place::Local(new_l))) = subst.get(index).cloned()
            {
                *index = new_l;
            }
            apply_subst_to_place_locals(base, subst);
        }
    }
}

fn apply_subst_to_rvalue(rv: &mut crate::Rvalue, subst: &HashMap<Local, Operand>) {
    match rv {
        crate::Rvalue::Use(op) => apply_subst_to_operand(op, subst),
        crate::Rvalue::BinaryOp { left, right, .. } => {
            apply_subst_to_operand(left, subst);
            apply_subst_to_operand(right, subst);
        }
        crate::Rvalue::UnaryOp { operand, .. } => apply_subst_to_operand(operand, subst),
        crate::Rvalue::Array(elems) => {
            for e in elems {
                apply_subst_to_operand(e, subst);
            }
        }
        crate::Rvalue::Map(entries) => {
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
        crate::Rvalue::IsType { operand, .. } => apply_subst_to_operand(operand, subst),
    }
}

fn apply_subst_to_statement(stmt: &mut crate::Statement, subst: &HashMap<Local, Operand>) {
    match &mut stmt.kind {
        crate::StatementKind::Assign { value, .. } => {
            apply_subst_to_rvalue(value, subst);
        }
        crate::StatementKind::WatchOptions { filter, .. } => {
            apply_subst_to_operand(filter, subst);
        }
        crate::StatementKind::Assert(op) => {
            apply_subst_to_operand(op, subst);
        }
        _ => {}
    }
}

fn apply_subst_to_terminator(term: &mut Terminator, subst: &HashMap<Local, Operand>) {
    match term {
        Terminator::Branch { condition, .. } => apply_subst_to_operand(condition, subst),
        Terminator::Switch { discriminant, .. } => apply_subst_to_operand(discriminant, subst),
        Terminator::Call { callee, args, .. } => {
            apply_subst_to_operand(callee, subst);
            for arg in args {
                apply_subst_to_operand(arg, subst);
            }
        }
        Terminator::DispatchFuture { callee, args, .. } => {
            apply_subst_to_operand(callee, subst);
            for arg in args {
                apply_subst_to_operand(arg, subst);
            }
        }
        Terminator::Throw { value } => apply_subst_to_operand(value, subst),
        Terminator::Goto { .. }
        | Terminator::Return
        | Terminator::Unreachable
        | Terminator::Await { .. } => {}
    }
}

// ============================================================================
// Phase 2b: Dead local elimination with renumbering
// ============================================================================

/// Phase 2b: Remove dead locals and renumber densely.
fn eliminate_dead_locals(body: &mut MirFunctionBody, arity: usize) {
    let mut uses = count_local_uses(body);

    // Force-alive: terminator destination locals can't be removed because
    // the terminator has side effects (Call, Await, DispatchFuture).
    // Even if the destination local has 0 read-uses, we must keep it.
    for block in &body.blocks {
        if let Some(term) = &block.terminator {
            let dest_local = match term {
                Terminator::Call { destination, .. } => Some(destination.base_local()),
                Terminator::Await { destination, .. } => Some(destination.base_local()),
                Terminator::DispatchFuture { future, .. } => Some(future.base_local()),
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
        let keep = i == 0              // return place
            || i <= arity              // parameter
            || uses[i] > 0            // has uses (including force-alive)
            || local_decl.is_watched; // watched variable
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

    // Rewrite all Local references
    for block in &mut body.blocks {
        for stmt in &mut block.statements {
            rewrite_locals_in_statement(stmt, &old_to_new);
        }
        if let Some(term) = &mut block.terminator {
            rewrite_locals_in_terminator(term, &old_to_new);
        }
    }

    // Rewrite unwind_error_locals values
    let old_unwind = std::mem::take(&mut body.unwind_error_locals);
    for (block_id, old_local) in old_unwind {
        if let Some(new_local) = old_to_new[old_local.0] {
            body.unwind_error_locals.insert(block_id, new_local);
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
        crate::Rvalue::Array(elems) => {
            for e in elems {
                remap_operand(e, map);
            }
        }
        crate::Rvalue::Map(entries) => {
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
        crate::Rvalue::IsType { operand, .. } => remap_operand(operand, map),
    }
}

fn rewrite_locals_in_statement(stmt: &mut crate::Statement, map: &[Option<Local>]) {
    match &mut stmt.kind {
        crate::StatementKind::Assign { destination, value } => {
            remap_place(destination, map);
            remap_rvalue(value, map);
        }
        crate::StatementKind::Drop(p) => remap_place(p, map),
        crate::StatementKind::Unwatch(l) => remap_local(l, map),
        crate::StatementKind::WatchOptions { local, filter } => {
            remap_local(local, map);
            remap_operand(filter, map);
        }
        crate::StatementKind::WatchNotify(l) => remap_local(l, map),
        crate::StatementKind::Assert(op) => remap_operand(op, map),
        crate::StatementKind::VizEnter(_)
        | crate::StatementKind::VizExit(_)
        | crate::StatementKind::NotifyBlock { .. }
        | crate::StatementKind::Nop => {}
    }
}

fn rewrite_locals_in_terminator(term: &mut Terminator, map: &[Option<Local>]) {
    match term {
        Terminator::Branch { condition, .. } => remap_operand(condition, map),
        Terminator::Switch { discriminant, .. } => remap_operand(discriminant, map),
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        } => {
            remap_operand(callee, map);
            for arg in args {
                remap_operand(arg, map);
            }
            remap_place(destination, map);
        }
        Terminator::DispatchFuture {
            callee,
            args,
            future,
            ..
        } => {
            remap_operand(callee, map);
            for arg in args {
                remap_operand(arg, map);
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
        Terminator::Throw { value } => remap_operand(value, map),
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
    }
}

// ============================================================================
// Phase 4: Post-cleanup MIR validation (debug only)
// ============================================================================

/// Verify MIR structural invariants after cleanup.
///
/// Debug-only — catches invariant drift between lowering, cleanup, and
/// downstream consumers. Modeled after V1's `verifier.rs`.
#[cfg(debug_assertions)]
fn verify_mir(body: &MirFunctionBody, name: &crate::ItemRef) {
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
            "dangling Local {} in {} of MIR function {}",
            l,
            ctx,
            name,
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
                Place::Field { base, .. } => cur = base,
                Place::Index { base, index, .. } => {
                    check_local(*index, ctx);
                    cur = base;
                }
            }
        }
    };

    let check_operand = |op: &Operand, ctx: &str| match op {
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
                        crate::Rvalue::Array(elems) => {
                            for e in elems {
                                check_operand(e, &blk);
                            }
                        }
                        crate::Rvalue::Map(entries) => {
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
                        crate::Rvalue::IsType { operand, .. } => check_operand(operand, &blk),
                    }
                }
                crate::StatementKind::Drop(p) => check_place(p, &blk),
                crate::StatementKind::Unwatch(l) => check_local(*l, &blk),
                crate::StatementKind::WatchOptions { local, filter } => {
                    check_local(*local, &blk);
                    check_operand(filter, &blk);
                }
                crate::StatementKind::WatchNotify(l) => check_local(*l, &blk),
                crate::StatementKind::Assert(op) => check_operand(op, &blk),
                _ => {}
            }
        }
    }

    // 4b. Also check Local references in terminators.
    for block in &body.blocks {
        let blk = format!("{:?}", block.id);
        if let Some(term) = &block.terminator {
            match term {
                Terminator::Branch { condition, .. } => check_operand(condition, &blk),
                Terminator::Switch { discriminant, .. } => check_operand(discriminant, &blk),
                Terminator::Call {
                    callee,
                    args,
                    destination,
                    ..
                } => {
                    check_operand(callee, &blk);
                    for a in args {
                        check_operand(a, &blk);
                    }
                    check_place(destination, &blk);
                }
                Terminator::DispatchFuture {
                    callee,
                    args,
                    future,
                    ..
                } => {
                    check_operand(callee, &blk);
                    for a in args {
                        check_operand(a, &blk);
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
                Terminator::Throw { value } => check_operand(value, &blk),
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

    // 6. Watch invariants: watched locals must have names, watch statements
    //    must reference watched locals.
    //    (Same as V1 verifier.rs:90-145)
    for (idx, decl) in body.locals.iter().enumerate() {
        if decl.is_watched {
            assert!(
                decl.name.is_some(),
                "watched local _{} must have a user-visible name in MIR function {}",
                idx,
                name,
            );
        }
    }

    for block in &body.blocks {
        for stmt in &block.statements {
            let watch_local = match &stmt.kind {
                crate::StatementKind::Unwatch(l)
                | crate::StatementKind::WatchNotify(l)
                | crate::StatementKind::WatchOptions { local: l, .. } => Some(*l),
                _ => None,
            };
            if let Some(local) = watch_local {
                let decl = &body.locals[local.0];
                assert!(
                    decl.is_watched,
                    "watch statement references non-watched local _{} in MIR function {}",
                    local.0, name,
                );
            }
        }
    }

    // 7. unwind_error_locals: keys must be valid BlockIds, values must be valid Locals.
    for (&block_id, &local) in &body.unwind_error_locals {
        assert!(
            block_id.0 < num_blocks,
            "dangling BlockId {:?} in unwind_error_locals of MIR function {}",
            block_id,
            name,
        );
        assert!(
            local.0 < num_locals,
            "dangling Local {} in unwind_error_locals of MIR function {}",
            local,
            name,
        );
    }

    // 8. Entry block must be valid.
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

    // Compute RPO via iterative DFS
    let mut visited = vec![false; num_blocks];
    let mut post_order: Vec<BlockId> = Vec::with_capacity(num_blocks);
    let mut stack: Vec<(BlockId, bool)> = vec![(body.entry, false)];

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
    let mut new_blocks: Vec<BasicBlock> = Vec::with_capacity(post_order.len());
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

    // Rewrite unwind_error_locals keys
    let old_unwind = std::mem::take(&mut body.unwind_error_locals);
    for (old_block, local) in old_unwind {
        if let Some(new_block) = old_to_new[old_block.0] {
            body.unwind_error_locals.insert(new_block, local);
        }
    }

    body.blocks = new_blocks;
}
