use baml_type::TyTemplate;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{Bytecode, HeapPtr, SysOp, Value};

/// Function type.
///
/// # Native Function Pointers
///
/// Native functions are stored as type-erased `*const ()` pointers to avoid
/// a circular dependency between crates:
///
/// - `baml_vm` defines `NativeFunction = fn(&mut Vm, &[Value]) -> Result<...>`
/// - This type references `Vm`, which is defined in `baml_vm`
/// - `baml_vm_types` cannot depend on `baml_vm` (that would be circular)
///
/// The type erasure allows different stages:
///
/// - **Compile time**: The compiler emits `NativeUnresolved` for built-in functions
/// - **Runtime**: The VM resolves these to `Native(ptr)` at load time
///
/// The resolution happens in `baml_vm::native::attach_builtins()`, which looks up
/// native function names and casts the real function pointers to `*const ()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FunctionKind {
    /// Regular executable function.
    ///
    /// The VM pushes a call frame onto the call stack and runs the bytecode.
    Bytecode,

    /// System operation (LLM calls, HTTP requests, file I/O, etc.).
    ///
    /// The VM yields control to the engine which executes the operation
    /// asynchronously via static dispatch on the `SysOp` enum.
    SysOp(SysOp),

    /// Unresolved native function (placeholder).
    ///
    /// The compiler emits this for built-in functions. The VM resolves these
    /// to `Native(ptr)` at load time. Panics if executed without resolution.
    NativeUnresolved,

    /// Rust native function (type-erased pointer).
    ///
    /// Contains a type-erased function pointer that the VM casts back to
    /// the real `NativeFunction` type when calling.
    ///
    /// # Safety
    ///
    /// The pointer must be cast from a valid `NativeFunction` and only
    /// cast back to that same type when calling.
    Native(*const ()),
}

// SAFETY: FunctionKind contains a raw pointer (*const ()) that points to
// immutable code (function pointers). Code doesn't change at runtime,
// so sharing the pointer between threads is safe.
#[allow(unsafe_code)]
unsafe impl Send for FunctionKind {}
#[allow(unsafe_code)]
unsafe impl Sync for FunctionKind {}

// Borsh proxy mirroring the previous serde-side shape — `Native(*const ())`
// is runtime-only and must collapse to `NativeUnresolved` on the wire.
#[derive(BorshSerialize, BorshDeserialize)]
enum FunctionKindWire {
    Bytecode,
    SysOp(SysOp),
    NativeUnresolved,
}

impl BorshSerialize for FunctionKind {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let wire = match self {
            Self::Bytecode => FunctionKindWire::Bytecode,
            Self::SysOp(op) => FunctionKindWire::SysOp(*op),
            Self::NativeUnresolved | Self::Native(_) => FunctionKindWire::NativeUnresolved,
        };
        wire.serialize(writer)
    }
}

impl BorshDeserialize for FunctionKind {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        Ok(match FunctionKindWire::deserialize_reader(reader)? {
            FunctionKindWire::Bytecode => Self::Bytecode,
            FunctionKindWire::SysOp(op) => Self::SysOp(op),
            FunctionKindWire::NativeUnresolved => Self::NativeUnresolved,
        })
    }
}

/// LLM-specific metadata for a function.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum FunctionMeta {
    Llm { client: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    Builtin,
    /// Synthesized by the auto-derive pass (e.g. `to_json` / `from_json`
    /// methods generated on every user class). Filterable from bytecode
    /// snapshots via `Function::origin`.
    AutoDerive,
}

impl FunctionOrigin {
    pub const fn is_user_callable(self) -> bool {
        matches!(self, Self::UserDefined | Self::Companion | Self::AutoDerive)
    }

