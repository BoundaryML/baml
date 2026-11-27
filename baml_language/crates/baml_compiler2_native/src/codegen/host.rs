//! The library form's compiled code: what a host program calls through the
//! runtime's `baml_lib_*` functions (`baml_native_rt/src/sched/host.rs`).
//!
//! A spike. For each exported function, an entry with the C convention
//! that takes the address of its arguments, a tag and a payload each, and
//! returns the function's value the same way, with [`layout::THROWN`] in
//! the tag when it threw: the hook natives call (`hooks.rs`) for any number
//! of values. Its handler takes every error, so nothing unwinds past it.
//!
//! A task the host starts begins in the host shim, which asks the runtime
//! for its entry and arguments, calls the entry and hands the runtime what
//! it returned. It is compiled code, so that no Rust frame is under a task
//! that parks and every frame has a stack map.
//!
//! The table of exports tells the runtime, and through it the host, each
//! export's name, entry, arity and signature, with a hash of each signature
//! and one of the whole interface, for bindings to check they match.

use std::collections::HashMap;

use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use cranelift_codegen::{
    Context,
    ir::{self, AbiParam, InstBuilder, MemFlags},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Linkage, Module};

use super::{Backend, Declared, Named, PAIR, Runtime, WORD, imm, internal, unwind};
use crate::{
    CompileError, layout,
    program::{Entry, FnKey},
};

/// The symbol of the shim every task the host starts begins in.
const HOST_SHIM: &str = "baml_native_host_shim";
/// The symbol of the table of exports.
const EXPORTS: &str = "baml_native_exports";
/// The version of its format, which the runtime checks.
const EXPORTS_VERSION: u32 = 1;
/// The bytes of one record of it.
const RECORD: usize = 40;

/// FNV-1a, 64 bits: a hash both sides can compute with no dependency. It
/// tells a stale binding from a fresh one; it is not a defense.
fn fnv(bytes: &[u8], mut hash: u64) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

/// The signature of an exported function as text: its name, each
/// parameter's name and type, and its result type.
fn signature_text(
    db: &dyn baml_compiler2_mir::Db,
    function: FunctionLoc<'_>,
    name: &str,
) -> String {
    let signature =
        baml_compiler2_hir_ty::callable::callable_signature(db, DeclRef::Source(function));
    let view = baml_compiler2_hir_ty::render::Viewpoint::canonical(db);
    let params: Vec<String> = signature
        .params
        .iter()
        .map(|param| {
            let name = param
                .name
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            let default = match param.mode {
                baml_type::FunctionParamMode::Required => "",
                baml_type::FunctionParamMode::Optional => " = default",
            };
            format!("{name}: {}{default}", view.labelled_text(&param.ty))
        })
        .collect();
    format!(
        "{name}({}) -> {}",
        params.join(", "),
        view.labelled_text(&signature.return_type)
    )
}

