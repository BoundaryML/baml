//! What every instruction does to the operand stack, and the load-time check
//! that proves a function's operand stack stays within a bound.
//!
//! [`Bytecode::stack_effect`] is the one model of each instruction's stack
//! effect. [`Bytecode::verify_stack`] walks a function's control flow with
//! it, the way `CPython`'s `flowgraph.c` and the .NET importer do, and rejects
//! the function unless every instruction is reached at one operand depth,
//! nothing pops a value that isn't there, and the depth stays under
//! [`MAX_OPERAND_DEPTH`]. The largest depth found becomes
//! [`CompactCode::max_stack`]: a frame reserves that much on entry, so the
//! dispatch loop pushes without a capacity check. Debug builds also keep the
//! depth of every instruction ([`CompactCode::operand_depths`]) and assert
//! the live stack matches it before each instruction runs, which ties this
//! table to the VM's handlers.

use std::fmt;

use crate::{
    GlobalIndex,
    bytecode::{Bytecode, CompactCode, Instruction},
};

/// The most operand values a function's frame may hold above its locals.
/// Far beyond what any real function needs; deeper code is rejected at load.
pub const MAX_OPERAND_DEPTH: usize = 1 << 20;

/// The [`CompactCode::max_stack`] of code lowered without
/// [`Bytecode::lower_verified`]. Entering a frame reserves locals plus this
/// many slots, which no stack budget can hold, so unverified code fails with
/// a stack overflow on entry instead of running with unchecked pushes.
pub const UNVERIFIED_MAX_STACK: usize = usize::MAX / 4;

/// Debug builds' [`CompactCode::operand_depths`] entry for a byte offset
/// where no reachable instruction starts.
pub const NO_DEPTH: u32 = u32::MAX;

/// The values an instruction pops from the operand stack, then pushes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackEffect {
    pub pops: usize,
    pub pushes: usize,
}

impl StackEffect {
    const fn new(pops: usize, pushes: usize) -> Self {
        Self { pops, pushes }
    }
}

/// What [`Bytecode::verify_stack`] needs from outside the bytecode.
pub struct StackContext<'a> {
    /// Operand values already on the stack when the frame starts: zero for
    /// a function the VM calls, the pre-pushed arguments for a trampoline.
    pub entry_depth: usize,
    /// The arity of the function a global names. A call without a recorded
    /// argument layout (`Bytecode::call_layouts`) passes exactly that many
    /// value arguments; `None` rejects such a call.
    pub global_arity: &'a dyn Fn(GlobalIndex) -> Option<usize>,
    /// The value arguments of a `CallIndirect` without a recorded layout.
    /// Only the VM's host-callable trampoline has one; `None` rejects it.
    pub indirect_args: Option<usize>,
}

impl<'a> StackContext<'a> {
    /// A function the VM calls, whose calls' callees `global_arity` knows.
    pub fn new(global_arity: &'a dyn Fn(GlobalIndex) -> Option<usize>) -> Self {
        Self {
            entry_depth: 0,
            global_arity,
            indirect_args: None,
        }
    }
}

/// Why [`Bytecode::verify_stack`] rejected a function. `index` is the
/// instruction's position in `Bytecode::instructions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackError {
    /// A call whose argument count neither a layout nor its callee states.
    UnknownArity { index: usize },
    /// An operand that names a table entry that does not exist.
    BadOperand { index: usize },
    /// A jump, jump-table entry or handler outside the function.
    BadTarget { index: usize, target: isize },
    /// Control runs past the last instruction.
    FallsOffEnd { index: usize },
    /// The instruction pops more values than the stack holds.
    Underflow {
        index: usize,
        depth: usize,
        pops: usize,
    },
    /// Two paths reach the instruction with different stack depths.
    Mismatch {
        index: usize,
        depth: usize,
        other: usize,
    },
    /// The stack would grow past [`MAX_OPERAND_DEPTH`].
    TooDeep { index: usize, depth: usize },
}

