//! The body type-diagnostic vocabulary: error kinds, arena-ID-anchored
//! diagnostics, and their span resolution - rust-analyzer's
//! `InferenceDiagnostic` discipline (the vocabulary lives with the
//! inference crate; renderers downstream consume it).
//!
//! Relocated verbatim from `baml_compiler2_tir::infer_context` during
//! S17: every payload is engine-neutral (shared plain `Ty`, `Name`,
//! AST ids), so both engines emit the same shapes and the whole message
//! stack downstream is shared. TIR re-exports these under its old paths
//! until its deletion.
//!
//! Diagnostics are Salsa-stable (no `TextRange`) - locations are stored
//! as arena IDs. The LSP layer maps them to source ranges at display
//! time.

use std::fmt;

use baml_base::{FileId, Name, SourceFile};
use baml_compiler2_ast::{AstSourceMap, ExprId, StmtId, TypeAnnotId};
use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, FunctionLoc},
};
use baml_type::{QualifiedTypeName, Ty};
use text_size::TextRange;

/// The syntactic context an irrefutable-pattern rule fires in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrrefutableContextKind {
    Let,
    ForLet,
}

impl IrrefutableContextKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Let => "let",
            Self::ForLet => "for-let",
        }
    }
}

// ── Error kinds ──────────────────────────────────────────────────────────────

/// The subject an associated-type projection failed to resolve `member` on —
/// the "container" named by [`TirTypeError::UnknownAssociatedType`].
///
/// A projection is nominal: `member` must be declared by an interface reachable
/// from the subject. Each variant is a subject kind with its own reason the
/// member can be unknown — an interface that doesn't declare it, a concrete
/// type none of whose impls provide it, or a type variable / projected type
/// with no interface bound to search (an unbounded subject cannot be proven to
/// implement any interface, so no interface can declare the member).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssocContainer {
    /// An interface subject (an existential base, a type variable's bound, or
    /// an explicit `(base as I)` qualifier) that does not declare the member.
    Interface(QualifiedTypeName),
    /// A concrete class none of whose impls' interfaces declare the member.
    Class(QualifiedTypeName),
    /// A concrete enum none of whose impls' interfaces declare the member.
    Enum(QualifiedTypeName),
    /// A type variable with no interface bound in scope.
    TypeVar(Name),
    /// Any other subject type (a primitive, list, map, or a projected type with
    /// no declared bound), rendered user-facing.
    Ty(Ty),
}

impl std::fmt::Display for AssocContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interface(qtn) => write!(f, "interface `{}`", qtn.render_user_facing()),
            Self::Class(qtn) => write!(f, "class `{}`", qtn.render_user_facing()),
            Self::Enum(qtn) => write!(f, "enum `{}`", qtn.render_user_facing()),
            Self::TypeVar(name) => write!(f, "type variable `{name}` (no interface bound)"),
            Self::Ty(ty) => write!(f, "type `{}`", ty.render_user_facing()),
        }
    }
}

/// Where a disallowed `Self` appears in a method called through an
/// interface-existential receiver — see [`TirTypeError::InvalidSelfCallThroughInterface`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfCallPosition {
    /// A non-receiver parameter typed with `Self`.
    Parameter,
    /// `Self` nested inside an invariant constructor in the return or throws type
    /// (e.g. `-> Self[]`, `-> Box<Self>`); a bare top-level `-> Self` is allowed.
    NestedInReturn,
}

/// The kind of enclosing declaration whose type-level parameter a method's
/// generic shadows — names the declaration accurately in the
/// [`TirTypeError::TypeParamShadowed`] message ("class `X`" / "interface `X`").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowedParamOwner {
    Class,
    Interface,
}

impl fmt::Display for ShadowedParamOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShadowedParamOwner::Class => write!(f, "class"),
            ShadowedParamOwner::Interface => write!(f, "interface"),
        }
    }
}

