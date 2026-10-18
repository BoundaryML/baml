use baml_base::Name;
use borsh::{BorshDeserialize, BorshSerialize};

use super::{BodyKey, FnPath, ImplBodyKey, InterfaceBound, ItemPath};
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

/// A natively compiled body for a [`FunctionKind::Bytecode`] function.
///
/// The BAML-to-Rust compiler emits one Rust function per admitted BAML
/// function; the loader attaches it here, and the VM's call path runs it in
/// place of the bytecode while leaving everything else about the function —
/// its kind, its telemetry identity, its frame, its exception handling — as it
/// is for bytecode. Type-erased for the same reason as
/// [`FunctionKind::Native`]: this crate cannot name the VM's body types.
/// Runtime-only; never serialized.
///
/// Built only through the `unsafe` [`NativeBody::new`], whose contract is what
/// makes calling the pointer sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeBody {
    ptr: *const (),
    resumable: bool,
    slots: u32,
    cause_slot: Option<u32>,
}

impl NativeBody {
    /// # Safety
    ///
    /// `ptr` must be cast from the `bex_vm::compiled_rt` body type `resumable`
    /// names (`ResumableBody` when set, `CompiledBody` otherwise); the VM casts
    /// it back to that type to call it.
    #[must_use]
    #[allow(unsafe_code)]
    pub const unsafe fn new(
        ptr: *const (),
        resumable: bool,
        slots: u32,
        cause_slot: Option<u32>,
    ) -> Self {
        Self {
            ptr,
            resumable,
            slots,
            cause_slot,
        }
    }

    /// The erased body.
    #[must_use]
    pub const fn ptr(self) -> *const () {
        self.ptr
    }

    /// Whether the body can suspend: it then runs as a resumable state
    /// machine driven by the VM instead of to completion inside a call.
    #[must_use]
    pub const fn resumable(self) -> bool {
        self.resumable
    }

    /// The stack slots the body keeps in its frame, from the frame's locals
    /// offset: a resumable body's arguments, the locals it keeps across a
    /// suspension and its resume point, then the cause slot.
    #[must_use]
    pub const fn slots(self) -> u32 {
        self.slots
    }

    /// The slot holding the `baml.errors.Context` of the error the body is
    /// handling (null outside a handler), which the cause chain reads; `None`
    /// for a body with no `catch`.
    #[must_use]
    pub const fn cause_slot(self) -> Option<u32> {
        self.cause_slot
    }
}

// SAFETY: a `NativeBody` points at immutable code (a function pointer), so
// sharing it between threads is safe, exactly as for `FunctionKind::Native`.
#[allow(unsafe_code)]
unsafe impl Send for NativeBody {}
#[allow(unsafe_code)]
unsafe impl Sync for NativeBody {}

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

/// What kind of model-facing function this is, when it is one.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum FunctionMeta {
    Llm {
        client: String,
    },
    /// An `ai.Client.invoke` implementation, or the step that settles a
    /// stream's turn: it returns an `ai.ModelTurn` whose own calls telemetry
    /// charges as model usage.
    ModelInvoke,
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