impl fmt::Display for StackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::UnknownArity { index } => {
                write!(
                    f,
                    "instruction {index}: a call whose argument count is unknown"
                )
            }
            Self::BadOperand { index } => {
                write!(f, "instruction {index}: an operand names no table entry")
            }
            Self::BadTarget { index, target } => {
                write!(
                    f,
                    "instruction {index}: control target {target} is outside the function"
                )
            }
            Self::FallsOffEnd { index } => {
                write!(
                    f,
                    "instruction {index}: control runs past the last instruction"
                )
            }
            Self::Underflow { index, depth, pops } => write!(
                f,
                "instruction {index}: pops {pops} operand(s) from a stack of {depth}"
            ),
            Self::Mismatch {
                index,
                depth,
                other,
            } => write!(
                f,
                "instruction {index}: reached with operand depths {depth} and {other}"
            ),
            Self::TooDeep { index, depth } => write!(
                f,
                "instruction {index}: operand depth {depth} exceeds {MAX_OPERAND_DEPTH}"
            ),
        }
    }
}

impl std::error::Error for StackError {}

/// Where control goes after an instruction. Offsets are from the
/// instruction itself.
#[derive(Clone, Copy)]
enum Flow {
    /// The next instruction.
    Next,
    /// Only the target.
    Jump(isize),
    /// The next instruction or the target, after the same effect.
    Branch(isize),
    /// The target with the tested value kept, or the next instruction with
    /// it popped (see `taken` in [`Bytecode::stack_effect`]).
    BranchOrPop(isize),
    /// A jump table's targets and its default.
    Table(usize),
    /// Nowhere in this frame.
    Stop,
}

/// What [`Bytecode::verify_stack`] proved about a function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackLayout {
    /// The largest operand depth any instruction reaches.
    pub max: usize,
    /// The operand depth before each instruction runs, `None` where no path
    /// reaches it.
    pub depths: Vec<Option<u32>>,
}

/// [`Bytecode::verify_stack`]'s worklist state.
struct Walk {
    depths: Vec<Option<u32>>,
    pending: Vec<usize>,
    max: usize,
}

impl Walk {
    /// Control reaches instruction `target` from `from` at `depth`: record
    /// that depth the first time, and insist on it after.
    fn reach(&mut self, from: usize, target: isize, depth: usize) -> Result<(), StackError> {
        let index = usize::try_from(target)
            .ok()
            .filter(|&index| index < self.depths.len())
            .ok_or(StackError::BadTarget {
                index: from,
                target,
            })?;
        if depth > MAX_OPERAND_DEPTH {
            return Err(StackError::TooDeep { index, depth });
        }
        #[allow(clippy::cast_possible_truncation)]
        let encoded = depth as u32;
        match self.depths[index] {
            None => {
                self.depths[index] = Some(encoded);
                self.max = self.max.max(depth);
                self.pending.push(index);
                Ok(())
            }
            Some(seen) if seen == encoded => Ok(()),
            Some(seen) => Err(StackError::Mismatch {
                index,
                depth: seen as usize,
                other: depth,
            }),
        }
    }
}

