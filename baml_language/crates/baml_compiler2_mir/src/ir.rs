//! MIR data structures.
//!
//! This module defines the core types for the Mid-level Intermediate Representation:
//! functions as control flow graphs, basic blocks, statements, terminators, and operands.

use std::fmt;

use baml_base::{Name, Span};
pub use baml_compiler2_ast::BuiltinKind;
use baml_type::DeclName;

/// MIR's types are headed by the compiler's own identity: a declaration IS
/// its [`DeclName`] (the declaring root plus its path), never a spelling. A
/// head is rendered as a name only where MIR is displayed or emitted as
/// display metadata, through the program's spelling table; emit anchors every
/// head to its declaration's object by identity, so nothing between the
/// checker and the unit re-derives a declaration from a name.
pub type RuntimeTy = baml_type::RuntimeTy<DeclName>;
/// See [`RuntimeTy`].
pub type RealizedTy = baml_type::RealizedTy<DeclName>;
/// See [`RuntimeTy`].
pub type TyTemplate = baml_type::TyTemplate<DeclName>;
/// See [`RuntimeTy`].
pub type TyTemplateInterface = baml_type::TyTemplateInterface<DeclName>;
use subenum::subenum;

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, salsa::Update)]
pub enum OptLevel {
    Zero,
    #[default]
    One,
    Two,
}

// ============================================================================
// Function
// ============================================================================

/// What the VM lands in a handler block with: the frame slots it writes the
/// caught error and its `baml.errors.Context` into before jumping to the
/// block. The context is the error's identity while it is in flight: a
/// rethrow from the handler carries it, and a throw during the handler's body
/// takes it as its cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landing {
    pub error_local: Local,
    pub context_local: Local,
}

/// The bytecode body of a MIR function — blocks, locals, and associated data.
///
/// This is the inner data for `MirFunctionKind::Bytecode`. All field accessors
/// live here, so callers destructure the `MirFunctionKind` first and then work
/// with `&MirFunctionBody` / `&mut MirFunctionBody` directly — no panics.
#[derive(Debug, Clone)]
pub struct MirFunctionBody<'db> {
    /// All basic blocks in the function.
    pub blocks: Vec<BasicBlock<'db>>,
    /// Entry block index (always 0 by convention).
    pub entry: BlockId,
    /// Local variable declarations.
    pub locals: Vec<LocalDecl>,
}

impl<'db> MirFunctionBody<'db> {
    /// Get a basic block by ID.
    pub fn block(&self, id: BlockId) -> &BasicBlock<'db> {
        &self.blocks[id.0]
    }

    /// Get a local declaration by ID.
    pub fn local(&self, id: Local) -> &LocalDecl {
        &self.locals[id.0]
    }

    /// The handler blocks — those some block unwinds to — with what the VM
    /// lands in each with.
    pub fn handlers(&self) -> impl Iterator<Item = (BlockId, Landing)> + '_ {
        self.blocks
            .iter()
            .filter_map(|block| block.landing.map(|landing| (block.id, landing)))
    }

    /// Whether `block` is a handler block.
    pub fn is_handler(&self, block: BlockId) -> bool {
        self.blocks[block.0].landing.is_some()
    }

    /// Iterate `(handler_block, error_local)` pairs, one per handler.
    pub fn unwind_error_locals(&self) -> impl Iterator<Item = (BlockId, Local)> + '_ {
        self.handlers()
            .map(|(handler, landing)| (handler, landing.error_local))
    }
}

/// Whether a MIR function has a bytecode body or is a Rust-bound builtin.
#[derive(Debug, Clone)]
pub enum MirFunctionKind<'db> {
    /// Has a body that will be compiled to bytecode.
    Bytecode(MirFunctionBody<'db>),
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
    pub param_types: Vec<TyTemplate>,
    /// Whether each parameter has a default, parallel to `param_names`.
    pub param_has_default: Vec<bool>,
    /// The return type. A template over the callee frame's type-arg slots (see
    /// [`Self::param_types`]).
    pub return_type: TyTemplate,
    /// The throws type, as a template over the callee frame's type-arg slots.
    /// `never` == cannot throw (the same spelling a function type uses), so a
    /// reconstructed value signature and a written type agree.
    pub throws_type: TyTemplate,
    /// The declaration's joined `///` doc-comment lines, if any.
    pub docstring: Option<String>,
    /// The name the declaration was written with; `None` for lambdas
    /// (which have no source-level name).
    pub name: Option<String>,
    /// Display strings for the generic type parameters (`T extends Bound`).
    pub display_type_params: Vec<String>,
    /// Interface bounds, parallel to the callee frame's De Bruijn generic
    /// parameter slots. Kept as executable metadata (not display text) so
    /// reflection and runtime specialization can check them.
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
    pub interface: DeclName,
    pub args: Vec<TyTemplate>,
    pub assoc: Vec<(baml_type::Name, TyTemplate)>,
}

