//! MIR data structures.
//!
//! This module defines the core types for the Mid-level Intermediate Representation:
//! functions as control flow graphs, basic blocks, statements, terminators, and operands.

use std::fmt;

use baml_base::{Name, Span};
pub use baml_compiler2_ast::BuiltinKind;
use baml_type::{RealizedTy, RuntimeTy, TyTemplate, TyTemplateInterface};

// ============================================================================
// Optimization Level
// ============================================================================

/// Optimization level controlling both MIR lowering and bytecode emission.
///
/// - `Zero`: No inlining of user-named locals. Compiler temps are still optimized.
///   Produces bytecode that closely mirrors the source structure.
/// - `One` (default): Full emit optimization — inline single-use locals, copy
///   propagation, stack carry — but no MIR-level constant folding. Useful for
///   testing individual instructions (e.g. `unary_op -` for `-5`).
/// - `Two`: Everything in `One` plus MIR-level constant folding and future
///   advanced transforms (e.g. type-tag switch dispatch).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptLevel {
    Zero,
    #[default]
    One,
    Two,
}

// ============================================================================
// Function
// ============================================================================

/// A catch region recorded during MIR lowering.
///
/// Describes the try-body entry block and the handler block for a `catch`
/// expression. The emitter uses this to build the bytecode exception table.
#[derive(Debug, Clone)]
pub struct CatchRegion {
    /// First block of the try body.
    pub body_entry: BlockId,
    /// Handler block that receives the exception.
    pub handler: BlockId,
    /// All blocks making up the handler body (the arms). BEP-042 cause-chain: a
    /// throw whose PC lies in any of these blocks is "during handling of"
    /// `error_local`, so that error's `ErrorContext` becomes the new error's
    /// cause. Captured as the blocks created while lowering the arms (plus the
    /// handler block itself); empty means "never chains" (e.g. a defer pad).
    /// Layout can fragment these across non-contiguous PCs, so the emitter must
    /// take their union rather than a single `[handler, join)` span.
    pub handler_body: Vec<BlockId>,
    /// Frame-local slot for the caught error value.
    pub error_local: Local,
    /// Frame-local slot for the stack trace value, if the catch clause
    /// has a second binding: `catch (e, st) { ... }`
    pub stack_trace_local: Option<Local>,
}

/// The bytecode body of a MIR function — blocks, locals, and associated data.
///
/// This is the inner data for `MirFunctionKind::Bytecode`. All field accessors
/// live here, so callers destructure the `MirFunctionKind` first and then work
/// with `&MirFunctionBody` / `&mut MirFunctionBody` directly — no panics.
#[derive(Debug, Clone)]
pub struct MirFunctionBody {
    /// All basic blocks in the function.
    pub blocks: Vec<BasicBlock>,
    /// Entry block index (always 0 by convention).
    pub entry: BlockId,
    /// Local variable declarations.
    pub locals: Vec<LocalDecl>,
    /// Catch regions mapping try-body extents to handler blocks.
    /// Populated during catch lowering; used by the emitter to build exception tables.
    pub catch_regions: Vec<CatchRegion>,
    /// Visualization nodes for control flow visualization.
    pub viz_nodes: Vec<VizNode>,
}

impl MirFunctionBody {
    /// Get a basic block by ID.
    pub fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id.0]
    }

    /// Get a local declaration by ID.
    pub fn local(&self, id: Local) -> &LocalDecl {
        &self.locals[id.0]
    }

    /// Iterate `(handler_block, error_local)` pairs derived from catch regions.
    ///
    /// Yields one entry per handler.
    pub fn unwind_error_locals(&self) -> impl Iterator<Item = (BlockId, Local)> + '_ {
        self.catch_regions
            .iter()
            .map(|r| (r.handler, r.error_local))
    }
}

/// Whether a MIR function has a bytecode body or is a Rust-bound builtin.
#[derive(Debug, Clone)]
pub enum MirFunctionKind {
    /// Has a body that will be compiled to bytecode.
    Bytecode(MirFunctionBody),
    /// Rust-bound builtin — `SysOp` (Io) or `NativeUnresolved` (Vm).
    Builtin(BuiltinKind),
}

