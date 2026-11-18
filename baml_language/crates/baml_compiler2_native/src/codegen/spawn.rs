//! `spawn` and `await`: starting a task and waiting for its future.
//!
//! A `spawn` calls the runtime with the closure to run and the address of a
//! task shim: the compiled function a new task starts in, which calls the
//! closure and settles the task's future with what it returns or throws.
//! There is one shim per machine type a closure can return, since the shim
//! is what knows which register the result is in. The runtime switches to
//! the new task before the call returns.
//!
//! An `await` reads the future's state itself, and loads the value when the
//! state says it is there. Otherwise it calls the runtime, which returns
//! once the future has settled, with a value or with an error: the state
//! is read again to tell which, and an error is thrown where the `await`
//! is.

use baml_compiler2_mir::{BlockId, Operand, Place};
use cranelift_codegen::ir::{self, AbiParam, InstBuilder, MemFlags, condcodes::IntCC, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};

use super::{
    Backend, Runtime, TRAP_AFTER_PANIC, Translator, WORD, clif_type, closure_access, imm, internal,
    offset, slot_access, unwind,
};
use crate::{
    CompileError, layout,
    types::{
        NativeTy, PLAN_BODY, PLAN_CANCEL_TOKENS, PLAN_SEMAPHORES, field_slots, narrowed,
        place_local, plan_body,
    },
};

impl Backend<'_> {
    /// The shim of tasks whose closure returns a value of the machine type
    /// `result`, declared the first time one is spawned.
    fn task_shim(&mut self, result: ir::Type) -> Result<FuncId, CompileError> {
        if let Some(shim) = self.task_shims.get(&result) {
            return Ok(*shim);
        }
        let mut signature = ir::Signature::new(self.call_conv());
        signature.params.push(AbiParam::new(WORD));
        let shim = self
            .module
            .declare_function(
                &format!("baml_task_shim_{result}"),
                Linkage::Local,
                &signature,
            )
            .map_err(internal)?;
        self.task_shims.insert(result, shim);
        Ok(shim)
    }

    /// The task shims declared, with the machine type each is for, in a
    /// stable order.
    pub(super) fn declared_task_shims(&self) -> Vec<(ir::Type, FuncId)> {
        let mut shims: Vec<_> = self
            .task_shims
            .iter()
            .map(|(result, id)| (*result, *id))
            .collect();
        shims.sort_unstable_by_key(|(_, id)| id.as_u32());
        shims
    }
}

/// The body of a task shim: what the runtime's boot code calls, on the new
/// task's stack and with the worker in the pinned register, to run a
/// closure that returns a `result`. When it returns the task is over.
///
/// Nothing caught what arrives at its handler: the task's future settles
/// with it, for whoever awaits the future to be thrown.
pub(super) fn task_shim(
    backend: &mut Backend<'_>,
    result: ir::Type,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) -> Result<(), CompileError> {
    let start = backend.runtime(Runtime::TaskStart)?;
    let finish = backend.runtime(Runtime::TaskFinish)?;
    let fail = backend.runtime(Runtime::TaskFail)?;
    let mut closure_signature = ir::Signature::new(cranelift_codegen::isa::CallConv::Tail);
    closure_signature.params.push(AbiParam::new(WORD));
    closure_signature.returns.push(AbiParam::new(result));

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
    builder.append_block_param(uncaught, WORD);
    builder.append_block_param(uncaught, WORD);
    builder.set_cold_block(uncaught);
    let returned = unwind::try_call(
        &mut builder,
        &unwind::Called::Indirect(closure_signature, code),
        &[closure],
        uncaught,
        false,
    );
    let bits = match result {
        types::F64 => builder.ins().bitcast(types::I64, MemFlags::new(), returned),
        types::I8 => builder.ins().uextend(types::I64, returned),
        _ => returned,
    };
    // The worker is read again: the task may have parked since it started,
    // and gone on under another.
    let worker = builder.ins().get_pinned_reg(WORD);
    let finish = backend.function_ref(finish, builder.func);
    builder.ins().call(finish, &[worker, bits]);
    builder.ins().return_(&[]);

    builder.switch_to_block(uncaught);
    let (error, trace) = {
        let arriving = builder.block_params(uncaught);
        (arriving[0], arriving[1])
    };
    let worker = builder.ins().get_pinned_reg(WORD);
    let fail = backend.function_ref(fail, builder.func);
    builder.ins().call(fail, &[worker, error, trace]);
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
        let (fields, body) =
            plan_body(self.db, &plan_ty).ok_or_else(|| internal("a checked spawn is of a plan"))?;
        let slots = field_slots(&fields);
        let field = |index: usize| layout::field_offset(slots[index]);
        let plan = self.operand(plan)?;

        // A task waits for no semaphore and hears of no cancellation yet,
        // so a plan that names either cannot be started.
        let mut limited = None;
        for index in [PLAN_SEMAPHORES, PLAN_CANCEL_TOKENS] {
            let array = self.load_slot(&fields[index], plan, field(index));
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
        let closure = self.load_slot(&fields[PLAN_BODY], plan, field(PLAN_BODY));
        let shim = self.backend.task_shim(clif_type(&body.result))?;
        let shim = self.function_ref(shim);
        let shim = self.builder.ins().func_addr(WORD, shim);
        let flags = self.word_const(if body.result.is_ref() {
            layout::FUTURE_VALUE_IS_REF
        } else {
            0
        });
        let started = self
            .call_runtime(Runtime::Spawn, &[closure, shim, flags])?
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
        let future = self.read_place(future)?;
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
        let worker = self.mutator();
        let error = self
            .call_runtime(Runtime::AwaitError, &[worker, future])?
            .ok_or_else(|| internal("a failed future has its error"))?;
        self.builder.declare_value_needs_stack_map(error);
        self.raise(error, None)?;

        self.builder.switch_to_block(load);
        let value = self.load_slot(&value_ty, future, layout::FUTURE_VALUE);
        let value = self.coerce(value, &value_ty, &expected)?;
        self.write_local(destination, value);
        let target = self.block(target);
        self.builder.ins().jump(target, &[]);
        Ok(())
    }
}