impl Bytecode {
    /// The values instruction `index` pops from the operand stack, then
    /// pushes, counted when its frame continues (a call's result, a
    /// suspension's resumed value). `taken` says whether a conditional
    /// branch jumps; only the `*OrPop` branches differ, keeping their tested
    /// value when they jump.
    ///
    /// This is the VM's one model of stack effects: the load-time check and
    /// the debug-build depth assertion in the dispatch loop both rest on it.
    /// Variable counts come from the operands, the call's recorded layout,
    /// the class-init plan, or `ctx` for a callee's arity.
    #[allow(clippy::too_many_lines)]
    pub fn stack_effect(
        &self,
        index: usize,
        taken: bool,
        ctx: &StackContext<'_>,
    ) -> Result<StackEffect, StackError> {
        use Instruction as I;
        let layout = || self.call_layouts.get(&index).map(|layout| layout.0.len());
        let callee_args = |callee: GlobalIndex| {
            layout()
                .or_else(|| (ctx.global_arity)(callee))
                .ok_or(StackError::UnknownArity { index })
        };
        let effect = |pops, pushes| Ok(StackEffect::new(pops, pushes));
        match self.instructions[index] {
            I::LoadConst(_)
            | I::LoadVar(_)
            | I::LoadGlobal(_)
            | I::LoadCurrentPackage(_)
            | I::LoadType(_)
            | I::LoadDeref(_)
            | I::LoadCapture(_)
            | I::CaptureRef(_)
            | I::Copy(_)
            | I::LoadVarField(..)
            | I::LoadVarFieldLocal(..)
            | I::ArrayIterNext { .. }
            | I::ArrayIterNextLocal { .. }
            | I::LoadVariant { .. }
            | I::BeginTraceHook(_) => effect(0, 1),
            I::LoadVar2(..) => effect(0, 2),
            I::StoreVarLoadVar(_)
            | I::AddLocalSmall(..)
            | I::TraceHookHidden
            | I::TraceHookTiming
            | I::TraceHookSpan
            | I::TraceHookRich
            | I::TraceHookEmptySpan
            | I::Jump(_)
            | I::JumpIfFalse(_)
            | I::JumpIfLocalsCmp { .. }
            | I::JumpIfLocalConstCmp { .. }
            | I::Unreachable => effect(0, 0),
            I::JumpIfFalseOrPop(_) | I::JumpIfTrueOrPop(_) | I::JumpIfNotNullOrPop(_) => {
                effect(usize::from(!taken), 0)
            }
            I::StoreVar(_)
            | I::StoreGlobal(_)
            | I::StoreDeref(_)
            | I::StoreCapture(_)
            | I::InitSpread(_)
            | I::BindType(_)
            | I::SetCallTrace
            | I::EndTraceHook
            | I::PopJumpIfFalse(_)
            | I::PopJumpIfTrue(_)
            | I::JumpTable(_)
            | I::Return
            | I::Throw => effect(1, 0),
            I::StoreVar2(..) | I::StoreField(_) | I::StoreFieldLocal(_) | I::Rethrow => {
                effect(2, 0)
            }
            I::VirtualStoreField(_)
            | I::StoreArrayElement
            | I::StoreArrayElementLocal
            | I::StoreMapElement => effect(3, 0),
            I::Pop(n) => effect(n, 0),
            I::LoadField(_)
            | I::LoadFieldLocal(_)
            | I::LoadTupleField(_)
            | I::UnaryOp(_)
            | I::TypeTag
            | I::Discriminant
            | I::IsType(_)
            | I::NarrowBind { .. }
            | I::DenseTag(_)
            | I::ContainerLen
            | I::ContainerLenLocal
            | I::IsNull
            | I::IsNotNull
            | I::MakeCell
            | I::MakeBoundMethod(_)
            | I::AllocVariant(_)
            | I::Await
            | I::AwaitAny
            | I::Spawn => effect(1, 1),
            I::BinOp(_)
            | I::CmpOp(_)
            | I::AddInt
            | I::SubInt
            | I::MulInt
            | I::DivInt
            | I::ModInt
            | I::AddFloat
            | I::SubFloat
            | I::MulFloat
            | I::DivFloat
            | I::AddBigint
            | I::SubBigint
            | I::MulBigint
            | I::DivBigint
            | I::ModBigint
            | I::BitAndBigint
            | I::BitOrBigint
            | I::BitXorBigint
            | I::ShlBigint
            | I::ShrBigint
            | I::CmpIntOp(_)
            | I::CmpFloatOp(_)
            | I::CmpBigintOp(_)
            | I::LoadArrayElement
            | I::LoadArrayElementLocal
            | I::LoadMapElement
            | I::VirtualLoadField(_)
            | I::InitField(_)
            | I::SatisfiesBound
            | I::CallEquals(_)
            // The engine resumes a log or event with a null pushed.
            | I::SendEvent => effect(2, 1),
            // The elements, then the declared element type on top.
            I::AllocArray(n) | I::AllocTuple(n) => effect(n + 1, 1),
            // The keys and values, then the key and value types on top.
            I::AllocMap(n) => effect(2 * n + 2, 1),
            I::ConcatStrings(n) => effect(n, 1),
            I::AllocInstance { ntypeargs, .. } | I::MakeGenericFunction { ntypeargs, .. } => {
                effect(usize::from(ntypeargs), 1)
            }
            I::InitInstance(plan) => {
                let plan = self
                    .class_init_plans
                    .get(plan)
                    .ok_or(StackError::BadOperand { index })?;
                effect(plan.fields.len() + usize::from(plan.ntypeargs), 1)
            }
            I::MakeClosure {
                capture_count,
                ntypeargs,
                ..
            } => effect(capture_count + ntypeargs, 1),
            // The type arguments, then the callable or receiver.
            I::MakeGenericFunctionFromValue { ntypeargs }
            | I::MakeGenericBoundMethod { ntypeargs, .. } => effect(usize::from(ntypeargs) + 1, 1),
            // Receiver or `Self` type, type arguments, interface, method name.
            I::MakeVirtualBoundMethod { ntypeargs } | I::MakeVirtualFunction { ntypeargs } => {
                effect(usize::from(ntypeargs) + 3, 1)
            }
            I::Call { callee, ntypeargs }
            | I::CallLocal { callee, ntypeargs }
            | I::CallHooked { callee, ntypeargs } => {
                effect(usize::from(ntypeargs) + callee_args(callee)?, 1)
            }
            I::SysOp(callee) => effect(
                (ctx.global_arity)(callee).ok_or(StackError::UnknownArity { index })?,
                1,
            ),
            I::CallIndirect => {
                let args = layout()
                    .or(ctx.indirect_args)
                    .ok_or(StackError::UnknownArity { index })?;
                effect(args + 1, 1)
            }
            // The arguments, their type arguments, then interface and name.
            I::VirtualCall {
                nargs, ntypeargs, ..
            } => effect(usize::from(nargs) + usize::from(ntypeargs) + 2, 1),
        }
    }