    /// True for methods synthesized by the auto-derive pass; used to filter
    /// them from default bytecode snapshots in tests.
    pub const fn is_auto_derived(self) -> bool {
        matches!(self, Self::AutoDerive)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CaptureOption {
    #[default]
    Disabled,
    Auto,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureCategory {
    Input,
    Output,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FunctionCaptureProps {
    pub inputs: CaptureOption,
    pub output: CaptureOption,
    pub error: CaptureOption,
}

impl FunctionCaptureProps {
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            inputs: CaptureOption::Disabled,
            output: CaptureOption::Disabled,
            error: CaptureOption::Disabled,
        }
    }

    #[must_use]
    pub const fn option(self, category: CaptureCategory) -> CaptureOption {
        match category {
            CaptureCategory::Input => self.inputs,
            CaptureCategory::Output => self.output,
            CaptureCategory::Error => self.error,
        }
    }

    #[must_use]
    pub const fn with_option(mut self, category: CaptureCategory, option: CaptureOption) -> Self {
        match category {
            CaptureCategory::Input => self.inputs = option,
            CaptureCategory::Output => self.output = option,
            CaptureCategory::Error => self.error = option,
        }
        self
    }

    #[must_use]
    pub const fn with_auto(self, category: CaptureCategory) -> Self {
        self.with_option(category, CaptureOption::Auto)
    }
}

/// Represents any Baml function.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Function {
    /// Function name.
    pub name: String,

    /// Source file path where this function is defined.
    ///
    /// Set at emit time for bytecode functions. Empty string for builtins and
    /// synthesized functions that have no source file.
    pub source_file: String,

    /// The declaration's joined `///` doc-comment lines, if any. Surfaced by
    /// runtime reflection (BEP-062 `reflect.signature`); `None` for lambdas
    /// without docs and synthesized functions.
    pub docstring: Option<String>,

    /// The name the function was declared with (`greet`, `bump`), recorded
    /// at lowering.
    /// `None` for callables that have no source-level name: lambdas and
    /// compiler-synthesized bodies, whose [`Self::name`] is a debug identity
    /// (`<lambda(...)>`), not a name. Surfaced by runtime reflection
    /// (BEP-062 `reflect.signature`); never inferred from [`Self::name`].
    pub declared_name: Option<String>,

    /// Number of arguments the function accepts.
    pub arity: usize,

    /// Number of additional local slots (beyond callee + params) needed by the frame.
    ///
    /// The VM allocates these slots when creating a bytecode frame, instead of
    /// relying on a dedicated bytecode instruction.
    pub real_local_count: usize,

    /// Bytecode to execute.
    ///
    /// Only relevant if [`Self::kind`] is [`FunctionKind::Bytecode`].
    pub bytecode: Bytecode,

    /// Type of function.
    pub kind: FunctionKind,

    /// Local variable names indexed by slot number.
    ///
    /// Debug info: maps eval-stack slot indices to variable names.
    /// Slot 0 is the function reference, slots 1..arity are parameters.
    pub local_names: Vec<String>,

    /// Lexical scope metadata for named locals.
    ///
    /// Used by debugger UIs to determine which variables are visible at a
    /// given source location.
    pub debug_locals: Vec<crate::bytecode::DebugLocalScope>,

    /// Span of the function as computed by the parser.
    pub span: baml_base::Span,

    /// Return type of the function, as a template over the callee frame's
    /// De Bruijn type-arg slots. A non-generic function's template is already
    /// realized; a generic one references the slots its callers seed, so a
    /// *value* of this function type reconstructs precisely by substituting the
    /// realized args it carries (see `bex_vm`'s `function_object_ty`).
    pub return_type: TyTemplate,

    /// Parameter names in declaration order.
    pub param_names: Vec<String>,

    /// Parameter types in declaration order, as templates over the callee
    /// frame's type-arg slots (see [`Self::return_type`]).
    pub param_types: Vec<TyTemplate>,

    /// Whether each parameter has a BAML default expression.
    pub param_has_default: Vec<bool>,

    /// Source/TIR-rendered generic parameters for documentation surfaces.
    ///
    /// `param_types` and `return_type` are precise but *positional*: a generic
    /// position is a [`TyTemplate::TypeArgRef`] that renders as its De Bruijn
    /// index, and a bound is not part of the type at all. Surfaces like
    /// `baml run --list` use these display fields to recover the written
    /// spelling — `<T extends BoxLike>(box: T) -> T.Item` rather than
    /// `(box: #0) -> #0.Item`.
    pub display_type_params: Vec<String>,

    /// Source/TIR-rendered parameter types in declaration order.
    pub display_param_types: Vec<String>,

    /// Source/TIR-rendered return type.
    pub display_return_type: String,

    /// Inferred throws type — the union of all types this function (and its
    /// callees) may throw, as a template over the callee frame's type-arg slots
    /// (see [`Self::return_type`]). Used by the engine to convert uncaught throw
    /// values to `BexExternalValue`.
    ///
    /// "Cannot throw" is [`TyTemplate::Never`] — the empty error set — which is
    /// also how a function *type* spells it statically, so a value's
    /// reconstructed signature and its written type agree. (`unknown` is the
    /// distinct "unannotated, no claim" case.)
    pub throws_type: TyTemplate,

    /// Provenance of this function in the compiler/runtime pipeline.
    pub origin: FunctionOrigin,

    /// LLM-specific metadata (prompt template, client name). `None` for non-LLM functions.
    pub body_meta: Option<FunctionMeta>,

    /// Capture policy hints for this function. Hosts resolve `Auto` against
    /// boundary defaults; ordinary functions default to disabled.
    pub capture: FunctionCaptureProps,

    /// Per-run profiling id (`0` = unassigned), written into the BEX event
    /// stream's `CallFunction` records and resolved through the per-run
    /// function table in the `.bamlprof` header. Assigned by the engine's
    /// interim provider at construction (plan §2.6); the M0 id table moves
    /// assignment to compile time. `#[borsh(skip)]` keeps it out of the pack
    /// envelope — it is runtime-only state, and skipping it leaves the wire
    /// format unchanged.
    #[borsh(skip)]
    pub function_id: u32,
}

impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<fn {}>", self.name)
    }
}