/// Runtime signature metadata stamped onto a compiled `Function` object — the
/// ONE shape both producers fill: top-level declarations (emit derives it from
/// the TIR/item-tree in `compute_function_metadata_from_item_tree`) and
/// lambdas (`lower_lambda` records it here on the `MirFunction`). Consumed by
/// runtime reflection (BEP-062 `reflect.signature` / `reflect.call_any`),
/// function-value type reconstruction, and display surfaces
/// (`baml run --list`, bytecode listings).
///
/// Every type here is the one the declaration actually has, not the one it
/// spells: an unwritten position takes TIR's inferred type, so a lambda's
/// reconstructed signature is as precise as an annotated declaration's. Both
/// producers reconstruct the same value type for the same callable.
#[derive(Debug, Clone)]
pub struct RuntimeSignature {
    /// Parameter names, in declaration order.
    pub param_names: Vec<String>,
    /// Parameter types, parallel to `param_names`, as templates over the
    /// callee frame's De Bruijn type-arg slots.
    pub param_types: Vec<baml_type::TyTemplate>,
    /// Whether each parameter has a default, parallel to `param_names`.
    pub param_has_default: Vec<bool>,
    /// The return type. A template over the callee frame's type-arg slots (see
    /// [`Self::param_types`]).
    pub return_type: baml_type::TyTemplate,
    /// The throws type, as a template over the callee frame's type-arg slots.
    /// `never` == cannot throw (the same spelling a function type uses), so a
    /// reconstructed value signature and a written type agree.
    pub throws_type: baml_type::TyTemplate,
    /// The declaration's joined `///` doc-comment lines, if any.
    pub docstring: Option<String>,
    /// The name the declaration was written with; `None` for lambdas
    /// (which have no source-level name).
    pub name: Option<String>,
    /// Display strings for the generic type parameters (`T extends Bound`).
    pub display_type_params: Vec<String>,
    /// Runtime-checkable interface bounds, parallel to the callee frame's
    /// De Bruijn generic parameter slots.  Kept separately from display text
    /// so `unreflect(...)` calls can validate opaque runtime types before the
    /// callee executes.
    pub generic_param_bounds: Vec<Vec<RuntimeInterfaceBound>>,
    /// Display strings for the parameter types, parallel to `param_names`.
    pub display_param_types: Vec<String>,
    /// Display string for the return type.
    pub display_return_type: String,
}

/// Loc-free, templated form of one declared generic interface bound.
///
/// MIR owns this transport shape so the compiler layers do not depend on VM
/// object types; emission converts it directly to `bex_vm_types::InterfaceBound`.
#[derive(Debug, Clone)]
pub struct RuntimeInterfaceBound {
    pub interface: baml_type::TypeName,
    pub args: Vec<baml_type::TyTemplate>,
    pub assoc: Vec<(baml_type::Name, baml_type::TyTemplate)>,
}

/// A function represented as a control flow graph.
#[derive(Debug, Clone)]
pub struct MirFunction {
    /// Parameter count.
    pub arity: usize,
    /// Source span for error reporting.
    pub span: Option<Span>,
    /// Fully-qualified identity (e.g., "`user.my_func`", "baml.sys.panic").
    pub item_ref: ItemRef,
    /// Whether this function has bytecode or is a builtin.
    pub kind: MirFunctionKind,
    /// Child lambda functions defined inside this function's body.
    ///
    /// Indexed by `lambda_idx` in `Rvalue::MakeClosure`.
    /// Empty until lambda lowering is implemented.
    pub lambdas: Vec<MirFunction>,
    /// Runtime signature metadata, populated by `lower_lambda` for lambda
    /// functions only. Top-level functions get theirs from TIR `func_data`
    /// during emit; `None` there (and on synthetic adapters, which fall back
    /// to no metadata).
    pub signature: Option<RuntimeSignature>,
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
pub struct LocalDecl {
    /// Variable name (None for compiler temporaries).
    pub name: Option<Name>,
    /// Type of this local.
    pub ty: RuntimeTy,
    /// Source span where this local is declared.
    pub span: Option<Span>,
    /// Source span where this local is in scope.
    ///
    /// This is debugger metadata used to resolve in-scope variables from
    /// source locations.
    pub scope_span: Option<Span>,
    /// Whether this local is captured by a nested closure.
    ///
    /// When `true`, the local's stack slot holds an `Object::Cell` rather than
    /// the value directly. Reads/writes go through `LoadDeref`/`StoreDeref`.
    pub is_captured: bool,
}

// ============================================================================
// Basic Block
// ============================================================================

/// A basic block: a sequence of statements ending with a terminator.
///
/// Basic blocks are the fundamental unit of control flow in MIR. Each block
/// executes its statements in order, then transfers control via its terminator.
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Unique identifier.
    pub id: BlockId,
    /// Statements executed in order.
    pub statements: Vec<Statement>,
    /// How this block exits (required after construction).
    pub terminator: Option<Terminator>,
    /// Source span covering this block.
    pub span: Option<Span>,
    /// Source span for the terminator.
    pub terminator_span: Option<Span>,
}

impl BasicBlock {
    /// Create a new empty basic block.
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            statements: Vec::new(),
            terminator: None,
            span: None,
            terminator_span: None,
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
pub struct Statement {
    pub kind: StatementKind,
    pub span: Option<Span>,
}

/// Log level for the `Log` intrinsic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Debug,
    Warn,
    Error,
}

