//! Loop passes: hoist what a loop's test recomputes on every trip
//! ([`hoist_invariants`]), then test the loop at its bottom instead of its
//! top ([`rotate_loops`]).
//!
//! # Rotation
//!
//! Lowering gives `while` and C-style `for` a header that tests the condition
//! and a latch that jumps back to it, so every iteration pays for the jump as
//! well as the test:
//!
//! ```text
//! header: test; branch -> [body, exit]        header: test; branch -> [body, exit]
//! body:   ...                                 body:   ...
//! latch:  step; goto -> header          =>    latch:  step; goto -> bottom
//!                                             bottom: test; branch -> [body, exit]
//! ```
//!
//! The header's test is copied into one new block that every in-loop
//! predecessor of the header enters instead. The copy falls out of the latch
//! and branches straight back to the body, so the back edge costs one
//! conditional jump where it cost a conditional jump and a jump.
//!
//! A `for` over an iterator tests the result of a call: its header calls
//! `next`, and the block the call returns to branches on what came back. The
//! copy is then the call in one block and the branch in the next.
//!
//! This is tail duplication and nothing else: on every path the program runs
//! the same statements in the same order under the same handler, so no trap,
//! effect, or heap read moves. The original header still runs the test once,
//! on entry, which keeps a zero-trip loop a zero-trip loop.

use std::collections::{HashMap, HashSet};

use crate::{
    BasicBlock, BlockId, IndexKind, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind,
    Terminator, memory, reducible,
};

/// Most statements a loop test may hold and still be copied.
const MAX_TEST_STATEMENTS: usize = 8;

/// A loop whose test can move to the bottom.
struct Rotation {
    /// The test: the header, then each single-entry block its `goto`s and
    /// calls lead through, the last of which ends in the branch that leaves
    /// the loop.
    test: Vec<usize>,
    /// The header's predecessors inside the loop.
    latches: Vec<usize>,
}

/// Rotate every loop whose header is a small test reached by an unconditional
/// back edge.
pub(super) fn rotate_loops(body: &mut MirFunctionBody<'_>, arity: usize) {
    // A rotation retires every unconditional back edge of one header and
    // creates none, so the loop count bounds the work; the block count bounds
    // that.
    for _ in 0..body.blocks.len() {
        let Some(rotation) = find_rotation(body) else {
            return;
        };
        rotate(body, arity, &rotation);
    }
}

fn find_rotation(body: &MirFunctionBody<'_>) -> Option<Rotation> {
    let entry = body.entry.0;
    let succs: Vec<Vec<usize>> = body
        .blocks
        .iter()
        .map(|block| {
            block
                .terminator
                .iter()
                .flat_map(Terminator::successors)
                .chain(block.unwind)
                .map(|b| b.0)
                .collect()
        })
        .collect();
    let rpo = reducible::reverse_postorder(entry, &succs);
    if !reducible::has_cycle(&succs, &rpo) {
        return None;
    }
    let idom = reducible::immediate_dominators(entry, &succs, &rpo);
    let dominates = |a: usize, mut b: usize| loop {
        if a == b {
            return true;
        }
        if b == entry {
            return false;
        }
        b = idom[b];
    };
    let mut preds = vec![Vec::new(); body.blocks.len()];
    for &u in &rpo {
        for &v in &succs[u] {
            preds[v].push(u);
        }
    }

    rpo.iter().find_map(|&header| {
        let head = &body.blocks[header];
        // The VM lands in a handler with its error slots; a copy would not be
        // a landing.
        if head.landing.is_some() {
            return None;
        }
        let latches: Vec<usize> = preds[header]
            .iter()
            .copied()
            .filter(|&pred| dominates(header, pred))
            .collect();
        // Only an unconditional back edge has a jump to save. This is also
        // what stops a rotated loop, whose back edge is the copied branch,
        // from being rotated again.
        if !latches.iter().any(|&latch| {
            matches!(
                body.blocks[latch].terminator,
                Some(Terminator::Goto { target }) if target.0 == header
            )
        }) {
            return None;
        }

        let mut test = vec![header];
        let mut statements = head.statements.len();
        let (then_block, else_block) = loop {
            let last = &body.blocks[*test.last().expect("the header")];
            let target = match &last.terminator {
                Some(Terminator::Goto { target }) => target,
                // A call the test makes is copied with it, and counts as a
                // statement of it. Its handler, if it has one, is entered
                // from the copy as it is from the original.
                Some(Terminator::Call { target, .. } | Terminator::VirtualCall { target, .. }) => {
                    statements += 1;
                    target
                }
                Some(Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                }) => break (then_block.0, else_block.0),
                _ => return None,
            };
            let next = &body.blocks[target.0];
            // The copy runs under the header's handler and inside the
            // header's inlined call, so every block of the test must already
            // run under both, and none may be entered from elsewhere.
            if test.contains(&target.0)
                || preds[target.0].len() != 1
                || next.landing.is_some()
                || next.unwind != head.unwind
                || next.handling != head.handling
                || next.shielded != head.shielded
                || next.inline != head.inline
            {
                return None;
            }
            test.push(target.0);
            statements += next.statements.len();
        };
        if statements > MAX_TEST_STATEMENTS {
            return None;
        }

        // The natural loop: every block that reaches a latch without passing
        // through the header.
        let mut in_loop = vec![false; body.blocks.len()];
        in_loop[header] = true;
        let mut pending = latches.clone();
        while let Some(block) = pending.pop() {
            if !in_loop[block] {
                in_loop[block] = true;
                pending.extend(&preds[block]);
            }
        }
        // The branch must be the loop's test: one way stays in, one leaves.
        let stays = match (in_loop[then_block], in_loop[else_block]) {
            (true, false) => then_block,
            (false, true) => else_block,
            _ => return None,
        };
        if test.contains(&stays) {
            return None;
        }
        Some(Rotation { test, latches })
    })
}