/// A closure: a function object paired with a list of captured variable cells.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Closure {
    /// Pointer to the underlying `Object::Function`.
    pub function: HeapPtr,
    /// Captured cells, one per closed-over variable (each is `Object::Cell`).
    pub captures: Box<[Value]>,
    /// Type arguments captured from the enclosing generic context at the time
    /// the closure is created by `MakeClosure`.
    ///
    /// Populated by the `MakeClosure { ntypeargs }` instruction which pops
    /// `ntypeargs` `Object::Type` values from the operand stack immediately
    /// before the cell captures.  These become `frame.type_args` when the
    /// closure is invoked, so that `LoadType(TypeArgRef(N))` inside the
    /// closure body resolves correctly.
    pub captured_type_args: Box<[baml_type::RealizedTy]>,
}

/// A method bound to a specific receiver instance.
///
/// Created by `MakeBoundMethod` (a statically-resolved method) or
/// `MakeVirtualBoundMethod` (an interface method resolved from the receiver's
/// runtime `Self` at bind time). The receiver is inserted as `self` at call time
/// by `CallIndirect`.
///
/// Like every callable value, a bound method is fully realized: its complete
/// type environment is curried in at creation via [`Self::type_args`], so the
/// `CallIndirect` that invokes it carries no type arguments of its own.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BoundMethod {
    /// Pointer to the underlying `Object::Function`.
    pub function: HeapPtr,
    /// The receiver value (inserted as `self` at call time).
    pub receiver: Value,
    /// The callee frame's **complete** curried type arguments, in the callee
    /// frame's De Bruijn order — materialized at bind time and installed as
    /// `frame.type_args` when the value is invoked by `CallIndirect`, so
    /// `LoadType(TypeArgRef(N))` / `IsType` inside the body resolve correctly.
    ///
    /// `MakeBoundMethod` curries `[class generics (→ Self), method fn generics]`
    /// — the exact vector a direct `receiver.method<…>(…)` call would seed.
    /// `MakeVirtualBoundMethod` instead curries the resolved impl's realized
    /// frame — the impl's own generics, or the interface's args + associated
    /// types for an inherited default (which the receiver's class args cannot
    /// express, e.g. a blanket `implement<T> I for T[]` bound at `int[]`) —
    /// followed by any method-level type args from the reference site.
    ///
    /// `RuntimeTy` (not `RealizedTy`) mirrors [`Closure::captured_type_args`]
    /// and [`GenericFunction::type_args`]: these positions should never carry a
    /// type variable, but the upstream fix that stops typevars leaking into
    /// value positions is still in flight, so all three stay `RuntimeTy` and
    /// narrow to `RealizedTy` together once it lands.
    pub type_args: Box<[baml_type::RealizedTy]>,
}

