//! Loop rotation: test a loop at its bottom instead of its top.
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
//! This is tail duplication and nothing else: on every path the program runs
//! the same statements in the same order under the same handler, so no trap,
//! effect, or heap read moves. The original header still runs the test once,
//! on entry, which keeps a zero-trip loop a zero-trip loop.

use std::collections::HashMap;

use crate::{
    BasicBlock, BlockId, Local, MirFunctionBody, Place, StatementKind, Terminator, reducible,
};

/// Most statements a loop test may hold and still be copied.
const MAX_TEST_STATEMENTS: usize = 8;

/// A loop whose test can move to the bottom.
struct Rotation {
    /// The test: the header, then each single-entry block its `goto`s lead
    /// through, the last of which ends in the branch that leaves the loop.
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
            match &last.terminator {
                Some(Terminator::Goto { target }) => {
                    let next = &body.blocks[target.0];
                    // The copy runs as one block under the header's handler,
                    // so every block of the test must already run under it,
                    // and none may be entered from elsewhere.
                    if test.contains(&target.0)
                        || preds[target.0].len() != 1
                        || next.landing.is_some()
                        || next.unwind != head.unwind
                        || next.handling != head.handling
                        || next.shielded != head.shielded
                    {
                        return None;
                    }
                    test.push(target.0);
                    statements += next.statements.len();
                }
                Some(Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                }) => break (then_block.0, else_block.0),
                _ => return None,
            }
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
    let mut copy = BasicBlock::new(bottom);
    copy.span = head.span;
    copy.unwind = head.unwind;
    copy.handling = head.handling;
    copy.shielded = head.shielded;
    for &block in &rotation.test {
        copy.statements
            .extend(body.blocks[block].statements.iter().cloned());
    }
    let last = &body.blocks[*rotation.test.last().expect("the header")];
    copy.terminator.clone_from(&last.terminator);
    copy.terminator_span = last.terminator_span;

    // A temporary that lives entirely inside the test gets its own local in
    // the copy. Sharing one would give it two definitions, and the emitter
    // keeps a local with two definitions in a slot instead of on the stack.
    let defs = super::count_local_defs(body);
    let uses = super::count_local_uses(body);
    let mut test_defs = vec![0usize; body.locals.len()];
    let mut test_uses = vec![0usize; body.locals.len()];
    for statement in &copy.statements {
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
    if let Some(terminator) = &copy.terminator {
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
    for statement in &mut copy.statements {
        super::rewrite_locals_in_statement(statement, &renamed);
    }
    if let Some(terminator) = &mut copy.terminator {
        super::rewrite_locals_in_terminator(terminator, &renamed);
    }

    let redirect = HashMap::from([(BlockId(header), bottom)]);
    for &latch in &rotation.latches {
        if let Some(terminator) = &mut body.blocks[latch].terminator {
            super::rewrite_block_ids_in_terminator_with_map(terminator, &redirect);
        }
    }
    body.blocks.push(copy);
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
        // A header that calls, as a `for` over an iterator does.
        let mut calling = counting_loop();
        calling.blocks[1].terminator = Some(Terminator::Call {
            has_trace: false,
            argument_layout: None,
            callee: Operand::copy_local(Local(1)),
            args: Vec::new(),
            ntypeargs: 0,
            destination: Place::Local(Local(3)),
            target: BlockId(2),
            unwind: None,
        });
        // A header whose branch stays in the loop both ways.
        let mut no_exit = counting_loop();
        no_exit.blocks[1].terminator = Some(branch(3, 2, 2));
        for mut body in [calling, no_exit] {
            rotate_loops(&mut body, 1);
            assert_eq!(body.blocks.len(), 4);
        }
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
}
