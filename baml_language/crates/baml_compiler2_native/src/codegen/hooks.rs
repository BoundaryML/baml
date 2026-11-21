//! The compiled code between the runtime's natives and BAML functions, in
//! both directions.
//!
//! A native calls a BAML function (a class's own `to_json`, say) through an
//! entry made here: a function with the C convention that takes one value
//! as a union holds it and returns two words. It calls the BAML function
//! under a handler that takes every error, so nothing ever unwinds into
//! the Rust frames below it: what the function returns comes back with a
//! zero, and what it throws with a one, its trace left in the worker's
//! scratch word. The handler is one the unwinder's look ahead goes past, so
//! whether the trace is made is still decided by the handlers further out.
//!
//! Compiled code calls an `async` native through a stub made here: a
//! function with the convention of a BAML function, which drives the
//! native. It calls the native's symbol, which makes the native's state,
//! and then the runtime's poll, again and again, doing between two polls
//! what the native stopped for: calling the entry the native asked for, or
//! waiting for its I/O. Both are done here, in compiled code, so that no
//! frame of the runtime is on the stack while the task is stopped or runs
//! BAML: every call into the runtime returns first. When the poll says the
//! native is done the stub returns its value, or throws again what the
//! native says compiled code threw. Through the stub the native's caller
//! sees a call that returns or unwinds, with the caller's handlers on
//! record at its return address.

use std::collections::HashMap;

use cranelift_codegen::{
    Context,
    ir::{self, AbiParam, InstBuilder, MemFlags, condcodes::IntCC, types},
    isa::CallConv,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};

use super::{
    Backend, Declared, Runtime, TRAP_AFTER_PANIC, WORD, internal, mutator_access, offset, unwind,
};
use crate::{
    CompileError,
    json::JsonTypes,
    layout,
    natives::Native,
    program::{FnKey, Program},
};

// A stub tests for a native that is done by testing for zero.
const _: () = assert!(layout::STEP_DONE == 0);

/// The tag of the one handler an entry has.
const ENTRY_HANDLER: u32 = 0;

#[derive(Default)]
pub(super) struct Hooks {
    /// Each entry a native calls, with the BAML function it runs.
    entries: Vec<(FuncId, FuncId)>,
    /// The stub each `async` native called so far is called through and
    /// the native's own symbol, by the builtin's path.
    stubs: HashMap<&'static str, (FuncId, FuncId)>,
}

/// Declare an entry for each `to_json` and `from_json` the runtime calls,
/// and tell the descriptions of their classes where they are. Called
/// before any type is described.
pub(super) fn declare<'db>(
    backend: &mut Backend<'db>,
    program: &Program<'db>,
    declared: &HashMap<FnKey<'db>, Declared<'db>>,
) -> Result<(), CompileError> {
    let mut signature = ir::Signature::new(backend.call_conv());
    signature.params = vec![AbiParam::new(WORD)];
    signature.returns = vec![AbiParam::new(WORD); 2];
    let mut entries = HashMap::new();
    for (class, functions) in &program.hooks {
        let mut ids = [None; 2];
        for (id, key) in ids.iter_mut().zip(functions) {
            let Some(key) = key else { continue };
            let target = declared
                .get(key)
                .ok_or_else(|| internal("a class's own to_json is part of the program"))?
                .id;
            let name = format!("baml_hook_{}", backend.hooks.entries.len());
            let entry = backend
                .module
                .declare_function(&name, Linkage::Local, &signature)
                .map_err(internal)?;
            backend.hooks.entries.push((entry, target));
            *id = Some(entry.as_u32());
        }
        entries.insert(*class, ids);
    }
    backend.json = JsonTypes::with_entries(entries);
    Ok(())
}