    /// Where control goes after instruction `index`.
    fn flow(&self, index: usize) -> Flow {
        use Instruction as I;
        match self.instructions[index] {
            I::Jump(offset) => Flow::Jump(offset),
            I::PopJumpIfFalse(offset)
            | I::PopJumpIfTrue(offset)
            | I::JumpIfFalse(offset)
            | I::JumpIfLocalsCmp { offset, .. }
            | I::JumpIfLocalConstCmp { offset, .. } => Flow::Branch(offset),
            I::JumpIfFalseOrPop(offset)
            | I::JumpIfTrueOrPop(offset)
            | I::JumpIfNotNullOrPop(offset) => Flow::BranchOrPop(offset),
            I::JumpTable(table) => Flow::Table(table),
            I::Return | I::Throw | I::Rethrow | I::Unreachable => Flow::Stop,
            _ => Flow::Next,
        }
    }

    /// Prove the function's operand stack consistent and bounded, or say why
    /// not. Starting from the entry (at `ctx.entry_depth`), every exception
    /// handler (unwinding truncates to the locals, depth 0) and the hook
    /// decision boundary (unwinding pushes one null before `EndTraceHook`),
    /// a worklist follows every edge with [`Self::stack_effect`]. Each
    /// reachable instruction must be reached at one depth, pop no more than
    /// the stack holds, and stay under [`MAX_OPERAND_DEPTH`].
    pub fn verify_stack(&self, ctx: &StackContext<'_>) -> Result<StackLayout, StackError> {
        let count = self.instructions.len();
        let mut walk = Walk {
            depths: vec![None; count],
            pending: Vec::new(),
            max: 0,
        };
        let at = |index: usize| isize::try_from(index).unwrap_or(isize::MAX);

        if count > 0 {
            walk.reach(0, 0, ctx.entry_depth)?;
        }
        for entry in &self.exception_table {
            walk.reach(entry.start_pc, at(entry.handler_pc), 0)?;
        }
        if let Some(finish) = self
            .instructions
            .iter()
            .position(|instruction| matches!(instruction, Instruction::EndTraceHook))
        {
            walk.reach(finish, at(finish), 1)?;
        }

        while let Some(index) = walk.pending.pop() {
            let depth = walk.depths[index].expect("queued instructions have a depth") as usize;
            let after = |taken: bool| -> Result<usize, StackError> {
                let StackEffect { pops, pushes } = self.stack_effect(index, taken, ctx)?;
                let rest =
                    depth
                        .checked_sub(pops)
                        .ok_or(StackError::Underflow { index, depth, pops })?;
                Ok(rest + pushes)
            };
            let next = || {
                if index + 1 == count {
                    Err(StackError::FallsOffEnd { index })
                } else {
                    Ok(at(index) + 1)
                }
            };
            let target = |offset: isize| at(index).saturating_add(offset);
            match self.flow(index) {
                Flow::Next => walk.reach(index, next()?, after(false)?)?,
                Flow::Jump(offset) => walk.reach(index, target(offset), after(true)?)?,
                Flow::Branch(offset) | Flow::BranchOrPop(offset) => {
                    walk.reach(index, next()?, after(false)?)?;
                    walk.reach(index, target(offset), after(true)?)?;
                }
                Flow::Table(table) => {
                    let after = after(false)?;
                    let table = self
                        .jump_tables
                        .get(table)
                        .ok_or(StackError::BadOperand { index })?;
                    for &offset in table.offsets.iter().flatten().chain([&table.default]) {
                        walk.reach(index, target(offset), after)?;
                    }
                }
                Flow::Stop => {
                    after(false)?;
                }
            }
        }
        Ok(StackLayout {
            max: walk.max,
            depths: walk.depths,
        })
    }