fn rotate(body: &mut MirFunctionBody<'_>, arity: usize, rotation: &Rotation) {
    let header = rotation.test[0];
    let bottom = BlockId(body.blocks.len());
    let head = &body.blocks[header];
    let empty_copy = |id: BlockId| {
        let mut copy = BasicBlock::new(id);
        copy.span = head.span;
        copy.unwind = head.unwind;
        copy.handling = head.handling;
        copy.shielded = head.shielded;
        copy.inline.clone_from(&head.inline);
        copy
    };
    // Blocks joined by a `goto` become one block. A call ends its block, so
    // what follows it starts the next.
    let mut copies = vec![empty_copy(bottom)];
    for (position, &block) in rotation.test.iter().enumerate() {
        let source = &body.blocks[block];
        let next = BlockId(bottom.0 + copies.len());
        let copy = copies.last_mut().expect("a copy in progress");
        copy.statements.extend(source.statements.iter().cloned());
        let Some(&continuation) = rotation.test.get(position + 1) else {
            copy.terminator.clone_from(&source.terminator);
            copy.terminator_span = source.terminator_span;
            break;
        };
        if matches!(source.terminator, Some(Terminator::Goto { .. })) {
            continue;
        }
        copy.terminator.clone_from(&source.terminator);
        copy.terminator_span = source.terminator_span;
        if let Some(terminator) = &mut copy.terminator {
            super::rewrite_block_ids_in_terminator_with_map(
                terminator,
                &HashMap::from([(BlockId(continuation), next)]),
            );
        }
        copies.push(empty_copy(next));
    }

    // A temporary that lives entirely inside the test gets its own local in
    // the copy. Sharing one would give it two definitions, and the emitter
    // keeps a local with two definitions in a slot instead of on the stack.
    let defs = super::count_local_defs(body);
    let uses = super::count_local_uses(body);
    let mut test_defs = vec![0usize; body.locals.len()];
    let mut test_uses = vec![0usize; body.locals.len()];
    for statement in copies.iter().flat_map(|copy| &copy.statements) {
        match &statement.kind {
            StatementKind::Assign {
                destination: Place::Local(local),
                ..
            } => test_defs[local.0] += 1,
            // A store through a projection defines no local of its own; the
            // count below can then never match, which keeps its base shared.
            StatementKind::Assign { .. }
            | StatementKind::Drop(_)
            | StatementKind::FreshCell { .. }
            | StatementKind::Intrinsic { .. }
            | StatementKind::VirtualFieldStore { .. }
            | StatementKind::Nop => {}
        }
        super::count_in_statement(statement, &mut test_uses);
    }
    for terminator in copies.iter().filter_map(|copy| copy.terminator.as_ref()) {
        // What a copied call returns is a definition of the test's too. A
        // result only the test reads then gets its own local in the copy, and
        // each of the two calls still hands its result straight to its branch.
        if let Terminator::Call {
            destination: Place::Local(local),
            ..
        }
        | Terminator::VirtualCall {
            destination: Place::Local(local),
            ..
        } = terminator
        {
            test_defs[local.0] += 1;
        }
        super::count_in_terminator(terminator, &mut test_uses);
    }
    let mut renamed: Vec<Option<Local>> = (0..body.locals.len()).map(|i| Some(Local(i))).collect();
    for index in arity + 1..test_defs.len() {
        let decl = &body.locals[index];
        if test_defs[index] > 0
            && defs[index] == test_defs[index]
            && uses[index] == test_uses[index]
            && decl.name.is_none()
            && !decl.is_captured
        {
            renamed[index] = Some(Local(body.locals.len()));
            body.locals.push(decl.clone());
        }
    }
    for copy in &mut copies {
        for statement in &mut copy.statements {
            super::rewrite_locals_in_statement(statement, &renamed);
        }
        if let Some(terminator) = &mut copy.terminator {
            super::rewrite_locals_in_terminator(terminator, &renamed);
        }
    }

    let redirect = HashMap::from([(BlockId(header), bottom)]);
    for &latch in &rotation.latches {
        if let Some(terminator) = &mut body.blocks[latch].terminator {
            super::rewrite_block_ids_in_terminator_with_map(terminator, &redirect);
        }
    }
    body.blocks.extend(copies);
}