impl<'db> Backend<'db> {
    /// The stub compiled code calls the `async` native `native` through,
    /// whose symbol takes `params`, the worker first.
    pub(super) fn async_stub(
        &mut self,
        native: &'static Native,
        params: &[AbiParam],
    ) -> Result<FuncId, CompileError> {
        if let Some((stub, _)) = self.hooks.stubs.get(native.path) {
            return Ok(*stub);
        }
        // The symbol makes the native's state and returns its address.
        let mut imported = ir::Signature::new(self.call_conv());
        imported.params = params.to_vec();
        imported.returns = vec![AbiParam::new(WORD)];
        let symbol = self
            .module
            .declare_function(native.symbol, Linkage::Import, &imported)
            .map_err(internal)?;
        let mut signature = ir::Signature::new(CallConv::Tail);
        signature.params = params[1..].to_vec();
        signature.returns = vec![AbiParam::new(types::I64)];
        let stub = self
            .module
            .declare_function(
                &format!("baml_async_{}", native.symbol),
                Linkage::Local,
                &signature,
            )
            .map_err(internal)?;
        self.hooks.stubs.insert(native.path, (stub, symbol));
        Ok(stub)
    }

    /// The stub of the `async` native `native`, which takes no type
    /// arguments, for the method table to enter: the classes its
    /// declaration names are described as for a call of it.
    pub(super) fn dispatched_stub(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        native: &'static Native,
    ) -> Result<FuncId, CompileError> {
        let (signature, _) = native.callable().map_err(internal)?;
        self.name_classes(db, native)?;
        let mut params = vec![AbiParam::new(WORD)];
        for ty in signature.params {
            params.push(
                super::native_call::abi_param(ty)
                    .ok_or_else(|| internal("a native takes no `null`"))?,
            );
        }
        self.async_stub(native, &params)
    }
}

/// Compile every entry and every stub declared.
pub(super) fn define_all(
    backend: &mut Backend<'_>,
    context: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    clif: &mut Option<String>,
    disassembly: &mut Option<String>,
) -> Result<(), CompileError> {
    let signature_of = |backend: &Backend<'_>, id: FuncId| {
        backend
            .module
            .declarations()
            .get_function_decl(id)
            .signature
            .clone()
    };
    for (entry, target) in backend.hooks.entries.clone() {
        context.func.signature = signature_of(backend, entry);
        entry_body(backend, target, &mut context.func, builder_context);
        // It has no name in a trace: the function it runs is the frame a
        // program knows.
        super::define(backend, context, entry, "", clif, disassembly)?;
    }
    let mut stubs: Vec<_> = backend
        .hooks
        .stubs
        .iter()
        .map(|(name, ids)| (*name, *ids))
        .collect();
    stubs.sort_unstable();
    // A stub is the native's frame in a trace, under the builtin's name.
    for (path, (stub, native)) in stubs {
        context.func.signature = signature_of(backend, stub);
        stub_body(backend, native, &mut context.func, builder_context)?;
        super::define(backend, context, stub, path, clif, disassembly)?;
    }
    Ok(())
}

/// The body of an entry: call `target` with the one argument, and return
/// what it returned and zero, or what it threw and one.
fn entry_body(
    backend: &mut Backend<'_>,
    target: FuncId,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) {
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let argument = builder.block_params(block)[0];
    let target = backend.function_ref(target, builder.func);
    let caught = builder.create_block();
    builder.append_block_param(caught, WORD);
    builder.append_block_param(caught, WORD);
    builder.set_cold_block(caught);
    backend
        .unwind
        .catches
        .insert(ENTRY_HANDLER, unwind::Catch::handing_on());
    let returned = unwind::try_call(
        &mut builder,
        &unwind::Called::Direct(target),
        &[argument],
        &[(Some(ENTRY_HANDLER), caught)],
    );
    let zero = builder.ins().iconst(WORD, 0);
    builder.ins().return_(&[returned, zero]);

    builder.switch_to_block(caught);
    let (error, trace) = {
        let arriving = builder.block_params(caught);
        (arriving[0], arriving[1])
    };
    // The native reads the trace before anything else uses the word.
    let worker = builder.ins().get_pinned_reg(WORD);
    builder.ins().store(
        mutator_access(),
        trace,
        worker,
        offset(layout::MUTATOR_SCRATCH),
    );
    let one = builder.ins().iconst(WORD, 1);
    builder.ins().return_(&[error, one]);
    builder.seal_all_blocks();
    builder.finalize();
}