/// Compile an entry for each of `exports`, the host shim and the table of
/// exports.
#[expect(clippy::too_many_arguments, reason = "what every define takes")]
pub(super) fn define_library<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    backend: &mut Backend<'db>,
    declared: &HashMap<FnKey<'db>, Declared<'db>>,
    exports: &[FunctionLoc<'db>],
    context: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    clif: &mut Option<String>,
    disassembly: &mut Option<String>,
) -> Result<(), CompileError> {
    let mut entry_signature = ir::Signature::new(backend.call_conv());
    entry_signature.params.push(AbiParam::new(WORD));
    entry_signature.returns = vec![AbiParam::new(WORD); 2];

    // (entry, name, signature text, arity)
    let mut records = Vec::new();
    for (index, function) in exports.iter().enumerate() {
        let name = baml_compiler2_hir::item_data::function_data(db, *function)
            .name
            .to_string();
        let target = declared
            .get(&FnKey::new(*function, Entry::Direct))
            .ok_or_else(|| internal("an export is part of the program"))?;
        if !target.type_args.is_empty() {
            return Err(CompileError::ExportIsGeneric(name));
        }
        let entry = backend
            .module
            .declare_function(
                &format!("baml_export_{index}"),
                Linkage::Local,
                &entry_signature,
            )
            .map_err(internal)?;
        context.func.signature = entry_signature.clone();
        export_body(backend, target, &mut context.func, builder_context);
        super::define(
            backend,
            context,
            entry,
            &Named::Listed("an entry a host calls"),
            clif,
            disassembly,
        )?;
        let text = signature_text(db, *function, &name);
        records.push((entry, name, text, target.signature.params.len()));
    }

    let mut shim_signature = ir::Signature::new(backend.call_conv());
    shim_signature.params.push(AbiParam::new(WORD));
    let shim = backend
        .module
        .declare_function(HOST_SHIM, Linkage::Export, &shim_signature)
        .map_err(internal)?;
    context.func.signature = shim_signature;
    shim_body(
        backend,
        &entry_signature,
        &mut context.func,
        builder_context,
    )?;
    super::define(
        backend,
        context,
        shim,
        &Named::Listed(HOST_SHIM),
        clif,
        disassembly,
    )?;

    // The table: a header, a record per export, then the texts.
    let mut description = DataDescription::new();
    let mut bytes: Vec<u8> = Vec::new();
    let count = u32::try_from(records.len()).map_err(internal)?;
    let interface = records.iter().fold(FNV_START, |hash, (_, _, text, _)| {
        fnv(&[0], fnv(text.as_bytes(), hash))
    });
    bytes.extend_from_slice(&EXPORTS_VERSION.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&interface.to_le_bytes());
    let mut text_at = bytes.len() + RECORD * records.len();
    let mut texts: Vec<u8> = Vec::new();
    for (entry, name, text, arity) in &records {
        let entry = backend
            .module
            .declare_func_in_data(*entry, &mut description);
        description.write_function_addr(u32::try_from(bytes.len()).map_err(internal)?, entry);
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        bytes.extend_from_slice(&fnv(text.as_bytes(), FNV_START).to_le_bytes());
        bytes.extend_from_slice(&u32::try_from(*arity).map_err(internal)?.to_le_bytes());
        for piece in [name, text] {
            let offset = u32::try_from(text_at).map_err(internal)?;
            let length = u32::try_from(piece.len()).map_err(internal)?;
            bytes.extend_from_slice(&offset.to_le_bytes());
            bytes.extend_from_slice(&length.to_le_bytes());
            texts.extend_from_slice(piece.as_bytes());
            text_at += piece.len();
        }
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.extend_from_slice(&texts);
    let id = backend
        .module
        .declare_data(EXPORTS, Linkage::Export, false, false)
        .map_err(internal)?;
    backend.define_data(id, description, bytes)
}

/// The body of an export's entry: read `2 * arity` words from the address
/// it is passed, call the function with them under a handler that takes
/// every error, and return its value, or the error with `THROWN` in the
/// tag. Nothing can collect between the reads and the call.
fn export_body(
    backend: &mut Backend<'_>,
    target: &Declared<'_>,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) {
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let args = builder.block_params(block)[0];
    let words: Vec<ir::Value> = (0..2 * target.signature.params.len())
        .map(|word| {
            builder.ins().load(
                WORD,
                MemFlags::trusted(),
                args,
                i32::try_from(word * layout::WORD).expect("few arguments"),
            )
        })
        .collect();
    let callee = backend.function_ref(target.id, builder.func);
    let caught = builder.create_block();
    builder.append_block_param(caught, PAIR);
    builder.append_block_param(caught, WORD);
    builder.set_cold_block(caught);
    let (tag, payload) = unwind::try_call(
        &mut builder,
        &unwind::Called::Direct(callee),
        &words,
        &[unwind::Edge {
            tag: None,
            landing: caught,
            reads_context: true,
        }],
    );
    builder.ins().return_(&[tag, payload]);

    builder.switch_to_block(caught);
    let error = builder.block_params(caught)[0];
    let (tag, payload) = builder.ins().isplit(error);
    let tag = builder.ins().bor_imm(tag, imm(layout::THROWN));
    builder.ins().return_(&[tag, payload]);
    builder.seal_all_blocks();
    builder.finalize();
}

/// The body of the host shim: ask the runtime for the entry and the
/// arguments of the task's call, call the entry, and hand the runtime what
/// it returned.
fn shim_body(
    backend: &mut Backend<'_>,
    entry_signature: &ir::Signature,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) -> Result<(), CompileError> {
    let start = backend.runtime(Runtime::HostStart)?;
    let finish = backend.runtime(Runtime::HostFinish)?;
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let worker = builder.block_params(block)[0];
    let start = backend.function_ref(start, builder.func);
    let call = builder.ins().call(start, &[worker]);
    let (entry, args) = {
        let results = builder.inst_results(call);
        (results[0], results[1])
    };
    let signature = builder.import_signature(entry_signature.clone());
    let call = builder.ins().call_indirect(signature, entry, &[args]);
    let (tag, payload) = {
        let results = builder.inst_results(call);
        (results[0], results[1])
    };
    // The task may have parked since it started, and gone on under another
    // worker.
    let worker = builder.ins().get_pinned_reg(WORD);
    let finish = backend.function_ref(finish, builder.func);
    builder.ins().call(finish, &[worker, tag, payload]);
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}
