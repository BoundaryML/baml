//! The compiled code between the runtime's natives and BAML functions, in
//! both directions.
//!
//! A native calls a BAML function (a class's own `to_json`, say) through an
//! entry made here: a function with the C convention that takes two values,
//! each a tag and a payload, and returns one. It calls the BAML function
//! under a handler that takes every error, so nothing ever unwinds into
//! the Rust frames below it: what the function returns comes back as a
//! pair, and what it throws as one with [`layout::THROWN`] set in its tag,
//! its trace left in the worker's scratch word by the unwinder. The handler
//! is one the unwinder's look ahead goes past, so whether the trace is made
//! is still decided by the handlers further out.
//!
//! Compiled code calls an `async` native through a stub made here: a
//! function with the convention of a BAML function, which drives the
//! native. It takes each of the native's parameters as a tag and a payload
//! and returns two words the same way, so a dispatching call enters it as
//! it enters any implementation. It calls the native's symbol with what the
//! native takes of those words, which makes the native's state,
//! and then the runtime's poll, again and again, doing between two polls
//! what the native stopped for: calling the entry the native asked for, or
//! waiting for its I/O. Both are done here, in compiled code, so that no
//! frame of the runtime is on the stack while the task is stopped or runs
//! BAML: every call into the runtime returns first. When the poll says the
//! native is done the stub returns the value it left in its state, a tag
//! and a payload, or throws again what the native says compiled code threw. Through the
//! stub the native's caller sees a call that returns or unwinds, with the
//! caller's handlers on record at its return address.

use std::collections::HashMap;

use baml_compiler2_hir_ty::extern_loc::ClassRef;
use cranelift_codegen::{
    Context,
    ir::{self, AbiParam, InstBuilder, MemFlags, condcodes::IntCC},
    isa::CallConv,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};

use super::{
    Backend, Declared, PAIR, Runtime, TRAP_AFTER_PANIC, WORD, internal, mutator_access, offset,
    unwind,
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
pub(super) struct Hooks<'db> {
    /// Each entry a native calls, with the BAML function it runs and how
    /// many values that function takes.
    entries: Vec<(FuncId, FuncId, usize)>,
    /// The entry of the `eq` of each class that has one of its own.
    equals: Vec<(ClassRef<'db>, FuncId)>,
    /// The stub each `async` native called so far is called through, the
    /// native's own symbol and what the stub takes, by the builtin's path.
    stubs: HashMap<&'static str, (FuncId, FuncId, Vec<StubParam>)>,
}

/// What the stub of an `async` native takes for one parameter of the
/// native's symbol after the worker.
#[derive(Clone)]
pub(super) enum StubParam {
    /// A type object, or zero: a word, passed on.
    Type,
    /// A value: its tag and its payload, both passed on.
    Value,
    /// A value of a type that fixes its tag, which the native takes as
    /// this one parameter: its tag and its payload, of which the payload
    /// is passed on.
    Exact(AbiParam),
}

/// Declare an entry for each `to_json` and `from_json` the runtime calls,
/// and tell the descriptions of their classes where they are. Called
/// before any type is described.
pub(super) fn declare<'db>(
    backend: &mut Backend<'db>,
    program: &Program<'db>,
    declared: &HashMap<FnKey<'db>, Declared<'db>>,
) -> Result<(), CompileError> {
    // Every entry takes as many words as a native passes, two values, and
    // reads as many of them as its function has parameters.
    let mut signature = ir::Signature::new(backend.call_conv());
    signature.params = vec![AbiParam::new(WORD); layout::NATIVE_CALL_ARGS];
    signature.returns = vec![AbiParam::new(WORD); 2];
    let mut entries = HashMap::new();
    let declare = |backend: &mut Backend<'db>, key: &FnKey<'db>, what: &str| {
        let target = declared
            .get(key)
            .ok_or_else(|| internal(format!("a class's own {what} is part of the program")))?;
        if 2 * target.signature.params.len() > layout::NATIVE_CALL_ARGS {
            return Err(internal(format!(
                "a class's own {what} takes more values than a native passes"
            )));
        }
        let name = format!("baml_hook_{}", backend.hooks.entries.len());
        let entry = backend
            .module
            .declare_function(&name, Linkage::Local, &signature)
            .map_err(internal)?;
        backend
            .hooks
            .entries
            .push((entry, target.id, target.signature.params.len()));
        Ok(entry)
    };
    for (class, functions) in &program.hooks {
        let mut ids = [None; 2];
        for (id, key) in ids.iter_mut().zip(functions) {
            let Some(key) = key else { continue };
            *id = Some(declare(backend, key, "to_json")?.as_u32());
        }
        entries.insert(*class, ids);
    }
    for (class, key) in &program.equals {
        let entry = declare(backend, key, "eq")?;
        backend.hooks.equals.push((*class, entry));
    }
    backend.json = JsonTypes::with_entries(entries);
    Ok(())
}