/// What went wrong — no location info, just the semantic error.
///
/// `TirTypeError` is intentionally span-free for Salsa cacheability.
/// Each error is paired with a primary `ExprId` in `TirDiagnostic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TirTypeError {
    /// Type mismatch: expected vs actual.
    TypeMismatch { expected: Ty, got: Ty },
    /// Member not found on a known type.
    ///
    /// Reported when the base type IS resolved (known class/enum) but the
    /// member doesn't exist. Error messages are tailored by base type:
    /// - Class: "Class `X` has no member `y`"
    /// - Enum: "Enum `X` has no variant `y`"
    UnresolvedMember { base_type: Ty, member: Name },
    /// A member (method or field) accessed on a union type is not reachable because
    /// the union's arms implement no *common* interface that declares it. Each arm may
    /// declare it independently (via distinct interfaces), but those are different
    /// members — the union has no single agreed-upon one.
    UnionMemberNoCommonInterface { union: Ty, member: Name },
    /// Name could not be resolved at all.
    UnresolvedName { name: Name },
    /// A value name was written bare in a generic slot. Runtime-computed
    /// slots require the whole-slot `unreflect(value)` marker.
    ComputedGenericArgumentRequiresUnreflect { name: Name },
    /// A mounted callable whose implementation is compiler-owned and has no
    /// location-free link ABI was invoked from a source-less consumer.
    MountedPackageCallUnsupported { path: Name },
    /// A shorthand property (`{ name }`) could not resolve its implicit value.
    /// Suggestions are in-scope values with similar names; the diagnostic
    /// renders them as explicit `name: suggestion` mappings.
    UnresolvedPropertyShorthand { name: Name, suggestions: Vec<Name> },
    /// A class constructor shorthand property resolves as a value but its name
    /// is not an exact class-field match.
    UnknownClassPropertyShorthand {
        class_name: QualifiedTypeName,
        name: Name,
        suggestions: Vec<Name>,
    },
    /// A class constructor explicitly names a field the class does not declare.
    UnknownClassField {
        class_name: QualifiedTypeName,
        field_name: Name,
        suggestions: Vec<Name>,
    },
    /// Sealed reflection-kind values are VM views and cannot be constructed
    /// with an object literal.
    CannotConstructReflectionKind {
        class_name: baml_type::QualifiedTypeName,
    },
    /// Unreachable code after a diverging statement (return/break/continue).
    DeadCode {
        after: StmtId,
        unreachable_count: usize,
    },
    /// A `void` expression (e.g. `if` without `else`) was used where a value
    /// is required — assigned to a variable, passed as an argument, or returned.
    VoidUsedAsValue,
    /// The return value of a void-returning function was used where a value
    /// is required — assigned to a variable, passed as an argument, etc.
    VoidFunctionResultUsed,
    /// A `spawn ... with` clause expression is not a middleware transformer
    /// (BEP-034: each `with` expression must be a function
    /// `(baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<U, F>`).
    SpawnWithNotATransformer { expected_input: Ty, got: Ty },
    /// Expression is not callable (e.g. `42(1)` or `Foo(1)` where Foo is a class).
    NotCallable { ty: Ty },
    /// Expression is not iterable (e.g. `for let i in 42 { ... }` where 42 is an int).
    NotIterable { ty: Ty },
    /// Expression is not indexable (e.g. `true[0]`).
    NotIndexable { ty: Ty },
    /// A class destructuring pattern names a field the class does not declare.
    UnknownClassPatternField {
        class_name: QualifiedTypeName,
        field_name: Name,
        suggestions: Vec<Name>,
    },
    /// A `map` type expression whose key type is not `string` (e.g. `map<int, V>`).
    /// Map keys are strings at runtime, so the key type must denote `string` or a
    /// subset of it.
    InvalidMapKeyType { key: Ty },
    /// Invalid operand types for a binary operator (e.g. `true + false`).
    InvalidBinaryOp {
        op: baml_compiler2_ast::BinaryOp,
        lhs: Ty,
        rhs: Ty,
    },
    /// Ordering (`<` `<=` `>` `>=`) between two different types. Ordering is exact-type:
    /// both operands must have the same type (subtyping is not enough — only `==` spans
    /// types), so this fires even when one operand is a subtype of the other.
    OrderingDifferentTypes {
        op: baml_compiler2_ast::BinaryOp,
        lhs: Ty,
        rhs: Ty,
    },
    /// Ordering (`<` `<=` `>` `>=`) on a common type that does not implement
    /// `baml.ops.Compare`, so no ordering is defined for it.
    OrderingRequiresCompare {
        op: baml_compiler2_ast::BinaryOp,
        ty: Ty,
    },
    /// An equality (`==` / `!=`) whose operand types are provably disjoint — no
    /// value of one can ever equal a value of the other — so the result is a
    /// constant (`==` always `false`, `!=` always `true`). A warning, not an
    /// error: the comparison is valid, just pointless.
    ComparisonAlwaysDisjoint {
        op: baml_compiler2_ast::BinaryOp,
        lhs: Ty,
        rhs: Ty,
    },
    /// Invalid operand type for a unary operator (e.g. `-"hello"`).
    InvalidUnaryOp {
        op: baml_compiler2_ast::UnaryOp,
        operand: Ty,
    },
    /// A type name in a type annotation could not be resolved.
    UnresolvedType {
        name: Name,
        /// "Did you mean" candidates, each a fully qualified `root.…` path.
        suggestions: Box<[Name]>,
    },
    /// An associated-type projection's explicit `as X` qualifier resolved to a
    /// non-interface type (a class, alias, etc.). The qualifier must name an
    /// interface; without one the projection cannot be resolved, so it must not
    /// silently fall back to an unqualified projection.
    NonInterfaceProjectionQualifier,
    /// A type-inference placeholder `_` (a `TypeExprKind::Infer` wildcard) could not have its
    /// type inferred — inference for `_` is unavailable, so the type must be written explicitly.
    /// Lowered to `Ty::Error` so it never reaches the canonical normalizer, which treats
    /// `Ty::Infer` as `unreachable!`.
    CannotInferType,
    /// An associated-type projection references `member`, but the subject it
    /// projects through does not declare (or cannot declare) it as an
    /// associated type.
    ///
    /// Interfaces do not inherit associated types through `requires` (it is a
    /// bound, not inheritance), so a member declared on a *required* interface
    /// must be projected through that interface directly: `(Foo as Iterator).Item`,
    /// not `(Foo as RequiresIterator).Item`.
    UnknownAssociatedType {
        member: Name,
        container: AssocContainer,
    },
    /// An unqualified associated-type projection `Base.member` matches more than
    /// one interface that declares `member`; it must be disambiguated with an
    /// explicit `(Base as Interface).member` qualifier.
    AmbiguousAssociatedTypeProjection {
        member: Name,
        candidates: Vec<QualifiedTypeName>,
    },
    /// Wrong number of arguments in a function call.
    ArgumentCountMismatch { expected: usize, got: usize },
    /// A positional argument appeared after a named argument in the same call.
    PositionalArgumentAfterNamed,
    /// A named argument was supplied more than once.
    DuplicateNamedArgument { name: Name },
    /// A call supplied a named argument that is not present in the callable type.
    UnknownNamedArgument { name: Name },
    /// A defaulted parameter was supplied positionally instead of by name.
    DefaultedParamPassedPositionally { name: Name },
    /// A required parameter was omitted.
    MissingRequiredArgument { name: Name },
    /// A required parameter appeared after a defaulted parameter in a declaration.
    RequiredParamAfterDefault { name: Name },
    /// The special `self` receiver cannot declare a default.
    SelfParamDefault,
    /// A default expression referenced a later parameter from the same signature.
    DefaultParamForwardReference { param: Name, referenced: Name },
    /// Function body ends without returning a value.
    MissingReturn { expected: Ty },
    /// Type alias participates in an invalid (unguarded) cycle.
    ///
    /// Examples: `type A = A`, `type A = B; type B = A`.
    /// Valid recursion through containers (`type JSON = string | JSON[]`) does NOT
    /// trigger this — only cycles with no base case.
    AliasCycle { name: Name },
    /// Class participates in an unconstructable required-field cycle.
    ///
    /// Examples: `class A { b B }; class B { a A }`.
    /// Cycles through optional, list, or map fields are valid since those can
    /// be null/empty, breaking the construction dependency.
    ClassCycle { name: Name, cycle_path: String },
    /// `match` is missing arms for one or more values.
    NonExhaustiveMatch {
        scrutinee_type: Ty,
        missing_cases: Vec<String>,
    },
    /// `catch_all` is missing arms for one or more caught throw types.
    NonExhaustiveCatchAll {
        caught_type: Ty,
        missing_cases: Vec<String>,
    },
    /// A `match`/`catch` arm can never execute because previous arms are exhaustive.
    UnreachableArm,
    /// Or-pattern alternatives bind the same name with conflicting narrow
    /// types. HIR already ensures the *names* line up across branches; this
    /// is the type-level counterpart.
    OrPatternBindingTypeMismatch {
        name: Name,
        first_type: Ty,
        other_type: Ty,
    },
    /// A generic class destructure with fields must write its type arguments
    /// directly on the class pattern, e.g. `Box<int> { value }`.
    GenericClassDestructureRequiresTypeArgs { class_name: Name },
    /// A rest pattern (`..`) carries a sub-pattern that is not binding-shaped.
    /// Allowed: `..let r`, `.._`, bind chains, and a terminal `: T` ascription
    /// link. Rejected: bare type patterns, structural destructures, and
    /// or-patterns — see `lower_array_pat` for why each is blocked and what
    /// expanding the set would take.
    RestSubPatternNotBinding,
    /// A `let` statement or `for-let` binding uses a pattern that can fail
    /// for values of the type flowing into it.
    RefutablePatternInLet { context: IrrefutableContextKind },
    /// An `if let` pattern that covers every value of the scrutinee — the
    /// `else` branch is unreachable. Suggests using a plain `let` instead.
    IrrefutablePatternInIfLet,
    /// `let … else { … }` whose else block does not have type `Ty::Never`.
    /// The else branch must diverge — return, throw, break, continue, or
    /// loop forever — so that fall-through past the binding cannot occur.
    LetElseMustDiverge { got: Ty },
    /// A `let … else` pattern that covers every value of the initializer
    /// type — the else branch is unreachable. Suggest using a plain `let`.
    IrrefutablePatternInLetElse,
    /// A `while let` pattern that covers every value of the scrutinee — the
    /// loop never exits via pattern failure (an unconditional infinite loop).
    /// Suggest a plain `while`/`loop` instead.
    IrrefutablePatternInWhileLet,
    /// `return`/`break`/`continue` inside a `defer` body that would escape the
    /// defer (BEP-042). Only `throw` may leave a defer. `keyword` is the
    /// offending control-flow keyword.
    DeferControlFlowEscape { keyword: &'static str },
    /// Catch binding cannot be typed as `any` or `unknown`.
    InvalidCatchBindingType { type_name: String },
    /// Inferred escaping throws are not covered by the declared throws contract.
    ThrowsContractViolation {
        declared: Ty,
        extra_types: Vec<String>,
    },
    /// Inferred escaping throws are explainable through a single callback path.
    CallbackThrowsContractViolation {
        callback_name: Name,
        declared: Ty,
        concrete_throws: Option<Ty>,
    },
    /// Declared throws contains extra types that never escape.
    ExtraneousThrowsDeclaration { extra_types: Vec<String> },
    /// A type parameter could not be inferred at a call site.
    CannotInferTypeParameter { name: Name },
    /// A method's generic type parameter shadows a type-level parameter (generic
    /// parameter or associated type) of the enclosing class or interface.
    TypeParamShadowed {
        param_name: Name,
        type_name: Name,
        owner: ShadowedParamOwner,
    },
    /// A method's generic type parameter shadows a generic parameter declared on
    /// the enclosing `implements` block.
    TypeParamShadowedImplParam { param_name: Name },
    /// A generic type parameter declared more than once in the same parameter list.
    DuplicateGenericParam { name: Name },
    /// An associated type declared more than once on the same interface.
    DuplicateAssociatedType { name: Name },
    /// An associated type sharing its name with one of the interface's own
    /// generic parameters.
    AssociatedTypeConflictsWithGenericParam { name: Name },
    /// Wrong number of type arguments for a generic class or interface.
    WrongNumberOfTypeArgs {
        type_name: Name,
        expected: usize,
        got: usize,
    },
    /// Wrong number of explicit type arguments at a function call site.
    ///
    /// E.g. `f<int>(x)` when `f` declares zero type params, or
    /// `f<int, string>(x)` when `f<T>` declares only one.
    WrongTypeArgArity {
        callee_name: Name,
        expected: usize,
        got: usize,
    },
    /// Runtime type arguments cannot enter the generated streaming
    /// specialization path.
    RuntimeTypeArgumentOnStreamingCall { callee_name: Name },
    /// Indirect call opcodes have no runtime-type-check operand, so allowing
    /// one would either panic during debug emission or skip the check in
    /// release builds.
    RuntimeTypeArgumentOnIndirectCall,
    /// Type arguments were supplied for a type that is not generic
    /// (enums and type aliases cannot take type parameters).
    TypeIsNotGeneric { type_name: Name, kind: &'static str },
    /// A generic function was referenced as a value without specialization
    /// (`let f = identity` where `identity<T>`). A generic function is a type
    /// constructor — it must be specialized (`identity<int>`) or have its type
    /// arguments inferable from context before it becomes a usable value.
    GenericFunctionValueNotSpecialized { name: Name },
    /// A lambda parameter has no type annotation and no expected type context
    /// to infer the type from.
    CannotInferLambdaParamType { param_name: Name },
    /// `?.` used on a non-nullable type (e.g. `a?.b` where `a` is not nullable).
    UnnecessaryOptionalChaining {
        /// The full expression text (e.g. `a?.b`)
        expr: String,
        /// The base expression text (e.g. `a`)
        base: String,
    },
    /// `??` used where the left operand is non-nullable.
    UnnecessaryNullCoalesce {
        /// LHS expression text
        lhs: String,
        /// Full expression text (e.g. `a ?? b`)
        expr: String,
    },
    /// `||` used where `??` was likely intended (nullable LHS).
    SuggestNullCoalesce {
        /// LHS expression text
        lhs: String,
        /// RHS expression text
        rhs: String,
    },
    /// `?? null` is a no-op.
    NullCoalesceWithNull {
        /// LHS expression text
        lhs: String,
    },
    /// Member access (`.field` or `[index]`) on a nullable type without `?.`.
    /// Occurs when parentheses break an optional chain: `(a?.b).c`.
    NullableMemberAccess {
        /// The base expression text (e.g. `a`)
        base: String,
        /// The member being accessed (e.g. `.name`)
        member: String,
        /// The full expression text (e.g. `a.name`)
        expr: String,
    },
    /// BEP-049 §10: a backtick template was attached to a tag that resolves to
    /// something that isn't a function (so it can't be a tagged-string tag).
    TaggedTagNotAFunction { name: Name },
    /// BEP-049 §10: the tag resolves to a function, but it lacks the
    /// `//baml:tagged_string` marker that makes it usable as a tagged-template tag.
    TaggedTagNotMarked { name: Name },
    /// BEP-049 §10: a `//baml:tagged_string` tag's first parameter is not a
    /// well-formed `body: (...) -> baml.TaggedString` (missing / wrong name /
    /// not a lambda / lambda return type isn't `baml.TaggedString`).
    TaggedTagBadBodyParam { name: Name },
    /// BEP-049 §11: an untagged `${expr}` interpolates a nullable value. The
    /// implicit `.to_string()` can't run on a possibly-null value; the user
    /// must coalesce (`${x ?? "…"}`) or unwrap first.
    InterpolatedValueMaybeNull { ty: Ty },
    /// BEP-049 §11: an untagged `${expr}` interpolates a value whose type has
    /// no `to_string` method, so it can't be implicitly stringified.
    TypeNotInterpolatable { ty: Ty },

    /// BEP-044 §"Method Disambiguation": an unqualified call resolves to
    /// a method declared by two or more interfaces — the receiver carries
    /// no information to pick one. `sources` lists every contributing
    /// interface as a namespace-qualified display string (e.g. `zoo.Animal`)
    /// so colliding same-simple-name interfaces from different namespaces are
    /// distinguishable and the suggested `as<…>` fix actually compiles.
    AmbiguousInterfaceMethod {
        class_name: Name,
        method_name: Name,
        sources: Vec<String>,
    },

    /// BEP-044 interface fields live in per-interface namespaces. A bare
    /// field access is ambiguous when multiple implemented interfaces provide
    /// the same field name and the class does not shadow it with an own field.
    AmbiguousInterfaceField {
        class_name: Name,
        field_name: Name,
        sources: Vec<Name>,
    },

    /// A concrete-typed receiver tried to access an interface field name that
    /// is only available after projecting to the interface view.
    InterfaceFieldRequiresProjection {
        class_name: Name,
        field_name: Name,
        interface_name: Name,
    },

    /// Interface-qualified field keys such as `Animal.name` are not class
    /// constructor fields. Interface fields are satisfied by class-owned fields
    /// or explicit `field as class_field` links.
    InterfaceFieldRequiresQualifiedConstruction {
        field_name: Name,
        qualified_name: Name,
    },

    /// `.as<T>` is an interface projection/upcast; the target must be an
    /// interface type.
    InvalidInterfaceUpcastTarget { target: Ty },

    /// Interface members are instance/view members, not static members on the
    /// interface type. Call through an interface-typed value or `.as<I>`.
    InterfaceMemberRequiresReceiver {
        interface_name: Name,
        member_name: Name,
    },

    /// An interface-existential (or union) receiver cannot call a method that uses
    /// `Self` outside the receiver position: a non-receiver `Self` parameter (the
    /// concrete implementor is unknown for those arguments), or `Self` nested inside
    /// an invariant constructor in the return/throws type (`-> Self[]`, `-> Box<Self>`
    /// — the impl returns a concretely-tagged container that is NOT a subtype of the
    /// existential-tagged one; containers are invariant). A bare top-level `-> Self`
    /// is fine (it collapses covariantly to the receiver). Rust `dyn Trait` parity.
    InvalidSelfCallThroughInterface {
        interface_name: Name,
        method_name: Name,
        position: SelfCallPosition,
    },

    /// BEP-044 §"default keyword scoping rules": `default.method()` on a
    /// required method (no default body) is a compile error.
    DefaultOnRequiredMethod {
        interface_name: Name,
        method_name: Name,
    },
    /// BEP-044: bare `default` (not `default.method(...)`) used as a value.
    /// `default` is only meaningful in call position.
    BareDefaultKeyword,
    /// BEP-044: `value.as<I>` where the concrete `value`'s type does not
    /// implement interface `I`. A clearer form of the generic type-mismatch.
    TypeDoesNotImplementInterface {
        value_type: Ty,
        /// The full interface type (with any generic args), so the diagnostic
        /// names `Cargo<int>` rather than the bare `Cargo`.
        interface: Ty,
    },
    /// BEP-044: a value almost satisfies an interface via a blanket impl, but a
    /// generic bound (`T extends Bound`) is not met. Names the failed bound.
    BlanketBoundNotSatisfied { value_type: Ty, bound: Ty },
    /// `$id` cannot be the target of a compound assignment (`$id += ...`):
    /// the runtime ID can only be replaced wholesale with an override from
    /// `baml.id.new()` via `$id = ...`.
    RuntimeIdCompoundAssignment,
    /// Member access on `$id` (e.g. `$id.len()`). `$id` reads as a plain
    /// string value but is not a binding; bind it to a local first.
    RuntimeIdMemberAccess { member: Name },
    /// A second `$id` side channel was supplied to one call.
    DuplicateRuntimeIdArgument,
    /// `$id` is trailing call metadata and an ordinary argument followed it.
    RuntimeIdArgumentMustBeLast,
    /// The `$id` side channel accepts only a `boundary.LocalId`.
    RuntimeIdArgumentTypeMismatch { got: Ty },
    /// An integer literal (or a constant-folded integer expression) is outside
    /// the representable `int` range `[-2^62, 2^62-1]`. `int` is 63-bit; larger
    /// magnitudes need a `bigint` literal (`n` suffix).
    IntegerLiteralOutOfRange { value: i64 },
    /// BEP-044: a generic parameter's bound (`<T extends X>`) resolved to a
    /// concrete non-interface type. Generic bounds must be interfaces.
    GenericBoundNotInterface { bound: Ty },
    /// BEP-062: an `implements` block targets a compiler-builtin interface
    /// (`baml.AnyFunction`), whose conformance is derived by the compiler and
    /// cannot be written by hand.
    BuiltinInterfaceNotImplementable { interface: QualifiedTypeName },
    /// BEP-062: a generic parameter's bound names a compiler-builtin interface
    /// (`baml.AnyFunction`) that is only legal as a value type (an
    /// existential), never as a bound.
    BuiltinInterfaceNotABound { interface: QualifiedTypeName },
    /// [`TYPE_SYSTEM.md` § Generics on Functions](TYPE_SYSTEM.md#generics-on-functions):
    /// an interface-bounded type parameter
    /// (`<T extends I>`) was given a non-concrete type argument (a union,
    /// interface-existential, literal, `unknown`, …). Only a concrete type has a
    /// single run-time representation, so virtual dispatch on the parameter is
    /// well-defined; a `T = A | B` would let `a.method(b)` dispatch on two
    /// different concrete types at once. Names the argument and the interface bound
    /// (a conjunction of interfaces — a non-interface bound is a separate
    /// `GenericBoundNotInterface` error, never this one).
    BoundedTypeArgNotConcrete {
        arg: Ty,
        bound: Box<[baml_type::Interface]>,
    },
    /// [`TYPE_SYSTEM.md` § Generics on Functions](TYPE_SYSTEM.md#generics-on-functions):
    /// an interface used as an **existential type** (a value's type — a parameter,
    /// return, field, or annotation) must specify every associated type, like Rust's
    /// `dyn Iterator<Item = …>` (E0191). Only associated types with a declared default
    /// may be omitted. An unpinned existential (`Iterator` instead of
    /// `Iterator<Item = int>`) is otherwise ill-formed: `Iterator` and
    /// `Iterator<Item = int>` would be mutually-incomparable types and membership would
    /// be vacuous. (Interface *bounds* — `<T extends Iterator>` — do NOT require this;
    /// they are not existentials.) Names the interface and the unpinned associated types.
    MissingAssociatedTypeBindings {
        interface: QualifiedTypeName,
        missing: Vec<Name>,
    },
    /// The dotted projection shorthand (`Base.Member`) was written with the
    /// interface itself as the base (`Iterator.Element`). The base of a
    /// projection is an *implementor* — a concrete type
    /// (`ArrayIterator.Element`), a bounded type variable (`T.Element`), or
    /// `Self` inside a body — never the interface: an associated type is
    /// defined per `(interface, implementor, member)` triple, so the
    /// interface alone does not determine it (Rust's E0223). Naming the
    /// interface explicitly takes a qualified projection
    /// (`(Base as Iterator).Element`).
    InterfaceProjectionBase {
        interface: QualifiedTypeName,
        member: Name,
    },
    /// A *bare* interface destructure pattern (`Source { value }`, no written generic
    /// args or associated bindings) adopts its associated-type bindings from the
    /// scrutinee — so the scrutinee must determine them uniquely. A scrutinee admitting
    /// two distinct realizations of the pattern's interface
    /// (`Source<Item = int> | Source<Item = string>`) is ambiguous: the pattern must
    /// write the bindings explicitly. (A *type* pattern never infers — it is an
    /// ordinary type and pins everything, per [`Self::MissingAssociatedTypeBindings`].)
    AmbiguousInterfacePatternBindings {
        interface: QualifiedTypeName,
        candidates: Vec<Ty>,
    },
    /// An `implements` block does not provide a body for a method the interface
    /// declares as required (no default). Impl conformance (E0113).
    MissingInterfaceMethod {
        interface: QualifiedTypeName,
        method: Name,
    },
    /// A `$rust_io_function` (sys-op) method inside an `implements` block has
    /// method-level generic parameters. An impl-block method is reached only
    /// through interface (virtual) dispatch, which does not reconstruct the
    /// synthetic type-argument slots a generic sys-op's glue reads off the
    /// stack — so such a call would fail at runtime. (A generic sys-op declared
    /// directly on a class is fine: it lowers to a direct `SysOp` instruction,
    /// which does supply those slots.) Impl conformance.
    GenericSysOpMethodInInterfaceImpl {
        interface: QualifiedTypeName,
        method: Name,
    },
    /// An interface that declares fields is implemented out-of-body
    /// (`implement I for T`). A field-bearing interface can only be implemented in the
    /// class body, where its fields are satisfied by the class's own fields (E0126).
    OutOfBodyImplementsFieldInterface { interface: QualifiedTypeName },
    /// An `implements` block provides a method the interface neither requires nor
    /// declares as a default — so it overrides nothing. Impl conformance (E0115).
    UnknownInterfaceMember {
        interface: QualifiedTypeName,
        member: Name,
    },
    /// An interface field is not satisfied by any class field — neither a same-named
    /// field nor an explicit `field as class_field` link. Impl conformance (E0124).
    MissingInterfaceField {
        interface: QualifiedTypeName,
        field: Name,
    },
    /// The class field satisfying an interface field has an incompatible type — field types
    /// are invariant, so they must be equivalent. Impl conformance (E0116).
    InterfaceFieldTypeMismatch {
        interface: QualifiedTypeName,
        field: Name,
        /// The interface's declared field type (realized at the impl's interface args).
        expected: Ty,
        /// The satisfying class field's type.
        got: Ty,
    },
    /// An override's signature is not a subtype of the interface's declared signature —
    /// args/kwargs are contravariant, return/throws covariant. Impl conformance (E0120).
    InterfaceMethodSignatureMismatch {
        interface: QualifiedTypeName,
        method: Name,
        /// The interface's declared signature (realized at the impl's interface args).
        expected: Ty,
        /// The override's signature.
        got: Ty,
    },
    /// An override declares a generic bound on one of its type parameters that the interface
    /// method does not require — the implementation has stricter requirements than the
    /// interface (Rust's E0276), so a caller satisfying the interface could be rejected.
    InterfaceMethodAddsGenericBound {
        interface: QualifiedTypeName,
        method: Name,
        param: Name,
        bound: baml_type::Interface,
    },
    /// The impl's target type does not implement an interface the implemented interface
    /// `requires` — implementing `I` requires also implementing each of `I`'s parents.
    /// `required` is the *realized* obligation (generic args and associated pins included),
    /// so a type that implements the parent at a different binding reads as what's missing
    /// (`Parent<Item = int>`, not a bare `Parent` it does implement). Impl conformance (E0125).
    MissingRequiredInterface {
        interface: QualifiedTypeName,
        required: baml_type::Interface,
    },
    /// An `implements` head names a type that is not an interface (a class, enum, or alias).
    /// Impl header (E0119).
    ImplTargetNotInterface { name: Name },
    /// An out-of-body impl's `for` target is not a single concrete impl subject — a union,
    /// optional, interface (`dyn`), literal, `unknown`, function, `Future`, … Impl header (E0138).
    ImplTargetNotConcrete { target: Ty },
    /// An impl declares a generic parameter that its `for` type and interface arguments do not
    /// determine, so it can never be inferred at a use site. Impl header (E0135).
    UnconstrainedImplTypeParam { name: Name },
    /// An out-of-body impl of a *foreign* interface is not anchored on a local type — the
    /// RFC-2451 covered rule (BEP-044). Impl header (E0139).
    ImplViolatesOrphanRule {
        interface: QualifiedTypeName,
        /// The uncovered type parameter appearing before any local type, if that is the
        /// failure; `None` when no local type appears anywhere in the impl's inputs.
        uncovered_param: Option<Name>,
    },
    /// The left side of a `field as class_field` link does not name a field of the
    /// implemented interface. Field-link well-formedness (E0128).
    UnknownInterfaceFieldLink {
        interface: QualifiedTypeName,
        field: Name,
    },
    /// The right side of a `field as class_field` link does not name a field of the
    /// class. Field-link well-formedness (E0129).
    UnknownClassFieldInInterfaceLink {
        class: Name,
        interface: QualifiedTypeName,
        field: Name,
    },
    /// The same interface field is linked by more than one `field as class_field` link in
    /// one `implements` block. Field-link well-formedness (E0130).
    DuplicateInterfaceFieldLink {
        interface: QualifiedTypeName,
        field: Name,
    },
    /// A `type Name = …` binding in an `implements` block names an associated type the
    /// interface does not declare. Assoc-binding hygiene.
    UnknownAssociatedTypeBinding {
        interface: QualifiedTypeName,
        name: Name,
    },
    /// An `implements` block binds the same associated type more than once. Assoc-binding
    /// hygiene.
    DuplicateAssociatedTypeBinding {
        interface: QualifiedTypeName,
        name: Name,
    },
    /// An `implements` block does not bind an associated type the interface declares with no
    /// default, so it is left undetermined. Assoc-binding hygiene. (Distinct from
    /// [`Self::MissingAssociatedTypeBindings`], which is the existential *value*-position rule.)
    MissingImplAssociatedTypeBinding {
        interface: QualifiedTypeName,
        name: Name,
    },
    /// Associated type bindings were written on an `implements` *target* (`implements
    /// I<Item = …>`) instead of inside the block (`type Item = …`). Assoc-binding hygiene.
    AssociatedTypeBindingsOnImplementsTarget { interface: QualifiedTypeName },
    /// An `implements` block binds an associated type to a type that does not implement the
    /// interface's declared bound for it (`type Item extends J`) — a bound is an *implements*
    /// relation, like a generic bound. Assoc-binding hygiene.
    AssociatedTypeBindingViolatesBound {
        interface: QualifiedTypeName,
        name: Name,
        /// The bound type the binding fails to implement.
        binding: Ty,
        bound: baml_type::Interface,
    },
    /// `Self` appears in an interface *field* type. `Self` is only meaningful in method
    /// signatures; a recursive field must name the interface itself. Interface-declaration well-formedness (E0136).
    SelfInInterfaceField {
        interface: QualifiedTypeName,
        field: Name,
    },
    /// Bare `Self` appears in an associated type's *default*. `Self` is universal — it
    /// denotes each implementor, not the existential — so at an interface-existential
    /// type (`I<…>`, where the implementor is hidden) such a default has nothing to
    /// resolve against. Defaulting it to the existential itself would pin the member to
    /// a type no impl ever binds, making the existential uninhabited. A `Self.Assoc`
    /// projection is fine: the existential's own pins already fix it.
    /// Interface-declaration well-formedness (E0157).
    SelfInAssociatedTypeDefault {
        interface: QualifiedTypeName,
        associated_type: Name,
    },
    /// An interface's `requires` clause names a type that is not an interface (a class, enum,
    /// alias, or any structural type — the clause parses a full type expression). Only
    /// interfaces can be required, exactly as only interfaces can be generic bounds (see
    /// [`Self::GenericBoundNotInterface`]). Interface-declaration well-formedness (E0133).
    InterfaceRequiresNonInterface {
        interface: QualifiedTypeName,
        target: Ty,
    },
    /// An interface's transitive `requires` graph cycles back to itself. Interface-declaration well-formedness (E0118).
    /// `chain` is the witnessing name path `[root, …, root]`.
    InterfaceRequiresCycle { chain: Vec<Name> },
    /// An interface associated type's default does not implement its declared bound (`type Item
    /// extends J = V` where `V` does not implement `J`). Interface-declaration well-formedness.
    AssociatedTypeDefaultViolatesBound {
        interface: QualifiedTypeName,
        name: Name,
        /// The default type that fails to implement the bound.
        default: Ty,
        bound: baml_type::Interface,
    },
    /// An impl header contains a concrete associated-type projection (`implement I for C.Item`, or
    /// an interface argument `I<X = C.Item>`) whose resolution enumerates this very impl set, so it
    /// cycles. The `impl_data` cycle fallback can't carry a diagnostic, so it is re-detected and
    /// reported here. Impl header.
    CyclicImplHeader,
    /// An interface method (required or default) omits its `throws` clause. Interface signatures
    /// must declare it explicitly — it is never inferred (`TYPE_SYSTEM.md` rule 1). Interface-declaration well-formedness.
    InterfaceMethodMissingThrows {
        interface: QualifiedTypeName,
        method: Name,
    },
    /// A function type omits its `throws` clause in a position where the error type cannot be
    /// inferred — a type alias, class field, `let` annotation, nested or return position
    /// (`TYPE_SYSTEM.md` rule 5). Only an immediate callback parameter of a function declaration
    /// may omit it; there the compiler opens it to a synthetic effect parameter (rule 4). Lambda
    /// parameters have no generic binder to open an effect parameter on, so they must declare it
    /// explicitly.
    FunctionTypeMissingThrows,
    /// A top-level declaration's initializer reaches an io sysop. Top-level
    /// declarations (`client Foo = …`, `let x = …`) are evaluated by the
    /// synthesized `$init` chainer when the engine is created, on a path that
    /// cannot suspend — so the io does not fail with a catchable BAML error,
    /// it kills engine construction with an opaque `InitFailed`. Detected by
    /// [`crate::init_io`] (E0158).
    InitIoNotAllowed {
        /// The declaration's name.
        declaration: Name,
        /// `client Foo = …` (true) versus a plain top-level `let` (false).
        /// Only affects wording.
        is_client: bool,
        /// Fully-qualified name of the io sysop reached (`baml.env.get`).
        sysop: Name,
        /// The first call hop from the initializer toward `sysop`. `None` when
        /// the initializer calls the sysop directly.
        via: Option<Name>,
    },
}

impl fmt::Display for TirTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TirTypeError::TypeMismatch { expected, got } => {
                write!(
                    f,
                    "type mismatch: expected {}, got {}",
                    expected.render_user_facing(),
                    got.render_user_facing()
                )
            }
            TirTypeError::UnresolvedMember { base_type, member } => {
                if matches!(base_type, Ty::BuiltinUnknown { .. }) {
                    write!(f, "cannot access field `{member}` on `unknown`")
                } else {
                    write!(
                        f,
                        "type `{}` has no member `{member}`",
                        base_type.render_user_facing()
                    )
                }
            }
            TirTypeError::UnionMemberNoCommonInterface { union, member } => {
                write!(
                    f,
                    "type `{}` has no member `{member}`: its members implement no common \
                     interface that declares `{member}`",
                    union.render_user_facing()
                )
            }
            TirTypeError::UnresolvedName { name } => {
                write!(f, "unresolved name: {name}")
            }
            TirTypeError::ComputedGenericArgumentRequiresUnreflect { name } => {
                let diagnostic =
                    baml_compiler_diagnostics::runtime_type::computed_generic_argument_requires_unreflect(
                        name.as_str(),
                    );
                f.write_str(diagnostic.message.as_str())
            }
            TirTypeError::UnresolvedPropertyShorthand { name, suggestions } => {
                if suggestions.is_empty() {
                    write!(
                        f,
                        "property shorthand `{name}` requires an in-scope value named `{name}`"
                    )
                } else if suggestions.len() == 1 {
                    write!(
                        f,
                        "property shorthand `{name}` requires an in-scope value named `{name}`. \
                         Did you mean `{name}: {}`?",
                        suggestions[0]
                    )
                } else {
                    let joined = suggestions
                        .iter()
                        .map(|suggestion| format!("{name}: {suggestion}"))
                        .collect::<Vec<_>>()
                        .join("`, `");
                    write!(
                        f,
                        "property shorthand `{name}` requires an in-scope value named `{name}`. \
                         Did you mean one of these: `{joined}`?"
                    )
                }
            }
            TirTypeError::UnknownClassPropertyShorthand {
                class_name,
                name,
                suggestions,
            } => {
                if suggestions.is_empty() {
                    write!(
                        f,
                        "property shorthand `{name}` requires class `{}` to have a field named \
                         `{name}`",
                        class_name.render_user_facing()
                    )
                } else if suggestions.len() == 1 {
                    write!(
                        f,
                        "class `{}` has no field `{name}` for property shorthand. Did you mean \
                         `{}: {name}`?",
                        class_name.render_user_facing(),
                        suggestions[0]
                    )
                } else {
                    let joined = suggestions
                        .iter()
                        .map(|field| format!("{field}: {name}"))
                        .collect::<Vec<_>>()
                        .join("`, `");
                    write!(
                        f,
                        "class `{}` has no field `{name}` for property shorthand. Did you mean \
                         one of these: `{joined}`?",
                        class_name.render_user_facing()
                    )
                }
            }
            TirTypeError::CannotConstructReflectionKind { class_name } => {
                let diagnostic =
                    baml_compiler_diagnostics::runtime_type::cannot_construct_reflection_kind(
                        &class_name.render_user_facing(),
                    );
                f.write_str(diagnostic.message.as_str())
            }
            TirTypeError::MountedPackageCallUnsupported { path } => {
                let diagnostic =
                    baml_compiler_diagnostics::runtime_type::mounted_package_call_unsupported(
                        path.as_str(),
                    );
                f.write_str(diagnostic.message.as_str())
            }
            TirTypeError::DeadCode {
                unreachable_count, ..
            } => {
                write!(
                    f,
                    "unreachable code: {unreachable_count} statement(s) after diverging statement"
                )
            }
            TirTypeError::VoidUsedAsValue => {
                write!(
                    f,
                    "`if` without `else` cannot be used as a value; add an `else` branch"
                )
            }
            TirTypeError::VoidFunctionResultUsed => {
                write!(f, "cannot use return value of a void function")
            }
            TirTypeError::SpawnWithNotATransformer {
                expected_input,
                got,
            } => {
                write!(
                    f,
                    "`spawn ... with` takes middleware transformer functions: this link receives `{}` and must return a `baml.spawn.SpawnParams`, got `{}`",
                    expected_input.render_user_facing(),
                    got.render_user_facing()
                )
            }
            TirTypeError::NotCallable { ty } => {
                write!(
                    f,
                    "`{}` is not a function — it cannot be called",
                    ty.render_user_facing()
                )
            }
            TirTypeError::NotIterable { ty } => {
                write!(f, "cannot iterate over type `{}`", ty.render_user_facing())
            }
            TirTypeError::NotIndexable { ty } => {
                write!(f, "type `{}` is not indexable", ty.render_user_facing())
            }
            TirTypeError::UnknownClassField {
                class_name,
                field_name,
                suggestions,
            }
            | TirTypeError::UnknownClassPatternField {
                class_name,
                field_name,
                suggestions,
            } => {
                if suggestions.is_empty() {
                    write!(
                        f,
                        "class `{}` has no field `{field_name}`",
                        class_name.render_user_facing()
                    )
                } else if suggestions.len() == 1 {
                    write!(
                        f,
                        "class `{}` has no field `{field_name}`. Did you mean `{}`?",
                        class_name.render_user_facing(),
                        suggestions[0]
                    )
                } else {
                    let joined = suggestions
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("`, `");
                    write!(
                        f,
                        "class `{}` has no field `{field_name}`. Did you mean one of these: \
                         `{joined}`?",
                        class_name.render_user_facing()
                    )
                }
            }
            TirTypeError::InvalidMapKeyType { key } => {
                write!(
                    f,
                    "map keys must be `string`; got `{}`. Declare the map as `map<string, V>`; \
                     convert non-string keys with `.to_string()` before `.set()` or `.get()`",
                    key.render_user_facing()
                )
            }
            TirTypeError::InvalidBinaryOp { op, lhs, rhs } => {
                write!(
                    f,
                    "operator `{op}` cannot be applied to `{}` and `{}`",
                    lhs.render_user_facing(),
                    rhs.render_user_facing()
                )
            }
            TirTypeError::OrderingDifferentTypes { op, lhs, rhs } => {
                write!(
                    f,
                    "cannot order `{}` and `{}` with `{op}`: ordering requires both operands \
                     to have the same type",
                    lhs.render_user_facing(),
                    rhs.render_user_facing()
                )
            }
            TirTypeError::OrderingRequiresCompare { op, ty } => {
                write!(
                    f,
                    "`{}` does not implement `Compare`, so it cannot be ordered with `{op}`",
                    ty.render_user_facing()
                )
            }
            TirTypeError::ComparisonAlwaysDisjoint { op, lhs, rhs } => {
                let always = if matches!(op, baml_compiler2_ast::BinaryOp::Ne) {
                    "true"
                } else {
                    "false"
                };
                write!(
                    f,
                    "`{}` and `{}` share no value, so this comparison is always {always}",
                    lhs.render_user_facing(),
                    rhs.render_user_facing()
                )
            }
            TirTypeError::InvalidUnaryOp { op, operand } => {
                write!(
                    f,
                    "operator `{op}` cannot be applied to `{}`",
                    operand.render_user_facing()
                )
            }
            TirTypeError::NonInterfaceProjectionQualifier => {
                write!(
                    f,
                    "qualified associated type projection must use an interface"
                )
            }
            TirTypeError::CannotInferType => {
                write!(f, "type inference failed; write the type explicitly")
            }
            TirTypeError::UnknownAssociatedType { member, container } => {
                write!(f, "unknown associated type `{member}` for {container}")
            }
            TirTypeError::AmbiguousAssociatedTypeProjection { member, candidates } => {
                let names = candidates
                    .iter()
                    .map(|c| format!("`{}`", c.render_user_facing()))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "ambiguous associated type `{member}`: declared by multiple interfaces \
                     ({names}); qualify the projection with `(... as Interface).{member}`"
                )
            }
            TirTypeError::UnresolvedType { name, suggestions } => {
                if suggestions.is_empty() {
                    write!(f, "unresolved type: {name}")
                } else if suggestions.len() == 1 {
                    write!(
                        f,
                        "unresolved type: {name}. Did you mean `{}`?",
                        suggestions[0]
                    )
                } else {
                    let joined = suggestions
                        .iter()
                        .map(Name::as_str)
                        .collect::<Vec<_>>()
                        .join("`, `");
                    write!(
                        f,
                        "unresolved type: {name}. Did you mean one of these: `{joined}`?"
                    )
                }
            }
            TirTypeError::ArgumentCountMismatch { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            TirTypeError::PositionalArgumentAfterNamed => {
                write!(
                    f,
                    "positional arguments cannot appear after named arguments"
                )
            }
            TirTypeError::DuplicateNamedArgument { name } => {
                write!(f, "duplicate named argument `{name}`")
            }
            TirTypeError::UnknownNamedArgument { name } => {
                write!(f, "unknown named argument `{name}`")
            }
            TirTypeError::DefaultedParamPassedPositionally { name } => {
                write!(f, "defaulted parameter `{name}` must be passed by name")
            }
            TirTypeError::MissingRequiredArgument { name } => {
                write!(f, "missing required argument `{name}`")
            }
            TirTypeError::RequiredParamAfterDefault { name } => {
                write!(
                    f,
                    "required parameter `{name}` cannot appear after a defaulted parameter"
                )
            }
            TirTypeError::SelfParamDefault => {
                write!(f, "`self` cannot have a default value")
            }
            TirTypeError::DefaultParamForwardReference { param, referenced } => {
                write!(
                    f,
                    "default for parameter `{param}` cannot reference later parameter `{referenced}`"
                )
            }
            TirTypeError::MissingReturn { expected } => {
                write!(
                    f,
                    "missing return: expected `{}`",
                    expected.render_user_facing()
                )
            }
            TirTypeError::AliasCycle { name } => {
                write!(f, "recursive type alias cycle: {name}")
            }
            TirTypeError::ClassCycle { cycle_path, .. } => {
                write!(f, "class cycle: {cycle_path}")
            }
            TirTypeError::NonExhaustiveMatch {
                scrutinee_type,
                missing_cases,
            } => {
                write!(
                    f,
                    "non-exhaustive match on `{}`; missing: {}",
                    scrutinee_type.render_user_facing(),
                    missing_cases.join(", ")
                )
            }
            TirTypeError::NonExhaustiveCatchAll {
                caught_type,
                missing_cases,
            } => {
                write!(
                    f,
                    "non-exhaustive catch_all on `{}`; missing: {}",
                    caught_type.render_user_facing(),
                    missing_cases.join(", ")
                )
            }
            TirTypeError::UnreachableArm => write!(f, "unreachable arm"),
            TirTypeError::OrPatternBindingTypeMismatch {
                name,
                first_type,
                other_type,
            } => write!(
                f,
                "Or-pattern alternatives bind `{}` with conflicting types: `{}` vs `{}`",
                name,
                first_type.render_user_facing(),
                other_type.render_user_facing()
            ),
            TirTypeError::GenericClassDestructureRequiresTypeArgs { class_name } => write!(
                f,
                "generic class destructure `{class_name} {{ ... }}` must specify type arguments"
            ),
            TirTypeError::RestSubPatternNotBinding => write!(
                f,
                "rest pattern `..` can only carry a binding; write `..let name` or `..let name: T[]`"
            ),
            TirTypeError::RefutablePatternInLet { context } => write!(
                f,
                "refutable pattern in {} binding; refutable patterns belong in `match`",
                context.as_str()
            ),
            TirTypeError::IrrefutablePatternInIfLet => write!(
                f,
                "irrefutable `if let` pattern; the `else` branch is unreachable — use a plain `let` binding instead"
            ),
            TirTypeError::LetElseMustDiverge { got } => write!(
                f,
                "`let … else` requires a diverging else block (`return`, `throw`, `break`, or `continue`); got `{}`",
                got.render_user_facing()
            ),
            TirTypeError::IrrefutablePatternInLetElse => write!(
                f,
                "irrefutable `let … else` pattern; the `else` branch is unreachable — use a plain `let` binding instead"
            ),
            TirTypeError::IrrefutablePatternInWhileLet => write!(
                f,
                "irrefutable `while let` pattern; the loop never exits by pattern failure — use a plain `while`/`loop` instead"
            ),
            TirTypeError::DeferControlFlowEscape { keyword } => write!(
                f,
                "`{keyword}` cannot leave a `defer` body; only `throw` may propagate out of a defer"
            ),
            TirTypeError::InvalidCatchBindingType { type_name } => write!(
                f,
                "invalid catch binding type `{type_name}`; use a concrete type instead"
            ),
            TirTypeError::ThrowsContractViolation {
                declared,
                extra_types,
            } => {
                write!(
                    f,
                    "declared throws is `{}`, but this function may also throw `{}`",
                    declared.render_user_facing(),
                    extra_types.join(" | ")
                )
            }
            TirTypeError::CallbackThrowsContractViolation {
                callback_name,
                declared,
                concrete_throws,
            } => {
                write!(
                    f,
                    "this body may throw through callback `{callback_name}`, but declared throws is `{}`. ",
                    declared.render_user_facing()
                )?;
                if let Some(concrete_throws) = concrete_throws {
                    write!(
                        f,
                        "Add `throws {}` to the callback, catch the call, or make the callback non-throwing.",
                        concrete_throws.render_user_facing()
                    )
                } else {
                    write!(
                        f,
                        "The callback type does not say what it can throw. If `{callback_name}` is an infallible host callback, annotate it with `throws never`; otherwise catch the call or let the enclosing function declare/propagate the callback's throws."
                    )
                }
            }
            TirTypeError::ExtraneousThrowsDeclaration { extra_types } => {
                write!(
                    f,
                    "extraneous throws declaration: {}",
                    extra_types.join(", ")
                )
            }
            TirTypeError::CannotInferTypeParameter { name } => {
                write!(f, "cannot infer type parameter `{name}`")
            }
            TirTypeError::WrongNumberOfTypeArgs {
                type_name,
                expected,
                got,
            } => {
                write!(
                    f,
                    "type `{type_name}` expects {expected} type argument(s), got {got}"
                )
            }
            TirTypeError::WrongTypeArgArity {
                callee_name,
                expected,
                got,
            } => {
                write!(
                    f,
                    "function `{callee_name}` expects {expected} type argument(s), got {got}"
                )
            }
            TirTypeError::RuntimeTypeArgumentOnStreamingCall { callee_name } => {
                write!(
                    f,
                    "runtime type arguments are not supported on streaming call `{callee_name}`"
                )
            }
            TirTypeError::RuntimeTypeArgumentOnIndirectCall => {
                let diagnostic =
                    baml_compiler_diagnostics::runtime_type::runtime_type_argument_on_indirect_call(
                    );
                f.write_str(diagnostic.message.as_str())
            }
            TirTypeError::TypeIsNotGeneric { type_name, kind } => {
                write!(
                    f,
                    "{kind} `{type_name}` is not generic and cannot take type arguments"
                )
            }
            TirTypeError::GenericFunctionValueNotSpecialized { name } => {
                write!(
                    f,
                    "generic function `{name}` must be specialized before it is used \
                    as a value (e.g. `{name}<int>`)"
                )
            }
            TirTypeError::TypeParamShadowed {
                param_name,
                type_name,
                owner,
            } => {
                write!(
                    f,
                    "type parameter `{param_name}` on method shadows the same parameter on {owner} `{type_name}`. \
                    Please use a different name for the type parameter."
                )
            }
            TirTypeError::TypeParamShadowedImplParam { param_name } => {
                write!(
                    f,
                    "type parameter `{param_name}` on method shadows the same parameter on the enclosing `implements` block. \
                    Please use a different name for the type parameter."
                )
            }
            TirTypeError::DuplicateGenericParam { name } => {
                write!(
                    f,
                    "generic type parameter `{name}` is declared more than once"
                )
            }
            TirTypeError::DuplicateAssociatedType { name } => {
                write!(
                    f,
                    "associated type `{name}` is declared more than once on this interface"
                )
            }
            TirTypeError::AssociatedTypeConflictsWithGenericParam { name } => {
                write!(
                    f,
                    "associated type `{name}` conflicts with the interface's generic parameter of the same name. \
                    Please use a different name for one of them."
                )
            }
            TirTypeError::CannotInferLambdaParamType { param_name } => {
                write!(
                    f,
                    "cannot infer type of lambda parameter `{param_name}` — add a type annotation or provide context"
                )
            }
            TirTypeError::UnnecessaryOptionalChaining { expr, base } => {
                // e.g. "did you mean `a.b`? `a?.b` is unnecessary, because `a` cannot be null"
                // Build the rewrite from the base/suffix boundary so we strip the
                // diagnosed `?.`, not the first one in the string (which may be
                // inside a nested sub-expression like `foo(bar?.baz)?.qux`).
                let suffix = &expr[base.len()..];
                let dotted = if let Some(rest) = suffix.strip_prefix("?.(") {
                    format!("{base}({rest}")
                } else if let Some(rest) = suffix.strip_prefix("?.[") {
                    format!("{base}[{rest}")
                } else if let Some(rest) = suffix.strip_prefix("?.") {
                    format!("{base}.{rest}")
                } else {
                    // Fallback: should not happen, but be safe
                    expr.replacen("?.", ".", 1)
                };
                write!(
                    f,
                    "did you mean `{dotted}`? `{expr}` is unnecessary, because `{base}` cannot be null"
                )
            }
            TirTypeError::UnnecessaryNullCoalesce { lhs, expr } => {
                // e.g. "did you mean `a`? `a ?? b` is unnecessary, because `a` cannot be null"
                write!(
                    f,
                    "did you mean `{lhs}`? `{expr}` is unnecessary, because `{lhs}` cannot be null"
                )
            }
            TirTypeError::SuggestNullCoalesce { lhs, rhs } => {
                // e.g. "did you mean `a ?? b`? BAML uses `??` instead of `||` for null coalescing"
                write!(
                    f,
                    "did you mean `{lhs} ?? {rhs}`? BAML uses `??` instead of `||` for null coalescing"
                )
            }
            TirTypeError::NullCoalesceWithNull { lhs } => {
                // e.g. "did you mean `a`? `... ?? null` is unnecessary because `a` is already nullable"
                write!(
                    f,
                    "did you mean `{lhs}`? `... ?? null` is unnecessary because `{lhs}` is already nullable"
                )
            }
            TirTypeError::NullableMemberAccess { base, member, expr } => {
                // member is ".name" or "[...]" — construct suggestion by inserting ?
                // e.g. base="a", member=".name" → "a?.name"
                // e.g. base="a", member="[...]" → "a?.[...]"
                let suggested = if member.starts_with('.') {
                    format!("{base}?{member}")
                } else {
                    format!("{base}?.{member}")
                };
                write!(
                    f,
                    "did you mean `{suggested}`? `{expr}` does not handle the case when `{base}` is null"
                )
            }
            TirTypeError::TaggedTagNotAFunction { name } => write!(
                f,
                "`{name}` is not a function — a tagged-template tag must be a function marked `//baml:tagged_string`"
            ),
            TirTypeError::TaggedTagNotMarked { name } => write!(
                f,
                "`{name}` is not a tagged-string function — only functions marked `//baml:tagged_string` can be used as a tagged-template tag"
            ),
            TirTypeError::TaggedTagBadBodyParam { name } => write!(
                f,
                "the first parameter of tagged-string function `{name}` must be `body: (...) -> baml.TaggedString`"
            ),
            TirTypeError::InterpolatedValueMaybeNull { ty } => write!(
                f,
                "cannot interpolate a value of type `{}` — it may be null; coalesce with `?? \"…\"` or unwrap it first",
                ty.render_user_facing()
            ),
            TirTypeError::TypeNotInterpolatable { ty } => write!(
                f,
                "cannot interpolate a value of type `{}` — it has no `to_string` method",
                ty.render_user_facing()
            ),
            TirTypeError::AmbiguousInterfaceMethod {
                class_name,
                method_name,
                sources,
            } => {
                let iface_list = sources
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let hint = sources
                    .iter()
                    .map(|n| format!("obj.as<{n}>.{method_name}()"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                write!(
                    f,
                    "method `{method_name}` on class `{class_name}` is declared by \
                     multiple interfaces: {iface_list}; unqualified calls will be \
                    ambiguous — use {hint}"
                )
            }
            TirTypeError::AmbiguousInterfaceField {
                class_name,
                field_name,
                sources,
            } => {
                let iface_list = sources
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let hint = sources
                    .iter()
                    .map(|n| format!("obj.as<{n}>.{field_name}"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                write!(
                    f,
                    "field `{field_name}` on class `{class_name}` is ambiguous because it is declared by multiple interfaces: {iface_list}; use {hint}"
                )
            }
            TirTypeError::InterfaceFieldRequiresProjection {
                class_name,
                field_name,
                interface_name,
            } => write!(
                f,
                "field `{field_name}` is an interface field on `{interface_name}`, not a concrete field on class `{class_name}`; use obj.as<{interface_name}>.{field_name}"
            ),
            TirTypeError::InterfaceFieldRequiresQualifiedConstruction {
                field_name,
                qualified_name,
            } => write!(
                f,
                "interface-qualified field `{field_name}` cannot be used in a class constructor; use class field `{qualified_name}`"
            ),
            TirTypeError::InvalidInterfaceUpcastTarget { target } => {
                write!(f, "`.as<T>` target must be an interface, got `{target}`")
            }
            TirTypeError::InterfaceMemberRequiresReceiver {
                interface_name,
                member_name,
            } => write!(
                f,
                "interface member `{member_name}` on `{interface_name}` must be accessed through a value; use value.as<{interface_name}>.{member_name}"
            ),
            TirTypeError::InvalidSelfCallThroughInterface {
                interface_name,
                method_name,
                position,
            } => {
                let position = match position {
                    SelfCallPosition::Parameter => "a parameter",
                    SelfCallPosition::NestedInReturn => {
                        "its return/throws type, nested in a container (e.g. `Self[]`)"
                    }
                };
                write!(
                    f,
                    "method `{method_name}` on interface `{interface_name}` uses `Self` in \
                     {position}, so it requires a concrete receiver, not an \
                     interface-existential one"
                )
            }
            TirTypeError::DefaultOnRequiredMethod {
                interface_name,
                method_name,
            } => write!(
                f,
                "`default.{method_name}()` is invalid: method `{method_name}` on interface \
                 `{interface_name}` has no default body"
            ),
            TirTypeError::BareDefaultKeyword => write!(
                f,
                "`default` may only be used to call an interface default method, as \
                 `default.method(...)`"
            ),
            TirTypeError::TypeDoesNotImplementInterface {
                value_type,
                interface,
            } => write!(
                f,
                "type `{}` does not implement interface `{}`",
                value_type.render_user_facing(),
                interface.render_user_facing()
            ),
            TirTypeError::BlanketBoundNotSatisfied { value_type, bound } => write!(
                f,
                "type `{}` does not satisfy the bound `{}` required by the blanket \
                 `implements` rule",
                value_type.render_user_facing(),
                bound.render_user_facing()
            ),
            TirTypeError::RuntimeIdCompoundAssignment => write!(
                f,
                "`$id` cannot be the target of a compound assignment; use `$id = ...` with an \
                 override from `baml.id.new()`"
            ),
            TirTypeError::RuntimeIdMemberAccess { member } => write!(
                f,
                "`$id` is a value, not a binding; bind it to a local before accessing `.{member}` \
                 (e.g. `let id = $id; id.{member}`)"
            ),
            TirTypeError::DuplicateRuntimeIdArgument => {
                write!(f, "duplicate `$id` call argument")
            }
            TirTypeError::RuntimeIdArgumentMustBeLast => write!(
                f,
                "`$id` must be the final call argument because it is trailing call metadata"
            ),
            TirTypeError::RuntimeIdArgumentTypeMismatch { got } => write!(
                f,
                "`$id` at a call site expects `boundary.LocalId`, got {}",
                got.render_user_facing()
            ),
            TirTypeError::IntegerLiteralOutOfRange { value } => write!(
                f,
                "integer literal `{value}` is out of range for `int` \
                 (which holds -4611686018427387904 to 4611686018427387903); \
                 append `n` to write it as a `bigint`"
            ),
            TirTypeError::GenericBoundNotInterface { bound } => write!(
                f,
                "generic bound `{}` is not an interface; bounds must be interfaces",
                bound.render_user_facing()
            ),
            TirTypeError::BuiltinInterfaceNotImplementable { interface } => write!(
                f,
                "`{}` is a compiler builtin and cannot be implemented by hand; \
                 every function type implements it automatically",
                interface.render_user_facing()
            ),
            TirTypeError::BuiltinInterfaceNotABound { interface } => write!(
                f,
                "`{}` cannot be used as a generic bound; use it as a value type \
                 instead (e.g. `f: {}`)",
                interface.render_user_facing(),
                interface.render_user_facing()
            ),
            TirTypeError::BoundedTypeArgNotConcrete { arg, bound } => {
                let bound = bound
                    .iter()
                    .map(|iface| iface.to_ty().render_user_facing())
                    .collect::<Vec<_>>()
                    .join(" & ");
                write!(
                    f,
                    "type argument `{}` is not concrete; a type parameter bounded by `{bound}` \
                     requires a concrete type that implements it (an abstract type like a union \
                     or interface has no single runtime type to dispatch on)",
                    arg.render_user_facing()
                )
            }
            TirTypeError::MissingAssociatedTypeBindings { interface, missing } => {
                let missing = missing
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "interface-existential type `{}` must specify its associated type(s) {missing} \
                     (only associated types with a default may be omitted; an interface *bound* \
                     `<T extends {}>` does not require them)",
                    interface.render_user_facing(),
                    interface.render_user_facing(),
                )
            }
            TirTypeError::InterfaceProjectionBase { interface, member } => {
                write!(
                    f,
                    "cannot project `{member}` directly off interface `{0}`: a projection's \
                     base is an implementor type, a bounded type variable, or `Self` — to name \
                     the interface explicitly, write a qualified projection \
                     (`(Base as {0}).{member}`)",
                    interface.render_user_facing(),
                )
            }
            TirTypeError::AmbiguousInterfacePatternBindings {
                interface,
                candidates,
            } => {
                let candidates = candidates
                    .iter()
                    .map(|ty| format!("`{}`", ty.render_user_facing()))
                    .collect::<Vec<_>>()
                    .join(" and ");
                write!(
                    f,
                    "cannot infer the associated type bindings for the bare interface pattern \
                     `{interface} {{ … }}`: the scrutinee admits {candidates}; write the bindings \
                     explicitly (`{interface}<…> {{ … }}`)",
                    interface = interface.render_user_facing(),
                )
            }
            TirTypeError::MissingInterfaceMethod { interface, method } => {
                write!(
                    f,
                    "missing implementation of method `{method}` required by interface `{}`",
                    interface.render_user_facing()
                )
            }
            TirTypeError::GenericSysOpMethodInInterfaceImpl { interface, method } => {
                write!(
                    f,
                    "`$rust_io_function` method `{method}` implementing interface `{}` may not \
                     declare its own generic parameters: a sys-op reached through interface \
                     dispatch cannot carry method-level type arguments",
                    interface.render_user_facing()
                )
            }
            TirTypeError::OutOfBodyImplementsFieldInterface { interface } => {
                write!(
                    f,
                    "interface `{}` declares fields and can only be implemented in the class \
                     body, not out-of-body",
                    interface.render_user_facing()
                )
            }
            TirTypeError::UnknownInterfaceMember { interface, member } => {
                write!(
                    f,
                    "`{member}` is not a method of interface `{}`",
                    interface.render_user_facing()
                )
            }
            TirTypeError::MissingInterfaceField { interface, field } => {
                write!(
                    f,
                    "missing field `{field}` required by interface `{}` (add a class field named \
                     `{field}` or link one with `{field} as <class_field>`)",
                    interface.render_user_facing()
                )
            }
            TirTypeError::InterfaceFieldTypeMismatch {
                interface,
                field,
                expected,
                got,
            } => {
                write!(
                    f,
                    "field `{field}` has type {}, but interface `{}` declares it as {}",
                    got.render_user_facing(),
                    interface.render_user_facing(),
                    expected.render_user_facing()
                )
            }
            TirTypeError::InterfaceMethodSignatureMismatch {
                interface,
                method,
                expected,
                got,
            } => {
                write!(
                    f,
                    "method `{method}` has signature {}, which does not conform to interface `{}`'s \
                     declared {}",
                    got.render_user_facing(),
                    interface.render_user_facing(),
                    expected.render_user_facing()
                )
            }
            TirTypeError::InterfaceMethodAddsGenericBound {
                interface,
                method,
                param,
                bound,
            } => {
                write!(
                    f,
                    "method `{method}` requires `{param}: {}`, but interface `{}`'s `{method}` \
                     declares no such bound — an implementation may not add requirements the \
                     interface does not",
                    bound.to_ty().render_user_facing(),
                    interface.render_user_facing()
                )
            }
            TirTypeError::MissingRequiredInterface {
                interface,
                required,
            } => {
                write!(
                    f,
                    "implementing `{}` also requires implementing `{}`",
                    interface.render_user_facing(),
                    required.to_ty().render_user_facing()
                )
            }
            TirTypeError::ImplTargetNotInterface { name } => {
                write!(f, "`{name}` is not an interface and cannot be implemented")
            }
            TirTypeError::ImplTargetNotConcrete { target } => {
                write!(
                    f,
                    "cannot implement an interface for {} — the target must be a single concrete \
                     type",
                    target.render_user_facing()
                )
            }
            TirTypeError::UnconstrainedImplTypeParam { name } => {
                write!(
                    f,
                    "generic parameter `{name}` is not constrained by the `for` type or interface \
                     arguments, so it can never be inferred"
                )
            }
            TirTypeError::ImplViolatesOrphanRule {
                interface,
                uncovered_param,
            } => match uncovered_param {
                Some(param) => write!(
                    f,
                    "orphan rule: implementing the foreign interface `{}` requires a local type \
                     before the uncovered parameter `{param}`",
                    interface.render_user_facing()
                ),
                None => write!(
                    f,
                    "orphan rule: a foreign interface `{}` can only be implemented for a local type",
                    interface.render_user_facing()
                ),
            },
            TirTypeError::UnknownInterfaceFieldLink { interface, field } => {
                write!(
                    f,
                    "interface `{}` has no field `{field}` to link",
                    interface.render_user_facing()
                )
            }
            TirTypeError::UnknownClassFieldInInterfaceLink {
                class,
                interface,
                field,
            } => {
                write!(
                    f,
                    "class `{class}` has no field `{field}` to link for interface `{}`",
                    interface.render_user_facing()
                )
            }
            TirTypeError::DuplicateInterfaceFieldLink { interface, field } => {
                write!(
                    f,
                    "field `{field}` of interface `{}` is linked more than once",
                    interface.render_user_facing()
                )
            }
            TirTypeError::UnknownAssociatedTypeBinding { interface, name } => {
                write!(
                    f,
                    "unknown associated type `{name}` for interface `{}`",
                    interface.render_user_facing()
                )
            }
            TirTypeError::DuplicateAssociatedTypeBinding { interface, name } => {
                write!(
                    f,
                    "associated type `{name}` of interface `{}` is bound more than once",
                    interface.render_user_facing()
                )
            }
            TirTypeError::MissingImplAssociatedTypeBinding { interface, name } => {
                write!(
                    f,
                    "missing associated type binding `{name}` for interface `{}` (add `type {name} = \
                     …` to the implements block)",
                    interface.render_user_facing()
                )
            }
            TirTypeError::AssociatedTypeBindingsOnImplementsTarget { interface } => {
                write!(
                    f,
                    "associated type bindings are not allowed on an `implements` target for `{}`; \
                     bind them inside the block with `type Name = …`",
                    interface.render_user_facing()
                )
            }
            TirTypeError::AssociatedTypeBindingViolatesBound {
                interface,
                name,
                binding,
                bound,
            } => {
                write!(
                    f,
                    "associated type binding `{name}` = `{}` does not implement bound `{}` declared \
                     by interface `{}`",
                    binding.render_user_facing(),
                    bound.to_ty().render_user_facing(),
                    interface.render_user_facing()
                )
            }
            TirTypeError::SelfInInterfaceField { interface, field } => {
                write!(
                    f,
                    "`Self` is not allowed in the type of interface field `{field}` on `{}`; name \
                     the interface itself for recursion",
                    interface.render_user_facing()
                )
            }
            TirTypeError::SelfInAssociatedTypeDefault {
                interface,
                associated_type,
            } => {
                write!(
                    f,
                    "`Self` is not allowed in the default for associated type `{associated_type}` \
                     on `{}`: `Self` names each implementor, so it has no meaning where `{0}` is \
                     used as an interface-existential type and the implementor is hidden. Drop the \
                     default and let each `implements` block bind `{associated_type}` (uses as a \
                     bound are unaffected; uses as an interface-existential type then write \
                     `{0}<…, {associated_type} = …>`). A `Self.Assoc` projection is allowed",
                    interface.render_user_facing()
                )
            }
            TirTypeError::InterfaceRequiresNonInterface { interface, target } => {
                write!(
                    f,
                    "interface `{}` cannot require `{}`, which is not an interface",
                    interface.render_user_facing(),
                    target.render_user_facing()
                )
            }
            TirTypeError::InterfaceRequiresCycle { chain } => {
                let rendered = chain
                    .iter()
                    .map(Name::as_str)
                    .collect::<Vec<_>>()
                    .join(" → ");
                write!(f, "interface `requires` cycle: {rendered}")
            }
            TirTypeError::AssociatedTypeDefaultViolatesBound {
                interface,
                name,
                default,
                bound,
            } => {
                write!(
                    f,
                    "associated type default `{name}` = `{}` does not implement bound `{}` declared \
                     by interface `{}`",
                    default.render_user_facing(),
                    bound.to_ty().render_user_facing(),
                    interface.render_user_facing()
                )
            }
            TirTypeError::CyclicImplHeader => write!(
                f,
                "a concrete associated-type projection in an impl header is illegal here (it \
                 resolves through this impl); name the resolved type directly"
            ),
            TirTypeError::InterfaceMethodMissingThrows { interface, method } => {
                write!(
                    f,
                    "interface method `{method}` on `{}` must declare an explicit `throws` clause",
                    interface.render_user_facing()
                )
            }
            TirTypeError::FunctionTypeMissingThrows => {
                write!(
                    f,
                    "function type must declare an explicit `throws` clause; add `throws never` \
                     if calling it cannot throw"
                )
            }
            TirTypeError::InitIoNotAllowed {
                declaration,
                is_client,
                sysop,
                via,
            } => {
                let kind = if *is_client { "client" } else { "declaration" };
                let reach = match via {
                    Some(via) => format!("reaches `{sysop}`, which performs io, through `{via}`"),
                    None => format!("calls `{sysop}`, which performs io"),
                };
                write!(
                    f,
                    "{kind} `{declaration}` {reach} — top-level declarations are evaluated at \
                     startup (`$init`), where io is unavailable; resolve at request time \
                     instead — e.g. `env.X`, a late-bound reference read only when the \
                     request is made"
                )
            }
        }
    }
}

/// Diagnostic severity used by compiler2 TIR diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

// ── Multi-span diagnostic structure ─────────────────────────────────────────

/// A location that may be in the current scope, another scope, or another file.
///
/// All variants use Salsa-stable IDs — no `TextRange`s. The LSP layer maps
/// each variant to `(File, TextRange)` via the appropriate source map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelatedLocation<'db> {
    /// Expression in the same scope's `ExprBody`.
    Expr(ExprId),
    /// A specific segment of a multi-segment `Path` expression.
    ExprSegment(ExprId, usize),
    /// Statement in the same scope's `ExprBody`.
    Stmt(StmtId),
    /// A function parameter (possibly in another file).
    Param(FunctionLoc<'db>, usize),
    /// A class field definition.
    ClassField(ClassLoc<'db>, Name),
    /// Any top-level item definition (class, enum, function, etc.).
    Item(Definition<'db>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedNote<'db> {
    pub location: RelatedLocation<'db>,
    pub message: String,
}

impl<'db> RelatedNote<'db> {
    pub fn new(location: RelatedLocation<'db>, message: impl Into<String>) -> Self {
        Self {
            location,
            message: message.into(),
        }
    }
}

/// Primary location for a diagnostic — either an expression, a statement,
/// or a raw source span (for type annotations that lack an `ExprId`).
/// "Did you mean" candidates for an unknown member/field name: fuzzy-scored
/// against the declared names (TIR's `similar_name_suggestions`, verbatim).
pub fn similar_name_suggestions<'a>(
    unknown_name: &Name,
    candidates: impl IntoIterator<Item = &'a Name>,
) -> Vec<Name> {
    let needle = unknown_name.as_str().to_ascii_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(f64, Name)> = candidates
        .into_iter()
        .map(|candidate| {
            let candidate_lower = candidate.as_str().to_ascii_lowercase();
            let mut score = strsim::jaro_winkler(&needle, &candidate_lower);
            if candidate_lower.starts_with(&needle) || needle.starts_with(&candidate_lower) {
                score += 0.15;
            }
            if candidate_lower.contains(&needle) || needle.contains(&candidate_lower) {
                score += 0.10;
            }
            (score.min(1.0), candidate.clone())
        })
        .filter(|(score, _)| *score >= 0.80)
        .collect();
    scored.sort_by(|(sa, na), (sb, nb)| {
        sb.partial_cmp(sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| na.as_str().cmp(nb.as_str()))
    });
    scored.into_iter().map(|(_, name)| name).take(3).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticLocation {
    Expr(ExprId),
    /// The member-name portion of a `MemberAccess` expression (after the dot).
    ExprMember(ExprId),
    /// A specific segment of a multi-segment `Path` expression.
    /// `ExprSegment(path_id, segment_idx)` resolves to `path_segment_span(path_id, segment_idx)`.
    ExprSegment(ExprId, usize),
    Stmt(StmtId),
    TypeAnnot(TypeAnnotId),
    /// A pattern's span (`hir_ty`'s emissions stay arena-anchored; TIR
    /// resolved pattern spans eagerly through its held source map).
    Pat(baml_compiler2_ast::PatId),
    /// A written type reference (body annotation), resolved through the
    /// body's `TypeRefSourceMap` at render time.
    TypeRef(baml_compiler2_hir::type_ref::TypeRefId),
    /// The field-NAME portion of an object-literal entry:
    /// `(object_expr, field_value_expr)` resolves through
    /// `object_field_name_span` at render time.
    ObjectFieldName(ExprId, ExprId),
    Span(TextRange),
}

/// A single type-check diagnostic with primary location and optional related spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TirDiagnostic<'db> {
    /// What went wrong.
    pub error: TirTypeError,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Primary location — where the error was detected.
    pub primary: DiagnosticLocation,
    /// Related locations — secondary spans with explanatory messages.
    pub related: Vec<RelatedNote<'db>>,
}

impl<'db> TirDiagnostic<'db> {
    /// Resolve this diagnostic's arena IDs to source ranges and produce a
    /// rendered diagnostic with a human-readable message and `TextRange`.
    ///
    /// `source_map` is the `AstSourceMap` for the function body that owns
    /// the expressions/statements referenced by `self.primary`.
    pub fn render(
        &self,
        db: &'db dyn baml_compiler2_ppir::Db,
        scope_file: SourceFile,
        source_map: Option<&AstSourceMap>,
    ) -> RenderedTirDiagnostic {
        self.render_with_type_refs(db, scope_file, source_map, None)
    }

    /// [`Self::render`] with the body's type-ref span map, for the
    /// annotation-anchored diagnostics (`DiagnosticLocation::TypeRef`).
    pub fn render_with_type_refs(
        &self,
        db: &'db dyn baml_compiler2_ppir::Db,
        scope_file: SourceFile,
        source_map: Option<&AstSourceMap>,
        type_ref_spans: Option<&baml_compiler2_hir::type_ref::TypeRefSourceMap>,
    ) -> RenderedTirDiagnostic {
        let primary_range = match &self.primary {
            DiagnosticLocation::Expr(id) => {
                source_map.map(|sm| sm.expr_span(*id)).unwrap_or_default()
            }
            DiagnosticLocation::ObjectFieldName(object, value) => source_map
                .map(|sm| sm.object_field_name_span(*object, *value))
                .unwrap_or_default(),
            DiagnosticLocation::ExprMember(id) => source_map
                .map(|sm| sm.member_access_member_span(*id))
                .unwrap_or_default(),
            DiagnosticLocation::ExprSegment(id, seg_idx) => source_map
                .map(|sm| sm.path_segment_span(*id, *seg_idx))
                .unwrap_or_default(),
            DiagnosticLocation::Stmt(id) => {
                source_map.map(|sm| sm.stmt_span(*id)).unwrap_or_default()
            }
            DiagnosticLocation::TypeAnnot(id) => source_map
                .map(|sm| sm.type_annotation_span(*id))
                .unwrap_or_default(),
            DiagnosticLocation::Pat(id) => source_map
                .map(|sm| sm.pattern_span(*id))
                .unwrap_or_default(),
            DiagnosticLocation::TypeRef(id) => type_ref_spans
                .map(|spans| spans.span(*id))
                .unwrap_or_default(),
            DiagnosticLocation::Span(range) => *range,
        };

        let related = self
            .related
            .iter()
            .filter_map(|note| {
                resolve_related_location(db, scope_file, source_map, &note.location).map(
                    |(file_id, range)| RenderedRelatedInformation {
                        file_id,
                        range,
                        message: note.message.clone(),
                    },
                )
            })
            .collect();

        RenderedTirDiagnostic {
            error: self.error.clone(),
            message: self.error.to_string(),
            range: primary_range,
            severity: self.severity,
            related,
        }
    }
}

fn resolve_related_location<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    scope_file: SourceFile,
    source_map: Option<&AstSourceMap>,
    location: &RelatedLocation<'db>,
) -> Option<(FileId, TextRange)> {
    match location {
        RelatedLocation::Expr(id) => {
            source_map.map(|sm| (scope_file.file_id(db), sm.expr_span(*id)))
        }
        RelatedLocation::ExprSegment(id, seg_idx) => {
            source_map.map(|sm| (scope_file.file_id(db), sm.path_segment_span(*id, *seg_idx)))
        }
        RelatedLocation::Stmt(id) => {
            source_map.map(|sm| (scope_file.file_id(db), sm.stmt_span(*id)))
        }
        RelatedLocation::Param(func_loc, idx) => {
            let signature_source_map =
                baml_compiler2_hir::signature::function_signature_source_map(db, *func_loc);
            signature_source_map
                .param_spans
                .get(*idx)
                .copied()
                .map(|range| (func_loc.file(db).file_id(db), range))
        }
        RelatedLocation::ClassField(class_loc, field_name) => {
            let class_data = baml_compiler2_ppir::item_data::class_data(db, *class_loc);
            let field_index = class_data
                .fields
                .iter()
                .position(|field| &field.name == field_name)?;
            let range = baml_compiler2_ppir::item_data::class_source_map(db, *class_loc)
                .field_name_spans
                .get(field_index)
                .copied()?;
            Some((class_loc.file(db).file_id(db), range))
        }
        RelatedLocation::Item(def) => {
            let file = def.file(db);
            let contributions = baml_compiler2_hir::file_symbol_contributions(db, file);
            contributions
                .types
                .iter()
                .chain(contributions.values.iter())
                .find_map(|(_, contribution)| {
                    (contribution.definition == *def)
                        .then_some((file.file_id(db), contribution.name_span))
                })
        }
    }
}

/// A fully rendered diagnostic — ready for display / LSP.
///
/// Contains the human-readable message and the resolved source `TextRange`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedTirDiagnostic {
    /// Original typed error for downstream ID mapping.
    pub error: TirTypeError,
    /// Human-readable error message (e.g. "type mismatch: expected int, got string").
    pub message: String,
    /// Source range within the file (resolved from `ExprId`/`StmtId`).
    pub range: TextRange,
    /// Severity level for rendering.
    pub severity: DiagnosticSeverity,
    /// Resolved related spans/messages for LSP consumers.
    pub related: Vec<RenderedRelatedInformation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedRelatedInformation {
    pub file_id: FileId,
    pub range: TextRange,
    pub message: String,
}

impl fmt::Display for RenderedTirDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = u32::from(self.range.start());
        let end = u32::from(self.range.end());
        write!(f, "{start}..{end}: {}", self.message)
    }
}

/// Accumulated diagnostics for a single scope inference run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeCheckDiagnostics<'db> {
    pub diagnostics: Vec<TirDiagnostic<'db>>,
}

impl<'db> TypeCheckDiagnostics<'db> {
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn extend(&mut self, other: &TypeCheckDiagnostics<'db>) {
        self.diagnostics.extend(other.diagnostics.iter().cloned());
    }
}
