//! Instruction set and bytecode representation.

use baml_base::Span;
use serde::{Deserialize, Serialize};

use crate::{GlobalIndex, ObjectIndex, types::ConstValue};

// ============================================================================
// Jump Table Data Structure
// ============================================================================

/// Jump table data for O(1) switch dispatch.
///
/// Maps a contiguous range of integer values to jump offsets.
/// Values outside the range or "holes" jump to the default offset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JumpTableData {
    /// Minimum discriminant value (maps to index 0).
    pub min: i64,
    /// Jump offsets for each value from min to min+len-1.
    /// None means "hole" - should jump to default.
    pub offsets: Vec<Option<isize>>,
    /// Symbolic names for each table entry (display only).
    /// Parallel to `offsets`: `names[i]` is the name for value `min + i`.
    pub names: Vec<Option<String>>,
    /// Offset to jump to for out-of-range or hole values.
    /// Set during bytecode patching after all arms are resolved.
    pub default: isize,
}

impl JumpTableData {
    /// Create a new jump table covering the range [min, max].
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn new(min: i64, max: i64) -> Self {
        // Safety: We limit jump tables to 256 entries max in codegen,
        // and max >= min is guaranteed by construction.
        let size = (max - min + 1) as usize;
        Self {
            min,
            offsets: vec![None; size],
            names: vec![None; size],
            default: 0, // patched later
        }
    }

    /// Set the offset for a specific value.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn set(&mut self, value: i64, offset: isize) {
        // Safety: We only call this with value >= min, so index is non-negative
        // and bounded by the table size.
        let index = (value - self.min) as usize;
        if index < self.offsets.len() {
            self.offsets[index] = Some(offset);
        }
    }

    /// Set the symbolic name for a specific value.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn set_name(&mut self, value: i64, name: String) {
        let index = (value - self.min) as usize;
        if index < self.names.len() {
            self.names[index] = Some(name);
        }
    }

    /// Lookup the offset for a value.
    /// Returns None if value is out of range or is a hole.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn lookup(&self, value: i64) -> Option<isize> {
        if value < self.min {
            return None;
        }
        // Safety: value >= min, so index is non-negative.
        let index = (value - self.min) as usize;
        self.offsets.get(index).copied().flatten()
    }
}

// ============================================================================
// Perfect Hash Table Data Structure
// ============================================================================

/// Compile-time minimal perfect hash table for O(1) type dispatch.
///
/// Used by `Instruction::DenseTag` to remap a sparse type tag to a dense
/// `[0, K-1]` index for jump table dispatch. The hash function is:
///
///   `h(tag) = ((tag as u64).wrapping_mul(multiply) >> shift) & mask`
///
/// Each entry stores the expected tag for verification — if the runtime tag
/// doesn't match, the value is not in the match and dispatch falls to default.
///
/// The hash constants are found by brute-force search at compile time.
/// For K ≤ 20 arms, the search completes in microseconds.
///
/// References:
/// - Neumann & Göbbert, "Improving Switch Statement Performance with Hashing
///   Optimized at Compile Time"
/// - Dietz 1992, "Coding Multiway Branches Using Customized Hash Functions"
/// - Proposed for LLVM (issue #96971), Roslyn (#66604), Go (#34381)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchHashTable {
    /// Multiplicative hash constant, found at compile time.
    pub multiply: u64,
    /// Right-shift amount applied after multiplication.
    pub shift: u8,
    /// Bitmask applied after shift. Always `table_size - 1` (power of 2).
    pub mask: u8,
    /// Verification + dispatch entries. `entries[h(tag)]` contains:
    /// - `expected_tag`: the type tag that should hash to this slot
    /// - `dense_index`: the dense arm index `[0, K-1]` for jump table dispatch
    ///   Unused slots have `expected_tag = i64::MIN` (sentinel).
    pub entries: Vec<MatchHashEntry>,
    /// Human-readable names for keys in arm order (display only).
    /// `key_names[i]` is the name for the i-th arm (e.g. "int", "`MyClass`").
    pub key_names: Vec<String>,
}

/// Single entry in a [`MatchHashTable`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchHashEntry {
    /// The type tag expected at this slot (for verification).
    pub expected_tag: i64,
    /// Dense arm index `[0, K-1]` — fed into the subsequent jump table.
    pub dense_index: u8,
}

