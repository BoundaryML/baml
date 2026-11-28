//! The library form's compiled code: what a host program calls through the
//! runtime's `baml_lib_*` functions (`baml_native_rt/src/sched/host.rs`).
//!
//! For each exported function, an entry with the C convention that takes
//! the address of its arguments, a tag and a payload each, and returns the
//! function's value the same way, with [`layout::THROWN`] in the tag when
//! it threw: the hook natives call (`hooks.rs`) for any number of values.
//! Its handler takes every error, so nothing unwinds past it.
//!
//! A task the host starts begins in the host shim, which asks the runtime
//! for its entry and arguments, calls the entry and hands the runtime what
//! it returned. It is compiled code, so that no Rust frame is under a task
//! that parks and every frame has a stack map.
//!
//! The table of exports is the library's interface (`interface.rs`) as the
//! runtime reads it:
//!
//! ```text
//! +0   u32  version (2)
//! +4   u32  how many functions
//! +8   u64  the interface hash
//! +16  u32  how many classes and enums
//! +20  u32  where their records are
//! +24  u32  the interface as JSON (`native_pack --lib --describe`):
//!           offset, then +28 length
//! +32  a record per function, 56 bytes:
//!      +0  u64  the entry's address
//!      +8  u64  the signature's hash
//!      +16 u32  arity
//!      +20 u32  name: offset, then +24 length
//!      +28 u32  signature text: offset, then +32 length
//!      +36 u32  the result's description
//!      +40 u32  the throws type's description, zero for none
//!      +44 u32  where its parameters' records are, 8 bytes each: the
//!               description, then flags (1: it has a default)
//!      +48 u32  how many type parameters it has, zero for now: their type
//!               arguments are to come first in a call's values
//!      +52 u32  zero
//!      a record per class or enum, 32 bytes:
//!      +0  u64  its class object's address
//!      +8  u32  name: offset, then +12 length
//!      +16 u32  kind: 0 a class, 1 an enum
//!      +20 u32  its description
//!      +24 u32  how many fields or variants
//!      +28 u32  how many type parameters it has, zero for now: their
//!               type arguments are to come first in an instance's values
//!      then the parameters' records and the texts
//! ```
//!
//! Offsets are from the table's start. A description is named by its
//! offset in the section of descriptions (`json_layout.rs`), whose records
//! give a class's fields with their slots and an enum's variants.

use std::collections::HashMap;

use cranelift_codegen::{
    Context,
    ir::{self, AbiParam, InstBuilder, MemFlags},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Linkage, Module};

use super::{
    Backend, Declared, Library, Named, PAIR, Runtime, WORD, classes::ClassLike, imm, internal,
    unwind,
};
use crate::{
    CompileError,
    interface::{Reached, TypeKind},
    layout,
    program::{Entry, FnKey},
};

/// The symbol of the shim every task the host starts begins in.
const HOST_SHIM: &str = "baml_native_host_shim";
/// The symbol of the table of exports.
const EXPORTS: &str = "baml_native_exports";
/// The version of its format, which the runtime checks.
const EXPORTS_VERSION: u32 = 2;
const HEADER: usize = 32;
const FUNCTION_RECORD: usize = 56;
const TYPE_RECORD: usize = 32;
const PARAM_RECORD: usize = 8;

fn word(value: usize) -> Result<u32, CompileError> {
    u32::try_from(value).map_err(internal)
}