/// The list the runtime finds the `eq` of a class in, when it compares two
/// instances it meets inside other values.
const EQUALS_SYMBOL: &str = "baml_native_equals";

impl<'db> Backend<'db> {
    /// Define the list of the classes with an `eq` of their own, for a
    /// program that calls what reads it: a count, and for each class the
    /// index of its class object and the entry of its `eq`, sorted by
    /// index.
    pub(super) fn define_equals(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
    ) -> Result<(), CompileError> {
        if !self.compares_values {
            return Ok(());
        }
        let mut classes = Vec::new();
        for (class, entry) in self.hooks.equals.clone() {
            let class = self.class_object(db, super::classes::ClassLike::Class(class))?;
            classes.push((class, entry));
        }
        classes.sort_unstable_by_key(|(id, _)| *id);
        let mut bytes = (classes.len() as u64).to_ne_bytes().to_vec();
        let mut description = DataDescription::new();
        for (id, entry) in classes {
            bytes.extend((id as u64).to_ne_bytes());
            let entry = self.module.declare_func_in_data(entry, &mut description);
            description.write_function_addr(super::table_word(bytes.len()), entry);
            bytes.extend([0; 8]);
        }
        let list = self
            .module
            .declare_data(EQUALS_SYMBOL, Linkage::Export, false, false)
            .map_err(internal)?;
        self.define_data(list, description, bytes)
    }
}

impl<'db> Backend<'db> {
    /// The stub compiled code calls the `async` native `native` through,
    /// which takes `params`: what the native's symbol takes after the
    /// worker.
    pub(super) fn async_stub(
        &mut self,
        native: &'static Native,
        params: &[StubParam],
    ) -> Result<FuncId, CompileError> {
        if let Some((stub, _, _)) = self.hooks.stubs.get(native.path) {
            return Ok(*stub);
        }
        let word = AbiParam::new(WORD);
        // The symbol makes the native's state and returns its address.
        let mut imported = ir::Signature::new(self.call_conv());
        imported.params = vec![word];
        // The stub returns what the native left in its state: a tag and a
        // payload.
        let mut signature = ir::Signature::new(CallConv::Tail);
        signature.returns = vec![word; 2];
        for param in params {
            match param {
                StubParam::Type => {
                    imported.params.push(word);
                    signature.params.push(word);
                }
                StubParam::Value => {
                    imported.params.extend([word; 2]);
                    signature.params.extend([word; 2]);
                }
                StubParam::Exact(taken) => {
                    imported.params.push(*taken);
                    signature.params.extend([word; 2]);
                }
            }
        }
        imported.returns = vec![word];
        let symbol = self
            .module
            .declare_function(native.symbol, Linkage::Import, &imported)
            .map_err(internal)?;
        let stub = self
            .module
            .declare_function(
                &format!("baml_async_{}", native.symbol),
                Linkage::Local,
                &signature,
            )
            .map_err(internal)?;
        self.hooks
            .stubs
            .insert(native.path, (stub, symbol, params.to_vec()));
        Ok(stub)
    }