/// Compiler intrinsic operations.
///
/// These are lowered from calls to `$compiler_intrinsic` functions during
/// MIR construction. They produce `StatementKind::Intrinsic` instead of
/// `Terminator::Call`, emitting inline side effects without splitting the
/// control-flow graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicOp {
    /// `log.info`, `log.debug`, `log.warn`, `log.error` — emit a `$baml_log` event.
    Log(LogLevel),
    /// Bind an exact runtime type value into this bytecode frame's type slot.
    BindType(usize),
}

/// The kind of a MIR statement.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum StatementKind {
    /// Assign a value to a place: `_1 = <rvalue>`
    Assign { destination: Place, value: Rvalue },

    /// Drop a value (run destructor if any).
    Drop(Place),

    /// Enter a visualization node.
    /// Emitted at the start of control flow structures (if, while, etc.).
    VizEnter(usize),

    /// Exit a visualization node.
    /// Emitted at the end of control flow structures.
    VizExit(usize),

    /// Replace a captured local's cell with a fresh one.
    /// Emitted at the top of for-loop iteration bodies so each iteration's
    /// closures capture a distinct cell.
    FreshCell(Local),

    /// Compiler intrinsic — a void side effect (log, send event).
    /// Lowered from calls to `$compiler_intrinsic` functions.
    Intrinsic { op: IntrinsicOp, args: Vec<Operand> },

    /// Write an interface field on a receiver whose concrete type is not known
    /// statically — the store counterpart of [`Rvalue::VirtualFieldAccess`], with
    /// the same operand meaning and the same resolution.
    ///
    /// A statement rather than an `Assign` to a `Place`, because the destination
    /// slot is only known once the receiver's impl is resolved at run time.
    VirtualFieldStore {
        iface: TyTemplateInterface,
        receiver: Operand,
        field_index: u32,
        field: Name,
        value: Operand,
    },

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
pub enum Terminator {
    /// Unconditional jump to another block.
    Goto { target: BlockId },

    /// Conditional branch based on a boolean.
    Branch {
        condition: Operand,
        then_block: BlockId,
        else_block: BlockId,
    },

    /// Test one value and bind that same value to `destination` on success.
    NarrowBind {
        source: Operand,
        ty_template: TyTemplate,
        destination: Local,
        then_block: BlockId,
        else_block: BlockId,
    },

    /// Multi-way branch based on integer discriminant.
    Switch {
        discriminant: Operand,
        /// Arms: (value, target block)
        arms: Vec<(i64, BlockId)>,
        /// Default target if no arm matches.
        otherwise: BlockId,
        /// Whether this switch is exhaustive (all possible values covered).
        /// When true, the last arm's comparison can be skipped since if all
        /// other arms failed, the discriminant must match the last one.
        exhaustive: bool,
        /// Symbolic names for arm values (debug metadata only).
        /// Maps integer discriminant values to human-readable names like
        /// `"DispatchState.Alpha"` or `"int"`.
        arm_names: Vec<(i64, String)>,
    },

    /// Return from function.
    ///
    /// The return value should already be stored in `_0` (the return place).
    Return,

    /// Call a function.
    Call {
        /// The function to call.
        callee: Operand,
        /// Arguments to pass.
        ///
        /// The first `ntypeargs` operands are type-argument values (`Object::Type`)
        /// followed by the `nargs` regular value arguments.  The `ntypeargs`
        /// count tells the emitter how many leading slots to account for in
        /// the `Instruction::Call { ntypeargs }` bytecode instruction.
        args: Vec<Operand>,
        /// Number of leading `args` entries that carry type arguments.
        ///
        /// Zero for non-generic calls (the common case).  Non-zero for
        /// calls to generic functions where at least one type argument is
        /// threaded at the call site (explicit `<T>` or type-arg forwarding).
        ntypeargs: usize,
        /// At least one explicit type argument was supplied through
        /// `unreflect(...)`. The emitter encodes this on the call instruction so
        /// the VM performs M-5/M-6 checks only for marker-instantiated calls.
        runtime_type_check: bool,
        /// Hidden `boundary.LocalId` operand from call-site `$id = ...`.
        ///
        /// This is not part of ordinary call arity. Emitters push it above the
        /// normal call payload and use an ID-aware bytecode call form.
        runtime_id: Option<Operand>,
        /// Where to store the result.
        destination: Place,
        /// Block to jump to after call returns normally.
        target: BlockId,
        /// Block to jump to if call throws (for catch).
        unwind: Option<BlockId>,
    },