/// A point where lowering could not produce code for a CHECKED program: what
/// the checker recorded and what lowering finds disagree, which is a compiler
/// bug and never a user error (lowering runs only on a program with no
/// errors). A function or initializer that hits one has no MIR — lowering
/// answers with this instead, and the compile fails.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct MirInternalError {
    pub message: String,
    /// The source being lowered when the inconsistency was found.
    pub span: Option<Span>,
}

impl std::fmt::Display for MirInternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MirInternalError {}

/// A function represented as a control flow graph.
#[derive(Debug, Clone)]
pub struct MirFunction<'db> {
    /// Parameter count.
    pub arity: usize,
    /// Source span for error reporting.
    pub span: Option<Span>,
    /// The function's identity: its declaration, or — for a lambda or a
    /// tagged-template body closure — the function it was lowered inside
    /// plus its ordinal there. Names are rendered from it at emit's boundary.
    pub identity: MirFunctionId<'db>,
    /// Whether this function has bytecode or is a builtin.
    pub kind: MirFunctionKind<'db>,
    /// Child lambda functions defined inside this function's body.
    ///
    /// Indexed by `lambda_idx` in `Rvalue::MakeClosure`.
    /// Empty until lambda lowering is implemented.
    pub lambdas: Vec<MirFunction<'db>>,
    /// Runtime signature metadata, populated by `lower_lambda` for lambda
    /// functions only. Top-level functions get theirs from TIR `func_data`
    /// during emit; `None` there (and on synthetic adapters, which fall back
    /// to no metadata).
    pub signature: Option<RuntimeSignature>,
}

// Safety: replacement-only `Update` (always report changed). MIR feeds the
// untracked emit stage, so backdating buys nothing, and the tree has no
// `PartialEq` to compare with; unconditionally replacing the old value is
// always sound under the `Update` contract ONLY under this premise:
// `MirFunction` OWNS all of its data — its sole `'db` members are Copy
// interned ids (no `&'db` references, no drop glue that could observe the
// old revision). Adding any `&'db` field would make the blind replacement
// UB; re-derive that before extending the struct.
#[expect(unsafe_code)]
unsafe impl salsa::Update for MirFunction<'_> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: pointer is Salsa-owned and valid for replacement.
        unsafe {
            std::ptr::drop_in_place(old_pointer);
            std::ptr::write(old_pointer, new_value);
        }
        true
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
    /// Whether a nested closure captures this local.
    ///
    /// When `true`, the local's stack slot holds an `Object::Cell` rather than
    /// the value directly, and reads/writes go through `LoadDeref`/`StoreDeref`.
    /// The slot holds a cell from the binding's [`StatementKind::FreshCell`]
    /// onward — parameters from frame entry, where the emitter wraps them —
    /// and every access is dominated by that statement (`verify_mir` checks
    /// this). Set when the local is declared, from HIR's capture analysis;
    /// nothing flips it afterwards.
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
pub struct BasicBlock<'db> {
    /// Unique identifier.
    pub id: BlockId,
    /// Statements executed in order.
    pub statements: Vec<Statement<'db>>,
    /// How this block exits (required after construction).
    pub terminator: Option<Terminator<'db>>,
    /// Source span covering this block.
    pub span: Option<Span>,
    /// Source span for the terminator.
    pub terminator_span: Option<Span>,
    /// The handler a throw or panic raised anywhere in this block unwinds to
    /// — the lexically enclosing `catch` handler or `defer` landing pad — or
    /// `None` to leave the frame. Fixed when the block is created, so a block
    /// is always a maximal run of code under one handler, and the emitter
    /// derives the exception table from it. A terminator that carries its own
    /// `unwind` edge agrees with it by construction.
    pub unwind: Option<BlockId>,
    /// The innermost handler whose body this block is lexically part of: a
    /// throw here is "during handling of" that handler's error and chains
    /// onto its context (BEP-042 cause chain). A handler block is part of its
    /// own body.
    pub handling: Option<BlockId>,
    /// Set on a handler block: what the VM lands here with.
    pub landing: Option<Landing>,
    /// Whether this block is part of a `defer` body, which runs shielded from
    /// cancellation: a thread executing here (or in a callee called from
    /// here) is not delivered `Cancelled` at its yield points, so cleanup may
    /// suspend. Fixed at creation, like `unwind`; the emitter derives the
    /// bytecode's shield table from it.
    pub shielded: bool,
}

