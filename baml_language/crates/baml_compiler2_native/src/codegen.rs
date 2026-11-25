//! MIR to Cranelift IR to a native object file.
//!
//! Every function of the program becomes one object-local function; they
//! call each other with Cranelift's tail convention and the runtime with the
//! platform's C convention. Integer arithmetic wraps in two's complement.
//! A value whose static type fixes its tag is held without one: an `int` is
//! an `i64`, a `float` an `f64`, a `bool` or `null` an `i8`, and a string,
//! class instance or array a pointer to a heap object laid out as
//! [`crate::layout`] says. A value of any other type (an optional, a union,
//! an `unknown`, a function value) is a pair, one `i128` with its tag in
//! the low half and its payload in the high one: the two words a slot of a
//! heap object holds every value as. [`Translator::slot_store`] and
//! [`Translator::slot_load`] go between a slot and a value. Across a call
//! every value is those two words, each in a register or a word of the
//! stack of its own, whatever its type: see [`Backend::clif_signature`].
//!
//! One register is set aside for the whole program: it holds the address of
//! the running thread's mutator, whose first two words drive inline
//! allocation.
//!
//! Every call is a point where the collector may run and move objects.
//! References and pairs live across a call are spilled to frame slots that
//! a stack map names and are reloaded afterwards; the maps of all functions
//! are emitted as one table, [`STACK_MAPS_SYMBOL`].
//!
//! The object exports two functions: [`ENTRY_SYMBOL`], which calls the
//! entry function and prints its result through the runtime, and the task
//! shim every spawned task starts in ([`spawn`]).

use std::{collections::HashMap, fmt::Write as _};

