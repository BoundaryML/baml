//! Fused instructions: replace runs of adjacent instructions with one
//! instruction that does the same work in one dispatch.
//!
//! The interpreter's cost on a tight loop is the number of instructions it
//! dispatches. A counted loop spends most of them moving operands between
//! locals and the stack around one integer operation, so the runs fused here
//! are the ones built around such an operation:
//!
//! | Run | Fused as |
//! | --- | --- |
//! | `[LoadVar \| LoadConst]* ; AddInt..ModInt ; [StoreVar]` | [`Instruction::IntArith`] |
//! | `[LoadVar \| LoadConst]* ; CmpIntOp ; PopJumpIf{True,False}` | [`Instruction::IntCmpJump`] |
//! | `LoadVar ; LoadField[Local]` | [`Instruction::LoadVarField`] |
//! | `LoadVar ; LoadVar ; LoadArrayElement[Local]` | [`Instruction::LoadVarElement`] |
//! | `LoadVar ; StoreVar` | [`Instruction::MoveVar`] |
//!
//! A fused instruction must behave as its run did, so a run is fused only
//! when no jump lands inside it and no handler, handler-body, shield or
//! inlined-call range starts or ends inside it. A panic raised by the fused
//! instruction is then caught by the handler, and reports the frames and
//! function names the unfused instruction would have.
//!
//! Its line is the one thing that may differ. A run may span source lines
//! and statement boundaries, and the fused instruction reports the line of
//! the run's first instruction: an error it raises can name a neighbouring
//! line of the same function instead of its own. Only the innermost frame is
//! affected. An outer frame's line is its call's, and a call is never fused.
//! The pass runs at the level whose line table no debugger steps by.
//!
//! The pass runs on finished bytecode, after jumps are patched, and rewrites
//! every PC the bytecode holds.

use bex_vm_types::{
    ConstValue, Instruction, IntArithOp, IntDst, IntLhs, IntRhs,
    bytecode::{Bytecode, InstructionMeta, OperandMeta},
};

/// One step of the original code. A `LoadVar2` is two steps, so a run may
/// begin at its second load; every other instruction is one.
#[derive(Clone, Copy)]
struct Step {
    /// The instruction this step does, with a `LoadVar2` split into its
    /// `LoadVar`s.
    instruction: Instruction,
    /// Index of the original instruction the step came from.
    origin: usize,
}

/// An instruction of the rewritten code and the original instructions it
/// stands for.
struct Fused {
    instruction: Instruction,
    meta: InstructionMeta,
    /// The original instruction of its first step.
    first: usize,
    /// The original instruction of its last step.
    last: usize,
}

/// Fuse the runs listed in the module documentation, in place. `local_names`
/// names the function's slots, for the operand metadata of a `LoadVar` split
/// out of a `LoadVar2`.
pub fn fuse(bytecode: &mut Bytecode, local_names: &[String]) {
    let count = bytecode.instructions.len();
    let steps = steps(&bytecode.instructions);
    let barriers = barriers(bytecode);
    // A run may not extend over a step that begins a barred instruction.
    let joins = |position: usize| {
        steps[position].origin == steps[position - 1].origin || !barriers[steps[position].origin]
    };

    let mut fused: Vec<Fused> = Vec::with_capacity(count);
    let mut any = false;
    let mut position = 0;
    while position < steps.len() {
        let matched = match_run(&steps[position..], &bytecode.constants).and_then(|run| {
            // A store that cannot join is dropped from the run; the rest may
            // still fuse.
            let run = if (1..run.len).all(|step| joins(position + step)) {
                run
            } else {
                run.without_store()?
            };
            (1..run.len)
                .all(|step| joins(position + step))
                .then_some(run)
        });
        let Some(run) = matched else {
            push_plain(&mut fused, bytecode, &steps, position, local_names);
            position += 1;
            continue;
        };
        any = true;
        let last = steps[position + run.len - 1].origin;
        fused.push(Fused {
            instruction: run.instruction,
            // A fused field read keeps the field's name; the other fused
            // instructions name only locals, which a listing reads from the
            // function.
            meta: match run.instruction {
                Instruction::LoadVarField { .. } => bytecode.meta[last].clone(),
                _ => InstructionMeta::default(),
            },
            first: steps[position].origin,
            last,
        });
        position += run.len;
    }
    if !any {
        return;
    }
    install(bytecode, &fused);
}

