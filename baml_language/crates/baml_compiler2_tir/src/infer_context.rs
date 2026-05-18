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

use crate::{
    ty::Ty,
    user_facing::{humanize_ty, humanize_type_names},
};

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
    /// Wrong number of type arguments for a generic class.
    WrongNumberOfTypeArgs {
        class_name: Name,
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
}

impl fmt::Display for TirTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TirTypeError::TypeMismatch { expected, got } => {
                write!(
                    f,
                    "type mismatch: expected {}, got {}",
                    humanize_ty(expected),
                    humanize_ty(got)
                )
            }
            TirTypeError::UnresolvedMember { base_type, member } => {
                write!(
                    f,
                    "type `{}` has no member `{member}`",
                    humanize_ty(base_type)
                )
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
            TirTypeError::NotCallable { ty } => {
                write!(
                    f,
                    "`{}` is not a function — it cannot be called",
                    humanize_ty(ty)
                )
            }
            TirTypeError::NotIterable { ty } => {
                write!(f, "cannot iterate over type `{}`", humanize_ty(ty))
            }
            TirTypeError::NotIndexable { ty } => {
                write!(f, "type `{}` is not indexable", humanize_ty(ty))
            }
            TirTypeError::InvalidBinaryOp { op, lhs, rhs } => {
                write!(
                    f,
                    "operator `{op:?}` cannot be applied to `{}` and `{}`",
                    humanize_ty(lhs),
                    humanize_ty(rhs)
                )
            }
            TirTypeError::InvalidUnaryOp { op, operand } => {
                write!(
                    f,
                    "operator `{op:?}` cannot be applied to `{}`",
                    humanize_ty(operand)
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
                write!(f, "missing return: expected `{}`", humanize_ty(expected))
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
                    humanize_ty(scrutinee_type),
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
                humanize_ty(first_type),
                humanize_ty(other_type)
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
            TirTypeError::InvalidCatchBindingType { type_name } => write!(
                f,
                "invalid catch binding type `{type_name}`; use a concrete type instead"
            ),
            TirTypeError::ThrowsContractViolation {
                declared,
                extra_types,
            } => {
                let extra_types = humanize_type_names(extra_types.iter().map(String::as_str));
                write!(
                    f,
                    "throws contract violation: `{}` is missing {}",
                    humanize_ty(declared),
                    extra_types.join(", ")
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
                    humanize_ty(declared)
                )?;
                if let Some(concrete_throws) = concrete_throws {
                    write!(
                        f,
                        "Add `throws {}` to the callback, catch the call, or make the callback non-throwing.",
                        humanize_ty(concrete_throws)
                    )
                } else {
                    write!(
                        f,
                        "Add an explicit `throws` to the callback, catch the call, or make the callback non-throwing."
                    )
                }
            }
            TirTypeError::ExtraneousThrowsDeclaration { extra_types } => {
                let extra_types = humanize_type_names(extra_types.iter().map(String::as_str));
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
                class_name,
                expected,
                got,
            } => {
                write!(
                    f,
                    "class `{class_name}` expects {expected} type argument(s), got {got}"
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