impl BasicBlock<'_> {
    /// Create a new empty basic block.
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            statements: Vec::new(),
            terminator: None,
            span: None,
            terminator_span: None,
            unwind: None,
            handling: None,
            landing: None,
            shielded: false,
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
    ///
    /// The slot is a frame type-argument index, the same space
    /// `TyTemplate::TypeArgRef` reads, so it carries that space's width: emit
    /// compares the two directly, and a lossy conversion there would decide a
    /// soundness question (whether a template read is clobbered) by accident.
    BindType(u32),
}

/// The kind of a MIR statement.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum StatementKind<'db> {
    /// Assign a value to a place: `_1 = <rvalue>`
    Assign {
        destination: Place,
        value: Rvalue<'db>,
    },

    /// Drop a value (run destructor if any).
    Drop(Place),

    /// Give a captured local a new cell: the slot now points at a cell no
    /// closure has captured yet.
    ///
    /// Emitted where the binding is created, so a declaration that runs once
    /// per loop iteration hands each iteration's closures their own cell. The
    /// new cell holds `null`, or — with `carry_value` — the value of the cell
    /// it replaces, which is how a C-style `for` header binding is copied into
    /// the next iteration before the step runs (JS/Go semantics).
    ///
    /// Only ever targets a local whose `is_captured` is set, and every read,
    /// write, or closure capture of that local is dominated by one of its
    /// `FreshCell`s: that is what makes a deref load discardable
    /// ([`Rvalue::can_discard`]) and per-iteration closures correct.
    FreshCell { local: Local, carry_value: bool },

    /// Compiler intrinsic — a void side effect (log, send event).
    /// Lowered from calls to `$compiler_intrinsic` functions.
    Intrinsic {
        op: IntrinsicOp,
        args: Vec<Operand<'db>>,
    },

    /// Write an interface field on a receiver whose concrete type is not known
    /// statically — the store counterpart of [`Rvalue::VirtualFieldAccess`], with
    /// the same operand meaning and the same resolution.
    ///
    /// A statement rather than an `Assign` to a `Place`, because the destination
    /// slot is only known once the receiver's impl is resolved at run time.
    VirtualFieldStore {
        iface: TyTemplateInterface,
        receiver: Operand<'db>,
        field_index: u32,
        field: Name,
        value: Operand<'db>,
    },

    /// No-op (placeholder for removed statements).
    Nop,
}

// ============================================================================
// Terminator
// ============================================================================

/// One arm key of a [`Terminator::Switch`].
///
/// The compiler never knows a declared head's tag: a class arm names its
/// DECLARATION, and whoever lays that declaration into an image assigns the
/// tag the switch dispatches on. A primitive kind's tag is a fixed constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitchKey<'db> {
    /// A fixed integer: an int literal, an enum discriminant, or a primitive
    /// kind's tag.
    Int(i64),
    /// The tag of this class declaration.
    Class(baml_compiler2_hir_ty::extern_loc::ClassRef<'db>),
}

/// What a type test decides membership in, as lowering determined it.
///
/// A nominal test names its declaration: lowering resolved the head once, and
/// the emitter tests identity against the declaration it is handed, never
/// resolving the head again.
#[derive(Debug, Clone)]
pub enum TypeTest<'db> {
    /// An instance of `class` at `args`; with none, any instance of it (a
    /// class without parameters has none).
    Class {
        class: baml_compiler2_hir_ty::extern_loc::ClassRef<'db>,
        args: Vec<TyTemplate>,
    },
    /// A value of this enum.
    Enum(baml_compiler2_hir_ty::extern_loc::EnumRef<'db>),
    /// Any other type, decided against the template — never a class or an
    /// enum, which lowering states by declaration.
    Template(TyTemplate),
}

