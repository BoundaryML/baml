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
use baml_compiler2_ast::{AstSourceMap, ExprId, StmtId};
use baml_compiler2_hir::{contributions::Definition, scope::ScopeId};
use text_size::TextRange;

use crate::ty::Ty;

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
    /// `match` is missing arms for one or more values.
    NonExhaustiveMatch {
        scrutinee_type: Ty,
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
    /// A method's generic type parameter shadows a class-level type parameter.
    TypeParamShadowed { param_name: Name, class_name: Name },
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

    /// The old `value.Interface.member` projection syntax has been replaced by
    /// `.as<Interface>.member`.
    DeprecatedInterfaceProjection {
        interface_name: Name,
        /// The `.as<...>` projection target with type args (e.g. `Container<int>`),
        /// which may differ from the bare `interface_name` the user wrote.
        as_target: String,
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
        interface_name: Name,
    },
    /// BEP-044: a value almost satisfies an interface via a blanket impl, but a
    /// generic bound (`T extends Bound`) is not met. Names the failed bound.
    BlanketBoundNotSatisfied { value_type: Ty, bound: Ty },
    /// BEP-044 wf3 #18: a class provides the SAME interface instantiation via
    /// more than one `implements` block (distinct generic blocks that collapse
    /// under the concrete type args, e.g. `Getter<L>`+`Getter<R>` at
    /// `Pair<int, int>`). Coercing to that interface is ambiguous.
    AmbiguousInterfaceInstantiation { class_name: Name, interface: Ty },
}

