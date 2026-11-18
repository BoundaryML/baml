//! MIR to Cranelift IR to a native object file.
//!
//! Every function of the program becomes one object-local function; they
//! call each other with Cranelift's tail convention and the runtime with the
//! platform's C convention. Integer arithmetic wraps in two's complement. Values have no tags: an `int` is an
//! `i64`, a `float` an `f64`, a `bool` or `null` an `i8`, and a string, class
//! instance or array a pointer to a heap object laid out as
//! [`crate::layout`] says (zero for the `null` of an optional).
//!
//! One register is set aside for the whole program: it holds the address of
//! the running thread's mutator, whose first two words drive inline
//! allocation.
//!
//! Every call is a point where the collector may run and move objects.
//! References live across a call are spilled to frame slots that a stack map
//! names and are reloaded afterwards; the maps of all functions are emitted
//! as one table, [`STACK_MAPS_SYMBOL`].
//!
//! The object exports one function, [`ENTRY_SYMBOL`], which calls the entry
//! function and prints its result through the runtime.

use std::{collections::HashMap, fmt::Write as _};

use baml_compiler2_hir::loc::DeclRef;
use baml_compiler2_hir_ty::extern_loc::ClassRef;
use baml_compiler2_mir::{
    AggregateKind, BinOp, BlockId, CellId, Constant, Local, MirFunction, MirFunctionBody, Operand,
    Place, Rvalue, ShortCircuitKind, StatementKind, SwitchKey, Terminator, UnaryOp,
};
use cranelift_codegen::{
    Context,
    ir::{
        self, AbiParam, AliasRegion, InstBuilder, MemFlags, StackSlotData, StackSlotKind, TrapCode,
        condcodes::{FloatCC, IntCC},
        types,
    },
    isa::CallConv,
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Switch, Variable};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::{
    CompileError, CompileOptions,
    check::{Callee, Intrinsic, body_of, direct_callee, narrow_test_wants_reference, signature},
    flow::{Flow, ProvenIndexes},
    layout,
    loops::{ArrayLoops, operand_local},
    program::{Entry, FnKey, Program},
    types::{
        BinaryMode, Body, NativeTy, Signature, binary_typing, class_layout, field_slots,
        is_comparison, layout_slots, local_ty, narrowed, operand_ty, place_local, place_ty,
        unary_typing,
    },
};

/// The symbol the runtime's process entry calls.
pub(crate) const ENTRY_SYMBOL: &str = "baml_native_entry";
/// The symbol of the table of stack maps the collector reads.
pub(crate) const STACK_MAPS_SYMBOL: &str = "baml_native_stack_maps";
/// The symbol of the table of types the collector reads.
pub(crate) const TYPES_SYMBOL: &str = "baml_native_types";

/// The trap after a call into a runtime function that does not return.
const TRAP_AFTER_PANIC: TrapCode = TrapCode::unwrap_user(1);
/// The trap for a MIR `Unreachable` terminator.
const TRAP_UNREACHABLE: TrapCode = TrapCode::unwrap_user(2);

/// Shift counts at or above this give a result that no longer depends on the
/// count.
const INT_BITS: i64 = 64;

/// The type of a pointer and of a heap slot.
const WORD: ir::Type = types::I64;

pub(crate) struct Output {
    pub(crate) object: Vec<u8>,
    pub(crate) clif: Option<String>,
    pub(crate) disassembly: Option<String>,
}

fn internal(error: impl std::fmt::Display) -> CompileError {
    CompileError::Codegen(error.to_string())
}

fn clif_type(ty: &NativeTy<'_>) -> ir::Type {
    if ty.is_pair() {
        return types::I128;
    }
    match ty {
        NativeTy::Int | NativeTy::Enum => types::I64,
        NativeTy::Float => types::F64,
        NativeTy::Bool | NativeTy::Null => types::I8,
        NativeTy::String
        | NativeTy::Class(_)
        | NativeTy::Array(_)
        | NativeTy::Function(_)
        | NativeTy::Cell(_)
        | NativeTy::Optional(_) => WORD,
    }
}

/// A byte offset as the immediate of a load or store.
fn offset(bytes: usize) -> i32 {
    i32::try_from(bytes).expect("object offsets fit 32 bits")
}

fn imm(bytes: usize) -> i64 {
    i64::try_from(bytes).expect("object sizes fit 63 bits")
}

/// Loads and stores of object memory never fault and are always aligned.
/// They fall into three sets of words that never overlap, which lets a load
/// from one survive a store to another: the fields of instances and the
/// elements of arrays; ...
fn slot_access() -> MemFlags {
    MemFlags::trusted().with_alias_region(Some(AliasRegion::Heap))
}

/// ... the length and storage of an array, the capacity of its storage and
/// the lengths of a string; ...
/// A word of a closure, written once where the closure is made and never
/// by the code that reads it.
fn closure_access() -> MemFlags {
    slot_access().with_readonly()
}

fn array_access() -> MemFlags {
    MemFlags::trusted().with_alias_region(Some(AliasRegion::Table))
}

/// ... and the mutator.
fn mutator_access() -> MemFlags {
    MemFlags::trusted().with_alias_region(Some(AliasRegion::Vmctx))
}

/// A function of the runtime library, called by symbol.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Runtime {
    PrintInt,
    PrintFloat,
    PrintBool,
    PrintString,
    PanicDivisionByZero,
    PanicNegativeShift,
    PanicIndexOutOfBounds,
    FloatRem,
    StackSlow,
    PollSlow,
    AllocSlow,
    ArrayNew,
    ArrayPushSlow,
    StringConcat,
    StringEq,
    StringCmp,
    StringFromInt,
    StringFromFloat,
}

impl Runtime {
    fn symbol(self) -> &'static str {
        match self {
            Self::PrintInt => "baml_rt_print_int",
            Self::PrintFloat => "baml_rt_print_float",
            Self::PrintBool => "baml_rt_print_bool",
            Self::PrintString => "baml_rt_print_string",
            Self::PanicDivisionByZero => "baml_rt_panic_division_by_zero",
            Self::PanicNegativeShift => "baml_rt_panic_negative_shift",
            Self::PanicIndexOutOfBounds => "baml_rt_panic_index_out_of_bounds",
            Self::FloatRem => "baml_rt_float_rem",
            Self::StackSlow => "baml_rt_stack_slow",
            Self::PollSlow => "baml_rt_poll_slow",
            Self::AllocSlow => "baml_rt_alloc_slow",
            Self::ArrayNew => "baml_rt_array_new",
            Self::ArrayPushSlow => "baml_rt_array_push_slow",
            Self::StringConcat => "baml_rt_string_concat",
            Self::StringEq => "baml_rt_string_eq",
            Self::StringCmp => "baml_rt_string_cmp",
            Self::StringFromInt => "baml_rt_string_from_int",
            Self::StringFromFloat => "baml_rt_string_from_float",
        }
    }

    fn signature(self, call_conv: CallConv) -> ir::Signature {
        let word = AbiParam::new(WORD);
        let float = AbiParam::new(types::F64);
        // The C ABI has the caller extend an argument narrower than a
        // register.
        let byte = AbiParam::new(types::I8).uext();
        let (params, returns) = match self {
            Self::PrintInt
            | Self::PrintString
            | Self::PanicDivisionByZero
            | Self::PollSlow
            | Self::PanicNegativeShift => (vec![word], vec![]),
            Self::StackSlow => (vec![], vec![]),
            Self::PrintFloat => (vec![float], vec![]),
            Self::PrintBool => (vec![byte], vec![]),
            Self::PanicIndexOutOfBounds => (vec![word, word], vec![]),
            Self::FloatRem => (vec![float, float], vec![float]),
            Self::AllocSlow | Self::StringFromInt => (vec![word, word], vec![word]),
            Self::StringFromFloat => (vec![word, float], vec![word]),
            Self::ArrayNew | Self::StringConcat => (vec![word, word, word], vec![word]),
            Self::ArrayPushSlow => (vec![word, word, word], vec![]),
            Self::StringEq => (vec![word, word], vec![AbiParam::new(types::I8)]),
            Self::StringCmp => (vec![word, word], vec![word]),
        };
        let mut signature = ir::Signature::new(call_conv);
        signature.params = params;
        signature.returns = returns;
        signature
    }
}

/// A runtime panic and the values its message is built from.
#[derive(Clone, Copy)]
enum Panic {
    DivisionByZero(ir::Value),
    NegativeShift(ir::Value),
    IndexOutOfBounds(ir::Value, ir::Value),
}

/// The stack maps of one function: for each call, the offset of its return
/// address in the function and the offsets from the stack pointer of the
/// slots holding references across it.
type FunctionStackMaps = Vec<(u32, Vec<u32>)>;

struct Backend<'db> {
    module: ObjectModule,
    runtime: HashMap<Runtime, FuncId>,
    /// The program's type table so far: the fixed types, then one entry per
    /// class met.
    types: Vec<u8>,
    /// The type id of each class met: the offset of its entry in `types`.
    class_types: HashMap<ClassRef<'db>, usize>,
    /// The type id of closures, by which of their captured words are
    /// references.
    closure_types: HashMap<Vec<bool>, usize>,
    /// The closure standing for each function used as a value without
    /// capturing anything.
    function_values: HashMap<FuncId, DataId>,
    strings: HashMap<String, DataId>,
    stack_maps: Vec<(FuncId, FunctionStackMaps)>,
}

impl<'db> Backend<'db> {
    /// A backend for the machine the compiler runs on, using every CPU
    /// feature that machine has.
    fn new() -> Result<Self, CompileError> {
        let mut flags = settings::builder();
        flags.set("opt_level", "speed").map_err(internal)?;
        flags.set("is_pic", "true").map_err(internal)?;
        // The mutator's address lives in the pinned register.
        flags.set("enable_pinned_reg", "true").map_err(internal)?;
        // The collector walks the chain of frame records.
        flags
            .set("preserve_frame_pointers", "true")
            .map_err(internal)?;
        let mut isa = cranelift_native::builder().map_err(internal)?;
        if cfg!(all(target_arch = "aarch64", target_vendor = "apple")) {
            // The host defaults sign return addresses, which the platform's
            // ABI for ordinary (not arm64e) code does not ask for, and which
            // the collector's stack walk would have to strip.
            isa.set("sign_return_address", "false").map_err(internal)?;
        }
        let isa = isa.finish(settings::Flags::new(flags)).map_err(internal)?;
        let builder =
            ObjectBuilder::new(isa, "baml_program", default_libcall_names()).map_err(internal)?;
        Ok(Self {
            module: ObjectModule::new(builder),
            runtime: HashMap::new(),
            types: layout::RUNTIME_TYPES
                .iter()
                .flat_map(|word| word.to_ne_bytes())
                .collect(),
            class_types: HashMap::new(),
            closure_types: HashMap::new(),
            function_values: HashMap::new(),
            strings: HashMap::new(),
            stack_maps: Vec::new(),
        })
    }