/// The body of a stub: start `native` with the worker and the stub's own
/// arguments, and poll it until it is done, doing here what it stops for.
///
/// The worker is read from the pinned register for every call: after a
/// stop it is the one the task went on under. What the stub keeps across a
/// stop is the address of the native's state, which is no heap object, so
/// its frame holds no root.
fn stub_body(
    backend: &mut Backend<'_>,
    native: FuncId,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) -> Result<(), CompileError> {
    let rethrow = backend.runtime(Runtime::Rethrow)?;
    let poll = backend.runtime(Runtime::NativePoll)?;
    let wait = backend.runtime(Runtime::IoWait)?;
    // An entry a native calls, with as many arguments as a native passes.
    let mut entry = ir::Signature::new(backend.call_conv());
    entry.params = vec![AbiParam::new(WORD); layout::NATIVE_CALL_ARGS];
    entry.returns = vec![AbiParam::new(WORD); 2];

    let mut builder = FunctionBuilder::new(function, builder_context);
    let start = builder.create_block();
    builder.append_block_params_for_function_params(start);
    // What the entry called last returned and whether it threw that: both
    // zero before the first poll and after a wait.
    let polling = builder.create_block();
    builder.append_block_param(polling, WORD);
    builder.append_block_param(polling, WORD);
    let done = builder.create_block();
    let stopped = builder.create_block();
    let raise = builder.create_block();
    let asked = builder.create_block();
    let call = builder.create_block();
    let waiting = builder.create_block();
    builder.set_cold_block(raise);

    builder.switch_to_block(start);
    let worker = builder.ins().get_pinned_reg(WORD);
    let mut arguments = vec![worker];
    arguments.extend_from_slice(builder.block_params(start));
    let native = backend.function_ref(native, builder.func);
    let started = builder.ins().call(native, &arguments);
    let state = builder.inst_results(started)[0];
    let zero = builder.ins().iconst(WORD, 0);
    builder.ins().jump(polling, &[zero.into(), zero.into()]);

    builder.switch_to_block(polling);
    let (returned, threw) = {
        let last = builder.block_params(polling);
        (last[0], last[1])
    };
    let worker = builder.ins().get_pinned_reg(WORD);
    let poll = backend.function_ref(poll, builder.func);
    let polled = builder.ins().call(poll, &[worker, state, returned, threw]);
    let (step, word) = {
        let results = builder.inst_results(polled);
        (results[0], results[1])
    };
    builder.ins().brif(step, stopped, &[], done, &[]);

    builder.switch_to_block(done);
    builder.ins().return_(&[word]);

    builder.switch_to_block(stopped);
    let is_throw = builder
        .ins()
        .icmp_imm(IntCC::Equal, step, super::imm(layout::STEP_THREW));
    builder.ins().brif(is_throw, raise, &[], asked, &[]);

    builder.switch_to_block(raise);
    let worker = builder.ins().get_pinned_reg(WORD);
    let trace = builder.ins().load(
        WORD,
        mutator_access(),
        worker,
        offset(layout::MUTATOR_SCRATCH),
    );
    let rethrow = backend.function_ref(rethrow, builder.func);
    builder.ins().call(rethrow, &[word, trace]);
    builder.ins().trap(TRAP_AFTER_PANIC);

    builder.switch_to_block(asked);
    let is_call = builder
        .ins()
        .icmp_imm(IntCC::Equal, step, super::imm(layout::STEP_CALL));
    builder.ins().brif(is_call, call, &[], waiting, &[]);

    // The entry may run for as long as any BAML, park and collect. What
    // it returns is rooted by the poll it is handed to, before anything
    // allocates.
    builder.switch_to_block(call);
    let passed: Vec<ir::Value> = (0..layout::NATIVE_CALL_ARGS)
        .map(|index| {
            builder.ins().load(
                WORD,
                MemFlags::trusted(),
                state,
                offset(layout::NATIVE_STATE_ARGS + index * layout::WORD),
            )
        })
        .collect();
    let entry = builder.import_signature(entry);
    let called = builder.ins().call_indirect(entry, word, &passed);
    let (returned, threw) = {
        let results = builder.inst_results(called);
        (results[0], results[1])
    };
    builder
        .ins()
        .jump(polling, &[returned.into(), threw.into()]);

    // Whatever is none of the others is a wait.
    builder.switch_to_block(waiting);
    let wait = backend.function_ref(wait, builder.func);
    builder.ins().call(wait, &[word]);
    builder.ins().jump(polling, &[zero.into(), zero.into()]);

    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}