/// Format a list of interface sources as `` `a`, `b` `` for the ambiguous
/// interface method/field diagnostics. Generic so it works over both
/// `Vec<String>` (methods) and `Vec<Name>` (fields).
fn ambiguous_iface_list<T: fmt::Display>(sources: &[T]) -> String {
    sources
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render a `TirTypeError` into its user-facing message, parameterized over the
/// strategy used to turn an embedded [`Ty`] into text.
///
/// This is the single source of truth for diagnostic *templates*. Both the
/// in-crate `impl fmt::Display for TirTypeError` (which renders `Ty` via
/// [`Ty::render_user_facing`]) and the LSP's source-aware renderer (which uses a
/// file/context-aware `Ty` renderer) call through here so the message wording
/// stays in lockstep across channels. Non-`Ty` payloads (names, ops, pre-joined
/// strings, suggestions, etc.) are emitted verbatim and never routed through
/// `render_ty`.
pub fn format_tir_type_error(error: &TirTypeError, render_ty: impl Fn(&Ty) -> String) -> String {
    match error {
        TirTypeError::TypeMismatch { expected, got } => {
            format!(
                "type mismatch: expected {}, got {}",
                render_ty(expected),
                render_ty(got)
            )
        }
        TirTypeError::UnresolvedMember { base_type, member } => {
            format!("type `{}` has no member `{member}`", render_ty(base_type))
        }
        TirTypeError::UnresolvedName { name } => {
            format!("unresolved name: {name}")
        }
        TirTypeError::DeadCode {
            unreachable_count, ..
        } => {
            format!(
                "unreachable code: {unreachable_count} statement(s) after diverging statement"
            )
        }
        TirTypeError::VoidUsedAsValue => {
            "`if` without `else` cannot be used as a value; add an `else` branch".to_string()
        }
        TirTypeError::VoidFunctionResultUsed => {
            "cannot use return value of a void function".to_string()
        }
        TirTypeError::NotCallable { ty } => {
            format!(
                "`{}` is not a function — it cannot be called",
                render_ty(ty)
            )
        }
        TirTypeError::NotIterable { ty } => {
            format!("cannot iterate over type `{}`", render_ty(ty))
        }
        TirTypeError::NotIndexable { ty } => {
            format!("type `{}` is not indexable", render_ty(ty))
        }
        TirTypeError::InvalidBinaryOp { op, lhs, rhs } => {
            format!(
                "operator `{op:?}` cannot be applied to `{}` and `{}`",
                render_ty(lhs),
                render_ty(rhs)
            )
        }
        TirTypeError::InvalidUnaryOp { op, operand } => {
            format!(
                "operator `{op:?}` cannot be applied to `{}`",
                render_ty(operand)
            )
        }
        TirTypeError::UnresolvedType { name, suggestions } => {
            if suggestions.is_empty() {
                format!("unresolved type: {name}")
            } else if suggestions.len() == 1 {
                format!("unresolved type: {name}. Did you mean `{}`?", suggestions[0])
            } else {
                format!(
                    "unresolved type: {name}. Did you mean one of these: `{}`?",
                    suggestions.join("`, `")
                )
            }
        }
        TirTypeError::ArgumentCountMismatch { expected, got } => {
            format!("expected {expected} argument(s), got {got}")
        }
        TirTypeError::PositionalArgumentAfterNamed => {
            "positional arguments cannot appear after named arguments".to_string()
        }
        TirTypeError::DuplicateNamedArgument { name } => {
            format!("duplicate named argument `{name}`")
        }
        TirTypeError::UnknownNamedArgument { name } => {
            format!("unknown named argument `{name}`")
        }
        TirTypeError::DefaultedParamPassedPositionally { name } => {
            format!("defaulted parameter `{name}` must be passed by name")
        }
        TirTypeError::MissingRequiredArgument { name } => {
            format!("missing required argument `{name}`")
        }
        TirTypeError::RequiredParamAfterDefault { name } => {
            format!("required parameter `{name}` cannot appear after a defaulted parameter")
        }
        TirTypeError::SelfParamDefault => "`self` cannot have a default value".to_string(),
        TirTypeError::DefaultParamForwardReference { param, referenced } => {
            format!(
                "default for parameter `{param}` cannot reference later parameter `{referenced}`"
            )
        }
        TirTypeError::MissingReturn { expected } => {
            format!("missing return value of type {}", render_ty(expected))
        }
        TirTypeError::NonExhaustiveMatch {
            scrutinee_type,
            missing_cases,
        } => {
            format!(
                "non-exhaustive match on type {}; missing: {}",
                render_ty(scrutinee_type),
                missing_cases.join(", ")
            )
        }
        TirTypeError::UnreachableArm => "unreachable arm".to_string(),
        TirTypeError::OrPatternBindingTypeMismatch {
            name,
            first_type,
            other_type,
        } => format!(
            "or-pattern binding `{}` has conflicting types: {} and {}",
            name,
            render_ty(first_type),
            render_ty(other_type)
        ),
        TirTypeError::GenericClassDestructureRequiresTypeArgs { class_name } => format!(
            "generic class destructure `{class_name} {{ ... }}` must specify type arguments"
        ),
        TirTypeError::RestSubPatternNotSupported => {
            "rest pattern `..` cannot carry a sub-pattern; only bare `..` is allowed".to_string()
        }
        TirTypeError::RefutablePatternInLet { context } => format!(
            "refutable pattern in {} binding; refutable patterns belong in `match`",
            context.as_str()
        ),
        TirTypeError::IrrefutablePatternInIfLet => {
            "irrefutable `if let` pattern; the `else` branch is unreachable — use a plain `let` binding instead".to_string()
        }
        TirTypeError::LetElseMustDiverge { got } => format!(
            "`let … else` requires a diverging else block (`return`, `throw`, `break`, or `continue`); got `{}`",
            render_ty(got)
        ),
        TirTypeError::IrrefutablePatternInLetElse => {
            "irrefutable `let … else` pattern; the `else` branch is unreachable — use a plain `let` binding instead".to_string()
        }
        TirTypeError::InvalidCatchBindingType { type_name } => format!(
            "invalid catch binding type `{type_name}`; use a concrete type instead"
        ),
        TirTypeError::ThrowsContractViolation {
            declared,
            extra_types,
        } => {
            format!(
                "declared throws is `{}`, but this function may also throw `{}`",
                render_ty(declared),
                extra_types.join(" | ")
            )
        }
        TirTypeError::CallbackThrowsContractViolation {
            callback_name,
            declared,
            concrete_throws,
        } => {
            let suffix = match concrete_throws {
                Some(concrete_throws) => format!(
                    "Add `throws {}` to the callback, catch the call, or make the callback non-throwing.",
                    render_ty(concrete_throws)
                ),
                None => "Add an explicit `throws` to the callback, catch the call, or make the callback non-throwing.".to_string(),
            };
            format!(
                "this body may throw through callback `{callback_name}`, but declared throws is `{}`. {suffix}",
                render_ty(declared)
            )
        }
        TirTypeError::ExtraneousThrowsDeclaration { extra_types } => {
            format!("extraneous throws declaration: {}", extra_types.join(", "))
        }
        TirTypeError::WrongTypeArgArity {
            callee_name,
            expected,
            got,
        } => {
            format!("function `{callee_name}` expects {expected} type argument(s), got {got}")
        }
        TirTypeError::TypeIsNotGeneric { type_name, kind } => {
            format!("{kind} `{type_name}` is not generic and cannot take type arguments")
        }
        TirTypeError::TypeParamShadowed {
            param_name,
            class_name,
        } => {
            format!(
                "type parameter `{param_name}` on method shadows the same parameter on class `{class_name}`. \
                Please use a different name for the type parameter."
            )
        }
        TirTypeError::CannotInferLambdaParamType { param_name } => {
            format!(
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
            format!(
                "did you mean `{dotted}`? `{expr}` is unnecessary, because `{base}` cannot be null"
            )
        }
        TirTypeError::UnnecessaryNullCoalesce { lhs, expr } => {
            // e.g. "did you mean `a`? `a ?? b` is unnecessary, because `a` cannot be null"
            format!(
                "did you mean `{lhs}`? `{expr}` is unnecessary, because `{lhs}` cannot be null"
            )
        }
        TirTypeError::SuggestNullCoalesce { lhs, rhs } => {
            // e.g. "did you mean `a ?? b`? BAML uses `??` instead of `||` for null coalescing"
            format!(
                "did you mean `{lhs} ?? {rhs}`? BAML uses `??` instead of `||` for null coalescing"
            )
        }
        TirTypeError::NullCoalesceWithNull { lhs } => {
            // e.g. "did you mean `a`? `... ?? null` is unnecessary because `a` is already nullable"
            format!(
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
            format!(
                "did you mean `{suggested}`? `{expr}` does not handle the case when `{base}` is null"
            )
        }
        TirTypeError::AmbiguousInterfaceMethod {
            class_name,
            method_name,
            sources,
        } => {
            let iface_list = ambiguous_iface_list(sources);
            let hint = sources
                .iter()
                .map(|n| format!("obj.as<{n}>.{method_name}()"))
                .collect::<Vec<_>>()
                .join(" or ");
            format!(
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
            let iface_list = ambiguous_iface_list(sources);
            let hint = sources
                .iter()
                .map(|n| format!("obj.as<{n}>.{field_name}"))
                .collect::<Vec<_>>()
                .join(" or ");
            format!(
                "field `{field_name}` on class `{class_name}` is ambiguous because it is declared by multiple interfaces: {iface_list}; use {hint}"
            )
        }
        TirTypeError::InterfaceFieldRequiresProjection {
            class_name,
            field_name,
            interface_name,
        } => format!(
            "field `{field_name}` is an interface field on `{interface_name}`, not a concrete field on class `{class_name}`; use obj.as<{interface_name}>.{field_name}"
        ),
        TirTypeError::InterfaceFieldRequiresQualifiedConstruction {
            field_name,
            qualified_name,
        } => format!(
            "interface-qualified field `{field_name}` cannot be used in a class constructor; use class field `{qualified_name}`"
        ),
        TirTypeError::DeprecatedInterfaceProjection {
            interface_name,
            as_target,
        } => format!(
            "interface projection uses `.as<{as_target}>`, not `.{interface_name}`"
        ),
        TirTypeError::InvalidInterfaceUpcastTarget { target } => {
            format!("`.as<T>` requires an interface target, got {}", render_ty(target))
        }
        TirTypeError::InterfaceMemberRequiresReceiver {
            interface_name,
            member_name,
        } => format!(
            "interface member `{member_name}` on `{interface_name}` must be accessed through a value; use value.as<{interface_name}>.{member_name}"
        ),
        TirTypeError::InvalidSelfCallThroughInterface {
            interface_name,
            method_name,
        } => format!(
            "method `{method_name}` on interface `{interface_name}` uses `Self` in \
             its parameters and requires a concrete receiver"
        ),
        TirTypeError::DefaultOnRequiredMethod {
            interface_name,
            method_name,
        } => format!(
            "`default.{method_name}()` is invalid: method `{method_name}` on interface \
             `{interface_name}` has no default body"
        ),
        TirTypeError::BareDefaultKeyword => {
            "`default` may only be used to call an interface default method, as \
             `default.method(...)`".to_string()
        }
        TirTypeError::TypeDoesNotImplementInterface {
            value_type,
            interface_name,
        } => format!(
            "type `{}` does not implement interface `{interface_name}`",
            render_ty(value_type)
        ),
        TirTypeError::BlanketBoundNotSatisfied { value_type, bound } => format!(
            "type `{}` does not satisfy the bound `{}` required by the blanket \
             `implements` rule",
            render_ty(value_type),
            render_ty(bound)
        ),
        TirTypeError::AmbiguousInterfaceInstantiation {
            class_name,
            interface,
        } => format!(
            "class `{class_name}` implements `{}` through more than one `implements` block at \
             this instantiation (distinct generic blocks collapse to the same type); the \
             projection is ambiguous",
            render_ty(interface)
        ),
    }
}

impl fmt::Display for TirTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format_tir_type_error(self, Ty::render_user_facing))
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
/// or a raw source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticLocation {
    Expr(ExprId),
    /// The member-name portion of a `MemberAccess` expression (after the dot).
    ExprMember(ExprId),
    /// A specific segment of a multi-segment `Path` expression.
    /// `ExprSegment(path_id, segment_idx)` resolves to `path_segment_span(path_id, segment_idx)`.
    ExprSegment(ExprId, usize),
    Stmt(StmtId),
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

impl TypeCheckDiagnostics<'_> {
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

// ── InferContext ─────────────────────────────────────────────────────────────

/// Diagnostic sink for a single scope inference run.
///
/// Held inside `TypeInferenceBuilder` — one per `infer_scope_types` call.
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

    /// Toggle suppression of diagnostics that arise from synthesized
    /// references for the current inference run. See
    /// `suppress_member_lookup_errors`.
    pub fn set_suppress_member_lookup_errors(&self, value: bool) {
        self.suppress_member_lookup_errors.set(value);
    }

    pub fn db(&self) -> &'db dyn crate::Db {
        self.db
    }

    pub fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    /// Push a diagnostic with the given severity, primary location, and related notes.
    fn push(
        &self,
        error: TirTypeError,
        severity: DiagnosticSeverity,
        primary: DiagnosticLocation,
        related: Vec<RelatedNote<'db>>,
    ) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity,
                primary,
                related,
            });
    }

    /// Push an `Error`-level diagnostic, applying the synthesized-code
    /// suppression guard (see `suppress_member_lookup_errors`).
    fn push_error(
        &self,
        error: TirTypeError,
        primary: DiagnosticLocation,
        related: Vec<RelatedNote<'db>>,
    ) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.push(error, DiagnosticSeverity::Error, primary, related);
    }

    /// Report a type error at a specific expression, with optional related locations.
    pub fn report(&self, error: TirTypeError, at: ExprId, related: Vec<RelatedNote<'db>>) {
        self.push_error(error, DiagnosticLocation::Expr(at), related);
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
        self.push_error(error, DiagnosticLocation::ExprMember(at), related);
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
        self.push_error(
            error,
            DiagnosticLocation::ExprSegment(at, segment_idx),
            related,
        );
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
        self.push_error(error, DiagnosticLocation::Span(span), related);
    }

    /// Report a warning-level diagnostic at the given location.
    pub fn report_warning(&self, error: TirTypeError, loc: DiagnosticLocation) {
        self.push(error, DiagnosticSeverity::Warning, loc, Vec::new());
    }

    /// Consume the context and return accumulated diagnostics.
    pub fn finish(self) -> TypeCheckDiagnostics<'db> {
        self.diagnostics.into_inner()
    }
}