// ============================================================================
// Invariant hoisting
// ============================================================================

/// Move what a loop's test recomputes identically on every trip into a new
/// block in front of the loop: `len` of a container, a field read, and the
/// arithmetic over them, when nothing the loop does can change the result.
///
/// Only the test is looked at: the header and the single-entry blocks its
/// `goto`s lead through. Those statements run every time the loop is reached,
/// trips or no trips, so running them once in front of the header instead adds
/// no evaluation the program did not have, and a zero-trip loop still does
/// exactly what its test did. The loop body is left alone: a read there may
/// sit behind a null check the loop performs.
///
/// A heap read is hoisted only from an object `thread_local` vouches for.
/// Another task may write a shared object while this one loops, and a loop
/// that waits for such a write would never see it through a hoisted read.
pub(super) fn hoist_invariants(
    body: &mut MirFunctionBody<'_>,
    arity: usize,
    thread_local: &mut dyn FnMut(&MirFunctionBody<'_>, &Place) -> bool,
) {
    // Each hoist empties one test of its invariants; a test is never refilled.
    for _ in 0..body.blocks.len() {
        let Some(hoist) = find_hoist(body, arity, thread_local) else {
            return;
        };
        hoist_into_preheader(body, &hoist);
    }
}

/// The statements of one loop's test to move in front of it.
struct Hoist {
    header: usize,
    /// The header's predecessors outside the loop.
    entries: Vec<usize>,
    /// `(block, statement)` of each statement to move, in execution order.
    statements: Vec<(usize, usize)>,
}

/// What a loop writes, gathered over every block of it.
struct LoopWrites {
    locals: HashSet<Local>,
    /// Field stores by index; `heap` is set by anything that runs code this
    /// frame cannot see.
    heap: memory::Resources,
    /// Whether a container may change length: any element write other than a
    /// store through an array index, which is bounds-checked and never grows
    /// the array.
    resizes: bool,
}

fn find_hoist(
    body: &MirFunctionBody<'_>,
    arity: usize,
    thread_local: &mut dyn FnMut(&MirFunctionBody<'_>, &Place) -> bool,
) -> Option<Hoist> {
    let entry = body.entry.0;
    let succs: Vec<Vec<usize>> = body
        .blocks
        .iter()
        .map(|block| {
            block
                .terminator
                .iter()
                .flat_map(Terminator::successors)
                .chain(block.unwind)
                .map(|b| b.0)
                .collect()
        })
        .collect();
    let rpo = reducible::reverse_postorder(entry, &succs);
    if !reducible::has_cycle(&succs, &rpo) {
        return None;
    }
    let idom = reducible::immediate_dominators(entry, &succs, &rpo);
    let dominates = |a: usize, mut b: usize| loop {
        if a == b {
            return true;
        }
        if b == entry {
            return false;
        }
        b = idom[b];
    };
    let mut preds = vec![Vec::new(); body.blocks.len()];
    for &u in &rpo {
        for &v in &succs[u] {
            preds[v].push(u);
        }
    }
    let defs = super::count_local_defs(body);
    let model = memory::ClobberModel::for_body(body);

    for &header in &rpo {
        let head = &body.blocks[header];
        // The entry block has no edge to redirect, and a handler is entered
        // by the VM, not through a block placed in front of it.
        if header == entry || head.landing.is_some() {
            continue;
        }
        let (latches, entries): (Vec<usize>, Vec<usize>) = preds[header]
            .iter()
            .partition(|&&pred| dominates(header, pred));
        if latches.is_empty() || entries.is_empty() {
            continue;
        }
        let mut in_loop = vec![false; body.blocks.len()];
        in_loop[header] = true;
        let mut pending = latches;
        while let Some(block) = pending.pop() {
            if !in_loop[block] {
                in_loop[block] = true;
                pending.extend(&preds[block]);
            }
        }

        // The test, as rotation reads it: blocks that run back to back under
        // one handler and one inlined call, entered only from the header.
        let mut test = vec![header];
        while let Some(Terminator::Goto { target }) =
            &body.blocks[*test.last().expect("the header")].terminator
        {
            let next = &body.blocks[target.0];
            if test.contains(&target.0)
                || preds[target.0].len() != 1
                || next.landing.is_some()
                || next.unwind != head.unwind
                || next.handling != head.handling
                || next.shielded != head.shielded
                || next.inline != head.inline
            {
                break;
            }
            test.push(target.0);
        }

        let writes = loop_writes(body, model, &in_loop);
        let mut hoisted: Vec<(usize, usize)> = Vec::new();
        let mut defined: HashSet<Local> = HashSet::new();
        // Set once the test has run something that stays behind and can be
        // observed. An evaluation that can trap may not move in front of it.
        let mut barrier = false;
        for &block in &test {
            for (index, statement) in body.blocks[block].statements.iter().enumerate() {
                let invariant = match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value,
                    } => {
                        let decl = &body.locals[local.0];
                        local.0 > arity
                            && decl.name.is_none()
                            && !decl.is_captured
                            && defs[local.0] == 1
                            && (!barrier || !memory::rvalue_can_trap(body, value))
                            && rvalue_is_invariant(body, value, &writes, &defined, thread_local)
                    }
                    _ => false,
                };
                if invariant {
                    if let StatementKind::Assign {
                        destination: Place::Local(local),
                        ..
                    } = &statement.kind
                    {
                        defined.insert(*local);
                    }
                    hoisted.push((block, index));
                } else {
                    let written = memory::statement_clobbers(body, model, &statement.kind);
                    // A trap that stays behind counts too: it would no longer
                    // be the first error the test raises.
                    let traps = matches!(
                        &statement.kind,
                        StatementKind::Assign { value, .. } if memory::rvalue_can_trap(body, value)
                    );
                    barrier |= traps
                        || written.order
                        || written.heap
                        || written.elements
                        || !written.fields.is_empty()
                        || !written.cells.is_empty();
                }
            }
        }

        // A bare copy is a load wherever it runs. It moves only to feed a
        // hoisted computation that reads it.
        let mut needed: HashSet<Local> = HashSet::new();
        let mut kept = Vec::new();
        for &(block, index) in hoisted.iter().rev() {
            let StatementKind::Assign {
                destination: Place::Local(local),
                value,
            } = &body.blocks[block].statements[index].kind
            else {
                unreachable!("only local assignments are hoisted");
            };
            if matches!(value, Rvalue::Use(_)) && !needed.contains(local) {
                continue;
            }
            memory::walk_rvalue_places(value, &mut |place| {
                needed.extend(place.base_local());
            });
            kept.push((block, index));
        }
        kept.reverse();
        if !kept.is_empty() {
            return Some(Hoist {
                header,
                entries,
                statements: kept,
            });
        }
    }
    None
}