use baml_compiler2_hir::loc::DeclRef;
use baml_compiler2_hir_ty::{extern_loc::ClassRef, layout::class_generic_params};
use baml_compiler2_mir::{
    AggregateKind, BinOp, BlockId, CellId, Constant, IndexKind, IntrinsicOp, Local, MirFunction,
    MirFunctionBody, Operand, Place, Rvalue, ShortCircuitKind, StatementKind, SwitchKey,
    Terminator, UnaryOp,
};
use baml_type::CallLayout;
use cranelift_codegen::{
    Context,
    ir::{
        self, AbiParam, AliasRegion, AtomicRmwOp, InstBuilder, MemFlags, StackSlotData,
        StackSlotKind, TrapCode,
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
    check::{
        Callee, Intrinsic, Operator, ScalarOp, TypeTestPlan, Virtual, body_of, call_slots,
        call_target, declared_layout, identified_arguments, is_intrinsic_call, narrow_bound,
        signature, type_test_plan, virtual_target,
    },
    dispatch::{Slot, Target},
    flow::{Flow, ProvenIndexes},
    layout,
    loops::{ArrayLoops, operand_local},
    narrow::TestKind,
    program::{Entry, FnKey, Program},
    types::{
        BinaryMode, Body, NativeTy, Signature, TypeArg, binary_typing, class_layout, is_comparison,
        local_ty, narrowed, operand_ty, place_local, place_ty, storage_ty, tuple_element,
        unary_typing,
    },
};

mod classes;
mod hooks;
mod json;
mod map;
mod native_call;
mod spawn;
mod string_slice;
mod unwind;

/// The symbol the runtime's process entry calls.
pub(crate) const ENTRY_SYMBOL: &str = "baml_native_entry";
/// The symbol of the table of stack maps the collector reads.
pub(crate) const STACK_MAPS_SYMBOL: &str = "baml_native_stack_maps";
/// The symbol of the table of functions: where each one's code is, and
/// where its shield ranges and telemetry slots are listed.
pub(crate) const FUNCTIONS_SYMBOL: &str = "baml_native_functions";
/// The version of the function table's format.
const FUNCTIONS_VERSION: u32 = 2;
/// The symbol of the program's table of types, which the runtime reads.
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

/// The type of a pair: a tag in the low half and a payload in the high
/// one.
const PAIR: ir::Type = types::I128;

fn clif_type(ty: &NativeTy<'_>) -> ir::Type {
    match ty {
        NativeTy::Int => types::I64,
        NativeTy::Float => types::F64,
        NativeTy::Bool | NativeTy::Null => types::I8,
        NativeTy::Enum(_)
        | NativeTy::String
        | NativeTy::Bigint
        | NativeTy::Bytes
        | NativeTy::Class(..)
        | NativeTy::Tuple(_)
        | NativeTy::Array(_)
        | NativeTy::Map(..)
        | NativeTy::Cell(_)
        | NativeTy::Future(_) => WORD,
        NativeTy::Function(_) | NativeTy::Union(_) | NativeTy::Unknown | NativeTy::Optional(_) => {
            PAIR
        }
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

/// The words of a slot other workers may reach are read and written with
/// atomic instructions, each sequentially consistent.
fn slot_atomic() -> MemFlags {
    MemFlags::trusted()
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

/// The bits of `value`, of a type held as one word, as a payload.
fn payload_of(builder: &mut FunctionBuilder<'_>, value: ir::Value, ty: &NativeTy<'_>) -> ir::Value {
    match ty {
        NativeTy::Float => builder.ins().bitcast(WORD, MemFlags::new(), value),
        NativeTy::Bool | NativeTy::Null => builder.ins().uextend(WORD, value),
        _ => value,
    }
}

/// The payload `bits` as a value of type `ty`, which is held as one word.
fn of_payload(builder: &mut FunctionBuilder<'_>, bits: ir::Value, ty: &NativeTy<'_>) -> ir::Value {
    match ty {
        NativeTy::Float => builder.ins().bitcast(types::F64, MemFlags::new(), bits),
        NativeTy::Bool | NativeTy::Null => builder.ins().ireduce(types::I8, bits),
        _ => bits,
    }
}

/// The tag of a value of type `ty` whose bits are its payload.
fn immediate_tag(ty: &NativeTy<'_>) -> Option<usize> {
    match ty {
        NativeTy::Null => Some(layout::TAG_NULL),
        NativeTy::Bool => Some(layout::TAG_BOOL),
        NativeTy::Int => Some(layout::TAG_INT),
        NativeTy::Float => Some(layout::TAG_FLOAT),
        _ => None,
    }
}

/// The type ids of the callables, a bit each: bit `id` is set when an
/// object of that type id has a call layout id in its header.
const CALLABLE_IDS: u64 = {
    assert!(layout::TAGS_END <= 64);
    let (mut ids, mut id) = (0, 0);
    while id < layout::TAGS_END {
        if layout::is_callable(id) {
            ids |= 1 << id;
        }
        id += 1;
    }
    ids
};

/// The tag of a value that points at the object at `object`, which is not
/// null, read from the object's header: what [`layout::tag_of`] makes of
/// it. Every tag compiled code reads from a header is read here.
///
/// The low half of a header is the type id and nothing else, so that half
/// is the whole tag of an object that is not a callable: the hash of a
/// string and the flags are in the other half. A callable's tag has its
/// call layout id too, so where the object `may_be_callable` the whole
/// word is read, and its payload kept only when the type id is a
/// callable's.
///
/// A tag read here is good until the next call: a collection can change
/// the type id of what a reference points at.
fn tag_of_object(
    builder: &mut FunctionBuilder<'_>,
    object: ir::Value,
    may_be_callable: bool,
) -> ir::Value {
    if !may_be_callable {
        return builder.ins().uload32(slot_access(), object, 0);
    }
    let header = builder.ins().load(WORD, slot_access(), object, 0);
    // A shift is by the low six bits of its count, which of a header are
    // those of the type id, and every type id is below 64.
    let callables = builder.ins().iconst(WORD, CALLABLE_IDS.cast_signed());
    let is_callable = builder.ins().ushr(callables, header);
    let is_callable = builder.ins().band_imm(is_callable, 1);
    let of_callable = builder.ins().iconst(WORD, imm(layout::CALLABLE_TAG_MASK));
    let of_other = builder.ins().iconst(WORD, imm(layout::TYPE_ID_MASK));
    let kept = builder.ins().select(is_callable, of_callable, of_other);
    builder.ins().band(header, kept)
}

/// The two words a slot of type `ty` holds for `value`: the tag and the
/// payload. A pair is its own two words. The tag of anything else is a
/// constant where every value of the type has the same one, and is read
/// from the object's header otherwise: a string may be a slice. `null`'s
/// payload is zero.
///
/// A tag read from a header is good until the next call.
fn slot_words(
    builder: &mut FunctionBuilder<'_>,
    ty: &NativeTy<'_>,
    value: ir::Value,
) -> (ir::Value, ir::Value) {
    let constant = match ty {
        _ if ty.is_pair() => return builder.ins().isplit(value),
        NativeTy::Null => {
            let null = builder.ins().iconst(WORD, imm(layout::TAG_NULL));
            return (null, null);
        }
        NativeTy::Cell(_) => layout::TAG_CELL,
        NativeTy::Array(_) => layout::TAG_ARRAY,
        NativeTy::Map(..) => layout::TAG_MAP,
        NativeTy::Future(_) => layout::TAG_FUTURE,
        NativeTy::Bigint => layout::TAG_BIGINT,
        NativeTy::Bytes => layout::TAG_BYTES,
        NativeTy::Tuple(_) => layout::TAG_TUPLE,
        NativeTy::Class(..) => layout::TAG_INSTANCE,
        NativeTy::Enum(_) => layout::TAG_ENUM_VARIANT,
        other => match immediate_tag(other) {
            Some(tag) => tag,
            // A string, which may be a slice.
            None => {
                debug_assert!(matches!(other, NativeTy::String));
                return (tag_of_object(builder, value, false), value);
            }
        },
    };
    let payload = payload_of(builder, value, ty);
    (builder.ins().iconst(WORD, imm(constant)), payload)
}

/// The value of type `ty` whose two words in a slot of that type are
/// `tag` and `payload`.
fn of_slot_words(
    builder: &mut FunctionBuilder<'_>,
    ty: &NativeTy<'_>,
    tag: ir::Value,
    payload: ir::Value,
) -> ir::Value {
    if ty.is_pair() {
        builder.ins().iconcat(tag, payload)
    } else {
        of_payload(builder, payload, ty)
    }
}

/// The pair `value` as a value of type `ty`.
fn of_pair(builder: &mut FunctionBuilder<'_>, ty: &NativeTy<'_>, value: ir::Value) -> ir::Value {
    if ty.is_pair() {
        return value;
    }
    let (tag, payload) = builder.ins().isplit(value);
    of_slot_words(builder, ty, tag, payload)
}

/// A function of the runtime library, called by symbol.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Runtime {
    PrintInt,
    PrintFloat,
    PrintBool,
    PrintString,
    PrintEnum,
    PanicDivisionByZero,
    PanicNegativeShift,
    PanicIndexOutOfBounds,
    PanicMapKeyNotFound,
    ErrorMapKeyNotFound,
    ErrorDivisionByZero,
    ErrorNegativeShift,
    ErrorIndexOutOfBounds,
    ErrorContext,
    Throw,
    Rethrow,
    Uncaught,
    FloatRem,
    PanicStackOverflow,
    PollSlow,
    AllocSlow,
    ArrayNew,
    ArrayPushSlow,
    ArraySort,
    BytesNew,
    BytesEq,
    StringConcat,
    StringEq,
    StringCmp,
    StringFromInt,
    StringFromFloat,
    StringFromAny,
    StringFromBigint,
    PrintBigint,
    BigintBinary,
    BigintCmp,
    BigintNeg,
    BigintToNanos,
    Sleep,
    Spawn,
    SpawnUnsupported,
    Await,
    AwaitError,
    TaskStart,
    TaskFinish,
    TaskFail,
    NativePoll,
    IoWait,
}

impl Runtime {
    fn symbol(self) -> &'static str {
        match self {
            Self::PrintInt => "baml_rt_print_int",
            Self::PrintFloat => "baml_rt_print_float",
            Self::PrintBool => "baml_rt_print_bool",
            Self::PrintString => "baml_rt_print_string",
            Self::PrintEnum => "baml_rt_print_enum",
            Self::PanicDivisionByZero => "baml_rt_panic_division_by_zero",
            Self::PanicNegativeShift => "baml_rt_panic_negative_shift",
            Self::PanicIndexOutOfBounds => "baml_rt_panic_index_out_of_bounds",
            Self::PanicMapKeyNotFound => "baml_rt_panic_map_key_not_found",
            Self::ErrorMapKeyNotFound => "baml_rt_error_map_key_not_found",
            Self::ErrorDivisionByZero => "baml_rt_error_division_by_zero",
            Self::ErrorNegativeShift => "baml_rt_error_negative_shift",
            Self::ErrorIndexOutOfBounds => "baml_rt_error_index_out_of_bounds",
            Self::ErrorContext => "baml_rt_error_context",
            Self::Throw => "baml_rt_throw",
            Self::Rethrow => "baml_rt_rethrow",
            Self::Uncaught => "baml_rt_uncaught",
            Self::FloatRem => "baml_rt_float_rem",
            Self::PanicStackOverflow => "baml_rt_panic_stack_overflow",
            Self::PollSlow => "baml_rt_poll_slow",
            Self::AllocSlow => "baml_rt_alloc_slow",
            Self::ArrayNew => "baml_rt_array_new",
            Self::ArrayPushSlow => "baml_rt_array_push_slow",
            Self::ArraySort => "baml_rt_array_sort",
            Self::BytesNew => "baml_rt_bytes_new",
            Self::BytesEq => "baml_rt_bytes_eq",
            Self::StringConcat => "baml_rt_string_concat",
            Self::StringEq => "baml_rt_string_eq",
            Self::StringCmp => "baml_rt_string_cmp",
            Self::StringFromInt => "baml_rt_string_from_int",
            Self::StringFromFloat => "baml_rt_string_from_float",
            Self::StringFromAny => "baml_rt_string_from_any",
            Self::StringFromBigint => "baml_rt_string_from_bigint",
            Self::PrintBigint => "baml_rt_print_bigint",
            Self::BigintBinary => "baml_rt_bigint_binary",
            Self::BigintCmp => "baml_rt_bigint_cmp",
            Self::BigintNeg => "baml_rt_bigint_neg",
            Self::BigintToNanos => "baml_rt_bigint_to_nanos",
            Self::Sleep => "baml_rt_sleep",
            Self::Spawn => "baml_rt_spawn",
            Self::SpawnUnsupported => "baml_rt_spawn_unsupported",
            Self::Await => "baml_rt_await",
            Self::AwaitError => "baml_rt_await_error",
            Self::TaskStart => "baml_rt_task_start",
            Self::TaskFinish => "baml_rt_task_finish",
            Self::TaskFail => "baml_rt_task_fail",
            Self::NativePoll => "baml_rt_native_poll",
            Self::IoWait => "baml_rt_io_wait",
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
            | Self::PrintEnum
            | Self::PrintBigint
            | Self::Sleep
            | Self::IoWait => (vec![word], vec![]),
            Self::BigintBinary | Self::NativePoll => {
                (vec![word, word, word, word], vec![word, word])
            }
            Self::BigintNeg => (vec![word, word], vec![word, word]),
            Self::BigintCmp => (vec![word, word, word], vec![word]),
            Self::PanicDivisionByZero | Self::PanicNegativeShift | Self::PanicStackOverflow => {
                (vec![word, word], vec![])
            }
            Self::PollSlow | Self::AllocSlow | Self::SpawnUnsupported => (vec![], vec![]),
            Self::Await => (vec![word], vec![]),
            Self::TaskStart => (vec![word], vec![word]),
            Self::TaskFinish | Self::Throw | Self::Rethrow => (vec![word, word, word], vec![]),
            Self::TaskFail | Self::Uncaught => (vec![word, word, word, word], vec![]),
            Self::Spawn => (vec![word], vec![word]),
            Self::AwaitError => (vec![word, word], vec![word, word]),
            Self::PrintFloat => (vec![float], vec![]),
            Self::PrintBool => (vec![byte], vec![]),
            Self::PanicIndexOutOfBounds | Self::ArraySort | Self::PanicMapKeyNotFound => {
                (vec![word, word], vec![])
            }
            Self::ErrorMapKeyNotFound | Self::BigintToNanos => (vec![word], vec![word]),
            Self::ErrorDivisionByZero | Self::ErrorNegativeShift => (vec![word, word], vec![word]),
            Self::ErrorIndexOutOfBounds | Self::StringFromAny => {
                (vec![word, word, word], vec![word])
            }
            Self::ErrorContext => (vec![word, word, word, word], vec![word]),
            Self::FloatRem => (vec![float, float], vec![float]),
            Self::StringFromInt | Self::StringFromBigint => (vec![word, word], vec![word]),
            Self::StringFromFloat => (vec![word, float], vec![word]),
            Self::ArrayNew => (vec![word, word], vec![word]),
            Self::BytesNew | Self::StringConcat => (vec![word, word, word], vec![word]),
            Self::ArrayPushSlow => (vec![word, word, word, word], vec![]),
            Self::StringEq | Self::BytesEq => (vec![word, word], vec![AbiParam::new(types::I8)]),
            Self::StringCmp => (vec![word, word], vec![word]),
        };
        // A slow path is a stub that preserves every register, so that
        // the call constrains nothing on the fast path around it. Such a
        // call returns nothing, and these take nothing either: an argument
        // register is one more the fast path would have to leave free.
        let call_conv = if self.preserves_registers() {
            CallConv::PreserveAll
        } else {
            call_conv
        };
        let mut signature = ir::Signature::new(call_conv);
        signature.params = params;
        signature.returns = returns;
        signature
    }

    fn preserves_registers(self) -> bool {
        matches!(self, Self::PollSlow | Self::AllocSlow)
    }
}

/// A runtime panic and the values its message is built from.
#[derive(Clone, Copy)]
enum Panic {
    DivisionByZero(ir::Value),
    NegativeShift(ir::Value),
    IndexOutOfBounds(ir::Value, ir::Value),
    MapKeyNotFound,
}

/// The stack maps of one function: for each call, the offset of its return
/// address in the function and the slots holding references or pairs
/// across it, which may be none. A slot is its offset from the stack
/// pointer, with [`layout::STACK_MAP_VALUE`] when it holds a pair.
type FunctionStackMaps = Vec<(u32, Vec<u32>)>;

struct Backend<'db> {
    module: ObjectModule,
    runtime: HashMap<Runtime, FuncId>,
    /// The program's table of types, which is defined last.
    types: DataId,
    /// The descriptions of the program's classes, and of the types calls
    /// of `baml.json` name: the section that ends the table of types.
    json: crate::json::JsonTypes<'db>,
    /// The class object of each class and enum met, and the number of
    /// each: its place here, which is its index.
    class_objects: Vec<classes::ClassObject<'db>>,
    class_ids: HashMap<classes::ClassLike<'db>, usize>,
    /// Whether the class objects are defined: no other can be met then.
    class_objects_defined: bool,
    /// The object of each package a class object names.
    packages: HashMap<String, DataId>,
    /// The type object of each type passed as a type argument, with where
    /// its description is.
    type_objects: HashMap<classes::TypeKey<'db>, (DataId, u32)>,
    /// The interfaces type tests ask about, each with a column of its own
    /// after the columns of the dispatched methods, in their order.
    interface_columns: Vec<baml_compiler2_mir::TyTemplateInterface>,
    /// The table of what each tag has in a column, for each column a call
    /// or a test reads by tag.
    tag_rows: HashMap<usize, DataId>,
    /// The classes the runtime makes instances of, once they are needed.
    well_known: Option<classes::WellKnown>,
    /// The call layouts met, each known by its number: its place here,
    /// counted from one. See [`Backend::layout_id`].
    layouts: Vec<CallLayout>,
    /// The closure standing for each function used as a value without
    /// capturing anything.
    function_values: HashMap<FuncId, DataId>,
    strings: HashMap<String, DataId>,
    /// The bytes of the byte string literals emitted so far.
    byte_strings: HashMap<Vec<u8>, DataId>,
    /// The `bigint` constants emitted so far.
    bigints: HashMap<num_bigint::BigInt, DataId>,
    stack_maps: Vec<(FuncId, FunctionStackMaps)>,
    /// Every function defined, in the order of their code, with its size
    /// in bytes.
    functions: Vec<(FuncId, u32)>,
    /// What the unwinder is told about them.
    unwind: unwind::Tables,
    /// The entries natives call, and the ones `async` natives are called
    /// through.
    hooks: hooks::Hooks<'db>,
    /// Whether the program calls what compares two values of any type,
    /// which reads the list of the classes with an `eq` of their own.
    compares_values: bool,
}

impl Backend<'_> {
    /// A backend for the machine the compiler runs on, using every CPU
    /// feature that machine has.
    fn new() -> Result<Self, CompileError> {
        let mut flags = settings::builder();
        flags.set("opt_level", "speed").map_err(internal)?;
        flags.set("is_pic", "true").map_err(internal)?;
        // The mutator's address lives in the pinned register.
        flags.set("enable_pinned_reg", "true").map_err(internal)?;
        // Every read of it wants the worker this code is running on now, so
        // an address off it can use the register itself.
        flags
            .set("fold_pinned_reg_reads", "true")
            .map_err(internal)?;
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
        let mut module = ObjectModule::new(builder);
        let types = module
            .declare_data(TYPES_SYMBOL, Linkage::Export, false, false)
            .map_err(internal)?;
        Ok(Self {
            module,
            runtime: HashMap::new(),
            types,
            json: crate::json::JsonTypes::new(),
            class_objects: Vec::new(),
            class_ids: HashMap::new(),
            class_objects_defined: false,
            packages: HashMap::new(),
            type_objects: HashMap::new(),
            interface_columns: Vec::new(),
            tag_rows: HashMap::new(),
            well_known: None,
            layouts: Vec::new(),
            function_values: HashMap::new(),
            strings: HashMap::new(),
            bigints: HashMap::new(),
            byte_strings: HashMap::new(),
            stack_maps: Vec::new(),
            functions: Vec::new(),
            unwind: unwind::Tables::default(),
            hooks: hooks::Hooks::default(),
            compares_values: false,
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
    /// entered as a bound method finds its receiver there. Then come
    /// `type_args` type objects, a word each, one per type parameter of a
    /// function that must be told its type arguments; then `values` values,
    /// each two words, its tag and its payload, whatever its type. The
    /// result is two words the same way. So every function of as many
    /// parameters is entered alike, by a call that names it, through a
    /// function value or through a row, and none of them has to know how
    /// the function holds what it is passed.
    fn clif_signature(values: usize, entry: Entry, type_args: usize) -> ir::Signature {
        let mut clif = ir::Signature::new(CallConv::Tail);
        let (closure, receiver) = match entry {
            Entry::Direct => (false, 0),
            Entry::Value => (true, 0),
            Entry::Bound => (true, 1),
        };
        clif.params = closure
            .then(|| AbiParam::new(WORD))
            .into_iter()
            .chain(std::iter::repeat_n(AbiParam::new(WORD), type_args))
            .chain(std::iter::repeat_n(
                AbiParam::new(WORD),
                2 * (values - receiver),
            ))
            .collect();
        clif.returns = vec![AbiParam::new(WORD); 2];
        clif
    }

    /// A reference to a function of this program or of the runtime, for use
    /// in `function`. Both end up in the one executable, so the reference is
    /// colocated: a call is a direct branch, where the module's own
    /// declaration would make every call of an imported function load its
    /// address first.
    fn function_ref(&self, id: FuncId, function: &mut ir::Function) -> ir::FuncRef {
        let declaration = self.module.declarations().get_function_decl(id);
        let signature = function.import_signature(declaration.signature.clone());
        let name = function.declare_imported_user_function(ir::UserExternalName {
            namespace: 0,
            index: id.as_u32(),
        });
        function.import_function(ir::ExtFuncData {
            name: ir::ExternalName::user(name),
            signature,
            colocated: true,
            patchable: false,
        })
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

    /// Read-only data of the object, aligned as an object of the heap is.
    fn define_data(
        &mut self,
        id: DataId,
        mut description: DataDescription,
        bytes: Vec<u8>,
    ) -> Result<(), CompileError> {
        description.define(bytes.into_boxed_slice());
        description.set_align(layout::VALUE as u64);
        self.module.define_data(id, &description).map_err(internal)
    }

    /// The number of a call layout: of the value slots a function's
    /// parameters read, or of those a call pushes. Every callable object
    /// has its function's in its header, and so in the tag of every value
    /// that refers to it, where a call through the value compares it with
    /// its own. No layout is numbered zero, which is what the header of a
    /// callable nothing here made says.
    fn layout_id(&mut self, layout: &CallLayout) -> Result<usize, CompileError> {
        let place = match self.layouts.iter().position(|known| known == layout) {
            Some(place) => place,
            None => {
                self.layouts.push(layout.clone());
                self.layouts.len() - 1
            }
        };
        if place + 1 > layout::HEADER_PAYLOAD_MASK {
            return Err(internal("more call layouts than a header can number"));
        }
        Ok(place + 1)
    }

    /// The header of a callable with the type id `tag` whose function has
    /// the call layout `layout`: also the tag of a value that refers to
    /// it.
    fn callable_header(&mut self, tag: usize, layout: &CallLayout) -> Result<usize, CompileError> {
        Ok(tag | self.layout_id(layout)? << layout::HEADER_PAYLOAD_SHIFT)
    }

    /// The closure that stands for a function used as a value: an object in
    /// the executable's data that captures nothing, with the header
    /// `header`.
    fn function_value(&mut self, function: FuncId, header: usize) -> Result<DataId, CompileError> {
        if let Some(id) = self.function_values.get(&function) {
            return Ok(*id);
        }
        let mut bytes = vec![0; layout::closure_size(0)];
        bytes[..layout::HEADER].copy_from_slice(&header.to_ne_bytes());
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

    /// A string object in the executable's data. The collector leaves
    /// objects outside the heap alone.
    fn string_constant(&mut self, text: &str) -> Result<DataId, CompileError> {
        if let Some(id) = self.strings.get(text) {
            return Ok(*id);
        }
        let lengths = text.len() | (text.chars().count() << layout::STRING_CHAR_COUNT_SHIFT);
        let mut bytes = vec![0; layout::string_size(text.len())];
        bytes[..layout::HEADER].copy_from_slice(&layout::TAG_STRING.to_ne_bytes());
        bytes[layout::STRING_LENGTHS..layout::STRING_TEXT].copy_from_slice(&lengths.to_ne_bytes());
        bytes[layout::STRING_TEXT..layout::STRING_TEXT + text.len()]
            .copy_from_slice(text.as_bytes());

        // Writable: a string's hash is cached in its header the first time
        // it is asked for, and the seed it is computed with is the
        // process's.
        let id = self
            .module
            .declare_anonymous_data(true, false)
            .map_err(internal)?;
        self.define_data(id, DataDescription::new(), bytes)?;
        self.strings.insert(text.to_owned(), id);
        Ok(id)
    }

    /// The bytes of a byte string literal in the executable's data, which
    /// the runtime copies into a new `uint8array` each time the literal is
    /// evaluated.
    fn bytes_constant(&mut self, bytes: &[u8]) -> Result<DataId, CompileError> {
        if let Some(id) = self.byte_strings.get(bytes) {
            return Ok(*id);
        }
        let id = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        // Never empty: a symbol needs a byte to be the address of.
        let mut data = bytes.to_vec();
        data.resize(bytes.len().max(1), 0);
        self.define_data(id, DataDescription::new(), data)?;
        self.byte_strings.insert(bytes.to_vec(), id);
        Ok(id)
    }

    /// A `bigint` object in the executable's data: see
    /// [`layout::BIGINT_LIMBS`].
    fn bigint_constant(&mut self, value: &num_bigint::BigInt) -> Result<DataId, CompileError> {
        if let Some(id) = self.bigints.get(value) {
            return Ok(*id);
        }
        let digits = value.to_signed_bytes_le();
        let limbs = digits.len().div_ceil(layout::WORD).max(1);
        let sign = if value.sign() == num_bigint::Sign::Minus {
            0xFF
        } else {
            0
        };
        let mut bytes = vec![sign; layout::bigint_size(limbs)];
        bytes[..layout::HEADER].copy_from_slice(&layout::TAG_BIGINT.to_ne_bytes());
        bytes[layout::BIGINT_LIMB_COUNT..layout::BIGINT_LIMBS]
            .copy_from_slice(&limbs.to_ne_bytes());
        bytes[layout::BIGINT_LIMBS..layout::BIGINT_LIMBS + digits.len()].copy_from_slice(&digits);
        let id = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        self.define_data(id, DataDescription::new(), bytes)?;
        self.bigints.insert(value.clone(), id);
        Ok(id)
    }

    /// Emit the function table. Every field is little-endian and the table
    /// is aligned to 8 bytes:
    ///
    /// ```text
    /// +0   u32  format version (2)
    /// +4   u32  F, the number of functions
    /// +8   u32  S, the number of shield ranges
    /// +12  u32  T, the number of telemetry slots
    /// +16  F function records of 24 bytes, in ascending address order:
    ///        +0   u64  address of the function's first instruction
    ///        +8   u32  size of its code in bytes
    ///        +12  u32  index of its first shield range
    ///        +16  u32  index of its first telemetry slot
    ///        +20  u32  offset in the table of the function's name
    ///      a function's ranges and slots end where the next function's
    ///      begin, the last function's at S and T
    /// then S shield ranges of 8 bytes, ascending within a function:
    ///        +0   u32  offset in the function of the range's first byte
    ///        +4   u32  offset in the function one past its last byte
    /// then T telemetry slots of 8 bytes, ascending within a function:
    ///        +0   u32  offset in the function of a call's return address
    ///        +4   u32  offset from the stack pointer of the slot there
    /// then each function's name: a u32 length and that many bytes of
    /// UTF-8, padded to 4 bytes
    /// ```
    ///
    /// A shield range is code of a `defer` body: the instructions generated
    /// for it, so a body is several ranges where moves and branches the
    /// register allocator placed fall between them. A call belongs to the
    /// range that holds the byte before its return address. No function has
    /// a telemetry slot yet: T is zero.
    fn define_functions(&mut self) -> Result<(), CompileError> {
        const RECORD: usize = 24;
        let functions = std::mem::take(&mut self.functions);
        let described = std::mem::take(&mut self.unwind.described);
        let mut description = DataDescription::new();
        let mut bytes: Vec<u8> = Vec::new();
        let count = u32::try_from(functions.len()).map_err(internal)?;
        let shields: usize = described.iter().map(|(ranges, _)| ranges.len()).sum();
        let shields = u32::try_from(shields).map_err(internal)?;
        for word in [FUNCTIONS_VERSION, count, shields, 0] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let mut name_offset = bytes.len() + RECORD * functions.len() + 8 * shields as usize;
        let mut first_shield = 0;
        for ((function, size), (ranges, name)) in functions.into_iter().zip(&described) {
            let function = self.module.declare_func_in_data(function, &mut description);
            description
                .write_function_addr(u32::try_from(bytes.len()).map_err(internal)?, function);
            bytes.extend_from_slice(&0_u64.to_le_bytes());
            let name_at = u32::try_from(name_offset).map_err(internal)?;
            for word in [size, first_shield, 0, name_at] {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
            first_shield += u32::try_from(ranges.len()).map_err(internal)?;
            name_offset += 4 + name.len().next_multiple_of(4);
        }
        for (start, end) in described.iter().flat_map(|(ranges, _)| ranges) {
            bytes.extend_from_slice(&start.to_le_bytes());
            bytes.extend_from_slice(&end.to_le_bytes());
        }
        for (_, name) in &described {
            let length = u32::try_from(name.len()).map_err(internal)?;
            bytes.extend_from_slice(&length.to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.resize(bytes.len().next_multiple_of(4), 0);
        }
        let id = self
            .module
            .declare_data(FUNCTIONS_SYMBOL, Linkage::Export, false, false)
            .map_err(internal)?;
        self.define_data(id, description, bytes)
    }

    /// Emit the table of every function's stack maps, in the format the
    /// runtime decodes: a count of functions, then for each its address, a
    /// count of call sites, and for each site its return address as an
    /// offset into the function, a count of slots, and the slots. Every
    /// entry is one word.
    ///
    /// Those are the calls across which a slot holds a reference. The
    /// others follow, for a runtime that checks that no compiled frame is
    /// without a map: a count of functions, then for each its address, a
    /// count of such calls, and the return address of each as an offset
    /// into the function. A runtime that does not check stops reading
    /// before them, so they cost a program's start nothing.
    fn define_stack_maps(&mut self) -> Result<(), CompileError> {
        let mut description = DataDescription::new();
        let mut bytes: Vec<u8> = Vec::new();
        let word = |bytes: &mut Vec<u8>, value: usize| {
            bytes.extend_from_slice(&value.to_ne_bytes());
        };
        let functions = std::mem::take(&mut self.stack_maps);
        for holding in [true, false] {
            let listed: Vec<_> = functions
                .iter()
                .filter_map(|(function, sites)| {
                    let sites: Vec<_> = sites
                        .iter()
                        .filter(|(_, slots)| slots.is_empty() != holding)
                        .collect();
                    (!sites.is_empty()).then_some((*function, sites))
                })
                .collect();
            word(&mut bytes, listed.len());
            for (function, sites) in listed {
                let function = self.module.declare_func_in_data(function, &mut description);
                description
                    .write_function_addr(u32::try_from(bytes.len()).map_err(internal)?, function);
                word(&mut bytes, 0);
                word(&mut bytes, sites.len());
                for (return_offset, slots) in sites {
                    word(&mut bytes, *return_offset as usize);
                    if holding {
                        word(&mut bytes, slots.len());
                        for slot in slots {
                            word(&mut bytes, *slot as usize);
                        }
                    }
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
    /// The value slots its parameters read.
    layout: CallLayout,
    /// How many type objects the function takes before its parameters: one
    /// per type parameter, or none.
    type_args: usize,
    /// Whether the function checks the stack limit on entry.
    checks_stack: bool,
}

/// The most locals a function that calls no other may have and still skip
/// the stack check. Its frame then holds the frame record, saved registers
/// and at most this many spilled values, well inside the space the limit
/// leaves below it.
const UNCHECKED_LEAF_LOCALS: usize = 12;

/// Whether `body` holds a `==` or a `!=` that is a call, where most are
/// compiled in place.
fn compares_out_of_line<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &MirFunctionBody<'db>,
    typed: &Body<'_, 'db>,
) -> bool {
    body.blocks
        .iter()
        .enumerate()
        .any(|(index, block)| match &block.terminator {
            Some(Terminator::Call {
                has_trace,
                callee,
                args,
                ntypeargs,
                ..
            }) if is_intrinsic_call(db, callee) => {
                typed.enter(index);
                let args = crate::types::passed(args, *has_trace);
                !matches!(
                    call_target(db, typed, callee, args, *ntypeargs),
                    Ok(Callee::Intrinsic(_)) | Err(_)
                )
            }
            _ => false,
        })
}

/// Whether a function must check the stack limit on entry: it calls
/// another compiled function, or its frame may not be small.
fn checks_stack<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &MirFunctionBody<'db>,
    loops: &ArrayLoops<'db>,
) -> bool {
    body.locals.len() > UNCHECKED_LEAF_LOCALS
        || body.blocks.iter().any(|block| match &block.terminator {
            Some(Terminator::Call { callee, .. }) => !is_intrinsic_call(db, callee),
            // An interface method call, unless it is a step of a loop over
            // an array, which is compiled in place.
            Some(terminator @ Terminator::VirtualCall { .. }) => {
                loops.start(terminator).is_none() && loops.step(terminator).is_none()
            }
            Some(
                Terminator::Goto { .. }
                | Terminator::Branch { .. }
                | Terminator::Switch { .. }
                | Terminator::Return
                | Terminator::Unreachable
                | Terminator::ShortCircuit { .. },
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
        let signature = signature(db, mir, body, &function.key.arguments).map_err(internal)?;
        let type_args = if identified_arguments(db, &function.key).contains(&true) {
            function.key.arguments.len()
        } else {
            0
        };
        let id = backend
            .module
            .declare_function(
                &symbol_name(index, &function.name),
                Linkage::Local,
                &Backend::clif_signature(signature.params.len(), function.key.entry, type_args),
            )
            .map_err(internal)?;
        let typed = Body::new(db, mir, body, &function.captures, &function.key);
        declared.insert(
            function.key.clone(),
            Declared {
                id,
                entry: function.key.entry,
                signature,
                layout: declared_layout(db, &function.key).map_err(internal)?,
                type_args,
                checks_stack: checks_stack(db, body, &ArrayLoops::of(db, &typed))
                    || compares_out_of_line(db, body, &typed),
            },
        );
        bodies.push((function, mir, typed));
    }
    hooks::declare(&mut backend, program, &declared)?;
    // The stub of each native a row enters, declared while stubs are still
    // to be compiled.
    for (_, methods) in &program.rows {
        for (_, code) in methods {
            if let crate::program::Code::Native(native) = code {
                backend.dispatched_stub(db, native)?;
            }
        }
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
        NativeTy::Bigint => Some(Runtime::PrintBigint),
        // An enum's variant is printed by name, which its object has.
        NativeTy::Enum(_) => Some(Runtime::PrintEnum),
        NativeTy::Null => None,
        NativeTy::Union(_)
        | NativeTy::Unknown
        | NativeTy::Bytes
        | NativeTy::Class(..)
        | NativeTy::Tuple(_)
        | NativeTy::Array(_)
        | NativeTy::Map(..)
        | NativeTy::Function(_)
        | NativeTy::Cell(_)
        | NativeTy::Future(_)
        | NativeTy::Optional(_) => {
            return Err(CompileError::EntryResultNotPrintable);
        }
    };

    let mut context = backend.module.make_context();
    let mut builder_context = FunctionBuilderContext::new();
    for (function, mir, body) in bodies {
        let this = &declared[&function.key];
        context.func.signature =
            Backend::clif_signature(this.signature.params.len(), this.entry, this.type_args);
        Translator::new(
            db,
            &mut backend,
            program,
            &declared,
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
    // Every task but the first starts in the task shim, which the runtime
    // knows by its symbol, as it knows the entry.
    let mut shim_signature = ir::Signature::new(backend.call_conv());
    shim_signature.params.push(AbiParam::new(WORD));
    let shim_id = backend
        .module
        .declare_function(spawn::TASK_SHIM, Linkage::Export, &shim_signature)
        .map_err(internal)?;
    context.func.signature = shim_signature;
    spawn::task_shim(&mut backend, &mut context.func, &mut builder_context)?;
    define(
        &mut backend,
        &mut context,
        shim_id,
        spawn::TASK_SHIM,
        &mut clif,
        &mut disassembly,
    )?;
    hooks::define_all(
        &mut backend,
        &mut context,
        &mut builder_context,
        &mut clif,
        &mut disassembly,
    )?;
    // Every class object is met by now, but for those the runtime knows
    // by position: their rows can be laid out.
    backend.well_known(db)?;
    backend.define_equals(db)?;
    backend.identify_described(db)?;
    backend.define_stack_maps()?;
    backend.define_unwind()?;
    backend.define_functions()?;
    backend.define_rows(db, program, &declared)?;
    backend.define_types(db)?;

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
    // Every call has an entry: the slots its stack map names, or none.
    // The maps are sorted by the offset of the return address.
    let maps = code.buffer.user_stack_maps();
    let mut mapped = 0;
    let sites: FunctionStackMaps = code
        .buffer
        .call_sites()
        .map(|site| {
            let slots = maps
                .binary_search_by_key(&site.ret_addr, |(return_offset, _, _)| *return_offset)
                .map_or_else(
                    |_| Vec::new(),
                    |index| {
                        mapped += 1;
                        maps[index]
                            .2
                            .entries()
                            .map(|(ty, slot)| {
                                if ty == PAIR {
                                    slot | table_word(layout::STACK_MAP_VALUE)
                                } else {
                                    slot
                                }
                            })
                            .collect()
                    },
                );
            (site.ret_addr, slots)
        })
        .collect();
    if mapped != maps.len() {
        return Err(internal(format!(
            "compiling `{name}`: a stack map is not of a call"
        )));
    }
    backend.stack_maps.push((id, sites));
    backend.unwind.record(id, name, code)?;
    backend.functions.push((id, code.buffer.total_size()));
    backend.module.clear_context(context);
    Ok(())
}

/// The exported function the program's first task starts in: run the entry
/// function, then print what it returned. The runtime's boot code calls it
/// with the worker in the pinned register, as it calls a task shim, and
/// reads the register again when it returns: the task may have stopped
/// since and gone on under another worker, so the worker it was passed is
/// not used.
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
    let callee = backend.function_ref(entry.id, builder.func);
    // Nothing caught what arrives here: the runtime reports it and ends the
    // process.
    let uncaught = builder.create_block();
    builder.append_block_param(uncaught, PAIR);
    builder.append_block_param(uncaught, WORD);
    builder.set_cold_block(uncaught);
    let (tag, payload) = unwind::try_call(
        &mut builder,
        &unwind::Called::Direct(callee),
        &[],
        &[unwind::Edge {
            tag: None,
            landing: uncaught,
            reads_context: true,
        }],
    );
    if let Some(print) = print {
        let result = of_slot_words(&mut builder, &entry.signature.result, tag, payload);
        let print = backend.function_ref(print, builder.func);
        builder.ins().call(print, &[result]);
    }
    builder.ins().return_(&[]);

    builder.switch_to_block(uncaught);
    let (error, trace) = {
        let arriving = builder.block_params(uncaught);
        (arriving[0], arriving[1])
    };
    let (tag, payload) = builder.ins().isplit(error);
    let worker = builder.ins().get_pinned_reg(WORD);
    let report = backend.runtime(Runtime::Uncaught)?;
    let report = backend.function_ref(report, builder.func);
    builder.ins().call(report, &[worker, tag, payload, trace]);
    builder.ins().trap(TRAP_AFTER_PANIC);
    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}

/// A captured parameter between its arrival and the making of its cell.
enum Waiting {
    /// Its value, in a variable unless it is `null`.
    Value(Option<Variable>),
    /// The pair the call passed, which may be the tag of an argument left
    /// to its default.
    Argument(Variable),
}

/// Translates one MIR function body into the Cranelift function it borrows.
struct Translator<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    backend: &'a mut Backend<'db>,
    program: &'a Program<'db>,
    declared: &'a HashMap<FnKey<'db>, Declared<'db>>,
    key: &'a FnKey<'db>,
    body: &'a Body<'a, 'db>,
    builder: FunctionBuilder<'a>,
    /// The closure this function was entered through, when it was.
    closure: Variable,
    /// The type object of each type argument, when the function is passed
    /// them.
    type_args: Vec<Variable>,
    /// The variable of each local that carries a value; `None` for a `null`
    /// local and for a local of a type outside the subset, which a checked
    /// body never reads or writes.
    variables: Vec<Option<Variable>>,
    /// The locals of a type held as one word whose variable holds a pair
    /// all the same: one that may be an argument left to its default,
    /// which only its tag says.
    tagged: Vec<bool>,
    /// For each captured parameter that may be left to its default,
    /// whether it was and nothing has been stored in its cell since. The
    /// cell itself never holds what the call passed for it.
    omitted_cells: HashMap<Local, Variable>,
    blocks: Vec<ir::Block>,
    function_refs: HashMap<FuncId, ir::FuncRef>,
    data_refs: HashMap<DataId, ir::GlobalValue>,
    loops: ArrayLoops<'db>,
    /// The array accesses a counted loop proves in range.
    proven: ProvenIndexes,
    /// The MIR block being translated.
    current: usize,
    /// The length and storage address already read from each array value in
    /// the code being generated; see [`Self::array_view`].
    array_views: HashMap<ir::Value, (ir::Value, ir::Value)>,
    /// The position behind each (index, length) pair already checked in
    /// that same code.
    checked_indexes: HashMap<(ir::Value, ir::Value), ir::Value>,
    /// The array and the index behind each array iterator local.
    iterator_variables: HashMap<Local, (Variable, Variable)>,
    /// The end-of-iteration flag and the element behind each local holding
    /// what `next()` answered. The element has no variable when it is `null`.
    item_variables: HashMap<Local, (Variable, Option<Variable>)>,
    /// The block a throw to each handler of this function jumps to, by the
    /// handler's MIR block; see [`unwind`].
    landings: HashMap<usize, ir::Block>,
    /// For each handler a call has an exception edge to, the handler
    /// around it in this function.
    chains: HashMap<usize, Option<BlockId>>,
}

/// A size, offset or count as the 32 bits a table stores it in.
fn table_word(value: usize) -> u32 {
    u32::try_from(value).expect("table entries fit 32 bits")
}

impl<'a, 'db> Translator<'a, 'db> {
    fn new(
        db: &'db dyn baml_compiler2_mir::Db,
        backend: &'a mut Backend<'db>,
        program: &'a Program<'db>,
        declared: &'a HashMap<FnKey<'db>, Declared<'db>>,
        body: &'a Body<'a, 'db>,
        function: &'a mut ir::Function,
        builder_context: &'a mut FunctionBuilderContext,
    ) -> Self {
        let mut builder = FunctionBuilder::new(function, builder_context);
        let mut tagged = vec![false; body.locals.len()];
        let variables = (0..body.locals.len())
            .map(|index| match storage_ty(db, body, Local(index)) {
                Ok(ty)
                    if body.may_be_omitted(Local(index))
                        && !ty.is_pair()
                        && !matches!(ty, NativeTy::Cell(_)) =>
                {
                    tagged[index] = true;
                    let variable = builder.declare_var(PAIR);
                    builder.declare_var_needs_stack_map(variable);
                    Some(variable)
                }
                Ok(NativeTy::Null) | Err(_) => None,
                Ok(ty) => {
                    let variable = builder.declare_var(clif_type(&ty));
                    if ty.is_traced() {
                        builder.declare_var_needs_stack_map(variable);
                    }
                    Some(variable)
                }
            })
            .collect();
        let blocks = body.blocks.iter().map(|_| builder.create_block()).collect();
        let loops = ArrayLoops::of(db, body);
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
                    if element.is_traced() {
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
            program,
            declared,
            key: body.key,
            body,
            builder,
            closure,
            type_args: Vec::new(),
            variables,
            tagged,
            omitted_cells: HashMap::new(),
            blocks,
            function_refs: HashMap::new(),
            data_refs: HashMap::new(),
            loops,
            proven: ProvenIndexes::default(),
            current: 0,
            array_views: HashMap::new(),
            checked_indexes: HashMap::new(),
            iterator_variables,
            item_variables,
            landings: HashMap::new(),
            chains: HashMap::new(),
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
        for _ in 0..self.declared(self.key)?.type_args {
            // A type object can be one of the heap, which a collection
            // moves.
            let type_arg = self.builder.declare_var(WORD);
            self.builder.declare_var_needs_stack_map(type_arg);
            let passed = arriving.next().expect("a type object per type parameter");
            self.builder.def_var(type_arg, passed);
            self.type_args.push(type_arg);
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
                let bound = self.builder.use_var(self.closure);
                self.pair_load(content, bound, layout::BOUND_RECEIVER)
            } else {
                // Every argument arrives as its tag and its payload.
                let tag = arriving.next().expect("a tag per argument");
                let payload = arriving.next().expect("a payload per argument");
                if self.body.may_be_omitted(local) {
                    // Kept as it arrived: the tag says whether the call
                    // left it out, and then the payload is nothing.
                    let arrived = self.pair(tag, payload);
                    if matches!(ty, NativeTy::Cell(_)) {
                        let waiting = self.builder.declare_var(PAIR);
                        self.builder.declare_var_needs_stack_map(waiting);
                        self.builder.def_var(waiting, arrived);
                        captured.push((local, content.clone(), Waiting::Argument(waiting)));
                    } else if let Some(variable) = self.variables[local.0] {
                        self.builder.def_var(variable, arrived);
                    }
                    continue;
                }
                of_slot_words(&mut self.builder, content, tag, payload)
            };
            if let NativeTy::Cell(content) = &ty {
                if **content != NativeTy::Null {
                    let waiting = self.builder.declare_var(clif_type(content));
                    if content.is_traced() {
                        self.builder.declare_var_needs_stack_map(waiting);
                    }
                    self.builder.def_var(waiting, value);
                    captured.push((local, (**content).clone(), Waiting::Value(Some(waiting))));
                } else {
                    captured.push((local, NativeTy::Null, Waiting::Value(None)));
                }
            } else {
                self.write_local(local, value);
            }
        }
        if checks_stack {
            self.check_stack()?;
        }
        for (local, content, waiting) in captured {
            let cell = match waiting {
                Waiting::Value(waiting) => {
                    let value = waiting.map(|waiting| self.builder.use_var(waiting));
                    self.new_cell(&content, value)?
                }
                Waiting::Argument(waiting) => self.cell_of_argument(local, &content, waiting)?,
            };
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
            if !self.body.is_reachable(index) {
                self.builder.ins().trap(TRAP_UNREACHABLE);
                continue;
            }
            self.body.enter(index);
            self.forget_arrays();
            self.shield(block.shielded);
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
        self.finish_landings()?;
        self.builder.seal_all_blocks();
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
                            Some(Terminator::Call {
                                callee,
                                args,
                                ntypeargs,
                                ..
                            }) => {
                                self.body.enter(block);
                                match call_target(self.db, self.body, callee, args, *ntypeargs) {
                                    Ok(Callee::Function(key)) => self
                                        .declared
                                        .get(&key)
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

    fn block(&self, id: BlockId) -> ir::Block {
        self.blocks[id.0]
    }

    fn local_ty(&self, local: Local) -> Result<NativeTy<'db>, CompileError> {
        local_ty(self.db, self.body, local).map_err(internal)
    }

    fn storage_ty(&self, local: Local) -> NativeTy<'db> {
        // A loop local has no type; nothing reads it as a value.
        storage_ty(self.db, self.body, local).unwrap_or(NativeTy::Null)
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

    /// Mark a reference or a pair so that it is spilled across calls and
    /// named in their stack maps.
    fn track(&mut self, value: ir::Value, ty: &NativeTy<'db>) -> ir::Value {
        if ty.is_traced() {
            self.builder.declare_value_needs_stack_map(value);
        }
        value
    }

    /// The pair of `tag` and `payload`.
    fn pair(&mut self, tag: ir::Value, payload: ir::Value) -> ir::Value {
        self.builder.ins().iconcat(tag, payload)
    }

    /// `value`, of type `ty`, as a pair, which the collector is told of.
    fn as_pair(&mut self, ty: &NativeTy<'db>, value: ir::Value) -> ir::Value {
        let pair = if ty.is_pair() {
            value
        } else {
            let (tag, payload) = slot_words(&mut self.builder, ty, value);
            self.builder.ins().iconcat(tag, payload)
        };
        self.builder.declare_value_needs_stack_map(pair);
        pair
    }

    /// The value of type `ty` every bit of which is zero: `0`, `0.0`,
    /// `false`, `null`, no object.
    fn zero(&mut self, ty: &NativeTy<'db>) -> ir::Value {
        match clif_type(ty) {
            PAIR => {
                let zero = self.builder.ins().iconst(WORD, 0);
                self.builder.ins().iconcat(zero, zero)
            }
            types::F64 => self.builder.ins().f64const(0.0),
            other => self.builder.ins().iconst(other, 0),
        }
    }

    /// The two words a call passes for a parameter it leaves to its
    /// default, whatever the parameter's type: the tag no value has, and a
    /// payload nothing reads.
    fn omitted_words(&mut self) -> (ir::Value, ir::Value) {
        (self.word_const(layout::TAG_OMITTED), self.word_const(0))
    }

    /// The words a call passes for parameters of the types `params`: the
    /// tag and the payload of each, in order. `args` has the argument of
    /// each, or none for a parameter the call does not name, which is
    /// passed as one it leaves to its default.
    ///
    /// Every argument is evaluated before any tag is made: a tag read from
    /// an object's header is good until the next call, and the call these
    /// are for is the next one.
    fn argument_words<'o>(
        &mut self,
        args: impl IntoIterator<Item = Option<&'o Operand<'db>>>,
        params: &[NativeTy<'db>],
    ) -> Result<Vec<ir::Value>, CompileError>
    where
        'db: 'o,
    {
        let mut values = Vec::with_capacity(params.len());
        for (argument, ty) in args.into_iter().zip(params) {
            values.push(match argument {
                Some(argument) if !crate::types::is_omitted(argument) => {
                    Some(self.operand_as(argument, ty)?)
                }
                _ => None,
            });
        }
        let mut words = Vec::with_capacity(2 * values.len());
        for (value, ty) in values.into_iter().zip(params) {
            let (tag, payload) = match value {
                Some(value) => slot_words(&mut self.builder, ty, value),
                None => self.omitted_words(),
            };
            words.extend([tag, payload]);
        }
        Ok(words)
    }

    /// Change the representation of `value` from `from` to `to`, which it is
    /// assignable to. A value held as one word becomes a pair where a pair
    /// is expected, with the tag its type gives it; a pair gives up its
    /// payload where it is known to hold a value of one type. Two types
    /// both held as one word, or both as a pair, hold a value the same
    /// way.
    fn coerce(&mut self, value: ir::Value, from: &NativeTy<'db>, to: &NativeTy<'db>) -> ir::Value {
        let held = match (from.is_pair(), to.is_pair()) {
            (false, true) => self.as_pair(from, value),
            (true, false) => of_pair(&mut self.builder, to, value),
            _ => return value,
        };
        // A new value, which the collector has to be told of like the one
        // it was made from.
        self.track(held, to)
    }

    /// Whether `value`, of static type `ty`, passes a type test decided as
    /// `plan` says.
    fn type_test(
        &mut self,
        value: ir::Value,
        ty: &NativeTy<'db>,
        plan: &TypeTestPlan<'db>,
    ) -> Result<ir::Value, CompileError> {
        let test = match plan {
            TypeTestPlan::Always(passes) => return Ok(self.bool_const(*passes)),
            TypeTestPlan::Present(wanted) => {
                let presence = self.presence(value);
                let condition = if *wanted {
                    IntCC::NotEqual
                } else {
                    IntCC::Equal
                };
                return Ok(self.builder.ins().icmp_imm(condition, presence, 0));
            }
            TypeTestPlan::Equals(literal) => {
                use crate::narrow::Value;
                return match literal {
                    Value::Int(wanted) => {
                        Ok(self.builder.ins().icmp_imm(IntCC::Equal, value, *wanted))
                    }
                    Value::Bool(wanted) => {
                        Ok(self
                            .builder
                            .ins()
                            .icmp_imm(IntCC::Equal, value, i64::from(*wanted)))
                    }
                    Value::String(text) => {
                        let wanted = self.backend.string_constant(text)?;
                        let wanted = self.data_address(wanted);
                        self.call_runtime(Runtime::StringEq, &[value, wanted])?
                            .ok_or_else(|| internal("string equality returns a bool"))
                    }
                };
            }
            TypeTestPlan::Header(test) => test,
        };
        debug_assert!(ty.is_pair(), "a test by tag is of a pair");
        let classes::Passing {
            tags,
            classes,
            interfaces,
            panics,
            null: null_passes,
        } = self.passing(test, true)?;
        // The tag is in a register: nothing is loaded to compare it.
        let (tag, payload) = self.builder.ins().isplit(value);
        let id = self.builder.ins().ireduce(types::I32, tag);
        let mut passes = null_passes.then(|| {
            self.builder
                .ins()
                .icmp_imm(IntCC::Equal, id, imm(layout::TAG_NULL))
        });
        let mut also = |translator: &mut Self, same: ir::Value| {
            passes = Some(match passes {
                Some(passes) => translator.builder.ins().bor(passes, same),
                None => same,
            });
        };
        if let TestKind::Argument(slot) = test {
            let same = self.is_of_type_argument(*slot, tag, payload)?;
            also(self, same);
        }
        for wanted in tags {
            let same = if wanted == layout::TAG_STRING {
                self.is_string_id(id)
            } else {
                self.builder.ins().icmp_imm(IntCC::Equal, id, imm(wanted))
            };
            also(self, same);
        }
        if !classes.is_empty() || !interfaces.is_empty() || panics {
            let same = self.is_of_classes(id, payload, &classes, &interfaces, panics)?;
            also(self, same);
        }
        Ok(match passes {
            Some(passes) => passes,
            // No type in the program passes.
            None => self.bool_const(false),
        })
    }

    /// Whether the value of tag `tag` and payload `payload` is of the type
    /// this function was passed for its type parameter `slot`. A type
    /// object whose tag word is not zero says it: a value is of the type
    /// when it has that tag and, where the object names a class object,
    /// is an instance or a variant of it. The runtime decides for any
    /// other, by the type's description.
    fn is_of_type_argument(
        &mut self,
        slot: usize,
        tag: ir::Value,
        payload: ir::Value,
    ) -> Result<ir::Value, CompileError> {
        let passed = self
            .type_args
            .get(slot)
            .ok_or_else(|| internal("a tested type argument has its type object"))?;
        let passed = self.builder.use_var(*passed);
        let flags = MemFlags::trusted();
        let wanted = self
            .builder
            .ins()
            .load(WORD, flags, passed, offset(layout::TYPE_TAG));
        let by_tag = self.builder.create_block();
        let by_class = self.builder.create_block();
        let compare = self.builder.create_block();
        let slow = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I8);
        self.builder.set_cold_block(slow);
        self.builder.ins().brif(wanted, by_tag, &[], slow, &[]);

        self.builder.switch_to_block(by_tag);
        // The tag a type object has for `string` is that of one holding
        // its text.
        let id = self.builder.ins().ireduce(types::I32, tag);
        let is_string = self.is_string_id(id);
        let flat = self
            .builder
            .ins()
            .iconst(types::I32, imm(layout::TAG_STRING));
        let id = self.builder.ins().select(is_string, flat, id);
        let wanted = self.builder.ins().ireduce(types::I32, wanted);
        let same = self.builder.ins().icmp(IntCC::Equal, id, wanted);
        let no = self.bool_const(false);
        self.builder
            .ins()
            .brif(same, by_class, &[], done, &[no.into()]);

        self.builder.switch_to_block(by_class);
        let class = self
            .builder
            .ins()
            .load(WORD, flags, passed, offset(layout::TYPE_CLASS));
        let yes = self.bool_const(true);
        self.builder
            .ins()
            .brif(class, compare, &[], done, &[yes.into()]);

        // A type object that names a class object has the tag of an
        // instance or of a variant: the payload is an object.
        self.builder.switch_to_block(compare);
        let own =
            self.builder
                .ins()
                .load(WORD, slot_access(), payload, offset(layout::INSTANCE_CLASS));
        let same = self.builder.ins().icmp(IntCC::Equal, own, class);
        self.builder.ins().jump(done, &[same.into()]);

        self.builder.switch_to_block(slow);
        let mutator = self.mutator();
        let is_type = self.import(
            "baml_rt_is_type",
            vec![AbiParam::new(WORD); 4],
            vec![AbiParam::new(types::I8)],
        )?;
        let call = self
            .builder
            .ins()
            .call(is_type, &[mutator, passed, tag, payload]);
        let decided = self.builder.inst_results(call)[0];
        self.builder.ins().jump(done, &[decided.into()]);

        self.builder.switch_to_block(done);
        Ok(self.builder.block_params(done)[0])
    }

    /// Whether the value whose tag has the type id `id` is an instance or
    /// a variant of one of the class objects `classes`, or is of a type
    /// with something in one of the columns `interfaces`, or, when
    /// `panics`, is a panic.
    ///
    /// An instance and a variant hold their class object where the other
    /// does, and no class object is both a class's and an enum's, so one
    /// comparison of that word decides either. A value that is neither has
    /// its row in the tables of tags.
    fn is_of_classes(
        &mut self,
        id: ir::Value,
        payload: ir::Value,
        classes: &[usize],
        interfaces: &[usize],
        panics: bool,
    ) -> Result<ir::Value, CompileError> {
        let from_instance = self.builder.ins().iadd_imm(id, -imm(layout::TAG_INSTANCE));
        let has_class = self.builder.ins().icmp_imm(
            IntCC::UnsignedLessThanOrEqual,
            from_instance,
            imm(layout::TAG_ENUM_VARIANT - layout::TAG_INSTANCE),
        );
        let by_class = self.builder.create_block();
        let by_tag = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I8);
        self.builder
            .ins()
            .brif(has_class, by_class, &[], by_tag, &[]);

        self.builder.switch_to_block(by_class);
        let mut passes = self.bool_const(false);
        let class =
            self.builder
                .ins()
                .load(WORD, slot_access(), payload, offset(layout::INSTANCE_CLASS));
        for wanted in classes {
            let wanted = self.backend.class_data(*wanted);
            let wanted = self.data_address(wanted);
            let same = self.builder.ins().icmp(IntCC::Equal, class, wanted);
            passes = self.builder.ins().bor(passes, same);
        }
        for column in interfaces {
            let held = self.builder.ins().load(
                WORD,
                MemFlags::trusted(),
                class,
                offset(layout::CLASS_METHODS + column * layout::WORD),
            );
            let holds = self.builder.ins().icmp_imm(IntCC::NotEqual, held, 0);
            passes = self.builder.ins().bor(passes, holds);
        }
        if panics {
            // Whether an instance is a panic is a flag of its own header:
            // a tag does not say.
            let header = self.builder.ins().load(WORD, slot_access(), payload, 0);
            let flag = self
                .builder
                .ins()
                .ushr_imm(header, i64::from(layout::HEADER_PANIC_BIT));
            let flag = self.builder.ins().band_imm(flag, 1);
            let flag = self.builder.ins().ireduce(types::I8, flag);
            passes = self.builder.ins().bor(passes, flag);
        }
        self.builder.ins().jump(done, &[passes.into()]);

        self.builder.switch_to_block(by_tag);
        let mut passes = self.bool_const(false);
        let at = self.builder.ins().uextend(WORD, id);
        for column in interfaces {
            let held = self.tag_row_word(*column, at)?;
            let holds = self.builder.ins().icmp_imm(IntCC::NotEqual, held, 0);
            passes = self.builder.ins().bor(passes, holds);
        }
        self.builder.ins().jump(done, &[passes.into()]);

        self.builder.switch_to_block(done);
        Ok(self.builder.block_params(done)[0])
    }

    /// What the tag whose type id is `id`, a word, has in `column`: the
    /// word of that number in the column's table of tags. The id is scaled
    /// to a word where the table is indexed, which is the load's own
    /// addressing mode.
    fn tag_row_word(&mut self, column: usize, id: ir::Value) -> Result<ir::Value, CompileError> {
        const _: () = assert!(layout::WORD == 1 << 3);
        let table = self.backend.tag_row(column)?;
        let table = self.data_address(table);
        let at = self.builder.ins().ishl_imm(id, 3);
        let at = self.builder.ins().iadd(table, at);
        Ok(self
            .builder
            .ins()
            .load(WORD, MemFlags::trusted().with_readonly(), at, 0))
    }

    /// Branch on the tag and the payload of a value whose type tag was
    /// read: to the first arm whose key is the value's type. An arm of a
    /// tag is found by the tag; an arm of a class by the index of the
    /// instance's class object, so an arm that takes every panic class
    /// takes each by its own.
    fn switch_on_tag(
        &mut self,
        tag: ir::Value,
        payload: ir::Value,
        arms: &[(SwitchKey<'db>, BlockId)],
        otherwise: BlockId,
    ) -> Result<(), CompileError> {
        // What each arm takes, first arm first.
        let mut taken: Vec<(classes::Taken, BlockId)> = Vec::new();
        for (key, target) in arms {
            let keys = match key {
                SwitchKey::Class(class) => vec![classes::Taken::Class(self.plain_class(*class)?)],
                SwitchKey::Int(tag) => {
                    let test = TestKind::of_tag(*tag)
                        .ok_or_else(|| internal("a checked type tag names a type"))?;
                    let passing = self.passing(&test, false)?;
                    let mut keys = Vec::new();
                    if passing.null {
                        keys.push(classes::Taken::Tag(layout::TAG_NULL));
                    }
                    for tag in passing.tags {
                        keys.push(classes::Taken::Tag(tag));
                        if tag == layout::TAG_STRING {
                            keys.push(classes::Taken::Tag(layout::TAG_STRING_SLICE));
                        }
                    }
                    keys.extend(passing.classes.into_iter().map(classes::Taken::Class));
                    if passing.panics {
                        let panics = self.panic_classes()?;
                        keys.extend(panics.into_iter().map(classes::Taken::Class));
                    }
                    keys
                }
            };
            for key in keys {
                if !taken.iter().any(|(seen, _)| *seen == key) {
                    taken.push((key, *target));
                }
            }
        }
        let otherwise = self.block(otherwise);
        let mut by_tag = Switch::new();
        let mut by_class = Switch::new();
        let mut instances = None;
        for (key, target) in taken {
            match key {
                classes::Taken::Tag(tag) => by_tag.set_entry(tag as u128, self.block(target)),
                classes::Taken::Class(index) => {
                    instances.get_or_insert_with(|| self.builder.create_block());
                    by_class.set_entry(index as u128, self.block(target));
                }
            }
        }
        if let Some(instances) = instances {
            by_tag.set_entry(layout::TAG_INSTANCE as u128, instances);
        }
        let id = self.builder.ins().ireduce(types::I32, tag);
        by_tag.emit(&mut self.builder, id, otherwise);
        if let Some(instances) = instances {
            self.builder.switch_to_block(instances);
            let class = self.builder.ins().load(
                WORD,
                slot_access(),
                payload,
                offset(layout::INSTANCE_CLASS),
            );
            let index = self.builder.ins().load(
                types::I32,
                MemFlags::trusted(),
                class,
                offset(layout::CLASS_INDEX),
            );
            by_class.emit(&mut self.builder, index, otherwise);
        }
        Ok(())
    }

    /// Whether the type id `id`, of a tag, is a string's: the two kinds of
    /// string have ids next to each other.
    fn is_string_id(&mut self, id: ir::Value) -> ir::Value {
        let from_first = self.builder.ins().iadd_imm(id, -imm(layout::TAG_STRING));
        self.builder.ins().icmp_imm(
            IntCC::UnsignedLessThan,
            from_first,
            imm(layout::STRING_TAGS_END - layout::TAG_STRING),
        )
    }

    /// Whether a pair holds a value, as a word that is zero for `null`:
    /// its tag.
    fn presence(&mut self, value: ir::Value) -> ir::Value {
        self.builder.ins().isplit(value).0
    }

    /// The type and byte offset of field `field` of the class instance a
    /// place of type `base` holds.
    fn field(&self, base: &Place, field: usize) -> Result<(NativeTy<'db>, usize), CompileError> {
        let NativeTy::Class(class, arguments) = narrowed(self.place_ty(base)?) else {
            return Err(internal("a checked field access is on a class instance"));
        };
        let layout = class_layout(self.db, class, &arguments).map_err(internal)?;
        Ok((layout[field].clone(), self.field_offset(class, field)))
    }

    /// The byte offset of field `field` of an instance of `class`, whose
    /// slots are one per type parameter of the class and then its fields.
    fn field_offset(&self, class: ClassRef<'db>, field: usize) -> usize {
        layout::field_offset(class_generic_params(self.db, class).len() + field)
    }

    /// The type and byte offset of the element the local `index` names of
    /// the tuple a place of type `base` holds.
    fn tuple_slot(
        &self,
        base: &Place,
        index: Local,
    ) -> Result<(NativeTy<'db>, usize), CompileError> {
        let NativeTy::Tuple(elements) = narrowed(self.place_ty(base)?) else {
            return Err(internal("a checked tuple index is on a tuple"));
        };
        let element = tuple_element(self.body, index)
            .filter(|element| *element < elements.len())
            .ok_or_else(|| internal("a checked tuple index is a constant"))?;
        Ok((elements[element].clone(), layout::field_offset(element)))
    }

    /// A new tuple of these element types holding these values.
    fn tuple(
        &mut self,
        elements: &[NativeTy<'db>],
        fields: &[Operand<'db>],
    ) -> Result<ir::Value, CompileError> {
        // Evaluated before allocating, as an instance's fields are.
        let values = fields
            .iter()
            .zip(elements)
            .map(|(field, ty)| self.operand_as(field, ty))
            .collect::<Result<Vec<_>, _>>()?;
        let object = self.allocate(layout::instance_size(elements.len()))?;
        let header = self.word_const(layout::TAG_TUPLE);
        self.builder.ins().store(slot_access(), header, object, 0);
        let count = self.word_const(elements.len());
        self.builder
            .ins()
            .store(slot_access(), count, object, offset(layout::TUPLE_COUNT));
        for (slot, (ty, value)) in elements.iter().zip(values).enumerate() {
            self.slot_init(ty, value, object, layout::field_offset(slot));
        }
        Ok(object)
    }

    fn read_local(&mut self, local: Local) -> ir::Value {
        let Some(variable) = self.variables[local.0] else {
            return self.null();
        };
        let held = self.builder.use_var(variable);
        if self.tagged[local.0] {
            let ty = self.storage_ty(local);
            of_pair(&mut self.builder, &ty, held)
        } else {
            held
        }
    }

    fn write_local(&mut self, local: Local, value: ir::Value) {
        let Some(variable) = self.variables[local.0] else {
            return;
        };
        let held = if self.tagged[local.0] {
            let ty = self.storage_ty(local);
            let (tag, payload) = slot_words(&mut self.builder, &ty, value);
            self.pair(tag, payload)
        } else {
            value
        };
        self.builder.def_var(variable, held);
    }

    /// The variable of a local that may hold an argument left to its
    /// default, which holds a pair whatever the local's type: the value
    /// with its tag, or the tag no value has.
    fn omissible(&self, local: Local) -> Option<Variable> {
        let held_so = self.tagged[local.0] || self.storage_ty(local).is_pair();
        self.variables[local.0].filter(|_| self.body.may_be_omitted(local) && held_so)
    }

    /// `operand` as a local that may be an omitted argument holds it, when
    /// it is the omitted argument itself or such a local.
    fn held_omissible(&mut self, operand: &Operand<'db>) -> Option<ir::Value> {
        if crate::types::is_omitted(operand) {
            let (tag, payload) = self.omitted_words();
            return Some(self.pair(tag, payload));
        }
        let variable = self.omissible(operand_local(operand)?)?;
        Some(self.builder.use_var(variable))
    }

    /// Whether `left`, a local or what a local's cell holds, is an
    /// argument the call left to its default.
    fn is_omitted(&mut self, left: &Operand<'db>) -> Result<ir::Value, CompileError> {
        let (Operand::Copy(place) | Operand::Move(place)) = left else {
            return Err(internal("a checked test for a default is of a local"));
        };
        match place {
            Place::Local(local) => {
                let variable = self
                    .omissible(*local)
                    .ok_or_else(|| internal("a local tested for a default keeps its tag"))?;
                let held = self.builder.use_var(variable);
                let (tag, _) = self.builder.ins().isplit(held);
                Ok(self
                    .builder
                    .ins()
                    .icmp_imm(IntCC::Equal, tag, imm(layout::TAG_OMITTED)))
            }
            // Only a parameter's cell is ever made for an argument.
            Place::Deref(CellId::Local(local)) => Ok(match self.omitted_cells.get(local) {
                Some(omitted) => self.builder.use_var(*omitted),
                None => self.bool_const(false),
            }),
            _ => Err(internal("a checked test for a default is of a local")),
        }
    }

    /// The pair that refers to the object at `pointer`, an object of any
    /// type, or `null` when `pointer` is null: its tag is read from the
    /// object's header.
    fn pair_of_pointer(&mut self, pointer: ir::Value) -> ir::Value {
        let read = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, WORD);
        let null = self.word_const(layout::TAG_NULL);
        self.builder
            .ins()
            .brif(pointer, read, &[], done, &[null.into()]);
        self.builder.seal_block(read);
        self.builder.switch_to_block(read);
        let tag = tag_of_object(&mut self.builder, pointer, true);
        self.builder.ins().jump(done, &[tag.into()]);
        self.builder.seal_block(done);
        self.builder.switch_to_block(done);
        let tag = self.builder.block_params(done)[0];
        self.pair(tag, pointer)
    }

    /// The address of the slot at `base + bytes`: of its tag word.
    fn slot_address(&mut self, base: ir::Value, bytes: usize) -> ir::Value {
        if bytes == 0 {
            base
        } else {
            self.builder.ins().iadd_imm(base, imm(bytes))
        }
    }

    /// Take the busy bit of the slot whose tag word is at `slot`, and
    /// return the tag as it was, the bit clear. One atomic or sets the bit
    /// and says whether another worker had it, in which case nothing
    /// changed and the or is tried again. From here to the store of a tag
    /// that gives the bit up, [`Self::slot_store_words`] and
    /// [`Self::slot_load_words`] emit no call, so no collection sees the
    /// bit.
    fn slot_lock(&mut self, slot: ir::Value) -> ir::Value {
        const _: () = assert!(layout::HEADER_BUSY == 1 << 63);
        let busy = self.builder.ins().iconst(WORD, i64::MIN);
        let acquire = self.builder.create_block();
        let held = self.builder.create_block();
        self.builder.ins().jump(acquire, &[]);
        self.builder.switch_to_block(acquire);
        let old = self
            .builder
            .ins()
            .atomic_rmw(WORD, slot_atomic(), AtomicRmwOp::Or, slot, busy);
        let taken = self.builder.ins().band(old, busy);
        self.builder.ins().brif(taken, acquire, &[], held, &[]);
        self.builder.seal_block(acquire);
        self.builder.seal_block(held);
        self.builder.switch_to_block(held);
        old
    }

    /// The two words of the slot at `base + bytes`, read under its busy
    /// bit: a pair some store wrote whole. The tag is stored back as it
    /// was, which gives the bit up.
    fn slot_load_words(&mut self, base: ir::Value, bytes: usize) -> (ir::Value, ir::Value) {
        let slot = self.slot_address(base, bytes);
        let tag = self.slot_lock(slot);
        let at = self.slot_address(slot, layout::VALUE_PAYLOAD);
        let payload = self.builder.ins().atomic_load(WORD, slot_atomic(), at);
        self.builder.ins().atomic_store(slot_atomic(), tag, slot);
        (tag, payload)
    }

    /// Load the value of type `ty` that the slot at `base + bytes` holds.
    /// With [`Self::slot_store`] and [`Self::slot_init`], the only code
    /// that reads or writes a slot of the heap: a field, an element, a
    /// map's value, a cell, a capture.
    ///
    /// A pair is read under the slot's busy bit. A type held as one word
    /// fixes the slot's type id, so only the payload is read, with one
    /// atomic load: whichever store it follows, the word is a payload of
    /// that type.
    fn slot_load(&mut self, ty: &NativeTy<'db>, base: ir::Value, bytes: usize) -> ir::Value {
        if *ty == NativeTy::Null {
            return self.null();
        }
        let value = if ty.is_pair() {
            let (tag, payload) = self.slot_load_words(base, bytes);
            self.pair(tag, payload)
        } else {
            let at = self.slot_address(base, bytes + layout::VALUE_PAYLOAD);
            let payload = self.builder.ins().atomic_load(WORD, slot_atomic(), at);
            of_payload(&mut self.builder, payload, ty)
        };
        self.track(value, ty)
    }

    /// Store `value`, of type `ty`, in the slot at `base + bytes`, which
    /// other workers may reach.
    ///
    /// A slot declared exactly `int`, `float` or `bool` got its tag when it
    /// was made and keeps it, so the store is one atomic store of the
    /// payload. Any other store writes both words under the busy bit.
    fn slot_store(&mut self, ty: &NativeTy<'db>, value: ir::Value, base: ir::Value, bytes: usize) {
        if matches!(ty, NativeTy::Int | NativeTy::Float | NativeTy::Bool) {
            let payload = payload_of(&mut self.builder, value, ty);
            let at = self.slot_address(base, bytes + layout::VALUE_PAYLOAD);
            self.builder.ins().atomic_store(slot_atomic(), payload, at);
            return;
        }
        let words = slot_words(&mut self.builder, ty, value);
        self.slot_store_words(words, base, bytes);
    }

    /// Store the two words [`slot_words`] gave for a value in the slot at
    /// `base + bytes` under its busy bit: the payload, then the tag, which
    /// gives the bit up.
    fn slot_store_words(
        &mut self,
        (tag, payload): (ir::Value, ir::Value),
        base: ir::Value,
        bytes: usize,
    ) {
        let slot = self.slot_address(base, bytes);
        self.slot_lock(slot);
        let at = self.slot_address(slot, layout::VALUE_PAYLOAD);
        self.builder.ins().atomic_store(slot_atomic(), payload, at);
        self.builder.ins().atomic_store(slot_atomic(), tag, slot);
    }

    /// Give the slot at `base + bytes` its first value, `value`, of type
    /// `ty`: both words, with plain stores. Only for a slot of an object
    /// made here whose address has been stored nowhere yet, so that no
    /// other worker can reach it.
    fn slot_init(&mut self, ty: &NativeTy<'db>, value: ir::Value, base: ir::Value, bytes: usize) {
        let (tag, payload) = slot_words(&mut self.builder, ty, value);
        self.builder
            .ins()
            .store(slot_access(), tag, base, offset(bytes + layout::VALUE_TAG));
        self.builder.ins().store(
            slot_access(),
            payload,
            base,
            offset(bytes + layout::VALUE_PAYLOAD),
        );
    }

    /// Load the value of type `ty` from the slot at `base + bytes`, which
    /// holds it as a pair whatever its type: what wrote the slot knew the
    /// value by another type than what reads it. A bound method's
    /// receiver is held so.
    fn pair_load(&mut self, ty: &NativeTy<'db>, base: ir::Value, bytes: usize) -> ir::Value {
        let (tag, payload) = self.slot_load_words(base, bytes);
        let value = of_slot_words(&mut self.builder, ty, tag, payload);
        self.track(value, ty)
    }

    /// The length of `array` and the address of its storage object, which
    /// for a `uint8array` is at another offset.
    ///
    /// Both are read once and reused until something that could change them
    /// or move the storage: a call, an append, or the end of the MIR block.
    /// Cranelift merges such loads only inside one of its own blocks, and
    /// every bounds check ends one.
    fn array_view(&mut self, array: ir::Value, is_bytes: bool) -> (ir::Value, ir::Value) {
        const _: () = assert!(layout::BYTES_LENGTH == layout::ARRAY_LENGTH);
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
        let storage = if is_bytes {
            layout::BYTES_PTR
        } else {
            layout::ARRAY_PTR
        };
        let data = self
            .builder
            .ins()
            .load(WORD, array_access(), array, offset(storage));
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
        // An index known not to be negative is its own position.
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
        if known.is_some_and(|index| index >= 0) {
            let outside = self
                .builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
            self.panic_if(outside, Panic::IndexOutOfBounds(index, length))?;
            return Ok(index);
        }
        // One unsigned comparison passes every index that names an element
        // from the front. The rest, out of line, are counted back from the
        // end when negative: the length added to an index that is out of
        // range either way leaves it out of range, unsigned.
        let inside = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, index, length);
        let rare = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        self.builder.set_cold_block(rare);
        self.builder
            .ins()
            .brif(inside, done, &[index.into()], rare, &[]);
        self.builder.seal_block(rare);

        self.builder.switch_to_block(rare);
        let from_end = self.builder.ins().iadd(index, length);
        let outside = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, from_end, length);
        self.panic_if(outside, Panic::IndexOutOfBounds(index, length))?;
        self.builder.ins().jump(done, &[from_end.into()]);
        self.builder.seal_block(done);

        self.builder.switch_to_block(done);
        Ok(self.builder.block_params(done)[0])
    }

    /// Where element `index` of `array` is, after checking the index: its
    /// address less the offset of the first element in the storage
    /// ([`layout::STORAGE_ELEMENTS`], or [`layout::BYTES_STORAGE_DATA`] of a
    /// `uint8array`). Every element access goes through here. An element
    /// is a value, or a byte of a `uint8array`.
    fn element(
        &mut self,
        array: ir::Value,
        index: ir::Value,
        is_bytes: bool,
    ) -> Result<ir::Value, CompileError> {
        let shift = Self::element_shift(is_bytes);
        let (length, data) = self.array_view(array, is_bytes);
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
        let scaled = self.builder.ins().ishl_imm(index, shift);
        Ok(self.builder.ins().iadd(data, scaled))
    }

    /// The power of two the size of an element is.
    fn element_shift(is_bytes: bool) -> i64 {
        const _: () = assert!(layout::VALUE == 1 << 4);
        if is_bytes { 0 } else { 4 }
    }

    /// Where the element of `array`, read from `base`, that the local
    /// `index` names is, as [`Self::element`] gives it, and whether the
    /// element is a byte: `base` is a `uint8array`.
    fn indexed(
        &mut self,
        base: &Place,
        array: ir::Value,
        index: Local,
    ) -> Result<(ir::Value, bool), CompileError> {
        let is_byte = narrowed(self.place_ty(base)?) == NativeTy::Bytes;
        let position = self.read_local(index);
        if let Place::Local(base) = base
            && self.proven.holds(self.current, *base, index)
        {
            // A counted loop over this array: the index names an element.
            let (_, data) = self.array_view(array, is_byte);
            let scaled = self
                .builder
                .ins()
                .ishl_imm(position, Self::element_shift(is_byte));
            return Ok((self.builder.ins().iadd(data, scaled), is_byte));
        }
        Ok((self.element(array, position, is_byte)?, is_byte))
    }

    /// The object the place `place` refers to, where MIR projects through
    /// it: an optional the checker proved is not `null` is read as what it
    /// holds.
    fn read_object(&mut self, place: &Place) -> Result<ir::Value, CompileError> {
        let ty = self.place_ty(place)?;
        let value = self.read_place(place)?;
        let held = narrowed(ty.clone());
        Ok(self.coerce(value, &ty, &held))
    }

    /// [`Self::read_object`] of what an operand names.
    fn operand_object(&mut self, operand: &Operand<'db>) -> Result<ir::Value, CompileError> {
        let ty = self.operand_ty(operand)?;
        let value = self.operand(operand)?;
        let held = narrowed(ty.clone());
        Ok(self.coerce(value, &ty, &held))
    }

    fn read_place(&mut self, place: &Place) -> Result<ir::Value, CompileError> {
        match place {
            Place::Local(local) => {
                // The variable holds a union as one; the block may know
                // which member it is.
                let kept = self.storage_ty(*local);
                let known = self.local_ty(*local)?;
                let value = self.read_local(*local);
                let value = self.coerce(value, &kept, &known);
                Ok(self.track(value, &known))
            }
            Place::Field { base, field } => {
                let (ty, bytes) = self.field(base, *field)?;
                let object = self.read_object(base)?;
                Ok(self.slot_load(&ty, object, bytes))
            }
            Place::Index {
                base,
                index,
                kind: IndexKind::Tuple,
            } => {
                let (ty, bytes) = self.tuple_slot(base, *index)?;
                let tuple = self.read_object(base)?;
                Ok(self.slot_load(&ty, tuple, bytes))
            }
            Place::Index {
                base,
                index,
                kind: IndexKind::Map,
            } => {
                let map_ty = self.place_ty(base)?;
                let map = self.read_object(base)?;
                let key = self.read_local(*index);
                self.map_index(map, &map_ty, key)
            }
            Place::Index { base, index, .. } => {
                let ty = self.place_ty(place)?;
                let array = self.read_object(base)?;
                let (element, is_byte) = self.indexed(base, array, *index)?;
                if is_byte {
                    return Ok(self.builder.ins().uload8(
                        types::I64,
                        slot_access(),
                        element,
                        offset(layout::BYTES_STORAGE_DATA),
                    ));
                }
                Ok(self.slot_load(&ty, element, layout::STORAGE_ELEMENTS))
            }
            Place::Capture(index) => {
                let ty = self.place_ty(place)?;
                let closure = self.builder.use_var(self.closure);
                Ok(self.slot_load(
                    &ty,
                    closure,
                    layout::CLOSURE_CAPTURES + index * layout::VALUE,
                ))
            }
            Place::Deref(cell) => {
                let ty = self.place_ty(place)?;
                let cell = self.cell(*cell)?;
                Ok(self.slot_load(&ty, cell, layout::CELL_VALUE))
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
        let cell = self.allocate(layout::CELL_SIZE)?;
        let header = self.word_const(layout::TAG_CELL);
        self.builder.ins().store(slot_access(), header, cell, 0);
        match value {
            Some(value) => self.slot_init(content, value, cell, layout::CELL_VALUE),
            // A slot declared `int`, `float` or `bool` is later stored to
            // by its payload alone, so it has that tag from the start.
            None if immediate_tag(content).is_some() => {
                let zero = self.zero(content);
                self.slot_init(content, zero, cell, layout::CELL_VALUE);
            }
            None => {
                let null = self.null();
                self.slot_init(&NativeTy::Null, null, cell, layout::CELL_VALUE);
            }
        }
        Ok(cell)
    }

    /// A new cell for the captured parameter `local`, whose content is of
    /// type `content` and may have been left to its default: `waiting`
    /// holds the argument as it arrived. An omitted argument is no value,
    /// and no slot of the heap holds one: the cell gets the zero of its
    /// type in its place, and a variable keeps whether it was omitted for
    /// the test that puts the default there.
    fn cell_of_argument(
        &mut self,
        local: Local,
        content: &NativeTy<'db>,
        waiting: Variable,
    ) -> Result<ir::Value, CompileError> {
        let arrived = self.builder.use_var(waiting);
        let (tag, _) = self.builder.ins().isplit(arrived);
        let was_omitted = self
            .builder
            .ins()
            .icmp_imm(IntCC::Equal, tag, imm(layout::TAG_OMITTED));
        let omitted = self.builder.declare_var(types::I8);
        self.builder.def_var(omitted, was_omitted);
        self.omitted_cells.insert(local, omitted);

        let cell = self.allocate(layout::CELL_SIZE)?;
        let header = self.word_const(layout::TAG_CELL);
        self.builder.ins().store(slot_access(), header, cell, 0);
        // Read again: the allocation may have moved what it refers to.
        let arrived = self.builder.use_var(waiting);
        let (tag, payload) = self.builder.ins().isplit(arrived);
        // A slot declared `int`, `float` or `bool` keeps that tag.
        let zero_tag = self.word_const(immediate_tag(content).unwrap_or(layout::TAG_NULL));
        let zero = self.word_const(0);
        let tag = self.builder.ins().select(was_omitted, zero_tag, tag);
        let payload = self.builder.ins().select(was_omitted, zero, payload);
        for (word, at) in [(tag, layout::VALUE_TAG), (payload, layout::VALUE_PAYLOAD)] {
            self.builder
                .ins()
                .store(slot_access(), word, cell, offset(layout::CELL_VALUE + at));
        }
        Ok(cell)
    }

    /// A new closure over the cells `captured`, each with its type, entered
    /// at `code`: the function value, a pair.
    fn closure(
        &mut self,
        code: &FnKey<'db>,
        captured: &[(ir::Value, NativeTy<'db>)],
    ) -> Result<ir::Value, CompileError> {
        let closure = self.allocate(layout::closure_size(captured.len()))?;
        let tag = self.callable(closure, layout::TAG_CLOSURE, code, captured.len())?;
        for (index, (value, ty)) in captured.iter().enumerate() {
            self.slot_init(
                ty,
                *value,
                closure,
                layout::CLOSURE_CAPTURES + index * layout::VALUE,
            );
        }
        Ok(self.pair(tag, closure))
    }

    /// A new bound method: the method entered at `code`, with the value
    /// `receiver`, of type `ty`, it is called on. The receiver is kept as
    /// a pair: the method reads it as its own first parameter's type.
    fn bound_method(
        &mut self,
        code: &FnKey<'db>,
        receiver: ir::Value,
        ty: &NativeTy<'db>,
    ) -> Result<ir::Value, CompileError> {
        // The receiver's tag is read after the allocation, which can
        // change the type id of what it refers to.
        let bound = self.allocate(layout::bound_size(0))?;
        let tag = self.callable(bound, layout::TAG_BOUND_METHOD, code, 0)?;
        self.slot_init(ty, receiver, bound, layout::BOUND_RECEIVER);
        Ok(self.pair(tag, bound))
    }

    /// Write the four words every callable made here starts with, at
    /// `object`: its header, which is the type id `tag` and the call
    /// layout of the function `code`, the address of that function's
    /// code, no function, and `count`, the number of what follows.
    /// Returns the tag of the function value the object is: its header.
    fn callable(
        &mut self,
        object: ir::Value,
        tag: usize,
        code: &FnKey<'db>,
        count: usize,
    ) -> Result<ir::Value, CompileError> {
        const _: () = assert!(layout::BOUND_CODE == layout::CLOSURE_CODE);
        const _: () = assert!(layout::BOUND_FUNCTION == layout::CLOSURE_FUNCTION);
        const _: () = assert!(layout::BOUND_TYPE_ARG_COUNT == layout::CLOSURE_CAPTURE_COUNT);
        let code = self.declared(code)?;
        let header = self.backend.callable_header(tag, &code.layout)?;
        let code = self.function_ref(code.id);
        let words = [
            (0, self.word_const(header)),
            (
                layout::CLOSURE_CODE,
                self.builder.ins().func_addr(WORD, code),
            ),
            (layout::CLOSURE_FUNCTION, self.word_const(0)),
            (layout::CLOSURE_CAPTURE_COUNT, self.word_const(count)),
        ];
        for (at, word) in words {
            self.builder
                .ins()
                .store(slot_access(), word, object, offset(at));
        }
        Ok(words[0].1)
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
                let object = self.read_object(base)?;
                self.slot_store(&ty, value, object, bytes);
            }
            Place::Index {
                base,
                index,
                kind: IndexKind::Tuple,
            } => {
                let (ty, bytes) = self.tuple_slot(base, *index)?;
                let tuple = self.read_object(base)?;
                self.slot_store(&ty, value, tuple, bytes);
            }
            Place::Index {
                base,
                index,
                kind: IndexKind::Map,
            } => {
                let map_ty = self.place_ty(base)?;
                let map = self.read_object(base)?;
                let key = self.read_local(*index);
                self.map_store(map, &map_ty, key, value)?;
            }
            Place::Index { base, index, .. } => {
                let ty = self.place_ty(place)?;
                let array = self.read_object(base)?;
                let (element, is_byte) = self.indexed(base, array, *index)?;
                if is_byte {
                    // The low eight bits, as assigning into a typed array
                    // keeps.
                    self.builder.ins().istore8(
                        slot_access(),
                        value,
                        element,
                        offset(layout::BYTES_STORAGE_DATA),
                    );
                } else {
                    self.slot_store(&ty, value, element, layout::STORAGE_ELEMENTS);
                }
            }
            Place::Capture(_) => return Err(internal("a capture is never assigned")),
            Place::Deref(cell) => {
                let ty = self.place_ty(place)?;
                let at = self.cell(*cell)?;
                self.slot_store(&ty, value, at, layout::CELL_VALUE);
                // Whatever the cell was made for, it holds a value now.
                if let CellId::Local(local) = cell
                    && let Some(omitted) = self.omitted_cells.get(local).copied()
                {
                    let stored = self.bool_const(false);
                    self.builder.def_var(omitted, stored);
                }
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
            // The address of the variant's object.
            Operand::Constant(Constant::EnumVariant {
                enum_ref, index, ..
            }) => {
                let class = self
                    .backend
                    .class_object(self.db, classes::ClassLike::Enum(*enum_ref))?;
                let variants = self.backend.variants_data(class)?;
                let variants = self.data_address(variants);
                let at = imm(*index as usize * layout::VARIANT_SIZE);
                Ok(self.builder.ins().iadd_imm(variants, at))
            }
            Operand::Constant(Constant::Float(value)) => Ok(self.builder.ins().f64const(*value)),
            Operand::Constant(Constant::Bool(value)) => Ok(self.bool_const(*value)),
            Operand::Constant(Constant::Null) => Ok(self.null()),
            Operand::Constant(Constant::String(text)) => {
                let string = self.backend.string_constant(text)?;
                Ok(self.data_address(string))
            }
            Operand::Constant(Constant::Bigint(value)) => {
                let bigint = self.backend.bigint_constant(value)?;
                Ok(self.data_address(bigint))
            }
            Operand::Constant(Constant::Function(DeclRef::Source(function))) => {
                let code = self.declared(&FnKey::new(*function, Entry::Value))?;
                let header = self
                    .backend
                    .callable_header(layout::TAG_CLOSURE, &code.layout)?;
                let closure = self.backend.function_value(code.id, header)?;
                let closure = self.data_address(closure);
                // The header of the closure in the executable's data.
                let tag = self.word_const(header);
                Ok(self.pair(tag, closure))
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
        // A union kept as one goes where a union is expected as it is,
        // whatever the block knows it holds.
        if expected.holds_any()
            && let Some(local) = operand_local(operand)
            && self.storage_ty(local).holds_any()
        {
            return Ok(self.read_local(local));
        }
        let found = self.operand_ty(operand)?;
        let value = self.operand(operand)?;
        Ok(self.coerce(value, &found, expected))
    }

    fn statement(&mut self, statement: &StatementKind<'db>) -> Result<(), CompileError> {
        match statement {
            StatementKind::Nop
            | StatementKind::Drop(_)
            | StatementKind::Intrinsic {
                op: IntrinsicOp::BuiltinTraceHook(_),
                ..
            } => Ok(()),
            // A type argument: nothing at run time.
            StatementKind::Assign {
                value: Rvalue::LoadType(_),
                ..
            } => Ok(()),
            // A local that may be an argument left to its default takes
            // one, or another such local, as it is held: with its tag.
            StatementKind::Assign {
                destination: Place::Local(local),
                value: Rvalue::Use(operand),
            } if let Some(variable) = self.omissible(*local)
                && let Some(held) = self.held_omissible(operand) =>
            {
                self.builder.def_var(variable, held);
                Ok(())
            }
            StatementKind::Assign { destination, value } => {
                let expected = self.place_ty(destination)?;
                if let Rvalue::Use(operand) = value
                    && expected.holds_any()
                    && !operand_local(operand)
                        .is_some_and(|local| self.item_variables.contains_key(&local))
                {
                    let value = self.operand_as(operand, &expected)?;
                    return self.write_place(destination, value);
                }
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
                    Some(self.slot_load(&content, old, layout::CELL_VALUE))
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
                    .map(|part| self.operand_as(part, &NativeTy::String))
                    .collect::<Result<Vec<_>, _>>()?;
                self.concat(&parts)?
            }
            // An enum's variant says its own position.
            Rvalue::Discriminant(place) => {
                let ty = self.place_ty(place)?;
                let value = self.read_place(place)?;
                let held = narrowed(ty.clone());
                let variant = self.coerce(value, &ty, &held);
                self.builder.ins().load(
                    types::I64,
                    MemFlags::trusted(),
                    variant,
                    offset(layout::VARIANT_INDEX),
                )
            }
            // What a switch on the tag reads: the tag of the pair as its
            // variable holds it, whatever the block knows it to be.
            Rvalue::TypeTag(place) => {
                let value = match place {
                    Place::Local(local) if self.storage_ty(*local).is_pair() => {
                        self.read_local(*local)
                    }
                    _ => self.read_place(place)?,
                };
                self.presence(value)
            }
            Rvalue::Len(place) => {
                let length_of = narrowed(self.place_ty(place)?);
                let array = self.read_object(place)?;
                if matches!(length_of, NativeTy::Map(..)) {
                    self.map_length(array)
                } else {
                    self.array_view(array, length_of == NativeTy::Bytes).0
                }
            }
            Rvalue::Uint8Array(bytes) => {
                let data = self.backend.bytes_constant(bytes)?;
                let data = self.data_address(data);
                let length = self.word_const(bytes.len());
                let mutator = self.mutator();
                self.call_runtime(Runtime::BytesNew, &[mutator, data, length])?
                    .ok_or_else(|| internal("byte array construction returns the array"))?
            }
            Rvalue::Map(_, _, entries) => self.map_literal(ty, entries)?,
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
            } => {
                let NativeTy::Class(_, arguments) = ty else {
                    return Err(internal("a checked class literal makes an instance"));
                };
                self.instance(*class, arguments, fields)?
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Tuple { .. },
                fields,
            } => {
                let NativeTy::Tuple(elements) = ty else {
                    return Err(internal("a checked tuple literal makes a tuple"));
                };
                self.tuple(elements, fields)?
            }
            Rvalue::IsType { operand, .. } | Rvalue::IsTypeTag { operand, .. } => {
                let operand_ty = self.operand_ty(operand)?;
                let (test, coarse) = match value {
                    Rvalue::IsType { test, .. } => {
                        (TestKind::of(self.db, test, self.body.arguments), false)
                    }
                    Rvalue::IsTypeTag { tag, .. } => (TestKind::of_tag(*tag), true),
                    _ => (None, false),
                };
                let plan = type_test_plan(self.db, &operand_ty, test, coarse).map_err(internal)?;
                let operand = self.operand(operand)?;
                self.type_test(operand, &operand_ty, &plan)?
            }
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            } => {
                let cells = captures
                    .iter()
                    .map(|capture| Ok((self.operand(capture)?, self.operand_ty(capture)?)))
                    .collect::<Result<Vec<_>, CompileError>>()?;
                self.closure(&self.key.lambda(*lambda_idx), &cells)?
            }
            Rvalue::MakeBoundMethod {
                func: DeclRef::Source(function),
                receiver,
                ..
            } => {
                let ty = self.operand_ty(receiver)?;
                let receiver = self.operand(receiver)?;
                let receiver = self.track(receiver, &ty);
                self.bound_method(&FnKey::new(*function, Entry::Bound), receiver, &ty)?
            }
            _ => return Err(internal("a checked rvalue is supported")),
        })
    }

    fn function_ref(&mut self, id: FuncId) -> ir::FuncRef {
        *self
            .function_refs
            .entry(id)
            .or_insert_with(|| self.backend.function_ref(id, self.builder.func))
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
        Ok(result)
    }

    /// Call a runtime function that returns a reference or what it threw
    /// instead, as a pair whose tag says which: the reference, with the
    /// throw raised where it happened.
    fn fallible_runtime(
        &mut self,
        function: Runtime,
        arguments: &[ir::Value],
    ) -> Result<ir::Value, CompileError> {
        self.forget_arrays();
        let function = self.runtime_ref(function)?;
        let call = self.builder.ins().call(function, arguments);
        let results = self.builder.inst_results(call).to_vec();
        let value = self.returned_or_raise(results[0], results[1])?;
        self.builder.declare_value_needs_stack_map(value);
        Ok(value)
    }

    /// What a function of the runtime that can throw returned as the two
    /// words `tag` and `payload`: the payload, with the throw raised here
    /// when the tag says the two are what was thrown. What follows is
    /// generated where nothing was.
    fn returned_or_raise(
        &mut self,
        tag: ir::Value,
        payload: ir::Value,
    ) -> Result<ir::Value, CompileError> {
        let thrown = self.builder.ins().band_imm(tag, imm(layout::THROWN));
        let raise = self.builder.create_block();
        let resume = self.builder.create_block();
        self.builder.set_cold_block(raise);
        self.builder.ins().brif(thrown, raise, &[], resume, &[]);
        self.builder.seal_block(raise);
        self.builder.seal_block(resume);
        self.builder.switch_to_block(raise);
        let tag = self.builder.ins().band_imm(tag, !imm(layout::THROWN));
        let error = self.pair(tag, payload);
        self.raise(error, None)?;
        self.builder.switch_to_block(resume);
        Ok(payload)
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

    /// Throw `baml.panics.StackOverflow` when this function is entered with
    /// less stack than the limit allows.
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
        self.builder.seal_block(resume);

        // Thrown from this function's caller on: the call has no handler
        // of this function's, which has not begun.
        self.builder.switch_to_block(slow);
        let unused = self.builder.ins().iconst(WORD, 0);
        self.emit_runtime_call(Runtime::PanicStackOverflow, &[unused, unused])?;
        self.builder.ins().trap(TRAP_AFTER_PANIC);

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
        self.call_runtime(Runtime::PollSlow, &[])?;
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

    /// The address of the worker this code is running on: the pinned
    /// register, read where it is used so that an access to the worker
    /// addresses off the register itself.
    fn mutator(&mut self) -> ir::Value {
        self.builder.ins().get_pinned_reg(WORD)
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
        // Not through `call_runtime`: this path does not come back, so what
        // is known about arrays still holds on the one that continues.
        if self.body.blocks[self.current].unwind.is_some() {
            // The handler is in this function: make the error and jump.
            let mutator = self.mutator();
            let (function, arguments) = match panic {
                Panic::DivisionByZero(dividend) => {
                    (Runtime::ErrorDivisionByZero, vec![mutator, dividend])
                }
                Panic::NegativeShift(count) => (Runtime::ErrorNegativeShift, vec![mutator, count]),
                Panic::IndexOutOfBounds(index, length) => {
                    (Runtime::ErrorIndexOutOfBounds, vec![mutator, index, length])
                }
                Panic::MapKeyNotFound => (Runtime::ErrorMapKeyNotFound, vec![mutator]),
            };
            let error = self
                .emit_runtime_call(function, &arguments)?
                .ok_or_else(|| internal("the runtime returns the error it makes"))?;
            // An instance.
            let tag = tag_of_object(&mut self.builder, error, false);
            let error = self.pair(tag, error);
            self.raise(error, None)?;
        } else {
            let zero = self.builder.ins().iconst(WORD, 0);
            let (function, arguments) = match panic {
                Panic::DivisionByZero(dividend) => {
                    (Runtime::PanicDivisionByZero, vec![dividend, zero])
                }
                Panic::NegativeShift(count) => (Runtime::PanicNegativeShift, vec![count, zero]),
                Panic::IndexOutOfBounds(index, length) => {
                    (Runtime::PanicIndexOutOfBounds, vec![index, length])
                }
                Panic::MapKeyNotFound => (Runtime::PanicMapKeyNotFound, vec![zero, zero]),
            };
            self.emit_runtime_call(function, &arguments)?;
            self.builder.ins().trap(TRAP_AFTER_PANIC);
        }

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
        // The size goes in and the address comes out through the mutator.
        let size = self.word_const(size);
        let mutator = self.mutator();
        self.builder.ins().store(
            mutator_access(),
            size,
            mutator,
            offset(layout::MUTATOR_SCRATCH),
        );
        self.call_runtime(Runtime::AllocSlow, &[])?;
        let mutator = self.mutator();
        let object = self.builder.ins().load(
            WORD,
            mutator_access(),
            mutator,
            offset(layout::MUTATOR_SCRATCH),
        );
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
        arguments: &[TypeArg<'db>],
        fields: &[Operand<'db>],
    ) -> Result<ir::Value, CompileError> {
        let layout = class_layout(self.db, class, arguments).map_err(internal)?;
        // Evaluate before allocating: the fields are stored with no call in
        // between, so the collector never sees a half-built object.
        let values = fields
            .iter()
            .zip(&layout)
            .map(|(field, ty)| self.operand_as(field, ty))
            .collect::<Result<Vec<_>, _>>()?;
        let generics = class_generic_params(self.db, class).len();
        let class_object = self
            .backend
            .class_object(self.db, classes::ClassLike::Class(class))?;
        let class_object = self.backend.class_data(class_object);
        let object = self.allocate(layout::instance_size(generics + layout.len()))?;
        // The flags are the top bits of the word, so it is no size.
        let header = Backend::class_header(self.db, class);
        let header = self
            .builder
            .ins()
            .iconst(WORD, i64::from_ne_bytes(header.to_ne_bytes()));
        self.builder.ins().store(slot_access(), header, object, 0);
        let class_object = self.data_address(class_object);
        self.builder.ins().store(
            slot_access(),
            class_object,
            object,
            offset(layout::INSTANCE_CLASS),
        );
        // The slots of the type parameters hold nothing yet.
        let null = self.null();
        for slot in 0..generics {
            self.slot_init(&NativeTy::Null, null, object, layout::field_offset(slot));
        }
        for (slot, (ty, value)) in layout.iter().zip(values).enumerate() {
            self.slot_init(ty, value, object, layout::field_offset(generics + slot));
        }
        Ok(object)
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
        let length = self.word_const(values.len());
        let mutator = self.mutator();
        let array = self
            .call_runtime(Runtime::ArrayNew, &[mutator, length])?
            .ok_or_else(|| internal("array construction returns the array"))?;
        if !values.is_empty() {
            let storage =
                self.builder
                    .ins()
                    .load(WORD, array_access(), array, offset(layout::ARRAY_PTR));
            for (index, value) in values.into_iter().enumerate() {
                self.slot_init(
                    element,
                    value,
                    storage,
                    layout::STORAGE_ELEMENTS + index * layout::VALUE,
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
        // Nothing between here and either use of the words can collect.
        let item = slot_words(&mut self.builder, element, item);
        let length =
            self.builder
                .ins()
                .load(WORD, array_access(), array, offset(layout::ARRAY_LENGTH));
        let storage =
            self.builder
                .ins()
                .load(WORD, array_access(), array, offset(layout::ARRAY_PTR));
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
        let capacity = self.builder.ins().load(
            WORD,
            array_access(),
            storage,
            offset(layout::STORAGE_CAPACITY),
        );
        let full = self.builder.ins().icmp(IntCC::Equal, length, capacity);
        self.builder.ins().brif(full, grow, &[], in_place, &[]);

        self.builder.switch_to_block(in_place);
        let scaled = self
            .builder
            .ins()
            .ishl_imm(length, Self::element_shift(false));
        let slot = self.builder.ins().iadd(storage, scaled);
        // Both words under the busy bit, whatever the element type: the
        // slot past the length has no tag of that type yet.
        self.slot_store_words(item, slot, layout::STORAGE_ELEMENTS);
        self.builder.ins().store(
            array_access(),
            new_length,
            array,
            offset(layout::ARRAY_LENGTH),
        );
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(grow);
        let mutator = self.mutator();
        self.call_runtime(Runtime::ArrayPushSlow, &[mutator, array, item.0, item.1])?;
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
        if crate::types::is_omitted(right) {
            return self.is_omitted(left);
        }
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
                // Whether a remainder by a power of two is zero is whether
                // the low bits are, whatever the sign: one test in place of
                // the rounding the remainder itself takes.
                let left_value = match (op, right, self.remainder_mask(left_value)) {
                    (
                        BinOp::Eq | BinOp::Ne,
                        Operand::Constant(Constant::Int(0)),
                        Some((dividend, mask)),
                    ) => self.builder.ins().band_imm(dividend, mask),
                    _ => left_value,
                };
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
                let optional = if optional_on_left {
                    left_value
                } else {
                    right_value
                };
                let presence = self.presence(optional);
                Ok(self.builder.ins().icmp_imm(int_condition, presence, 0))
            }
            BinaryMode::Map => {
                let equal = self.map_equal(&left_ty, left_value, right_value)?;
                Ok(if op == BinOp::Eq {
                    equal
                } else {
                    self.builder.ins().bxor_imm(equal, 1)
                })
            }
            BinaryMode::Bytes => {
                let equal = self
                    .call_runtime(Runtime::BytesEq, &[left_value, right_value])?
                    .ok_or_else(|| internal("byte array equality returns a bool"))?;
                Ok(if op == BinOp::Eq {
                    equal
                } else {
                    self.builder.ins().bxor_imm(equal, 1)
                })
            }
            BinaryMode::Bigint => {
                let mut flags = 0;
                if left_ty == NativeTy::Int {
                    flags |= layout::BIGINT_LEFT_IS_INT;
                }
                if right_ty == NativeTy::Int {
                    flags |= layout::BIGINT_RIGHT_IS_INT;
                }
                if is_comparison(op) {
                    let flags = self.word_const(flags);
                    let order = self
                        .call_runtime(Runtime::BigintCmp, &[flags, left_value, right_value])?
                        .ok_or_else(|| internal("bigint comparison returns an order"))?;
                    return Ok(self.builder.ins().icmp_imm(int_condition, order, 0));
                }
                let op = match op {
                    BinOp::Add => layout::BIGINT_OP_ADD,
                    BinOp::Sub => layout::BIGINT_OP_SUB,
                    BinOp::Mul => layout::BIGINT_OP_MUL,
                    BinOp::Div => layout::BIGINT_OP_DIV,
                    BinOp::Mod => layout::BIGINT_OP_REM,
                    BinOp::BitAnd => layout::BIGINT_OP_AND,
                    BinOp::BitOr => layout::BIGINT_OP_OR,
                    BinOp::BitXor => layout::BIGINT_OP_XOR,
                    BinOp::Shl => layout::BIGINT_OP_SHL,
                    BinOp::Shr => layout::BIGINT_OP_SHR,
                    _ => return Err(internal("a checked bigint operator is one of these")),
                };
                let mutator = self.mutator();
                let op = self.word_const(op | flags);
                self.fallible_runtime(
                    Runtime::BigintBinary,
                    &[mutator, op, left_value, right_value],
                )
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

    /// The constant `value` is, when it is the result of an `iconst`.
    fn constant_of(&self, value: ir::Value) -> Option<i64> {
        let dfg = &self.builder.func.dfg;
        let instruction = dfg.value_def(dfg.resolve_aliases(value)).inst()?;
        match dfg.insts[instruction] {
            ir::InstructionData::UnaryImm {
                opcode: ir::Opcode::Iconst,
                imm,
            } => Some(imm.bits()),
            _ => None,
        }
    }

    /// When `value` is `dividend % divisor` for a constant `divisor` that is
    /// a power of two above one: the dividend, and the mask of the bits
    /// below the divisor's.
    fn remainder_mask(&self, value: ir::Value) -> Option<(ir::Value, i64)> {
        let dfg = &self.builder.func.dfg;
        let instruction = dfg.value_def(dfg.resolve_aliases(value)).inst()?;
        let ir::InstructionData::Binary {
            opcode: ir::Opcode::Srem,
            args: [dividend, divisor],
        } = dfg.insts[instruction]
        else {
            return None;
        };
        let divisor = self.constant_of(divisor)?;
        (divisor > 1 && divisor & (divisor - 1) == 0).then_some((dividend, divisor - 1))
    }

    /// `value * factor` as a shift and an add, for the factors that are one
    /// more than a power of two and small: a single instruction where a
    /// multiply needs the factor in a register first.
    fn multiply_by_shift_and_add(&mut self, value: ir::Value, factor: i64) -> Option<ir::Value> {
        let shift = match factor {
            3 => 1,
            5 => 2,
            9 => 3,
            _ => return None,
        };
        let shifted = self.builder.ins().ishl_imm(value, shift);
        Some(self.builder.ins().iadd(value, shifted))
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
            BinOp::Mul => {
                let by_constant = match (self.constant_of(left), self.constant_of(right)) {
                    (_, Some(factor)) => self.multiply_by_shift_and_add(left, factor),
                    (Some(factor), None) => self.multiply_by_shift_and_add(right, factor),
                    (None, None) => None,
                };
                Ok(match by_constant {
                    Some(product) => product,
                    None => self.builder.ins().imul(left, right),
                })
            }
            BinOp::Div => self.divide(left, right, right_constant),
            // The remainder of the smallest int by -1 is zero, which is what
            // the instruction gives on every target: unlike division it is
            // defined not to trap there.
            BinOp::Mod => {
                self.require_nonzero_divisor(left, right, right_constant)?;
                Ok(match right_constant {
                    // A constant divisor is left to the optimizer.
                    Some(_) => self.builder.ins().srem(left, right),
                    // Zero went the other way just above, so the
                    // instruction need not look for it again.
                    None => self.builder.ins().srem_unchecked(left, right),
                })
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
        if cfg!(target_arch = "aarch64") {
            // The arm64 instruction does not trap, and what it gives for the
            // smallest int over -1 is the smallest int, as wrapping does:
            // only zero is looked for.
            self.require_nonzero_divisor(left, right, None)?;
            return Ok(self.builder.ins().sdiv_unchecked(left, right));
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
        // Neither divisor the instruction could not divide by gets here, so
        // it is the bare divide, with no checks of its own.
        let quotient = self.builder.ins().sdiv_unchecked(left, right);
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
            (UnaryOp::Neg, NativeTy::Bigint) => {
                let mutator = self.mutator();
                self.fallible_runtime(Runtime::BigintNeg, &[mutator, value])?
            }
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
            (Intrinsic::EnumEq | Intrinsic::EnumNe, [left, right])
                if matches!(self.operand_ty(left)?, NativeTy::Map(..)) =>
            {
                let ty = self.operand_ty(left)?;
                let (left, right) = (self.operand(left)?, self.operand(right)?);
                let equal = self.map_equal(&ty, left, right)?;
                Ok(if intrinsic == Intrinsic::EnumEq {
                    equal
                } else {
                    self.builder.ins().bxor_imm(equal, 1)
                })
            }
            (Intrinsic::EnumEq | Intrinsic::EnumNe, [left, right])
                if self.operand_ty(left)? == NativeTy::Bytes =>
            {
                let op = if intrinsic == Intrinsic::EnumEq {
                    BinOp::Eq
                } else {
                    BinOp::Ne
                };
                self.binary(op, left, right)
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
            (Intrinsic::Scalar(op), [value]) => {
                let value = self.operand(value)?;
                let ins = self.builder.ins();
                Ok(match op {
                    ScalarOp::FloatSqrt => ins.sqrt(value),
                    ScalarOp::FloatAbs => ins.fabs(value),
                    ScalarOp::FloatFloor => ins.floor(value),
                    ScalarOp::FloatCeil => ins.ceil(value),
                    ScalarOp::FloatTrunc => ins.trunc(value),
                    ScalarOp::FloatIsNan => ins.fcmp(FloatCC::Unordered, value, value),
                    // The smallest `int` is its own absolute value: `int`
                    // wraps.
                    ScalarOp::IntAbs => ins.iabs(value),
                    ScalarOp::IntLeadingZeros => ins.clz(value),
                    ScalarOp::IntTrailingZeros => ins.ctz(value),
                    ScalarOp::IntCountOnes => ins.popcnt(value),
                })
            }
            (Intrinsic::ToString, [value]) => {
                let ty = self.operand_ty(value)?;
                let value = self.operand(value)?;
                let mutator = self.mutator();
                match ty {
                    NativeTy::String => Ok(value),
                    NativeTy::Bigint => self
                        .call_runtime(Runtime::StringFromBigint, &[mutator, value])?
                        .ok_or_else(|| internal("bigint to string returns a string")),
                    NativeTy::Int => self
                        .call_runtime(Runtime::StringFromInt, &[mutator, value])?
                        .ok_or_else(|| internal("int to string returns a string")),
                    NativeTy::Float => self
                        .call_runtime(Runtime::StringFromFloat, &[mutator, value])?
                        .ok_or_else(|| internal("float to string returns a string")),
                    // A variant's object holds its name.
                    NativeTy::Enum(_) => {
                        let name = self.builder.ins().load(
                            WORD,
                            MemFlags::trusted(),
                            value,
                            offset(layout::VARIANT_NAME),
                        );
                        self.builder.declare_value_needs_stack_map(name);
                        Ok(name)
                    }
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
        let array = self.operand_object(array)?;
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
            let nothing = self.zero(&element_ty);
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
                    .load(WORD, array_access(), array, offset(layout::ARRAY_PTR));
            let scaled = self
                .builder
                .ins()
                .ishl_imm(index, Self::element_shift(false));
            let slot = self.builder.ins().iadd(storage, scaled);
            let element = self.slot_load(&element_ty, slot, layout::STORAGE_ELEMENTS);
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
                let plan = type_test_plan(
                    self.db,
                    &source_ty,
                    TestKind::of(self.db, test, self.body.arguments),
                    false,
                )
                .map_err(internal)?;
                let value = self.operand(source)?;
                let passes = self.type_test(value, &source_ty, &plan)?;
                // Bound where the test passes: taking a value out of its
                // box is only right there.
                let bind = self.builder.create_block();
                let (then_block, else_block) = (self.block(*then_block), self.block(*else_block));
                self.builder.ins().brif(passes, bind, &[], else_block, &[]);
                self.builder.seal_block(bind);
                self.builder.switch_to_block(bind);
                let bound_ty = narrow_bound(&source_ty, &plan);
                let kept = self.storage_ty(*destination);
                let bound = self.coerce(value, &source_ty, &bound_ty);
                let bound = self.coerce(bound, &bound_ty, &kept);
                self.write_local(*destination, bound);
                self.builder.ins().jump(then_block, &[]);
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
                let (join, eval_rhs) = (self.block(*join), self.block(*eval_rhs));
                if *kind == ShortCircuitKind::Coalesce && operand_ty.is_pair() {
                    // What the operand holds is the result only where it
                    // holds something: `null` is not read as a value.
                    let presence = self.presence(value);
                    let present = self.builder.create_block();
                    self.builder
                        .ins()
                        .brif(presence, present, &[], eval_rhs, &[]);
                    self.builder.seal_block(present);
                    self.builder.switch_to_block(present);
                    let result = self.coerce(value, &operand_ty, &destination_ty);
                    self.write_place(destination, result)?;
                    self.builder.ins().jump(join, &[]);
                    return Ok(());
                }
                let result = self.coerce(value, &operand_ty, &destination_ty);
                self.write_place(destination, result)?;
                match kind {
                    ShortCircuitKind::And => {
                        self.builder.ins().brif(value, eval_rhs, &[], join, &[]);
                    }
                    ShortCircuitKind::Or => {
                        self.builder.ins().brif(value, join, &[], eval_rhs, &[]);
                    }
                    // A value that is never null is always the result.
                    ShortCircuitKind::Coalesce => {
                        self.builder.ins().jump(join, &[]);
                    }
                }
            }
            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
                ..
            } => {
                // A type tag is read where it is switched on: the value it
                // is of is read again here, for what its tag does not say.
                if crate::check::switched_tag(self.body, self.current).is_some() {
                    let read = self.body.blocks[self.current].statements.last();
                    let Some(StatementKind::Assign {
                        value: Rvalue::TypeTag(place),
                        ..
                    }) = read.map(|statement| &statement.kind)
                    else {
                        return Err(internal("a switched type tag is read where it is switched"));
                    };
                    let value = match place {
                        Place::Local(local) if self.storage_ty(*local).is_pair() => {
                            self.read_local(*local)
                        }
                        _ => self.read_place(place)?,
                    };
                    let (tag, payload) = self.builder.ins().isplit(value);
                    return self.switch_on_tag(tag, payload, arms, *otherwise);
                }
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
                // Every caller takes the result as its tag and its payload.
                let value = self.read_local(Local(0));
                let ty = self.storage_ty(Local(0));
                let (tag, payload) = slot_words(&mut self.builder, &ty, value);
                self.builder.ins().return_(&[tag, payload]);
            }
            Terminator::Unreachable => {
                self.builder.ins().trap(TRAP_UNREACHABLE);
            }
            Terminator::Call {
                has_trace,
                argument_layout,
                callee,
                args,
                ntypeargs,
                destination,
                target,
                ..
            } => {
                let args = crate::types::passed(args, *has_trace);
                let destination = place_local(destination).map_err(internal)?;
                let expected = self.local_ty(destination)?;
                let called =
                    call_target(self.db, self.body, callee, args, *ntypeargs).map_err(internal)?;
                let (result, produced) = match called {
                    // Nothing reads them: no reference at all.
                    Callee::Intrinsic(Intrinsic::TraceOptions) => {
                        (self.zero(&expected), expected.clone())
                    }
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
                    Callee::Native(native) => self.native_call(native, args, *ntypeargs)?,
                    Callee::Value => {
                        let NativeTy::Function(signature) = narrowed(self.operand_ty(callee)?)
                        else {
                            return Err(internal("a checked call is of a function value"));
                        };
                        // Read first and kept in the frame's map: what an
                        // argument does may collect.
                        let function = self.operand(callee)?;
                        self.builder.declare_value_needs_stack_map(function);
                        let words =
                            self.argument_words(args.iter().map(Some), &signature.params)?;
                        let (_, closure) = self.builder.ins().isplit(function);
                        let mut arguments = vec![closure];
                        arguments.extend(words);
                        // A load marked read-only can be moved above the
                        // test that proved the closure is not `null`.
                        let never_null = operand_local(callee).is_some_and(|local| {
                            matches!(self.storage_ty(local), NativeTy::Function(_))
                        });
                        let access = if never_null {
                            closure_access()
                        } else {
                            slot_access()
                        };
                        let code = self.builder.ins().load(
                            WORD,
                            access,
                            closure,
                            offset(layout::CLOSURE_CODE),
                        );
                        let clif = Backend::clif_signature(signature.params.len(), Entry::Value, 0);
                        let clif = self.builder.import_signature(clif);
                        // The callee can collect, which moves storage.
                        self.forget_arrays();
                        let called = unwind::Called::Indirect(clif, code);
                        let (tag, payload) = self.baml_call(&called, &arguments)?;
                        let result =
                            of_slot_words(&mut self.builder, &signature.result, tag, payload);
                        (result, signature.result)
                    }
                    Callee::Function(key) => self.call_function(
                        &key,
                        argument_layout.as_ref(),
                        &args[..*ntypeargs],
                        &args[*ntypeargs..],
                    )?,
                };
                let result = self.track(result, &produced);
                let result = self.coerce(result, &produced, &expected);
                self.write_local(destination, result);
                let target = self.block(*target);
                self.builder.ins().jump(target, &[]);
            }
            Terminator::VirtualCall {
                has_trace,
                argument_layout,
                iface,
                method,
                args,
                ntypeargs,
                self_arg,
                destination,
                target,
                ..
            } => {
                let args = crate::types::passed(args, *has_trace);
                let destination = place_local(destination).map_err(internal)?;
                let expected = self.local_ty(destination)?;
                let found = virtual_target(
                    self.db, self.body, iface, method, args, *ntypeargs, *self_arg,
                )
                .map_err(internal)?;
                let args = &args[*ntypeargs..];
                let (result, produced) = match found {
                    Virtual::Operator(operator) => {
                        let types = args
                            .iter()
                            .map(|argument| self.operand_ty(argument))
                            .collect::<Result<Vec<_>, _>>()?;
                        let produced = operator.result(&types).map_err(internal)?;
                        let result = match (operator, args) {
                            (Operator::Binary(op), [left, right]) => {
                                self.binary(op, left, right)?
                            }
                            (Operator::Unary(op), [operand]) => self.unary(op, operand)?,
                            _ => return Err(internal("a checked operator has its operands")),
                        };
                        (result, produced)
                    }
                    Virtual::Static(Target::ToString) => (
                        self.intrinsic(Intrinsic::ToString, &args[*self_arg..=*self_arg])?,
                        NativeTy::String,
                    ),
                    Virtual::Static(Target::Sort(kind)) => {
                        let ty = narrowed(self.operand_ty(&args[*self_arg])?);
                        let array = self.operand_object(&args[*self_arg])?;
                        let kind = self.word_const(kind);
                        self.call_runtime(Runtime::ArraySort, &[array, kind])?;
                        (array, ty)
                    }
                    Virtual::Static(Target::Function(key)) => {
                        self.call_function(&key, argument_layout.as_ref(), &[], args)?
                    }
                    Virtual::Static(Target::Native(native)) => {
                        self.native_call_out_of_line(native, args, 0)?
                    }
                    Virtual::Dynamic(slot, ..) => self.dispatch(&slot, args)?,
                };
                let result = self.track(result, &produced);
                let result = self.coerce(result, &produced, &expected);
                self.write_local(destination, result);
                let target = self.block(*target);
                self.builder.ins().jump(target, &[]);
            }
            Terminator::Spawn {
                plan,
                future,
                resume,
            } => self.spawn(plan, future, *resume)?,
            Terminator::Await {
                future,
                destination,
                target,
                ..
            } => self.await_future(future, destination, *target)?,
            // A checked system operation is the sleep.
            // A system operation the runtime implements as a native.
            Terminator::SysOp {
                callee: Operand::Constant(Constant::Function(function)),
                args,
                destination,
                target,
                ..
            } if let Some(native) = crate::natives::named(
                &baml_compiler2_mir::function_display_name(self.db, *function),
            )
            .filter(|native| native.flags.is_some()) =>
            {
                let (value, produced) = self.native_call_out_of_line(native, args, 0)?;
                let expected = self.place_ty(destination)?;
                let value = self.coerce(value, &produced, &expected);
                self.write_place(destination, value)?;
                let target = self.block(*target);
                self.builder.ins().jump(target, &[]);
            }
            Terminator::SysOp {
                args,
                destination,
                target,
                ..
            } => {
                let delay = self.operand(&args[0])?;
                let nanos = self
                    .call_runtime(Runtime::BigintToNanos, &[delay])?
                    .ok_or_else(|| internal("a count of nanoseconds is an int"))?;
                self.call_runtime(Runtime::Sleep, &[nanos])?;
                let null = self.null();
                let expected = self.place_ty(destination)?;
                let null = self.coerce(null, &NativeTy::Null, &expected);
                self.write_place(destination, null)?;
                let target = self.block(*target);
                self.builder.ins().jump(target, &[]);
            }
            Terminator::Throw { value } => self.throw(value)?,
            Terminator::Rethrow { value, context } => self.rethrow(value, context)?,
            Terminator::AwaitAny { .. } => {
                return Err(internal("a checked terminator is supported"));
            }
        }
        Ok(())
    }

    /// Call one copy of a function with these type and value arguments,
    /// from a call checked against the layout `site`. The function is
    /// known, so the call's slots are laid over its own here: it is
    /// passed each of its parameters, those the call does not have as
    /// left to their defaults.
    fn call_function(
        &mut self,
        key: &FnKey<'db>,
        site: Option<&CallLayout>,
        types: &[Operand<'db>],
        args: &[Operand<'db>],
    ) -> Result<(ir::Value, NativeTy<'db>), CompileError> {
        let callee = self.declared(key)?;
        let slots = call_slots(site, args.len(), &callee.layout)
            .ok_or_else(|| internal("a checked call can be laid over its callee"))?;
        let mut arguments = Vec::new();
        if callee.type_args != 0 {
            for argument in types {
                arguments.push(self.type_argument(argument)?);
            }
        }
        arguments.extend(self.argument_words(
            slots.iter().map(|slot| slot.map(|at| &args[at])),
            &callee.signature.params,
        )?);
        let produced = callee.signature.result.clone();
        let callee = self.function_ref(callee.id);
        // The callee can collect, which moves storage.
        self.forget_arrays();
        let (tag, payload) = self.baml_call(&unwind::Called::Direct(callee), &arguments)?;
        let result = of_slot_words(&mut self.builder, &produced, tag, payload);
        Ok((result, produced))
    }

    /// Call the code the receiver's type has for an interface method. An
    /// instance's class object holds its row, and an enum's variant's
    /// enum's, where the other holds its own; a value of any other type
    /// has what its tag has in the method's table of tags. The method's
    /// column says where in the row.
    fn dispatch(
        &mut self,
        slot: &Slot,
        args: &[Operand<'db>],
    ) -> Result<(ir::Value, NativeTy<'db>), CompileError> {
        let program = self.program;
        let slot = program
            .slots
            .iter()
            .find(|known| known.slot == *slot)
            .ok_or_else(|| internal("a dispatched method has a slot"))?;
        // The receiver is a value like any other: its tag says where its
        // row is, and the implementation reads it as its own first
        // parameter.
        let receiver = self.operand_as(&args[0], &NativeTy::Unknown)?;
        let rest = self.argument_words(args[1..].iter().map(Some), &slot.signature.params)?;
        let (tag, payload) = self.builder.ins().isplit(receiver);
        let mut arguments = vec![tag, payload];
        arguments.extend(rest);
        let id = self.builder.ins().ireduce(types::I32, tag);
        let from_instance = self.builder.ins().iadd_imm(id, -imm(layout::TAG_INSTANCE));
        let has_class = self.builder.ins().icmp_imm(
            IntCC::UnsignedLessThanOrEqual,
            from_instance,
            imm(layout::TAG_ENUM_VARIANT - layout::TAG_INSTANCE),
        );
        let by_class = self.builder.create_block();
        let by_tag = self.builder.create_block();
        let found = self.builder.create_block();
        self.builder.append_block_param(found, WORD);
        self.builder
            .ins()
            .brif(has_class, by_class, &[], by_tag, &[]);

        self.builder.switch_to_block(by_class);
        let class =
            self.builder
                .ins()
                .load(WORD, slot_access(), payload, offset(layout::INSTANCE_CLASS));
        let code = self.builder.ins().load(
            WORD,
            MemFlags::trusted(),
            class,
            offset(layout::CLASS_METHODS + slot.column * layout::WORD),
        );
        self.builder.ins().jump(found, &[code.into()]);

        self.builder.switch_to_block(by_tag);
        let at = self.builder.ins().uextend(WORD, id);
        let code = self.tag_row_word(slot.column, at)?;
        self.builder.ins().jump(found, &[code.into()]);

        self.builder.switch_to_block(found);
        let code = self.builder.block_params(found)[0];
        let signature = Backend::clif_signature(arguments.len() / 2, Entry::Direct, 0);
        let signature = self.builder.import_signature(signature);
        // The callee can collect, which moves storage.
        self.forget_arrays();
        let called = unwind::Called::Indirect(signature, code);
        let produced = slot.signature.result.clone();
        if !slot.falls_back {
            let (tag, payload) = self.baml_call(&called, &arguments)?;
            let result = of_slot_words(&mut self.builder, &produced, tag, payload);
            return Ok((result, produced));
        }
        // A type that declares no `to_string` has no code in its row: the
        // runtime writes it out.
        let own = self.builder.create_block();
        let default = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, WORD);
        self.builder.set_cold_block(default);
        self.builder.ins().brif(code, own, &[], default, &[]);
        self.builder.seal_block(own);
        self.builder.seal_block(default);

        self.builder.switch_to_block(default);
        let mutator = self.mutator();
        let text = self
            .call_runtime(Runtime::StringFromAny, &[mutator, tag, payload])?
            .ok_or_else(|| internal("to string returns a string"))?;
        self.builder.ins().jump(done, &[text.into()]);

        self.builder.switch_to_block(own);
        // A string's payload is the string.
        let (_, text) = self.baml_call(&called, &arguments)?;
        self.builder.ins().jump(done, &[text.into()]);

        self.builder.seal_block(done);
        self.builder.switch_to_block(done);
        Ok((self.builder.block_params(done)[0], NativeTy::String))
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
