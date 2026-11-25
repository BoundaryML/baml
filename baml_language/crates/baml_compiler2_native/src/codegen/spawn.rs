//! `spawn` and `await`: starting a task and waiting for its future.
//!
//! A `spawn` calls the runtime with the closure to run. A new task starts
//! in the task shim, a compiled function the runtime knows by its symbol,
//! which calls the closure and settles the task's future with what it
//! returns or throws. Every function returns a tag and a payload whatever
//! its result's type, so one shim serves every task. The runtime switches
//! to the new task before the call returns.
//!
//! An `await` reads the future's state itself, and loads the value when the
//! state says it is there: the pair the task's closure returned, read as
//! the type the future is of. Otherwise it calls the runtime, which returns
//! once the future has settled, with a value or with an error: the state
//! is read again to tell which, and an error is thrown where the `await`
//! is.

use baml_compiler2_mir::{BlockId, Operand, Place};
use cranelift_codegen::ir::{self, InstBuilder, MemFlags, condcodes::IntCC, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use super::{
    Backend, PAIR, Runtime, TRAP_AFTER_PANIC, Translator, WORD, closure_access, imm, internal,
    offset, slot_access, unwind,
};
use crate::{
    CompileError, layout,
    types::{
        NativeTy, PLAN_BODY, PLAN_CANCEL_TOKENS, PLAN_SEMAPHORES, narrowed, place_local, plan_body,
    },
};

/// The symbol of the task shim, which the object exports.
pub(crate) const TASK_SHIM: &str = "baml_native_task_shim";

/// The body of the task shim: what the runtime's boot code calls, on the
/// new task's stack and with the worker in the pinned register, to run the
/// task's closure. When it returns the task is over.
///
/// Nothing caught what arrives at its handler, which takes every error and
/// reads its trace as the program's entry does: the task's future settles
/// with the two, for whoever awaits the future to be thrown them.
pub(super) fn task_shim(
    backend: &mut Backend<'_>,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) -> Result<(), CompileError> {
    let start = backend.runtime(Runtime::TaskStart)?;
    let finish = backend.runtime(Runtime::TaskFinish)?;
    let fail = backend.runtime(Runtime::TaskFail)?;
    // A task's closure takes nothing but itself.
    let closure_signature = Backend::clif_signature(0, crate::program::Entry::Value, 0);

    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let worker = builder.block_params(block)[0];
    let start = backend.function_ref(start, builder.func);
    let call = builder.ins().call(start, &[worker]);
    let closure = builder.inst_results(call)[0];
    builder.declare_value_needs_stack_map(closure);
    let code = builder.ins().load(
        WORD,
        closure_access(),
        closure,
        offset(layout::CLOSURE_CODE),
    );
    let closure_signature = builder.import_signature(closure_signature);
    let uncaught = builder.create_block();
    builder.append_block_param(uncaught, PAIR);
    builder.append_block_param(uncaught, WORD);
    builder.set_cold_block(uncaught);
    let (tag, payload) = unwind::try_call(
        &mut builder,
        &unwind::Called::Indirect(closure_signature, code),
        &[closure],
        &[unwind::Edge {
            tag: None,
            landing: uncaught,
            reads_context: true,
        }],
    );
    // The worker is read again: the task may have parked since it started,
    // and gone on under another.
    let worker = builder.ins().get_pinned_reg(WORD);
    let finish = backend.function_ref(finish, builder.func);
    builder.ins().call(finish, &[worker, tag, payload]);
    builder.ins().return_(&[]);

    builder.switch_to_block(uncaught);
    let (error, trace) = {
        let arriving = builder.block_params(uncaught);
        (arriving[0], arriving[1])
    };
    let (tag, payload) = builder.ins().isplit(error);
    let worker = builder.ins().get_pinned_reg(WORD);
    let fail = backend.function_ref(fail, builder.func);
    builder.ins().call(fail, &[worker, tag, payload, trace]);
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}

impl<'db> Translator<'_, 'db> {
    /// `future = spawn plan`, then go on at `resume`.
    pub(super) fn spawn(
        &mut self,
        plan: &Operand<'db>,
        future: &Place,
        resume: BlockId,
    ) -> Result<(), CompileError> {
        let plan_ty = narrowed(self.operand_ty(plan)?);
        let (fields, _) =
            plan_body(self.db, &plan_ty).ok_or_else(|| internal("a checked spawn is of a plan"))?;
        let NativeTy::Class(plan_class, _) = plan_ty else {
            return Err(internal("a checked spawn is of a plan"));
        };
        let plan = self.operand_object(plan)?;

        // A task waits for no semaphore and hears of no cancellation yet,
        // so a plan that names either cannot be started.
        let mut limited = None;
        for index in [PLAN_SEMAPHORES, PLAN_CANCEL_TOKENS] {
            let at = self.field_offset(plan_class, index);
            let array = self.slot_load(&fields[index], plan, at);
            let length = self.builder.ins().load(
                types::I64,
                slot_access(),
                array,
                offset(layout::ARRAY_LENGTH),
            );
            limited = Some(match limited {
                Some(other) => self.builder.ins().bor(other, length),
                None => length,
            });
        }
        let limited = limited.expect("a plan has both arrays");
        let unsupported = self.builder.create_block();
        let start = self.builder.create_block();
        self.builder.set_cold_block(unsupported);
        self.builder
            .ins()
            .brif(limited, unsupported, &[], start, &[]);
        self.builder.seal_block(unsupported);
        self.builder.seal_block(start);
        self.builder.switch_to_block(unsupported);
        self.emit_runtime_call(Runtime::SpawnUnsupported, &[])?;
        self.builder.ins().trap(TRAP_AFTER_PANIC);

        self.builder.switch_to_block(start);
        let at = self.field_offset(plan_class, PLAN_BODY);
        let body = self.slot_load(&fields[PLAN_BODY], plan, at);
        let (_, closure) = self.builder.ins().isplit(body);
        let started = self
            .call_runtime(Runtime::Spawn, &[closure])?
            .ok_or_else(|| internal("a spawn returns the future"))?;
        self.builder.declare_value_needs_stack_map(started);
        self.write_place(future, started)?;
        let resume = self.block(resume);
        self.builder.ins().jump(resume, &[]);
        Ok(())
    }

    /// `destination = await future`, then go on at `target`.
    pub(super) fn await_future(
        &mut self,
        future: &Place,
        destination: &Place,
        target: BlockId,
    ) -> Result<(), CompileError> {
        let NativeTy::Future(value_ty) = narrowed(self.place_ty(future)?) else {
            return Err(internal("a checked await is of a future"));
        };
        let destination = place_local(destination).map_err(internal)?;
        let expected = self.local_ty(destination)?;
        let future = self.read_object(future)?;
        self.builder.declare_value_needs_stack_map(future);

        // The state is read before the value, and in that order: it is
        // written last, by whichever worker ran the task.
        let state = self
            .builder
            .ins()
            .iadd_imm(future, imm(layout::FUTURE_STATE));
        let state = self
            .builder
            .ins()
            .atomic_load(types::I64, MemFlags::trusted(), state);
        let ready = self
            .builder
            .ins()
            .icmp_imm(IntCC::Equal, state, imm(layout::FUTURE_READY));
        let load = self.builder.create_block();
        let wait = self.builder.create_block();
        self.builder.set_cold_block(wait);
        self.builder.ins().brif(ready, load, &[], wait, &[]);
        self.builder.seal_block(wait);

        self.builder.switch_to_block(wait);
        self.call_runtime(Runtime::Await, &[future])?;
        // It has settled, so its state no longer changes.
        let state = self.builder.ins().load(
            types::I64,
            slot_access(),
            future,
            offset(layout::FUTURE_STATE),
        );
        let ready = self
            .builder
            .ins()
            .icmp_imm(IntCC::Equal, state, imm(layout::FUTURE_READY));
        let failed = self.builder.create_block();
        self.builder.set_cold_block(failed);
        self.builder.ins().brif(ready, load, &[], failed, &[]);
        self.builder.seal_block(failed);
        self.builder.seal_block(load);

        self.builder.switch_to_block(failed);
        self.forget_arrays();
        let worker = self.mutator();
        let take = self.runtime_ref(Runtime::AwaitError)?;
        let taken = self.builder.ins().call(take, &[worker, future]);
        let (tag, payload) = {
            let error = self.builder.inst_results(taken);
            (error[0], error[1])
        };
        let error = self.pair(tag, payload);
        self.builder.declare_value_needs_stack_map(error);
        // Thrown again as a handler in the task would have: with what the
        // task's throw travelled with, so that a handler here reads the
        // context of that throw.
        let trace =
            self.builder
                .ins()
                .load(WORD, slot_access(), future, offset(layout::FUTURE_TRACE));
        self.builder.declare_value_needs_stack_map(trace);
        self.raise(error, Some(trace))?;

        self.builder.switch_to_block(load);
        // What the task's closure returned: a value, whatever its type.
        let value = self.pair_load(&value_ty, future, layout::FUTURE_VALUE);
        let value = self.coerce(value, &value_ty, &expected);
        self.write_local(destination, value);
        let target = self.block(target);
        self.builder.ins().jump(target, &[]);
        Ok(())
    }
}