/// Individual bytecode instruction.
///
/// For faster iteration we'll start with an in-memory data structure that
/// represents the bytecode instead of real binary instructions since getting
/// those to work correctly is much harder (unsafe Rust, pointer arithmetic).
///
/// We do need to respect some sort of "instruction format" however. In
/// stack-based VMs some instructions don't take any arguments (for example,
/// the `ADD` instruction would grab its operands from the evaluation stack),
/// but some others such as `LOAD_CONST` need to know which constant to load,
/// so they take an unsigned integer as an argument (the index of the constant
/// in the constant pool). Same goes for jump instructions, we need to know the
/// offset.
///
/// We are not limited to one single argument, we can have variable-length
/// instructions in the VM, but we do have to keep the arguments limited to
/// "bytes" (unsigned integers, signed integers, etc). Use the arguments to
/// index into runtime structures such as constant pools, object pools, etc.
/// Don't embed complex data structures in an instruction. Avoid this:
///
/// ```ignore
/// enum Instruction {
///     MySuperDuperInstruction(HashMap<String, Vec<Function>>)
/// }
/// ```
///
/// Instead store the state or complex structure in the `Vm` struct (in `bex_vm` crate) and
/// find a way to reference it with very simple instructions.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Instruction {
    /// Loads a constant from the bytecode's constant pool.
    ///
    /// Format: `LOAD_CONST i` where `i` is the index of the constant in the
    /// [`Bytecode::constants`] pool.
    LoadConst(usize),

    /// Loads a variable from the frame's local variable slots.
    ///
    /// Format: `LOAD_VAR i` where `i` is the relative index of the variable in
    /// `Vm::stack` array.
    LoadVar(usize),

    /// Stores a value in the frame's local variable slots.
    ///
    /// Format: `STORE_VAR i` where `i` is the relative index of the variable in
    /// `Vm::stack` array.
    StoreVar(usize),

    /// Load a global variable from the `Vm::globals` array.
    ///
    /// Format: `LOAD_GLOBAL i` where `i` is the index of the global variable
    /// in the `Vm::globals` array.
    ///
    /// Note that functions are also globals and can be passed around and stored
    /// in local variables, so we need to load their name in the stack before we
    /// call the function.
    LoadGlobal(GlobalIndex),

    /// Store a value in a global variable.
    ///
    /// Format: `STORE_GLOBAL i` where `i` is the index of the global variable
    /// in the `Vm::globals` array.
    ///
    /// # Init-only invariant
    ///
    /// Only `$init` (or `$init_test`) functions may emit `StoreGlobal`. The
    /// compiler enforces this by emitting `StoreGlobal` exclusively from
    /// `compile_init_function`. Post-`$init` globals are shared across VMs as a
    /// frozen `Arc<[Value]>`; the runtime treats a `StoreGlobal` against that
    /// shared view as a `VmInternalError`. Hand-written or fuzzed bytecode that
    /// emits `StoreGlobal` outside of `$init` will be rejected at runtime.
    StoreGlobal(GlobalIndex),

    /// Load a field of an object.
    ///
    /// Format: `LOAD_FIELD i` where `i` is the index of the field in the
    /// object's fields array.
    LoadField(usize),

    /// Store the value on top of the stack in the field of an object.
    ///
    /// Format: `STORE_FIELD i` where `i` is the index of the field in the
    /// object's fields array.
    StoreField(usize),

    /// Initialize a field during construction: pops the value, stores it in the field,
    /// and keeps the instance on the stack (unlike `StoreField` which pops both).
    ///
    /// Format: `INIT_FIELD i` where `i` is the index of the field.
    InitField(usize),

    /// Pop N values from the top of `Vm::stack` (the evaluation stack).
    ///
    /// Format: `POP n` where `n` is the number of values to pop.
    Pop(usize),

    /// Copy the i-th value from the top of the stack to the top.
    ///
    /// Format: `COPY i` where `i` is the offset from the top of the stack.
    /// `COPY 0` copies the top element (duplicates it).
    /// `COPY 1` copies the second element from the top.
    Copy(usize),

    /// Jump to another instruction.
    ///
    /// Format: `JUMP o` where `o` is the offset from the current instruction
    /// to the target instruction (can be negative to jump backwards).
    Jump(isize),

    /// Pop the condition and jump to another instruction if it is false.
    ///
    /// Format: `POP_JUMP_IF_FALSE o` where `o` is the offset from the current
    /// instruction to the target instruction (can be negative to jump
    /// backwards).
    ///
    /// This instruction pops the condition value from the stack after checking
    /// it, ensuring the condition doesn't leak on the evaluation stack.
    PopJumpIfFalse(isize),

    /// Peek at the top of the stack and jump if the value is false.
    /// Unlike `PopJumpIfFalse`, this does NOT pop the value — it stays on
    /// the stack regardless of the branch taken. Used for short-circuit
    /// `&&` / `||` where the tested value is also the expression result.
    JumpIfFalse(isize),

    /// Performs an arithmetic binary operation.
    ///
    /// Format: `BIN_OP op` where `op` is the binary operation to perform.
    BinOp(BinOp),

    /// Performs a comparison binary operation.
    ///
    /// Format: `CMP_OP op` where `op` is the comparison operation to perform.
    CmpOp(CmpOp),

    // ── Specialized arithmetic (type dispatch eliminated at compile time) ──
    /// `[left: Int, right: Int] → [Int]`
    AddInt,
    /// `[left: Int, right: Int] → [Int]`
    SubInt,
    /// `[left: Int, right: Int] → [Int]`
    MulInt,
    /// `[left: Int, right: Int] → [Int]` — throws `DivisionByZero` if right == 0
    DivInt,
    /// `[left: Int, right: Int] → [Int]`
    ModInt,

    /// `[left: Float, right: Float] → [Float]`
    AddFloat,
    /// `[left: Float, right: Float] → [Float]`
    SubFloat,
    /// `[left: Float, right: Float] → [Float]`
    MulFloat,
    /// `[left: Float, right: Float] → [Float]` — throws `DivisionByZero` if right == 0.0
    DivFloat,

    /// `[left: Int, right: Int] → [Bool]`
    CmpIntOp(CmpOp),
    /// `[left: Float, right: Float] → [Bool]`
    CmpFloatOp(CmpOp),

    /// Performs a unary operation.
    ///
    /// Format: `UNARY_OP op` where `op` is the unary operation to perform.
    UnaryOp(UnaryOp),

    /// Builds an array and allocates it on the heap.
    ///
    /// Format: `ALLOC_ARRAY n` where `n` is the number of elements in the
    /// array. All elements must be on the stack by the time this instruction is
    /// executed.
    AllocArray(usize),

    /// Builds a map and allocates it on the heap.
    ///
    /// Format `ALLOC_MAP n` where `n` is the number of entries in the map.
    /// `n` keys are popped first and then `n` values are popped after that.
    /// In total that's 2n stack required before the instruction is executed.
    AllocMap(usize),

    /// Loads an element from an array at a given index.
    ///
    /// Format: `LOAD_ARRAY_ELEMENT` where the stack contains [array, index] and
    /// the result is the element at that index.
    LoadArrayElement,

    /// Loads a value from a map at a given key.
    ///
    /// Format: `LOAD_MAP_ELEMENT` where the stack contains [map, key] and
    /// the result is the value at that key.
    LoadMapElement,

    /// Stores a value into an array at a given index.
    ///
    /// Format: `STORE_ARRAY_ELEMENT` where the stack contains \[array, index, value\]
    /// and stores the value at `array[index]`.
    StoreArrayElement,

    /// Stores a value into a map at a given key.
    ///
    /// Format: `STORE_MAP_ELEMENT` where the stack contains \[map, key, value\]
    /// and stores the value at `map[key]`.
    StoreMapElement,

    /// Builds an instance of a class and allocates it on the heap.
    ///
    /// Format: `ALLOC_INSTANCE { class_obj: i, ntypeargs: n }` where `i` is
    /// the index of the class in the `Vm::objects` array and `n` type-arg
    /// `Object::Type` values sit on the stack below any pending field values
    /// (popped before the instance is created).  `n == 0` for non-generic
    /// classes.
    AllocInstance {
        class_obj: ObjectIndex,
        ntypeargs: u16,
    },

    /// Builds a variant of an enum and allocates it on the heap.
    ///
    /// Format: `ALLOC_VARIANT i` where `i` is the index of the enum in the
    /// `Vm::objects` array.
    AllocVariant(ObjectIndex),

    /// BEP-034 phase D′: invoke a statically-known global sys-op and
    /// push its return value back on the stack in a single VM↔engine
    /// round trip.
    ///
    /// Format: `SYS_OP g` where `g` is the global index of the sys-op
    /// function. Arguments are popped from the eval stack (arity from
    /// the callee's metadata, same as `ScheduleFuture`).
    ///
    /// Yields `VmExecState::SysOp { operation, args }`. The engine runs
    /// the operation, races it against the active cancel token, and
    /// pushes the resulting value back on the stack before resuming.
    /// No `Object::Future` is allocated and no `FutureManager` entry is
    /// created — sys-ops are not user-observable futures in BAML, so
    /// the schedule + await pair is pure overhead.
    SysOp(GlobalIndex),

    /// BEP-034 `spawn { body }`. Pops `[closure_value, name_value]` from
    /// the stack, allocates an `UnscheduledFuture { closure, name }`
    /// into the TLAB, and yields `VmExecState::Spawn(ptr)` so the
    /// engine routes the closure to a fresh `BexThread`.
    Spawn,

    /// Awaits the future on top of the stack.
    ///
    /// VM yields execution back to the embedder because it is blocked awaiting
    /// a future. But obviously, the VM will not "block", it just returns
    /// control flow to the embedder and doesn't care about anything else.
    Await,

    /// Creates a watched var and tracks its state.
    ///
    /// Format: `WATCH i` where `i` is the relative index of the variable in the
    /// `Vm::stack` array.
    Watch(usize),

    /// Unregisters a watched variable when it goes out of scope.
    ///
    /// Format: `UNWATCH i` where `i` is the relative index of the variable in the
    /// `Vm::stack` array.
    Unwatch(usize),

    /// Manually triggers notifications for a watched variable.
    Notify(usize),

    /// Call a statically-known global function.
    ///
    /// Format: `CALL g ntypeargs` where `g` is the global index of the callee
    /// function and `ntypeargs` is the number of type-argument `Object::Type`
    /// values that precede the regular arguments on the eval stack.
    ///
    /// Stack layout (top-of-stack on the right):
    ///
    /// ```text
    /// [type_arg_0, ..., type_arg_{ntypeargs-1}, val_arg_0, ..., val_arg_{nargs-1}]
    /// ```
    ///
    /// The VM pops `ntypeargs` `Object::Type` values into the new frame's
    /// `type_args` vector, then pops `nargs` regular value arguments.
    /// `nargs` is inferred from the function's arity metadata.
    ///
    /// When no type arguments are threaded, set `ntypeargs = 0`.
    Call {
        /// Global index of the callee function.
        callee: GlobalIndex,
        /// Number of type-argument `Object::Type` values on the stack
        /// immediately below the regular value arguments.
        ntypeargs: u16,
    },

    /// Call a function value from the eval stack.
    ///
    /// Format: `CALL_INDIRECT`.
    ///
    /// Stack layout: `[arg1, ..., argN, callee]`.
    ///
    /// Arity is read from the runtime callee function object.
    CallIndirect,

    /// Throw the value on top of the stack.
    ///
    /// Stack: `[error_value]` -> `[]` (control transfers to unwind handler or caller)
    Throw,

    /// Return from a function.
    ///
    /// No arguments needed, result is stored in the eval stack and the VM
    /// simply has to clean up the call stack and continue execution.
    Return,

    /// Notifies about entering or exiting a block.
    ///
    /// Format: `NOTIFY_BLOCK block_index` where `block_index` is the index
    /// into the current function's `block_notifications` array.
    NotifyBlock(usize),

    /// Enter a visualization node.
    ///
    /// Format: `VIZ_ENTER i` where `i` is the index into the current
    /// function's `viz_nodes` array.
    VizEnter(usize),

    /// Exit a visualization node.
    ///
    /// Format: `VIZ_EXIT i` where `i` is the index into the current
    /// function's `viz_nodes` array.
    VizExit(usize),

    /// Jump through a table based on integer discriminant.
    ///
    /// Stack: `[discriminant: Int]` -> `[]` (jumps)
    ///
    /// Pops discriminant, looks up in jump table at `table_idx`.
    /// If value is in range and not a hole, jumps to that offset.
    /// Otherwise jumps to `default` offset.
    ///
    /// The default offset for out-of-range or hole values is stored in
    /// `Bytecode::jump_tables[idx].default`.
    JumpTable(usize),

    /// Extract the variant index from an enum value.
    ///
    /// Stack: `[enum_value: Variant]` -> `[discriminant: Int]`
    ///
    /// Used to convert enum values to integers for jump table dispatch.
    /// Example: `Status.Active -> 0`, `Status.Inactive -> 1`, `Status.Pending -> 2`
    Discriminant,

    /// Extract the runtime type tag from any value.
    ///
    /// Stack: `[any_value]` -> `[type_tag: Int]`
    ///
    /// Used for jump table dispatch on union types (type patterns in match).
    /// Type tags are global constants:
    /// - Primitives: `int=0`, `string=1`, `bool=2`, `null=3`, `float=4`
    /// - Classes: assigned unique IDs starting at 100
    TypeTag,

    /// Check if the value on top of the stack matches the type identified by
    /// the constant at index `i`. The constant is either:
    /// - `Value::Object(class_ptr)` — class identity check (`inst.class == class_ptr`)
    /// - `Value::Int(tag)` — type tag check (`value_type_tag(value) == tag`)
    ///
    /// Pops the value, pushes `Bool` result.
    IsType(usize),

    /// Materialise a `Ty` from a constant-pool `TyTemplate`, substituting
    /// any `TypeArgRef(n)` leaves with `frame.type_args[n]`.
    ///
    /// Pushes `Value::Object(Object::Type(ty))`.
    ///
    /// For fully-concrete templates (no `TypeArgRef`), no substitution walk
    /// is performed — the concrete `Ty` is cloned directly.
    ///
    /// Format: `LOAD_TYPE i` where `i` indexes into `Bytecode::constants`
    /// which must hold a `ConstValue::Type(TyTemplate)` at that slot.
    LoadType(usize),

    /// Remap a sparse type tag to a dense index via perfect hash lookup.
    ///
    /// Pops the type tag (from a preceding `TypeTag` instruction), computes
    /// `h(tag) = ((tag as u64).wrapping_mul(M) >> S) & mask`, verifies the
    /// entry's expected tag, and pushes the dense arm index. On verification
    /// failure (tag not in the match), pushes `-1` as a sentinel — the
    /// subsequent `JumpTable`'s default arm handles this.
    ///
    /// Design rationale: perfect hashing replaces O(log K) `BinarySearch` with
    /// O(1) dispatch for sparse ≥4-arm `TypeTag` switches. Memory per match
    /// site scales with K (arms) not N (total classes). See [`MatchHashTable`]
    /// for algorithm references.
    DenseTag(usize),

    /// If the top-of-stack value is a panic instance (`baml.panics.*`), throw it.
    /// Otherwise pop the value and continue to the next instruction.
    ///
    /// Stack: `[value]` -> `[]` (continues) or unwinds (throws)
    ///
    /// Used in catch handlers before wildcard arms to prevent them from
    /// swallowing panics the programmer didn't explicitly name.
    ThrowIfPanic,

    /// Halt execution with an unreachable code error.
    ///
    /// This instruction should never be executed at runtime. If it is,
    /// it indicates a bug in the compiler or type system (e.g., a non-exhaustive
    /// match expression that the compiler incorrectly marked as exhaustive).
    ///
    /// Throws `RuntimeError::Unreachable` (in `bex_vm` crate).
    Unreachable,

    /// Allocate a `Closure` object wrapping a function from the object pool.
    ///
    /// Stack layout (top-of-stack on the right):
    ///
    /// ```text
    /// [type_arg_0, ..., type_arg_{ntypeargs-1}, cap_0, cap_1, ..., cap_{capture_count-1}]
    /// ```
    ///
    /// 1. Pop `capture_count` cell values (left-to-right order, reversed after
    ///    popping) into `Closure::captures`.
    /// 2. Pop `ntypeargs` `Object::Type` values into `Closure::captured_type_args`.
    /// 3. Push the resulting `Object::Closure`.
    ///
    /// When there are no enclosing type parameters, `ntypeargs = 0` and step 2
    /// is a no-op (backward-compatible with all existing call sites).
    ///
    /// Note: this struct variant carries two `usize` payloads on top of an
    /// `ObjectIndex`, which keeps `size_of::<Instruction>() == 24` naturally.
    /// 24 bytes is the optimal enum size on `AArch64`; benchmarks showed 16-byte
    /// enums regressed perf by 5-12% (worse LLVM codegen, register allocation,
    /// and branch structure — not cache pressure). `MakeClosure` being the
    /// largest variant locks the size at 24 without needing a synthetic pad.
    MakeClosure {
        /// Index into the object pool for the underlying `Object::Function`.
        obj_idx: ObjectIndex,
        /// Number of cell captures to pop from the stack.
        capture_count: usize,
        /// Number of `Object::Type` values to pop from the stack into
        /// `Closure::captured_type_args`.  Zero for non-generic contexts.
        ntypeargs: usize,
    },

    /// Create a bound method from a global function index and a receiver on the stack.
    ///
    /// Pops the receiver from the stack, looks up the function at `global_idx` in
    /// `Vm::globals`, and pushes the resulting `Object::BoundMethod`.
    ///
    /// Stack: `[receiver]` -> `[bound_method]`
    MakeBoundMethod(GlobalIndex),

    /// Wrap the top-of-stack value in a `Cell` object.
    ///
    /// Stack: `[value]` -> `[cell]`
    MakeCell,

    /// Load the value stored inside a `Cell` local variable.
    ///
    /// The local at `slot` must hold an `Object::Cell`.
    ///
    /// Stack: `[]` -> `[value]`
    LoadDeref(usize),

    /// Store a value into a `Cell` local variable.
    ///
    /// The local at `slot` must hold an `Object::Cell`.
    ///
    /// Stack: `[value]` -> `[]`
    StoreDeref(usize),

    /// Load a value from a capture slot of the current closure.
    ///
    /// The current frame's function must be an `Object::Closure`. Reads through
    /// the cell at `captures[idx]`.
    ///
    /// Stack: `[]` -> `[value]`
    LoadCapture(usize),

    /// Store a value into a capture slot of the current closure.
    ///
    /// The current frame's function must be an `Object::Closure`. Writes through
    /// the cell at `captures[idx]`.
    ///
    /// Stack: `[value]` -> `[]`
    StoreCapture(usize),

    /// Load the raw cell pointer from a capture slot of the current closure.
    ///
    /// Unlike `LoadCapture`, which reads through the cell to obtain the inner
    /// value, `CaptureRef` pushes the cell object pointer itself.  Used when
    /// forwarding a captured cell to an inner (nested) closure so both closures
    /// share the same cell.
    ///
    /// The current frame's function must be an `Object::Closure`.
    ///
    /// Stack: `[]` -> `[cell_ptr]`
    CaptureRef(usize),

    /// Pop (`event_name`: String, data: any) from stack and yield to embedder.
    ///
    /// Stack: `[event_name: String, data: any]` -> `[]`
    ///
    /// The VM yields `VmExecState::Event { event_name, data }` so the engine
    /// can emit a `CustomEvent` with full span context. Execution resumes
    /// after the engine processes the event.
    SendEvent,
}