/// The original instructions as steps.
fn steps(instructions: &[Instruction]) -> Vec<Step> {
    let mut steps = Vec::with_capacity(instructions.len() + instructions.len() / 4);
    for (origin, instruction) in instructions.iter().enumerate() {
        if let Instruction::LoadVar2(first, second) = *instruction {
            for slot in [first, second] {
                steps.push(Step {
                    instruction: Instruction::LoadVar(slot),
                    origin,
                });
            }
        } else {
            steps.push(Step {
                instruction: *instruction,
                origin,
            });
        }
    }
    steps
}

/// Per original instruction (and one past the last): whether a run that
/// started earlier must stop before it. That is every PC a jump lands on or
/// a table names as the start or end of a range. The line table is not such
/// a table: see the module docs.
fn barriers(bytecode: &Bytecode) -> Vec<bool> {
    let count = bytecode.instructions.len();
    let mut barriers = vec![false; count + 1];
    let mut bar = |pc: usize| {
        if let Some(barrier) = barriers.get_mut(pc) {
            *barrier = true;
        }
    };
    for (index, instruction) in bytecode.instructions.iter().enumerate() {
        match instruction {
            Instruction::Jump(offset)
            | Instruction::PopJumpIfFalse(offset)
            | Instruction::JumpIfFalse(offset)
            | Instruction::PopJumpIfTrue(offset)
            | Instruction::JumpIfFalseOrPop(offset)
            | Instruction::JumpIfTrueOrPop(offset)
            | Instruction::JumpIfNotNullOrPop(offset) => bar(index.wrapping_add_signed(*offset)),
            Instruction::IntCmpJump { offset, .. } => {
                bar(index.wrapping_add_signed(*offset as isize));
            }
            Instruction::JumpTable(table) => {
                let table = &bytecode.jump_tables[*table];
                bar(index.wrapping_add_signed(table.default));
                for offset in table.offsets.iter().flatten() {
                    bar(index.wrapping_add_signed(*offset));
                }
            }
            _ => {}
        }
    }
    for entry in &bytecode.exception_table {
        bar(entry.start_pc);
        bar(entry.end_pc);
        bar(entry.handler_pc);
    }
    for entry in &bytecode.handler_context_table {
        bar(entry.start_pc);
        bar(entry.end_pc);
        bar(entry.handler_pc);
    }
    for range in &bytecode.shield_table {
        bar(range.start_pc);
        bar(range.end_pc);
    }
    for entry in &bytecode.inline_table {
        bar(entry.pc);
    }
    for block in &bytecode.block_pcs {
        if let Some(start) = block.start {
            bar(start);
        }
    }
    barriers
}

/// A run found at the head of the steps: the instruction that replaces it
/// and how many steps it covers.
struct Run {
    instruction: Instruction,
    len: usize,
}

impl Run {
    /// The same run without its trailing `StoreVar`, when it has one and
    /// what remains still fuses something.
    fn without_store(self) -> Option<Self> {
        match self.instruction {
            Instruction::IntArith {
                op,
                lhs,
                rhs,
                dst: IntDst::Var(_),
            } if rhs != IntRhs::Stack => Some(Self {
                instruction: Instruction::IntArith {
                    op,
                    lhs,
                    rhs,
                    dst: IntDst::Stack,
                },
                len: self.len - 1,
            }),
            _ => None,
        }
    }
}