    /// Open-world virtual interface-method dispatch.
    ///
    /// Used when the receiver's concrete type is not statically known — a
    /// bounded type-var `T extends I`, an interface-existential `I`, a union,
    /// or `Self` inside an interface default body. The implementation is
    /// resolved **at runtime** from the receiver's concrete `Self` type against
    /// `iface` (coherence makes `(Self, iface)` pick at most one impl), then
    /// invoked exactly like a direct [`Terminator::Call`] — no value is
    /// materialized. This is the open-world replacement for the old
    /// compile-time type-tag switch.
    VirtualCall {
        /// The interface to resolve against, as a template the emitter pushes
        /// with `LoadType`. Non-generic today (`baml.ops.Equals`/`Compare`); a
        /// parameterized interface bakes its arguments into the template.
        iface: TyTemplateInterface,
        /// The interface method to dispatch (e.g. `"eq"`, `"lt"`, `"neq"`).
        method: String,
        /// `args[..ntypeargs]` are the method-level type-argument values
        /// (`Object::Type`, for a generic interface method like
        /// `Iterator.map<R, E2>`); `args[ntypeargs..]` are the value args,
        /// **receiver first**. The receiver's runtime concrete type is the `Self`
        /// the method resolves on; the type args are appended to the resolved
        /// frame.
        args: Vec<Operand>,
        /// Number of leading `args` entries that are method-level type arguments.
        /// Zero for a non-generic method.
        ntypeargs: usize,
        /// Whether this call carries an `unreflect(...)` type argument and must
        /// execute the runtime generic gate before entering the resolved method.
        runtime_type_check: bool,
        /// Hidden `boundary.LocalId` operand from call-site `$id = ...`.
        runtime_id: Option<Operand>,
        /// Where to store the result.
        destination: Place,
        /// Block to jump to after the call returns normally.
        target: BlockId,
        /// Block to jump to if the call throws (for catch).
        unwind: Option<BlockId>,
    },

    /// Unreachable code (for exhaustive match).
    ///
    /// Indicates this block should never be reached. If execution reaches
    /// an Unreachable terminator, it's a compiler bug.
    Unreachable,

    /// BEP-034 phase D′: invoke a sys-op and bind its return value
    /// directly into `destination`. Replaces the old `ScheduleFuture` +
    /// `Await` pair that allocated a `Future` heap object just to
    /// consume it on the next instruction.
    ///
    /// Suspend point — control returns to the embedder.
    SysOp {
        /// The sys-op global to invoke.
        callee: Operand,
        /// Arguments to the sys-op.
        args: Vec<Operand>,
        /// Hidden `boundary.LocalId` operand from call-site `$id = ...`.
        runtime_id: Option<Operand>,
        /// Where to store the sys-op's return value.
        destination: Place,
        /// Block to resume at after the sys-op returns.
        target: BlockId,
        /// Block to jump to if the sys-op throws (catch context).
        unwind: Option<BlockId>,
    },