/// Compact bytecode opcodes.
///
/// Each variant maps to a 1-byte opcode in the `CompactCode.code` stream.
/// The operand format is determined by the opcode — see `OpCode::encoded_size()`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpCode {
    // ── Unit ops (no operands, 1 byte) ─────────────────────────
    Return = 0,
    Await,
    Throw,
    LoadArrayElement,
    LoadMapElement,
    StoreArrayElement,
    StoreMapElement,
    CallIndirect,
    Discriminant,
    TypeTag,
    ThrowIfPanic,
    Unreachable,
    MakeCell,
    SendEvent,

    // ── Expanded arithmetic (no operands, 1 byte) ──────────────
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,

    // ── Expanded comparison (no operands, 1 byte) ──────────────
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,

    // ── Specialized arithmetic (no operands, 1 byte) ────────────
    // These skip type checks using unreachable_unchecked — the compiler
    // guarantees operand types at emit time.
    AddInt,
    SubInt,
    MulInt,
    DivInt,
    ModInt,
    AddFloat,
    SubFloat,
    MulFloat,
    DivFloat,

    // ── Specialized comparison (no operands, 1 byte) ───────────
    CmpIntEq,
    CmpIntNotEq,
    CmpIntLt,
    CmpIntLtEq,
    CmpIntGt,
    CmpIntGtEq,
    CmpFloatEq,
    CmpFloatNotEq,
    CmpFloatLt,
    CmpFloatLtEq,
    CmpFloatGt,
    CmpFloatGtEq,

    // ── Expanded unary (no operands, 1 byte) ───────────────────
    Not,
    Neg,

    // ── Common constants (1-2 bytes) ───────────────────────────
    LoadNull,     // 1 byte
    LoadTrue,     // 1 byte
    LoadFalse,    // 1 byte
    LoadIntSmall, // 2 bytes: opcode + i8

    // ── Single u32 operand (5 bytes) ───────────────────────────
    LoadConst,
    LoadVar,
    StoreVar,
    LoadGlobal,
    StoreGlobal,
    LoadField,
    StoreField,
    InitField,
    Pop,
    Copy,
    AllocArray,
    AllocMap,
    AllocInstance,
    AllocVariant,
    SysOp,
    Spawn,
    Watch,
    Unwatch,
    Notify,
    Call,
    NotifyBlock,
    VizEnter,
    VizExit,
    IsType,
    DenseTag,
    LoadType,
    MakeBoundMethod,
    LoadDeref,
    StoreDeref,
    LoadCapture,
    StoreCapture,
    CaptureRef,

    // ── Jump i32 operand (5 bytes) ─────────────────────────────
    Jump,
    PopJumpIfFalse,
    JumpIfFalse,

    // ── Two operands (9 bytes) ─────────────────────────────────
    JumpTable,   // u32 table_idx + i32 default_offset
    MakeClosure, // u32 object_idx (capture_count is popped from the stack)
}

impl OpCode {
    /// Total encoded size in bytes (opcode + operands).
    pub const fn encoded_size(self) -> usize {
        match self {
            // Unit ops + expanded arith/cmp/unary + LoadNull/LoadTrue/LoadFalse
            Self::Return
            | Self::Await
            | Self::Throw
            | Self::LoadArrayElement
            | Self::LoadMapElement
            | Self::StoreArrayElement
            | Self::StoreMapElement
            | Self::CallIndirect
            | Self::Discriminant
            | Self::TypeTag
            | Self::ThrowIfPanic
            | Self::Unreachable
            | Self::MakeCell
            | Self::SendEvent
            | Self::Spawn
            | Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Mod
            | Self::BitAnd
            | Self::BitOr
            | Self::BitXor
            | Self::Shl
            | Self::Shr
            | Self::Eq
            | Self::NotEq
            | Self::Lt
            | Self::LtEq
            | Self::Gt
            | Self::GtEq
            | Self::AddInt
            | Self::SubInt
            | Self::MulInt
            | Self::DivInt
            | Self::ModInt
            | Self::AddFloat
            | Self::SubFloat
            | Self::MulFloat
            | Self::DivFloat
            | Self::CmpIntEq
            | Self::CmpIntNotEq
            | Self::CmpIntLt
            | Self::CmpIntLtEq
            | Self::CmpIntGt
            | Self::CmpIntGtEq
            | Self::CmpFloatEq
            | Self::CmpFloatNotEq
            | Self::CmpFloatLt
            | Self::CmpFloatLtEq
            | Self::CmpFloatGt
            | Self::CmpFloatGtEq
            | Self::Not
            | Self::Neg
            | Self::LoadNull
            | Self::LoadTrue
            | Self::LoadFalse => 1,

            // 2-byte: opcode + i8
            Self::LoadIntSmall => 2,

            // 5-byte: opcode + u32/i32
            Self::LoadConst
            | Self::LoadVar
            | Self::StoreVar
            | Self::LoadGlobal
            | Self::StoreGlobal
            | Self::LoadField
            | Self::StoreField
            | Self::InitField
            | Self::Pop
            | Self::Copy
            | Self::AllocArray
            | Self::AllocMap
            | Self::AllocVariant
            | Self::SysOp
            | Self::Watch
            | Self::Unwatch
            | Self::Notify
            | Self::NotifyBlock
            | Self::VizEnter
            | Self::VizExit
            | Self::IsType
            | Self::DenseTag
            | Self::LoadType
            | Self::MakeBoundMethod
            | Self::LoadDeref
            | Self::StoreDeref
            | Self::LoadCapture
            | Self::StoreCapture
            | Self::CaptureRef
            | Self::Jump
            | Self::PopJumpIfFalse
            | Self::JumpIfFalse => 5,

            // 7-byte: opcode + u32 + u16 (type-arg threading)
            Self::AllocInstance | Self::Call => 7,

            // 9-byte: opcode + u32 + u16 + u16 (closure with capture+typearg counts)
            Self::MakeClosure => 9,

            // 9-byte: opcode + u32 + i32
            Self::JumpTable => 9,
        }
    }
}