/// The longest run starting at `steps[0]`.
fn match_run(steps: &[Step], constants: &[ConstValue]) -> Option<Run> {
    let at = |index: usize| steps.get(index).map(|step| step.instruction);
    let var = |index: usize| match at(index)? {
        Instruction::LoadVar(slot) => u16::try_from(slot).ok(),
        _ => None,
    };
    // An operand a fused int instruction can name: a local or a small
    // integer constant.
    let named = |index: usize| match at(index)? {
        Instruction::LoadVar(slot) => u16::try_from(slot).ok().map(IntRhs::Var),
        Instruction::LoadConst(constant) => match constants.get(constant)? {
            ConstValue::Int(value) => i32::try_from(*value).ok().map(IntRhs::Imm),
            _ => None,
        },
        _ => None,
    };
    // The int operation at `index` with what follows it fused in: a store of
    // its result, or the branch on it.
    let operation = |index: usize, lhs: IntLhs, rhs: IntRhs| -> Option<Run> {
        match at(index)? {
            Instruction::CmpIntOp(op) => {
                let (op, offset) = match at(index + 1)? {
                    Instruction::PopJumpIfTrue(offset) => (op, offset),
                    Instruction::PopJumpIfFalse(offset) => (op.negated(), offset),
                    _ => return None,
                };
                // Held relative to the branch for now; `install` retargets it.
                let offset = i32::try_from(offset).ok()?;
                Some(Run {
                    instruction: Instruction::IntCmpJump {
                        op,
                        lhs,
                        rhs,
                        offset,
                    },
                    len: index + 2,
                })
            }
            instruction => {
                let op = IntArithOp::of(&instruction)?;
                let stored = match at(index + 1) {
                    Some(Instruction::StoreVar(slot)) => u16::try_from(slot).ok(),
                    _ => None,
                };
                let (dst, len) = match stored {
                    Some(slot) => (IntDst::Var(slot), index + 2),
                    None => (IntDst::Stack, index + 1),
                };
                // The bare operation is already one instruction.
                (rhs != IntRhs::Stack || dst != IntDst::Stack).then_some(Run {
                    instruction: Instruction::IntArith { op, lhs, rhs, dst },
                    len,
                })
            }
        }
    };

    if let (Some(left), Some(right)) = (var(0), named(1)) {
        if let Some(run) = operation(2, IntLhs::Var(left), right) {
            return Some(run);
        }
    }
    if let Some(right) = named(0) {
        if let Some(run) = operation(1, IntLhs::Stack, right) {
            return Some(run);
        }
    }
    if let Some(run) = operation(0, IntLhs::Stack, IntRhs::Stack) {
        return Some(run);
    }
    let Instruction::LoadVar(first) = at(0)? else {
        return None;
    };
    match (at(1)?, at(2)) {
        (Instruction::LoadField(field), _) => Some(Run {
            instruction: Instruction::LoadVarField {
                var: first,
                field,
                thread_local: false,
            },
            len: 2,
        }),
        (Instruction::LoadFieldLocal(field), _) => Some(Run {
            instruction: Instruction::LoadVarField {
                var: first,
                field,
                thread_local: true,
            },
            len: 2,
        }),
        (Instruction::LoadVar(index), Some(Instruction::LoadArrayElement)) => Some(Run {
            instruction: Instruction::LoadVarElement {
                array: first,
                index,
                thread_local: false,
            },
            len: 3,
        }),
        (Instruction::LoadVar(index), Some(Instruction::LoadArrayElementLocal)) => Some(Run {
            instruction: Instruction::LoadVarElement {
                array: first,
                index,
                thread_local: true,
            },
            len: 3,
        }),
        (Instruction::StoreVar(destination), _) => Some(Run {
            instruction: Instruction::MoveVar {
                source: first,
                destination,
            },
            len: 2,
        }),
        _ => None,
    }
}

/// Carry the step at `position` over unfused. The two loads of a `LoadVar2`
/// that both stay are one `LoadVar2` again.
fn push_plain(
    fused: &mut Vec<Fused>,
    bytecode: &Bytecode,
    steps: &[Step],
    position: usize,
    local_names: &[String],
) {
    let step = steps[position];
    let original = bytecode.instructions[step.origin];
    let Instruction::LoadVar2(first, second) = original else {
        fused.push(Fused {
            instruction: original,
            meta: bytecode.meta[step.origin].clone(),
            first: step.origin,
            last: step.origin,
        });
        return;
    };
    let is_second = position > 0 && steps[position - 1].origin == step.origin;
    if is_second {
        if let Some(previous) = fused.last_mut() {
            if previous.last == step.origin
                && matches!(previous.instruction, Instruction::LoadVar(slot) if slot == first)
            {
                previous.instruction = original;
                return;
            }
        }
    }
    // A lone half is a `LoadVar`. The pair's metadata names its first slot.
    let meta = if is_second {
        InstructionMeta {
            operand: local_names
                .get(second)
                .filter(|name| !name.is_empty())
                .map(|name| OperandMeta::Var(name.clone())),
        }
    } else {
        bytecode.meta[step.origin].clone()
    };
    fused.push(Fused {
        instruction: step.instruction,
        meta,
        first: step.origin,
        last: step.origin,
    });
}