/// What a function IS, structurally: the identity every rendered name of it
/// derives from ([`Function::name`]). A function carries no baked name — a
/// declared one is its path in its package, a body its [`BodyKey`], a closure
/// body its parent and ordinal, an initializer its role — and the package it
/// belongs to is [`Function::owner`], so a name is always rendered from a
/// viewpoint (today, lossily, the owner's display label), never stored.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum FunctionIdentity {
    /// A declared free function or class-inherent method.
    Declared(FnPath),
    /// An interface-machinery body: an interface's default-method body or an
    /// `implements` block's provided method. Pooled and slotted like any
    /// function — a statically resolved call stays a direct `Call(GlobalIndex)`
    /// — but not a logical item: it enters no package name table, so no
    /// rendered view of the executable names it, and every runtime name scan
    /// skips it. Its rendering is display only (traces, snapshots).
    InterfaceBody(BodyKey),
    /// A closure body lowered inside `parent`: the `ordinal`-th of its `kind`
    /// there. Renders as `<lambda(parent, ordinal)>` or
    /// `<tagged(parent, ordinal)>` under an empty package segment, the
    /// spelling the bytecode display pins.
    Synthetic {
        parent: Box<SyntheticParent>,
        kind: SyntheticKind,
        ordinal: u32,
    },
    /// A package's `$init`: runs its top-level `let` initializers in
    /// dependency order.
    Init,
    /// The helper that evaluates a package's `ordinal`-th top-level `let`
    /// initializer, which `$init` calls before storing the result.
    InitLet(u32),
    /// A package's `$init_test` chainer.
    InitTest,
    /// A trampoline the VM synthesizes at run time to enter a callee from the
    /// host; `callee` is the callee's rendered name, display only. Never in
    /// an executable: `Program::validate` refuses one.
    Entry(Box<str>),
}

/// What a synthetic function was lowered from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum SyntheticKind {
    /// A lambda expression.
    Lambda,
    /// A tagged template's body closure.
    Tagged,
}

/// Where a synthetic function was lowered: a function (declared, body, or
/// itself synthetic), or a top-level `let` initializer, which is lowered as a
/// body and only ever OWNS synthetic functions.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum SyntheticParent {
    Function(FunctionIdentity),
    Let(ItemPath),
}

impl FunctionIdentity {
    /// A closure body's identity: the `ordinal`-th `kind` lowered in `parent`.
    #[must_use]
    pub fn synthetic(parent: SyntheticParent, kind: SyntheticKind, ordinal: u32) -> Self {
        Self::Synthetic {
            parent: Box::new(parent),
            kind,
            ordinal,
        }
    }

    /// The name the function was declared with (`greet`, `bump`); `None` for
    /// a callable with no source-level name — a closure body, an
    /// initializer, an entry trampoline.
    #[must_use]
    pub fn declared_name(&self) -> Option<&Name> {
        match self {
            Self::Declared(FnPath::Free(path)) => Some(&path.name),
            Self::Declared(FnPath::Method { name, .. }) => Some(name),
            Self::InterfaceBody(BodyKey::Default { method, .. }) => Some(method),
            Self::InterfaceBody(BodyKey::ImplMethod(body)) => Some(&body.method),
            Self::Synthetic { .. }
            | Self::Init
            | Self::InitLet(_)
            | Self::InitTest
            | Self::Entry(_) => None,
        }
    }

    /// Whether this is an interface-machinery body (see
    /// [`InterfaceBody`](Self::InterfaceBody)).
    #[must_use]
    pub fn is_interface_body(&self) -> bool {
        matches!(self, Self::InterfaceBody(_))
    }

    /// The namespace of the declaring package this function lives in, when
    /// its identity carries one: a declared function's or its owner type's.
    /// A provided impl body carries its coherence key, not a namespace.
    #[must_use]
    pub fn namespace(&self) -> &[Name] {
        match self {
            Self::Declared(FnPath::Free(path) | FnPath::Method { class: path, .. })
            | Self::InterfaceBody(BodyKey::Default {
                interface: path, ..
            }) => &path.namespace,
            Self::InterfaceBody(BodyKey::ImplMethod(_))
            | Self::Synthetic { .. }
            | Self::Init
            | Self::InitLet(_)
            | Self::InitTest
            | Self::Entry(_) => &[],
        }
    }