fn loop_writes(
    body: &MirFunctionBody<'_>,
    model: memory::ClobberModel,
    in_loop: &[bool],
) -> LoopWrites {
    let mut writes = LoopWrites {
        locals: HashSet::new(),
        heap: memory::Resources::default(),
        resizes: false,
    };
    for block in body.blocks.iter().filter(|block| in_loop[block.id.0]) {
        if let Some(landing) = block.landing {
            writes.locals.insert(landing.error_local);
            writes.locals.insert(landing.context_local);
        }
        for statement in &block.statements {
            let written = memory::statement_clobbers(body, model, &statement.kind);
            let array_store = matches!(
                &statement.kind,
                StatementKind::Assign {
                    destination: Place::Index {
                        kind: IndexKind::Array,
                        ..
                    },
                    ..
                }
            );
            writes.resizes |= written.elements && !array_store;
            writes.locals.extend(&written.locals);
            writes.heap.extend(&written);
        }
        if let Some(terminator) = &block.terminator {
            let written = memory::terminator_clobbers(model, terminator);
            writes.resizes |= written.elements;
            writes.locals.extend(&written.locals);
            writes.heap.extend(&written);
        }
    }
    writes
}

/// Whether a place reads the same value on every trip. An index read is left
/// out: it is bounds-checked, and its element may be the one the loop sorts.
fn place_is_invariant(
    body: &MirFunctionBody<'_>,
    place: &Place,
    writes: &LoopWrites,
    defined: &HashSet<Local>,
    thread_local: &mut dyn FnMut(&MirFunctionBody<'_>, &Place) -> bool,
) -> bool {
    match place {
        Place::Local(local) => !writes.locals.contains(local) || defined.contains(local),
        Place::Field { base, field } => {
            !writes.heap.heap
                && !writes.heap.fields.contains(field)
                && place_is_invariant(body, base, writes, defined, thread_local)
                && thread_local(body, base)
        }
        Place::Index { .. } | Place::Capture(_) | Place::Deref(_) => false,
    }
}

fn rvalue_is_invariant(
    body: &MirFunctionBody<'_>,
    value: &Rvalue<'_>,
    writes: &LoopWrites,
    defined: &HashSet<Local>,
    thread_local: &mut dyn FnMut(&MirFunctionBody<'_>, &Place) -> bool,
) -> bool {
    let mut operand = |operand: &Operand<'_>| match operand {
        Operand::Constant(_) => true,
        Operand::Copy(place) => place_is_invariant(body, place, writes, defined, thread_local),
        // A move consumes its source.
        Operand::Move(_) => false,
    };
    match value {
        Rvalue::Use(value) | Rvalue::UnaryOp { operand: value, .. } => operand(value),
        Rvalue::BinaryOp { left, right, .. } => operand(left) && operand(right),
        Rvalue::Len(place) => {
            !writes.heap.heap
                && !writes.resizes
                && place_is_invariant(body, place, writes, defined, thread_local)
                && thread_local(body, place)
        }
        _ => false,
    }
}

fn hoist_into_preheader(body: &mut MirFunctionBody<'_>, hoist: &Hoist) {
    let preheader = BlockId(body.blocks.len());
    let head = &body.blocks[hoist.header];
    // The hoisted statements trap, if they trap, where the header's did:
    // under its handler and inside its inlined call.
    let mut block = BasicBlock::new(preheader);
    block.span = head.span;
    block.unwind = head.unwind;
    block.handling = head.handling;
    block.shielded = head.shielded;
    block.inline.clone_from(&head.inline);
    block.terminator = Some(Terminator::Goto {
        target: BlockId(hoist.header),
    });
    for &(source, index) in &hoist.statements {
        let statement = &mut body.blocks[source].statements[index];
        block.statements.push(statement.clone());
        statement.kind = StatementKind::Nop;
    }
    for &(source, _) in &hoist.statements {
        body.blocks[source]
            .statements
            .retain(|statement| !matches!(statement.kind, StatementKind::Nop));
    }
    let redirect = HashMap::from([(BlockId(hoist.header), preheader)]);
    for &entry in &hoist.entries {
        if let Some(terminator) = &mut body.blocks[entry].terminator {
            super::rewrite_block_ids_in_terminator_with_map(terminator, &redirect);
        }
    }
    body.blocks.push(block);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BinOp, Constant, Landing, LocalDecl, Operand, RuntimeTy, Rvalue, Statement};

    fn body(blocks: Vec<BasicBlock<'static>>, locals: usize) -> MirFunctionBody<'static> {
        MirFunctionBody {
            blocks,
            entry: BlockId(0),
            locals: (0..locals)
                .map(|_| LocalDecl {
                    name: None,
                    ty: RuntimeTy::int(),
                    span: None,
                    scope_span: None,
                    is_captured: false,
                })
                .collect(),
        }
    }

    fn block(
        id: usize,
        statements: Vec<Statement<'static>>,
        terminator: Terminator<'static>,
    ) -> BasicBlock<'static> {
        let mut block = BasicBlock::new(BlockId(id));
        block.statements = statements;
        block.terminator = Some(terminator);
        block
    }

    fn set(destination: usize, value: Rvalue<'static>) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(Local(destination)),
                value,
            },
            span: None,
        }
    }

    fn binary(op: BinOp, left: usize, right: Operand<'static>) -> Rvalue<'static> {
        Rvalue::BinaryOp {
            op,
            left: Operand::copy_local(Local(left)),
            right,
        }
    }

    fn goto(target: usize) -> Terminator<'static> {
        Terminator::Goto {
            target: BlockId(target),
        }
    }

    fn call(destination: usize, target: usize) -> Terminator<'static> {
        Terminator::Call {
            has_trace: false,
            argument_layout: None,
            callee: Operand::copy_local(Local(1)),
            args: Vec::new(),
            ntypeargs: 0,
            destination: Place::Local(Local(destination)),
            target: BlockId(target),
            unwind: None,
        }
    }

    fn branch(condition: usize, then_block: usize, else_block: usize) -> Terminator<'static> {
        Terminator::Branch {
            condition: Operand::copy_local(Local(condition)),
            then_block: BlockId(then_block),
            else_block: BlockId(else_block),
        }
    }

    /// `i = 0; while i < n { i = i + 1 }; return i`, with `n` in `_1`, `i` in
    /// `_2`, and the test in `_3`.
    fn counting_loop() -> MirFunctionBody<'static> {
        body(
            vec![
                block(
                    0,
                    vec![set(2, Rvalue::Use(Operand::Constant(Constant::Int(0))))],
                    goto(1),
                ),
                block(
                    1,
                    vec![set(3, binary(BinOp::Lt, 2, Operand::copy_local(Local(1))))],
                    branch(3, 2, 3),
                ),
                block(
                    2,
                    vec![set(
                        2,
                        binary(BinOp::Add, 2, Operand::Constant(Constant::Int(1))),
                    )],
                    goto(1),
                ),
                block(
                    3,
                    vec![set(0, Rvalue::Use(Operand::copy_local(Local(2))))],
                    Terminator::Return,
                ),
            ],
            4,
        )
    }

    /// A body the inliner changed goes through the whole cleanup again, and
    /// a copied callee has been through it once already.
    #[test]
    fn cleaning_up_a_cleaned_up_body_changes_nothing() {
        let mut body = counting_loop();
        super::super::optimize_body(&mut body, 1, crate::OptLevel::Two, None);
        let once = format!("{body:?}");
        assert_eq!(
            body.blocks
                .iter()
                .filter(|block| matches!(block.terminator, Some(Terminator::Branch { .. })))
                .count(),
            2,
            "the loop is rotated: {once}"
        );
        super::super::optimize_body(&mut body, 1, crate::OptLevel::Two, None);
        assert_eq!(once, format!("{body:?}"));
    }

    #[test]
    fn the_test_moves_to_the_bottom_and_still_guards_entry() {
        let mut body = counting_loop();
        rotate_loops(&mut body, 1);

        // Entry still runs the header's test, so a zero-trip loop leaves
        // without running the body.
        assert!(matches!(
            body.blocks[0].terminator,
            Some(Terminator::Goto { target }) if target.0 == 1
        ));
        assert!(matches!(
            body.blocks[1].terminator,
            Some(Terminator::Branch { then_block, else_block, .. })
                if then_block.0 == 2 && else_block.0 == 3
        ));

        // The latch enters a copy of the test, which branches back to the
        // body on a temporary of its own.
        assert_eq!(body.blocks.len(), 5);
        assert!(matches!(
            body.blocks[2].terminator,
            Some(Terminator::Goto { target }) if target.0 == 4
        ));
        let bottom = &body.blocks[4];
        let [
            Statement {
                kind:
                    StatementKind::Assign {
                        destination: Place::Local(test),
                        value: Rvalue::BinaryOp { op: BinOp::Lt, .. },
                    },
                ..
            },
        ] = bottom.statements.as_slice()
        else {
            panic!("the copy holds the test: {:?}", bottom.statements);
        };
        assert_eq!(*test, Local(4));
        assert!(matches!(
            &bottom.terminator,
            Some(Terminator::Branch {
                condition: Operand::Copy(Place::Local(condition)),
                then_block,
                else_block,
            }) if condition == test && then_block.0 == 2 && else_block.0 == 3
        ));
    }

    #[test]
    fn a_rotated_loop_is_left_alone() {
        let mut body = counting_loop();
        rotate_loops(&mut body, 1);
        let (blocks, locals) = (body.blocks.len(), body.locals.len());
        rotate_loops(&mut body, 1);
        assert_eq!((body.blocks.len(), body.locals.len()), (blocks, locals));
    }

    #[test]
    fn a_test_temporary_read_by_the_body_stays_shared() {
        let mut body = counting_loop();
        // The body reads `_3`, so the copy must define that same local.
        body.blocks[2]
            .statements
            .push(set(0, Rvalue::Use(Operand::copy_local(Local(3)))));
        rotate_loops(&mut body, 1);
        assert_eq!(body.locals.len(), 4);
        assert!(matches!(
            &body.blocks[4].statements[0].kind,
            StatementKind::Assign {
                destination: Place::Local(Local(3)),
                ..
            }
        ));
    }

    #[test]
    fn a_header_that_does_not_end_in_the_exit_test_is_not_rotated() {
        // A header whose call returns straight into the body.
        let mut calling = counting_loop();
        calling.blocks[1].terminator = Some(call(3, 2));
        // A header whose branch stays in the loop both ways.
        let mut no_exit = counting_loop();
        no_exit.blocks[1].terminator = Some(branch(3, 2, 2));
        for mut body in [calling, no_exit] {
            rotate_loops(&mut body, 1);
            assert_eq!(body.blocks.len(), 4);
        }
    }

    /// header: `_4 = call` -> bb4: `_3 = i < _4; branch`.
    fn call_tested_loop() -> MirFunctionBody<'static> {
        let mut body = counting_loop();
        body.blocks[1].statements.clear();
        body.blocks[1].terminator = Some(call(4, 4));
        body.blocks.push(block(
            4,
            vec![set(3, binary(BinOp::Lt, 2, Operand::copy_local(Local(4))))],
            branch(3, 2, 3),
        ));
        body.locals.push(body.locals[0].clone());
        body
    }

    fn call_destination(block: &BasicBlock<'static>) -> Local {
        match &block.terminator {
            Some(Terminator::Call {
                destination: Place::Local(local),
                ..
            }) => *local,
            other => panic!("not a call: {other:?}"),
        }
    }

    #[test]
    fn a_test_of_what_a_call_returns_is_copied_call_and_all() {
        let mut body = call_tested_loop();
        rotate_loops(&mut body, 1);

        // Entry still calls and tests through the original two blocks.
        assert!(matches!(
            body.blocks[1].terminator,
            Some(Terminator::Call { target, .. }) if target.0 == 4
        ));
        // The latch enters the copied call, which returns to the copied test.
        assert_eq!(body.blocks.len(), 7);
        assert!(matches!(
            body.blocks[2].terminator,
            Some(Terminator::Goto { target }) if target.0 == 5
        ));
        assert!(matches!(
            body.blocks[5].terminator,
            Some(Terminator::Call { target, .. }) if target.0 == 6
        ));
        assert!(matches!(
            body.blocks[6].terminator,
            Some(Terminator::Branch { then_block, else_block, .. })
                if then_block.0 == 2 && else_block.0 == 3
        ));
        // Only the test reads the result, so the copy has a result of its own
        // and each call still feeds its branch directly.
        let result = call_destination(&body.blocks[5]);
        assert!(result.0 > 4, "a fresh local: {result}");
        assert!(matches!(
            &body.blocks[6].statements[0].kind,
            StatementKind::Assign {
                value: Rvalue::BinaryOp {
                    right: Operand::Copy(Place::Local(read)),
                    ..
                },
                ..
            } if *read == result
        ));

        let blocks = body.blocks.len();
        rotate_loops(&mut body, 1);
        assert_eq!(body.blocks.len(), blocks);
    }

    #[test]
    fn a_call_result_the_body_reads_is_the_same_local_in_the_copy() {
        // As a `for` over an iterator reads the item `next` returned.
        let mut body = call_tested_loop();
        body.blocks[2]
            .statements
            .push(set(0, Rvalue::Use(Operand::copy_local(Local(4)))));
        rotate_loops(&mut body, 1);
        assert_eq!(body.blocks.len(), 7);
        assert_eq!(call_destination(&body.blocks[5]), Local(4));
    }

    #[test]
    fn a_test_split_across_handlers_is_not_copied() {
        // header -> bb4 (the test, protected by the handler bb5) -> body/exit.
        let mut body = counting_loop();
        let test = std::mem::take(&mut body.blocks[1].statements);
        body.blocks[1].terminator = Some(goto(4));
        let mut protected = block(4, test, branch(3, 2, 3));
        protected.unwind = Some(BlockId(5));
        let mut handler = block(5, Vec::new(), goto(3));
        handler.landing = Some(Landing {
            error_local: Local(4),
            context_local: Local(5),
        });
        body.blocks.push(protected);
        body.blocks.push(handler);
        let extra = body.locals[0].clone();
        body.locals.extend([extra.clone(), extra]);
        rotate_loops(&mut body, 1);
        // One copy could not run the test under both handlers.
        assert_eq!(body.blocks.len(), 6);
    }

    fn field(base: usize, field: usize) -> Place {
        Place::Field {
            base: Box::new(Place::Local(Local(base))),
            field,
        }
    }

    /// `i = 0; while i < len(xs) - 1 { i = i + 1 }`, with `xs` in `_1`, `i`
    /// in `_2`, and the test's temporaries in `_3` to `_5`.
    fn len_loop() -> MirFunctionBody<'static> {
        let mut body = counting_loop();
        body.blocks[1].statements = vec![
            set(3, Rvalue::Len(Place::Local(Local(1)))),
            set(
                4,
                binary(BinOp::Sub, 3, Operand::Constant(Constant::Int(1))),
            ),
            set(5, binary(BinOp::Lt, 2, Operand::copy_local(Local(4)))),
        ];
        body.blocks[1].terminator = Some(branch(5, 2, 3));
        let extra = body.locals[0].clone();
        body.locals.extend([extra.clone(), extra.clone(), extra]);
        body
    }

    fn hoist(body: &mut MirFunctionBody<'static>, thread_local: bool) {
        hoist_invariants(body, 1, &mut |_, _| thread_local);
    }

    #[test]
    fn a_length_and_the_arithmetic_over_it_run_once_in_front_of_the_loop() {
        let mut body = len_loop();
        hoist(&mut body, true);

        // Entry reaches the header through the new block; the latch does not.
        assert_eq!(body.blocks.len(), 5);
        assert!(matches!(
            body.blocks[0].terminator,
            Some(Terminator::Goto { target }) if target.0 == 4
        ));
        assert!(matches!(
            body.blocks[2].terminator,
            Some(Terminator::Goto { target }) if target.0 == 1
        ));
        let front = &body.blocks[4];
        assert!(matches!(
            front.terminator,
            Some(Terminator::Goto { target }) if target.0 == 1
        ));
        assert!(matches!(
            front.statements.as_slice(),
            [
                Statement {
                    kind: StatementKind::Assign {
                        value: Rvalue::Len(_),
                        ..
                    },
                    ..
                },
                Statement {
                    kind: StatementKind::Assign {
                        value: Rvalue::BinaryOp { op: BinOp::Sub, .. },
                        ..
                    },
                    ..
                },
            ]
        ));
        // The header keeps the comparison and still decides a zero-trip loop.
        assert_eq!(body.blocks[1].statements.len(), 1);
        assert!(matches!(
            body.blocks[1].terminator,
            Some(Terminator::Branch { .. })
        ));

        // Nothing is left to move.
        hoist(&mut body, true);
        assert_eq!(body.blocks.len(), 5);
    }

    #[test]
    fn an_object_another_task_may_reach_is_read_on_every_trip() {
        let mut body = len_loop();
        hoist(&mut body, false);
        assert_eq!(body.blocks.len(), 4);
        assert_eq!(body.blocks[1].statements.len(), 3);
    }

    #[test]
    fn a_length_is_hoisted_past_an_element_store_but_not_past_a_call() {
        let mut storing = len_loop();
        storing.blocks[2].statements.push(Statement {
            kind: StatementKind::Assign {
                destination: Place::Index {
                    base: Box::new(Place::Local(Local(1))),
                    index: Local(2),
                    kind: IndexKind::Array,
                },
                value: Rvalue::Use(Operand::Constant(Constant::Int(0))),
            },
            span: None,
        });
        hoist(&mut storing, true);
        assert_eq!(storing.blocks.len(), 5);

        // The callee may push to the array.
        let mut calling = len_loop();
        calling.blocks[2].terminator = Some(call(0, 1));
        hoist(&mut calling, true);
        assert_eq!(calling.blocks.len(), 4);
    }

    #[test]
    fn a_field_is_hoisted_unless_the_loop_stores_it() {
        for (stored, hoisted) in [(1, true), (0, false)] {
            let mut body = len_loop();
            // The bound is `_1.0 - 1`, and the loop stores a field of `_1`.
            body.blocks[1].statements[0] = set(3, Rvalue::Use(Operand::Copy(field(1, 0))));
            body.blocks[2].statements.push(Statement {
                kind: StatementKind::Assign {
                    destination: field(1, stored),
                    value: Rvalue::Use(Operand::copy_local(Local(2))),
                },
                span: None,
            });
            hoist(&mut body, true);
            assert_eq!(body.blocks.len() == 5, hoisted, "store to field {stored}");
        }
    }

    #[test]
    fn a_copy_nothing_hoisted_reads_stays_in_the_test() {
        let mut body = counting_loop();
        // `_4 = copy n; _3 = i < _4`: moving the copy would save nothing.
        body.blocks[1].statements = vec![
            set(4, Rvalue::Use(Operand::copy_local(Local(1)))),
            set(3, binary(BinOp::Lt, 2, Operand::copy_local(Local(4)))),
        ];
        body.locals.push(body.locals[0].clone());
        hoist(&mut body, true);
        assert_eq!(body.blocks.len(), 4);
    }

    #[test]
    fn a_trap_does_not_move_in_front_of_what_the_test_does_first() {
        let mut body = len_loop();
        // The test first stores to a field, then subtracts, which may
        // overflow. The store has to be seen before that error is.
        body.blocks[1].statements.insert(
            0,
            Statement {
                kind: StatementKind::Assign {
                    destination: field(1, 0),
                    value: Rvalue::Use(Operand::copy_local(Local(2))),
                },
                span: None,
            },
        );
        hoist(&mut body, true);
        // The length cannot trap and goes; the subtraction stays.
        assert_eq!(body.blocks.len(), 5);
        assert_eq!(body.blocks[4].statements.len(), 1);
        assert_eq!(body.blocks[1].statements.len(), 3);
    }
}