/// A generic function instantiation carrying concrete type arguments.
///
/// Unlike `Closure`/`BoundMethod`, the base function is referenced by its
/// **global slot** (`GlobalIndex`), not a `HeapPtr` — so a `GenericFunction`
/// can live in the immutable compile-time object pool and be interned by
/// `(function, type_args)`, giving pointer-stable identity. Both fields are
/// non-pointer data, so GC treats this as a leaf (nothing to trace or fix up).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct GenericFunction {
    /// Global slot of the underlying `Object::Function` (resolved at call time
    /// via the global table, mirroring `MakeBoundMethod`).
    pub function: crate::GlobalIndex,
    /// Concrete type arguments to seed into `frame.type_args` when called.
    pub type_args: Box<[baml_type::RealizedTy]>,
}

/// A host-language callable bound to a BAML function type.
///
/// Created at the FFI boundary when a `HostValue` is passed for a
/// `RuntimeTy::Function` parameter. Calling it (`CallIndirect`) dispatches
/// `SysOp::BamlHostCallHostValue`, which fires the bridge's
/// `HostDispatchFn` and awaits the host's response.
///
/// `Box<RuntimeTy>` keeps the `Object` enum within its `<= 64`-byte budget
/// (see the `size_of::<Object>()` assertion in `object.rs`).
#[derive(Clone, Debug)]
pub struct HostClosure {
    /// Opaque handle to the host-owned callable. `Drop` of the last clone
    /// fires the registered `HostReleaseFn`; see
    /// [`bex_resource_types::HostValueArc`].
    pub handle: std::sync::Arc<bex_resource_types::HostValueArc>,
    /// The declared return type of the host-callable, threaded through
    /// `SysOp::BamlHostCallHostValue` as `type_arg_0` so the sysop impl
    /// can validate the host's returned value against the BAML signature.
    pub ret_ty: Box<baml_type::RealizedTy>,
    /// The declared error/throws contract of the host-callable (`E` in
    /// `call_host_value<T, E>`), threaded through
    /// `SysOp::BamlHostCallHostValue` as `type_arg_1`. A host throw is
    /// checked against this contract. The FFI entry boundary (see
    /// `bex_engine::conversion`'s `HostValue` arm) normalizes an
    /// unbounded/undeclared generic throws to `RuntimeTy::BuiltinUnknown`, which
    /// accepts any thrown value — the "unknown" fallback. Concrete throws
    /// (e.g. `throws ParseError`) pass through unchanged so the contract
    /// check can reject off-type throws as `HostContractViolation`.
    pub throws_ty: Box<baml_type::RealizedTy>,
    /// Number of value arguments the host callable expects.
    ///
    /// `CallIndirect` reads this to drain the right number of operand slots
    /// off the eval stack — host closures don't wrap an `Object::Function`,
    /// so there is no `arity` field to read from there.
    pub arity: usize,
    /// The callable's declared parameters (names + required/optional mode),
    /// captured at bind time from the `RuntimeTy::Function`. When the VM
    /// dispatches the call (`host_closure_call_sysop`) it uses these to split
    /// the args into a positional list (required) and a name→value map (supplied
    /// optionals, omitted ones dropped), so each bridge can apply its calling
    /// convention (e.g. TypeScript's trailing `$opts`) without the callee type
    /// on the wire. `Box`-ed to keep `Object` within its size budget.
    pub params: Box<Vec<baml_type::RealizedFunctionParamTy>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum FunctionType {
    /// Top of function type lattice: represents all function types.
    Any,
    Callable,
    SysOp,
}

impl std::fmt::Display for FunctionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunctionType::Any => write!(f, "any"),
            FunctionType::Callable => write!(f, "callable"),
            FunctionType::SysOp => write!(f, "sys_op"),
        }
    }
}

impl From<&FunctionKind> for FunctionType {
    fn from(value: &FunctionKind) -> Self {
        if matches!(value, FunctionKind::SysOp(_)) {
            FunctionType::SysOp
        } else {
            FunctionType::Callable
        }
    }
}