impl TypeTest<'_> {
    /// Every frame type-argument slot the test reads.
    pub fn for_each_type_arg_ref(&self, f: &mut impl FnMut(u32)) {
        match self {
            Self::Class { args, .. } => {
                for arg in args {
                    arg.for_each_type_arg_ref(f);
                }
            }
            Self::Enum(_) => {}
            Self::Template(template) => template.for_each_type_arg_ref(f),
        }
    }

    /// The type the test decides membership in, as a template: what a
    /// display shows.
    pub fn template(&self, db: &dyn baml_compiler2_hir::Db) -> TyTemplate {
        match self {
            Self::Class { class, args } => TyTemplate::Class(
                baml_compiler2_hir_ty::layout::class_head(db, *class),
                args.clone().into_boxed_slice(),
            ),
            Self::Enum(enum_ref) => {
                TyTemplate::Enum(baml_compiler2_hir_ty::layout::enum_head(db, *enum_ref))
            }
            Self::Template(template) => template.clone(),
        }
    }
}

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

    /// Test one value and bind that same value to `destination` on success.
    NarrowBind {
        source: Operand<'db>,
        test: TypeTest<'db>,
        destination: Local,
        then_block: BlockId,
        else_block: BlockId,
    },

    /// Multi-way branch on an integer discriminant or a value's type tag.
    Switch {
        discriminant: Operand<'db>,
        /// Arms: (key, target block).
        arms: Vec<(SwitchKey<'db>, BlockId)>,
        /// Default target if no arm matches.
        otherwise: BlockId,
        /// Whether this switch is exhaustive (all possible values covered).
        /// When true, the last arm's comparison can be skipped since if all
        /// other arms failed, the discriminant must match the last one.
        exhaustive: bool,
        /// Symbolic names for arm keys (debug metadata only): human-readable
        /// names like `"DispatchState.Alpha"`, `"int"`, or a class's name.
        arm_names: Vec<(SwitchKey<'db>, String)>,
    },

    /// Return from function.
    ///
    /// The return value should already be stored in `_0` (the return place).
    Return,

    /// Call a function.
    Call {
        /// Whether the final `args` operand is an invocation trace attachment,
        /// rather than a callee parameter.
        has_trace: bool,
        /// The value slots this site was checked against, leading type
        /// arguments excluded. `None` only for compiler-synthesized calls,
        /// whose operands are already in the callee's own layout.
        argument_layout: Option<baml_type::CallLayout>,
        /// The function to call.
        callee: Operand<'db>,
        /// Arguments to pass.
        ///
        /// The first `ntypeargs` operands are type-argument values (`Object::Type`)
        /// followed by the `nargs` regular value arguments.  The `ntypeargs`
        /// count tells the emitter how many leading slots to account for in
        /// the `Instruction::Call { ntypeargs }` bytecode instruction.
        args: Vec<Operand<'db>>,
        /// Number of leading `args` entries that carry type arguments.
        ///
        /// Zero for non-generic calls (the common case).  Non-zero for
        /// calls to generic functions where at least one type argument is
        /// threaded at the call site (explicit `<T>` or type-arg forwarding).
        ntypeargs: usize,
        destination: Place,
        /// Block to jump to after call returns normally.
        target: BlockId,
        /// Block to jump to if call throws (for catch).
        unwind: Option<BlockId>,
    },

    /// Open-world virtual interface-method dispatch.
    ///
    /// Used when `Self`'s concrete type is not statically known — a bounded
    /// type-var `T extends I`, an interface-existential `I`, a union, or
    /// `Self` inside an interface default body. The implementation is
    /// resolved **at runtime** from the concrete type of the dispatch argument
    /// (`args[ntypeargs + self_arg]`, the method's one `Self`-typed parameter)
    /// against `iface` (coherence makes `(Self, iface)` pick at most one
    /// impl), then invoked exactly like a direct [`Terminator::Call`] — no
    /// value is materialized. This is the open-world replacement for the old
    /// compile-time type-tag switch.
    VirtualCall {
        /// Whether the final `args` operand is an invocation trace attachment.
        has_trace: bool,
        /// The value slots this site was checked against (receiver included,
        /// type arguments excluded); see [`Terminator::Call::argument_layout`].
        argument_layout: Option<baml_type::CallLayout>,
        /// The interface to resolve against, as a template the emitter pushes
        /// with `LoadType`. Non-generic today (`baml.ops.Equals`/`Compare`); a
        /// parameterized interface bakes its arguments into the template.
        iface: TyTemplateInterface,
        /// The interface method to dispatch (e.g. `"eq"`, `"lt"`, `"neq"`).
        method: String,
        /// `args[..ntypeargs]` are the method-level type-argument values
        /// (`Object::Type`, for a generic interface method like
        /// `Iterator.map<R, E2>`); `args[ntypeargs..]` are the value args in
        /// the method's **declared parameter order** (`self` first when the
        /// method takes a receiver). The type args are appended to the
        /// resolved frame.
        args: Vec<Operand<'db>>,
        /// Number of leading `args` entries that are method-level type arguments.
        /// Zero for a non-generic method.
        ntypeargs: usize,
        /// Index among the value args (`0..args.len() - ntypeargs`) of the one
        /// whose runtime concrete type is `Self`: the method's single required
        /// `Self`-typed parameter, `0` for a `self` receiver.
        self_arg: usize,
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
        callee: Operand<'db>,
        /// Arguments to the sys-op.
        args: Vec<Operand<'db>>,
        destination: Place,
        /// Block to resume at after the sys-op returns.
        target: BlockId,
        /// Block to jump to if the sys-op throws (catch context).
        unwind: Option<BlockId>,
    },

    /// Launch a `baml.spawn.Plan<T, E>` as a new task and bind its
    /// `Future<T, E>` into `future`. The plan carries everything the engine
    /// needs — the body, its limits and cancel tokens, its cancellation
    /// parent, and the future's types — so it is the only operand. Launching
    /// never throws.
    Spawn {
        /// The `baml.spawn.Plan` value.
        plan: Operand<'db>,
        /// Where the future handle is stored.
        future: Place,
        /// Block to continue at once the task is launched.
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
        futures: Operand<'db>,
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
        value: Operand<'db>,
    },

    /// Re-throw a caught error value with the context it was caught with, so
    /// it keeps its original trace and cause.
    Rethrow {
        /// The caught error value to rethrow.
        value: Operand<'db>,
        /// Its `baml.errors.Context`: the landing's context local.
        context: Operand<'db>,
    },

    /// If the value is a panic instance (`baml.panics.*`), throw it.
    /// Otherwise continue to `otherwise` block.
    ///
    /// Used before wildcard catch arms to prevent them from swallowing
    /// panics the programmer didn't explicitly name.
    ThrowIfPanic {
        value: Operand<'db>,
        /// The value's `baml.errors.Context`, carried by the rethrow.
        context: Operand<'db>,
        otherwise: BlockId,
    },

    /// Short-circuit `&&` / `||` / `??`.
    ///
    /// On the short-circuit edge, assign `operand` to `destination` and go
    /// to `join`: when false for `&&`, true for `||`, or non-null for `??`.
    /// Otherwise, go to `eval_rhs` without assigning `destination`.
    ///
    /// The `eval_rhs` block must assign to `destination` and then goto `join`.
    /// At `join`, `destination` holds the expression value. Stackification
    /// decides whether that value lives on the operand stack or in a slot.
    ShortCircuit {
        operand: Operand<'db>,
        kind: ShortCircuitKind,
        destination: Place,
        eval_rhs: BlockId,
        join: BlockId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortCircuitKind {
    And,
    Or,
    Coalesce,
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
///
/// Cell access is explicit. A local a closure captures
/// ([`LocalDecl::is_captured`]) holds a cell pointer, and `Local(l)` names
/// that pointer; `Capture(i)` names the pointer in a closure's capture array.
/// Those two variants are also [`CellId`], the identity of a cell; the value
/// behind one is `Deref(cell)`, and every read or write of a captured binding
/// goes through it. Only a `MakeClosure` capture operand and a `FreshCell`
/// target name the pointer bare. `verify_mir` checks all of this.
#[subenum(CellId(derive(Copy)))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    /// A local variable: `_1`. For a captured local, its cell pointer.
    #[subenum(CellId)]
    Local(Local),

    /// Field access: `_1.field_idx`
    Field { base: Box<Place>, field: usize },

    /// Indexing: `_1[_2]`
    Index {
        base: Box<Place>,
        index: Local,
        kind: IndexKind,
    },

    /// The cell pointer in the `idx`-th slot of the enclosing
    /// `Object::Closure.captures` array. Reading it bare emits `CaptureRef`
    /// (forwarding the cell to a nested closure); the value behind it is
    /// `Deref(Capture(idx))`. Only valid inside a lambda body.
    #[subenum(CellId)]
    Capture(usize),

    /// The value in a cell: `*_1`, `*capture[0]`.
    ///
    /// Reads emit `LoadDeref`/`LoadCapture` and writes `StoreDeref`/`StoreCapture`.
    Deref(CellId),
}

impl CellId {
    /// The local whose slot holds this cell's pointer, if it is a local's cell.
    pub fn local(&self) -> Option<Local> {
        match self {
            CellId::Local(local) => Some(*local),
            CellId::Capture(_) => None,
        }
    }
}

impl fmt::Display for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellId::Local(l) => write!(f, "{l}"),
            CellId::Capture(idx) => write!(f, "capture[{idx}]"),
        }
    }
}

