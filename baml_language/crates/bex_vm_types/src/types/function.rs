use borsh::{BorshDeserialize, BorshSerialize};

use super::InterfaceBound;
use crate::{Bytecode, HeapPtr, SysOp, TyTemplate, Value};

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

/// Represents any Baml function.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Function {
    /// Telemetry-only identity. None means unsupported in an executable object.
    /// Loaders assign IDs to supported compiler templates before execution;
    /// synthetic entry wrappers stay unsupported. Moving GC preserves identity.
    /// Serialized programs never carry runtime identities.
    #[borsh(skip)]
    pub telemetry_function_id: Option<btel_types::FunctionId>,

    /// Published once on first telemetry reference; moving GC preserves it.
    #[borsh(skip)]
    pub telemetry_registration: btel_types::FunctionRegistration,

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

    /// Current immutable/resolved telemetry policy. This runtime-only atomic
    /// is not serialized; loaded programs begin at policy zero and the engine
    /// may publish a newer policy while calls are in flight.
    #[borsh(skip)]
    pub telemetry_policy_id: btel_types::TelemetryPolicyId,

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

    /// Interface bounds for each De Bruijn type-argument slot.  Unlike
    /// `display_type_params`, this is executable metadata: the VM substitutes
    /// the actual call-frame types and rejects a failing bound before entering
    /// the function body.
    pub generic_param_bounds: Vec<Vec<InterfaceBound>>,

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

    /// True for interface-machinery bodies: impl-block methods (in-class or
    /// free) and interface default-method bodies.
    ///
    /// An interface body is pooled and slotted like any function —
    /// statically resolved
    /// calls stay direct `Call(GlobalIndex)` — but it is not itself a logical
    /// item, so it has no name anywhere: bodies are excluded from
    /// `Program::function_indices` / `function_global_indices` and every
    /// runtime name scan skips them; compile boundaries recover a body's
    /// coordinates structurally (Pass-1 slot replay + the globals array).
    /// [`Self::name`] on an interface body is display-only (traces,
    /// snapshots).
    pub is_interface_body: bool,

    /// Native-dispatch key for `$rust_function` bodies (stdlib-only): the key
    /// `attach_builtins` resolves through each package's generated
    /// `get_native_fn` table. `None` for every bytecode/sys-op function.
    ///
    /// This is deliberately separate from [`Self::name`]: the display string
    /// is not an identity, while this key must match the codegen-produced
    /// dispatch tables exactly.
    pub native_key: Option<Box<str>>,

    /// LLM-specific metadata (prompt template, client name). `None` for non-LLM functions.
    pub body_meta: Option<FunctionMeta>,

    /// Owning runtime package for dynamically grafted functions. Static
    /// functions use null and address operands through the engine image.
    #[borsh(skip)]
    pub runtime_package: HeapPtr,
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
    pub captured_type_args: Box<[crate::RealizedTy]>,
}

/// A method bound to a specific receiver instance.
///
/// Created by `MakeBoundMethod` (a statically-resolved method) or
/// `MakeVirtualBoundMethod` (an interface method resolved from the receiver's
/// runtime `Self` at bind time). The receiver is inserted as `self` at call time
/// by `CallIndirect`.
///
/// The type environment resolved when the method is bound is curried in via
/// [`Self::type_args`], so `CallIndirect` carries no separate type arguments.
/// A later explicit generic application appends its method type arguments
/// while retaining the receiver and the existing type environment.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BoundMethod {
    /// Pointer to the underlying `Object::Function`.
    pub function: HeapPtr,
    /// The receiver value (inserted as `self` at call time).
    pub receiver: Value,
    /// The callee frame's canonical curried type arguments, in the callee
    /// frame's De Bruijn order — materialized at bind time and installed as
    /// `frame.type_args` when the value is invoked by `CallIndirect`.
    ///
    /// `MakeBoundMethod` curries `[class generics (→ Self), method fn generics]`
    /// — the exact vector a direct `receiver.method<…>(…)` call would seed.
    /// `MakeVirtualBoundMethod` instead curries the resolved impl's realized
    /// frame — the impl's own generics for a provided method,
    /// `[Self, interface args..]` for an adopted default (which the receiver's
    /// class args cannot express, e.g. a blanket `implement<T> I for T[]`
    /// bound at `int[]`) —
    /// followed by any method-level type args from the reference site.
    ///
    /// `RuntimeTy` (not `RealizedTy`) mirrors [`Closure::captured_type_args`]
    /// and [`GenericFunction::type_args`]: these positions should never carry a
    /// type variable, but the upstream fix that stops typevars leaking into
    /// value positions is still in flight, so all three stay `RuntimeTy` and
    /// narrow to `RealizedTy` together once it lands.
    pub type_args: Box<[crate::RealizedTy]>,
}

/// A generic function instantiation carrying concrete type arguments.
///
/// Unlike `Closure`/`BoundMethod`, the base function is referenced by its
/// **global slot** (`GlobalIndex`), not a `HeapPtr` — so a `GenericFunction`
/// can live in the immutable compile-time object pool and be interned by
/// `(function, type_args)`, giving pointer-stable identity.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct GenericFunction {
    /// Global slot of the underlying `Object::Function` (resolved at call time
    /// via the global table, mirroring `MakeBoundMethod`).
    pub function: crate::GlobalIndex,
    /// Concrete type arguments to seed into `frame.type_args` when called.
    pub type_args: Box<[crate::RealizedTy]>,
    /// Owning runtime package for resolving `function` in its local globals.
    #[borsh(skip)]
    pub runtime_package: HeapPtr,
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
    pub ret_ty: Box<crate::RealizedTy>,
    /// The declared error/throws contract of the host-callable (`E` in
    /// `call_host_value<T, E>`), threaded through
    /// `SysOp::BamlHostCallHostValue` as `type_arg_1`. A host throw is
    /// checked against this contract. The FFI entry boundary (see
    /// `bex_engine::conversion`'s `HostValue` arm) normalizes an
    /// unbounded/undeclared generic throws to `RuntimeTy::Unknown`, which
    /// accepts any thrown value — the "unknown" fallback. Concrete throws
    /// (e.g. `throws ParseError`) pass through unchanged so the contract
    /// check can reject off-type throws as `HostContractViolation`.
    pub throws_ty: Box<crate::RealizedTy>,
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
    pub params: Box<Vec<baml_type::RealizedFunctionParamTy<crate::TypeHead>>>,
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