impl TryFrom<u8> for OpCode {
    type Error = u8;
    #[allow(clippy::too_many_lines)]
    fn try_from(byte: u8) -> Result<Self, u8> {
        match byte {
            x if x == Self::Return as u8 => Ok(Self::Return),
            x if x == Self::Await as u8 => Ok(Self::Await),
            x if x == Self::Throw as u8 => Ok(Self::Throw),
            x if x == Self::LoadArrayElement as u8 => Ok(Self::LoadArrayElement),
            x if x == Self::LoadMapElement as u8 => Ok(Self::LoadMapElement),
            x if x == Self::StoreArrayElement as u8 => Ok(Self::StoreArrayElement),
            x if x == Self::StoreMapElement as u8 => Ok(Self::StoreMapElement),
            x if x == Self::CallIndirect as u8 => Ok(Self::CallIndirect),
            x if x == Self::Discriminant as u8 => Ok(Self::Discriminant),
            x if x == Self::TypeTag as u8 => Ok(Self::TypeTag),
            x if x == Self::ThrowIfPanic as u8 => Ok(Self::ThrowIfPanic),
            x if x == Self::Unreachable as u8 => Ok(Self::Unreachable),
            x if x == Self::MakeCell as u8 => Ok(Self::MakeCell),
            x if x == Self::SendEvent as u8 => Ok(Self::SendEvent),
            x if x == Self::Add as u8 => Ok(Self::Add),
            x if x == Self::Sub as u8 => Ok(Self::Sub),
            x if x == Self::Mul as u8 => Ok(Self::Mul),
            x if x == Self::Div as u8 => Ok(Self::Div),
            x if x == Self::Mod as u8 => Ok(Self::Mod),
            x if x == Self::BitAnd as u8 => Ok(Self::BitAnd),
            x if x == Self::BitOr as u8 => Ok(Self::BitOr),
            x if x == Self::BitXor as u8 => Ok(Self::BitXor),
            x if x == Self::Shl as u8 => Ok(Self::Shl),
            x if x == Self::Shr as u8 => Ok(Self::Shr),
            x if x == Self::Eq as u8 => Ok(Self::Eq),
            x if x == Self::NotEq as u8 => Ok(Self::NotEq),
            x if x == Self::Lt as u8 => Ok(Self::Lt),
            x if x == Self::LtEq as u8 => Ok(Self::LtEq),
            x if x == Self::Gt as u8 => Ok(Self::Gt),
            x if x == Self::GtEq as u8 => Ok(Self::GtEq),
            x if x == Self::AddInt as u8 => Ok(Self::AddInt),
            x if x == Self::SubInt as u8 => Ok(Self::SubInt),
            x if x == Self::MulInt as u8 => Ok(Self::MulInt),
            x if x == Self::DivInt as u8 => Ok(Self::DivInt),
            x if x == Self::ModInt as u8 => Ok(Self::ModInt),
            x if x == Self::AddFloat as u8 => Ok(Self::AddFloat),
            x if x == Self::SubFloat as u8 => Ok(Self::SubFloat),
            x if x == Self::MulFloat as u8 => Ok(Self::MulFloat),
            x if x == Self::DivFloat as u8 => Ok(Self::DivFloat),
            x if x == Self::CmpIntEq as u8 => Ok(Self::CmpIntEq),
            x if x == Self::CmpIntNotEq as u8 => Ok(Self::CmpIntNotEq),
            x if x == Self::CmpIntLt as u8 => Ok(Self::CmpIntLt),
            x if x == Self::CmpIntLtEq as u8 => Ok(Self::CmpIntLtEq),
            x if x == Self::CmpIntGt as u8 => Ok(Self::CmpIntGt),
            x if x == Self::CmpIntGtEq as u8 => Ok(Self::CmpIntGtEq),
            x if x == Self::CmpFloatEq as u8 => Ok(Self::CmpFloatEq),
            x if x == Self::CmpFloatNotEq as u8 => Ok(Self::CmpFloatNotEq),
            x if x == Self::CmpFloatLt as u8 => Ok(Self::CmpFloatLt),
            x if x == Self::CmpFloatLtEq as u8 => Ok(Self::CmpFloatLtEq),
            x if x == Self::CmpFloatGt as u8 => Ok(Self::CmpFloatGt),
            x if x == Self::CmpFloatGtEq as u8 => Ok(Self::CmpFloatGtEq),
            x if x == Self::Not as u8 => Ok(Self::Not),
            x if x == Self::Neg as u8 => Ok(Self::Neg),
            x if x == Self::LoadNull as u8 => Ok(Self::LoadNull),
            x if x == Self::LoadTrue as u8 => Ok(Self::LoadTrue),
            x if x == Self::LoadFalse as u8 => Ok(Self::LoadFalse),
            x if x == Self::LoadIntSmall as u8 => Ok(Self::LoadIntSmall),
            x if x == Self::LoadConst as u8 => Ok(Self::LoadConst),
            x if x == Self::LoadVar as u8 => Ok(Self::LoadVar),
            x if x == Self::StoreVar as u8 => Ok(Self::StoreVar),
            x if x == Self::LoadGlobal as u8 => Ok(Self::LoadGlobal),
            x if x == Self::StoreGlobal as u8 => Ok(Self::StoreGlobal),
            x if x == Self::LoadField as u8 => Ok(Self::LoadField),
            x if x == Self::StoreField as u8 => Ok(Self::StoreField),
            x if x == Self::InitField as u8 => Ok(Self::InitField),
            x if x == Self::Pop as u8 => Ok(Self::Pop),
            x if x == Self::Copy as u8 => Ok(Self::Copy),
            x if x == Self::AllocArray as u8 => Ok(Self::AllocArray),
            x if x == Self::AllocMap as u8 => Ok(Self::AllocMap),
            x if x == Self::AllocInstance as u8 => Ok(Self::AllocInstance),
            x if x == Self::AllocVariant as u8 => Ok(Self::AllocVariant),
            x if x == Self::SysOp as u8 => Ok(Self::SysOp),
            x if x == Self::Spawn as u8 => Ok(Self::Spawn),
            x if x == Self::Watch as u8 => Ok(Self::Watch),
            x if x == Self::Unwatch as u8 => Ok(Self::Unwatch),
            x if x == Self::Notify as u8 => Ok(Self::Notify),
            x if x == Self::Call as u8 => Ok(Self::Call),
            x if x == Self::NotifyBlock as u8 => Ok(Self::NotifyBlock),
            x if x == Self::VizEnter as u8 => Ok(Self::VizEnter),
            x if x == Self::VizExit as u8 => Ok(Self::VizExit),
            x if x == Self::IsType as u8 => Ok(Self::IsType),
            x if x == Self::DenseTag as u8 => Ok(Self::DenseTag),
            x if x == Self::LoadType as u8 => Ok(Self::LoadType),
            x if x == Self::MakeBoundMethod as u8 => Ok(Self::MakeBoundMethod),
            x if x == Self::LoadDeref as u8 => Ok(Self::LoadDeref),
            x if x == Self::StoreDeref as u8 => Ok(Self::StoreDeref),
            x if x == Self::LoadCapture as u8 => Ok(Self::LoadCapture),
            x if x == Self::StoreCapture as u8 => Ok(Self::StoreCapture),
            x if x == Self::CaptureRef as u8 => Ok(Self::CaptureRef),
            x if x == Self::Jump as u8 => Ok(Self::Jump),
            x if x == Self::PopJumpIfFalse as u8 => Ok(Self::PopJumpIfFalse),
            x if x == Self::JumpIfFalse as u8 => Ok(Self::JumpIfFalse),
            x if x == Self::JumpTable as u8 => Ok(Self::JumpTable),
            x if x == Self::MakeClosure as u8 => Ok(Self::MakeClosure),
            _ => Err(byte),
        }
    }
}

impl std::fmt::Display for OpCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Return => "RETURN",
            Self::Await => "AWAIT",
            Self::Throw => "THROW",
            Self::LoadArrayElement => "LOAD_ARRAY_ELEMENT",
            Self::LoadMapElement => "LOAD_MAP_ELEMENT",
            Self::StoreArrayElement => "STORE_ARRAY_ELEMENT",
            Self::StoreMapElement => "STORE_MAP_ELEMENT",
            Self::CallIndirect => "CALL_INDIRECT",
            Self::Discriminant => "DISCRIMINANT",
            Self::TypeTag => "TYPE_TAG",
            Self::ThrowIfPanic => "THROW_IF_PANIC",
            Self::Unreachable => "UNREACHABLE",
            Self::MakeCell => "MAKE_CELL",
            Self::SendEvent => "SEND_EVENT",
            Self::Add => "ADD",
            Self::Sub => "SUB",
            Self::Mul => "MUL",
            Self::Div => "DIV",
            Self::Mod => "MOD",
            Self::BitAnd => "BIT_AND",
            Self::BitOr => "BIT_OR",
            Self::BitXor => "BIT_XOR",
            Self::Shl => "SHL",
            Self::Shr => "SHR",
            Self::Eq => "EQ",
            Self::NotEq => "NOT_EQ",
            Self::Lt => "LT",
            Self::LtEq => "LT_EQ",
            Self::Gt => "GT",
            Self::GtEq => "GT_EQ",
            Self::AddInt => "ADD_INT",
            Self::SubInt => "SUB_INT",
            Self::MulInt => "MUL_INT",
            Self::DivInt => "DIV_INT",
            Self::ModInt => "MOD_INT",
            Self::AddFloat => "ADD_FLOAT",
            Self::SubFloat => "SUB_FLOAT",
            Self::MulFloat => "MUL_FLOAT",
            Self::DivFloat => "DIV_FLOAT",
            Self::CmpIntEq => "CMP_INT_EQ",
            Self::CmpIntNotEq => "CMP_INT_NOT_EQ",
            Self::CmpIntLt => "CMP_INT_LT",
            Self::CmpIntLtEq => "CMP_INT_LT_EQ",
            Self::CmpIntGt => "CMP_INT_GT",
            Self::CmpIntGtEq => "CMP_INT_GT_EQ",
            Self::CmpFloatEq => "CMP_FLOAT_EQ",
            Self::CmpFloatNotEq => "CMP_FLOAT_NOT_EQ",
            Self::CmpFloatLt => "CMP_FLOAT_LT",
            Self::CmpFloatLtEq => "CMP_FLOAT_LT_EQ",
            Self::CmpFloatGt => "CMP_FLOAT_GT",
            Self::CmpFloatGtEq => "CMP_FLOAT_GT_EQ",
            Self::Not => "NOT",
            Self::Neg => "NEG",
            Self::LoadNull => "LOAD_NULL",
            Self::LoadTrue => "LOAD_TRUE",
            Self::LoadFalse => "LOAD_FALSE",
            Self::LoadIntSmall => "LOAD_INT_SMALL",
            Self::LoadConst => "LOAD_CONST",
            Self::LoadVar => "LOAD_VAR",
            Self::StoreVar => "STORE_VAR",
            Self::LoadGlobal => "LOAD_GLOBAL",
            Self::StoreGlobal => "STORE_GLOBAL",
            Self::LoadField => "LOAD_FIELD",
            Self::StoreField => "STORE_FIELD",
            Self::InitField => "INIT_FIELD",
            Self::Pop => "POP",
            Self::Copy => "COPY",
            Self::AllocArray => "ALLOC_ARRAY",
            Self::AllocMap => "ALLOC_MAP",
            Self::AllocInstance => "ALLOC_INSTANCE",
            Self::AllocVariant => "ALLOC_VARIANT",
            Self::SysOp => "SYS_OP",
            Self::Spawn => "SPAWN",
            Self::Watch => "WATCH",
            Self::Unwatch => "UNWATCH",
            Self::Notify => "NOTIFY",
            Self::Call => "CALL",
            Self::NotifyBlock => "NOTIFY_BLOCK",
            Self::VizEnter => "VIZ_ENTER",
            Self::VizExit => "VIZ_EXIT",
            Self::IsType => "IS_TYPE",
            Self::DenseTag => "DENSE_TAG",
            Self::LoadType => "LOAD_TYPE",
            Self::MakeBoundMethod => "MAKE_BOUND_METHOD",
            Self::LoadDeref => "LOAD_DEREF",
            Self::StoreDeref => "STORE_DEREF",
            Self::LoadCapture => "LOAD_CAPTURE",
            Self::StoreCapture => "STORE_CAPTURE",
            Self::CaptureRef => "CAPTURE_REF",
            Self::Jump => "JUMP",
            Self::PopJumpIfFalse => "POP_JUMP_IF_FALSE",
            Self::JumpIfFalse => "JUMP_IF_FALSE",
            Self::JumpTable => "JUMP_TABLE",
            Self::MakeClosure => "MAKE_CLOSURE",
        };
        f.write_str(name)
    }
}

/// Read a little-endian u32 from `code[*pc..*pc+4]` and advance `*pc` by 4.
#[inline]
pub fn read_u32(code: &[u8], pc: &mut usize) -> u32 {
    let val = u32::from_le_bytes(code[*pc..*pc + 4].try_into().unwrap());
    *pc += 4;
    val
}

/// Read a little-endian u16 from `code[*pc..*pc+2]` and advance `*pc` by 2.
#[inline]
pub fn read_u16(code: &[u8], pc: &mut usize) -> u16 {
    let val = u16::from_le_bytes(code[*pc..*pc + 2].try_into().unwrap());
    *pc += 2;
    val
}

/// Read a little-endian i32 from `code[*pc..*pc+4]` and advance `*pc` by 4.
#[inline]
pub fn read_i32(code: &[u8], pc: &mut usize) -> i32 {
    let val = i32::from_le_bytes(code[*pc..*pc + 4].try_into().unwrap());
    *pc += 4;
    val
}