    /// [`Self::lower_to_compact`] (or, with `keep_offsets`,
    /// [`Self::lower_to_compact_with_offsets`]) for code that passes
    /// [`Self::verify_stack`], with its proven bound as `max_stack` and, with
    /// the function's `locals`, its `frame_slots`. The only way to get code a
    /// frame can run.
    pub fn lower_verified(
        &self,
        ctx: &StackContext<'_>,
        locals: usize,
        keep_offsets: bool,
    ) -> Result<CompactCode, StackError> {
        let layout = self.verify_stack(ctx)?;
        let (mut compact, offsets) = self.lower_with_layout(keep_offsets);
        compact.max_stack = layout.max;
        compact.frame_slots = locals + layout.max;
        #[cfg(debug_assertions)]
        {
            let mut depths = vec![NO_DEPTH; compact.code.len()];
            for (index, depth) in layout.depths.iter().enumerate() {
                if let Some(depth) = depth {
                    depths[offsets[index]] = *depth;
                }
            }
            compact.operand_depths = depths;
        }
        #[cfg(not(debug_assertions))]
        let _ = offsets;
        Ok(compact)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        bytecode::{ClassInitPlan, CmpOp, ExceptionTableEntry, InstructionMeta, JumpTableData},
        types::ConstValue,
    };

    fn bytecode(instructions: Vec<Instruction>) -> Bytecode {
        let meta = vec![InstructionMeta { operand: None }; instructions.len()];
        Bytecode {
            instructions,
            constants: vec![ConstValue::Null, ConstValue::Int(1)],
            call_layouts: BTreeMap::new(),
            meta,
            ..Bytecode::default()
        }
    }

