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
//! function with the convention of a BAML function, which calls the native
//! and throws again what the native says compiled code threw. The native
//! runs BAML and so ends the ways BAML does, and through the stub its
//! caller sees exactly that: a call that returns or unwinds, with the
//! caller's handlers on record at its return address.

use std::collections::HashMap;

use cranelift_codegen::{
    Context,
    ir::{self, AbiParam, InstBuilder, types},
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

impl Backend<'_> {
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
        let mut imported = ir::Signature::new(self.call_conv());
        imported.params = params.to_vec();
        imported.returns = vec![AbiParam::new(types::I64), AbiParam::new(WORD)];
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

/// The body of a stub: call `native` with the worker and the stub's own
/// arguments, and return what it returned, or throw again what it says
/// compiled code threw, with the trace it left in the scratch word.
fn stub_body(
    backend: &mut Backend<'_>,
    native: FuncId,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) -> Result<(), CompileError> {
    let rethrow = backend.runtime(Runtime::Rethrow)?;
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let worker = builder.ins().get_pinned_reg(WORD);
    let mut arguments = vec![worker];
    arguments.extend_from_slice(builder.block_params(block));
    let native = backend.function_ref(native, builder.func);
    let call = builder.ins().call(native, &arguments);
    let (bits, threw) = {
        let results = builder.inst_results(call);
        (results[0], results[1])
    };
    let raise = builder.create_block();
    let done = builder.create_block();
    builder.set_cold_block(raise);
    builder.ins().brif(threw, raise, &[], done, &[]);

    builder.switch_to_block(done);
    builder.ins().return_(&[bits]);

    builder.switch_to_block(raise);
    let worker = builder.ins().get_pinned_reg(WORD);
    let trace = builder.ins().load(
        WORD,
        mutator_access(),
        worker,
        offset(layout::MUTATOR_SCRATCH),
    );
    let rethrow = backend.function_ref(rethrow, builder.func);
    builder.ins().call(rethrow, &[bits, trace]);
    builder.ins().trap(TRAP_AFTER_PANIC);
    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}