    fn call_conv(&self) -> CallConv {
        self.module.isa().default_call_conv()
    }

    /// The signature of a BAML function. Functions call each other with the
    /// tail convention, the one Cranelift has both tail calls and exception
    /// edges for on every target.
    ///
    /// A function entered through a value takes the closure first; one
    /// entered as a bound method finds its receiver there.
    fn clif_signature(signature: &Signature<'_>, entry: Entry) -> ir::Signature {
        let mut clif = ir::Signature::new(CallConv::Tail);
        let (closure, receiver) = match entry {
            Entry::Direct => (false, 0),
            Entry::Value => (true, 0),
            Entry::Bound => (true, 1),
        };
        clif.params = closure
            .then(|| AbiParam::new(WORD))
            .into_iter()
            .chain(
                signature
                    .params
                    .iter()
                    .skip(receiver)
                    .map(|ty| AbiParam::new(clif_type(ty))),
            )
            .collect();
        clif.returns = vec![AbiParam::new(clif_type(&signature.result))];
        clif
    }

    fn runtime(&mut self, function: Runtime) -> Result<FuncId, CompileError> {
        if let Some(id) = self.runtime.get(&function) {
            return Ok(*id);
        }
        let signature = function.signature(self.call_conv());
        let id = self
            .module
            .declare_function(function.symbol(), Linkage::Import, &signature)
            .map_err(internal)?;
        self.runtime.insert(function, id);
        Ok(id)
    }

    /// Read-only data of the object, aligned to a word.
    fn define_data(
        &mut self,
        id: DataId,
        mut description: DataDescription,
        bytes: Vec<u8>,
    ) -> Result<(), CompileError> {
        description.define(bytes.into_boxed_slice());
        description.set_align(layout::WORD as u64);
        self.module.define_data(id, &description).map_err(internal)
    }

    /// The type id of a class, adding its entry to the type table the first
    /// time: what the collector needs to copy and scan an instance.
    fn class_type(&mut self, class: ClassRef<'db>, fields: &[NativeTy<'db>]) -> usize {
        if let Some(id) = self.class_types.get(&class) {
            return *id;
        }
        let references: Vec<usize> = fields
            .iter()
            .zip(field_slots(fields))
            .filter(|(field, _)| field.is_ref())
            .map(|(_, slot)| layout::field_offset(slot))
            .collect();
        let id = self.fixed_type(layout::instance_size(layout_slots(fields)), &references);
        self.class_types.insert(class, id);
        id
    }

    /// A new type of objects of `size` bytes with references at these byte
    /// offsets.
    fn fixed_type(&mut self, size: usize, references: &[usize]) -> usize {
        let id = self.types.len();
        let entry = [
            layout::KIND_FIXED,
            table_word(size),
            table_word(references.len()),
            0,
        ];
        for word in entry
            .into_iter()
            .chain(references.iter().map(|offset| table_word(*offset)))
        {
            self.types.extend_from_slice(&word.to_ne_bytes());
        }
        // Entries start on a word, so that an id leaves the low header bits
        // to the collector.
        self.types.resize(layout::words(self.types.len()), 0);
        id
    }

    /// The type id of closures whose captured words are references where
    /// `references` says so.
    fn closure_type(&mut self, references: &[bool]) -> usize {
        if let Some(id) = self.closure_types.get(references) {
            return *id;
        }
        let offsets: Vec<usize> = references
            .iter()
            .enumerate()
            .filter(|(_, is_ref)| **is_ref)
            .map(|(index, _)| layout::CLOSURE_CAPTURES + index * layout::WORD)
            .collect();
        let size = layout::CLOSURE_CAPTURES + references.len() * layout::WORD;
        let id = self.fixed_type(size, &offsets);
        self.closure_types.insert(references.to_vec(), id);
        id
    }

    /// The closure that stands for a function used as a value: an object in
    /// the executable's data that captures nothing.
    fn function_value(&mut self, function: FuncId) -> Result<DataId, CompileError> {
        if let Some(id) = self.function_values.get(&function) {
            return Ok(*id);
        }
        let mut bytes = vec![0; layout::CLOSURE_CAPTURES];
        bytes[..layout::HEADER].copy_from_slice(&self.closure_type(&[]).to_ne_bytes());
        let mut description = DataDescription::new();
        let code = self.module.declare_func_in_data(function, &mut description);
        description.write_function_addr(table_word(layout::CLOSURE_CODE), code);
        let id = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        self.define_data(id, description, bytes)?;
        self.function_values.insert(function, id);
        Ok(id)
    }

    /// Emit the type table.
    fn define_types(&mut self) -> Result<(), CompileError> {
        let id = self
            .module
            .declare_data(TYPES_SYMBOL, Linkage::Export, false, false)
            .map_err(internal)?;
        let types = std::mem::take(&mut self.types);
        self.define_data(id, DataDescription::new(), types)
    }

    /// A string object in the executable's data. The collector leaves
    /// objects outside the heap alone.
    fn string_constant(&mut self, text: &str) -> Result<DataId, CompileError> {
        if let Some(id) = self.strings.get(text) {
            return Ok(*id);
        }
        let lengths = text.len() | (text.chars().count() << layout::STRING_CHAR_COUNT_SHIFT);
        let mut bytes = vec![0; layout::STRING_TEXT + layout::words(text.len())];
        bytes[..layout::HEADER].copy_from_slice(&layout::TYPE_STRING.to_ne_bytes());
        bytes[layout::STRING_LENGTHS..layout::STRING_TEXT].copy_from_slice(&lengths.to_ne_bytes());
        bytes[layout::STRING_TEXT..layout::STRING_TEXT + text.len()]
            .copy_from_slice(text.as_bytes());

        let id = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        self.define_data(id, DataDescription::new(), bytes)?;
        self.strings.insert(text.to_owned(), id);
        Ok(id)
    }

    /// Emit the table of every function's stack maps, in the format the
    /// runtime decodes: a count of functions, then for each its address, a
    /// count of call sites, and for each site its return address as an
    /// offset into the function, a count of slots, and the slots. Every
    /// entry is one word.
    fn define_stack_maps(&mut self) -> Result<(), CompileError> {
        let mut description = DataDescription::new();
        let mut bytes: Vec<u8> = Vec::new();
        let word = |bytes: &mut Vec<u8>, value: usize| {
            bytes.extend_from_slice(&value.to_ne_bytes());
        };
        let functions: Vec<_> = std::mem::take(&mut self.stack_maps)
            .into_iter()
            .filter(|(_, sites)| !sites.is_empty())
            .collect();
        word(&mut bytes, functions.len());
        for (function, sites) in functions {
            let function = self.module.declare_func_in_data(function, &mut description);
            description
                .write_function_addr(u32::try_from(bytes.len()).map_err(internal)?, function);
            word(&mut bytes, 0);
            word(&mut bytes, sites.len());
            for (return_offset, slots) in sites {
                word(&mut bytes, return_offset as usize);
                word(&mut bytes, slots.len());
                for slot in slots {
                    word(&mut bytes, slot as usize);
                }
            }
        }
        let id = self
            .module
            .declare_data(STACK_MAPS_SYMBOL, Linkage::Export, false, false)
            .map_err(internal)?;
        self.define_data(id, description, bytes)
    }
}

struct Declared<'db> {
    id: FuncId,
    entry: Entry,
    signature: Signature<'db>,
    /// Whether the function checks the stack limit on entry.
    checks_stack: bool,
}

/// The most locals a function that calls no other may have and still skip
/// the stack check. Its frame then holds the frame record, saved registers
/// and at most this many spilled values, well inside the space the limit
/// leaves below it.
const UNCHECKED_LEAF_LOCALS: usize = 12;

/// Whether a function must check the stack limit on entry: it calls
/// another compiled function, or its frame may not be small.
fn checks_stack<'db>(db: &'db dyn baml_compiler2_mir::Db, body: &MirFunctionBody<'db>) -> bool {
    body.locals.len() > UNCHECKED_LEAF_LOCALS
        || body.blocks.iter().any(|block| match &block.terminator {
            Some(Terminator::Call { callee, .. }) => {
                !matches!(direct_callee(db, callee), Ok(Callee::Intrinsic(_)))
            }
            Some(
                Terminator::Goto { .. }
                | Terminator::Branch { .. }
                | Terminator::Switch { .. }
                | Terminator::Return
                | Terminator::Unreachable
                | Terminator::ShortCircuit { .. }
                | Terminator::VirtualCall { .. },
            )
            | None => false,
            Some(_) => true,
        })
}

/// A symbol name that shows the function's display name in a profile or
/// disassembly. `index` keeps it unique.
fn symbol_name(index: usize, display_name: &str) -> String {
    let readable: String = display_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("baml.{readable}.{index}")
}

pub(crate) fn generate<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    program: &Program<'db>,
    options: CompileOptions,
) -> Result<Output, CompileError> {
    let mut backend = Backend::new()?;
    let mut clif = options.emit_clif.then(String::new);
    let mut disassembly = options.emit_disassembly.then(String::new);

    // Declare everything first: calls can go forward and in cycles.
    let mut bodies = Vec::with_capacity(program.functions.len());
    let mut declared: HashMap<FnKey<'db>, Declared<'db>> = HashMap::new();
    for (index, function) in program.functions.iter().enumerate() {
        let mir = function
            .mir
            .ok_or_else(|| internal(format!("`{}` has no MIR", function.name)))?;
        let body = body_of(mir).map_err(internal)?;
        let signature = signature(db, mir, body).map_err(internal)?;
        let id = backend
            .module
            .declare_function(
                &symbol_name(index, &function.name),
                Linkage::Local,
                &Backend::clif_signature(&signature, function.key.entry),
            )
            .map_err(internal)?;
        declared.insert(
            function.key.clone(),
            Declared {
                id,
                entry: function.key.entry,
                signature,
                checks_stack: checks_stack(db, body),
            },
        );
        bodies.push((function, mir, Body::new(db, mir, body, &function.captures)));
    }

    let entry = program
        .functions
        .first()
        .and_then(|function| declared.get(&function.key))
        .ok_or_else(|| internal("a program has an entry function"))?;
    if !entry.signature.params.is_empty() {
        return Err(CompileError::EntryTakesArguments);
    }
    let print = match entry.signature.result {
        NativeTy::Int => Some(Runtime::PrintInt),
        NativeTy::Float => Some(Runtime::PrintFloat),
        NativeTy::Bool => Some(Runtime::PrintBool),
        NativeTy::String => Some(Runtime::PrintString),
        NativeTy::Null => None,
        NativeTy::Enum
        | NativeTy::Class(_)
        | NativeTy::Array(_)
        | NativeTy::Function(_)
        | NativeTy::Cell(_)
        | NativeTy::Optional(_) => {
            return Err(CompileError::EntryResultNotPrintable);
        }
    };

    let mut context = backend.module.make_context();
    let mut builder_context = FunctionBuilderContext::new();
    for (function, mir, body) in bodies {
        let this = &declared[&function.key];
        context.func.signature = Backend::clif_signature(&this.signature, this.entry);
        Translator::new(
            db,
            &mut backend,
            &declared,
            &function.key,
            &body,
            &mut context.func,
            &mut builder_context,
        )
        .function(mir, this.checks_stack)?;
        define(
            &mut backend,
            &mut context,
            this.id,
            &function.name,
            &mut clif,
            &mut disassembly,
        )?;
    }

    let mut entry_signature = ir::Signature::new(backend.call_conv());
    entry_signature.params.push(AbiParam::new(WORD));
    let entry_id = backend
        .module
        .declare_function(ENTRY_SYMBOL, Linkage::Export, &entry_signature)
        .map_err(internal)?;
    context.func.signature = entry_signature;
    entry_shim(
        &mut backend,
        entry,
        print,
        &mut context.func,
        &mut builder_context,
    )?;
    define(
        &mut backend,
        &mut context,
        entry_id,
        ENTRY_SYMBOL,
        &mut clif,
        &mut disassembly,
    )?;
    backend.define_stack_maps()?;
    backend.define_types()?;

    let object = backend.module.finish().emit().map_err(internal)?;
    Ok(Output {
        object,
        clif,
        disassembly,
    })
}

