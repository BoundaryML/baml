//! Instruction set and bytecode representation.

use crate::{GlobalIndex, ObjectIndex, types::Value};

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
/// Instead store the state or complex structure in the [`crate::Vm`] struct and
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
    /// [`crate::Vm::stack`] array.
    LoadVar(usize),

    /// Stores a value in the frame's local variable slots.
    ///
    /// Format: `STORE_VAR i` where `i` is the relative index of the variable in
    /// [`crate::Vm::stack`] array.
    StoreVar(usize),

    /// Load a global variable from the [`crate::Vm::globals`] array.
    ///
    /// Format: `LOAD_GLOBAL i` where `i` is the index of the global variable
    /// in the [`crate::Vm::globals`] array.
    ///
    /// Note that functions are also globals and can be passed around and stored
    /// in local variables, so we need to load their name in the stack before we
    /// call the function.
    LoadGlobal(GlobalIndex),

    /// Store a value in a global variable.
    ///
    /// Format: `STORE_GLOBAL i` where `i` is the index of the global variable
    /// in the [`crate::Vm::globals`] array.
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

    /// Pop N values from the top of [`crate::Vm::stack`] (the evaluation stack).
    ///
    /// Format: `POP n` where `n` is the number of values to pop.
    Pop(usize),

    /// Copy the i-th value from the top of the stack to the top.
    ///
    /// Format: `COPY i` where `i` is the offset from the top of the stack.
    /// `COPY 0` copies the top element (duplicates it).
    /// `COPY 1` copies the second element from the top.
    Copy(usize),

    /// End a nested block and put the result value on top of the stack.
    ///
    /// Format: `POP_REPLACE n` where `n` is the number of locals in the block's
    /// scope.
    ///
    /// This is instruction is necessary to support "blocks as expressions".
    /// Example:
    ///
    /// ```ignore
    /// fn main() {
    ///     let a = {
    ///         let b = 1;
    ///         b
    ///     };
    /// }
    /// ```
    PopReplace(usize),

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
    /// [`crate::Vm::objects`] array.
    AllocInstance(ObjectIndex),

    /// Builds a variant of an enum and allocates it on the heap.
    ///
    /// Format: `ALLOC_VARIANT i` where `i` is the index of the enum in the
    /// [`crate::Vm::objects`] array.
    AllocVariant(ObjectIndex),

    /// Creates a pending future, pushes it on the stack and notifies embedder.
    ///
    /// Format: `DISPATCH_FUTURE n` where `n` is the number of arguments passed
    /// to the _callable_ future.
    ///
    /// [`Instruction::DispatchFuture`] behaves like a function call
    /// ([`Instruction::Call`]). That is due to the fact that as of right now
    /// the only "futures" we can really run are LLM calls, and the VM doesn't
    /// even run those, that's up to the embedder. So, just like a function
    /// call, the stack should contain the future followed by the arguments, and
    /// this instruction takes care of emmiting a notification to the embedder
    /// so that it can schedule the future.
    DispatchFuture(usize),

    /// Awaits the future on top of the stack.
    ///
    /// VM yields execution back to the embedder because it is blocked awaiting
    /// a future. But obviously, the VM will not "block", it just returns
    /// control flow to the embedder and doesn't care about anything else.
    Await,

    /// Creates a watched var and tracks its state.
    ///
    /// Format: `WATCH i` where `i` is the relative index of the variable in the
    /// [`crate::Vm::stack`] array.
    Watch(usize),

    /// Unregisters a watched variable when it goes out of scope.
    ///
    /// Format: `UNWATCH i` where `i` is the relative index of the variable in the
    /// [`crate::Vm::stack`] array.
    Unwatch(usize),

    /// Manually triggers notifications for a watched variable.
    Notify(usize),

    /// Call a function.
    ///
    /// Format: `CALL n` where `n` is the number of arguments passed to the
    /// function.
    ///
    /// Arguments are pushed onto the eval stack and the name of the function
    /// is right below them.
    Call(usize),

    /// Return from a function.
    ///
    /// No arguments needed, result is stored in the eval stack and the VM
    /// simply has to clean up the call stack and continue execution.
    Return,

    /// Pops a `Bool` value from the stack. If the value is `false`, raises
    /// an assertion error.
    ///
    /// Format: `ASSERT`
    Assert,

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
    /// Used for jump table dispatch on union types (instanceof patterns).
    /// Type tags are global constants:
    /// - Primitives: `int=0`, `string=1`, `bool=2`, `null=3`, `float=4`
    /// - Classes: assigned unique IDs starting at 100
    TypeTag,

    /// Halt execution with an unreachable code error.
    ///
    /// This instruction should never be executed at runtime. If it is,
    /// it indicates a bug in the compiler or type system (e.g., a non-exhaustive
    /// match expression that the compiler incorrectly marked as exhaustive).
    ///
    /// Throws [`super::RuntimeError::Unreachable`].
    Unreachable,
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
    InstanceOf,
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
            CmpOp::InstanceOf => "instanceof",
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
            Instruction::Pop(n) => write!(f, "POP {n}"),
            Instruction::Copy(i) => write!(f, "COPY {i}"),
            Instruction::PopReplace(n) => write!(f, "POP_REPLACE {n}"),
            Instruction::Jump(o) => write!(f, "JUMP {o:+}"),
            Instruction::PopJumpIfFalse(o) => write!(f, "POP_JUMP_IF_FALSE {o:+}"),
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
            Instruction::DispatchFuture(i) => write!(f, "DISPATCH_FUTURE {i}"),
            Instruction::Await => f.write_str("AWAIT"),
            Instruction::Call(n) => write!(f, "CALL {n}"),
            Instruction::Return => f.write_str("RETURN"),
            Instruction::Assert => f.write_str("ASSERT"),
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
            Instruction::Unreachable => f.write_str("UNREACHABLE"),
        }
    }
}

/// Executable bytecode.
///
/// Contains the instructions to run and all the associated constants.
#[derive(Clone, Debug)]
pub struct Bytecode {
    /// Sequence of instructions.
    pub instructions: Vec<Instruction>,

    /// Constant pool.
    pub constants: Vec<Value>,

    /// Jump tables for switch dispatch (indexed by `JumpTable` instruction).
    pub jump_tables: Vec<JumpTableData>,

    /// Source line mapping.
    ///
    /// Maps instruction indices to their source line numbers.
    /// Each element corresponds to an instruction at the same index.
    pub source_lines: Vec<usize>,

    pub scopes: Vec<usize>,
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
            jump_tables: Vec::new(),
            source_lines: Vec::new(),
            scopes: Vec::new(),
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
