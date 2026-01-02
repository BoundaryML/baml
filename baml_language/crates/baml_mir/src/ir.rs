//! MIR data structures.
//!
//! This module defines the core types for the Mid-level Intermediate Representation:
//! functions as control flow graphs, basic blocks, statements, terminators, and operands.

use std::fmt;

use baml_base::Name;
use baml_thir::Ty;
use text_size::TextRange;

// ============================================================================
// Function
// ============================================================================

/// A function represented as a control flow graph.
#[derive(Debug, Clone)]
pub struct MirFunction<'db> {
    /// Function name for debugging.
    pub name: String,
    /// Parameter count.
    pub arity: usize,
    /// All basic blocks in the function.
    pub blocks: Vec<BasicBlock<'db>>,
    /// Entry block index (always 0 by convention).
    pub entry: BlockId,
    /// Local variable declarations.
    pub locals: Vec<LocalDecl<'db>>,
    /// Source span for error reporting.
    pub span: Option<TextRange>,
}

impl<'db> MirFunction<'db> {
    /// Get a basic block by ID.
    pub fn block(&self, id: BlockId) -> &BasicBlock<'db> {
        &self.blocks[id.0]
    }

    /// Get a mutable reference to a basic block by ID.
    pub fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock<'db> {
        &mut self.blocks[id.0]
    }

    /// Get a local declaration by ID.
    pub fn local(&self, id: Local) -> &LocalDecl<'db> {
        &self.locals[id.0]
    }
}

// ============================================================================
// Identifiers
// ============================================================================

/// Unique identifier for a basic block within a function.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

/// Unique identifier for a local variable or temporary.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Local(pub usize);

impl fmt::Display for Local {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "_{}", self.0)
    }
}

// ============================================================================
// Local Declaration
// ============================================================================

/// Declaration of a local variable or temporary.
#[derive(Debug, Clone)]
pub struct LocalDecl<'db> {
    /// Variable name (None for compiler temporaries).
    pub name: Option<Name>,
    /// Type of this local.
    pub ty: Ty<'db>,
    /// Source span (for diagnostics).
    pub span: Option<TextRange>,
}

// ============================================================================
// Basic Block
// ============================================================================

/// A basic block: a sequence of statements ending with a terminator.
///
/// Basic blocks are the fundamental unit of control flow in MIR. Each block
/// executes its statements in order, then transfers control via its terminator.
#[derive(Debug, Clone)]
pub struct BasicBlock<'db> {
    /// Unique identifier.
    pub id: BlockId,
    /// Statements executed in order.
    pub statements: Vec<Statement<'db>>,
    /// How this block exits (required after construction).
    pub terminator: Option<Terminator<'db>>,
    /// Source span covering this block.
    pub span: Option<TextRange>,
}

impl BasicBlock<'_> {
    /// Create a new empty basic block.
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            statements: Vec::new(),
            terminator: None,
            span: None,
        }
    }

    /// Check if this block has been terminated.
    pub fn is_terminated(&self) -> bool {
        self.terminator.is_some()
    }
}

// ============================================================================
// Statement
// ============================================================================

/// A single MIR statement (does not transfer control).
#[derive(Debug, Clone)]
pub struct Statement<'db> {
    pub kind: StatementKind<'db>,
    pub span: Option<TextRange>,
}

/// The kind of a MIR statement.
#[derive(Debug, Clone)]
pub enum StatementKind<'db> {
    /// Assign a value to a place: `_1 = <rvalue>`
    Assign {
        destination: Place,
        value: Rvalue<'db>,
    },

    /// Drop a value (run destructor if any).
    Drop(Place),

    /// No-op (placeholder for removed statements).
    Nop,
}

// ============================================================================
// Terminator
// ============================================================================

/// How a basic block transfers control.
///
/// Every basic block must end with exactly one terminator. Terminators are
/// the only way control can flow between blocks.
#[derive(Debug, Clone)]
pub enum Terminator<'db> {
    /// Unconditional jump to another block.
    Goto { target: BlockId },

    /// Conditional branch based on a boolean.
    Branch {
        condition: Operand<'db>,
        then_block: BlockId,
        else_block: BlockId,
    },

    /// Multi-way branch based on integer discriminant.
    Switch {
        discriminant: Operand<'db>,
        /// Arms: (value, target block)
        arms: Vec<(i64, BlockId)>,
        /// Default target if no arm matches.
        otherwise: BlockId,
    },

    /// Return from function.
    ///
    /// The return value should already be stored in `_0` (the return place).
    Return,

    /// Call a function.
    Call {
        /// The function to call.
        callee: Operand<'db>,
        /// Arguments to pass.
        args: Vec<Operand<'db>>,
        /// Where to store the result.
        destination: Place,
        /// Block to jump to after call returns normally.
        target: BlockId,
        /// Block to jump to if call throws (for catch).
        unwind: Option<BlockId>,
    },

    /// Unreachable code (for exhaustive match).
    ///
    /// Indicates this block should never be reached. If execution reaches
    /// an Unreachable terminator, it's a compiler bug.
    Unreachable,

    /// Dispatch an async operation (LLM call) without blocking.
    ///
    /// This is a suspend point - control returns to the embedder.
    DispatchFuture {
        /// The LLM function to call.
        callee: Operand<'db>,
        /// Arguments to the function.
        args: Vec<Operand<'db>>,
        /// Where to store the future handle.
        future: Place,
        /// Block to resume at after dispatch.
        resume: BlockId,
    },

    /// Await a future - suspend until result is ready.
    ///
    /// This is a suspend point - control returns to the embedder.
    Await {
        /// The future to await.
        future: Place,
        /// Where to store the result.
        destination: Place,
        /// Block to continue at after result is ready.
        target: BlockId,
        /// Block to jump to if the future fails (for catch).
        unwind: Option<BlockId>,
    },
}