/// Replace the bytecode's instructions with `fused` and move every PC it
/// holds to the instruction that now stands for it.
#[allow(clippy::cast_possible_wrap)]
fn install(bytecode: &mut Bytecode, fused: &[Fused]) {
    let count = bytecode.instructions.len();
    // Where an original PC is now: the first instruction holding any of its
    // steps (what a jump to it, or a range starting at it, means) and the
    // last (where its own operation runs). One past the end maps to one past
    // the end, for exclusive range ends.
    let mut first_at = vec![fused.len(); count + 1];
    let mut last_at = vec![fused.len(); count + 1];
    for (index, instruction) in fused.iter().enumerate().rev() {
        first_at[instruction.first..=instruction.last].fill(index);
    }
    for (index, instruction) in fused.iter().enumerate() {
        last_at[instruction.first..=instruction.last].fill(index);
    }
    let retarget = |origin: usize, offset: isize, index: usize| -> isize {
        first_at[origin.wrapping_add_signed(offset)] as isize - index as isize
    };

    let mut instructions = Vec::with_capacity(fused.len());
    let mut meta = Vec::with_capacity(fused.len());
    for (index, entry) in fused.iter().enumerate() {
        // A jump is the last step of whatever it was fused into.
        let origin = entry.last;
        let instruction = match entry.instruction {
            Instruction::Jump(offset) => Instruction::Jump(retarget(origin, offset, index)),
            Instruction::PopJumpIfFalse(offset) => {
                Instruction::PopJumpIfFalse(retarget(origin, offset, index))
            }
            Instruction::JumpIfFalse(offset) => {
                Instruction::JumpIfFalse(retarget(origin, offset, index))
            }
            Instruction::PopJumpIfTrue(offset) => {
                Instruction::PopJumpIfTrue(retarget(origin, offset, index))
            }
            Instruction::JumpIfFalseOrPop(offset) => {
                Instruction::JumpIfFalseOrPop(retarget(origin, offset, index))
            }
            Instruction::JumpIfTrueOrPop(offset) => {
                Instruction::JumpIfTrueOrPop(retarget(origin, offset, index))
            }
            Instruction::JumpIfNotNullOrPop(offset) => {
                Instruction::JumpIfNotNullOrPop(retarget(origin, offset, index))
            }
            Instruction::IntCmpJump {
                op,
                lhs,
                rhs,
                offset,
            } => Instruction::IntCmpJump {
                op,
                lhs,
                rhs,
                offset: i32::try_from(retarget(origin, offset as isize, index))
                    .unwrap_or_else(|_| unreachable!("a jump shortened by fusing still fits")),
            },
            Instruction::JumpTable(table) => {
                let table_data = &mut bytecode.jump_tables[table];
                table_data.default = retarget(origin, table_data.default, index);
                for offset in table_data.offsets.iter_mut().flatten() {
                    *offset = retarget(origin, *offset, index);
                }
                Instruction::JumpTable(table)
            }
            other => other,
        };
        instructions.push(instruction);
        meta.push(entry.meta.clone());
    }

    // Run-length tables: each instruction keeps the entry that covered its
    // first step, and an entry is written where that changes.
    let origins: Vec<usize> = fused.iter().map(|entry| entry.first).collect();
    bytecode.line_table = rerun(
        &bytecode.line_table,
        &origins,
        |entry| entry.pc,
        |entry, pc| {
            entry.pc = pc;
        },
    );
    bytecode.inline_table = rerun(
        &bytecode.inline_table,
        &origins,
        |entry| entry.pc,
        |entry, pc| entry.pc = pc,
    );

    for entry in &mut bytecode.exception_table {
        entry.start_pc = first_at[entry.start_pc];
        entry.end_pc = first_at[entry.end_pc];
        entry.handler_pc = first_at[entry.handler_pc];
    }
    for entry in &mut bytecode.handler_context_table {
        entry.start_pc = first_at[entry.start_pc];
        entry.end_pc = first_at[entry.end_pc];
        entry.handler_pc = first_at[entry.handler_pc];
    }
    for range in &mut bytecode.shield_table {
        range.start_pc = first_at[range.start_pc];
        range.end_pc = first_at[range.end_pc];
    }
    bytecode.call_layouts = std::mem::take(&mut bytecode.call_layouts)
        .into_iter()
        .map(|(pc, layout)| (last_at[pc], layout))
        .collect();
    for block in &mut bytecode.block_pcs {
        block.start = block.start.map(|pc| first_at[pc]);
        block.terminator = block.terminator.map(|pc| last_at[pc]);
        for statement in &mut block.statements {
            *statement = statement.map(|pc| last_at[pc]);
        }
    }
    bytecode.instructions = instructions;
    bytecode.meta = meta;
}