/// Compile the function in `context` to machine code under `id`, and keep
/// its stack maps.
fn define(
    backend: &mut Backend<'_>,
    context: &mut Context,
    id: FuncId,
    name: &str,
    clif: &mut Option<String>,
    disassembly: &mut Option<String>,
) -> Result<(), CompileError> {
    if let Some(text) = clif {
        let _ = writeln!(text, "; {name}\n{}", context.func.display());
    }
    context.set_disasm(disassembly.is_some());
    backend
        .module
        .define_function(id, context)
        .map_err(|error| internal(format!("compiling `{name}`: {error:?}")))?;
    let code = context
        .compiled_code()
        .ok_or_else(|| internal("a defined function has machine code"))?;
    if let Some(text) = disassembly
        && let Some(listing) = &code.vcode
    {
        let _ = writeln!(text, "; {name}\n{listing}");
    }
    let sites = code
        .buffer
        .user_stack_maps()
        .iter()
        .map(|(return_offset, _, map)| {
            (
                *return_offset,
                map.entries().map(|(_, slot)| slot).collect::<Vec<_>>(),
            )
        })
        .filter(|(_, slots)| !slots.is_empty())
        .collect();
    backend.stack_maps.push((id, sites));
    backend.module.clear_context(context);
    Ok(())
}

/// The exported function the runtime calls with the thread's mutator: put it
/// in the pinned register, run the entry function, then print what it
/// returned.
fn entry_shim(
    backend: &mut Backend<'_>,
    entry: &Declared<'_>,
    print: Option<Runtime>,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) -> Result<(), CompileError> {
    let print = print.map(|print| backend.runtime(print)).transpose()?;
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let mutator = builder.block_params(block)[0];
    // The pinned register belongs to the caller under the C convention.
    let saved = builder.ins().get_pinned_reg(WORD);
    builder.ins().set_pinned_reg(mutator);
    let callee = backend.module.declare_func_in_func(entry.id, builder.func);
    let call = builder.ins().call(callee, &[]);
    let result = builder.inst_results(call)[0];
    if let Some(print) = print {
        let print = backend.module.declare_func_in_func(print, builder.func);
        builder.ins().call(print, &[result]);
    }
    builder.ins().set_pinned_reg(saved);
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}

/// Translates one MIR function body into the Cranelift function it borrows.
struct Translator<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    backend: &'a mut Backend<'db>,
    declared: &'a HashMap<FnKey<'db>, Declared<'db>>,
    key: &'a FnKey<'db>,
    body: &'a Body<'a, 'db>,
    builder: FunctionBuilder<'a>,
    /// The closure this function was entered through, when it was.
    closure: Variable,
    /// The variable of each local that carries a value; `None` for a `null`
    /// local and for a local of a type outside the subset, which a checked
    /// body never reads or writes.
    variables: Vec<Option<Variable>>,
    blocks: Vec<ir::Block>,
    function_refs: HashMap<FuncId, ir::FuncRef>,
    data_refs: HashMap<DataId, ir::GlobalValue>,
    loops: ArrayLoops<'db>,
    /// The array accesses a counted loop proves in range.
    proven: ProvenIndexes,
    /// The MIR block being translated.
    current: usize,
    /// The address of the worker this code runs on: what the pinned register
    /// held on entry and after the latest call. A call can come back on
    /// another worker, so nothing read before one is used after it.
    worker: Variable,
    /// The length and data address already read from each array value in the
    /// code being generated; see [`Self::array_view`].
    array_views: HashMap<ir::Value, (ir::Value, ir::Value)>,
    /// The position behind each (index, length) pair already checked in
    /// that same code.
    checked_indexes: HashMap<(ir::Value, ir::Value), ir::Value>,
    /// The array and the index behind each array iterator local.
    iterator_variables: HashMap<Local, (Variable, Variable)>,
    /// The end-of-iteration flag and the element behind each local holding
    /// what `next()` answered. The element has no variable when it is `null`.
    item_variables: HashMap<Local, (Variable, Option<Variable>)>,
}

/// A size, offset or count as the 32 bits the type table stores it in.
fn table_word(value: usize) -> u32 {
    u32::try_from(value).expect("type table entries fit 32 bits")
}

impl<'a, 'db> Translator<'a, 'db> {
    fn new(
        db: &'db dyn baml_compiler2_mir::Db,
        backend: &'a mut Backend<'db>,
        declared: &'a HashMap<FnKey<'db>, Declared<'db>>,
        key: &'a FnKey<'db>,
        body: &'a Body<'a, 'db>,
        function: &'a mut ir::Function,
        builder_context: &'a mut FunctionBuilderContext,
    ) -> Self {
        let mut builder = FunctionBuilder::new(function, builder_context);
        let variables = (0..body.locals.len())
            .map(|index| match local_ty(db, body, Local(index)) {
                Ok(NativeTy::Null) | Err(_) => None,
                Ok(ty) => {
                    let variable = builder.declare_var(clif_type(&ty));
                    if ty.is_ref() {
                        builder.declare_var_needs_stack_map(variable);
                    }
                    Some(variable)
                }
            })
            .collect();
        let blocks = body.blocks.iter().map(|_| builder.create_block()).collect();
        let loops = ArrayLoops::of(db, body);
        let worker = builder.declare_var(WORD);
        let closure = builder.declare_var(WORD);
        builder.declare_var_needs_stack_map(closure);
        let mut iterator_variables = HashMap::new();
        let mut item_variables = HashMap::new();
        for index in 0..body.locals.len() {
            let local = Local(index);
            if loops.iterator(local).is_some() {
                let array = builder.declare_var(WORD);
                builder.declare_var_needs_stack_map(array);
                iterator_variables.insert(local, (array, builder.declare_var(types::I64)));
            }
            if let Some(element) = loops.item(local) {
                let done = builder.declare_var(types::I8);
                let value = (*element != NativeTy::Null).then(|| {
                    let value = builder.declare_var(clif_type(element));
                    if element.is_ref() {
                        builder.declare_var_needs_stack_map(value);
                    }
                    value
                });
                item_variables.insert(local, (done, value));
            }
        }
        Self {
            db,
            backend,
            declared,
            key,
            body,
            builder,
            closure,
            variables,
            blocks,
            function_refs: HashMap::new(),
            data_refs: HashMap::new(),
            loops,
            proven: ProvenIndexes::default(),
            current: 0,
            worker,
            array_views: HashMap::new(),
            checked_indexes: HashMap::new(),
            iterator_variables,
            item_variables,
        }
    }

    fn function(mut self, mir: &MirFunction<'db>, checks_stack: bool) -> Result<(), CompileError> {
        // The MIR entry block can be a loop header, and a Cranelift entry
        // block cannot have predecessors, so parameters land in a block of
        // their own.
        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);
        let mut arriving = self.builder.block_params(entry).to_vec().into_iter();
        if self.key.entry != Entry::Direct {
            let closure = arriving.next().expect("a closure parameter");
            self.builder.def_var(self.closure, closure);
        }
        // A captured parameter waits in a variable of its own until its
        // cell can be allocated.
        let mut captured = Vec::new();
        for index in 1..=mir.arity {
            let local = Local(index);
            let ty = self.local_ty(local)?;
            let content = match &ty {
                NativeTy::Cell(content) => content,
                other => other,
            };
            let value = if index == 1 && self.key.entry == Entry::Bound {
                let closure = self.builder.use_var(self.closure);
                let receiver = self.builder.ins().load(
                    clif_type(content),
                    closure_access(),
                    closure,
                    offset(layout::CLOSURE_CAPTURES),
                );
                self.track(receiver, content)
            } else {
                arriving.next().expect("a parameter per argument")
            };
            if let NativeTy::Cell(content) = &ty {
                if **content != NativeTy::Null {
                    let waiting = self.builder.declare_var(clif_type(content));
                    if content.is_ref() {
                        self.builder.declare_var_needs_stack_map(waiting);
                    }
                    self.builder.def_var(waiting, value);
                    captured.push((local, (**content).clone(), Some(waiting)));
                } else {
                    captured.push((local, NativeTy::Null, None));
                }
            } else {
                self.write_local(local, value);
            }
        }
        self.read_worker();
        if checks_stack {
            self.check_stack()?;
        }
        for (local, content, waiting) in captured {
            let value = waiting.map(|waiting| self.builder.use_var(waiting));
            let cell = self.new_cell(&content, value)?;
            self.write_local(local, cell);
        }
        let first = self.block(self.body.entry);
        self.builder.ins().jump(first, &[]);

