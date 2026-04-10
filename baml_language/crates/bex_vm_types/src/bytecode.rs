//! Instruction set and bytecode representation.

use baml_base::Span;

use crate::{GlobalIndex, ObjectIndex, types::ConstValue};

// ============================================================================
// Jump Table Data Structure
// ============================================================================

/// Jump table data for O(1) switch dispatch.
///
/// Maps a contiguous range of integer values to jump offsets.
/// Values outside the range or "holes" jump to the default offset.
#[derive(Clone, Debug, PartialEq)]
pub struct JumpTableData {
    /// Minimum discriminant value (maps to index 0).
    pub min: i64,
    /// Jump offsets for each value from min to min+len-1.
    /// None means "hole" - should jump to default.
    pub offsets: Vec<Option<isize>>,
    /// Symbolic names for each table entry (display only).
    /// Parallel to `offsets`: `names[i]` is the name for value `min + i`.
    pub names: Vec<Option<String>>,
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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// Format: `ALLOC_INSTANCE i` where `i` is the index of the class in the
    /// `Vm::objects` array.
    AllocInstance(ObjectIndex),

    /// Builds a variant of an enum and allocates it on the heap.
    ///
    /// Format: `ALLOC_VARIANT i` where `i` is the index of the enum in the
    /// `Vm::objects` array.
    AllocVariant(ObjectIndex),

    /// Dispatch a statically-known global `sys_op` and create a pending future.
    ///
    /// Format: `DISPATCH_FUTURE g` where `g` is the global index of the
    /// `sys_op` function.
    ///
    /// Arguments are pushed onto the eval stack. The callee is read from
    /// `Vm::globals[g]`, and arity is read from function metadata.
    DispatchFuture(GlobalIndex),

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
    /// Format: `CALL g` where `g` is the global index of the callee function.
    ///
    /// Arguments are pushed onto the eval stack. The callee is read from
    /// `Vm::globals[g]`, and arity is read from function metadata.
    Call(GlobalIndex),

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
    /// Format: `JUMP_TABLE table_idx, default` where:
    /// - `table_idx` is the index into `Bytecode::jump_tables`
    /// - `default` is the offset to jump to for out-of-range or hole values
    JumpTable {
        /// Index into `Bytecode::jump_tables`.
        table_idx: usize,
        /// Offset to jump to for out-of-range or hole values.
        default: isize,
    },

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
    /// Pops `capture_count` values from the stack (left-to-right order, reversed
    /// after popping), pairs them with the function at `obj_idx`, and pushes the
    /// resulting `Object::Closure`.
    ///
    /// Stack: `[cap_0, cap_1, ..., cap_{n-1}]` -> `[closure]`
    MakeClosure(ObjectIndex, usize),

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
}

/// Block notification metadata stored in the Function struct.
/// The `function_name` field is populated at runtime from the Function containing this notification.

#[derive(Clone, Debug, PartialEq)]
pub struct BlockNotification {
    pub function_name: String, // Populated at runtime from Function::name
    pub block_name: String,
    pub level: usize,
    pub block_type: BlockNotificationType,
    pub is_enter: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockNotificationType {
    Statement,
    If,
    While,
    For,
    Function,
}

/// Visualization node metadata stored in the Function struct.
/// Used for control flow visualization (branches, loops, scopes).
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VizExecDelta {
    /// Entering a visualization node.
    Enter,
    /// Exiting a visualization node.
    Exit,
}

/// Visualization execution event emitted when entering/exiting a viz node.
#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
            Instruction::UnaryOp(op) => write!(f, "UNARY_OP {op}"),
            Instruction::AllocArray(n) => write!(f, "ALLOC_ARRAY {n}"),
            Instruction::LoadArrayElement => f.write_str("LOAD_ARRAY_ELEMENT"),
            Instruction::LoadMapElement => f.write_str("LOAD_MAP_ELEMENT"),
            Instruction::StoreArrayElement => f.write_str("STORE_ARRAY_ELEMENT"),
            Instruction::StoreMapElement => f.write_str("STORE_MAP_ELEMENT"),
            Instruction::AllocInstance(i) => write!(f, "ALLOC_INSTANCE {i}"),
            Instruction::AllocVariant(i) => write!(f, "ALLOC_VARIANT {i}"),
            Instruction::DispatchFuture(callee) => write!(f, "DISPATCH_FUTURE {callee}"),
            Instruction::Await => f.write_str("AWAIT"),
            Instruction::Call(callee) => write!(f, "CALL {callee}"),
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
            Instruction::JumpTable { table_idx, default } => {
                write!(f, "JUMP_TABLE {table_idx}, {default:+}")
            }
            Instruction::Discriminant => f.write_str("DISCRIMINANT"),
            Instruction::TypeTag => f.write_str("TYPE_TAG"),
            Instruction::IsType(i) => write!(f, "IS_TYPE {i}"),
            Instruction::DenseTag(i) => write!(f, "DENSE_TAG {i}"),
            Instruction::ThrowIfPanic => f.write_str("THROW_IF_PANIC"),
            Instruction::Unreachable => f.write_str("UNREACHABLE"),
            Instruction::MakeClosure(obj_idx, count) => {
                write!(f, "MAKE_CLOSURE {} {}", obj_idx.raw(), count)
            }
            Instruction::MakeCell => f.write_str("MAKE_CELL"),
            Instruction::LoadDeref(slot) => write!(f, "LOAD_DEREF {slot}"),
            Instruction::StoreDeref(slot) => write!(f, "STORE_DEREF {slot}"),
            Instruction::LoadCapture(idx) => write!(f, "LOAD_CAPTURE {idx}"),
            Instruction::StoreCapture(idx) => write!(f, "STORE_CAPTURE {idx}"),
            Instruction::CaptureRef(idx) => write!(f, "CAPTURE_REF {idx}"),
        }
    }
}

/// Resolved operand name for debug/display purposes.
///
/// Populated by the compiler at emit time so that debug display doesn't
/// need to resolve names from the `ObjectPool` or runtime stack.
#[derive(Clone, Debug)]
pub enum OperandMeta {
    /// `LoadVar`, `StoreVar`, `Watch`, `Unwatch`, `Notify` — variable name.
    Var(String),
    /// `LoadField`, `StoreField` — field name.
    Field(String),
    /// `Call`, `DispatchFuture` — function name.
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
#[derive(Clone, Debug, Default)]
pub struct InstructionMeta {
    /// Resolved operand name (if applicable to the instruction type).
    pub operand: Option<OperandMeta>,
}

/// Run-length encoded source mapping entry.
///
/// Each entry applies from `pc` (inclusive) until the next entry.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct ExceptionTableEntry {
    /// First protected instruction (inclusive).
    pub start_pc: usize,
    /// End of protected range (exclusive).
    pub end_pc: usize,
    /// Instruction pointer of the handler block.
    pub handler_pc: usize,
    /// Frame-local slot index for the caught error value.
    pub error_slot: usize,
}

/// Executable bytecode.
///
/// Contains the instructions to run and all the associated constants.
#[derive(Clone, Debug)]
pub struct Bytecode {
    /// Sequence of instructions.
    pub instructions: Vec<Instruction>,

    /// Constant pool (compile-time, serializable).
    /// Contains `ObjectIndex` for object references.
    pub constants: Vec<ConstValue>,

    /// Resolved constants (runtime, populated at load time).
    /// Contains `HeapPtr` for object references. Used by `LoadConst`.
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
            .map(|cv| cv.to_value(&resolve))
            .collect();
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