    /// The stub of the `async` native `native`, which takes no type
    /// arguments, for a row to hold: the classes its declaration names are
    /// described as for a call of it.
    pub(super) fn dispatched_stub(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        native: &'static Native,
    ) -> Result<FuncId, CompileError> {
        let (signature, _) = native.callable().map_err(internal)?;
        self.name_classes(db, native)?;
        let params = signature
            .params
            .iter()
            .map(StubParam::of)
            .collect::<Result<Vec<_>, _>>()?;
        self.async_stub(native, &params)
    }
}

impl StubParam {
    /// What a stub takes for a parameter its native declares as `ty`.
    pub(super) fn of(ty: &crate::natives::Ty) -> Result<Self, CompileError> {
        Ok(match super::native_call::abi_params(ty)?.as_slice() {
            [taken] => Self::Exact(*taken),
            _ => Self::Value,
        })
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
    for (entry, target, values) in backend.hooks.entries.clone() {
        context.func.signature = signature_of(backend, entry);
        entry_body(backend, target, values, &mut context.func, builder_context);
        // It has no name in a trace: the function it runs is the frame a
        // program knows.
        super::define(backend, context, entry, "", clif, disassembly)?;
    }
    let mut stubs: Vec<_> = backend
        .hooks
        .stubs
        .iter()
        .map(|(name, stub)| (*name, stub.clone()))
        .collect();
    stubs.sort_unstable_by_key(|(name, _)| *name);
    // A stub is the native's frame in a trace, under the builtin's name.
    for (path, (stub, native, params)) in stubs {
        context.func.signature = signature_of(backend, stub);
        stub_body(backend, native, &params, &mut context.func, builder_context)?;
        super::define(backend, context, stub, path, clif, disassembly)?;
    }
    Ok(())
}

/// The body of an entry: call `target` with the first `values` of the
/// values the entry was passed, each the tag and the payload it arrived as,
/// and return the tag and the payload it returned, or what it threw with
/// [`layout::THROWN`] set in the tag. The unwinder left the trace of a
/// throw in the worker's scratch word, where the native reads it before
/// anything else uses the word.
fn entry_body(
    backend: &mut Backend<'_>,
    target: FuncId,
    values: usize,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) {
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let passed = builder.block_params(block).to_vec();
    let arguments = &passed[..2 * values];
    let target = backend.function_ref(target, builder.func);
    let caught = builder.create_block();
    builder.append_block_param(caught, PAIR);
    builder.append_block_param(caught, WORD);
    builder.set_cold_block(caught);
    backend
        .unwind
        .catches
        .insert(ENTRY_HANDLER, unwind::Catch::handing_on());
    let (tag, payload) = unwind::try_call(
        &mut builder,
        &unwind::Called::Direct(target),
        arguments,
        &[unwind::Edge {
            tag: Some(ENTRY_HANDLER),
            landing: caught,
            reads_context: false,
        }],
    );
    builder.ins().return_(&[tag, payload]);

    builder.switch_to_block(caught);
    let error = builder.block_params(caught)[0];
    let (tag, payload) = builder.ins().isplit(error);
    let tag = builder.ins().bor_imm(tag, super::imm(layout::THROWN));
    builder.ins().return_(&[tag, payload]);
    builder.seal_all_blocks();
    builder.finalize();
}

/// The body of a stub: start `native` with the worker and what it takes
/// of the stub's own arguments, which are `params`, and poll it until it
/// is done, doing here what it stops for.
///
/// The worker is read from the pinned register for every call: after a
/// stop it is the one the task went on under. What the stub keeps across a
/// stop is the address of the native's state, which is no heap object, so
/// its frame holds no root.
fn stub_body(
    backend: &mut Backend<'_>,
    native: FuncId,
    params: &[StubParam],
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) -> Result<(), CompileError> {
    let rethrow = backend.runtime(Runtime::Rethrow)?;
    let poll = backend.runtime(Runtime::NativePoll)?;
    let wait = backend.runtime(Runtime::IoWait)?;
    // An entry a native calls, with as many words as a native passes.
    let mut entry = ir::Signature::new(backend.call_conv());
    entry.params = vec![AbiParam::new(WORD); layout::NATIVE_CALL_ARGS];
    entry.returns = vec![AbiParam::new(WORD); 2];

    let mut builder = FunctionBuilder::new(function, builder_context);
    let start = builder.create_block();
    builder.append_block_params_for_function_params(start);
    // The tag and the payload of what the entry called last returned or
    // threw: both zero before the first poll and after a wait.
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
    let mut passed = builder.block_params(start).to_vec().into_iter();
    let mut next = || passed.next().expect("a stub is passed what it takes");
    for param in params {
        match param {
            StubParam::Type => arguments.push(next()),
            StubParam::Value => arguments.extend([next(), next()]),
            StubParam::Exact(taken) => {
                let (_, payload) = (next(), next());
                arguments.push(match taken.value_type {
                    ir::types::F64 => {
                        builder
                            .ins()
                            .bitcast(ir::types::F64, MemFlags::new(), payload)
                    }
                    ir::types::I8 => builder.ins().ireduce(ir::types::I8, payload),
                    _ => payload,
                });
            }
        }
    }
    let native = backend.function_ref(native, builder.func);
    let started = builder.ins().call(native, &arguments);
    let state = builder.inst_results(started)[0];
    let zero = builder.ins().iconst(WORD, 0);
    builder.ins().jump(polling, &[zero.into(), zero.into()]);

    builder.switch_to_block(polling);
    let (tag, payload) = {
        let last = builder.block_params(polling);
        (last[0], last[1])
    };
    let worker = builder.ins().get_pinned_reg(WORD);
    let poll = backend.function_ref(poll, builder.func);
    let polled = builder.ins().call(poll, &[worker, state, tag, payload]);
    let (step, word) = {
        let results = builder.inst_results(polled);
        (results[0], results[1])
    };
    builder.ins().brif(step, stopped, &[], done, &[]);

    // The value a native that is done, or that threw, left in its state:
    // read before anything else is called, which may free the state.
    let left = |builder: &mut FunctionBuilder<'_>| {
        let word = |builder: &mut FunctionBuilder<'_>, index: usize| {
            builder.ins().load(
                WORD,
                MemFlags::trusted(),
                state,
                offset(layout::NATIVE_STATE_ARGS + index * layout::WORD),
            )
        };
        (word(builder, 0), word(builder, 1))
    };

    builder.switch_to_block(done);
    let (tag, payload) = left(&mut builder);
    builder.ins().return_(&[tag, payload]);

    builder.switch_to_block(stopped);
    let is_throw = builder
        .ins()
        .icmp_imm(IntCC::Equal, step, super::imm(layout::STEP_THREW));
    builder.ins().brif(is_throw, raise, &[], asked, &[]);

    builder.switch_to_block(raise);
    let (tag, payload) = left(&mut builder);
    let worker = builder.ins().get_pinned_reg(WORD);
    let trace = builder.ins().load(
        WORD,
        mutator_access(),
        worker,
        offset(layout::MUTATOR_SCRATCH),
    );
    let rethrow = backend.function_ref(rethrow, builder.func);
    builder.ins().call(rethrow, &[tag, payload, trace]);
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
    let (tag, payload) = {
        let results = builder.inst_results(called);
        (results[0], results[1])
    };
    builder.ins().jump(polling, &[tag.into(), payload.into()]);

    // Whatever is none of the others is a wait.
    builder.switch_to_block(waiting);
    let wait = backend.function_ref(wait, builder.func);
    builder.ins().call(wait, &[word]);
    builder.ins().jump(polling, &[zero.into(), zero.into()]);

    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}