        let flow = Flow::of(self.body);
        self.proven = ProvenIndexes::of(self.body, mir.arity, &flow);
        let closes_loop = self.polled_blocks(&flow);
        for (index, block) in self.body.blocks.iter().enumerate() {
            self.builder.switch_to_block(self.blocks[index]);
            self.current = index;
            self.forget_arrays();
            for statement in &block.statements {
                self.statement(&statement.kind)?;
            }
            let terminator = block
                .terminator
                .as_ref()
                .ok_or_else(|| internal("a checked block has a terminator"))?;
            if closes_loop[index] {
                self.poll()?;
            }
            self.terminator(terminator)?;
        }
        self.builder.seal_all_blocks();
        self.resolve_aliases();
        self.builder.finalize();
        Ok(())
    }

    /// The blocks that end an iteration of a loop which must ask whether to
    /// stop: one with no call to a function that checks the stack on entry,
    /// which asks the same question.
    fn polled_blocks(&self, flow: &Flow) -> Vec<bool> {
        let mut polled = vec![false; self.body.blocks.len()];
        for each in &flow.loops {
            let asks = self
                .body
                .blocks
                .iter()
                .enumerate()
                .any(|(block, contents)| {
                    each.blocks[block]
                        && match &contents.terminator {
                            Some(Terminator::Call { callee, .. }) => {
                                match direct_callee(self.db, callee) {
                                    Ok(Callee::Function(function)) => self
                                        .declared
                                        .get(&FnKey::new(function, Entry::Direct))
                                        .is_some_and(|callee| callee.checks_stack),
                                    _ => false,
                                }
                            }
                            _ => false,
                        }
                });
            if !asks {
                for &latch in &each.latches {
                    polled[latch] = true;
                }
            }
        }
        polled
    }

    /// Replace every use of a value alias by the value it stands for.
    ///
    /// Building SSA form leaves aliases behind, mostly as branch arguments.
    /// The pass that spills references around calls (run by `finalize`)
    /// decides what is live with aliases resolved but reloads only uses that
    /// name the value itself, so a use through an alias would keep reading
    /// the register the collector did not update.
    fn resolve_aliases(&mut self) {
        let function = &mut *self.builder.func;
        let blocks: Vec<_> = function.layout.blocks().collect();
        let mut values = Vec::new();
        for block in blocks {
            let instructions: Vec<_> = function.layout.block_insts(block).collect();
            for instruction in instructions {
                values.extend(
                    function
                        .dfg
                        .inst_values(instruction)
                        .map(|value| function.dfg.resolve_aliases(value)),
                );
                function
                    .dfg
                    .overwrite_inst_values(instruction, values.drain(..));
            }
        }
    }

    fn block(&self, id: BlockId) -> ir::Block {
        self.blocks[id.0]
    }

    fn local_ty(&self, local: Local) -> Result<NativeTy<'db>, CompileError> {
        local_ty(self.db, self.body, local).map_err(internal)
    }

    fn place_ty(&self, place: &Place) -> Result<NativeTy<'db>, CompileError> {
        place_ty(self.db, self.body, place).map_err(internal)
    }

    fn operand_ty(&self, operand: &Operand<'db>) -> Result<NativeTy<'db>, CompileError> {
        operand_ty(self.db, self.body, operand).map_err(internal)
    }

    fn null(&mut self) -> ir::Value {
        self.builder.ins().iconst(types::I8, 0)
    }

    fn bool_const(&mut self, value: bool) -> ir::Value {
        self.builder.ins().iconst(types::I8, i64::from(value))
    }

    fn word_const(&mut self, value: usize) -> ir::Value {
        self.builder.ins().iconst(WORD, imm(value))
    }

    /// Mark a reference so that it is spilled across calls and named in
    /// their stack maps.
    fn track(&mut self, value: ir::Value, ty: &NativeTy<'db>) -> ir::Value {
        if ty.is_ref() {
            self.builder.declare_value_needs_stack_map(value);
        }
        value
    }

    /// Change the representation of `value` from `from` to `to`, which it is
    /// assignable to. A reference and its optional are the same bits; an
    /// optional scalar is a pair to build or take apart.
    fn coerce(&mut self, value: ir::Value, from: &NativeTy<'db>, to: &NativeTy<'db>) -> ir::Value {
        if from == to {
            return value;
        }
        match (from, to) {
            (NativeTy::Null, to) if to.is_pair() => {
                let zero = self.builder.ins().iconst(WORD, 0);
                self.builder.ins().iconcat(zero, zero)
            }
            (NativeTy::Null, _) => self.builder.ins().iconst(WORD, 0),
            (from, to) if to.is_pair() => {
                let bits = self.as_slot(value, from);
                let present = self.builder.ins().iconst(WORD, 1);
                self.builder.ins().iconcat(bits, present)
            }
            (from, to) if from.is_pair() => {
                let (bits, _) = self.builder.ins().isplit(value);
                match to {
                    NativeTy::Float => {
                        self.builder
                            .ins()
                            .bitcast(types::F64, MemFlags::new(), bits)
                    }
                    NativeTy::Bool => self.builder.ins().ireduce(types::I8, bits),
                    _ => bits,
                }
            }
            _ => value,
        }
    }

    /// Whether an optional holds a value, as a word that is zero for `null`.
    fn presence(&mut self, value: ir::Value, ty: &NativeTy<'db>) -> ir::Value {
        if ty.is_pair() {
            self.builder.ins().isplit(value).1
        } else {
            value
        }
    }

    /// The type and byte offset of field `field` of the class instance a
    /// place of type `base` holds.
    fn field(&self, base: &Place, field: usize) -> Result<(NativeTy<'db>, usize), CompileError> {
        let NativeTy::Class(class) = narrowed(self.place_ty(base)?) else {
            return Err(internal("a checked field access is on a class instance"));
        };
        let layout = class_layout(self.db, class).map_err(internal)?;
        let slot = field_slots(&layout)[field];
        Ok((layout[field].clone(), layout::field_offset(slot)))
    }

    fn read_local(&mut self, local: Local) -> ir::Value {
        match self.variables[local.0] {
            Some(variable) => self.builder.use_var(variable),
            None => self.null(),
        }
    }

    fn write_local(&mut self, local: Local, value: ir::Value) {
        if let Some(variable) = self.variables[local.0] {
            self.builder.def_var(variable, value);
        }
    }

    /// Load a slot of type `ty` at `base + bytes`.
    fn load_slot(&mut self, ty: &NativeTy<'db>, base: ir::Value, bytes: usize) -> ir::Value {
        if *ty == NativeTy::Null {
            return self.null();
        }
        let value = self
            .builder
            .ins()
            .load(clif_type(ty), slot_access(), base, offset(bytes));
        self.track(value, ty)
    }

    fn store_slot(&mut self, ty: &NativeTy<'db>, value: ir::Value, base: ir::Value, bytes: usize) {
        if *ty != NativeTy::Null {
            self.builder
                .ins()
                .store(slot_access(), value, base, offset(bytes));
        }
    }

    /// The length of `array` and the address of its first element.
    ///
    /// Both are read once and reused until something that could change them
    /// or move the storage: a call, an append, or the end of the MIR block.
    /// Cranelift merges such loads only inside one of its own blocks, and
    /// every bounds check ends one.
    fn array_view(&mut self, array: ir::Value) -> (ir::Value, ir::Value) {
        // A variable read after a join can come back as an alias of the
        // value read before it.
        let array = self.builder.func.dfg.resolve_aliases(array);
        if let Some(view) = self.array_views.get(&array) {
            return *view;
        }
        let length =
            self.builder
                .ins()
                .load(WORD, array_access(), array, offset(layout::ARRAY_LENGTH));
        let data = self
            .builder
            .ins()
            .load(WORD, array_access(), array, offset(layout::ARRAY_DATA));
        self.array_views.insert(array, (length, data));
        (length, data)
    }

    /// Drop what is known about arrays: the code that follows may run after
    /// a collection or an append, or is reached some other way.
    fn forget_arrays(&mut self) {
        self.array_views.clear();
        self.checked_indexes.clear();
    }

    /// The position `index` names in an array of `length` elements: itself,
    /// or counted back from the end when negative. Panics when that is not
    /// an element.
    fn position(&mut self, index: ir::Value, length: ir::Value) -> Result<ir::Value, CompileError> {
        // Add the length to a negative index without a branch: the sign,
        // spread over the word, selects the length or zero. An index known
        // not to be negative is its own position.
        let known =
            self.builder
                .func
                .dfg
                .value_def(index)
                .inst()
                .and_then(
                    |instruction| match self.builder.func.dfg.insts[instruction] {
                        ir::InstructionData::UnaryImm {
                            opcode: ir::Opcode::Iconst,
                            imm,
                        } => Some(imm.bits()),
                        _ => None,
                    },
                );
        let position = if known.is_some_and(|index| index >= 0) {
            index
        } else {
            let sign = self.builder.ins().sshr_imm(index, INT_BITS - 1);
            let from_end = self.builder.ins().band(sign, length);
            self.builder.ins().iadd(index, from_end)
        };
        // One unsigned comparison also rejects a position still negative.
        let outside = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, position, length);
        self.panic_if(outside, Panic::IndexOutOfBounds(index, length))?;
        Ok(position)
    }

    /// The address of element `index` of `array`, after checking the index.
    /// Every element access goes through here.
    fn element(&mut self, array: ir::Value, index: ir::Value) -> Result<ir::Value, CompileError> {
        let (length, data) = self.array_view(array);
        let index = self.builder.func.dfg.resolve_aliases(index);
        // An index already checked against this length, with nothing since
        // that could have changed the length, needs no second check.
        let index = match self.checked_indexes.get(&(index, length)) {
            Some(position) => *position,
            None => {
                let position = self.position(index, length)?;
                self.checked_indexes.insert((index, length), position);
                position
            }
        };
        let scaled = self.builder.ins().ishl_imm(index, 3);
        Ok(self.builder.ins().iadd(data, scaled))
    }

    /// The address of the element of `array`, read from `base`, that the
    /// local `index` names.
    fn indexed(
        &mut self,
        base: &Place,
        array: ir::Value,
        index: Local,
    ) -> Result<ir::Value, CompileError> {
        let position = self.read_local(index);
        if let Place::Local(base) = base
            && self.proven.holds(self.current, *base, index)
        {
            // A counted loop over this array: the index names an element.
            let (_, data) = self.array_view(array);
            let scaled = self.builder.ins().ishl_imm(position, 3);
            return Ok(self.builder.ins().iadd(data, scaled));
        }
        self.element(array, position)
    }

    fn read_place(&mut self, place: &Place) -> Result<ir::Value, CompileError> {
        match place {
            Place::Local(local) => Ok(self.read_local(*local)),
            Place::Field { base, field } => {
                let (ty, bytes) = self.field(base, *field)?;
                let object = self.read_place(base)?;
                Ok(self.load_slot(&ty, object, bytes))
            }
            Place::Index { base, index, .. } => {
                let ty = self.place_ty(place)?;
                let array = self.read_place(base)?;
                let element = self.indexed(base, array, *index)?;
                Ok(self.load_slot(&ty, element, 0))
            }
            Place::Capture(index) => {
                let closure = self.builder.use_var(self.closure);
                let cell = self.builder.ins().load(
                    WORD,
                    closure_access(),
                    closure,
                    offset(layout::CLOSURE_CAPTURES + index * layout::WORD),
                );
                self.builder.declare_value_needs_stack_map(cell);
                Ok(cell)
            }
            Place::Deref(cell) => {
                let ty = self.place_ty(place)?;
                let cell = self.cell(*cell)?;
                Ok(self.load_slot(&ty, cell, layout::CELL_VALUE))
            }
        }
    }

    /// The address of a cell.
    fn cell(&mut self, cell: CellId) -> Result<ir::Value, CompileError> {
        self.read_place(&match cell {
            CellId::Local(local) => Place::Local(local),
            CellId::Capture(index) => Place::Capture(index),
        })
    }

    /// A new cell for a value of type `content`, holding `value` or, without
    /// one, `null`.
    fn new_cell(
        &mut self,
        content: &NativeTy<'db>,
        value: Option<ir::Value>,
    ) -> Result<ir::Value, CompileError> {
        let (cell_type, slots) = if content.is_pair() {
            (layout::TYPE_CELL_PAIR, 2)
        } else if content.is_ref() {
            (layout::TYPE_CELL_REF, 1)
        } else {
            (layout::TYPE_CELL_VALUE, 1)
        };
        let cell = self.allocate(layout::instance_size(slots))?;
        let cell_type = self.word_const(cell_type);
        self.builder.ins().store(slot_access(), cell_type, cell, 0);
        match value {
            Some(value) => self.store_slot(content, value, cell, layout::CELL_VALUE),
            None => {
                let zero = self.word_const(0);
                for slot in 0..slots {
                    self.builder.ins().store(
                        slot_access(),
                        zero,
                        cell,
                        offset(layout::field_offset(slot)),
                    );
                }
            }
        }
        Ok(cell)
    }

    /// A new closure over `captured`, entered at `code`.
    fn closure(
        &mut self,
        code: FuncId,
        captured: &[(ir::Value, bool)],
    ) -> Result<ir::Value, CompileError> {
        let references: Vec<bool> = captured.iter().map(|(_, is_ref)| *is_ref).collect();
        let closure_type = self.backend.closure_type(&references);
        let closure = self.allocate(layout::CLOSURE_CAPTURES + captured.len() * layout::WORD)?;
        let closure_type = self.word_const(closure_type);
        self.builder
            .ins()
            .store(slot_access(), closure_type, closure, 0);
        let code = self.function_ref(code);
        let code = self.builder.ins().func_addr(WORD, code);
        self.builder
            .ins()
            .store(slot_access(), code, closure, offset(layout::CLOSURE_CODE));
        for (index, (value, _)) in captured.iter().enumerate() {
            self.builder.ins().store(
                slot_access(),
                *value,
                closure,
                offset(layout::CLOSURE_CAPTURES + index * layout::WORD),
            );
        }
        Ok(closure)
    }

    fn declared(&self, key: &FnKey<'db>) -> Result<&'a Declared<'db>, CompileError> {
        self.declared
            .get(key)
            .ok_or_else(|| internal("a function that is used is part of the program"))
    }

    fn write_place(&mut self, place: &Place, value: ir::Value) -> Result<(), CompileError> {
        match place {
            Place::Local(local) => self.write_local(*local, value),
            Place::Field { base, field } => {
                let (ty, bytes) = self.field(base, *field)?;
                let object = self.read_place(base)?;
                self.store_slot(&ty, value, object, bytes);
            }
            Place::Index { base, index, .. } => {
                let ty = self.place_ty(place)?;
                let array = self.read_place(base)?;
                let element = self.indexed(base, array, *index)?;
                self.store_slot(&ty, value, element, 0);
            }
            Place::Capture(_) => return Err(internal("a capture is never assigned")),
            Place::Deref(cell) => {
                let ty = self.place_ty(place)?;
                let cell = self.cell(*cell)?;
                self.store_slot(&ty, value, cell, layout::CELL_VALUE);
            }
        }
        Ok(())
    }

    fn operand(&mut self, operand: &Operand<'db>) -> Result<ir::Value, CompileError> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.read_place(place),
            Operand::Constant(Constant::Int(value)) => {
                Ok(self.builder.ins().iconst(types::I64, *value))
            }
            Operand::Constant(Constant::EnumVariant { index, .. }) => {
                Ok(self.builder.ins().iconst(types::I64, i64::from(*index)))
            }
            Operand::Constant(Constant::Float(value)) => Ok(self.builder.ins().f64const(*value)),
            Operand::Constant(Constant::Bool(value)) => Ok(self.bool_const(*value)),
            Operand::Constant(Constant::Null) => Ok(self.null()),
            Operand::Constant(Constant::String(text)) => {
                let string = self.backend.string_constant(text)?;
                Ok(self.data_address(string))
            }
            Operand::Constant(Constant::Function(DeclRef::Source(function))) => {
                let code = self.declared(&FnKey::new(*function, Entry::Value))?.id;
                let closure = self.backend.function_value(code)?;
                Ok(self.data_address(closure))
            }
            Operand::Constant(_) => Err(internal("a checked operand is a supported constant")),
        }
    }

    /// `operand` in the representation of `expected`.
    fn operand_as(
        &mut self,
        operand: &Operand<'db>,
        expected: &NativeTy<'db>,
    ) -> Result<ir::Value, CompileError> {
        let found = self.operand_ty(operand)?;
        let value = self.operand(operand)?;
        Ok(self.coerce(value, &found, expected))
    }

    fn statement(&mut self, statement: &StatementKind<'db>) -> Result<(), CompileError> {
        match statement {
            StatementKind::Nop | StatementKind::Drop(_) => Ok(()),
            // A type argument: nothing at run time.
            StatementKind::Assign {
                value: Rvalue::LoadType(_),
                ..
            } => Ok(()),
            StatementKind::Assign { destination, value } => {
                let expected = self.place_ty(destination)?;
                let (value, found) = self.rvalue(value, &expected)?;
                let value = self.coerce(value, &found, &expected);
                self.write_place(destination, value)
            }
            StatementKind::FreshCell { local, carry_value } => {
                let NativeTy::Cell(content) = self.local_ty(*local)? else {
                    return Err(internal("a checked fresh cell is for a captured local"));
                };
                let carried = if *carry_value {
                    let old = self.read_local(*local);
                    Some(self.load_slot(&content, old, layout::CELL_VALUE))
                } else {
                    None
                };
                let cell = self.new_cell(&content, carried)?;
                self.write_local(*local, cell);
                Ok(())
            }
            _ => Err(internal("a checked statement is an assignment")),
        }
    }

    fn rvalue(
        &mut self,
        value: &Rvalue<'db>,
        expected: &NativeTy<'db>,
    ) -> Result<(ir::Value, NativeTy<'db>), CompileError> {
        let ty = crate::check::rvalue_ty(self.db, self.body, &self.loops, value, expected)
            .map_err(internal)?;
        let item = match value {
            Rvalue::Use(operand) | Rvalue::IsType { operand, .. } => {
                operand_local(operand).and_then(|local| self.item_variables.get(&local).copied())
            }
            _ => None,
        };
        let value = match (value, item) {
            (Rvalue::Use(_), Some((_, Some(element)))) => self.builder.use_var(element),
            (Rvalue::Use(_), Some((_, None))) => self.null(),
            (Rvalue::IsType { .. }, Some((done, _))) => self.builder.use_var(done),
            _ => self.plain_rvalue(value, &ty)?,
        };
        Ok((self.track(value, &ty), ty))
    }

    fn plain_rvalue(
        &mut self,
        value: &Rvalue<'db>,
        ty: &NativeTy<'db>,
    ) -> Result<ir::Value, CompileError> {
        Ok(match value {
            Rvalue::Use(operand) => self.operand(operand)?,
            Rvalue::BinaryOp { op, left, right } => self.binary(*op, left, right)?,
            Rvalue::UnaryOp { op, operand } => self.unary(*op, operand)?,
            Rvalue::ConcatStrings(parts) => {
                let parts = parts
                    .iter()
                    .map(|part| self.operand(part))
                    .collect::<Result<Vec<_>, _>>()?;
                self.concat(&parts)?
            }
            Rvalue::Discriminant(place) => self.read_place(place)?,
            Rvalue::Len(place) => {
                let array = self.read_place(place)?;
                self.array_view(array).0
            }
            Rvalue::Array(_, elements)
            | Rvalue::Aggregate {
                kind: AggregateKind::Array,
                fields: elements,
            } => {
                let NativeTy::Array(element) = ty else {
                    return Err(internal("a checked array literal makes an array"));
                };
                self.array_literal(element, elements)?
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Class { class, .. },
                fields,
            } => self.instance(*class, fields)?,
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            } => {
                let cells = captures
                    .iter()
                    .map(|capture| Ok((self.operand(capture)?, true)))
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let code = self.declared(&self.key.lambda(*lambda_idx))?.id;
                self.closure(code, &cells)?
            }
            Rvalue::MakeBoundMethod {
                func: DeclRef::Source(function),
                receiver,
                ..
            } => {
                let receiver = self.operand(receiver)?;
                let code = self.declared(&FnKey::new(*function, Entry::Bound))?.id;
                self.closure(code, &[(receiver, true)])?
            }
            _ => return Err(internal("a checked rvalue is supported")),
        })
    }

    fn function_ref(&mut self, id: FuncId) -> ir::FuncRef {
        *self.function_refs.entry(id).or_insert_with(|| {
            self.backend
                .module
                .declare_func_in_func(id, self.builder.func)
        })
    }

    fn runtime_ref(&mut self, function: Runtime) -> Result<ir::FuncRef, CompileError> {
        let id = self.backend.runtime(function)?;
        Ok(self.function_ref(id))
    }

    /// Call a runtime function and return its result, if it has one.
    fn call_runtime(
        &mut self,
        function: Runtime,
        arguments: &[ir::Value],
    ) -> Result<Option<ir::Value>, CompileError> {
        // The runtime can collect, which moves storage.
        self.forget_arrays();
        let result = self.emit_runtime_call(function, arguments)?;
        self.read_worker();
        Ok(result)
    }

    /// The one place a call into the runtime is emitted, whatever it takes
    /// to get there. Use [`Self::call_runtime`] unless the call does not
    /// return.
    fn emit_runtime_call(
        &mut self,
        function: Runtime,
        arguments: &[ir::Value],
    ) -> Result<Option<ir::Value>, CompileError> {
        let function = self.runtime_ref(function)?;
        let call = self.builder.ins().call(function, arguments);
        Ok(self.builder.inst_results(call).first().copied())
    }

    /// Call the runtime when this function is entered with less stack than
    /// the limit allows.
    fn check_stack(&mut self) -> Result<(), CompileError> {
        let mutator = self.mutator();
        let limit = self.builder.ins().load(
            WORD,
            mutator_access(),
            mutator,
            offset(layout::MUTATOR_STACK_LIMIT),
        );
        let stack = self.builder.ins().get_stack_pointer(WORD);
        let low = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, stack, limit);
        let slow = self.builder.create_block();
        let resume = self.builder.create_block();
        self.builder.set_cold_block(slow);
        self.builder.ins().brif(low, slow, &[], resume, &[]);
        self.builder.seal_block(slow);

        self.builder.switch_to_block(slow);
        self.call_runtime(Runtime::StackSlow, &[])?;
        self.builder.ins().jump(resume, &[]);
        self.builder.seal_block(resume);

        self.builder.switch_to_block(resume);
        Ok(())
    }

    /// Call the runtime before going round a loop again when the poll word
    /// asks for it.
    fn poll(&mut self) -> Result<(), CompileError> {
        let mutator = self.mutator();
        let requested =
            self.builder
                .ins()
                .uload32(mutator_access(), mutator, offset(layout::MUTATOR_POLL));
        let slow = self.builder.create_block();
        let resume = self.builder.create_block();
        self.builder.set_cold_block(slow);
        self.builder.ins().brif(requested, slow, &[], resume, &[]);
        self.builder.seal_block(slow);

        self.builder.switch_to_block(slow);
        let mutator = self.mutator();
        self.call_runtime(Runtime::PollSlow, &[mutator])?;
        self.builder.ins().jump(resume, &[]);
        self.builder.seal_block(resume);

        self.builder.switch_to_block(resume);
        Ok(())
    }

    /// The address of a data object.
    fn data_address(&mut self, id: DataId) -> ir::Value {
        let global = *self.data_refs.entry(id).or_insert_with(|| {
            self.backend
                .module
                .declare_data_in_func(id, self.builder.func)
        });
        self.builder.ins().symbol_value(WORD, global)
    }

    /// The address of the worker this code is running on.
    fn mutator(&mut self) -> ir::Value {
        self.builder.use_var(self.worker)
    }

    /// Read the worker's address from the pinned register: on entry, and
    /// after every call that comes back.
    fn read_worker(&mut self) {
        let worker = self.builder.ins().get_pinned_reg(WORD);
        self.builder.def_var(self.worker, worker);
    }

    /// Branch to a cold block that raises `panic` when `condition` is true,
    /// and continue in a fresh block otherwise.
    fn panic_if(&mut self, condition: ir::Value, panic: Panic) -> Result<(), CompileError> {
        let raise = self.builder.create_block();
        let resume = self.builder.create_block();
        self.builder.set_cold_block(raise);
        self.builder.ins().brif(condition, raise, &[], resume, &[]);
        // Both blocks have this one predecessor. Saying so now makes a
        // variable read after the check the same value as before it, which
        // is what lets `array_view` recognise an array it has already read.
        self.builder.seal_block(raise);
        self.builder.seal_block(resume);

        self.builder.switch_to_block(raise);
        let (function, arguments) = match panic {
            Panic::DivisionByZero(dividend) => (Runtime::PanicDivisionByZero, vec![dividend]),
            Panic::NegativeShift(count) => (Runtime::PanicNegativeShift, vec![count]),
            Panic::IndexOutOfBounds(index, length) => {
                (Runtime::PanicIndexOutOfBounds, vec![index, length])
            }
        };
        // Not through `call_runtime`: this path does not come back, so what
        // is known about arrays still holds on the one that continues.
        self.emit_runtime_call(function, &arguments)?;
        self.builder.ins().trap(TRAP_AFTER_PANIC);

        self.builder.switch_to_block(resume);
        Ok(())
    }

    /// Allocate `size` bytes for an object: bump the mutator's pointer, or
    /// call the runtime when its span is exhausted. The memory is not
    /// initialized, so the caller fills every slot before the next call.
    fn allocate(&mut self, size: usize) -> Result<ir::Value, CompileError> {
        let mutator = self.mutator();
        let object = self.builder.ins().load(
            WORD,
            mutator_access(),
            mutator,
            offset(layout::MUTATOR_ALLOC_PTR),
        );
        let limit = self.builder.ins().load(
            WORD,
            mutator_access(),
            mutator,
            offset(layout::MUTATOR_ALLOC_LIMIT),
        );
        let end = self.builder.ins().iadd_imm(object, imm(size));
        let exhausted = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, end, limit);

        let bump = self.builder.create_block();
        let slow = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, WORD);
        self.builder.set_cold_block(slow);
        self.builder.ins().brif(exhausted, slow, &[], bump, &[]);

        self.builder.switch_to_block(bump);
        self.builder.ins().store(
            mutator_access(),
            end,
            mutator,
            offset(layout::MUTATOR_ALLOC_PTR),
        );
        self.builder.ins().jump(done, &[object.into()]);

        self.builder.switch_to_block(slow);
        let size = self.word_const(size);
        let mutator = self.mutator();
        let object = self
            .call_runtime(Runtime::AllocSlow, &[mutator, size])?
            .ok_or_else(|| internal("allocation returns an address"))?;
        self.builder.ins().jump(done, &[object.into()]);

        self.builder.switch_to_block(done);
        let object = self.builder.block_params(done)[0];
        self.builder.declare_value_needs_stack_map(object);
        Ok(object)
    }

    /// A new instance of `class` with the given field values.
    fn instance(
        &mut self,
        class: ClassRef<'db>,
        fields: &[Operand<'db>],
    ) -> Result<ir::Value, CompileError> {
        let layout = class_layout(self.db, class).map_err(internal)?;
        // Evaluate before allocating: the fields are stored with no call in
        // between, so the collector never sees a half-built object.
        let values = fields
            .iter()
            .zip(&layout)
            .map(|(field, ty)| self.operand_as(field, ty))
            .collect::<Result<Vec<_>, _>>()?;
        let class_type = self.backend.class_type(class, &layout);
        let object = self.allocate(layout::instance_size(layout_slots(&layout)))?;
        let class_type = self.word_const(class_type);
        self.builder
            .ins()
            .store(slot_access(), class_type, object, 0);
        for ((ty, value), slot) in layout.iter().zip(values).zip(field_slots(&layout)) {
            self.store_slot(ty, value, object, layout::field_offset(slot));
        }
        Ok(object)
    }

    /// The bits of `value` as a whole slot.
    fn as_slot(&mut self, value: ir::Value, ty: &NativeTy<'db>) -> ir::Value {
        match ty {
            NativeTy::Float => self.builder.ins().bitcast(WORD, MemFlags::new(), value),
            NativeTy::Bool | NativeTy::Null => self.builder.ins().uextend(WORD, value),
            _ => value,
        }
    }

    /// The id of the type of arrays of `element`.
    fn array_type(&mut self, element: &NativeTy<'db>) -> ir::Value {
        self.word_const(if element.is_ref() {
            layout::TYPE_ARRAY_OF_REFS
        } else {
            layout::TYPE_ARRAY_OF_VALUES
        })
    }

    /// A new array holding `elements`.
    fn array_literal(
        &mut self,
        element: &NativeTy<'db>,
        elements: &[Operand<'db>],
    ) -> Result<ir::Value, CompileError> {
        let values = elements
            .iter()
            .map(|operand| self.operand_as(operand, element))
            .collect::<Result<Vec<_>, _>>()?;
        let array_type = self.array_type(element);
        let length = self.word_const(values.len());
        let mutator = self.mutator();
        let array = self
            .call_runtime(Runtime::ArrayNew, &[mutator, array_type, length])?
            .ok_or_else(|| internal("array construction returns the array"))?;
        if !values.is_empty() {
            let storage =
                self.builder
                    .ins()
                    .load(WORD, array_access(), array, offset(layout::ARRAY_DATA));
            for (index, value) in values.into_iter().enumerate() {
                let value = self.as_slot(value, element);
                self.builder.ins().store(
                    slot_access(),
                    value,
                    storage,
                    offset(index * layout::WORD),
                );
            }
        }
        Ok(array)
    }

    /// `array.push(item)`: store in place while the storage has room, call
    /// the runtime to grow it otherwise. The result is the new length.
    fn array_push(
        &mut self,
        array: ir::Value,
        item: ir::Value,
        element: &NativeTy<'db>,
    ) -> Result<ir::Value, CompileError> {
        let item = self.as_slot(item, element);
        let length =
            self.builder
                .ins()
                .load(WORD, array_access(), array, offset(layout::ARRAY_LENGTH));
        let storage =
            self.builder
                .ins()
                .load(WORD, array_access(), array, offset(layout::ARRAY_DATA));
        let new_length = self.builder.ins().iadd_imm(length, 1);

        let has_storage = self.builder.create_block();
        let in_place = self.builder.create_block();
        let grow = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.set_cold_block(grow);
        self.builder
            .ins()
            .brif(storage, has_storage, &[], grow, &[]);

        self.builder.switch_to_block(has_storage);
        let capacity =
            self.builder
                .ins()
                .load(WORD, array_access(), storage, layout::DATA_CAPACITY);
        let full = self.builder.ins().icmp(IntCC::Equal, length, capacity);
        self.builder.ins().brif(full, grow, &[], in_place, &[]);

        self.builder.switch_to_block(in_place);
        let scaled = self.builder.ins().ishl_imm(length, 3);
        let slot = self.builder.ins().iadd(storage, scaled);
        self.builder.ins().store(slot_access(), item, slot, 0);
        self.builder.ins().store(
            array_access(),
            new_length,
            array,
            offset(layout::ARRAY_LENGTH),
        );
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(grow);
        let mutator = self.mutator();
        self.call_runtime(Runtime::ArrayPushSlow, &[mutator, array, item])?;
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(done);
        self.forget_arrays();
        Ok(new_length)
    }

    /// Join strings into a new one. The parts are handed to the runtime in a
    /// frame slot, which it keeps current if it has to collect.
    fn concat(&mut self, parts: &[ir::Value]) -> Result<ir::Value, CompileError> {
        let size = u32::try_from(parts.len() * layout::WORD).map_err(internal)?;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            3,
        ));
        for (index, part) in parts.iter().enumerate() {
            self.builder
                .ins()
                .stack_store(*part, slot, offset(index * layout::WORD));
        }
        let address = self.builder.ins().stack_addr(WORD, slot, 0);
        let count = self.word_const(parts.len());
        let mutator = self.mutator();
        self.call_runtime(Runtime::StringConcat, &[mutator, address, count])?
            .ok_or_else(|| internal("concatenation returns the string"))
    }

    fn binary(
        &mut self,
        op: BinOp,
        left: &Operand<'db>,
        right: &Operand<'db>,
    ) -> Result<ir::Value, CompileError> {
        let left_ty = self.operand_ty(left)?;
        let right_ty = self.operand_ty(right)?;
        let (mode, _) = binary_typing(op, &left_ty, &right_ty).map_err(internal)?;
        let left_value = self.operand(left)?;
        let right_value = self.operand(right)?;
        let int_condition = match op {
            BinOp::Eq => IntCC::Equal,
            BinOp::Ne => IntCC::NotEqual,
            BinOp::Lt => IntCC::SignedLessThan,
            BinOp::Le => IntCC::SignedLessThanOrEqual,
            BinOp::Gt => IntCC::SignedGreaterThan,
            _ => IntCC::SignedGreaterThanOrEqual,
        };
        match mode {
            BinaryMode::Int if is_comparison(op) => {
                Ok(self
                    .builder
                    .ins()
                    .icmp(int_condition, left_value, right_value))
            }
            BinaryMode::Int => {
                let right_constant = match right {
                    Operand::Constant(Constant::Int(value)) => Some(*value),
                    _ => None,
                };
                self.int_arithmetic(op, left_value, right_value, right_constant)
            }
            BinaryMode::Float => {
                let left_float = self.float_from(left_value, &left_ty);
                let right_float = self.float_from(right_value, &right_ty);
                if is_comparison(op) {
                    Ok(self.float_comparison(
                        op,
                        (left_float, may_be_nan(left, &left_ty)),
                        (right_float, may_be_nan(right, &right_ty)),
                    ))
                } else {
                    self.float_arithmetic(op, left_float, right_float)
                }
            }
            BinaryMode::Bool => Ok(self
                .builder
                .ins()
                .icmp(int_condition, left_value, right_value)),
            // `null` is the one value of its type.
            BinaryMode::Null => Ok(self.bool_const(op == BinOp::Eq)),
            BinaryMode::NullTest { optional_on_left } => {
                let (optional, ty) = if optional_on_left {
                    (left_value, &left_ty)
                } else {
                    (right_value, &right_ty)
                };
                let presence = self.presence(optional, ty);
                Ok(self.builder.ins().icmp_imm(int_condition, presence, 0))
            }
            BinaryMode::String if op == BinOp::Add => self.concat(&[left_value, right_value]),
            BinaryMode::String if matches!(op, BinOp::Eq | BinOp::Ne) => {
                let equal = self
                    .call_runtime(Runtime::StringEq, &[left_value, right_value])?
                    .ok_or_else(|| internal("string equality returns a bool"))?;
                Ok(if op == BinOp::Eq {
                    equal
                } else {
                    self.builder.ins().bxor_imm(equal, 1)
                })
            }
            BinaryMode::String => {
                let order = self
                    .call_runtime(Runtime::StringCmp, &[left_value, right_value])?
                    .ok_or_else(|| internal("string comparison returns an order"))?;
                Ok(self.builder.ins().icmp_imm(int_condition, order, 0))
            }
        }
    }

    fn float_from(&mut self, value: ir::Value, ty: &NativeTy<'db>) -> ir::Value {
        if *ty == NativeTy::Int {
            self.builder.ins().fcvt_from_sint(types::F64, value)
        } else {
            value
        }
    }

    fn int_arithmetic(
        &mut self,
        op: BinOp,
        left: ir::Value,
        right: ir::Value,
        right_constant: Option<i64>,
    ) -> Result<ir::Value, CompileError> {
        match op {
            BinOp::Add => Ok(self.builder.ins().iadd(left, right)),
            BinOp::Sub => Ok(self.builder.ins().isub(left, right)),
            BinOp::Mul => Ok(self.builder.ins().imul(left, right)),
            BinOp::Div => self.divide(left, right, right_constant),
            // The remainder of the smallest int by -1 is zero, which is what
            // the instruction gives on every target: unlike division it is
            // defined not to trap there.
            BinOp::Mod => {
                self.require_nonzero_divisor(left, right, right_constant)?;
                Ok(self.builder.ins().srem(left, right))
            }
            BinOp::BitAnd => Ok(self.builder.ins().band(left, right)),
            BinOp::BitOr => Ok(self.builder.ins().bor(left, right)),
            BinOp::BitXor => Ok(self.builder.ins().bxor(left, right)),
            BinOp::Shl => {
                let past_width = self.require_shift_count(right)?;
                let shifted = self.builder.ins().ishl(left, right);
                let zero = self.builder.ins().iconst(types::I64, 0);
                Ok(self.builder.ins().select(past_width, zero, shifted))
            }
            BinOp::Shr => {
                let past_width = self.require_shift_count(right)?;
                // Shifting by the full width or more leaves only the sign.
                let most = self.builder.ins().iconst(types::I64, INT_BITS - 1);
                let count = self.builder.ins().select(past_width, most, right);
                Ok(self.builder.ins().sshr(left, count))
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                Err(internal("comparisons are not arithmetic"))
            }
        }
    }

    /// `left / right`. A zero divisor panics. The smallest int divided by
    /// -1 wraps to itself; the division instruction traps there instead (as
    /// the x86-64 one does in hardware), so a divisor of -1 is a negation
    /// and never reaches it.
    fn divide(
        &mut self,
        left: ir::Value,
        right: ir::Value,
        right_constant: Option<i64>,
    ) -> Result<ir::Value, CompileError> {
        match right_constant {
            Some(-1) => return Ok(self.builder.ins().ineg(left)),
            Some(divisor) if divisor != 0 => return Ok(self.builder.ins().sdiv(left, right)),
            _ => {}
        }
        // One comparison finds both divisors that are not divided by: plus
        // one, they are the only values at most one, unsigned.
        let shifted = self.builder.ins().iadd_imm(right, 1);
        let special = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedLessThanOrEqual, shifted, 1);
        let rare = self.builder.create_block();
        let divide = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        self.builder.set_cold_block(rare);
        self.builder.ins().brif(special, rare, &[], divide, &[]);
        self.builder.seal_block(rare);
        self.builder.seal_block(divide);

        self.builder.switch_to_block(rare);
        let is_zero = self.builder.ins().icmp_imm(IntCC::Equal, right, 0);
        self.panic_if(is_zero, Panic::DivisionByZero(left))?;
        let negated = self.builder.ins().ineg(left);
        self.builder.ins().jump(done, &[negated.into()]);

        self.builder.switch_to_block(divide);
        let quotient = self.builder.ins().sdiv(left, right);
        self.builder.ins().jump(done, &[quotient.into()]);
        self.builder.seal_block(done);

        self.builder.switch_to_block(done);
        Ok(self.builder.block_params(done)[0])
    }

    fn require_nonzero_divisor(
        &mut self,
        dividend: ir::Value,
        divisor: ir::Value,
        divisor_constant: Option<i64>,
    ) -> Result<(), CompileError> {
        if divisor_constant.is_some_and(|divisor| divisor != 0) {
            return Ok(());
        }
        let is_zero = self.builder.ins().icmp_imm(IntCC::Equal, divisor, 0);
        self.panic_if(is_zero, Panic::DivisionByZero(dividend))
    }

    /// Panic on a negative shift count; the result is whether the count is
    /// at least the width of `int`.
    fn require_shift_count(&mut self, count: ir::Value) -> Result<ir::Value, CompileError> {
        let negative = self.builder.ins().icmp_imm(IntCC::SignedLessThan, count, 0);
        self.panic_if(negative, Panic::NegativeShift(count))?;
        Ok(self
            .builder
            .ins()
            .icmp_imm(IntCC::SignedGreaterThanOrEqual, count, INT_BITS))
    }

    fn float_arithmetic(
        &mut self,
        op: BinOp,
        left: ir::Value,
        right: ir::Value,
    ) -> Result<ir::Value, CompileError> {
        Ok(match op {
            BinOp::Add => self.builder.ins().fadd(left, right),
            BinOp::Sub => self.builder.ins().fsub(left, right),
            BinOp::Mul => self.builder.ins().fmul(left, right),
            BinOp::Div => self.builder.ins().fdiv(left, right),
            BinOp::Mod => self
                .call_runtime(Runtime::FloatRem, &[left, right])?
                .ok_or_else(|| internal("float remainder returns a float"))?,
            _ => return Err(internal("a checked float operator is arithmetic")),
        })
    }

    /// Compare in BAML's total order on floats: IEEE order, except that NaN
    /// equals NaN and is greater than every number. Each operand comes with
    /// whether it can be NaN at all; the NaN tests of one that cannot are
    /// left out.
    fn float_comparison(
        &mut self,
        op: BinOp,
        (left, left_nan): (ir::Value, bool),
        (right, right_nan): (ir::Value, bool),
    ) -> ir::Value {
        let ins =
            |translator: &mut Self, condition, a, b| translator.builder.ins().fcmp(condition, a, b);
        match op {
            BinOp::Eq | BinOp::Ne => {
                let mut equal = ins(self, FloatCC::Equal, left, right);
                if left_nan && right_nan {
                    let left_is_nan = ins(self, FloatCC::Unordered, left, left);
                    let right_is_nan = ins(self, FloatCC::Unordered, right, right);
                    let both = self.builder.ins().band(left_is_nan, right_is_nan);
                    equal = self.builder.ins().bor(equal, both);
                }
                if op == BinOp::Eq {
                    equal
                } else {
                    self.builder.ins().bxor_imm(equal, 1)
                }
            }
            // Less also when only the right side is NaN.
            BinOp::Lt => {
                let less = ins(self, FloatCC::LessThan, left, right);
                if !right_nan {
                    return less;
                }
                let mut right_only = ins(self, FloatCC::Unordered, right, right);
                if left_nan {
                    let left_is_number = ins(self, FloatCC::Ordered, left, left);
                    right_only = self.builder.ins().band(right_only, left_is_number);
                }
                self.builder.ins().bor(less, right_only)
            }
            // Less or equal also whenever the right side is NaN.
            BinOp::Le => {
                let less_or_equal = ins(self, FloatCC::LessThanOrEqual, left, right);
                if !right_nan {
                    return less_or_equal;
                }
                let right_is_nan = ins(self, FloatCC::Unordered, right, right);
                self.builder.ins().bor(less_or_equal, right_is_nan)
            }
            // Greater also when only the left side is NaN.
            BinOp::Gt => {
                let greater = ins(self, FloatCC::GreaterThan, left, right);
                if !left_nan {
                    return greater;
                }
                let mut left_only = ins(self, FloatCC::Unordered, left, left);
                if right_nan {
                    let right_is_number = ins(self, FloatCC::Ordered, right, right);
                    left_only = self.builder.ins().band(left_only, right_is_number);
                }
                self.builder.ins().bor(greater, left_only)
            }
            // Greater or equal also whenever the left side is NaN.
            _ => {
                let greater_or_equal = ins(self, FloatCC::GreaterThanOrEqual, left, right);
                if !left_nan {
                    return greater_or_equal;
                }
                let left_is_nan = ins(self, FloatCC::Unordered, left, left);
                self.builder.ins().bor(greater_or_equal, left_is_nan)
            }
        }
    }

    fn unary(&mut self, op: UnaryOp, operand: &Operand<'db>) -> Result<ir::Value, CompileError> {
        let ty = self.operand_ty(operand)?;
        unary_typing(op, &ty).map_err(internal)?;
        let value = self.operand(operand)?;
        Ok(match (op, &ty) {
            (UnaryOp::Neg, NativeTy::Float) => self.builder.ins().fneg(value),
            (UnaryOp::Neg, _) => self.builder.ins().ineg(value),
            // False, null, zero and 0.0 are the falsy scalars; NaN is truthy.
            (UnaryOp::Truthy, NativeTy::Bool) => value,
            (UnaryOp::Truthy, NativeTy::Int) => {
                self.builder.ins().icmp_imm(IntCC::NotEqual, value, 0)
            }
            (UnaryOp::Truthy, NativeTy::Float) => {
                let zero = self.builder.ins().f64const(0.0);
                self.builder.ins().fcmp(FloatCC::NotEqual, value, zero)
            }
            (UnaryOp::Truthy, _) => self.bool_const(false),
            (UnaryOp::Not, NativeTy::Bool | NativeTy::Int) => {
                self.builder.ins().icmp_imm(IntCC::Equal, value, 0)
            }
            (UnaryOp::Not, NativeTy::Float) => {
                let zero = self.builder.ins().f64const(0.0);
                self.builder.ins().fcmp(FloatCC::Equal, value, zero)
            }
            (UnaryOp::Not, _) => self.bool_const(true),
        })
    }

    fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        arguments: &[Operand<'db>],
    ) -> Result<ir::Value, CompileError> {
        match (intrinsic, arguments) {
            (Intrinsic::ArrayPush, [array, item]) => {
                let NativeTy::Array(element) = self.operand_ty(array)? else {
                    return Err(internal("a checked push is on an array"));
                };
                let array = self.operand(array)?;
                let item = self.operand_as(item, &element)?;
                self.array_push(array, item, &element)
            }
            (Intrinsic::EnumEq | Intrinsic::EnumNe, [left, right]) => {
                let (left, right) = (self.operand(left)?, self.operand(right)?);
                let condition = if intrinsic == Intrinsic::EnumEq {
                    IntCC::Equal
                } else {
                    IntCC::NotEqual
                };
                Ok(self.builder.ins().icmp(condition, left, right))
            }
            (Intrinsic::StringCharCount, [string]) => {
                let string = self.operand(string)?;
                let lengths = self.builder.ins().load(
                    WORD,
                    array_access(),
                    string,
                    offset(layout::STRING_LENGTHS),
                );
                Ok(self
                    .builder
                    .ins()
                    .ushr_imm(lengths, i64::from(layout::STRING_CHAR_COUNT_SHIFT)))
            }
            (Intrinsic::ToString, [value]) => {
                let ty = self.operand_ty(value)?;
                let value = self.operand(value)?;
                let mutator = self.mutator();
                match ty {
                    NativeTy::String => Ok(value),
                    NativeTy::Int => self
                        .call_runtime(Runtime::StringFromInt, &[mutator, value])?
                        .ok_or_else(|| internal("int to string returns a string")),
                    NativeTy::Float => self
                        .call_runtime(Runtime::StringFromFloat, &[mutator, value])?
                        .ok_or_else(|| internal("float to string returns a string")),
                    NativeTy::Bool => {
                        let yes = self.backend.string_constant("true")?;
                        let no = self.backend.string_constant("false")?;
                        let (yes, no) = (self.data_address(yes), self.data_address(no));
                        Ok(self.builder.ins().select(value, yes, no))
                    }
                    _ => {
                        let null = self.backend.string_constant("null")?;
                        Ok(self.data_address(null))
                    }
                }
            }
            _ => Err(internal("a checked builtin call has its arguments")),
        }
    }

    /// `array.iter()`: the iterator is the array and a position in it.
    fn loop_start(
        &mut self,
        array: &Operand<'db>,
        iterator: Local,
        target: BlockId,
    ) -> Result<(), CompileError> {
        let array = self.operand(array)?;
        let (array_variable, index_variable) = self.iterator_variables[&iterator];
        self.builder.def_var(array_variable, array);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(index_variable, zero);
        let target = self.block(target);
        self.builder.ins().jump(target, &[]);
        Ok(())
    }

    /// `iterator.next()`: the end of the iteration is the position reaching
    /// the array's current length; otherwise the element there, and the
    /// position moves on.
    fn loop_step(
        &mut self,
        iterator: Local,
        item: Local,
        target: BlockId,
    ) -> Result<(), CompileError> {
        let element_ty = self
            .loops
            .item(item)
            .cloned()
            .ok_or_else(|| internal("a loop item has an element type"))?;
        let (array_variable, index_variable) = self.iterator_variables[&iterator];
        let (done_variable, element_variable) = self.item_variables[&item];
        let array = self.builder.use_var(array_variable);
        let index = self.builder.use_var(index_variable);
        let length =
            self.builder
                .ins()
                .load(WORD, array_access(), array, offset(layout::ARRAY_LENGTH));
        let done = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
        self.builder.def_var(done_variable, done);
        if let Some(variable) = element_variable {
            let nothing = match element_ty {
                NativeTy::Float => self.builder.ins().f64const(0.0),
                _ => self.builder.ins().iconst(clif_type(&element_ty), 0),
            };
            self.builder.def_var(variable, nothing);
        }

        let fetch = self.builder.create_block();
        let target = self.block(target);
        self.builder.ins().brif(done, target, &[], fetch, &[]);

        self.builder.switch_to_block(fetch);
        if let Some(variable) = element_variable {
            let storage =
                self.builder
                    .ins()
                    .load(WORD, array_access(), array, offset(layout::ARRAY_DATA));
            let scaled = self.builder.ins().ishl_imm(index, 3);
            let slot = self.builder.ins().iadd(storage, scaled);
            let element = self.load_slot(&element_ty, slot, 0);
            self.builder.def_var(variable, element);
        }
        let next = self.builder.ins().iadd_imm(index, 1);
        self.builder.def_var(index_variable, next);
        self.builder.ins().jump(target, &[]);
        Ok(())
    }

    fn terminator(&mut self, terminator: &Terminator<'db>) -> Result<(), CompileError> {
        if let (Some((array, iterator)), Terminator::VirtualCall { target, .. }) =
            (self.loops.start(terminator), terminator)
        {
            return self.loop_start(array, iterator, *target);
        }
        if let (Some((iterator, item)), Terminator::VirtualCall { target, .. }) =
            (self.loops.step(terminator), terminator)
        {
            return self.loop_step(iterator, item, *target);
        }
        match terminator {
            Terminator::Goto { target } => {
                let target = self.block(*target);
                self.builder.ins().jump(target, &[]);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let condition = self.operand(condition)?;
                let (then_block, else_block) = (self.block(*then_block), self.block(*else_block));
                self.builder
                    .ins()
                    .brif(condition, then_block, &[], else_block, &[]);
            }
            Terminator::NarrowBind {
                source,
                test,
                destination,
                then_block,
                else_block,
            } => {
                let source_ty = self.operand_ty(source)?;
                let wants_reference =
                    narrow_test_wants_reference(&source_ty, test).map_err(internal)?;
                let value = self.operand(source)?;
                // Bound on both edges; only the matching one reads it.
                if self.local_ty(*destination)?.is_ref() {
                    self.write_local(*destination, value);
                }
                let (then_block, else_block) = (self.block(*then_block), self.block(*else_block));
                let (on_reference, on_null) = if wants_reference {
                    (then_block, else_block)
                } else {
                    (else_block, then_block)
                };
                self.builder
                    .ins()
                    .brif(value, on_reference, &[], on_null, &[]);
            }
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                eval_rhs,
                join,
            } => {
                let operand_ty = self.operand_ty(operand)?;
                let destination_ty = self.place_ty(destination)?;
                let value = self.operand(operand)?;
                // The left operand is the whole result on the edge that
                // skips the right one. Storing it on both edges is harmless:
                // the other edge overwrites it.
                let result = self.coerce(value, &operand_ty, &destination_ty);
                self.write_place(destination, result)?;
                let (join, eval_rhs) = (self.block(*join), self.block(*eval_rhs));
                match kind {
                    ShortCircuitKind::And => {
                        self.builder.ins().brif(value, eval_rhs, &[], join, &[]);
                    }
                    ShortCircuitKind::Or => {
                        self.builder.ins().brif(value, join, &[], eval_rhs, &[]);
                    }
                    ShortCircuitKind::Coalesce => {
                        let presence = self.presence(value, &operand_ty);
                        self.builder.ins().brif(presence, join, &[], eval_rhs, &[]);
                    }
                }
            }
            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
                ..
            } => {
                let value = self.operand(discriminant)?;
                let mut switch = Switch::new();
                let mut seen = Vec::with_capacity(arms.len());
                for (key, target) in arms {
                    let SwitchKey::Int(key) = key else {
                        return Err(internal("a checked switch has int keys"));
                    };
                    // The first arm with a key wins, as in a chain of tests.
                    if !seen.contains(key) {
                        seen.push(*key);
                        switch.set_entry(u128::from(key.cast_unsigned()), self.block(*target));
                    }
                }
                let otherwise = self.block(*otherwise);
                switch.emit(&mut self.builder, value, otherwise);
            }
            Terminator::Return => {
                let value = self.read_local(Local(0));
                self.builder.ins().return_(&[value]);
            }
            Terminator::Unreachable => {
                self.builder.ins().trap(TRAP_UNREACHABLE);
            }
            Terminator::Call {
                callee,
                args,
                ntypeargs,
                destination,
                target,
                ..
            } => {
                let destination = place_local(destination).map_err(internal)?;
                let expected = self.local_ty(destination)?;
                let (result, produced) = match direct_callee(self.db, callee).map_err(internal)? {
                    Callee::Intrinsic(intrinsic) => {
                        let arguments = &args[*ntypeargs..];
                        let types = arguments
                            .iter()
                            .map(|argument| self.operand_ty(argument))
                            .collect::<Result<Vec<_>, _>>()?;
                        let produced = intrinsic
                            .result(&types)
                            .ok_or_else(|| internal("a checked builtin call has a result"))?;
                        (self.intrinsic(intrinsic, arguments)?, produced)
                    }
                    Callee::Value => {
                        let NativeTy::Function(signature) = narrowed(self.operand_ty(callee)?)
                        else {
                            return Err(internal("a checked call is of a function value"));
                        };
                        let closure = self.operand(callee)?;
                        let mut arguments = vec![closure];
                        for (argument, ty) in args.iter().zip(&signature.params) {
                            arguments.push(self.operand_as(argument, ty)?);
                        }
                        let code = self.builder.ins().load(
                            WORD,
                            closure_access(),
                            closure,
                            offset(layout::CLOSURE_CODE),
                        );
                        let clif = Backend::clif_signature(&signature, Entry::Value);
                        let clif = self.builder.import_signature(clif);
                        // The callee can collect, which moves storage.
                        self.forget_arrays();
                        let call = self.builder.ins().call_indirect(clif, code, &arguments);
                        self.read_worker();
                        (self.builder.inst_results(call)[0], signature.result)
                    }
                    Callee::Function(function) => {
                        let callee = self.declared(&FnKey::new(function, Entry::Direct))?;
                        let arguments = args
                            .iter()
                            .zip(&callee.signature.params)
                            .map(|(argument, ty)| self.operand_as(argument, ty))
                            .collect::<Result<Vec<_>, _>>()?;
                        let produced = callee.signature.result.clone();
                        let callee = self.function_ref(callee.id);
                        // The callee can collect, which moves storage.
                        self.forget_arrays();
                        let call = self.builder.ins().call(callee, &arguments);
                        self.read_worker();
                        (self.builder.inst_results(call)[0], produced)
                    }
                };
                let result = self.track(result, &produced);
                let result = self.coerce(result, &produced, &expected);
                self.write_local(destination, result);
                let target = self.block(*target);
                self.builder.ins().jump(target, &[]);
            }
            _ => return Err(internal("a checked terminator is supported")),
        }
        Ok(())
    }
}

/// Whether an operand of a float operation can be NaN: a float that is not a
/// constant number. An int converted to float never is.
fn may_be_nan(operand: &Operand<'_>, ty: &NativeTy<'_>) -> bool {
    match operand {
        Operand::Constant(Constant::Float(value)) => value.is_nan(),
        _ => *ty == NativeTy::Float,
    }
}
