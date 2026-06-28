//! Diagnostic sink for a single scope inference run.
//!
//! `InferContext` is held inside `TypeInferenceBuilder` and accumulates
//! type errors discovered during expression walking. Consuming `finish()`
//! returns the accumulated `TypeCheckDiagnostics`.
//!
//! Diagnostics are Salsa-stable (no `TextRange`) — locations are stored as
//! arena IDs. The LSP layer maps them to source ranges at display time.

use std::{cell::RefCell, fmt};

use baml_base::{FileId, Name, SourceFile};
use baml_compiler2_ast::{AstSourceMap, ExprId, StmtId, TypeAnnotId};
use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, FunctionLoc},
    scope::ScopeId,
};
use text_size::TextRange;

use crate::ty::{QualifiedTypeName, Ty};

// ── Error kinds ──────────────────────────────────────────────────────────────

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
    /// Name could not be resolved at all.
    UnresolvedName { name: Name },
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
        suggestions: Vec<String>,
    },
    /// An associated-type projection's explicit `as X` qualifier resolved to a
    /// non-interface type (a class, alias, etc.). The qualifier must name an
    /// interface; without one the projection cannot be resolved, so it must not
    /// silently fall back to an unqualified projection.
    NonInterfaceProjectionQualifier,
    /// An associated-type projection references `member`, but the interface or
    /// class it projects through does not declare it as an associated type.
    ///
    /// Interfaces do not inherit associated types through `requires` (it is a
    /// bound, not inheritance), so a member declared on a *required* interface
    /// must be projected through that interface directly: `(Foo as Iterator).Item`,
    /// not `(Foo as RequiresIterator).Item`.
    UnknownAssociatedType {
        member: Name,
        container: QualifiedTypeName,
        container_is_interface: bool,
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
    /// A rest pattern (`..`) carries a sub-pattern (`..let r`, `..[a, b]`,
    /// `..pat: T`, etc.). Currently unsupported — only bare `..` is allowed
    /// while we settle the rest-vs-slice typing semantics.
    RestSubPatternNotSupported,
    /// A `let` statement or `for-let` binding uses a pattern that can fail
    /// for values of the type flowing into it.
    RefutablePatternInLet {
        context: crate::builder::IrrefutableContextKind,
    },
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
    /// A method's generic type parameter shadows a class-level type parameter.
    TypeParamShadowed { param_name: Name, class_name: Name },
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

    /// Interface-typed receivers cannot call methods with additional `Self`
    /// parameters. The concrete implementor must be known for those arguments.
    InvalidSelfCallThroughInterface {
        interface_name: Name,
        method_name: Name,
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
    /// BEP-044 wf3 #18: a class provides the SAME interface instantiation via
    /// more than one `implements` block (distinct generic blocks that collapse
    /// under the concrete type args, e.g. `Getter<L>`+`Getter<R>` at
    /// `Pair<int, int>`). Coercing to that interface is ambiguous.
    AmbiguousInterfaceInstantiation { class_name: Name, interface: Ty },
    /// `$id` cannot be the target of a compound assignment (`$id += ...`):
    /// the runtime ID can only be replaced wholesale with an override from
    /// `baml.id.new()` via `$id = ...`.
    RuntimeIdCompoundAssignment,
    /// Member access on `$id` (e.g. `$id.len()`). `$id` reads as a plain
    /// string value but is not a binding; bind it to a local first.
    RuntimeIdMemberAccess { member: Name },
    /// `$id` used as a call-site argument label (`foo($id = x)`). Overrides
    /// are set inside the callee body with `$id = ...`, not by the caller.
    RuntimeIdCallSiteArgument,
    /// An integer literal (or a constant-folded integer expression) is outside
    /// the representable `int` range `[-2^62, 2^62-1]`. `int` is 63-bit; larger
    /// magnitudes need a `bigint` literal (`n` suffix).
    IntegerLiteralOutOfRange { value: i64 },
    /// BEP-044: a generic parameter's bound (`<T extends X>`) resolved to a
    /// concrete non-interface type. Generic bounds must be interfaces.
    GenericBoundNotInterface { bound: Ty },
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
            TirTypeError::UnresolvedName { name } => {
                write!(f, "unresolved name: {name}")
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
            TirTypeError::UnknownAssociatedType {
                member,
                container,
                container_is_interface,
            } => {
                let kind = if *container_is_interface {
                    "interface"
                } else {
                    "class"
                };
                write!(
                    f,
                    "unknown associated type `{member}` for {kind} `{}`",
                    container.render_user_facing()
                )
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
                    write!(
                        f,
                        "unresolved type: {name}. Did you mean one of these: `{}`?",
                        suggestions.join("`, `")
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
            TirTypeError::RestSubPatternNotSupported => write!(
                f,
                "rest pattern `..` cannot carry a sub-pattern; only bare `..` is allowed"
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
                class_name,
            } => {
                write!(
                    f,
                    "type parameter `{param_name}` on method shadows the same parameter on class `{class_name}`. \
                    Please use a different name for the type parameter."
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
            } => write!(
                f,
                "method `{method_name}` on interface `{interface_name}` uses `Self` in \
                 its parameters and requires a concrete receiver"
            ),
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
            TirTypeError::AmbiguousInterfaceInstantiation {
                class_name,
                interface,
            } => write!(
                f,
                "class `{class_name}` implements `{}` through more than one `implements` block at \
                 this instantiation (distinct generic blocks collapse to the same type); the \
                 projection is ambiguous",
                interface.render_user_facing()
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
            TirTypeError::RuntimeIdCallSiteArgument => write!(
                f,
                "`$id` cannot be set at the call site; assign `$id = ...` inside the function \
                 body instead"
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
        db: &'db dyn crate::Db,
        scope_file: SourceFile,
        source_map: Option<&AstSourceMap>,
    ) -> RenderedTirDiagnostic {
        let primary_range = match &self.primary {
            DiagnosticLocation::Expr(id) => {
                source_map.map(|sm| sm.expr_span(*id)).unwrap_or_default()
            }
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
    db: &'db dyn crate::Db,
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
            let item_tree = baml_compiler2_hir::file_item_tree(db, class_loc.file(db));
            let source_map = baml_compiler2_hir::file_item_tree_source_map(db, class_loc.file(db));
            let class_data = &item_tree[class_loc.id(db)];
            let field_index = class_data
                .fields
                .iter()
                .position(|field| &field.name == field_name)?;
            let range = source_map
                .class_field_spans
                .get(&class_loc.id(db))?
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

// ── InferContext ─────────────────────────────────────────────────────────────

/// Diagnostic sink for a single scope inference run.
///
/// Held inside `TypeInferenceBuilder` — one per `infer_scope_types` call.
/// Modeled after Ty's `InferContext` (`context.rs:37-46`).
pub struct InferContext<'db> {
    db: &'db dyn crate::Db,
    scope: ScopeId<'db>,
    diagnostics: RefCell<TypeCheckDiagnostics<'db>>,
    /// When `true`, suppress diagnostics that arise from synthesized
    /// references to user types/names/members. Set while inferring an
    /// auto-derived function body (synthesized `to_json` / `from_json`):
    /// those bodies reference user fields by name, so when a class has a
    /// malformed field, the synthesizer's `self.<f>.to_json()` and
    /// `baml.json.from_json<F>(...)` calls surface duplicate
    /// `UnresolvedType` / `UnresolvedMember` / `NotCallable` errors whose
    /// spans point back at the user's class — confusing because the user
    /// didn't write that code. The user's underlying field declaration
    /// already reports the real error.
    suppress_member_lookup_errors: std::cell::Cell<bool>,
}

/// Returns `true` for diagnostic kinds that may arise spuriously from
/// auto-derived function bodies (synthesized code referencing user types).
/// We suppress these inside auto-derive bodies; the user's underlying type
/// declaration already reports the same condition without the synthesized
/// span confusion.
fn is_synthesized_code_diag(error: &TirTypeError) -> bool {
    matches!(
        error,
        TirTypeError::UnresolvedMember { .. }
            | TirTypeError::UnresolvedType { .. }
            | TirTypeError::UnresolvedName { .. }
            | TirTypeError::NotCallable { .. }
    )
}

impl<'db> InferContext<'db> {
    pub fn new(db: &'db dyn crate::Db, scope: ScopeId<'db>) -> Self {
        Self {
            db,
            scope,
            diagnostics: RefCell::new(TypeCheckDiagnostics::default()),
            suppress_member_lookup_errors: std::cell::Cell::new(false),
        }
    }

    /// Toggle suppression of `UnresolvedMember` diagnostics for the
    /// current inference run. See `suppress_member_lookup_errors`.
    pub fn set_suppress_member_lookup_errors(&self, value: bool) {
        self.suppress_member_lookup_errors.set(value);
    }

    pub fn db(&self) -> &'db dyn crate::Db {
        self.db
    }

    /// Number of diagnostics recorded so far. Paired with
    /// [`truncate_diagnostics`](Self::truncate_diagnostics) to run a
    /// speculative resolution (e.g. probing one member of a union) and roll
    /// back any diagnostics it emitted.
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.borrow().diagnostics.len()
    }

    /// Drop every diagnostic recorded after index `n` (see
    /// [`diagnostic_count`](Self::diagnostic_count)).
    pub fn truncate_diagnostics(&self, n: usize) {
        self.diagnostics.borrow_mut().diagnostics.truncate(n);
    }

    /// Drop every diagnostic recorded after index `n` EXCEPT genuine
    /// `UnresolvedName`s. Used by the untagged-backtick (`Default` template)
    /// path: inferring the desugared `elaborated` concat synthesizes
    /// `expr.to_string()` member calls and a `+`-fold, whose failures
    /// (`NotCallable`/`UnresolvedMember`) are noise pointing at synthetic spans
    /// (the strict-stringify errors are re-reported on the original `${…}` spans
    /// by [`check_template_interps_stringable`](crate::builder)). But a bare
    /// unresolved *name* — `${ nope }`, or `nope` on a spliced `let`'s RHS — is
    /// never introduced by that desugaring (it emits member/call nodes and a
    /// guaranteed-bound accumulator, never a fresh name reference), so an
    /// `UnresolvedName` here is always genuine user code. Keeping it surfaces the
    /// real error and prevents the unresolved `Ty::Unknown` from slipping through
    /// to MIR, where runtime lowering of an error-recovery type ICEs.
    pub fn retain_user_name_diagnostics(&self, n: usize) {
        let mut diags = self.diagnostics.borrow_mut();
        let len = diags.diagnostics.len();
        if n >= len {
            return;
        }
        // Keep `[..n]` verbatim; from `[n..]` keep only `UnresolvedName`.
        let tail: Vec<TirDiagnostic<'db>> = diags
            .diagnostics
            .drain(n..)
            .filter(|d| matches!(d.error, TirTypeError::UnresolvedName { .. }))
            .collect();
        diags.diagnostics.extend(tail);
    }

    /// Freeze the source spans of diagnostics recorded at index `[start..]`,
    /// resolving their arena-relative locations against `source_map` and
    /// replacing them with absolute [`DiagnosticLocation::Span`]s.
    ///
    /// Used when a nested lambda body is inferred *inline* in an enclosing
    /// scope (`infer_lambda_body`): those diagnostics carry the lambda's own
    /// arena IDs but are recorded in the enclosing scope's diagnostic set, so
    /// at render time they'd be resolved against the *enclosing* scope's source
    /// map — which can't resolve a nested-arena ID, collapsing the span to
    /// `0..0`. Resolving them here, while the lambda's source map is in hand,
    /// makes them render correctly regardless of which scope renders them.
    /// Already-frozen (`Span`) locations and deeper-lambda diagnostics (frozen
    /// by their own `infer_lambda_body`) are left unchanged.
    pub fn freeze_diagnostic_spans_from(&self, start: usize, source_map: &AstSourceMap) {
        let mut diags = self.diagnostics.borrow_mut();
        let len = diags.diagnostics.len();
        for d in &mut diags.diagnostics[start.min(len)..] {
            d.primary = Self::freeze_location(&d.primary, source_map);
        }
    }

    fn freeze_location(loc: &DiagnosticLocation, sm: &AstSourceMap) -> DiagnosticLocation {
        let span = match loc {
            DiagnosticLocation::Expr(id) => sm.expr_span(*id),
            DiagnosticLocation::ExprMember(id) => sm.member_access_member_span(*id),
            DiagnosticLocation::ExprSegment(id, seg) => sm.path_segment_span(*id, *seg),
            DiagnosticLocation::Stmt(id) => sm.stmt_span(*id),
            DiagnosticLocation::TypeAnnot(id) => sm.type_annotation_span(*id),
            // Already absolute (e.g. a deeper lambda's frozen diagnostic, or a
            // class-field span) — leave it.
            DiagnosticLocation::Span(r) => *r,
        };
        DiagnosticLocation::Span(span)
    }

    pub fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    /// Report a type error at a specific expression, with optional related locations.
    pub fn report(&self, error: TirTypeError, at: ExprId, related: Vec<RelatedNote<'db>>) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::Expr(at),
                related,
            });
    }

    /// Convenience: report an error with no related locations.
    pub fn report_simple(&self, error: TirTypeError, at: ExprId) {
        self.report(error, at, Vec::new());
    }

    /// Report a type error at the member-name portion of a `MemberAccess` expression.
    pub fn report_at_member(
        &self,
        error: TirTypeError,
        at: ExprId,
        related: Vec<RelatedNote<'db>>,
    ) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::ExprMember(at),
                related,
            });
    }

    /// Convenience: report at member with no related locations.
    pub fn report_at_member_simple(&self, error: TirTypeError, at: ExprId) {
        self.report_at_member(error, at, Vec::new());
    }

    /// Report a type error at a specific segment of a multi-segment `Path` expression.
    /// `segment_idx` is the index into `path_segment_spans[at]`.
    pub fn report_at_segment(
        &self,
        error: TirTypeError,
        at: ExprId,
        segment_idx: usize,
        related: Vec<RelatedNote<'db>>,
    ) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::ExprSegment(at, segment_idx),
                related,
            });
    }

    /// Report a type error at a type annotation location.
    pub fn report_at_type_annot(&self, error: TirTypeError, at: TypeAnnotId) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::TypeAnnot(at),
                related: Vec::new(),
            });
    }

    /// Report a type error at a raw source span (for type annotations).
    pub fn report_at_span(&self, error: TirTypeError, span: TextRange) {
        self.report_at_span_with_related(error, span, Vec::new());
    }

    /// Report a type error at a raw source span with related notes.
    pub fn report_at_span_with_related(
        &self,
        error: TirTypeError,
        span: TextRange,
        related: Vec<RelatedNote<'db>>,
    ) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::Span(span),
                related,
            });
    }

    /// Report a type error at a specific statement.
    pub fn report_at_stmt(&self, error: TirTypeError, at: StmtId) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::Stmt(at),
                related: Vec::new(),
            });
    }

    /// Report a warning-level diagnostic at a specific statement.
    pub fn report_warning_at_stmt(&self, error: TirTypeError, at: StmtId) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Warning,
                primary: DiagnosticLocation::Stmt(at),
                related: Vec::new(),
            });
    }

    /// Report a warning-level diagnostic at an expression.
    pub fn report_warning_simple(&self, error: TirTypeError, at: ExprId) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Warning,
                primary: DiagnosticLocation::Expr(at),
                related: Vec::new(),
            });
    }

    /// Report a warning-level diagnostic at a raw source span.
    pub fn report_warning_at_span(&self, error: TirTypeError, span: TextRange) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Warning,
                primary: DiagnosticLocation::Span(span),
                related: Vec::new(),
            });
    }

    /// Consume the context and return accumulated diagnostics.
    pub fn finish(self) -> TypeCheckDiagnostics<'db> {
        self.diagnostics.into_inner()
    }
}