    /// The owner-qualified tail of the rendered name — everything after the
    /// package and namespace segments: `f` for a free function, `Class.m` for
    /// a method, `Interface.m` for a default body, `<(target as iface)>.m`
    /// for a provided one, `<lambda(parent, i)>` for a closure body (its
    /// parent by ITS tail), and the role name of an initializer or entry.
    /// The ONE spelling of a function's tail: [`Function::name`] prefixes it,
    /// and a nested synthetic names its parent by it, so the two cannot drift.
    /// `label` is the owning package's display label; an impl-provided body
    /// spells the heads of its own package by it (`<(user.X as user.I)>.m`),
    /// the same label the name is prefixed with, so one name reads from one
    /// viewpoint. `None` renders those heads as `root`.
    #[must_use]
    pub fn tail(&self, label: Option<&Name>) -> String {
        match self {
            Self::Declared(FnPath::Free(path)) => path.name.to_string(),
            Self::Declared(FnPath::Method { class, name }) => format!("{}.{name}", class.name),
            Self::InterfaceBody(BodyKey::Default { interface, method }) => {
                format!("{}.{method}", interface.name)
            }
            Self::InterfaceBody(BodyKey::ImplMethod(body)) => impl_body_tail(body, label),
            Self::Synthetic {
                parent,
                kind,
                ordinal,
            } => {
                let kind = match kind {
                    SyntheticKind::Lambda => "lambda",
                    SyntheticKind::Tagged => "tagged",
                };
                let parent = match parent.as_ref() {
                    SyntheticParent::Function(function) => function.tail(label),
                    SyntheticParent::Let(path) => path.name.to_string(),
                };
                format!("<{kind}({parent}, {ordinal})>")
            }
            Self::Init => "$init".to_string(),
            Self::InitLet(ordinal) => format!("$init_let_{ordinal}"),
            Self::InitTest => "$init_test".to_string(),
            Self::Entry(callee) => format!("$entry::{callee}"),
        }
    }

    /// The rendered name under a package labelled `label` (`None`: a pooled
    /// function before the loader placed it, or a closure body, which no
    /// package table lists): `label.ns.tail` for a declared function or body,
    /// `.tail` for a synthetic (an empty package segment), the bare tail for
    /// an initializer or entry.
    #[must_use]
    pub fn render(&self, label: Option<&Name>) -> String {
        match self {
            Self::Declared(_) | Self::InterfaceBody(_) => {
                let mut segments: Vec<&str> = Vec::new();
                if let Some(label) = label {
                    segments.push(label.as_str());
                }
                segments.extend(self.namespace().iter().map(Name::as_str));
                let tail = self.tail(label);
                segments.push(&tail);
                segments.join(".")
            }
            Self::Synthetic { .. } => format!(".{}", self.tail(label)),
            Self::Init | Self::InitLet(_) | Self::InitTest | Self::Entry(_) => self.tail(label),
        }
    }
}