    fn no_arity(_: GlobalIndex) -> Option<usize> {
        None
    }

    fn verify(bc: &Bytecode) -> Result<StackLayout, StackError> {
        bc.verify_stack(&StackContext::new(&no_arity))
    }

    #[test]
    fn straight_line_code_is_as_deep_as_its_deepest_expression() {
        // `a + b * c`, stored, then a pair loaded and dropped.
        let bc = bytecode(vec![
            Instruction::LoadVar(1),
            Instruction::LoadVar(2),
            Instruction::LoadVar(3),
            Instruction::MulInt,
            Instruction::AddInt,
            Instruction::StoreVar(4),
            Instruction::LoadVar2(1, 2),
            Instruction::Pop(2),
            Instruction::LoadConst(0),
            Instruction::Return,
        ]);
        let layout = verify(&bc).unwrap();
        assert_eq!(layout.max, 3);
        assert_eq!(
            layout.depths,
            [0, 1, 2, 3, 2, 1, 0, 2, 0, 1].map(Some).to_vec()
        );
        let compact = bc
            .lower_verified(&StackContext::new(&no_arity), 0, false)
            .unwrap();
        assert_eq!(compact.max_stack, 3);
        // Plain lowering never yields a runnable bound.
        assert_eq!(bc.lower_to_compact().max_stack, UNVERIFIED_MAX_STACK);
    }

    #[test]
    fn loops_and_branches_settle_on_one_depth() {
        // while (i < n) { if (a or b) { s = s + 1 } i += 1 }; return s
        let bc = bytecode(vec![
            /* 0 */
            Instruction::JumpIfLocalsCmp {
                cmp: CmpOp::GtEq,
                a: 1,
                b: 2,
                offset: 11,
            },
            /* 1 */ Instruction::LoadVar(3),
            /* 2 */ Instruction::JumpIfTrueOrPop(2),
            /* 3 */ Instruction::LoadVar(4),
            /* 4 */ Instruction::PopJumpIfFalse(5),
            /* 5 */ Instruction::LoadVar(5),
            /* 6 */ Instruction::LoadConst(1),
            /* 7 */ Instruction::AddInt,
            /* 8 */ Instruction::StoreVar(5),
            /* 9 */ Instruction::AddLocalSmall(1, 1, 1),
            /* 10 */ Instruction::Jump(-10),
            /* 11 */ Instruction::LoadVar(5),
            /* 12 */ Instruction::Return,
        ]);
        let layout = verify(&bc).unwrap();
        assert_eq!(layout.max, 2);
        // The `or` keeps its value when it jumps and pops it when it falls.
        assert_eq!(layout.depths[4], Some(1));
        assert_eq!(layout.depths[3], Some(0));
    }

    #[test]
    fn a_loop_that_grows_the_stack_is_rejected() {
        let bc = bytecode(vec![Instruction::LoadConst(1), Instruction::Jump(-1)]);
        assert_eq!(
            verify(&bc),
            Err(StackError::Mismatch {
                index: 0,
                depth: 0,
                other: 1
            })
        );
    }

    #[test]
    fn underflow_overflow_bad_targets_and_falling_off_are_rejected() {
        assert_eq!(
            verify(&bytecode(vec![Instruction::AddInt, Instruction::Return])),
            Err(StackError::Underflow {
                index: 0,
                depth: 0,
                pops: 2
            })
        );
        assert_eq!(
            verify(&bytecode(vec![Instruction::Jump(7)])),
            Err(StackError::BadTarget {
                index: 0,
                target: 7
            })
        );
        assert_eq!(
            verify(&bytecode(vec![Instruction::LoadConst(0)])),
            Err(StackError::FallsOffEnd { index: 0 })
        );
        let mut deep = vec![Instruction::LoadConst(1); MAX_OPERAND_DEPTH + 1];
        deep.push(Instruction::Return);
        assert_eq!(
            verify(&bytecode(deep)),
            Err(StackError::TooDeep {
                index: MAX_OPERAND_DEPTH + 1,
                depth: MAX_OPERAND_DEPTH + 1
            })
        );
    }