/// Read a signed byte from `code[*pc]` and advance `*pc` by 1.
#[inline]
#[allow(clippy::cast_possible_wrap)]
pub fn read_i8(code: &[u8], pc: &mut usize) -> i8 {
    let val = code[*pc] as i8;
    *pc += 1;
    val
}

/// Block notification metadata stored in the Function struct.
/// The `function_name` field is populated at runtime from the Function containing this notification.

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockNotification {
    pub function_name: String, // Populated at runtime from Function::name
    pub block_name: String,
    pub level: usize,
    pub block_type: BlockNotificationType,
    pub is_enter: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlockNotificationType {
    Statement,
    If,
    While,
    For,
    Function,
}

/// Visualization node metadata stored in the Function struct.
/// Used for control flow visualization (branches, loops, scopes).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VizNodeMeta {
    /// Unique node ID within this function.
    pub node_id: u32,
    /// Encoded log filter key for this node.
    pub log_filter_key: String,
    /// Parent node's log filter key (None for root).
    pub parent_log_filter_key: Option<String>,
    /// Type of this visualization node.
    pub node_type: VizNodeType,
    /// Human-readable label for this node.
    pub label: String,
    /// Header level (only for `HeaderContextEnter`).
    pub header_level: Option<u8>,
}

/// Type of visualization node.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum VizNodeType {
    /// Root of a function's control flow.
    FunctionRoot,
    /// Header context from `//# header` annotation.
    HeaderContextEnter,
    /// Group of branches (if-else chain).
    BranchGroup,
    /// Single branch arm (if/else if/else).
    BranchArm,
    /// Loop construct (while/for).
    Loop,
    /// Other block scope.
    OtherScope,
}

/// Delta type for viz execution events.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum VizExecDelta {
    /// Entering a visualization node.
    Enter,
    /// Exiting a visualization node.
    Exit,
}

/// Visualization execution event emitted when entering/exiting a viz node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VizExecEvent {
    /// Enter or exit.
    pub delta: VizExecDelta,
    /// Node ID within the function.
    pub node_id: u32,
    /// Type of the node.
    pub node_type: VizNodeType,
    /// Human-readable label.
    pub label: String,
    /// Header level (for `HeaderContextEnter`).
    pub header_level: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Neg,
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        })
    }
}

impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtEq => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtEq => ">=",
        })
    }
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            UnaryOp::Not => "!",
            UnaryOp::Neg => "-",
        })
    }
}

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instruction::LoadConst(i) => write!(f, "LOAD_CONST {i}"),
            Instruction::LoadVar(i) => write!(f, "LOAD_VAR {i}"),
            Instruction::StoreVar(i) => write!(f, "STORE_VAR {i}"),
            Instruction::LoadGlobal(i) => write!(f, "LOAD_GLOBAL {i}"),
            Instruction::StoreGlobal(i) => write!(f, "STORE_GLOBAL {i}"),
            Instruction::LoadField(i) => write!(f, "LOAD_FIELD {i}"),
            Instruction::StoreField(i) => write!(f, "STORE_FIELD {i}"),
            Instruction::InitField(i) => write!(f, "INIT_FIELD {i}"),
            Instruction::Pop(n) => write!(f, "POP {n}"),
            Instruction::Copy(i) => write!(f, "COPY {i}"),
            Instruction::Jump(o) => write!(f, "JUMP {o:+}"),
            Instruction::PopJumpIfFalse(o) => write!(f, "POP_JUMP_IF_FALSE {o:+}"),
            Instruction::JumpIfFalse(o) => write!(f, "JUMP_IF_FALSE {o:+}"),
            Instruction::BinOp(op) => write!(f, "BIN_OP {op}"),
            Instruction::CmpOp(op) => write!(f, "CMP_OP {op}"),
            Instruction::AddInt => f.write_str("ADD_INT"),
            Instruction::SubInt => f.write_str("SUB_INT"),
            Instruction::MulInt => f.write_str("MUL_INT"),
            Instruction::DivInt => f.write_str("DIV_INT"),
            Instruction::ModInt => f.write_str("MOD_INT"),
            Instruction::AddFloat => f.write_str("ADD_FLOAT"),
            Instruction::SubFloat => f.write_str("SUB_FLOAT"),
            Instruction::MulFloat => f.write_str("MUL_FLOAT"),
            Instruction::DivFloat => f.write_str("DIV_FLOAT"),
            Instruction::CmpIntOp(op) => write!(f, "CMP_INT_OP {op}"),
            Instruction::CmpFloatOp(op) => write!(f, "CMP_FLOAT_OP {op}"),
            Instruction::UnaryOp(op) => write!(f, "UNARY_OP {op}"),
            Instruction::AllocArray(n) => write!(f, "ALLOC_ARRAY {n}"),
            Instruction::LoadArrayElement => f.write_str("LOAD_ARRAY_ELEMENT"),
            Instruction::LoadMapElement => f.write_str("LOAD_MAP_ELEMENT"),
            Instruction::StoreArrayElement => f.write_str("STORE_ARRAY_ELEMENT"),
            Instruction::StoreMapElement => f.write_str("STORE_MAP_ELEMENT"),
            Instruction::AllocInstance {
                class_obj,
                ntypeargs,
            } => {
                write!(f, "ALLOC_INSTANCE {class_obj} ntypeargs={ntypeargs}")
            }
            Instruction::AllocVariant(i) => write!(f, "ALLOC_VARIANT {i}"),
            Instruction::SysOp(callee) => write!(f, "SYS_OP {callee}"),
            Instruction::Spawn => write!(f, "SPAWN"),
            Instruction::Await => f.write_str("AWAIT"),
            Instruction::Call { callee, ntypeargs } => {
                write!(f, "CALL {callee} ntypeargs={ntypeargs}")
            }
            Instruction::CallIndirect => f.write_str("CALL_INDIRECT"),
            Instruction::Throw => f.write_str("THROW"),

            Instruction::Return => f.write_str("RETURN"),
            Instruction::AllocMap(n) => write!(f, "ALLOC_MAP {n}"),
            Instruction::Watch(i) => write!(f, "WATCH {i}"),
            Instruction::Unwatch(i) => write!(f, "UNWATCH {i}"),
            Instruction::NotifyBlock(block_index) => {
                write!(f, "NOTIFY_BLOCK {block_index}")
            }
            Instruction::Notify(i) => write!(f, "NOTIFY {i}"),
            Instruction::VizEnter(i) => write!(f, "VIZ_ENTER {i}"),
            Instruction::VizExit(i) => write!(f, "VIZ_EXIT {i}"),
            Instruction::JumpTable(table_idx) => {
                write!(f, "JUMP_TABLE {table_idx}")
            }
            Instruction::Discriminant => f.write_str("DISCRIMINANT"),
            Instruction::TypeTag => f.write_str("TYPE_TAG"),
            Instruction::IsType(i) => write!(f, "IS_TYPE {i}"),
            Instruction::LoadType(i) => write!(f, "LOAD_TYPE {i}"),
            Instruction::DenseTag(i) => write!(f, "DENSE_TAG {i}"),
            Instruction::ThrowIfPanic => f.write_str("THROW_IF_PANIC"),
            Instruction::Unreachable => f.write_str("UNREACHABLE"),
            Instruction::MakeClosure {
                obj_idx,
                capture_count,
                ntypeargs,
            } => {
                if *ntypeargs > 0 {
                    write!(
                        f,
                        "MAKE_CLOSURE {} captures={} ntypeargs={}",
                        obj_idx.raw(),
                        capture_count,
                        ntypeargs
                    )
                } else {
                    write!(f, "MAKE_CLOSURE {} {}", obj_idx.raw(), capture_count)
                }
            }
            Instruction::MakeBoundMethod(global_idx) => {
                write!(f, "MAKE_BOUND_METHOD {global_idx}")
            }
            Instruction::MakeCell => f.write_str("MAKE_CELL"),
            Instruction::LoadDeref(slot) => write!(f, "LOAD_DEREF {slot}"),
            Instruction::StoreDeref(slot) => write!(f, "STORE_DEREF {slot}"),
            Instruction::LoadCapture(idx) => write!(f, "LOAD_CAPTURE {idx}"),
            Instruction::StoreCapture(idx) => write!(f, "STORE_CAPTURE {idx}"),
            Instruction::CaptureRef(idx) => write!(f, "CAPTURE_REF {idx}"),
            Instruction::SendEvent => f.write_str("SEND_EVENT"),
        }
    }
}

/// Resolved operand name for debug/display purposes.
///
/// Populated by the compiler at emit time so that debug display doesn't
/// need to resolve names from the `ObjectPool` or runtime stack.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OperandMeta {
    /// `LoadVar`, `StoreVar`, `Watch`, `Unwatch`, `Notify` — variable name.
    Var(String),
    /// `LoadField`, `StoreField` — field name.
    Field(String),
    /// `Call`, `SysOp` — function name.
    Callable(String),
    /// `LoadGlobal`, `StoreGlobal` — display value.
    Global(String),
    /// `AllocInstance`, `AllocVariant` — class/enum name.
    Object(String),
    /// `LoadConst` — display value.
    Const(String),
}

impl OperandMeta {
    /// Get the inner string regardless of variant.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Var(s)
            | Self::Field(s)
            | Self::Callable(s)
            | Self::Global(s)
            | Self::Object(s)
            | Self::Const(s) => s,
        }
    }
}

/// Per-instruction debug metadata, populated by the compiler.
///
/// Parallel to `Bytecode::instructions`. Contains resolved operand names for
/// debug display.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InstructionMeta {
    /// Resolved operand name (if applicable to the instruction type).
    pub operand: Option<OperandMeta>,
}

/// Run-length encoded source mapping entry.
///
/// Each entry applies from `pc` (inclusive) until the next entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineTableEntry {
    /// Bytecode program counter where this entry begins.
    pub pc: usize,
    /// Source span for this bytecode range.
    pub span: Span,
    /// 1-indexed source line for quick stack traces/disassembly.
    pub line: usize,
    /// True when this entry is a debugger sequence point.
    pub sequence_point: bool,
    /// Distinguishes multiple stops on the same line.
    pub discriminator: u32,
}

/// Debug metadata for a named local variable and its lexical scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugLocalScope {
    /// Stack slot used by this local.
    pub slot: usize,
    /// User-facing variable name.
    pub name: String,
    /// Source span where this variable is in scope.
    pub scope_span: Span,
}