/// Compile an entry for each export of `library`, the host shim and the
/// table of exports. Every class and enum the interface reaches gets a
/// class object and a description.
#[expect(clippy::too_many_arguments, reason = "what every define takes")]
pub(super) fn define_library<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    backend: &mut Backend<'db>,
    declared: &HashMap<FnKey<'db>, Declared<'db>>,
    library: &Library<'_, 'db>,
    context: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    clif: &mut Option<String>,
    disassembly: &mut Option<String>,
) -> Result<(), CompileError> {
    let mut entry_signature = ir::Signature::new(backend.call_conv());
    entry_signature.params.push(AbiParam::new(WORD));
    entry_signature.returns = vec![AbiParam::new(WORD); 2];

    let interface = library.interface;
    let mut entries = Vec::new();
    for (index, function) in library.exports.iter().enumerate() {
        let target = declared
            .get(&FnKey::new(*function, Entry::Direct))
            .ok_or_else(|| internal("an export is part of the program"))?;
        if !target.type_args.is_empty() {
            return Err(CompileError::NotExportable(
                interface.functions[index].name.clone(),
                "it takes type arguments".to_owned(),
            ));
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
        entries.push(entry);
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

    // Descriptions, and the class object of every class and enum reached.
    let describe = |backend: &mut Backend<'db>, ty: &baml_compiler2_mir::RuntimeTy| {
        backend
            .json
            .describe(db, ty)
            .map_err(|blocker| internal(format!("an exported type has a description: {blocker}")))
    };
    let mut functions = Vec::new();
    for detail in &library.detail.functions {
        let params = detail
            .params
            .iter()
            .map(|ty| describe(backend, ty))
            .collect::<Result<Vec<_>, _>>()?;
        let result = describe(backend, &detail.result)?;
        let throws = match &detail.throws {
            Some(ty) => describe(backend, ty)?,
            None => 0,
        };
        functions.push((params, result, throws));
    }
    let mut types = Vec::new();
    for reached in &library.detail.types {
        let (what, ty) = match reached {
            Reached::Class(class, ty) => (ClassLike::Class(*class), ty),
            Reached::Enum(enum_ref, ty) => (ClassLike::Enum(*enum_ref), ty),
        };
        let described = describe(backend, ty)?;
        let class = backend.class_object(db, what)?;
        types.push((backend.class_data(class), described));
    }

    // The table: the header, the records, then the parameters and the texts.
    let mut description = DataDescription::new();
    let mut bytes: Vec<u8> = Vec::new();
    let types_at = HEADER + FUNCTION_RECORD * interface.functions.len();
    let params_at = types_at + TYPE_RECORD * interface.types.len();
    let param_count: usize = interface.functions.iter().map(|f| f.params.len()).sum();
    let mut text_at = params_at + PARAM_RECORD * param_count;
    let mut texts: Vec<u8> = Vec::new();
    let mut text = |piece: &str, texts: &mut Vec<u8>| -> Result<[u32; 2], CompileError> {
        let at = [word(text_at)?, word(piece.len())?];
        texts.extend_from_slice(piece.as_bytes());
        text_at += piece.len();
        Ok(at)
    };
    let push = |bytes: &mut Vec<u8>, words: &[u32]| {
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    };
    push(
        &mut bytes,
        &[EXPORTS_VERSION, word(interface.functions.len())?],
    );
    bytes.extend_from_slice(&interface.hash.to_le_bytes());
    let json = text(&interface.to_json(), &mut texts)?;
    push(
        &mut bytes,
        &[
            word(interface.types.len())?,
            word(types_at)?,
            json[0],
            json[1],
        ],
    );
    let mut param_records: Vec<u8> = Vec::new();
    let mut next_param = params_at;
    for ((function, entry), (params, result, throws)) in
        interface.functions.iter().zip(&entries).zip(&functions)
    {
        let entry = backend
            .module
            .declare_func_in_data(*entry, &mut description);
        description.write_function_addr(word(bytes.len())?, entry);
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        bytes.extend_from_slice(&function.hash.to_le_bytes());
        let name = text(&function.name, &mut texts)?;
        let signature = text(&function.signature, &mut texts)?;
        push(
            &mut bytes,
            &[
                word(params.len())?,
                name[0],
                name[1],
                signature[0],
                signature[1],
                *result,
                *throws,
                word(next_param)?,
                0,
                0,
            ],
        );
        for (described, param) in params.iter().zip(&function.params) {
            push(
                &mut param_records,
                &[*described, u32::from(param.has_default)],
            );
        }
        next_param += PARAM_RECORD * params.len();
    }
    for (ty, (class, described)) in interface.types.iter().zip(&types) {
        let class = backend
            .module
            .declare_data_in_data(*class, &mut description);
        description.write_data_addr(word(bytes.len())?, class, 0);
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        let name = text(&ty.name, &mut texts)?;
        let (kind, count) = match &ty.kind {
            TypeKind::Class(fields) => (0, fields.len()),
            TypeKind::Enum(variants) => (1, variants.len()),
        };
        push(
            &mut bytes,
            &[name[0], name[1], kind, *described, word(count)?, 0],
        );
    }
    debug_assert_eq!(bytes.len(), params_at);
    bytes.extend_from_slice(&param_records);
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