impl Place {
    /// Create a place for a local variable.
    pub fn local(local: Local) -> Self {
        Place::Local(local)
    }

    /// Get the base local of this place, if it is rooted in a local.
    ///
    /// Transparent through `Deref`: the local whose cell holds the value is
    /// still the local the place is rooted in.
    pub fn base_local(&self) -> Option<Local> {
        match self {
            Place::Local(l) => Some(*l),
            Place::Field { base, .. } | Place::Index { base, .. } => base.base_local(),
            Place::Deref(cell) => cell.local(),
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
            Place::Deref(cell) => write!(f, "*{cell}"),
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

    /// Create an array: `[_1, _2, _3]`. The first field is the static element
    /// type (a [`TyTemplate`] so a generic `T[]` resolves against the frame's
    /// type args at runtime), carried so the heap array records its declared
    /// element type.
    Array(TyTemplate, Vec<Operand<'db>>),

    /// Create a byte array from a literal: `b"hello"`
    Uint8Array(Vec<u8>),

    /// Create a map: `{ key1: value1, key2: value2, ... }`. Each entry is a
    /// (key, value) pair. The first two fields are the static key and value
    /// types (as [`TyTemplate`]s), carried so the heap map records its declared
    /// key/value types.
    Map(TyTemplate, TyTemplate, Vec<(Operand<'db>, Operand<'db>)>),

    /// Create an aggregate (array, class instance): `ClassName { _1, _2 }`. An
    /// enum variant is a [`Constant::EnumVariant`], never an aggregate.
    Aggregate {
        kind: AggregateKind<'db>,
        fields: Vec<Operand<'db>>,
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
        operand: Operand<'db>,
        test: TypeTest<'db>,
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
    IsTypeTag { operand: Operand<'db>, tag: i64 },

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
        captures: Vec<Operand<'db>>,
        /// Templates for enclosing generic type params captured by this closure.
        /// Empty (the common case) when the enclosing function has no type params.
        type_arg_templates: Vec<TyTemplate>,
    },

    /// Create a bound method value from a class-inherent method and its
    /// receiver: `receiver` is the instance the method is bound to.
    MakeBoundMethod {
        func: baml_compiler2_hir_ty::extern_loc::FunctionRef<'db>,
        receiver: Operand<'db>,
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
        receiver: Operand<'db>,
        /// Method-level type-argument templates from the reference site (a
        /// generic interface method's own generics, when specialized there).
        /// Appended to the resolved impl frame by the VM — dropping them would
        /// lose the method's own generics.
        type_args: Vec<TyTemplate>,
    },

    /// Resolve an *interface* method to an unbound callable from a `Self`
    /// TYPE — the type-keyed twin of [`Rvalue::MakeVirtualBoundMethod`],
    /// where `Self` is PASSED as a template rather than DERIVED from a
    /// receiver value. The only dispatch form for a method with no `self`
    /// receiver (`(Widget as Makeable).make`), and the value form of any
    /// qualified item reference. The VM resolves the impl (coherence
    /// guarantees at most one) and produces a capture-less closure carrying
    /// the impl's realized frame.
    MakeVirtualFunction {
        /// The `Self` type to resolve on, pushed with `LoadType` — a typevar
        /// `Self` (`(T as Makeable).make` in a generic caller) lowers to its
        /// `TypeArgRef` slot and arrives at the resolver realized.
        self_ty: TyTemplate,
        /// The interface to resolve against, as a template the emitter pushes
        /// with `LoadType`.
        iface: TyTemplateInterface,
        /// The interface method's name.
        method: String,
        /// Method-level type-argument OPERANDS from the reference site,
        /// appended to the resolved impl frame by the VM. Every argument is
        /// a template today - a scoped `type T = …` slot included - so these
        /// could be templates; they stay operands because the producer
        /// materializes each as a `LoadType` temp anyway and the VM pops an
        /// `Object::Type` either way, which keeps one stack discipline for
        /// the whole call shape.
        type_args: Vec<Operand<'db>>,
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
        receiver: Operand<'db>,
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
        func: baml_compiler2_hir_ty::extern_loc::FunctionRef<'db>,
        /// One template per type argument; may contain `TypeArgRef(N)`.
        type_arg_templates: Vec<TyTemplate>,
    },

    /// Specialize a runtime callable *value* with explicit type arguments
    /// (`g<int>` where `g` is a local/captured function value, not a
    /// compile-time-resolvable function reference). The emitter pushes a
    /// `LoadType` for each template then a `MakeGenericFunctionFromValue`
    /// instruction, which wraps the evaluated `value` in a `Closure` carrying
    /// the types as `captured_type_args`. Used when `lower_generic_apply`'s base
    /// is not a function ref; the function-ref cases use `Constant::GenericFunction`
    /// (concrete) or `MakeGenericFunction` (param-dependent) instead.
    MakeGenericFunctionFromValue {
        /// The callable value to specialize.
        value: Operand<'db>,
        /// One template per type argument; may contain `TypeArgRef(N)`.
        type_arg_templates: Vec<TyTemplate>,
    },

    /// Materialize a `Ty` from a `TyTemplate`.
    ///
    /// For a fully-realized template, the `Ty` is baked in at compile time.
    /// For templates containing `TypeArgRef(N)`, the VM substitutes
    /// `frame.type_args[N]` at execution time.
    ///
    /// Emitted by the `reflect.Type.of<T>()` intrinsic.
    /// Lowers to `Instruction::LoadType(const_idx)` in bytecode.
    LoadType(TyTemplate),

    /// Reify the package lexically enclosing this call site. The package name
    /// is baked by lowering; dynamically compiled code substitutes its owning
    /// runtime package at execution.
    CurrentPackage(String),
}

impl Rvalue<'_> {
    /// Whether an unused evaluation can be erased, including its operand reads.
    /// Allocation identity is unobservable when the result does not escape, but
    /// evaluating an initializer can still fail (checked arithmetic or indexing).
    pub fn can_discard(&self) -> bool {
        self.can_discard_with(|_| false)
    }

    /// A bounds proof applies to this evaluation site, not to the entire local.
    pub(crate) fn can_discard_with(&self, in_bounds: impl Fn(&Place) -> bool) -> bool {
        fn read(place: &Place, in_bounds: &impl Fn(&Place) -> bool) -> bool {
            match place {
                // A deref load cannot fail: every access of a captured local is
                // dominated by its `FreshCell` (`verify_mir`), so the cell exists.
                Place::Local(_) | Place::Capture(_) | Place::Deref(_) => true,
                // A fixed field projection is type-checked, not a user accessor.
                // An indexing operation in its base still needs its own proof.
                Place::Field { base, .. } => read(base, in_bounds),
                Place::Index { base, .. } => read(base, in_bounds) && in_bounds(place),
            }
        }
        let operand = |op: &Operand<'_>| match op {
            Operand::Constant(_) => true,
            Operand::Copy(place) | Operand::Move(place) => read(place, &in_bounds),
        };
        match self {
            Self::Use(op) => operand(op),
            Self::BinaryOp { op, left, right } => {
                matches!(
                    op,
                    BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                        | BinOp::BitAnd
                        | BinOp::BitOr
                        | BinOp::BitXor
                ) && operand(left)
                    && operand(right)
            }
            Self::UnaryOp { op, operand: arg } => {
                matches!(op, UnaryOp::Not | UnaryOp::Truthy) && operand(arg)
            }
            Self::IsType { operand: arg, .. } | Self::IsTypeTag { operand: arg, .. } => {
                operand(arg)
            }
            Self::TypeTag(place) | Self::Discriminant(place) | Self::Len(place) => {
                read(place, &in_bounds)
            }
            Self::LoadType(_)
            | Self::CurrentPackage(_)
            | Self::Uint8Array(_)
            | Self::MakeGenericFunction { .. } => true,
            Self::Array(_, elements)
            | Self::Aggregate {
                fields: elements, ..
            } => elements.iter().all(operand),
            Self::Map(_, _, entries) => entries
                .iter()
                .all(|(key, value)| operand(key) && operand(value)),
            Self::MakeClosure { captures, .. } => captures.iter().all(operand),
            Self::MakeBoundMethod { receiver, .. } => operand(receiver),
            Self::VirtualFieldAccess { receiver, .. } => {
                // The type checker proves the implements rule and total field
                // links (E0124). Resolution failure is a VM internal invariant
                // violation, not a BAML panic; the receiver can still trap.
                operand(receiver)
            }
            Self::MakeVirtualBoundMethod { .. }
            | Self::MakeVirtualFunction { .. }
            | Self::MakeGenericFunctionFromValue { .. } => false,
        }
    }
}

/// The kind of aggregate being constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateKind<'db> {
    /// An array.
    Array,
    /// A class instance with optional type-arg templates.
    ///
    /// `class` is the declaration the literal constructs, wherever it lives;
    /// its wire spelling is rendered only at emit's boundary
    /// ([`crate::class_link_name`]). `type_arg_templates` is non-empty only
    /// for generic class instantiations: each element corresponds to one
    /// class-level type parameter in De Bruijn order (matching
    /// `enclosing_generic_params()`). These templates are emitted as
    /// `LoadType` instructions before `AllocInstance` so the VM can store
    /// resolved `Ty` values in `Instance::class_type_args`.
    Class {
        class: baml_compiler2_hir_ty::extern_loc::ClassRef<'db>,
        type_arg_templates: Vec<TyTemplate>,
    },
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
    /// items that ARE functions; a top-level `let` read is
    /// [`Constant::GlobalItem`].
    Function(baml_compiler2_hir_ty::extern_loc::FunctionRef<'db>),
    /// A top-level `let` read (a `client` declaration is one): the value
    /// the program's `$init` stored in the binding's global slot. Emitted as
    /// a plain `LoadGlobal`, never wrapped — the slot holds an ordinary value
    /// (an instance, a closure, ...), not a `Function` object. The only
    /// non-function item that is a value; a type declaration in value
    /// position is a checker error and never reaches here.
    GlobalItem(baml_compiler2_hir::loc::LetLoc<'db>),
    /// A generic function instantiated with concrete type arguments
    /// (`foo<int>` referenced as a value). Emitted as a pooled, interned
    /// `Object::GenericFunction` so identical instantiations share one object
    /// (pointer-stable identity) and calling it seeds `frame.type_args`.
    GenericFunction {
        /// The base generic function.
        func: baml_compiler2_hir_ty::extern_loc::FunctionRef<'db>,
        /// The concrete type arguments — fully realized (no type parameters),
        /// exactly what the runtime `Object::GenericFunction` carries.
        type_args: Vec<RealizedTy>,
    },
    /// An enum variant value.
    EnumVariant {
        /// The enum's declaration, wherever it lives.
        enum_ref: baml_compiler2_hir_ty::extern_loc::EnumRef<'db>,
        /// The variant's discriminant: its index in the enum's declared
        /// variant order (`layout::enum_variant_index`), the value the
        /// runtime `AllocVariant` carries. The name is rendered from it only
        /// for display.
        index: u32,
    },
}

/// A function's identity in MIR: its declaration, or — for a lambda or a
/// tagged-template body closure — the owner it was lowered inside plus its
/// ordinal there. Rendered to a name only at emit's boundary
/// ([`MirFunctionId::link_name`] / [`crate::function_link_name`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirFunctionId<'db> {
    Declared(baml_compiler2_hir::loc::FunctionLoc<'db>),
    /// The `ordinal`-th synthetic function of its `kind` lowered inside
    /// `parent`.
    Synthetic {
        parent: Box<FunctionOwner<'db>>,
        kind: SyntheticKind,
        ordinal: usize,
    },
}

/// What a synthetic function was lowered from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticKind {
    /// A lambda expression.
    Lambda,
    /// A tagged template's body closure.
    Tagged,
}

/// Where a function body is lowered: a declared function, a top-level `let`
/// initializer (lowered as a body, never a `MirFunction` of its own — it
/// only ever OWNS synthetic functions), or a synthetic function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionOwner<'db> {
    Function(baml_compiler2_hir::loc::FunctionLoc<'db>),
    Let(baml_compiler2_hir::loc::LetLoc<'db>),
    Synthetic(Box<MirFunctionId<'db>>),
}

impl MirFunctionId<'_> {
    /// The name this function links and displays as — the declaration's
    /// link name, or the synthetic spelling `<lambda(owner, i)>` /
    /// `<tagged(owner, i)>` under an EMPTY package segment (the historical
    /// rendering, which the bytecode display pins).
    pub fn link_name(&self, db: &dyn crate::Db) -> String {
        match self {
            MirFunctionId::Declared(func) => crate::lower::definition_link_name(
                db,
                baml_compiler2_hir::contributions::Definition::Function(*func),
            ),
            MirFunctionId::Synthetic { .. } => format!(".{}", self.short_name(db)),
        }
    }

    /// The spelling a nested synthetic function names its owner by: the
    /// declaration's link-name tail (`definition_short_name` — `f`,
    /// `Class.m`, `Interface.m`, `<(target as iface)>.m`), or the owner's
    /// own synthetic spelling.
    pub fn short_name(&self, db: &dyn crate::Db) -> String {
        match self {
            MirFunctionId::Declared(func) => crate::lower::definition_short_name(
                db,
                baml_compiler2_hir::contributions::Definition::Function(*func),
            ),
            MirFunctionId::Synthetic {
                parent,
                kind,
                ordinal,
            } => {
                let kind = match kind {
                    SyntheticKind::Lambda => "lambda",
                    SyntheticKind::Tagged => "tagged",
                };
                format!("<{kind}({}, {ordinal})>", parent.short_name(db))
            }
        }
    }

    /// This identity as a [`fmt::Display`] value that renders
    /// [`Self::link_name`] when formatted, not when created.
    pub fn display<'a>(&'a self, db: &'a dyn crate::Db) -> impl fmt::Display + 'a {
        // Assertion messages format their arguments only when they fire, so
        // a caller that never fails never pays for `definition_link_name`.
        struct Lazy<'a, 'db> {
            db: &'a dyn crate::Db,
            id: &'a MirFunctionId<'db>,
        }
        impl fmt::Display for Lazy<'_, '_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.id.link_name(self.db))
            }
        }
        Lazy { db, id: self }
    }
}

impl<'db> FunctionOwner<'db> {
    /// The spelling a synthetic function names this owner by — see
    /// [`MirFunctionId::short_name`].
    pub fn short_name(&self, db: &dyn crate::Db) -> String {
        use baml_compiler2_hir::contributions::Definition;
        match self {
            FunctionOwner::Function(func) => {
                crate::lower::definition_short_name(db, Definition::Function(*func))
            }
            FunctionOwner::Let(binding) => {
                crate::lower::definition_short_name(db, Definition::Let(*binding))
            }
            FunctionOwner::Synthetic(id) => id.short_name(db),
        }
    }

    /// The identity of a function built under this owner. A `let`
    /// initializer is lowered as a body, so it never builds one.
    pub fn into_identity(self) -> MirFunctionId<'db> {
        match self {
            FunctionOwner::Function(func) => MirFunctionId::Declared(func),
            FunctionOwner::Synthetic(id) => *id,
            FunctionOwner::Let(_) => {
                unreachable!("a top-level let initializer is lowered as a body, never built")
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