/// Exception table entry mapping a PC range to a handler.
///
/// Any instruction at PC in `[start_pc, end_pc)` that raises an error
/// transfers control to `handler_pc`, with the exception value stored
/// in the frame-local slot `error_slot`.
///
/// Entries are sorted by `start_pc`. For nested catch blocks the innermost
/// (narrowest range) entry appears first.
///
/// All exceptions (user-thrown values and VM panics) are routed to the
/// handler. The handler bytecode is responsible for filtering: a
/// `ThrowIfPanic` instruction before wildcard arms rethrows panics the
/// programmer didn't explicitly name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExceptionTableEntry {
    /// First protected instruction (inclusive).
    pub start_pc: usize,
    /// End of protected range (exclusive).
    pub end_pc: usize,
    /// Instruction pointer of the handler block.
    pub handler_pc: usize,
    /// Frame-local slot index for the caught error value.
    pub error_slot: usize,
    /// Frame-local slot for the stack trace value.
    /// `usize::MAX` means no stack trace binding (catch (e) without second param).
    pub stack_trace_slot: usize,
}

impl ExceptionTableEntry {
    pub const NO_STACK_TRACE: usize = usize::MAX;

    pub fn has_stack_trace_slot(&self) -> bool {
        self.stack_trace_slot != Self::NO_STACK_TRACE
    }
}

/// Compact jump table: maps discriminant values to i32 byte offsets
/// (relative to the end of the `JumpTable` instruction in the compact stream).
/// Parallel to `Bytecode::jump_tables` but with translated offsets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactJumpTable {
    /// Minimum discriminant value (maps to index 0), same as `JumpTableData::min`.
    pub min: i64,
    /// Byte offsets (relative to instruction end) for each value from min to min+len-1.
    /// None means "hole" — should use the default offset encoded in the instruction.
    pub offsets: Vec<Option<i32>>,
}

impl CompactJumpTable {
    /// Lookup the byte offset for a discriminant value.
    /// Returns `None` if value is out of range or is a hole (use default).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn lookup(&self, value: i64) -> Option<i32> {
        if value < self.min {
            return None;
        }
        let index = (value - self.min) as usize;
        self.offsets.get(index).copied().flatten()
    }
}

/// Compact bytecode encoding.
///
/// A re-encoding of `Vec<Instruction>` as `Vec<u8>` with 1-byte opcodes and
/// fixed u32 operands. Produced by `Bytecode::lower_to_compact()` at engine
/// load time. The line table and exception table are translated to byte-offset PCs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactCode {
    /// The encoded instruction stream.
    pub code: Vec<u8>,
    /// Line table with PCs translated to byte offsets.
    pub line_table: Vec<LineTableEntry>,
    /// Exception table with PCs translated to byte offsets.
    pub exception_table: Vec<ExceptionTableEntry>,
    /// Jump tables with offsets translated to byte offsets.
    /// Parallel to `Bytecode::jump_tables`.
    pub jump_tables: Vec<CompactJumpTable>,
}

impl CompactCode {
    /// Get the source mapping entry for a byte-offset PC.
    pub fn line_entry_for_pc(&self, pc: usize) -> Option<&LineTableEntry> {
        if self.line_table.is_empty() {
            return None;
        }
        let idx = self.line_table.partition_point(|entry| entry.pc <= pc);
        (idx > 0).then(|| &self.line_table[idx - 1])
    }

    /// Get the 1-indexed source line for a byte-offset PC.
    pub fn source_line_for_pc(&self, pc: usize) -> usize {
        self.line_entry_for_pc(pc).map_or(0, |entry| entry.line)
    }

    /// Iterate exception table entries whose byte-offset PC range covers `pc`.
    pub fn exception_handlers_for_pc(
        &self,
        pc: usize,
    ) -> impl Iterator<Item = &ExceptionTableEntry> {
        self.exception_table
            .iter()
            .filter(move |e| pc >= e.start_pc && pc < e.end_pc)
    }
}

/// Executable bytecode.
///
/// Contains the instructions to run and all the associated constants.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bytecode {
    /// Sequence of instructions.
    pub instructions: Vec<Instruction>,

    /// Constant pool (compile-time, serializable).
    /// Contains `ObjectIndex` for object references.
    pub constants: Vec<ConstValue>,

    /// Resolved constants (runtime, populated at load time).
    /// Contains `HeapPtr` for object references. Used by `LoadConst`.
    #[serde(skip)]
    pub resolved_constants: Vec<crate::Value>,

    /// Jump tables for switch dispatch (indexed by `JumpTable` instruction).
    pub jump_tables: Vec<JumpTableData>,

    /// Perfect hash tables for sparse `TypeTag` switch dispatch.
    /// Indexed by `DenseTag` instruction operand.
    pub match_hash_tables: Vec<MatchHashTable>,

    /// Line table mapping bytecode PCs to source spans.
    ///
    /// Entries are run-length encoded by PC ranges.
    pub line_table: Vec<LineTableEntry>,

    /// Per-instruction debug metadata (resolved operand names).
    ///
    /// Parallel to `instructions`. Populated by the compiler at emit time.
    pub meta: Vec<InstructionMeta>,

    /// Exception table mapping PC ranges to catch handlers.
    ///
    /// Sorted by `start_pc`. The VM searches this table when an error occurs
    /// to find a handler covering the faulting instruction.
    pub exception_table: Vec<ExceptionTableEntry>,

    /// Compact bytecode encoding. Populated at engine load time by
    /// `lower_to_compact()`. `None` until lowering runs.
    #[serde(skip)]
    pub compact: Option<CompactCode>,
}

impl Default for Bytecode {
    fn default() -> Self {
        Self::new()
    }
}