/// `<(target as iface<args>)>.method` for a provided impl body. Every head is
/// located from the body's own package by edge path; the body's own package
/// is spelled by `label` (the owner's display label, as the name's prefix
/// is) and a dependency by the edges that reach it (`edge.ns.X`).
fn impl_body_tail(body: &ImplBodyKey, label: Option<&Name>) -> String {
    /// Spells a head's package segment verbatim — the label or the edge
    /// path — so no package folds away the way `TypeName`'s display elides a
    /// local one.
    struct BodySpelling<'a> {
        label: Option<&'a Name>,
    }
    impl BodySpelling<'_> {
        fn package(&self, path: &baml_type::wire::EdgePath) -> String {
            match self.label {
                Some(label) if path.is_own() => label.to_string(),
                _ => path.to_string(),
            }
        }
    }
    impl baml_type::TyRenderStrategy<baml_type::PathName> for BodySpelling<'_> {
        fn qtn(&self, head: &baml_type::PathName) -> String {
            std::iter::once(self.package(head.key()))
                .chain(head.namespace().iter().map(ToString::to_string))
                .chain(std::iter::once(head.name().to_string()))
                .collect::<Vec<_>>()
                .join(".")
        }

        fn type_var(&self, name: &Name) -> String {
            name.to_string()
        }
    }
    let spelling = BodySpelling { label };
    let display = |template: &baml_type::TyTemplate<baml_type::PathName>| {
        template.to_display_ty().render_with(&spelling)
    };
    let mut interface = format!(
        "{}.{}",
        spelling.package(&body.interface.package),
        body.interface.path
    );
    if !body.coherence.interface_args.is_empty() {
        let args: Vec<String> = body.coherence.interface_args.iter().map(display).collect();
        interface.push('<');
        interface.push_str(&args.join(", "));
        interface.push('>');
    }
    format!(
        "<({} as {interface})>.{}",
        display(&body.coherence.for_ty_pattern),
        body.method
    )
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

    /// What this function is ([`FunctionIdentity`]); its name is rendered
    /// from this and [`Self::owner`] by [`Self::name`], never stored.
    pub identity: FunctionIdentity,

    /// Source file path where this function is defined.
    ///
    /// Set at emit time for bytecode functions. Empty string for builtins and
    /// synthesized functions that have no source file.
    pub source_file: String,

    /// The declaration's joined `///` doc-comment lines, if any. Surfaced by
    /// runtime reflection (BEP-062 `reflect.signature`); `None` for lambdas
    /// without docs and synthesized functions.
    pub docstring: Option<String>,

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

    /// Natively compiled body, attached at load time to a
    /// [`FunctionKind::Bytecode`] function the BAML-to-Rust compiler admitted.
    /// When set, the call path runs it instead of [`Self::bytecode`]; the
    /// bytecode stays authoritative for everything else (and for execution
    /// when this is `None`). Runtime-only; `None` on the wire.
    #[borsh(skip)]
    pub native_body: Option<NativeBody>,

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

    /// The name of each type-argument slot in a call's frame
    /// (`frame.type_args`), in slot order: an owner's type parameters (an
    /// interface method's `Self` first), then the function's own; a lambda's
    /// are its enclosing frame's. Telemetry records a call's type arguments
    /// by these names.
    pub type_param_names: Vec<String>,

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

    /// Native-dispatch key for `$rust_function` bodies (stdlib-only): the key
    /// `attach_builtins` resolves through each package's generated
    /// `get_native_fn` table. `None` for every bytecode/sys-op function.
    ///
    /// This is deliberately separate from the rendered name: a display string
    /// is not an identity, while this key must match the codegen-produced
    /// dispatch tables exactly.
    pub native_key: Option<Box<str>>,

    /// LLM-specific metadata (prompt template, client name). `None` for non-LLM functions.
    pub body_meta: Option<FunctionMeta>,

    /// Compiler/stdlib forwarding entry: hold controls without an automatic span.
    pub trace_passthrough: bool,

    /// The package this function belongs to — the same edge a class carries
    /// to its package ([`super::Owner`]): bound by the loader for a static
    /// package's functions, set by the grafter for a runtime package's. A
    /// static function's owner is a compile-time pointer; a grafted
    /// function's is the runtime package it addresses its operands through
    /// (`BexVm::runtime_owner` tells the two apart). Every pooled function is
    /// owned by the package that placed it (`ProgramPackage::objects`);
    /// null only for the VM's own entry trampolines.
    #[borsh(skip)]
    pub owner: HeapPtr,
}

impl Function {
    /// The function's rendered name: [`FunctionIdentity::render`] under its
    /// owner package's display label (lossy display, as a class's
    /// [`name`](super::Class::name) is), or the identity alone when it has no
    /// owner.
    #[must_use]
    pub fn name(&self) -> String {
        self.identity
            .render(super::owner::package_label(self.owner).as_ref())
    }

    /// The name the function was declared with; see
    /// [`FunctionIdentity::declared_name`].
    #[must_use]
    pub fn declared_name(&self) -> Option<&Name> {
        self.identity.declared_name()
    }

    /// Whether this is an interface-machinery body; see
    /// [`FunctionIdentity::InterfaceBody`].
    #[must_use]
    pub fn is_interface_body(&self) -> bool {
        self.identity.is_interface_body()
    }
}

impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<fn {}>", self.name())
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
/// A later explicit generic application appends method arguments while
/// preserving the receiver and already-curried class/impl arguments.
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
    /// Names for the argument slots telemetry captures: `arity` slots, with a
    /// method's `self` receiver first. Only declaration names are used; `None`
    /// when they do not name every slot, e.g. synthesized bodies.
    pub fn telemetry_argument_layout(&self) -> Option<btel_types::ArgumentLayout> {
        (self.param_names.len() == self.arity).then(|| btel_types::ArgumentLayout {
            slots: self
                .param_names
                .iter()
                .map(|name| btel_types::ArgumentSlot {
                    receiver: name == "self",
                    name: Some(name.clone()),
                })
                .collect(),
        })
    }

    /// Copy metadata while the registered function is live and protected from GC.
    /// No runtime heap references escape in the returned value.
    pub fn runtime_metadata(&self) -> Option<btel_types::FunctionMetadata> {
        let function_id = self.telemetry_function_id?;
        let label = super::owner::package_label(self.owner);
        let fqn = self.identity.render(label.as_ref());
        let display_name = self
            .identity
            .declared_name()
            .map_or_else(|| self.identity.tail(label.as_ref()), ToString::to_string);
        let package_name = label.as_ref().map(ToString::to_string);
        let namespace: Vec<String> = self
            .identity
            .namespace()
            .iter()
            .map(ToString::to_string)
            .collect();
        // A method's owner type (its class; an interface default body's, its
        // interface), and a closure body's parent and path within it, read off
        // the identity rather than sniffed from a rendering. An impl-provided
        // body has no owner type: its for-target is a pattern, not a
        // declaration.
        let under_label = |path: &super::LocalName| match &label {
            Some(label) => path.under(label).to_string(),
            None => path.to_string(),
        };
        let owner_type = match &self.identity {
            FunctionIdentity::Declared(FnPath::Method { class, .. }) => Some(
                btel_types::DefinitionKey(format!("class:{}", under_label(class))),
            ),
            FunctionIdentity::InterfaceBody(BodyKey::Default { interface, .. }) => Some(
                btel_types::DefinitionKey(format!("interface:{}", under_label(interface))),
            ),
            FunctionIdentity::Declared(FnPath::Free(_))
            | FunctionIdentity::InterfaceBody(BodyKey::ImplMethod(_))
            | FunctionIdentity::Synthetic { .. }
            | FunctionIdentity::Init
            | FunctionIdentity::InitLet(_)
            | FunctionIdentity::InitTest
            | FunctionIdentity::Entry(_) => None,
        };
        let (parent_function, lambda_path) = match &self.identity {
            FunctionIdentity::Synthetic { parent, .. } => {
                let parent = match parent.as_ref() {
                    SyntheticParent::Function(function) => Some(btel_types::DefinitionKey(
                        format!("function:{}", function.render(label.as_ref())),
                    )),
                    SyntheticParent::Let(_) => None,
                };
                (parent, Some(self.identity.tail(label.as_ref())))
            }
            _ => (None, None),
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
            argument_layout: self.telemetry_argument_layout(),
            source_map: self.telemetry_source_map(),
            model_invoke: matches!(self.body_meta, Some(FunctionMeta::ModelInvoke)),
        })
    }

    /// The executed (compact) code's line table, in the byte-offset PCs that
    /// call paths and error evidence record. `None` before lowering and for
    /// functions without bytecode. Out-of-range values saturate, and the
    /// reader rejects them.
    pub fn telemetry_source_map(&self) -> Option<btel_types::SourceMap> {
        if !matches!(self.kind, FunctionKind::Bytecode) {
            return None;
        }
        let compact = self.bytecode.compact.as_ref()?;
        let clamp = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
        Some(btel_types::SourceMap {
            code_bytes: clamp(compact.code.len()),
            entries: compact
                .line_table
                .iter()
                .map(|entry| btel_types::SourceMapEntry {
                    pc: clamp(entry.pc),
                    file_id: entry.span.file_id.as_u32(),
                    start: entry.span.range.start().into(),
                    end: entry.span.range.end().into(),
                    line: clamp(entry.line),
                })
                .collect(),
        })
    }
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