    /// BEP-034 `spawn name? { body }` — schedules a fresh BAML thread to
    /// run `closure`'s body and yields a `Future<T, E>` handle.
    ///
    /// `closure` carries the body packaged via `MakeClosure` (a 0-arg
    /// lambda that captures the surrounding bindings); `name` is an
    /// optional human-readable label.
    Spawn {
        /// Closure object representing the spawn body.
        closure: Operand,
        /// Optional name expression (string or null).
        name: Operand,
        /// Optional `baml.spawn.options(...)` config value from a `with`
        /// clause (BEP-034 spawn options). `None` when there is no `with`
        /// clause; the engine reads the config's `cancel` (and later
        /// `group`/`detach`) to derive the spawn's effective cancel token.
        /// Boxed to keep `Terminator`'s footprint down (clippy
        /// `large_enum_variant`): `Spawn` is rare relative to `Call`/`Goto`.
        config: Option<Box<Operand>>,
        /// The `T`/`E` of the `Future<T, E>` this spawn yields. Boxed for the
        /// same footprint reason as `config`.
        future_ty: Box<SpawnFutureTy>,
        /// Where to store the resulting Future handle.
        future: Place,
        /// Block to resume after the spawn schedules.
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

    /// BEP-034 `baml.future.__await_any(futures)` — suspend until the FIRST
    /// of an array of futures settles, then bind the `int` index (in input
    /// order) of the first-settled future.
    ///
    /// Like `Await`, this is a suspend point. The `race`/`any` combinators
    /// are pure BAML built on top of it. `__await_any` is declared `throws
    /// never` (it only reports *which* future settled, never re-throws), so
    /// `unwind` is normally `None`; it is kept for shape-parity with `Await`.
    AwaitAny {
        /// The array of futures to wait on (a read operand).
        futures: Operand,
        /// Where to store the winning index (`int`).
        destination: Place,
        /// Block to continue at after the first future settles.
        target: BlockId,
        /// Catch context (unused — `__await_any` throws never).
        unwind: Option<BlockId>,
    },

    /// Throw an error value, unwinding to the nearest catch handler.
    ///
    /// If no catch handler is active, the error propagates to the caller.
    /// The `value` operand holds the error object to be thrown.
    Throw {
        /// The error value to throw.
        value: Operand,
    },

    /// Re-throw a caught error value, preserving its original trace origin.
    Rethrow {
        /// The caught error value to rethrow.
        value: Operand,
    },

    /// If the value is a panic instance (`baml.panics.*`), throw it.
    /// Otherwise continue to `otherwise` block.
    ///
    /// Used before wildcard catch arms to prevent them from swallowing
    /// panics the programmer didn't explicitly name.
    ThrowIfPanic { value: Operand, otherwise: BlockId },

    /// Short-circuit `&&` / `||`.
    ///
    /// Evaluates `operand` and peeks at the result (without popping):
    /// - `&&` (`is_and = true`): if false, jump to `join` (value stays on stack);
    ///   if true, pop and fall through to `eval_rhs`.
    /// - `||` (`is_and = false`): if true, jump to `join` (value stays on stack);
    ///   if false, pop and fall through to `eval_rhs`.
    ///
    /// The `eval_rhs` block must assign to `destination` and then goto `join`.
    /// At `join`, `destination` is on TOS from whichever path executed.
    ShortCircuit {
        operand: Operand,
        is_and: bool,
        destination: Place,
        eval_rhs: BlockId,
        join: BlockId,
    },
}

/// The type arguments of the `Future<T, E>` a [`Terminator::Spawn`] yields.
///
/// Held as [`TyTemplate`]s rather than resolved types so a spawn inside a
/// generic function (`fn f<T>(x: T) { spawn { x } }`) resolves against the
/// frame's type arguments at runtime, exactly as an array literal's element
/// type does. The runtime stores the resolved pair on the heap `Future` so
/// reflection and `is`/`match` can see the future's generic parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnFutureTy {
    /// The `T` of `Future<T, E>` — the value the spawned body returns.
    pub returns: TyTemplate,
    /// The `E` of `Future<T, E>` — what the spawned body may throw. A body that
    /// statically cannot throw spells this `never`.
    pub throws: TyTemplate,
}

impl Terminator {
    /// Get all successor block IDs.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Goto { target } => vec![*target],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            Terminator::NarrowBind {
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
            Terminator::Unreachable => vec![],
            Terminator::Spawn { resume, .. } => vec![*resume],
            Terminator::Call { target, unwind, .. }
            | Terminator::VirtualCall { target, unwind, .. }
            | Terminator::SysOp { target, unwind, .. }
            | Terminator::Await { target, unwind, .. }
            | Terminator::AwaitAny { target, unwind, .. } => {
                let mut succs = vec![*target];
                if let Some(u) = unwind {
                    succs.push(*u);
                }
                succs
            }
            Terminator::Throw { .. } | Terminator::Rethrow { .. } => vec![],
            Terminator::ThrowIfPanic { otherwise, .. } => vec![*otherwise],
            Terminator::ShortCircuit { eval_rhs, join, .. } => vec![*eval_rhs, *join],
        }
    }
}

// ============================================================================
// Place
// ============================================================================

/// The kind of indexing operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexKind {
    /// Array indexing: `arr[i]` (array or `uint8array`)
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

    /// A captured variable in a closure body, by capture index.
    ///
    /// `Capture(idx)` refers to the `idx`-th capture in the enclosing
    /// `Object::Closure.captures` array.  Reads emit `LoadCapture(idx)` and
    /// writes emit `StoreCapture(idx)`.  Only valid inside a lambda body.
    Capture(usize),
}

impl Place {
    /// Create a place for a local variable.
    pub fn local(local: Local) -> Self {
        Place::Local(local)
    }