impl Bytecode {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            constants: Vec::new(),
            resolved_constants: Vec::new(),
            jump_tables: Vec::new(),
            match_hash_tables: Vec::new(),
            line_table: Vec::new(),
            meta: Vec::new(),
            exception_table: Vec::new(),
            compact: None,
        }
    }

    /// Get the source mapping entry that applies to the given bytecode PC.
    pub fn line_entry_for_pc(&self, pc: usize) -> Option<&LineTableEntry> {
        if self.line_table.is_empty() {
            return None;
        }
        let idx = self.line_table.partition_point(|entry| entry.pc <= pc);
        (idx > 0).then(|| &self.line_table[idx - 1])
    }

    /// Get the 1-indexed source line for a bytecode PC.
    pub fn source_line_for_pc(&self, pc: usize) -> usize {
        self.line_entry_for_pc(pc).map_or(0, |entry| entry.line)
    }

    /// Iterate all exception table entries whose PC range covers `pc`.
    ///
    /// Returns entries in table order (innermost / first-declared first).
    /// The caller is responsible for picking the first entry whose filter
    /// matches the exception value.
    pub fn exception_handlers_for_pc(
        &self,
        pc: usize,
    ) -> impl Iterator<Item = &ExceptionTableEntry> {
        self.exception_table
            .iter()
            .filter(move |e| pc >= e.start_pc && pc < e.end_pc)
    }

    /// Resolve constants from `ConstValue` to Value using a resolver function.
    /// Called at load time to convert `ObjectIndex` to `HeapPtr`.
    pub fn resolve_constants<F>(&mut self, resolve: F)
    where
        F: Fn(crate::ObjectIndex) -> crate::HeapPtr,
    {
        self.resolved_constants = self
            .constants
            .iter()
            .map(|cv| match cv {
                // TyTemplate constants are NOT pre-resolved: `LoadType` reads
                // them directly from `constants` at execution time.
                ConstValue::Type(_) => crate::Value::Null,
                // ClassWithTypeArgs constants are NOT pre-resolved: `IsType`
                // reads them directly from `constants` at execution time and
                // resolves `class_obj` to a `HeapPtr` via `idx_to_ptr`.
                ConstValue::ClassWithTypeArgs { .. } => crate::Value::Null,
                other => other.to_value(&resolve),
            })
            .collect();
    }

    /// Encode `self.instructions` into a compact `Vec<u8>` byte stream.
    ///
    /// Two-pass algorithm:
    /// 1. Walk instructions, determine compact opcode per instruction, compute
    ///    byte offsets via `encoded_size()`. Build `index_to_offset` map.
    /// 2. Walk again, emit opcode + operand bytes. Translate jump offsets
    ///    from instruction-index-relative to byte-offset-relative.
    ///
    /// Also translates `line_table` and `exception_table` PCs to byte offsets.
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_lossless
    )]
    pub fn lower_to_compact(&self) -> CompactCode {
        let n = self.instructions.len();
        // index_to_offset[i] = byte offset of instruction i
        // index_to_offset[n] = total byte count (sentinel for end-of-code)
        let mut index_to_offset: Vec<usize> = Vec::with_capacity(n + 1);
        // Per-instruction compact opcode (determined in pass 1, used in pass 2)
        let mut opcodes: Vec<OpCode> = Vec::with_capacity(n);
        let mut byte_offset: usize = 0;

        // ── Pass 1: determine opcodes and build offset map ───────────
        for instr in &self.instructions {
            index_to_offset.push(byte_offset);
            let op = self.instruction_to_opcode(instr);
            byte_offset += op.encoded_size();
            opcodes.push(op);
        }
        index_to_offset.push(byte_offset); // sentinel

        // ── Pass 2: emit bytes ───────────────────────────────────────
        let mut code: Vec<u8> = Vec::with_capacity(byte_offset);

        for (i, instr) in self.instructions.iter().enumerate() {
            let op = opcodes[i];
            code.push(op as u8);

            match instr {
                // ── Unit ops: no operands ────────────────────────────
                Instruction::Return
                | Instruction::Await
                | Instruction::Throw
                | Instruction::LoadArrayElement
                | Instruction::LoadMapElement
                | Instruction::StoreArrayElement
                | Instruction::StoreMapElement
                | Instruction::CallIndirect
                | Instruction::Discriminant
                | Instruction::TypeTag
                | Instruction::ThrowIfPanic
                | Instruction::Unreachable
                | Instruction::MakeCell
                | Instruction::SendEvent
                | Instruction::Spawn => {}

                // ── Expanded sub-enum ops: no operands ──────────────
                Instruction::BinOp(_)
                | Instruction::CmpOp(_)
                | Instruction::UnaryOp(_)
                | Instruction::AddInt
                | Instruction::SubInt
                | Instruction::MulInt
                | Instruction::DivInt
                | Instruction::ModInt
                | Instruction::AddFloat
                | Instruction::SubFloat
                | Instruction::MulFloat
                | Instruction::DivFloat
                | Instruction::CmpIntOp(_)
                | Instruction::CmpFloatOp(_) => {}

                // ── Constant specialization ──────────────────────────
                Instruction::LoadConst(idx) => {
                    match op {
                        OpCode::LoadNull | OpCode::LoadTrue | OpCode::LoadFalse => {
                            // opcode already emitted, no operands
                        }
                        OpCode::LoadIntSmall => {
                            // Constant is Int(n) where n fits in i8
                            let ConstValue::Int(n) = self.constants[*idx] else {
                                unreachable!("pass 1 chose LoadIntSmall");
                            };
                            code.push(n as i8 as u8);
                        }
                        OpCode::LoadConst => {
                            // Generic LoadConst with u32 index
                            code.extend_from_slice(
                                &u32::try_from(*idx)
                                    .expect("constant index fits u32")
                                    .to_le_bytes(),
                            );
                        }
                        _ => unreachable!("pass 1 opcode mismatch for LoadConst"),
                    }
                }

                // ── Single usize operand → u32 ─────────────────────
                Instruction::LoadVar(v)
                | Instruction::StoreVar(v)
                | Instruction::LoadField(v)
                | Instruction::StoreField(v)
                | Instruction::InitField(v)
                | Instruction::Pop(v)
                | Instruction::Copy(v)
                | Instruction::AllocArray(v)
                | Instruction::AllocMap(v)
                | Instruction::Watch(v)
                | Instruction::Unwatch(v)
                | Instruction::Notify(v)
                | Instruction::NotifyBlock(v)
                | Instruction::VizEnter(v)
                | Instruction::VizExit(v)
                | Instruction::IsType(v)
                | Instruction::DenseTag(v)
                | Instruction::LoadType(v)
                | Instruction::LoadDeref(v)
                | Instruction::StoreDeref(v)
                | Instruction::LoadCapture(v)
                | Instruction::StoreCapture(v)
                | Instruction::CaptureRef(v) => {
                    code.extend_from_slice(
                        &u32::try_from(*v).expect("operand fits u32").to_le_bytes(),
                    );
                }

                // ── GlobalIndex operand → u32 ───────────────────────
                Instruction::LoadGlobal(g)
                | Instruction::StoreGlobal(g)
                | Instruction::SysOp(g)
                | Instruction::MakeBoundMethod(g) => {
                    code.extend_from_slice(
                        &u32::try_from(g.into_raw())
                            .expect("global index fits u32")
                            .to_le_bytes(),
                    );
                }

                // ── Call: u32 callee + u16 ntypeargs ─────────────────
                Instruction::Call { callee, ntypeargs } => {
                    code.extend_from_slice(
                        &u32::try_from(callee.into_raw())
                            .expect("global index fits u32")
                            .to_le_bytes(),
                    );
                    code.extend_from_slice(&ntypeargs.to_le_bytes());
                }

                // ── ObjectIndex operand → u32 ───────────────────────
                Instruction::AllocVariant(o) => {
                    code.extend_from_slice(
                        &u32::try_from(o.into_raw())
                            .expect("object index fits u32")
                            .to_le_bytes(),
                    );
                }

                // ── AllocInstance: u32 class_obj + u16 ntypeargs ────
                Instruction::AllocInstance {
                    class_obj,
                    ntypeargs,
                } => {
                    code.extend_from_slice(
                        &u32::try_from(class_obj.into_raw())
                            .expect("object index fits u32")
                            .to_le_bytes(),
                    );
                    code.extend_from_slice(&ntypeargs.to_le_bytes());
                }

                // ── Jump operands: translate to byte offsets ────────
                Instruction::Jump(offset)
                | Instruction::PopJumpIfFalse(offset)
                | Instruction::JumpIfFalse(offset) => {
                    // In the old VM, offset is relative to the instruction
                    // itself (IP was pre-incremented before step() ran, and
                    // the jump uses instruction_ptr.checked_add_signed(offset)
                    // where instruction_ptr is the pre-increment value).
                    // So target = i + offset.
                    let target_instr = (i as isize + offset) as usize;
                    let target_byte = index_to_offset[target_instr];
                    // In compact bytecode, offset is relative to the end of
                    // this instruction (after all operand bytes are read).
                    let instr_end = index_to_offset[i] + op.encoded_size();
                    let byte_delta = target_byte as i64 - instr_end as i64;
                    code.extend_from_slice(&(byte_delta as i32).to_le_bytes());
                }

                // ── JumpTable: u32 table_idx + i32 default_offset ───
                Instruction::JumpTable(table_idx) => {
                    code.extend_from_slice(
                        &u32::try_from(*table_idx)
                            .expect("table index fits u32")
                            .to_le_bytes(),
                    );
                    // default offset: stored in jump_tables[table_idx].default
                    let default = self.jump_tables[*table_idx].default;
                    let target_instr = (i as isize + default) as usize;
                    let target_byte = index_to_offset[target_instr];
                    let instr_end = index_to_offset[i] + op.encoded_size();
                    let byte_delta = target_byte as i64 - instr_end as i64;
                    code.extend_from_slice(&(byte_delta as i32).to_le_bytes());
                }

                // ── MakeClosure: u32 obj_idx + u16 capture_count + u16 ntypeargs ─
                Instruction::MakeClosure {
                    obj_idx,
                    capture_count,
                    ntypeargs,
                } => {
                    code.extend_from_slice(
                        &u32::try_from(obj_idx.into_raw())
                            .expect("object index fits u32")
                            .to_le_bytes(),
                    );
                    code.extend_from_slice(
                        &u16::try_from(*capture_count)
                            .expect("capture_count fits u16")
                            .to_le_bytes(),
                    );
                    code.extend_from_slice(
                        &u16::try_from(*ntypeargs)
                            .expect("ntypeargs fits u16")
                            .to_le_bytes(),
                    );
                }
            }
        }

        debug_assert_eq!(code.len(), byte_offset, "pass 2 size mismatch");

        // ── Translate tables ─────────────────────────────────────────
        let line_table = self
            .line_table
            .iter()
            .map(|entry| LineTableEntry {
                pc: index_to_offset[entry.pc],
                span: entry.span,
                line: entry.line,
                sequence_point: entry.sequence_point,
                discriminator: entry.discriminator,
            })
            .collect();

        let exception_table = self
            .exception_table
            .iter()
            .map(|entry| ExceptionTableEntry {
                start_pc: index_to_offset[entry.start_pc],
                end_pc: index_to_offset[entry.end_pc],
                handler_pc: index_to_offset[entry.handler_pc],
                error_slot: entry.error_slot,
                stack_trace_slot: entry.stack_trace_slot,
            })
            .collect();

        // ── Translate jump tables ────────────────────────────────────────
        // For each JumpTableData instruction at index `i`, the JumpTable opcode
        // is at byte offset `index_to_offset[i]` and its encoded size is 9.
        // The instruction end (after reading both u32+i32 operands) is at
        // `index_to_offset[i] + 9`. The default offset in the compact code is
        // already computed as `target_byte - instr_end` in pass 2 above.
        // For per-entry offsets, we need: byte_target - instr_end.
        //
        // We iterate the instructions to find JumpTable instructions and their
        // original index, then translate each entry's isize offset.
        let jump_tables: Vec<CompactJumpTable> = self
            .jump_tables
            .iter()
            .enumerate()
            .map(|(table_idx, jtd)| {
                // Find the instruction index of the JumpTable using this table.
                // We need the byte offset of the instruction end to compute relative offsets.
                // Find the instruction that uses this table_idx.
                let instr_idx = self
                    .instructions
                    .iter()
                    .position(|instr| matches!(instr, Instruction::JumpTable(t) if *t == table_idx))
                    .expect("JumpTable instruction must exist for each jump_tables entry");
                let instr_end_byte = index_to_offset[instr_idx] + OpCode::JumpTable.encoded_size();

                let offsets: Vec<Option<i32>> = jtd
                    .offsets
                    .iter()
                    .map(|offset_opt| {
                        offset_opt.map(|isize_offset| {
                            // target instruction index = instr_idx + isize_offset
                            // (same formula as the old VM: target = i + offset)
                            let target_instr = (instr_idx as isize + isize_offset) as usize;
                            let target_byte = index_to_offset[target_instr];
                            let byte_offset = target_byte as i64 - instr_end_byte as i64;
                            byte_offset as i32
                        })
                    })
                    .collect();

                CompactJumpTable {
                    min: jtd.min,
                    offsets,
                }
            })
            .collect();

        CompactCode {
            code,
            line_table,
            exception_table,
            jump_tables,
        }
    }

    /// Determine the compact opcode for an instruction.
    ///
    /// `LoadConst` is specialized to `LoadNull`/`LoadTrue`/`LoadFalse`/`LoadIntSmall`
    /// when the constant value matches. `BinOp`/`CmpOp`/`UnaryOp` are expanded
    /// to individual opcodes.
    fn instruction_to_opcode(&self, instr: &Instruction) -> OpCode {
        match instr {
            Instruction::Return => OpCode::Return,
            Instruction::Await => OpCode::Await,
            Instruction::Throw => OpCode::Throw,
            Instruction::LoadArrayElement => OpCode::LoadArrayElement,
            Instruction::LoadMapElement => OpCode::LoadMapElement,
            Instruction::StoreArrayElement => OpCode::StoreArrayElement,
            Instruction::StoreMapElement => OpCode::StoreMapElement,
            Instruction::CallIndirect => OpCode::CallIndirect,
            Instruction::Discriminant => OpCode::Discriminant,
            Instruction::TypeTag => OpCode::TypeTag,
            Instruction::ThrowIfPanic => OpCode::ThrowIfPanic,
            Instruction::Unreachable => OpCode::Unreachable,
            Instruction::MakeCell => OpCode::MakeCell,
            Instruction::SendEvent => OpCode::SendEvent,

            // Expanded sub-enum variants
            Instruction::BinOp(op) => match op {
                BinOp::Add => OpCode::Add,
                BinOp::Sub => OpCode::Sub,
                BinOp::Mul => OpCode::Mul,
                BinOp::Div => OpCode::Div,
                BinOp::Mod => OpCode::Mod,
                BinOp::BitAnd => OpCode::BitAnd,
                BinOp::BitOr => OpCode::BitOr,
                BinOp::BitXor => OpCode::BitXor,
                BinOp::Shl => OpCode::Shl,
                BinOp::Shr => OpCode::Shr,
            },
            Instruction::CmpOp(op) => match op {
                CmpOp::Eq => OpCode::Eq,
                CmpOp::NotEq => OpCode::NotEq,
                CmpOp::Lt => OpCode::Lt,
                CmpOp::LtEq => OpCode::LtEq,
                CmpOp::Gt => OpCode::Gt,
                CmpOp::GtEq => OpCode::GtEq,
            },
            Instruction::UnaryOp(op) => match op {
                UnaryOp::Not => OpCode::Not,
                UnaryOp::Neg => OpCode::Neg,
            },

            // Constant specialization
            Instruction::LoadConst(idx) => match &self.constants[*idx] {
                ConstValue::Null => OpCode::LoadNull,
                ConstValue::Bool(true) => OpCode::LoadTrue,
                ConstValue::Bool(false) => OpCode::LoadFalse,
                ConstValue::Int(n) if i8::try_from(*n).is_ok() => OpCode::LoadIntSmall,
                _ => OpCode::LoadConst,
            },

            // Single-operand variants
            Instruction::LoadVar(_) => OpCode::LoadVar,
            Instruction::StoreVar(_) => OpCode::StoreVar,
            Instruction::LoadGlobal(_) => OpCode::LoadGlobal,
            Instruction::StoreGlobal(_) => OpCode::StoreGlobal,
            Instruction::LoadField(_) => OpCode::LoadField,
            Instruction::StoreField(_) => OpCode::StoreField,
            Instruction::InitField(_) => OpCode::InitField,
            Instruction::Pop(_) => OpCode::Pop,
            Instruction::Copy(_) => OpCode::Copy,
            Instruction::AllocArray(_) => OpCode::AllocArray,
            Instruction::AllocMap(_) => OpCode::AllocMap,
            Instruction::AllocInstance { .. } => OpCode::AllocInstance,
            Instruction::AllocVariant(_) => OpCode::AllocVariant,
            Instruction::SysOp(_) => OpCode::SysOp,
            Instruction::Spawn => OpCode::Spawn,
            Instruction::Watch(_) => OpCode::Watch,
            Instruction::Unwatch(_) => OpCode::Unwatch,
            Instruction::Notify(_) => OpCode::Notify,
            Instruction::Call { .. } => OpCode::Call,
            Instruction::NotifyBlock(_) => OpCode::NotifyBlock,
            Instruction::VizEnter(_) => OpCode::VizEnter,
            Instruction::VizExit(_) => OpCode::VizExit,
            Instruction::IsType(_) => OpCode::IsType,
            Instruction::DenseTag(_) => OpCode::DenseTag,
            Instruction::LoadType(_) => OpCode::LoadType,
            Instruction::MakeBoundMethod(_) => OpCode::MakeBoundMethod,
            Instruction::LoadDeref(_) => OpCode::LoadDeref,
            Instruction::StoreDeref(_) => OpCode::StoreDeref,
            Instruction::LoadCapture(_) => OpCode::LoadCapture,
            Instruction::StoreCapture(_) => OpCode::StoreCapture,
            Instruction::CaptureRef(_) => OpCode::CaptureRef,

            // Specialized arithmetic (dedicated opcodes, skip type dispatch)
            Instruction::AddInt => OpCode::AddInt,
            Instruction::SubInt => OpCode::SubInt,
            Instruction::MulInt => OpCode::MulInt,
            Instruction::DivInt => OpCode::DivInt,
            Instruction::ModInt => OpCode::ModInt,
            Instruction::AddFloat => OpCode::AddFloat,
            Instruction::SubFloat => OpCode::SubFloat,
            Instruction::MulFloat => OpCode::MulFloat,
            Instruction::DivFloat => OpCode::DivFloat,
            Instruction::CmpIntOp(op) => match op {
                CmpOp::Eq => OpCode::CmpIntEq,
                CmpOp::NotEq => OpCode::CmpIntNotEq,
                CmpOp::Lt => OpCode::CmpIntLt,
                CmpOp::LtEq => OpCode::CmpIntLtEq,
                CmpOp::Gt => OpCode::CmpIntGt,
                CmpOp::GtEq => OpCode::CmpIntGtEq,
            },
            Instruction::CmpFloatOp(op) => match op {
                CmpOp::Eq => OpCode::CmpFloatEq,
                CmpOp::NotEq => OpCode::CmpFloatNotEq,
                CmpOp::Lt => OpCode::CmpFloatLt,
                CmpOp::LtEq => OpCode::CmpFloatLtEq,
                CmpOp::Gt => OpCode::CmpFloatGt,
                CmpOp::GtEq => OpCode::CmpFloatGtEq,
            },

            // Jump variants
            Instruction::Jump(_) => OpCode::Jump,
            Instruction::PopJumpIfFalse(_) => OpCode::PopJumpIfFalse,
            Instruction::JumpIfFalse(_) => OpCode::JumpIfFalse,

            // Two-operand variants
            Instruction::JumpTable(_) => OpCode::JumpTable,
            Instruction::MakeClosure { .. } => OpCode::MakeClosure,
        }
    }
}