impl Terminator<'_> {
    /// Get all successor block IDs.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Goto { target } => vec![*target],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            Terminator::Switch {
                arms, otherwise, ..
            } => {
                let mut succs: Vec<BlockId> = arms.iter().map(|(_, b)| *b).collect();
                succs.push(*otherwise);
                succs
            }
            Terminator::Return => vec![],
            Terminator::Call { target, unwind, .. } => {
                let mut succs = vec![*target];
                if let Some(u) = unwind {
                    succs.push(*u);
                }
                succs
            }
            Terminator::Unreachable => vec![],
            Terminator::DispatchFuture { resume, .. } => vec![*resume],
            Terminator::Await { target, unwind, .. } => {
                let mut succs = vec![*target];
                if let Some(u) = unwind {
                    succs.push(*u);
                }
                succs
            }
        }
    }
}

// ============================================================================
// Place
// ============================================================================

/// The kind of indexing operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexKind {
    /// Array indexing: `arr[i]`
    Array,
    /// Map indexing: `map[key]`
    Map,
}

/// A place in memory (lvalue).
///
/// Places represent locations that can be read from or written to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    /// A local variable: `_1`
    Local(Local),

    /// Field access: `_1.field_idx`
    Field { base: Box<Place>, field: usize },

    /// Indexing: `_1[_2]`
    Index {
        base: Box<Place>,
        index: Local,
        kind: IndexKind,
    },
}

impl Place {
    /// Create a place for a local variable.
    pub fn local(local: Local) -> Self {
        Place::Local(local)
    }

    /// Create a field projection.
    pub fn field(base: Place, field: usize) -> Self {
        Place::Field {
            base: Box::new(base),
            field,
        }
    }

    /// Create an index projection.
    pub fn index(base: Place, index: Local, kind: IndexKind) -> Self {
        Place::Index {
            base: Box::new(base),
            index,
            kind,
        }
    }

    /// Get the base local of this place.
    pub fn base_local(&self) -> Local {
        match self {
            Place::Local(l) => *l,
            Place::Field { base, .. } | Place::Index { base, .. } => base.base_local(),
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::Local(l) => write!(f, "{l}"),
            Place::Field { base, field } => write!(f, "{base}.{field}"),
            Place::Index { base, index, .. } => write!(f, "{base}[{index}]"),
        }
    }
}

// ============================================================================
// Rvalue
// ============================================================================

/// A value computation (rvalue).
///
/// Rvalues are computations that produce values. They appear on the right-hand
/// side of assignments.
#[derive(Debug, Clone)]
pub enum Rvalue<'db> {
    /// Use an operand directly.
    Use(Operand<'db>),

    /// Binary operation: `_1 + _2`
    BinaryOp {
        op: BinOp,
        left: Operand<'db>,
        right: Operand<'db>,
    },

    /// Unary operation: `!_1`, `-_1`
    UnaryOp { op: UnaryOp, operand: Operand<'db> },

    /// Create an array: `[_1, _2, _3]`
    Array(Vec<Operand<'db>>),

    /// Create a map: `{ key1: value1, key2: value2, ... }`
    /// Each entry is a (key, value) pair.
    Map(Vec<(Operand<'db>, Operand<'db>)>),

    /// Create an aggregate (class instance, enum variant): `ClassName { _1, _2 }`
    Aggregate {
        kind: AggregateKind,
        fields: Vec<Operand<'db>>,
    },

    /// Read discriminant of enum/union: `discriminant(_1)`
    Discriminant(Place),

    /// Get length of array: `len(_1)`
    Len(Place),

    /// Type check for pattern matching: `is_type(_1, Type)`
    IsType { operand: Operand<'db>, ty: Ty<'db> },
}

/// The kind of aggregate being constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateKind {
    /// An array.
    Array,
    /// A class instance.
    Class(String),
    /// An enum variant.
    EnumVariant { enum_name: String, variant: String },
}

// ============================================================================
// Operand
// ============================================================================

/// An operand: either a place (read) or a constant.
#[derive(Debug, Clone)]
pub enum Operand<'db> {
    /// Copy value from place.
    Copy(Place),

    /// Move value from place (consume it).
    Move(Place),

    /// A constant value.
    Constant(Constant<'db>),
}

impl<'db> Operand<'db> {
    /// Create a copy operand from a local.
    pub fn copy_local(local: Local) -> Self {
        Operand::Copy(Place::Local(local))
    }

    /// Create a move operand from a local.
    pub fn move_local(local: Local) -> Self {
        Operand::Move(Place::Local(local))
    }

    /// Create a constant operand.
    pub fn constant(c: Constant<'db>) -> Self {
        Operand::Constant(c)
    }
}

// ============================================================================
// Constant
// ============================================================================

/// A constant value in MIR.
#[derive(Debug, Clone)]
pub enum Constant<'db> {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    /// A function reference.
    Function(Name),
    /// An enum variant value.
    EnumVariant {
        enum_name: Name,
        variant: Name,
    },
    /// Placeholder for type info when needed.
    #[allow(dead_code)]
    Ty(Ty<'db>),
}

// ============================================================================
// Operations
// ============================================================================

/// Binary operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,

    // Type checking
    Instanceof,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Instanceof => "instanceof",
        };
        write!(f, "{s}")
    }
}

/// Unary operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            UnaryOp::Not => "!",
            UnaryOp::Neg => "-",
        };
        write!(f, "{s}")
    }
}