    /// Get the base local of this place, if it is rooted in a local.
    pub fn base_local(&self) -> Option<Local> {
        match self {
            Place::Local(l) => Some(*l),
            Place::Field { base, .. } | Place::Index { base, .. } => base.base_local(),
            Place::Capture(_) => None,
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::Local(l) => write!(f, "{l}"),
            Place::Field { base, field } => write!(f, "{base}.{field}"),
            Place::Index { base, index, .. } => write!(f, "{base}[{index}]"),
            Place::Capture(idx) => write!(f, "capture[{idx}]"),
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
pub enum Rvalue {
    /// Use an operand directly.
    Use(Operand),

    /// Binary operation: `_1 + _2`
    BinaryOp {
        op: BinOp,
        left: Operand,
        right: Operand,
    },

    /// Unary operation: `!_1`, `-_1`
    UnaryOp { op: UnaryOp, operand: Operand },

    /// Create an array: `[_1, _2, _3]`. The first field is the static element
    /// type (a [`TyTemplate`] so a generic `T[]` resolves against the frame's
    /// type args at runtime), carried so the heap array records its declared
    /// element type.
    Array(TyTemplate, Vec<Operand>),

    /// Create a byte array from a literal: `b"hello"`
    Uint8Array(Vec<u8>),

    /// Create a map: `{ key1: value1, key2: value2, ... }`. Each entry is a
    /// (key, value) pair. The first two fields are the static key and value
    /// types (as [`TyTemplate`]s), carried so the heap map records its declared
    /// key/value types.
    Map(TyTemplate, TyTemplate, Vec<(Operand, Operand)>),

    /// Create an aggregate (class instance, enum variant): `ClassName { _1, _2 }`
    Aggregate {
        kind: AggregateKind,
        fields: Vec<Operand>,
    },

    /// Read discriminant of enum/union: `discriminant(_1)`
    Discriminant(Place),

    /// Extract runtime type tag from any value: `type_tag(_1)`
    ///
    /// Used for jump table dispatch on union types (type patterns in match).
    /// Type tags are global constants:
    /// - Primitives: `int=0`, `string=1`, `bool=2`, `null=3`, `float=4`
    /// - Classes: assigned unique IDs starting at 100
    TypeTag(Place),

    /// Get length of array: `len(_1)`
    Len(Place),

    /// Type check for pattern matching: `is_type(_1, Type)`
    ///
    /// The type is stored as a `TyTemplate` so that generic class checks like
    /// `value is Foo<T>` (where `T` is a type parameter in scope) resolve
    /// correctly at runtime via `TypeArgRef` substitution.  A fully-realized
    /// template narrows to a `RealizedTy`, which the emitter handles on the
    /// same tag / class-identity fast path as before.
    ///
    /// The template is *complete* by type — `TyTemplate` has no match-any
    /// holes — so the test always denotes exactly one type per frame. The
    /// deliberately-coarse container test carries its own rvalue instead
    /// ([`Rvalue::IsTypeTag`], a proven-sufficient tag).
    IsType {
        operand: Operand,
        ty_template: TyTemplate,
    },

    /// Coarse runtime type-tag test: `is_type_tag(_1, LIST)`.
    ///
    /// Used when MIR lowering has *proven* the coarse tag equivalent to the
    /// element-precise structural test for this scrutinee (the container
    /// tag-sufficiency analysis) — the tag is the whole check, deliberately
    /// blind to generic arguments. Carrying the decision explicitly keeps
    /// `IsType`'s template a complete type: previously the same intent was
    /// smuggled as a container template with `Wildcard` elements for the
    /// emitter to sniff out. `tag` is a `baml_type::typetag` constant; the
    /// emitter lowers this to the same `IsType`-against-`Int` bytecode as the
    /// other coarse tag checks.
    IsTypeTag { operand: Operand, tag: i64 },

    /// Runtime-mint identity filter used by `is unreflect(t)` patterns.
    /// `type_value` evaluates to an `Object::Type`; the VM reconstructs the
    /// nominal mint of `operand` and compares the two identity tokens.
    RuntimeIsType {
        operand: Operand,
        type_value: Operand,
    },

    /// Allocate a closure object from a child lambda function.
    ///
    /// `lambda_idx` indexes into `MirFunction::lambdas` of the enclosing function.
    /// `captures` is the ordered list of captured values (each will become a Cell).
    /// `type_arg_templates` carries one `TyTemplate` per enclosing generic type
    /// parameter; the emitter pushes `LoadType` instructions for each before
    /// the cell captures so the VM's `MakeClosure { ntypeargs }` instruction
    /// can pop them into `Closure::captured_type_args`.
    MakeClosure {
        lambda_idx: usize,
        captures: Vec<Operand>,
        /// Templates for enclosing generic type params captured by this closure.
        /// Empty (the common case) when the enclosing function has no type params.
        type_arg_templates: Vec<TyTemplate>,
    },

    /// Create a bound method value from a method reference and its receiver.
    ///
    /// `item_ref` identifies the method (class + name).
    /// `receiver` is the instance the method is bound to.
    MakeBoundMethod {
        item_ref: ItemRef,
        receiver: Operand,
    },

    /// Create a bound method value for an *interface* method whose impl is
    /// unknown statically — the value analogue of [`Terminator::VirtualCall`]
    /// (`let f = x.eq` on an existential / bounded-type-var receiver). The VM
    /// resolves the receiver's concrete `Self` to its impl at bind time and
    /// produces a `BoundMethod` over the resolved method, carrying the impl's
    /// realized frame type args.
    MakeVirtualBoundMethod {
        /// The interface to resolve against, as a template the emitter pushes
        /// with `LoadType` (like [`Terminator::VirtualCall`]'s `iface`).
        iface: TyTemplateInterface,
        /// The interface method's name.
        method: String,
        /// The receiver whose runtime concrete type is the `Self` to resolve on.
        receiver: Operand,
        /// Method-level type-argument templates from the reference site (a
        /// generic interface method's own generics, when specialized there).
        /// Appended to the resolved impl frame by the VM — dropping them would
        /// lose the method's own generics.
        type_args: Vec<TyTemplate>,
    },

    /// Read an interface field from a receiver whose concrete type is not known
    /// statically — the field analogue of [`Terminator::VirtualCall`], and the
    /// structural twin of [`Rvalue::MakeVirtualBoundMethod`].
    ///
    /// A `Place::Field` cannot express this: its index is a slot in the receiver's
    /// own layout, and two classes implementing the same interface link the same
    /// interface field to different slots. `field_index` is instead the field's
    /// position in the *interface's* declared field list, which the VM maps to a
    /// slot through the resolved impl's `field_links`.
    VirtualFieldAccess {
        /// The interface resolved through, pushed by the emitter with `LoadType` —
        /// so an interface argument that is an enclosing generic (`Slot<T>`)
        /// arrives at the resolver realized against the caller's frame, which is
        /// what discriminates a class implementing one interface family at several
        /// instantiations with different links.
        iface: TyTemplateInterface,
        /// The receiver whose runtime concrete type is the `Self` to resolve on.
        receiver: Operand,
        /// Index into `iface`'s declared fields.
        field_index: u32,
        /// The field's name — for the pretty-printer and the emitter's
        /// `OperandMeta` only. Dispatch reads `field_index`.
        field: Name,
    },

    /// Create a generic-function value (`foo<T>`) whose type arguments depend on
    /// the enclosing frame's type params, so they cannot be a compile-time
    /// constant. The emitter pushes a `LoadType` for each template (resolved
    /// against `frame.type_args` at runtime) before the `MakeGenericFunction`
    /// instruction, which builds an `Object::GenericFunction`. The
    /// fully-concrete case uses the pooled, interned `Constant::GenericFunction`
    /// instead.
    MakeGenericFunction {
        item: ItemRef,
        /// One template per type argument; may contain `TypeArgRef(N)`.
        type_arg_templates: Vec<TyTemplate>,
    },

    /// Specialize a runtime callable *value* with explicit type arguments
    /// (`g<int>` where `g` is a local/captured function value, not a
    /// compile-time-resolvable function reference). The emitter pushes a
    /// `LoadType` for each template then a `MakeGenericFunctionFromValue`
    /// instruction, which wraps the evaluated `value` in a `Closure` carrying
    /// the types as `captured_type_args`. Used when `lower_generic_apply`'s base
    /// is not an `ItemRef`; the `ItemRef` cases use `Constant::GenericFunction`
    /// (concrete) or `MakeGenericFunction` (param-dependent) instead.
    MakeGenericFunctionFromValue {
        /// The callable value to specialize.
        value: Operand,
        /// One template per type argument; may contain `TypeArgRef(N)`.
        type_arg_templates: Vec<TyTemplate>,
    },

    /// Materialize a `Ty` from a `TyTemplate`.
    ///
    /// For a fully-realized template, the `Ty` is baked in at compile time.
    /// For templates containing `TypeArgRef(N)`, the VM substitutes
    /// `frame.type_args[N]` at execution time.
    ///
    /// Emitted by the `type.of<T>()` intrinsic.
    /// Lowers to `Instruction::LoadType(const_idx)` in bytecode.
    LoadType(TyTemplate),

    /// Reify the package lexically enclosing this call site. The package name
    /// is baked by lowering; dynamically compiled code substitutes its owning
    /// runtime package at execution.
    CurrentPackage(String),
}

/// The kind of aggregate being constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateKind {
    /// An array.
    Array,
    /// A class instance with optional type-arg templates.
    ///
    /// `type_arg_templates` is non-empty only for generic class instantiations:
    /// each element corresponds to one class-level type parameter in De Bruijn
    /// order (matching `enclosing_generic_params()`).  These templates are
    /// emitted as `LoadType` instructions before `AllocInstance` so the VM can
    /// store resolved `Ty` values in `Instance::class_type_args`.
    Class {
        name: String,
        type_arg_templates: Vec<baml_type::TyTemplate>,
    },
    /// An enum variant.
    EnumVariant { enum_name: String, variant: String },
}

// ============================================================================
// Operand
// ============================================================================

/// An operand: either a place (read) or a constant.
#[derive(Debug, Clone)]
pub enum Operand {
    /// Copy value from place.
    Copy(Place),

    /// Move value from place (consume it).
    Move(Place),

    /// A constant value.
    Constant(Constant),
}

impl Operand {
    /// Create a copy operand from a local.
    pub fn copy_local(local: Local) -> Self {
        Operand::Copy(Place::Local(local))
    }

    /// Create a constant operand.
    pub fn constant(c: Constant) -> Self {
        Operand::Constant(c)
    }
}

// ============================================================================
// Constant
// ============================================================================

/// A constant value in MIR.
#[derive(Debug, Clone)]
pub enum Constant {
    Int(i64),
    Bigint(num_bigint::BigInt),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    /// Internal sentinel used for omitted defaulted function parameters.
    ///
    /// User BAML code cannot construct this value. Callee-entry default
    /// prologues replace it before user body code observes the parameter.
    OmittedArg,
    /// A function reference with structured item identification.
    ///
    /// Carried from TIR resolution through lowering. Converted to a
    /// runtime string only in the emit phase, where it becomes a pooled
    /// function-value wrapper (see `emit_pooled_function_value`). Only for
    /// items that ARE functions; a non-function global item read (a client,
    /// a top-level `let`, ...) is [`Constant::GlobalItem`].
    Function(ItemRef),
    /// A non-function global item read (a `client<llm>` declaration, a
    /// top-level `let`, a template string, ...): the value the program's
    /// `$init` stored in the item's global slot. Emitted as a plain
    /// `LoadGlobal`, never wrapped — the slot holds an ordinary value
    /// (an instance, a closure, ...), not a `Function` object.
    GlobalItem(ItemRef),
    /// A generic function instantiated with concrete type arguments
    /// (`foo<int>` referenced as a value). Emitted as a pooled, interned
    /// `Object::GenericFunction` so identical instantiations share one object
    /// (pointer-stable identity) and calling it seeds `frame.type_args`.
    GenericFunction {
        /// The base generic function.
        item: ItemRef,
        /// The concrete type arguments — fully realized (no type parameters),
        /// exactly what the runtime `Object::GenericFunction` carries.
        type_args: Vec<RealizedTy>,
    },
    /// An enum variant value.
    EnumVariant {
        /// Structured reference to the enum type.
        enum_ref: ItemRef,
        /// The variant name within the enum.
        variant: Name,
    },
}

/// A structured reference to a named item (function, method, enum type).
///
/// Uses explicit fields for package, namespace, class, and name.
/// No string-path encoding or display-logic special-casing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemRef {
    /// A free function or top-level item: `baml.env.get`, `Foo`, `baml.sys.panic`
    Free {
        package: Name,
        namespace: Vec<Name>,
        name: Name,
    },
    /// A class method: `baml.Array.length`, `Baz.Greeting`
    Method {
        package: Name,
        namespace: Vec<Name>,
        class: Name,
        name: Name,
    },
    /// An enum type reference: `HttpMethod`, `Color`
    EnumType {
        package: Name,
        namespace: Vec<Name>,
        name: Name,
    },
}

impl fmt::Display for ItemRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Always include the package prefix (including "user").
        // All parts are joined with ".".
        match self {
            ItemRef::Free {
                package,
                namespace,
                name,
            } => {
                let mut parts: Vec<&str> = vec![package.as_str()];
                for ns in namespace {
                    parts.push(ns.as_str());
                }
                parts.push(name.as_str());
                write!(f, "{}", parts.join("."))
            }
            ItemRef::Method {
                package,
                namespace,
                class,
                name,
            } => {
                let mut parts: Vec<&str> = vec![package.as_str()];
                for ns in namespace {
                    parts.push(ns.as_str());
                }
                parts.push(class.as_str());
                parts.push(name.as_str());
                write!(f, "{}", parts.join("."))
            }
            ItemRef::EnumType {
                package,
                namespace,
                name,
            } => {
                let mut parts: Vec<&str> = vec![package.as_str()];
                for ns in namespace {
                    parts.push(ns.as_str());
                }
                parts.push(name.as_str());
                write!(f, "{}", parts.join("."))
            }
        }
    }
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
        };
        write!(f, "{s}")
    }
}

/// Unary operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    /// Truthiness coercion (B-1563): `bool(value)` - false for `false`,
    /// `null`, zero, and empty string/list/map/bytes; true otherwise.
    Truthy,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            UnaryOp::Not => "!",
            UnaryOp::Neg => "-",
            UnaryOp::Truthy => "truthy ",
        };
        write!(f, "{s}")
    }
}

// ============================================================================
// Visualization Nodes
// ============================================================================

/// Type of visualization node for control flow visualization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Visualization node metadata for control flow visualization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VizNode {
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