impl std::fmt::Display for Bytecode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for instruction in &self.instructions {
            writeln!(f, "{instruction}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod compact_tests {
    use super::*;
    use crate::types::ConstValue;

    /// Helper to build a minimal Bytecode with given instructions and constants.
    fn make_bytecode(instructions: Vec<Instruction>, constants: Vec<ConstValue>) -> Bytecode {
        let meta = vec![InstructionMeta { operand: None }; instructions.len()];
        Bytecode {
            instructions,
            constants,
            resolved_constants: Vec::new(),
            jump_tables: Vec::new(),
            match_hash_tables: Vec::new(),
            line_table: Vec::new(),
            meta,
            exception_table: Vec::new(),
            compact: None,
        }
    }

    #[test]
    fn encode_load_int_small_and_return() {
        let bc = make_bytecode(
            vec![Instruction::LoadConst(0), Instruction::Return],
            vec![ConstValue::Int(42)],
        );
        let compact = bc.lower_to_compact();
        // LoadIntSmall(42) = 2 bytes, Return = 1 byte
        assert_eq!(compact.code.len(), 3);
        assert_eq!(compact.code[0], OpCode::LoadIntSmall as u8);
        assert_eq!(compact.code[1], 42u8);
        assert_eq!(compact.code[2], OpCode::Return as u8);
    }

    #[test]
    fn encode_load_null_true_false() {
        let bc = make_bytecode(
            vec![
                Instruction::LoadConst(0), // Null
                Instruction::LoadConst(1), // Bool(true)
                Instruction::LoadConst(2), // Bool(false)
            ],
            vec![
                ConstValue::Null,
                ConstValue::Bool(true),
                ConstValue::Bool(false),
            ],
        );
        let compact = bc.lower_to_compact();
        assert_eq!(compact.code.len(), 3);
        assert_eq!(compact.code[0], OpCode::LoadNull as u8);
        assert_eq!(compact.code[1], OpCode::LoadTrue as u8);
        assert_eq!(compact.code[2], OpCode::LoadFalse as u8);
    }

    #[test]
    fn encode_large_const_not_specialized() {
        let bc = make_bytecode(
            vec![Instruction::LoadConst(0)],
            vec![ConstValue::Int(1000)], // > i8::MAX
        );
        let compact = bc.lower_to_compact();
        // LoadConst + u32 = 5 bytes
        assert_eq!(compact.code.len(), 5);
        assert_eq!(compact.code[0], OpCode::LoadConst as u8);
        let idx = u32::from_le_bytes([
            compact.code[1],
            compact.code[2],
            compact.code[3],
            compact.code[4],
        ]);
        assert_eq!(idx, 0);
    }

    #[test]
    fn encode_expanded_binop() {
        let bc = make_bytecode(vec![Instruction::BinOp(BinOp::Add)], vec![]);
        let compact = bc.lower_to_compact();
        assert_eq!(compact.code.len(), 1);
        assert_eq!(compact.code[0], OpCode::Add as u8);
    }

    #[test]
    fn encode_jump_forward() {
        // Jump(+2) from instruction 0 should skip instruction 1 and land on instruction 2.
        // Layout: [Jump(+2), Return, Return, Return]
        // Instruction 0 = Jump, target = instruction 0+2 = instruction 2
        let bc = make_bytecode(
            vec![
                Instruction::Jump(2), // i=0, byte offset 0..5, targets i=2
                Instruction::Return,  // i=1, byte offset 5
                Instruction::Return,  // i=2, byte offset 6 (target)
                Instruction::Return,  // i=3, byte offset 7
            ],
            vec![],
        );
        let compact = bc.lower_to_compact();
        // Jump = 5 bytes (0..5). Return bytes at 5, 6, 7.
        // Target byte offset = index_to_offset[2] = 6
        // Instruction end = 0 + 5 = 5
        // Encoded offset = 6 - 5 = 1
        assert_eq!(compact.code[0], OpCode::Jump as u8);
        let encoded_offset = i32::from_le_bytes([
            compact.code[1],
            compact.code[2],
            compact.code[3],
            compact.code[4],
        ]);
        assert_eq!(encoded_offset, 1); // skip 1 byte (the Return at i=1)
    }

    #[test]
    fn encode_jump_backward() {
        // Layout: [Return, Jump(-1)]
        // Instruction 1 = Jump, target = instruction 1+(-1) = instruction 0
        let bc = make_bytecode(
            vec![
                Instruction::Return,   // i=0, byte offset 0
                Instruction::Jump(-1), // i=1, byte offset 1..6
            ],
            vec![],
        );
        let compact = bc.lower_to_compact();
        // Return = 1 byte at offset 0. Jump = 5 bytes at offset 1..6.
        // Target byte offset = index_to_offset[0] = 0
        // Instruction end = 1 + 5 = 6
        // Encoded offset = 0 - 6 = -6
        let encoded_offset = i32::from_le_bytes([
            compact.code[2],
            compact.code[3],
            compact.code[4],
            compact.code[5],
        ]);
        assert_eq!(encoded_offset, -6);
    }

    #[test]
    fn line_table_translated() {
        let bc = Bytecode {
            instructions: vec![
                Instruction::LoadConst(0), // i=0: will be LoadIntSmall = 2 bytes
                Instruction::Return,       // i=1: 1 byte
            ],
            constants: vec![ConstValue::Int(1)],
            resolved_constants: Vec::new(),
            jump_tables: Vec::new(),
            match_hash_tables: Vec::new(),
            line_table: vec![
                LineTableEntry {
                    pc: 0,
                    span: Span::default(),
                    line: 1,
                    sequence_point: true,
                    discriminator: 0,
                },
                LineTableEntry {
                    pc: 1,
                    span: Span::default(),
                    line: 2,
                    sequence_point: true,
                    discriminator: 0,
                },
            ],
            meta: vec![InstructionMeta { operand: None }; 2],
            exception_table: Vec::new(),
            compact: None,
        };
        let compact = bc.lower_to_compact();
        assert_eq!(compact.line_table[0].pc, 0); // instruction 0 → byte 0
        assert_eq!(compact.line_table[1].pc, 2); // instruction 1 → byte 2 (after 2-byte LoadIntSmall)
    }

    #[test]
    fn exception_table_translated() {
        let bc = Bytecode {
            instructions: vec![
                Instruction::LoadConst(0), // i=0: LoadIntSmall = 2 bytes
                Instruction::Return,       // i=1: 1 byte
                Instruction::Return,       // i=2: 1 byte (handler)
            ],
            constants: vec![ConstValue::Int(0)],
            resolved_constants: Vec::new(),
            jump_tables: Vec::new(),
            match_hash_tables: Vec::new(),
            line_table: Vec::new(),
            meta: vec![InstructionMeta { operand: None }; 3],
            exception_table: vec![ExceptionTableEntry {
                start_pc: 0,
                end_pc: 2,
                handler_pc: 2,
                error_slot: 0,
                stack_trace_slot: ExceptionTableEntry::NO_STACK_TRACE,
            }],
            compact: None,
        };
        let compact = bc.lower_to_compact();
        let entry = &compact.exception_table[0];
        assert_eq!(entry.start_pc, 0); // instruction 0 → byte 0
        assert_eq!(entry.end_pc, 3); // instruction 2 → byte 3 (2-byte LoadIntSmall + 1-byte Return)
        assert_eq!(entry.handler_pc, 3); // instruction 2 → byte 3
    }
}