/// Rebuild a table whose entries each apply from their PC to the next
/// entry's, for instructions whose first steps came from `origins`.
fn rerun<T: Clone>(
    table: &[T],
    origins: &[usize],
    pc_of: impl Fn(&T) -> usize,
    set_pc: impl Fn(&mut T, usize),
) -> Vec<T> {
    let mut rebuilt = Vec::with_capacity(table.len());
    let mut current = None;
    for (index, origin) in origins.iter().enumerate() {
        let covering = table
            .partition_point(|entry| pc_of(entry) <= *origin)
            .checked_sub(1);
        if covering != current {
            current = covering;
            if let Some(covering) = covering {
                let mut entry = table[covering].clone();
                set_pc(&mut entry, index);
                rebuilt.push(entry);
            }
        }
    }
    rebuilt
}

#[cfg(test)]
mod tests {
    use bex_vm_types::{
        CmpOp,
        bytecode::{ExceptionTableEntry, LineTableEntry},
    };

    use super::*;

    fn bytecode(instructions: Vec<Instruction>) -> Bytecode {
        let mut bytecode = Bytecode::new();
        bytecode.meta = vec![InstructionMeta::default(); instructions.len()];
        bytecode.instructions = instructions;
        bytecode.constants = vec![ConstValue::Int(1), ConstValue::Int(i64::from(i32::MAX) + 1)];
        bytecode
    }

    fn line(pc: usize, line: usize, sequence_point: bool) -> LineTableEntry {
        LineTableEntry {
            pc,
            span: baml_base::Span::fake(),
            line,
            sequence_point,
            discriminator: 0,
        }
    }

    /// `while (i < n) { total = total + i; i = i + 1 }` as the emitter lays
    /// it out, rotated: the body, then the test that closes the loop.
    fn counted_loop() -> Vec<Instruction> {
        vec![
            Instruction::LoadVar2(1, 2),
            Instruction::CmpIntOp(CmpOp::Lt),
            Instruction::PopJumpIfFalse(11),
            Instruction::LoadVar2(3, 1),
            Instruction::AddInt,
            Instruction::StoreVar(3),
            Instruction::LoadVar(1),
            Instruction::LoadConst(0),
            Instruction::AddInt,
            Instruction::StoreVar(1),
            Instruction::LoadVar2(1, 2),
            Instruction::CmpIntOp(CmpOp::Lt),
            Instruction::PopJumpIfTrue(-9),
            Instruction::LoadVar(3),
            Instruction::Return,
        ]
    }