impl Function {
    /// The name of parameter `index` when it is optional. Synthesized
    /// functions may carry no parameter metadata; their slots are required.
    fn optional_param_name(&self, index: usize) -> Option<&str> {
        self.param_has_default
            .get(index)
            .copied()
            .unwrap_or(false)
            .then(|| self.param_names.get(index).map(String::as_str))
            .flatten()
    }

    /// The value slots the body reads, receiver included for a method.
    pub fn argument_layout(&self) -> baml_type::CallLayout {
        baml_type::CallLayout(
            (0..self.arity)
                .map(|index| self.optional_param_name(index).map(baml_type::Name::new))
                .collect(),
        )
    }

    /// Whether `layout` equals [`Self::argument_layout`], without building it.
    pub fn argument_layout_is(&self, layout: &baml_type::CallLayout) -> bool {
        layout.len() == self.arity
            && layout
                .0
                .iter()
                .enumerate()
                .all(|(index, slot)| slot.as_deref() == self.optional_param_name(index))
    }
}

impl Function {
    /// Copy metadata while the registered function is live and protected from GC.
    /// No runtime heap references escape in the returned value.
    pub fn runtime_metadata(&self) -> Option<btel_types::FunctionMetadata> {
        let function_id = self.telemetry_function_id?;
        let fqn = self.name.clone();
        let owner_type = derive_owner_type_definition_key(&fqn);
        let (parent_function, lambda_path) = derive_lambda_metadata(&fqn);
        let mut parts = fqn.split('.').map(str::to_string).collect::<Vec<_>>();
        let display_name = parts.last().cloned().unwrap_or_else(|| fqn.clone());
        let package_name = if parts.len() > 1 {
            Some(parts.remove(0))
        } else {
            None
        };
        let namespace = if parts.len() > 1 {
            parts[..parts.len() - 1].to_vec()
        } else {
            Vec::new()
        };
        let source_file = (!self.source_file.is_empty()).then(|| self.source_file.clone());
        let source_span = Some(btel_types::SourceSpan {
            file_id: self.span.file_id.as_u32(),
            start: self.span.range.start().into(),
            end: self.span.range.end().into(),
        });

        Some(btel_types::FunctionMetadata {
            function_id,
            fqn: fqn.clone(),
            display_name,
            source_file,
            source_span,
            kind: self.kind.into(),
            origin: self.origin.into(),
            owner_type,
            parent_function,
            lambda_path,
            definition_key: Some(btel_types::DefinitionKey(format!("function:{fqn}"))),
            package_name,
            namespace,
        })
    }
}

// TODO(bep-053): replace with compiler-owned metadata — capital-letter
// sniffing on FQN segments is an acknowledged-interim heuristic and must not
// become load-bearing.
fn derive_owner_type_definition_key(fqn: &str) -> Option<btel_types::DefinitionKey> {
    let parts = fqn.split('.').collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }

    let owner = parts[parts.len() - 2];
    if !owner
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        return None;
    }

    Some(btel_types::DefinitionKey(format!(
        "class:{}",
        parts[..parts.len() - 1].join(".")
    )))
}

// TODO(bep-053): replace with compiler-owned lambda identity — the
// `find("<lambda")` name sniff is an acknowledged-interim heuristic.
fn derive_lambda_metadata(fqn: &str) -> (Option<btel_types::DefinitionKey>, Option<String>) {
    let Some(lambda_start) = fqn.find("<lambda") else {
        return (None, None);
    };

    let parent = fqn[..lambda_start].trim_end_matches(['.', ':']).to_string();
    let parent_function =
        (!parent.is_empty()).then(|| btel_types::DefinitionKey(format!("function:{parent}")));
    let lambda_path = Some(fqn[lambda_start..].to_string());
    (parent_function, lambda_path)
}

impl From<crate::FunctionKind> for btel_types::RuntimeFunctionKind {
    fn from(value: crate::FunctionKind) -> Self {
        match value {
            crate::FunctionKind::Bytecode => Self::Bytecode,
            crate::FunctionKind::SysOp(op) => Self::SysOp(format!("{op:?}")),
            crate::FunctionKind::NativeUnresolved => Self::NativeUnresolved,
            crate::FunctionKind::Native(_) => Self::Native,
        }
    }
}

impl From<crate::FunctionOrigin> for btel_types::RuntimeFunctionOrigin {
    fn from(value: crate::FunctionOrigin) -> Self {
        match value {
            crate::FunctionOrigin::UserDefined => Self::UserDefined,
            crate::FunctionOrigin::Companion => Self::Companion,
            crate::FunctionOrigin::Internal => Self::Internal,
            crate::FunctionOrigin::Builtin => Self::Builtin,
            crate::FunctionOrigin::AutoDerive => Self::AutoDerive,
        }
    }
}