    #[test]
    fn handlers_start_empty_and_the_hook_boundary_holds_one_null() {
        let mut bc = bytecode(vec![
            /* 0 */ Instruction::LoadVar(1),
            /* 1 */ Instruction::LoadVar(2),
            /* 2 */ Instruction::Throw,
            // Reachable only by unwinding, which truncates to the locals.
            /* 3 */
            Instruction::LoadVar(4),
            /* 4 */ Instruction::Return,
        ]);
        bc.exception_table.push(ExceptionTableEntry {
            start_pc: 0,
            end_pc: 3,
            handler_pc: 3,
            error_slot: 4,
            context_slot: 5,
        });
        let layout = verify(&bc).unwrap();
        assert_eq!(layout.depths[3], Some(0));
        assert_eq!(layout.max, 2);

        let bc = bytecode(vec![
            Instruction::BeginTraceHook(false),
            Instruction::EndTraceHook,
            Instruction::LoadVar(1),
            Instruction::Return,
        ]);
        assert_eq!(verify(&bc).unwrap().depths[1], Some(1));
    }

    #[test]
    fn call_arguments_come_from_the_layout_or_the_callee() {
        let call = |layout: Option<usize>, arity: Option<usize>| {
            let mut bc = bytecode(vec![
                Instruction::LoadVar(1),
                Instruction::LoadVar(2),
                Instruction::Call {
                    callee: GlobalIndex::from_raw(0),
                    ntypeargs: 0,
                },
                Instruction::Return,
            ]);
            if let Some(slots) = layout {
                bc.call_layouts
                    .insert(2, baml_type::CallLayout(vec![None; slots]));
            }
            let arity_of = move |_: GlobalIndex| arity;
            bc.verify_stack(&StackContext::new(&arity_of))
                .map(|layout| layout.depths[3])
        };
        assert_eq!(call(Some(2), None), Ok(Some(1)));
        assert_eq!(call(None, Some(2)), Ok(Some(1)));
        assert_eq!(call(None, None), Err(StackError::UnknownArity { index: 2 }));
        // A trampoline's arguments are on the stack before its first
        // instruction.
        let bc = bytecode(vec![Instruction::CallIndirect, Instruction::Return]);
        let ctx = StackContext {
            entry_depth: 3,
            global_arity: &no_arity,
            indirect_args: Some(2),
        };
        assert_eq!(bc.verify_stack(&ctx).unwrap().depths[1], Some(1));
    }

    #[test]
    fn variable_arity_instructions_count_their_operands() {
        let mut bc = bytecode(vec![
            Instruction::LoadVar(1),
            Instruction::LoadVar(2),
            Instruction::LoadType(0),
            Instruction::AllocArray(2),
            Instruction::LoadVar(1),
            Instruction::LoadVar(2),
            Instruction::InitInstance(0),
            Instruction::Pop(2),
            Instruction::LoadVar(1),
            Instruction::JumpTable(0),
            Instruction::LoadConst(0),
            Instruction::Return,
        ]);
        bc.class_init_plans.push(ClassInitPlan {
            class_obj: crate::ObjectIndex::from_raw(0),
            ntypeargs: 1,
            fields: vec![0],
        });
        let mut table = JumpTableData::new(0, 0);
        table.set(0, 1);
        table.default = 1;
        bc.jump_tables.push(table);
        let layout = verify(&bc).unwrap();
        // `[a, b, T]` becomes one array; a field and a type argument become
        // one instance.
        assert_eq!(layout.depths[4], Some(1));
        assert_eq!(layout.depths[7], Some(2));
        assert_eq!(layout.max, 3);
    }
}