    #[test]
    fn a_counted_loop_is_three_instructions_an_iteration() {
        let mut code = bytecode(counted_loop());
        fuse(&mut code, &[]);
        assert_eq!(
            code.instructions,
            vec![
                // The entry test, negated: leave when `i >= n`.
                Instruction::IntCmpJump {
                    op: CmpOp::GtEq,
                    lhs: IntLhs::Var(1),
                    rhs: IntRhs::Var(2),
                    offset: 4,
                },
                Instruction::IntArith {
                    op: IntArithOp::Add,
                    lhs: IntLhs::Var(3),
                    rhs: IntRhs::Var(1),
                    dst: IntDst::Var(3),
                },
                Instruction::IntArith {
                    op: IntArithOp::Add,
                    lhs: IntLhs::Var(1),
                    rhs: IntRhs::Imm(1),
                    dst: IntDst::Var(1),
                },
                Instruction::IntCmpJump {
                    op: CmpOp::Lt,
                    lhs: IntLhs::Var(1),
                    rhs: IntRhs::Var(2),
                    offset: -2,
                },
                Instruction::LoadVar(3),
                Instruction::Return,
            ]
        );
        assert_eq!(code.meta.len(), code.instructions.len());
    }

    #[test]
    fn a_run_stops_at_a_jump_target_and_a_range_edge_but_not_at_a_new_line() {
        // The same two instructions three times; each time something
        // different separates the load from the store.
        let pair = [Instruction::LoadVar(1), Instruction::StoreVar(2)];
        let mut instructions = vec![Instruction::Jump(2)];
        for _ in 0..4 {
            instructions.extend(pair);
        }
        instructions.push(Instruction::Return);
        let mut code = bytecode(instructions);
        // Pair 1 (pcs 1, 2): the jump lands on its store.
        // Pair 2 (pcs 3, 4): a handler range ends between them.
        code.exception_table.push(ExceptionTableEntry {
            start_pc: 0,
            end_pc: 4,
            handler_pc: 9,
            error_slot: 3,
            context_slot: 4,
        });
        // Pair 3 (pcs 5, 6): the store is on the next line, at a statement
        // boundary. It fuses, and reports its load's line.
        // Pair 4 (pcs 7, 8): the store only narrows the span.
        code.line_table = vec![line(0, 1, true), line(6, 2, true), line(8, 2, false)];
        fuse(&mut code, &[]);
        let moved = Instruction::MoveVar {
            source: 1,
            destination: 2,
        };
        assert_eq!(
            code.instructions,
            vec![
                Instruction::Jump(2),
                pair[0],
                pair[1],
                pair[0],
                pair[1],
                moved.clone(),
                moved,
                Instruction::Return,
            ]
        );
        // Pair 3, at pc 5, is on line 1 with its load. Pair 4, at pc 6,
        // starts on line 2, and the return after it keeps its own entry.
        assert_eq!(
            code.line_table
                .iter()
                .map(|entry| (entry.pc, entry.line))
                .collect::<Vec<_>>(),
            [(0, 1), (6, 2), (7, 2)]
        );
        assert_eq!(code.exception_table[0].end_pc, 4);
        assert_eq!(code.exception_table[0].handler_pc, 7);
    }

    #[test]
    fn a_run_may_start_at_the_second_load_of_a_pair() {
        let mut code = bytecode(vec![
            Instruction::LoadVar2(4, 5),
            Instruction::LoadConst(0),
            Instruction::SubInt,
            Instruction::LoadArrayElement,
            Instruction::Return,
        ]);
        let names: Vec<String> = ["", "", "", "", "items", "at"].map(String::from).to_vec();
        code.meta[0].operand = Some(OperandMeta::Var("items".into()));
        fuse(&mut code, &names);
        assert_eq!(
            code.instructions,
            vec![
                Instruction::LoadVar(4),
                Instruction::IntArith {
                    op: IntArithOp::Sub,
                    lhs: IntLhs::Var(5),
                    rhs: IntRhs::Imm(1),
                    dst: IntDst::Stack,
                },
                Instruction::LoadArrayElement,
                Instruction::Return,
            ]
        );
        assert!(matches!(&code.meta[0].operand, Some(OperandMeta::Var(name)) if name == "items"));
    }

    #[test]
    fn a_constant_too_wide_for_an_immediate_stays_a_load() {
        let mut code = bytecode(vec![
            Instruction::LoadVar(1),
            Instruction::LoadConst(1),
            Instruction::MulInt,
            Instruction::Return,
        ]);
        fuse(&mut code, &[]);
        // Only the multiply's right operand could be named, and it cannot.
        assert_eq!(code.instructions.len(), 4);
    }
}
