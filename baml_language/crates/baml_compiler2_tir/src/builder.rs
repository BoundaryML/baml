//! Per-scope type inference builder.
//!
//! `TypeInferenceBuilder` is the mutable accumulator during a single scope's
//! type inference run. It walks the `ExprBody` arena for expressions belonging
//! to the scope being analyzed, recording inferred types in `expressions`.
//!
//! Implements bidirectional type checking:
//! - `infer_expr` (synthesis): compute type bottom-up
//! - `check_expr` (checking): verify against expected type top-down
//! - `check_stmt`: type-check a statement
//!
//! Key invariant: when encountering a lambda expression body, the builder does
//! NOT recurse into it — lambda bodies are separate scopes with their own
//! `infer_scope_types` Salsa query.

use std::collections::{BTreeSet, HashMap};

use baml_base::Name;
use baml_compiler2_ast::{
    self as ast, AstSourceMap, Expr, ExprBody, ExprId, PatId, Stmt, StmtId, TypeExpr,
};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
    scope::{FileScopeId, ScopeId},
};
use rustc_hash::{FxHashMap, FxHashSet};
use text_size::TextRange;

use crate::{
    infer_context::{
        InferContext, RelatedLocation, RelatedNote, TirTypeError, TypeCheckDiagnostics,
    },
    package_interface::PackageResolutionContext,
    throws_analysis::ThrowsAnalysisContext,
    ty::{Freshness, FunctionParamMode, FunctionParamTy, PrimitiveType, Ty, TyAttr},
};

// ── Well-known type constructors ──────────────────────────────────────────────
//
// These helpers construct `Ty` values for well-known types that appear in
// synthesized method signatures (e.g., the universal `to_json`/`from_json` on
// `Ty::TypeVar`). They are free functions so they can be called from both
// `resolve_member` (mutable context) and `try_resolve_member_on_ty` (shared).

/// Construct `Ty::TypeAlias` for `baml.json.json`.
fn json_alias_ty() -> Ty {
    Ty::TypeAlias(
        crate::ty::QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("json")],
            Name::new("json"),
        ),
        TyAttr::default(),
    )
}

/// Construct `Ty::Class` for `baml.json.JsonSerializationError`.
fn json_serialization_error_ty() -> Ty {
    Ty::Class(
        crate::ty::QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("json")],
            Name::new("JsonSerializationError"),
        ),
        vec![],
        TyAttr::default(),
    )
}

/// Construct `Ty::Class` for `baml.json.JsonParseError`.
fn json_parse_error_ty() -> Ty {
    Ty::Class(
        crate::ty::QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("json")],
            Name::new("JsonParseError"),
        ),
        vec![],
        TyAttr::default(),
    )
}

/// Construct the throws type `JsonSerializationError | JsonParseError`.
///
/// Used as the conservative throws clause for the universal `to_json` method on
/// `Ty::TypeVar` — it is a superset of any concrete type's actual throws, so
/// call-site throw inference stays sound.
fn json_serialization_or_parse_error_ty() -> Ty {
    Ty::Union(
        vec![json_serialization_error_ty(), json_parse_error_ty()],
        TyAttr::default(),
    )
}

/// Construct the throws type `JsonParseError | JsonSerializationError`.
///
/// Used as the conservative throws clause for the universal `from_json` method
/// on `Ty::TypeVar`.
fn json_parse_or_serialization_error_ty() -> Ty {
    // Same members, different ordering to match the semantic direction of
    // each method. Could be the same union; keep separate for clarity.
    Ty::Union(
        vec![json_parse_error_ty(), json_serialization_error_ty()],
        TyAttr::default(),
    )
}

/// Format an f64 as a string suitable for a float literal.
/// Returns `None` for non-finite values (inf, NaN).
fn format_float(v: f64) -> Option<String> {
    if !v.is_finite() {
        return None;
    }
    let s = format!("{v}");
    // Ensure the string always has a decimal point so it reads as float.
    if s.contains('.') {
        Some(s)
    } else {
        Some(format!("{v}.0"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum PatternMatchStrength {
    NoMatch,
    MayMatch,
    DefiniteMatch,
}

#[derive(Debug, Default, Clone)]
struct ThrowPatternMatches {
    may_match: BTreeSet<Ty>,
    definitely_handled: BTreeSet<Ty>,
}

struct IrrefutablePatternContext {
    context: IrrefutableContextKind,
    fallback_expr: Option<ExprId>,
}

/// Where a refutable pattern is being rejected. Used to make the
/// `RefutablePatternInLet` diagnostic's prose specific to the binding form.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

/// Cache key discriminator for [`TypeInferenceBuilder::pattern_natural_type`].
/// The function takes an `unconstrained` `Ty` to substitute at leaves; in
/// practice it's always one of these two, so the cache is keyed on the
/// discriminator rather than the full `Ty`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum NaturalKind {
    /// `Ty::Unknown` at unconstrained leaves — union dispatch / opaque-scrut
    /// path, where unconstrained patterns target every member.
    Unknown,
    /// `Ty::Never` at unconstrained leaves — strict subtype check, where
    /// unconstrained patterns trivially pass via `Never <: anything`.
    Never,
}

#[derive(Debug, Clone)]
enum PatternExpectedTy {
    Full(Ty),
    Partial(Ty),
}

impl PatternExpectedTy {
    fn ty(&self) -> &Ty {
        match self {
            Self::Full(ty) | Self::Partial(ty) => ty,
        }
    }

    fn into_ty(self) -> Ty {
        match self {
            Self::Full(ty) | Self::Partial(ty) => ty,
        }
    }
}

struct CallbackThrowProvenance {
    callback_name: Name,
    forwarding_call_expr: ExprId,
    callback_value_expr: Option<ExprId>,
    callback_concrete_throws: Option<Ty>,
}

struct ScopedLocalsSnapshot {
    locals: FxHashMap<Name, LocalBinding>,
    scoped_local_declarations_len: usize,
    scoped_local_assignments_len: usize,
}

#[derive(Clone)]
pub(crate) struct LocalBinding {
    pub(crate) current_ty: Ty,
    pub(crate) declared_ty: Option<Ty>,
    pub(crate) pattern: Option<PatId>,
}

struct ScopedLocalDeclaration {
    name: Name,
    /// The pattern of this declaration. Used by `restore_scoped_locals_inner`
    /// to identify "inner" bindings (those declared in the closing scope) so
    /// assignments to inner bindings can be filtered out — Slack rule 3 vs
    /// rule 2. The pattern (rather than name) is needed to distinguish
    /// inner-shadow assignments from outer-binding assignments.
    pattern: PatId,
    previous_binding: Option<LocalBinding>,
}

/// One entry in `scoped_local_assignments`: a per-name assignment recorded
/// during type inference. `pattern` carries the binding identity at the
/// assignment site:
///   - `Some(PatId)` means the assignment targets a let-binding's pattern
///     — used to distinguish inner-shadow assignments (drop on scope exit)
///     from outer-binding assignments (propagate).
///   - `None` means the name has no let-binding pattern in scope (e.g. a
///     function parameter assignment). These are always treated as
///     outer-scope and propagate on scope exit.
#[derive(Clone)]
struct ScopedAssignment {
    name: Name,
    pattern: Option<PatId>,
}

struct BuilderThrowsAnalysis<'a, 'db> {
    builder: &'a TypeInferenceBuilder<'db>,
}

impl ThrowsAnalysisContext for BuilderThrowsAnalysis<'_, '_> {
    fn expression_type(&self, expr_id: ExprId) -> Option<Ty> {
        self.builder.expressions.get(&expr_id).cloned()
    }

    fn catch_residual_throws(&self, expr_id: ExprId) -> Option<BTreeSet<Ty>> {
        self.builder.catch_residual_throws.get(&expr_id).cloned()
    }

    fn instantiated_callee_throws(
        &self,
        callee_expr_id: ExprId,
        args: &[ExprId],
        unwrap_optional_callee: bool,
    ) -> Option<Ty> {
        let call_plan = self
            .builder
            .call_plans
            .values()
            .find(|plan| plan.matches_provided_args(args));
        self.builder.instantiated_callee_throws(
            callee_expr_id,
            args,
            unwrap_optional_callee,
            call_plan,
        )
    }

    fn named_callee_summary(
        &self,
        callee_expr_id: ExprId,
        body: &ExprBody,
    ) -> Option<BTreeSet<Ty>> {
        let target = self.builder.call_target_name(callee_expr_id, body)?;
        self.builder.lookup_named_throw_summary(&target)
    }
}

/// Result of resolving a member on a builtin class (Array, Map, String, media types).
/// Distinguishes methods (which have locs) from fields (which are just types).
enum BuiltinResolution<'db> {
    Method {
        ty: Ty,
        class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    },
    Field(Ty),
}

impl BuiltinResolution<'_> {
    fn into_ty(self) -> Ty {
        match self {
            BuiltinResolution::Method { ty, .. } => ty,
            BuiltinResolution::Field(ty) => ty,
        }
    }
}

struct OptionalBaseInfo {
    expanded: Ty,
    inner: Ty,
}

impl OptionalBaseInfo {
    fn is_nullable(&self) -> bool {
        self.expanded != self.inner
    }

    fn is_null_only(&self) -> bool {
        matches!(self.inner, Ty::Never { .. })
    }
}

/// Result from the shared call pipeline. Callers use this to decide
/// expression-specific postprocessing (subtype checks, optional wrapping).
struct CheckedCallInner {
    /// The inferred return type after generic substitution and typevar erasure.
    result: Ty,
    /// True only when bindings were derived from inference driven by the
    /// caller's expected type (Phase 0 reverse inference). Callers use this
    /// to suppress a redundant result-vs-expected diagnostic, since by
    /// construction the result equals the expected type in that case.
    /// Bindings supplied via explicit type args at the call site (e.g.
    /// `foo<int>(x)`) do NOT set this flag — those bindings are independent
    /// of the expected type, so the result-vs-expected check is still
    /// meaningful.
    bindings_from_inference: bool,
}

#[derive(Clone, Copy)]
struct CallContext<'a> {
    expr_id: ExprId,
    args: &'a [ExprId],
    call_args: Option<&'a [ast::CallArg]>,
    body: &'a ExprBody,
    expected: &'a Ty,
}

struct CallCheckRequest<'a> {
    context: CallContext<'a>,
    callee_ty: Ty,
    is_method_call: bool,
    is_optional_call: bool,
    /// Pre-computed type-arg bindings when explicit `<T1, T2, ...>` were written at the call
    /// site. `Some(map)` means the caller already validated arity and resolved each `TypeExpr`;
    /// `None` means use the existing forward/reverse inference paths.
    explicit_type_arg_bindings: Option<FxHashMap<Name, Ty>>,
}

#[derive(Clone, Copy)]
struct OptionalCallContext<'a> {
    call: CallContext<'a>,
    callee_id: ExprId,
    is_method_call: bool,
}

/// Per-scope inference builder.
///
/// Created at the start of `infer_scope_types`, discarded when done.
/// Modeled after Ty's `TypeInferenceBuilder`.
pub struct TypeInferenceBuilder<'db> {
    /// Diagnostic sink.
    context: InferContext<'db>,
    /// Expression types being built up.
    expressions: FxHashMap<ExprId, Ty>,
    /// Pattern types: the type each pattern is associated with. Two distinct
    /// roles, both keyed by the pattern's `PatId`:
    ///   - `Pattern::Bind`: the type bound to the variable (post widening /
    ///     annotation), exposed downstream so name resolution can answer
    ///     "what type is this variable?".
    ///   - `Pattern::Type` / `Pattern::Class`: the type the pattern tests
    ///     against at runtime. MIR uses this to choose between an `==`
    ///     constant test (for literal/null/enum-variant types) or an
    ///     `IsType` shape test (for primitives/classes/etc.).
    pattern_types: FxHashMap<PatId, Ty>,
    /// Memoized [`Self::pattern_natural_type`] results, keyed by `PatId` and
    /// the choice of "unconstrained leaf" sentinel (`Unknown` for union
    /// dispatch, `Never` for the strict subtype check). The function walks
    /// the pattern AST and resolves any class/type names through silent
    /// resolution — no side effects — so memoization is safe and saves
    /// O(depth) re-walks per `analyze_and_lower` recursion (the same `pat_id`
    /// is queried by `check_pattern_vs_scrut_subtype`,
    /// `union_targets_for_pattern`, and the opaque-scrut dispatch).
    pattern_natural_cache: FxHashMap<(PatId, NaturalKind), Ty>,
    /// Local variable bindings. Each entry keeps the flow-sensitive type used
    /// for reads, the stable assignment contract from an explicit annotation,
    /// and the optional pattern identity for scoped assignment propagation.
    locals: FxHashMap<Name, LocalBinding>,
    /// Per-declaration restore points for active name-keyed lookup maps.
    ///
    /// A lexical scope exit must remove declarations introduced inside that
    /// scope, but it must restore a shadowed name to the state immediately
    /// before the shadowing declaration rather than to scope entry. That keeps
    /// earlier outer assignments in the same scope visible after the block.
    scoped_local_declarations: Vec<ScopedLocalDeclaration>,
    /// Assignments whose active local type was updated by assignment or
    /// container establishment. Tracked by binding identity (`PatId`) so
    /// scope-restore can filter inner-shadow assignments (which must NOT
    /// propagate — rule 3) from outer-binding assignments (which MUST
    /// propagate — rule 2).
    scoped_local_assignments: Vec<ScopedAssignment>,
    /// Member resolutions: for field-access expressions that resolved to a
    /// class field, enum variant, method, or free function — records the
    /// structural path so MIR can emit the correct `QualifiedName` and LSP
    /// can navigate to the definition.
    resolutions: FxHashMap<ExprId, crate::inference::MemberResolution<'db>>,
    /// Resolution context: own `PackageItems` + dependency `PackageInterfaces`.
    res_ctx: &'db PackageResolutionContext<'db>,
    /// Convenience: own package items (from `res_ctx`).
    package_items: &'db PackageItems<'db>,
    /// Current package ID (for throw-set queries).
    package_id: PackageId<'db>,
    /// The scope being analyzed (kept for future use).
    #[allow(dead_code)]
    scope: ScopeId<'db>,
    /// Declared return type for the function (used to check return statements).
    declared_return_ty: Option<Ty>,
    /// Resolved type alias map: alias qualified name → expanded Ty.
    /// Used by the normalizer for structural subtype checking.
    aliases: HashMap<crate::ty::QualifiedTypeName, Ty>,
    /// Namespace path for the file being analyzed (e.g. `["env"]` for `baml/env.baml`).
    ns_context: Vec<Name>,
    /// Residual throw facts for each catch expression after applying all clauses.
    catch_residual_throws: FxHashMap<ExprId, BTreeSet<Ty>>,
    /// Match expressions that the exhaustiveness checker determined cover all cases.
    exhaustive_matches: FxHashSet<ExprId>,
    /// Generic type parameters in scope for this function (e.g. `["T"]` for
    /// `function foo<T>(...)`). Used when lowering type annotations inside the
    /// function body so that `T` resolves to `Ty::TypeVar("T", TyAttr::default())` rather than
    /// `Ty::Unknown`.
    pub generic_params: Vec<Name>,
    /// Source map for the body being analyzed. Set by `infer_scope_types`
    /// before checking. Used to resolve `PatId` → `TextRange` when emitting
    /// pattern-position diagnostics.
    body_source_map: Option<AstSourceMap>,
    /// Depth counter for `OptionalChain` scopes. When > 0, `FieldAccess` and
    /// `Index` auto-unwrap nullable bases (null is caught by the chain wrapper).
    /// When 0, accessing a member on a nullable type is a type error.
    in_optional_chain: usize,
    /// TIR-inferred type of the root (first) segment for each multi-segment
    /// `Path` expression. Populated in `infer_path` so that MIR lowering can
    /// chain field projections even when the MIR local was declared with a
    /// coarser type (e.g. catch variables are declared as `BuiltinUnknown`).
    pub path_root_types: FxHashMap<ExprId, Ty>,
    /// TIR-inferred type of every prefix `segments[..=i]` for multi-segment
    /// local-rooted `Path` expressions. Index `0` matches `path_root_types`;
    /// later indices give the type of each chained field access. MIR uses this
    /// to thread class-level type args from the receiver-prefix (segment
    /// `len-2`) of method-call paths like `holder.box.describe()`.
    pub path_segment_types: FxHashMap<(ExprId, usize), Ty>,
    /// Per-segment member resolutions for multi-segment local-rooted `Path`
    /// expressions. Populated by `infer_local_rooted_path`.
    pub path_member_resolutions: FxHashMap<ExprId, Vec<crate::inference::MemberResolution<'db>>>,
    /// Parameter types for this scope (populated for lambda/function scopes).
    /// Used by LSP to resolve lambda parameter types.
    pub param_types: Vec<(Name, Ty)>,
    /// Full parameter binding plans for checked call expressions.
    pub call_plans: FxHashMap<ExprId, crate::inference::CallPlan>,
    /// Function adapters required by checked optional-parameter coercions.
    pub function_coercions: FxHashMap<ExprId, crate::inference::FunctionCoercion>,
    /// Metadata produced while checking parameter defaults. Kept separate from
    /// the function body because defaults use their own expression arena.
    default_parameter_inference: crate::inference::DefaultParameterInference<'db>,
    /// Accumulates `FileScopeId → Ty::Function` for every lambda expression
    /// encountered during inline body inference (including nested lambdas).
    /// NOT saved/restored by `infer_lambda_body`, so types from arbitrarily
    /// nested lambdas are visible in the outermost (Function/Let) scope.
    pub nested_lambda_types: FxHashMap<FileScopeId, Ty>,
    /// Diagnostic-only concrete escaping throws for lambda expressions in the
    /// current scope. Used to explain callback forwarding without affecting
    /// call instantiation or throws checking semantics.
    lambda_effective_throws: FxHashMap<ExprId, Ty>,
    /// `true` when the function being analyzed is auto-derived (e.g.
    /// synthesized `to_json` / `from_json`).  Suppresses diagnostics from
    /// type-arg lowering — auto-derive references field types verbatim, and
    /// when those types are user-broken (parser error recovery, typos) we
    /// don't want to surface synthetic-call type-arg errors that look like the
    /// user wrote them.  Real type errors still surface from the user's
    /// own field declaration.
    is_auto_derived_body: bool,
}

impl<'db> TypeInferenceBuilder<'db> {
    fn snapshot_scoped_locals(&self) -> ScopedLocalsSnapshot {
        ScopedLocalsSnapshot {
            locals: self.locals.clone(),
            scoped_local_declarations_len: self.scoped_local_declarations.len(),
            scoped_local_assignments_len: self.scoped_local_assignments.len(),
        }
    }

    fn restore_scoped_locals(&mut self, snapshot: &ScopedLocalsSnapshot) {
        self.restore_scoped_locals_inner(snapshot);
    }

    fn restore_scoped_locals_inner(&mut self, snapshot: &ScopedLocalsSnapshot) {
        // Pull the new assignments and declarations introduced since the
        // snapshot. We filter assignments by binding identity below, so the
        // names alone are not enough.
        let new_assignments: Vec<ScopedAssignment> = self
            .scoped_local_assignments
            .split_off(snapshot.scoped_local_assignments_len);
        let scoped_declarations = self
            .scoped_local_declarations
            .split_off(snapshot.scoped_local_declarations_len);

        // The PatIds of bindings declared inside the closing scope. An
        // assignment whose pattern is in this set targets an inner shadow
        // and must NOT propagate to the outer scope (Slack rule 3).
        let inner_pat_ids: FxHashSet<PatId> = scoped_declarations
            .iter()
            .map(|declaration| declaration.pattern)
            .collect();

        // Filter assignments: keep those that target a binding declared in an
        // outer scope (or have no pattern, meaning a parameter assignment —
        // always propagated).
        let kept_assignments: Vec<ScopedAssignment> = new_assignments
            .into_iter()
            .filter(|assignment| match assignment.pattern {
                Some(pat) => !inner_pat_ids.contains(&pat),
                None => true,
            })
            .collect();
        let assigned_names: FxHashSet<Name> = kept_assignments
            .iter()
            .map(|assignment| assignment.name.clone())
            .collect();

        // Roll back inner declarations: each declaration's previous binding
        // captures the full local state immediately before the declaration.
        // Walking declarations in reverse restores the outer snapshot — except
        // where a kept (outer) assignment updated the same name, which we
        // preserve in the locals loop below.
        for declaration in scoped_declarations.into_iter().rev() {
            Self::restore_map_entry(
                &mut self.locals,
                declaration.name,
                declaration.previous_binding,
            );
        }

        let local_names = self
            .locals
            .keys()
            .chain(snapshot.locals.keys())
            .cloned()
            .collect::<FxHashSet<_>>();
        for name in local_names {
            if assigned_names.contains(&name) {
                continue;
            }
            Self::restore_map_entry(
                &mut self.locals,
                name.clone(),
                snapshot.locals.get(&name).cloned(),
            );
        }

        // Re-extend the outer scope's assignment record with the kept
        // (outer-targeting) assignments so a further enclosing scope's
        // restore can also see them.
        self.scoped_local_assignments.extend(kept_assignments);
    }

    fn restore_map_entry<T>(map: &mut FxHashMap<Name, T>, name: Name, previous: Option<T>) {
        if let Some(previous) = previous {
            map.insert(name, previous);
        } else {
            map.remove(&name);
        }
    }

    fn declare_scoped_local(
        &mut self,
        name: Name,
        pattern: PatId,
        ty: Ty,
        declared_ty: Option<Ty>,
    ) {
        self.scoped_local_declarations.push(ScopedLocalDeclaration {
            previous_binding: self.locals.get(&name).cloned(),
            name: name.clone(),
            pattern,
        });

        self.locals.insert(
            name,
            LocalBinding {
                current_ty: ty,
                declared_ty,
                pattern: Some(pattern),
            },
        );
    }

    fn assign_local(&mut self, name: Name, ty: Ty) {
        // Resolve the binding identity at the assignment site. Let bindings
        // carry a pattern id; params/captures have none and always propagate
        // through scope restore as outer assignments.
        let pattern = self.locals.get(&name).and_then(|binding| binding.pattern);
        if let Some(binding) = self.locals.get_mut(&name) {
            binding.current_ty = ty;
        } else {
            self.locals.insert(
                name.clone(),
                LocalBinding {
                    current_ty: ty,
                    declared_ty: None,
                    pattern: None,
                },
            );
        }
        self.scoped_local_assignments
            .push(ScopedAssignment { name, pattern });
    }

    pub fn new(
        context: InferContext<'db>,
        res_ctx: &'db PackageResolutionContext<'db>,
        package_id: PackageId<'db>,
        scope: ScopeId<'db>,
        aliases: HashMap<crate::ty::QualifiedTypeName, Ty>,
    ) -> Self {
        let db = context.db();
        let package_items = &res_ctx.own_items;
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, scope.file(db));
        let ns_context = pkg_info.namespace_path;
        Self {
            context,
            expressions: FxHashMap::default(),
            pattern_types: FxHashMap::default(),
            pattern_natural_cache: FxHashMap::default(),
            locals: FxHashMap::default(),
            scoped_local_declarations: Vec::new(),
            scoped_local_assignments: Vec::new(),
            body_source_map: None,
            resolutions: FxHashMap::default(),
            res_ctx,
            package_items,
            package_id,
            scope,
            declared_return_ty: None,
            aliases,
            ns_context,
            catch_residual_throws: FxHashMap::default(),
            exhaustive_matches: FxHashSet::default(),
            generic_params: Vec::new(),
            in_optional_chain: 0,
            path_root_types: FxHashMap::default(),
            path_segment_types: FxHashMap::default(),
            path_member_resolutions: FxHashMap::default(),
            param_types: Vec::new(),
            call_plans: FxHashMap::default(),
            function_coercions: FxHashMap::default(),
            default_parameter_inference: crate::inference::DefaultParameterInference::empty(),
            nested_lambda_types: FxHashMap::default(),
            lambda_effective_throws: FxHashMap::default(),
            is_auto_derived_body: false,
        }
    }

    /// Set the generic type parameters for this function scope.
    /// Install the source map for the body being analyzed. Used to resolve
    /// `PatId` → `TextRange` for pattern-position diagnostic spans.
    pub fn set_body_source_map(&mut self, sm: AstSourceMap) {
        self.body_source_map = Some(sm);
    }

    pub fn set_generic_params(&mut self, params: Vec<Name>) {
        self.generic_params = params;
    }

    /// Mark this builder as analyzing an auto-derived function body. See
    /// the `is_auto_derived_body` field comment for what this gates.
    /// Also toggles the corresponding suppression flag on the diagnostic
    /// context so member-lookup errors emitted from `MemberAccess` lowering
    /// (e.g. `self.<f>.to_json()` where `f`'s type is malformed) are
    /// silenced — the user's underlying field-type error already covers it.
    pub fn set_auto_derived(&mut self, value: bool) {
        self.is_auto_derived_body = value;
        self.context.set_suppress_member_lookup_errors(value);
    }

    /// Finish building and return the accumulated results.
    #[allow(clippy::type_complexity)]
    pub fn finish(
        self,
    ) -> (
        FxHashMap<ExprId, Ty>,
        FxHashMap<PatId, Ty>,
        FxHashMap<ExprId, crate::inference::MemberResolution<'db>>,
        FxHashMap<ExprId, BTreeSet<Ty>>,
        FxHashSet<ExprId>,
        TypeCheckDiagnostics<'db>,
        FxHashMap<ExprId, Ty>,
        FxHashMap<(ExprId, usize), Ty>,
        FxHashMap<ExprId, Vec<crate::inference::MemberResolution<'db>>>,
        Vec<(Name, Ty)>,
        FxHashMap<ExprId, crate::inference::CallPlan>,
        FxHashMap<ExprId, crate::inference::FunctionCoercion>,
        FxHashMap<FileScopeId, Ty>,
        crate::inference::DefaultParameterInference<'db>,
    ) {
        let diagnostics = self.context.finish();
        (
            self.expressions,
            self.pattern_types,
            self.resolutions,
            self.catch_residual_throws,
            self.exhaustive_matches,
            diagnostics,
            self.path_root_types,
            self.path_segment_types,
            self.path_member_resolutions,
            self.param_types,
            self.call_plans,
            self.function_coercions,
            self.nested_lambda_types,
            self.default_parameter_inference,
        )
    }

    /// Set the declared return type (for return statement checking).
    pub fn set_return_type(&mut self, ty: Ty) {
        self.declared_return_ty = Some(ty);
    }

    /// Report a type error at a raw source span (for type annotations).
    pub fn report_at_span(&self, error: TirTypeError, span: text_size::TextRange) {
        self.context.report_at_span(error, span);
    }

    /// Add a local variable binding (e.g. function parameters).
    ///
    /// Also records the declared type (parameters always have annotations).
    /// Uses `entry().or_insert()` so repeated calls (e.g. from narrowing
    /// save/restore) don't overwrite the original declared type.
    ///
    /// Function and lambda parameters do not have AST `PatId`s, so they
    /// cannot flow through `declare_scoped_local`. Their assignments are
    /// tracked separately via `ScopedAssignment { pattern: None }`.
    pub fn add_local(&mut self, name: Name, ty: Ty) {
        if let Some(binding) = self.locals.get_mut(&name) {
            if binding.declared_ty.is_none() {
                binding.declared_ty = Some(ty.clone());
            }
            binding.current_ty = ty;
        } else {
            self.locals.insert(
                name,
                LocalBinding {
                    current_ty: ty.clone(),
                    declared_ty: Some(ty),
                    pattern: None,
                },
            );
        }
    }

    /// Apply a transient type narrowing for `name` — used inside match arms
    /// to refine the scrutinee's type for the arm body. This is NOT a
    /// binding declaration: the surrounding `snapshot_scoped_locals` /
    /// `restore_scoped_locals` pair owns the rollback. Tracked
    /// assignments inside the arm body still propagate per Slack rule 2.
    ///
    /// Exists so all `self.locals` writes are named.
    fn narrow_local(&mut self, name: Name, ty: Ty) {
        if let Some(binding) = self.locals.get_mut(&name) {
            binding.current_ty = ty;
        } else {
            self.locals.insert(
                name,
                LocalBinding {
                    current_ty: ty,
                    declared_ty: None,
                    pattern: None,
                },
            );
        }
    }

    /// Seed a captured-name marker as `Ty::Unknown` to suppress false
    /// "unresolved name" diagnostics inside a lambda body. This is NOT a
    /// binding; the actual capture's type is resolved by the parent scope.
    ///
    /// Exists so all `self.locals` writes are named.
    fn seed_capture_unknown(&mut self, name: Name) {
        self.locals.insert(
            name,
            LocalBinding {
                current_ty: Ty::Unknown {
                    attr: TyAttr::default(),
                },
                declared_ty: None,
                pattern: None,
            },
        );
    }

    fn sync_let_binding_type(&mut self, name: &Name, ty: Ty) {
        if let Some(pattern_id) = self.locals.get(name).and_then(|binding| binding.pattern) {
            self.pattern_types.insert(pattern_id, ty);
        }
    }

    /// Record the type of an expression.
    pub fn record_expr_type(&mut self, expr_id: ExprId, ty: Ty) {
        self.expressions.insert(expr_id, ty);
    }

    fn expand_alias_chains(&self, ty: Ty) -> Ty {
        let mut ty = ty;
        for _ in 0..64 {
            match &ty {
                Ty::TypeAlias(qtn, _) => match self.aliases.get(qtn) {
                    Some(expanded) => ty = expanded.clone(),
                    None => break,
                },
                _ => break,
            }
        }
        ty
    }

    /// Pattern-matrix-internal normalization of a scrutinee type.
    /// Rewrites `Ty::Optional(T)` into `Ty::Union([T, null])` so the
    /// matrix's `UnionMember` dispatch applies uniformly. Optionals
    /// embedded as members of an outer Union are flattened in the same
    /// way (so `int? | string` becomes `int | null | string`), and
    /// duplicate `null` members get deduplicated.
    ///
    /// Only the matrix's column type, dpat scrut tags, and witness
    /// inputs use this form — `Ty::Optional` remains the canonical
    /// representation everywhere else (subtyping, codegen, display in
    /// non-match diagnostics, `pattern_types` map values, `matched_ty`
    /// flowing into binding inference).
    fn matrix_normalize_scrut(&self, ty: &Ty) -> Ty {
        let expanded = self.expand_alias_chains(ty.clone());
        match expanded {
            Ty::Optional(inner, _) => Ty::Union(
                vec![
                    *inner,
                    Ty::Primitive(PrimitiveType::Null, TyAttr::default()),
                ],
                TyAttr::default(),
            ),
            Ty::Union(members, attr) => {
                let mut flat: Vec<Ty> = Vec::with_capacity(members.len());
                let mut has_null = false;
                for m in members {
                    match self.expand_alias_chains(m) {
                        Ty::Optional(inner, _) => {
                            flat.push(*inner);
                            has_null = true;
                        }
                        Ty::Primitive(PrimitiveType::Null, _) => {
                            has_null = true;
                        }
                        other => flat.push(other),
                    }
                }
                if has_null {
                    flat.push(Ty::Primitive(PrimitiveType::Null, TyAttr::default()));
                }
                Ty::Union(flat, attr)
            }
            other => other,
        }
    }

    fn expected_lambda_function_ty(&self, expected: &Ty) -> Option<Ty> {
        fn peel(builder: &TypeInferenceBuilder<'_>, ty: Ty, depth: usize) -> Option<Ty> {
            if depth == 0 {
                return None;
            }

            match builder.expand_alias_chains(ty) {
                fn_ty @ Ty::Function { .. } => Some(fn_ty),
                Ty::Optional(inner, _) => peel(builder, *inner, depth - 1),
                Ty::Union(members, _) => {
                    let mut function_member = None;
                    for member in members {
                        let expanded_member = builder.expand_alias_chains(member);
                        if matches!(expanded_member, Ty::Primitive(PrimitiveType::Null, _)) {
                            continue;
                        }

                        let member_fn = peel(builder, expanded_member, depth - 1)?;
                        if function_member.is_some() {
                            return None;
                        }
                        function_member = Some(member_fn);
                    }
                    function_member
                }
                _ => None,
            }
        }

        peel(self, expected.clone(), 64)
    }

    fn lower_lambda_type_expr(
        &mut self,
        type_expr: &TypeExpr,
        generic_params: &[Name],
        span: TextRange,
    ) -> Ty {
        let mut diags = Vec::new();
        let ty = crate::lower_type_expr::lower_type_expr_in_ns(
            self.context.db(),
            type_expr,
            self.package_items,
            &self.ns_context,
            generic_params,
            &mut diags,
        );
        for diag in diags {
            self.context.report_at_span(diag, span);
        }
        ty
    }

    fn choose_lambda_throws_surface(
        &mut self,
        func_def: &baml_compiler2_ast::FunctionDef,
        generic_params: &[Name],
        contextual_throws: Option<&Ty>,
    ) -> (Ty, TextRange, bool) {
        if let Some(throws) = &func_def.throws {
            let ty = self.lower_lambda_type_expr(&throws.expr, generic_params, throws.span);
            (ty, throws.span, true)
        } else if let Some(contextual) = contextual_throws {
            (contextual.clone(), func_def.span, false)
        } else if Self::is_spawn_body_lambda(func_def) {
            // BEP-034: a `spawn { body }` body is wrapped in a synthetic
            // 0-arg lambda whose throws are captured into the resulting
            // `Future<T, E>`'s E parameter, not propagated to the
            // enclosing function. Use `Unknown` here so
            // `check_throws_surface`'s open-slot check skips the
            // declared-vs-effective comparison; the effective throws are
            // still computed and read by `infer_expr`'s Spawn arm to
            // build the Future's E.
            (
                Ty::Unknown {
                    attr: TyAttr::default(),
                },
                func_def.span,
                false,
            )
        } else {
            (
                Ty::Never {
                    attr: TyAttr::default(),
                },
                func_def.span,
                false,
            )
        }
    }

    /// Synthetic lambda produced by `lower_spawn_expr` carries the name
    /// `<spawn>`. The marker is the only safe way to distinguish a
    /// user-written `() => { ... }` from spawn's body wrapper at this
    /// layer (their `FunctionDef`s are otherwise identical).
    fn is_spawn_body_lambda(func_def: &baml_compiler2_ast::FunctionDef) -> bool {
        func_def.name.as_str() == "<spawn>"
    }

    fn throws_surface_has_open_slot(throws_facts: &BTreeSet<Ty>) -> bool {
        throws_facts.iter().any(|fact| {
            matches!(
                fact,
                Ty::TypeVar(_, _)
                    | Ty::Unknown { .. }
                    | Ty::BuiltinUnknown { .. }
                    | Ty::Error { .. }
            )
        })
    }

    fn is_synthetic_effect_param_name(name: &Name) -> bool {
        name.as_str()
            .strip_prefix("__effect_param_")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
    }

    fn synthetic_effect_param_name(fact: &Ty) -> Option<&Name> {
        match fact {
            Ty::TypeVar(name, _) if Self::is_synthetic_effect_param_name(name) => Some(name),
            _ => None,
        }
    }

    fn ty_from_concrete_facts(facts: &BTreeSet<Ty>) -> Option<Ty> {
        if facts.is_empty() || Self::throws_surface_has_open_slot(facts) {
            return None;
        }

        let mut iter = facts.iter();
        let first = iter.next()?.clone();
        Some(iter.fold(first, |acc, fact| crate::generics::union_ty(&acc, fact)))
    }

    fn callback_concrete_throws_from_expr(&self, expr_id: ExprId) -> Option<Ty> {
        if let Some(throws) = self.lambda_effective_throws.get(&expr_id) {
            let facts = crate::throw_inference::flatten_ty_to_facts(throws);
            if let Some(concrete) = Self::ty_from_concrete_facts(&facts) {
                return Some(concrete);
            }
        }

        let expr_ty = self.expressions.get(&expr_id)?;
        let function_ty = self.expected_lambda_function_ty(expr_ty)?;
        let Ty::Function { throws, .. } = function_ty else {
            return None;
        };
        let facts = crate::throw_inference::flatten_ty_to_facts(&throws);
        Self::ty_from_concrete_facts(&facts)
    }

    fn function_throws_exactly_missing_effect(&self, ty: &Ty, missing_effect_fact: &Ty) -> bool {
        let Some(function_ty) = self.expected_lambda_function_ty(ty) else {
            return false;
        };
        let Ty::Function { throws, .. } = function_ty else {
            return false;
        };
        let facts = crate::throw_inference::flatten_ty_to_facts(&throws);
        facts.len() == 1 && facts.contains(missing_effect_fact)
    }

    fn direct_callback_name(expr_id: ExprId, body: &ExprBody) -> Option<Name> {
        match &body.exprs[expr_id] {
            Expr::Path(segments) => segments.last().cloned(),
            Expr::MemberAccess { member, .. } | Expr::OptionalMemberAccess { member, .. } => {
                Some(member.clone())
            }
            _ => None,
        }
    }

    fn find_callback_throw_provenance(
        &self,
        body: &ExprBody,
        missing_effect_fact: &Ty,
    ) -> Option<CallbackThrowProvenance> {
        let mut matches = Vec::new();

        for (expr_id, expr) in body.exprs.iter() {
            let (callee_expr_id, args, unwrap_optional_callee) = match expr {
                Expr::Call { callee, args, .. } => (*callee, args.as_slice(), false),
                Expr::OptionalCall { callee, args } => (*callee, args.as_slice(), true),
                _ => continue,
            };
            let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();

            let call_plan = self.call_plans.get(&expr_id);
            let Some(call_throws) = self.instantiated_callee_throws(
                callee_expr_id,
                &arg_exprs,
                unwrap_optional_callee,
                call_plan,
            ) else {
                continue;
            };
            let call_facts = crate::throw_inference::flatten_ty_to_facts(&call_throws);
            if !call_facts.contains(missing_effect_fact) {
                continue;
            }

            let callee_ty = self
                .expressions
                .get(&callee_expr_id)
                .cloned()
                .unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            let typed_callee = if unwrap_optional_callee {
                self.analyze_optional_base(&callee_ty).inner
            } else {
                callee_ty.clone()
            };

            let mut higher_order_matches = Vec::new();
            if let Ty::Function { params, .. } = typed_callee {
                let effective_params = if self.callee_uses_method_call_convention(callee_expr_id) {
                    crate::generics::skip_self_param(&params)
                } else {
                    params.as_slice()
                };

                for (index, param) in effective_params.iter().enumerate() {
                    if !self.function_throws_exactly_missing_effect(&param.ty, missing_effect_fact)
                    {
                        continue;
                    }
                    let Some(callback_name) = param.name.clone() else {
                        continue;
                    };
                    let callback_value_expr = if let Some(call_plan) = call_plan {
                        call_plan.provided_arg_for_param(index)
                    } else {
                        arg_exprs.get(index).copied()
                    };
                    higher_order_matches.push(CallbackThrowProvenance {
                        callback_name,
                        forwarding_call_expr: expr_id,
                        callback_value_expr,
                        callback_concrete_throws: callback_value_expr.and_then(|value_expr| {
                            self.callback_concrete_throws_from_expr(value_expr)
                        }),
                    });
                }
            }

            if higher_order_matches.len() == 1 {
                matches.push(higher_order_matches.pop().expect("len checked"));
                continue;
            }

            let callee_fn_throws_missing =
                self.function_throws_exactly_missing_effect(&callee_ty, missing_effect_fact);
            if callee_fn_throws_missing
                && let Some(callback_name) = Self::direct_callback_name(callee_expr_id, body)
            {
                matches.push(CallbackThrowProvenance {
                    callback_name,
                    forwarding_call_expr: expr_id,
                    callback_value_expr: None,
                    callback_concrete_throws: None,
                });
            }
        }

        if matches.len() == 1 {
            matches.pop()
        } else {
            None
        }
    }

    fn check_throws_surface(
        &mut self,
        body: &ExprBody,
        throws_ty: &Ty,
        span: TextRange,
        warn_extraneous: bool,
    ) {
        let declared = crate::throw_inference::flatten_ty_to_facts(throws_ty);
        let effective = self.collect_effective_throws(body);
        let has_open_slot = Self::throws_surface_has_open_slot(&declared);

        let extra_facts: BTreeSet<Ty> = if has_open_slot {
            BTreeSet::new()
        } else {
            effective.difference(&declared).cloned().collect()
        };
        let mut extra: Vec<String> = extra_facts
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        let mut extraneous: Vec<String> = if warn_extraneous && !has_open_slot {
            declared
                .difference(&effective)
                .map(std::string::ToString::to_string)
                .collect()
        } else {
            Vec::new()
        };
        extra.sort();
        extraneous.sort();

        if !extra.is_empty() {
            let missing_effect_fact = extra_facts.iter().next();
            if extra_facts.len() == 1
                && let Some(missing_effect_fact) = missing_effect_fact
                && Self::synthetic_effect_param_name(missing_effect_fact).is_some()
                && let Some(provenance) =
                    self.find_callback_throw_provenance(body, missing_effect_fact)
            {
                let mut related = vec![RelatedNote::new(
                    RelatedLocation::Expr(provenance.forwarding_call_expr),
                    format!(
                        "this call forwards whatever callback `{}` throws",
                        provenance.callback_name
                    ),
                )];

                if let Some(callback_value_expr) = provenance.callback_value_expr {
                    let message = if let Some(concrete_throws) =
                        provenance.callback_concrete_throws.clone()
                    {
                        format!(
                            "this callback throws `{}`",
                            crate::user_facing::humanize_ty(&concrete_throws)
                        )
                    } else {
                        "this callback may throw".to_string()
                    };
                    related.push(RelatedNote::new(
                        RelatedLocation::Expr(callback_value_expr),
                        message,
                    ));
                }

                self.context.report_at_span_with_related(
                    TirTypeError::CallbackThrowsContractViolation {
                        callback_name: provenance.callback_name,
                        declared: throws_ty.clone(),
                        concrete_throws: provenance.callback_concrete_throws,
                    },
                    span,
                    related,
                );
            } else {
                self.context.report_at_span(
                    TirTypeError::ThrowsContractViolation {
                        declared: throws_ty.clone(),
                        extra_types: extra,
                    },
                    span,
                );
            }
        }
        if !extraneous.is_empty() {
            self.context.report_warning_at_span(
                TirTypeError::ExtraneousThrowsDeclaration {
                    extra_types: extraneous,
                },
                span,
            );
        }
    }

    fn argument_matches_expected(&self, got: &Ty, expected: &Ty) -> bool {
        if self.is_subtype(got, expected) {
            return true;
        }

        let expanded_expected = self.expand_alias_chains(expected.clone());
        let expanded_got = self.expand_alias_chains(got.clone());

        match (&expanded_expected, &expanded_got) {
            (Ty::Class(class_name, expected_args, _), Ty::List(actual_inner, _))
                if class_name.is_builtin_root_type("Array") && expected_args.len() == 1 =>
            {
                self.is_subtype(actual_inner, &expected_args[0])
            }
            (Ty::Class(class_name, expected_args, _), Ty::EvolvingList(actual_inner, _))
                if class_name.is_builtin_root_type("Array") && expected_args.len() == 1 =>
            {
                self.is_subtype(actual_inner, &expected_args[0])
            }
            (
                Ty::Class(class_name, expected_args, _),
                Ty::Map(actual_key, actual_val, _) | Ty::EvolvingMap(actual_key, actual_val, _),
            ) if class_name.is_builtin_root_type("Map") && expected_args.len() == 2 => {
                self.is_subtype(actual_key, &expected_args[0])
                    && self.is_subtype(actual_val, &expected_args[1])
            }
            _ => false,
        }
    }

    fn function_coercion_for(
        &self,
        got: &Ty,
        expected: &Ty,
    ) -> Option<crate::inference::FunctionCoercion> {
        let expanded_got = self.expand_alias_chains(got.clone());
        let expanded_expected = self.expand_alias_chains(expected.clone());
        let (
            Ty::Function {
                params: source_params,
                ..
            },
            Ty::Function {
                params: target_params,
                ret: target_return,
                ..
            },
        ) = (&expanded_got, &expanded_expected)
        else {
            return None;
        };

        if !self.is_subtype(&expanded_got, &expanded_expected) {
            return None;
        }

        if Self::function_params_runtime_compatible(source_params, target_params) {
            return None;
        }

        Some(crate::inference::FunctionCoercion {
            source_params: source_params.clone(),
            target_params: target_params.clone(),
            target_return: target_return.as_ref().clone(),
        })
    }

    fn function_params_runtime_compatible(
        source_params: &[FunctionParamTy],
        target_params: &[FunctionParamTy],
    ) -> bool {
        if source_params.len() != target_params.len() {
            return false;
        }

        source_params
            .iter()
            .zip(target_params)
            .all(|(source, target)| {
                source.mode == target.mode && (source.is_required() || source.name == target.name)
            })
    }

    fn record_function_coercion_if_needed(&mut self, expr_id: ExprId, got: &Ty, expected: &Ty) {
        if let Some(coercion) = self.function_coercion_for(got, expected) {
            self.function_coercions.insert(expr_id, coercion);
        }
    }

    fn analyze_optional_base(&self, ty: &Ty) -> OptionalBaseInfo {
        let expanded = self.expand_alias_chains(ty.clone());
        let inner = crate::narrowing::remove_null(&expanded);
        OptionalBaseInfo { expanded, inner }
    }

    fn infer_args_for_recovery(&mut self, args: &[ExprId], body: &ExprBody) {
        for arg in args {
            self.infer_expr(*arg, body);
        }
    }

    /// Resolve explicit type arguments written at a call site (e.g. `foo<int, string>(x)`).
    ///
    /// Returns `Some(bindings)` when all type args are valid, where `bindings` maps each
    /// declared type-param name to its resolved `Ty`. Returns `None` when:
    /// - The callee is not a known free function (no resolution recorded), or
    /// - The arity is wrong (a `WrongTypeArgArity` diagnostic is emitted).
    ///
    /// Emits `WrongTypeArgArity` when the count of provided type args does not match the
    /// count of declared user generic params for the callee.
    fn resolve_explicit_type_args(
        &mut self,
        callee_id: ExprId,
        type_args: &[TypeExpr],
        call_expr_id: ExprId,
    ) -> Option<FxHashMap<Name, Ty>> {
        // Look up the callee's resolution to find the declared generic param names.
        let resolution = self.resolutions.get(&callee_id).cloned()?;
        let (func_loc, treat_as_static_method) = match resolution {
            crate::inference::MemberResolution::Free { func_loc } => (func_loc, true),
            // `UnboundMethod` covers `Class.method` / `Class<...>.method` call
            // sites where the receiver is a type name.  When the call writes
            // `Class<...>.method(...)`, the receiver-type's `<...>` is parsed
            // as the call's type-args by `find_callee_generic_args` in
            // `lower_expr_body.rs`; those args fill the *enclosing class's*
            // generic params (BEP-039), so we include them in the
            // expected-arity check below.
            crate::inference::MemberResolution::UnboundMethod { func_loc, .. } => (func_loc, true),
            // BoundMethod calls (`inst.method(args)`) get class type-args
            // from the receiver instance's `class_type_args` at runtime, not
            // from the call site.
            crate::inference::MemberResolution::BoundMethod { func_loc, .. } => (func_loc, false),
            _ => return None,
        };
        let db = self.context.db();
        let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
        // Only user-declared generic params are supplied explicitly; synthetic effect params
        // are always inferred.  For static-method-on-generic-class calls, prepend the
        // class's generic params: type-args fill `[class_params..., function_params...]`.
        let class_params: Vec<Name> = if treat_as_static_method {
            let file = func_loc.file(db);
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            item_tree
                .classes
                .values()
                .find(|class_data| class_data.methods.contains(&func_loc.id(db)))
                .map(|class_data| class_data.generic_params.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut declared_params: Vec<Name> = class_params;
        declared_params.extend(sig.user_generic_params.iter().cloned());

        if type_args.len() != declared_params.len() {
            self.context.report_simple(
                TirTypeError::WrongTypeArgArity {
                    callee_name: sig.name.clone(),
                    expected: declared_params.len(),
                    got: type_args.len(),
                },
                call_expr_id,
            );
            return None;
        }

        // Resolve each type argument in the current namespace context.
        let mut bindings = FxHashMap::default();
        let ns = self.ns_context.clone();
        let caller_generic_params = self.generic_params.clone();
        let suppress_diags = self.is_auto_derived_body;
        for (param_name, type_arg_expr) in declared_params.iter().zip(type_args.iter()) {
            let mut diags = Vec::new();
            let ty = crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                type_arg_expr,
                self.package_items,
                &ns,
                &caller_generic_params,
                &mut diags,
            );
            // Auto-derived bodies (`to_json` / `from_json` synthesized by
            // `auto_derive_json`) reference field types verbatim. When a
            // class has malformed/unresolved field types (parser error
            // recovery, typos), the synthesizer's `baml.json.from_json<F>`
            // call surfaces those as type-arg-resolution errors, with spans
            // pointing at the user's source.  Suppress them here — the real
            // diagnostic comes from the user's field declaration itself.
            if !suppress_diags {
                for d in diags {
                    self.context.report_simple(d, call_expr_id);
                }
            }
            bindings.insert(param_name.clone(), ty);
        }
        Some(bindings)
    }

    fn bind_call_args<'a>(
        &mut self,
        expr_id: ExprId,
        effective_params: &'a [FunctionParamTy],
        args: &[ExprId],
        call_args: Option<&[ast::CallArg]>,
        body: &ExprBody,
    ) -> Vec<(&'a FunctionParamTy, ExprId)> {
        let mut bindings: Vec<Option<ExprId>> = vec![None; effective_params.len()];
        let mut provided_args = FxHashSet::default();
        let mut name_to_index = FxHashMap::default();
        for (index, param) in effective_params.iter().enumerate() {
            if let Some(name) = &param.name {
                name_to_index.insert(name.clone(), index);
            }
        }

        let mut next_positional = 0usize;
        let mut saw_named = false;
        let mut reported_overflow_arity = false;
        let has_named_args =
            call_args.is_some_and(|call_args| call_args.iter().any(|arg| arg.label.is_some()));
        for (arg_index, arg_expr) in args.iter().copied().enumerate() {
            let label = call_args
                .and_then(|call_args| call_args.get(arg_index))
                .and_then(|arg| arg.label.as_ref());

            if let Some(label) = label {
                saw_named = true;
                let Some(param_index) = name_to_index.get(label).copied() else {
                    self.context.report_simple(
                        TirTypeError::UnknownNamedArgument {
                            name: label.clone(),
                        },
                        arg_expr,
                    );
                    continue;
                };
                if bindings[param_index].is_some() {
                    self.context.report_simple(
                        TirTypeError::DuplicateNamedArgument {
                            name: label.clone(),
                        },
                        arg_expr,
                    );
                    continue;
                }
                bindings[param_index] = Some(arg_expr);
                provided_args.insert(arg_expr);
                continue;
            }

            if saw_named {
                self.context
                    .report_simple(TirTypeError::PositionalArgumentAfterNamed, arg_expr);
                continue;
            }

            if next_positional >= effective_params.len() {
                if !reported_overflow_arity {
                    self.context.report_simple(
                        TirTypeError::ArgumentCountMismatch {
                            expected: effective_params.len(),
                            got: args.len(),
                        },
                        expr_id,
                    );
                    reported_overflow_arity = true;
                }
                continue;
            }

            let param = &effective_params[next_positional];
            if param.is_optional() {
                self.context.report_simple(
                    TirTypeError::DefaultedParamPassedPositionally {
                        name: param
                            .name
                            .clone()
                            .expect("optional function parameters must be named"),
                    },
                    arg_expr,
                );
            }
            bindings[next_positional] = Some(arg_expr);
            provided_args.insert(arg_expr);
            next_positional += 1;
        }

        let required_count = effective_params
            .iter()
            .filter(|param| param.is_required())
            .count();
        let reported_positional_arity = !has_named_args && args.len() < required_count;
        if reported_positional_arity {
            self.context.report_simple(
                TirTypeError::ArgumentCountMismatch {
                    expected: required_count,
                    got: args.len(),
                },
                expr_id,
            );
        }

        let mut reported_anonymous_missing_arity = false;
        for (param, binding) in effective_params.iter().zip(bindings.iter()) {
            if param.is_required() && binding.is_none() {
                if reported_positional_arity {
                    continue;
                }
                if let Some(name) = &param.name {
                    self.context.report_simple(
                        TirTypeError::MissingRequiredArgument { name: name.clone() },
                        expr_id,
                    );
                } else if !reported_anonymous_missing_arity {
                    self.context.report_simple(
                        TirTypeError::ArgumentCountMismatch {
                            expected: required_count,
                            got: args.len(),
                        },
                        expr_id,
                    );
                    reported_anonymous_missing_arity = true;
                }
            }
        }

        for arg in args {
            if !provided_args.contains(arg) {
                self.infer_expr(*arg, body);
            }
        }

        let pairs: Vec<_> = effective_params
            .iter()
            .zip(bindings.iter())
            .filter_map(|(param, arg)| arg.map(|arg| (param, arg)))
            .collect();

        let plan_bindings = effective_params
            .iter()
            .enumerate()
            .filter_map(|(param_index, param)| match bindings[param_index] {
                Some(arg) => Some(crate::inference::ParamBinding::Provided { param_index, arg }),
                None if param.is_optional() => {
                    Some(crate::inference::ParamBinding::OmittedDefault {
                        param_index,
                        param_name: param
                            .name
                            .clone()
                            .expect("optional function parameters must be named"),
                    })
                }
                None => None,
            })
            .collect();
        self.call_plans.insert(
            expr_id,
            crate::inference::CallPlan {
                bindings: plan_bindings,
            },
        );

        pairs
    }

    pub fn check_function_parameter_defaults(
        &mut self,
        params: &[baml_compiler2_hir::item_tree::FunctionParam],
        parameter_defaults: &baml_compiler2_hir::signature::FunctionParameterDefaults,
        param_types: &[(Name, Ty)],
    ) {
        let mut seen_default = false;
        let saved_expressions = std::mem::take(&mut self.expressions);
        let saved_pattern_types = std::mem::take(&mut self.pattern_types);
        let saved_resolutions = std::mem::take(&mut self.resolutions);
        let saved_catch_residual_throws = std::mem::take(&mut self.catch_residual_throws);
        let saved_exhaustive_matches = std::mem::take(&mut self.exhaustive_matches);
        let saved_path_root_types = std::mem::take(&mut self.path_root_types);
        let saved_path_segment_types = std::mem::take(&mut self.path_segment_types);
        let saved_path_member_resolutions = std::mem::take(&mut self.path_member_resolutions);
        let saved_call_plans = std::mem::take(&mut self.call_plans);
        let saved_function_coercions = std::mem::take(&mut self.function_coercions);
        let saved_lambda_effective_throws = std::mem::take(&mut self.lambda_effective_throws);
        let defaults = &parameter_defaults.defaults;
        let saved_body_source_map = self.body_source_map.replace(defaults.source_map.clone());
        let saved_locals = self.locals.clone();
        let saved_scoped_local_declarations_len = self.scoped_local_declarations.len();
        let saved_scoped_local_assignments_len = self.scoped_local_assignments.len();

        for (index, param) in params.iter().enumerate() {
            let Some(default_ref) = parameter_defaults.param_default(index) else {
                if seen_default {
                    self.report_at_span(
                        TirTypeError::RequiredParamAfterDefault {
                            name: param.name.clone(),
                        },
                        param.span,
                    );
                }
                continue;
            };

            seen_default = true;

            let default_expr = default_ref.expr.expr();
            let default_span = defaults.source_map.expr_span(default_expr);

            if param.name.as_str() == "self" {
                self.report_at_span(TirTypeError::SelfParamDefault, default_span);
                continue;
            }

            let expected_ty =
                param_types
                    .get(index)
                    .map(|(_, ty)| ty.clone())
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
            let got_ty = self.infer_expr(default_expr, &defaults.exprs);
            if !matches!(expected_ty, Ty::Unknown { .. } | Ty::Error { .. })
                && !self.argument_matches_expected(&got_ty, &expected_ty)
            {
                self.report_at_span(
                    TirTypeError::TypeMismatch {
                        expected: expected_ty,
                        got: got_ty,
                    },
                    default_span,
                );
            }

            let later_params: FxHashSet<Name> = params
                .iter()
                .skip(index + 1)
                .map(|param| param.name.clone())
                .collect();
            for referenced in
                Self::default_expr_forward_references(default_expr, &defaults.exprs, &later_params)
            {
                self.report_at_span(
                    TirTypeError::DefaultParamForwardReference {
                        param: param.name.clone(),
                        referenced,
                    },
                    default_span,
                );
            }
        }

        self.default_parameter_inference = crate::inference::DefaultParameterInference {
            expressions: std::mem::take(&mut self.expressions),
            pattern_types: std::mem::take(&mut self.pattern_types),
            resolutions: std::mem::take(&mut self.resolutions),
            catch_residual_throws: std::mem::take(&mut self.catch_residual_throws),
            exhaustive_matches: std::mem::take(&mut self.exhaustive_matches),
            path_root_types: std::mem::take(&mut self.path_root_types),
            path_segment_types: std::mem::take(&mut self.path_segment_types),
            path_member_resolutions: std::mem::take(&mut self.path_member_resolutions),
            call_plans: std::mem::take(&mut self.call_plans),
            function_coercions: std::mem::take(&mut self.function_coercions),
        };

        self.expressions = saved_expressions;
        self.pattern_types = saved_pattern_types;
        self.resolutions = saved_resolutions;
        self.catch_residual_throws = saved_catch_residual_throws;
        self.exhaustive_matches = saved_exhaustive_matches;
        self.path_root_types = saved_path_root_types;
        self.path_segment_types = saved_path_segment_types;
        self.path_member_resolutions = saved_path_member_resolutions;
        self.call_plans = saved_call_plans;
        self.function_coercions = saved_function_coercions;
        self.lambda_effective_throws = saved_lambda_effective_throws;
        self.body_source_map = saved_body_source_map;
        self.locals = saved_locals;
        self.scoped_local_declarations
            .truncate(saved_scoped_local_declarations_len);
        self.scoped_local_assignments
            .truncate(saved_scoped_local_assignments_len);
    }

    fn default_expr_forward_references(
        expr_id: ExprId,
        body: &ExprBody,
        later_params: &FxHashSet<Name>,
    ) -> Vec<Name> {
        let mut shadowed = Vec::new();
        let mut refs = Vec::new();
        Self::collect_default_expr_forward_references(
            expr_id,
            body,
            later_params,
            &mut shadowed,
            &mut refs,
        );
        refs
    }

    fn collect_default_expr_forward_references(
        expr_id: ExprId,
        body: &ExprBody,
        later_params: &FxHashSet<Name>,
        shadowed: &mut Vec<Name>,
        refs: &mut Vec<Name>,
    ) {
        match &body.exprs[expr_id] {
            Expr::Path(segments) => {
                if let Some(root) = segments.first()
                    && later_params.contains(root)
                    && !shadowed.iter().rev().any(|name| name == root)
                    && !refs.contains(root)
                {
                    refs.push(root.clone());
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::collect_default_expr_forward_references(
                    *condition,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                Self::collect_default_expr_forward_references(
                    *then_branch,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                if let Some(expr) = else_branch {
                    Self::collect_default_expr_forward_references(
                        *expr,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                Self::collect_default_expr_forward_references(
                    *scrutinee,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                for arm_id in arms {
                    let arm = &body.match_arms[*arm_id];
                    let saved_len = shadowed.len();
                    Self::push_pattern_bindings(arm.pattern, body, shadowed);
                    if let Some(guard) = arm.guard {
                        Self::collect_default_expr_forward_references(
                            guard,
                            body,
                            later_params,
                            shadowed,
                            refs,
                        );
                    }
                    Self::collect_default_expr_forward_references(
                        arm.body,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    shadowed.truncate(saved_len);
                }
            }
            Expr::Is { scrutinee, .. } => {
                // The pattern has no body and its bindings don't escape, so we
                // only need to recurse into the scrutinee.
                Self::collect_default_expr_forward_references(
                    *scrutinee,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Expr::Catch { base, clauses } => {
                Self::collect_default_expr_forward_references(
                    *base,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                for clause in clauses {
                    let clause_saved_len = shadowed.len();
                    Self::push_pattern_bindings(clause.binding, body, shadowed);
                    if let Some(stack_trace_binding) = clause.stack_trace_binding {
                        Self::push_pattern_bindings(stack_trace_binding, body, shadowed);
                    }
                    for arm_id in &clause.arms {
                        let arm = &body.catch_arms[*arm_id];
                        let arm_saved_len = shadowed.len();
                        Self::push_pattern_bindings(arm.pattern, body, shadowed);
                        Self::collect_default_expr_forward_references(
                            arm.body,
                            body,
                            later_params,
                            shadowed,
                            refs,
                        );
                        shadowed.truncate(arm_saved_len);
                    }
                    shadowed.truncate(clause_saved_len);
                }
            }
            Expr::Throw { value } | Expr::Unary { expr: value, .. } => {
                Self::collect_default_expr_forward_references(
                    *value,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Expr::Binary { lhs, rhs, .. } => {
                Self::collect_default_expr_forward_references(
                    *lhs,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                Self::collect_default_expr_forward_references(
                    *rhs,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Expr::Call { callee, args, .. } | Expr::OptionalCall { callee, args } => {
                Self::collect_default_expr_forward_references(
                    *callee,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                for arg in args {
                    Self::collect_default_expr_forward_references(
                        arg.expr,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
            }
            Expr::Object {
                fields, spreads, ..
            } => {
                for (_, expr) in fields {
                    Self::collect_default_expr_forward_references(
                        *expr,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                for spread in spreads {
                    Self::collect_default_expr_forward_references(
                        spread.expr,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
            }
            Expr::Array { elements } => {
                for expr in elements {
                    Self::collect_default_expr_forward_references(
                        *expr,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
            }
            Expr::Map { entries } => {
                for (key, value) in entries {
                    Self::collect_default_expr_forward_references(
                        *key,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    Self::collect_default_expr_forward_references(
                        *value,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
            }
            Expr::Block { stmts, tail_expr } => {
                let saved_len = shadowed.len();
                for stmt in stmts {
                    Self::collect_default_stmt_forward_references(
                        *stmt,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                if let Some(expr) = tail_expr {
                    Self::collect_default_expr_forward_references(
                        *expr,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                shadowed.truncate(saved_len);
            }
            Expr::MemberAccess { base, .. }
            | Expr::OptionalMemberAccess { base, .. }
            | Expr::OptionalChain { expr: base } => {
                Self::collect_default_expr_forward_references(
                    *base,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
                Self::collect_default_expr_forward_references(
                    *base,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                Self::collect_default_expr_forward_references(
                    *index,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Expr::Lambda(func_def) => {
                let saved_len = shadowed.len();
                for param in &func_def.params {
                    shadowed.push(param.name.clone());
                }
                for param in &func_def.params {
                    if let Some(default) = param.default {
                        Self::collect_default_expr_forward_references(
                            default.expr(),
                            &func_def.defaults.exprs,
                            later_params,
                            shadowed,
                            refs,
                        );
                    }
                }
                if let Some(ast::FunctionBodyDef::Expr(lambda_body, _)) = &func_def.body
                    && let Some(root_expr) = lambda_body.root_expr
                {
                    Self::collect_default_expr_forward_references(
                        root_expr,
                        lambda_body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                shadowed.truncate(saved_len);
            }
            Expr::Spawn {
                name,
                body: spawn_body,
            } => {
                if let Some(name_id) = name {
                    Self::collect_default_expr_forward_references(
                        *name_id,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                Self::collect_default_expr_forward_references(
                    *spawn_body,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Expr::Await { future } => {
                Self::collect_default_expr_forward_references(
                    *future,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Expr::Literal(_) | Expr::ByteStringLiteral(_) | Expr::Null | Expr::Missing => {}
        }
    }

    fn collect_default_stmt_forward_references(
        stmt_id: StmtId,
        body: &ExprBody,
        later_params: &FxHashSet<Name>,
        shadowed: &mut Vec<Name>,
        refs: &mut Vec<Name>,
    ) {
        match &body.stmts[stmt_id] {
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) | Stmt::Throw { value: expr } => {
                Self::collect_default_expr_forward_references(
                    *expr,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Stmt::Let {
                pattern,
                initializer,
                ..
            } => {
                if let Some(expr) = initializer {
                    Self::collect_default_expr_forward_references(
                        *expr,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                Self::push_pattern_bindings(*pattern, body, shadowed);
            }
            Stmt::While {
                condition,
                body: loop_body,
                after,
                ..
            } => {
                Self::collect_default_expr_forward_references(
                    *condition,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                let saved_len = shadowed.len();
                Self::collect_default_expr_forward_references(
                    *loop_body,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                if let Some(stmt) = after {
                    Self::collect_default_stmt_forward_references(
                        *stmt,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                shadowed.truncate(saved_len);
            }
            Stmt::For {
                binding,
                collection,
                body: loop_body,
                ..
            } => {
                Self::collect_default_expr_forward_references(
                    *collection,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                let saved_len = shadowed.len();
                Self::push_pattern_bindings(*binding, body, shadowed);
                Self::collect_default_expr_forward_references(
                    *loop_body,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                shadowed.truncate(saved_len);
            }
            Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
                Self::collect_default_expr_forward_references(
                    *target,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                Self::collect_default_expr_forward_references(
                    *value,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            Stmt::Return(None)
            | Stmt::Break
            | Stmt::Continue
            | Stmt::Missing
            | Stmt::HeaderComment { .. } => {}
        }
    }

    fn push_pattern_bindings(pat_id: PatId, body: &ExprBody, shadowed: &mut Vec<Name>) {
        for name in body.patterns[pat_id].bound_names(&body.patterns) {
            shadowed.push(name.clone());
        }
    }

    /// Shared call pipeline: alias expansion, function matching, `skip_self_param`,
    /// arity check, two-pass generic inference, return type substitution.
    ///
    /// Does NOT perform the final subtype check or `record_expr_type`; callers handle those.
    fn check_call_inner(&mut self, request: CallCheckRequest<'_>) -> CheckedCallInner {
        let CallCheckRequest {
            context:
                CallContext {
                    expr_id,
                    args,
                    call_args,
                    body,
                    expected,
                },
            callee_ty,
            is_method_call,
            is_optional_call,
            explicit_type_arg_bindings,
        } = request;
        let explicit_args_used = explicit_type_arg_bindings.is_some();
        let callee_ty = self.expand_alias_chains(callee_ty);

        match &callee_ty {
            Ty::Function { params, ret, .. } => {
                let effective_params = if is_method_call {
                    crate::generics::skip_self_param(params)
                } else {
                    params.as_slice()
                };

                // When explicit type args were provided at the call site (e.g. `foo<int>(x)`),
                // skip Phase 0/1a/1b inference and use the pre-computed bindings directly.
                // This avoids ambiguity when the user has been explicit about type instantiation.
                let mut bindings: FxHashMap<Name, Ty> =
                    explicit_type_arg_bindings.unwrap_or_default();

                // Only run Phase 0/1a/1b when we don't have explicit type-arg bindings.
                let run_inference_phases = bindings.is_empty();

                let phase0_expected = if is_optional_call
                    && !matches!(expected, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    let ret_info = self.analyze_optional_base(ret);
                    let expected_info = self.analyze_optional_base(expected);
                    if ret_info.is_nullable() {
                        Some(expected_info.expanded)
                    } else if expected_info.is_null_only() {
                        None
                    } else {
                        Some(expected_info.inner)
                    }
                } else {
                    Some(expected.clone())
                };

                // Phase 0: reverse-infer from expected return type (low priority).
                // Skip when expected is Unknown/Error, or when explicit type args were given.
                if run_inference_phases {
                    if let Some(phase0_expected) = phase0_expected.as_ref() {
                        if crate::generics::contains_typevar(ret)
                            && !matches!(phase0_expected, Ty::Unknown { .. } | Ty::Error { .. })
                        {
                            crate::generics::infer_bindings(ret, phase0_expected, &mut bindings);
                        }
                    }
                }

                // Phase 1: forward-infer from arguments (high priority, overrides).
                // Two-pass: first process non-lambda args to bind type vars,
                // then process lambda args with resolved bindings.
                // Skip when explicit type args were provided — bindings are already set.
                let param_arg_pairs =
                    self.bind_call_args(expr_id, effective_params, args, call_args, body);

                if run_inference_phases {
                    for (param, arg) in &param_arg_pairs {
                        if matches!(&body.exprs[*arg], Expr::Lambda(_)) {
                            continue;
                        }
                        let param_ty = &param.ty;
                        let substituted = crate::generics::substitute_ty(param_ty, &bindings);
                        let arg_ty = if !crate::generics::contains_typevar(&substituted) {
                            self.check_expr(*arg, body, &substituted)
                        } else {
                            self.infer_expr(*arg, body)
                        };
                        crate::generics::infer_bindings(param_ty, &arg_ty, &mut bindings);
                    }

                    for (param, arg) in &param_arg_pairs {
                        if !matches!(&body.exprs[*arg], Expr::Lambda(_)) {
                            continue;
                        }
                        let param_ty = &param.ty;
                        let substituted = crate::generics::substitute_ty(param_ty, &bindings);
                        let arg_ty = if !crate::generics::contains_typevar(&substituted) {
                            self.check_expr(*arg, body, &substituted)
                        } else if let Some(Ty::Function {
                            params: fn_params, ..
                        }) = self.expected_lambda_function_ty(&substituted)
                        {
                            let all_params_concrete = fn_params
                                .iter()
                                .all(|param| !crate::generics::contains_typevar(&param.ty));
                            if all_params_concrete {
                                self.check_expr(*arg, body, &substituted)
                            } else {
                                self.infer_expr(*arg, body)
                            }
                        } else {
                            self.infer_expr(*arg, body)
                        };
                        crate::generics::infer_bindings(param_ty, &arg_ty, &mut bindings);
                    }
                } else {
                    // Explicit type args: still need to type-check all value arguments.
                    // Use the substituted param types (now concrete) for checking.
                    for (param, arg) in &param_arg_pairs {
                        let param_ty = &param.ty;
                        let substituted = crate::generics::substitute_ty(param_ty, &bindings);
                        if !crate::generics::contains_typevar(&substituted) {
                            self.check_expr(*arg, body, &substituted);
                        } else {
                            self.infer_expr(*arg, body);
                        }
                    }
                }

                // Final argument validation after bindings are known. This is
                // required for higher-order parameters whose type became
                // concrete only after effect/generic inference.
                for (param, arg) in &param_arg_pairs {
                    let param_ty = &param.ty;
                    if !crate::generics::contains_typevar(param_ty) {
                        continue;
                    }

                    let expected_arg_ty = crate::generics::substitute_ty(param_ty, &bindings);
                    if matches!(&body.exprs[*arg], Expr::Lambda(_)) {
                        continue;
                    }

                    if matches!(expected_arg_ty, Ty::Unknown { .. } | Ty::Error { .. })
                        || crate::generics::contains_typevar(&expected_arg_ty)
                    {
                        continue;
                    }

                    let arg_ty = self
                        .expressions
                        .get(arg)
                        .cloned()
                        .unwrap_or_else(|| self.infer_expr(*arg, body));

                    if !self.argument_matches_expected(&arg_ty, &expected_arg_ty) {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: expected_arg_ty.clone(),
                                got: arg_ty,
                            },
                            *arg,
                            Vec::new(),
                        );
                    } else {
                        self.record_function_coercion_if_needed(*arg, &arg_ty, &expected_arg_ty);
                    }
                }

                let substituted_ret = crate::generics::substitute_ty(ret, &bindings);
                let mut erase_diags = Vec::new();
                let result =
                    crate::generics::erase_unresolved_typevars(&substituted_ret, &mut erase_diags);
                for d in erase_diags {
                    self.context.report_simple(d, expr_id);
                }

                CheckedCallInner {
                    result,
                    bindings_from_inference: !bindings.is_empty() && !explicit_args_used,
                }
            }
            Ty::Unknown { .. } | Ty::Error { .. } => {
                self.infer_args_for_recovery(args, body);
                CheckedCallInner {
                    result: Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                    bindings_from_inference: false,
                }
            }
            Ty::Union(arms, _) => {
                // When ALL union arms are functions (e.g. method lookup on a union
                // type such as the `json` alias `null | bool | int | ... | json[]`),
                // fold them into a single representative callable.
                //
                // Fold rules:
                //   params  — first arm's params (all arms share the same method
                //             signature modulo the `self` type, which is erased here)
                //   ret     — union of all return types (deduped)
                //   throws  — union of all throws types (deduped)
                //
                // This is sound because the call site's actual value will be one of
                // the union arms at runtime, so the folded return / throws is an
                // over-approximation that covers all possible outcomes.
                let arms_clone = arms.clone();
                let expanded: Vec<Ty> = arms_clone
                    .iter()
                    .map(|a| self.expand_alias_chains(a.clone()))
                    .collect();

                let all_fns = expanded.iter().all(|a| matches!(a, Ty::Function { .. }));
                if all_fns && !expanded.is_empty() {
                    // Extract (params, ret, throws) from each arm.
                    let fn_components: Vec<_> = expanded
                        .iter()
                        .map(|a| {
                            let Ty::Function {
                                params,
                                ret,
                                throws,
                                ..
                            } = a
                            else {
                                unreachable!()
                            };
                            (params.clone(), *ret.clone(), *throws.clone())
                        })
                        .collect();

                    // Guard the fold: every arm must agree on arity and on the
                    // non-self param types. Otherwise the call site has no
                    // single signature to typecheck against — drop to the
                    // not-callable branch below to surface the ambiguity.
                    let first_non_self: Vec<&Ty> = fn_components[0]
                        .0
                        .iter()
                        .skip(1)
                        .map(|param| &param.ty)
                        .collect();
                    let arms_compatible = fn_components.iter().all(|(p, _, _)| {
                        p.len() == fn_components[0].0.len()
                            && p.iter()
                                .skip(1)
                                .zip(&first_non_self)
                                .all(|(param, expected)| &param.ty == *expected)
                    });
                    if !arms_compatible {
                        self.context.report_simple(
                            TirTypeError::NotCallable {
                                ty: callee_ty.clone(),
                            },
                            expr_id,
                        );
                        self.infer_args_for_recovery(args, body);
                        return CheckedCallInner {
                            result: Ty::Unknown {
                                attr: TyAttr::default(),
                            },
                            bindings_from_inference: false,
                        };
                    }

                    // Use first arm's params as representative.
                    let first_params = fn_components[0].0.clone();

                    // Union of all return types (deduplicated).
                    let ret_set: BTreeSet<Ty> =
                        fn_components.iter().map(|(_, r, _)| r.clone()).collect();
                    let folded_ret = match ret_set.len() {
                        1 => ret_set.into_iter().next().unwrap(),
                        _ => Ty::Union(ret_set.into_iter().collect(), TyAttr::default()),
                    };

                    // Union of all throws types (flattened and deduplicated).
                    let mut throws_set: BTreeSet<Ty> = BTreeSet::new();
                    for (_, _, t) in &fn_components {
                        throws_set.extend(crate::throw_inference::flatten_ty_to_facts(t));
                    }
                    let folded_throws = match throws_set.len() {
                        0 => Ty::Never {
                            attr: TyAttr::default(),
                        },
                        1 => throws_set.into_iter().next().unwrap(),
                        _ => Ty::Union(throws_set.into_iter().collect(), TyAttr::default()),
                    };

                    let folded_fn = Ty::Function {
                        params: first_params,
                        ret: Box::new(folded_ret),
                        throws: Box::new(folded_throws),
                        attr: TyAttr::default(),
                    };

                    return self.check_call_inner(CallCheckRequest {
                        context: CallContext {
                            expr_id,
                            args,
                            call_args,
                            body,
                            expected,
                        },
                        callee_ty: folded_fn,
                        is_method_call,
                        is_optional_call,
                        explicit_type_arg_bindings,
                    });
                }

                // Not all arms are functions — report not callable.
                self.context.report_simple(
                    TirTypeError::NotCallable {
                        ty: callee_ty.clone(),
                    },
                    expr_id,
                );
                self.infer_args_for_recovery(args, body);
                CheckedCallInner {
                    result: Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                    bindings_from_inference: false,
                }
            }
            _ => {
                self.context.report_simple(
                    TirTypeError::NotCallable {
                        ty: callee_ty.clone(),
                    },
                    expr_id,
                );
                self.infer_args_for_recovery(args, body);
                CheckedCallInner {
                    result: Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                    bindings_from_inference: false,
                }
            }
        }
    }

    fn report_result_type_mismatch(&mut self, expr_id: ExprId, got: &Ty, expected: &Ty) {
        if !matches!(expected, Ty::Unknown { .. } | Ty::Error { .. })
            && !self.is_subtype(got, expected)
        {
            self.context.report(
                TirTypeError::TypeMismatch {
                    expected: expected.clone(),
                    got: got.clone(),
                },
                expr_id,
                Vec::new(),
            );
        }
    }

    fn finalize_optional_callee_call(
        &mut self,
        context: OptionalCallContext<'_>,
        callee_ty: &Ty,
    ) -> Ty {
        let OptionalCallContext {
            call,
            callee_id,
            is_method_call,
        } = context;
        let CallContext {
            expr_id,
            args,
            call_args: _,
            body,
            expected,
        } = call;
        let callee_info = self.analyze_optional_base(callee_ty);

        if let Some(result_ty) = self.try_container_method_call(callee_id, args, body) {
            let final_ty = Self::make_optional(result_ty);
            self.report_result_type_mismatch(expr_id, &final_ty, expected);
            self.record_expr_type(expr_id, final_ty.clone());
            return final_ty;
        }

        if callee_info.is_null_only() {
            self.infer_args_for_recovery(args, body);
            let ty = Ty::Primitive(PrimitiveType::Null, TyAttr::default());
            self.report_result_type_mismatch(expr_id, &ty, expected);
            self.record_expr_type(expr_id, ty.clone());
            return ty;
        }

        let checked = self.check_call_inner(CallCheckRequest {
            context: call,
            callee_ty: callee_info.inner,
            is_method_call,
            is_optional_call: true,
            explicit_type_arg_bindings: None,
        });
        let final_ty = Self::make_optional(checked.result);
        self.report_result_type_mismatch(expr_id, &final_ty, expected);
        self.record_expr_type(expr_id, final_ty.clone());
        final_ty
    }

    // ── Bidirectional Type Checking ─────────────────────────────────────────

    /// Synthesis mode: compute the type of an expression bottom-up.
    pub fn infer_expr(&mut self, expr_id: ExprId, body: &ExprBody) -> Ty {
        let expr = &body.exprs[expr_id];
        let ty = match expr {
            Expr::Literal(lit) => Self::infer_literal(lit),
            Expr::ByteStringLiteral(_) => {
                Ty::Primitive(PrimitiveType::Uint8Array, TyAttr::default())
            }
            Expr::Null => Ty::Primitive(PrimitiveType::Null, TyAttr::default()),
            Expr::Path(segments) => self.infer_path(segments.as_slice(), body, expr_id),
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // Infer the condition first so its type is in `self.expressions`.
                self.infer_expr(*condition, body);

                // Extract narrowings from the condition expression.
                let narrowings = crate::narrowing::extract_narrowings(
                    *condition,
                    body,
                    &self.expressions,
                    &self.pattern_types,
                );

                // Apply then-branch narrowings, saving originals.
                let saved = crate::narrowing::apply_then_narrowings(&narrowings, &mut self.locals);

                let then_ty = self.infer_expr(*then_branch, body);

                // Restore originals and apply else-branch narrowings.
                crate::narrowing::restore_and_apply_else(&narrowings, &saved, &mut self.locals);

                let result_ty = if let Some(else_id) = else_branch {
                    let else_ty = self.infer_expr(*else_id, body);
                    Self::join_types(&then_ty, &else_ty)
                } else {
                    Ty::Void {
                        attr: TyAttr::default(),
                    }
                };

                // Restore original types after the if expression.
                crate::narrowing::restore_narrowings(saved, &mut self.locals);

                result_ty
            }
            Expr::Call { .. } => {
                // Delegate to check_expr with Ty::Unknown so the generic
                // inference logic in check_expr handles all call expressions.
                self.check_expr(
                    expr_id,
                    body,
                    &Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                )
            }
            Expr::Block { stmts, tail_expr } => {
                let snapshot = self.snapshot_scoped_locals();
                let mut diverged_at: Option<(usize, StmtId)> = None;
                for (i, stmt_id) in stmts.iter().enumerate() {
                    if self.check_stmt_with_early_return_narrowing(*stmt_id, body) {
                        diverged_at = Some((i, *stmt_id));
                        break;
                    }
                }
                let ty = if let Some((div_idx, div_stmt)) = diverged_at {
                    let remaining = stmts.len() - div_idx - 1 + usize::from(tail_expr.is_some());
                    if remaining > 0 {
                        self.context.report_warning_at_stmt(
                            crate::infer_context::TirTypeError::DeadCode {
                                after: div_stmt,
                                unreachable_count: remaining,
                            },
                            div_stmt,
                        );
                    }
                    Ty::Never {
                        attr: TyAttr::default(),
                    }
                } else {
                    tail_expr
                        .map(|e| self.infer_expr(e, body))
                        .unwrap_or(Ty::Void {
                            attr: TyAttr::default(),
                        })
                };
                self.restore_scoped_locals(&snapshot);
                ty
            }
            Expr::MemberAccess { base, member } => {
                // `MemberAccess` now only comes from `FIELD_ACCESS_EXPR` (complex base
                // expressions like `f().a`, `arr[0].x`). Package-qualified paths are
                // always `Expr::Path` nodes (never `MemberAccess`) after Phase 1.
                //
                // Still handle primitive-type static method access (e.g. an expression
                // evaluating to an image type followed by `.from_url`) via the existing
                // try_primitive_static_access helper.
                if let Some(ty) = self.try_primitive_static_access(expr_id, *base, member, body) {
                    ty
                } else {
                    let base_ty = self.infer_expr(*base, body);

                    // Determine if the base is a runtime value (local variable, function
                    // result, etc.) or a bare type name used as a namespace (e.g.
                    // `Factory<int>` in `Factory<int>.create(42)`).
                    // A base is a type name if it's a `Path` whose root segment is NOT
                    // a local variable — multi-segment paths like
                    // `root.pkg.inner.Box<int>.from_json(j)` resolve to a class type at
                    // the base position, so the access is an "unbound" static-method
                    // reference (bound = false).
                    let base_is_value = match &body.exprs[*base] {
                        Expr::Path(segments) if !segments.is_empty() => {
                            self.locals.contains_key(&segments[0])
                        }
                        _ => true, // complex expressions are always values
                    };

                    let inner = crate::narrowing::remove_null(&base_ty);
                    // `Primitive(Null)` is a concrete non-optional type (the null value
                    // itself) with its own companion class. Treat it like any other
                    // primitive — do NOT require `?.` chaining for direct method calls.
                    let is_pure_null = matches!(base_ty, Ty::Primitive(PrimitiveType::Null, _));
                    if inner != base_ty
                        && !is_pure_null
                        && !matches!(base_ty, Ty::Unknown { .. } | Ty::Error { .. })
                    {
                        if self.in_optional_chain > 0 {
                            // Inside an OptionalChain: auto-unwrap nullable base,
                            // resolve the member, and re-wrap in Optional.
                            // This allows `a?.b.c` where `a?.b` returns `T?`.
                            let member_ty =
                                self.resolve_member(&inner, member, expr_id, base_is_value);
                            Self::make_optional(member_ty)
                        } else {
                            // Outside any chain: accessing `.member` on a nullable type
                            // is an error (e.g. `(a?.b).c`). Use `?.` instead.
                            let base_text = body.display_expr(*base);
                            self.context.report_simple(
                                TirTypeError::NullableMemberAccess {
                                    base: base_text.clone(),
                                    member: format!(".{member}"),
                                    expr: format!("{base_text}.{member}"),
                                },
                                expr_id,
                            );
                            // Still resolve for downstream inference
                            let member_ty =
                                self.resolve_member(&inner, member, expr_id, base_is_value);
                            Self::make_optional(member_ty)
                        }
                    } else {
                        self.resolve_member(&base_ty, member, expr_id, base_is_value)
                    }
                }
            }
            Expr::OptionalMemberAccess { base, member } => {
                // Optional chaining: a?.b — if a is null, short-circuit to null.
                // Type: if a: T?, resolve member on T, wrap result in Optional.
                let base_ty = self.infer_expr(*base, body);
                let base_info = self.analyze_optional_base(&base_ty);
                // E2: warn if base is not nullable (?.  is unnecessary)
                if !base_info.is_nullable()
                    && !matches!(base_ty, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    let base_text = body.display_expr(*base);
                    self.context.report_simple(
                        TirTypeError::UnnecessaryOptionalChaining {
                            expr: format!("{base_text}?.{member}"),
                            base: base_text,
                        },
                        expr_id,
                    );
                }
                if base_info.is_null_only() {
                    // Base is just null — result is null
                    Ty::Primitive(PrimitiveType::Null, TyAttr::default())
                } else {
                    // OptionalMemberAccess always has a value base → bound = true.
                    let member_ty = self.resolve_member(&base_info.inner, member, expr_id, true);
                    Self::make_optional(member_ty)
                }
            }
            Expr::Array { elements } => {
                let elem_types: Vec<Ty> =
                    elements.iter().map(|e| self.infer_expr(*e, body)).collect();
                let elem_ty = Self::join_all(&elem_types).widen_fresh();
                Ty::List(Box::new(elem_ty), TyAttr::default())
            }
            Expr::Map { entries } => {
                let mut key_types = Vec::new();
                let mut val_types = Vec::new();
                for (k, v) in entries {
                    key_types.push(self.infer_expr(*k, body));
                    val_types.push(self.infer_expr(*v, body));
                }
                let key_ty = Self::join_all(&key_types).widen_fresh();
                let val_ty = Self::join_all(&val_types).widen_fresh();
                Ty::Map(Box::new(key_ty), Box::new(val_ty), TyAttr::default())
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.infer_expr(*lhs, body);
                let rhs_ty = self.infer_expr(*rhs, body);

                // Optional chaining diagnostics for ?? and ||
                match op {
                    baml_compiler2_ast::BinaryOp::NullCoalesce => {
                        // E3: LHS is non-nullable — ?? is unnecessary
                        let inner_lhs = crate::narrowing::remove_null(&lhs_ty);
                        if inner_lhs == lhs_ty
                            && !matches!(lhs_ty, Ty::Unknown { .. } | Ty::Error { .. })
                        {
                            let lhs_text = body.display_expr(*lhs);
                            let expr_text = body.display_expr(expr_id);
                            self.context.report_simple(
                                TirTypeError::UnnecessaryNullCoalesce {
                                    lhs: lhs_text,
                                    expr: expr_text,
                                },
                                expr_id,
                            );
                        }
                        // W2: RHS is null — ?? null is a no-op
                        if matches!(&body.exprs[*rhs], Expr::Null) {
                            let lhs_text = body.display_expr(*lhs);
                            self.context.report_warning_simple(
                                TirTypeError::NullCoalesceWithNull { lhs: lhs_text },
                                expr_id,
                            );
                        }
                    }
                    baml_compiler2_ast::BinaryOp::Or => {
                        // W1: LHS is nullable — suggest ?? instead of ||
                        let inner_lhs = crate::narrowing::remove_null(&lhs_ty);
                        if inner_lhs != lhs_ty
                            && !matches!(lhs_ty, Ty::Unknown { .. } | Ty::Error { .. })
                        {
                            let lhs_text = body.display_expr(*lhs);
                            let rhs_text = body.display_expr(*rhs);
                            self.context.report_warning_simple(
                                TirTypeError::SuggestNullCoalesce {
                                    lhs: lhs_text,
                                    rhs: rhs_text,
                                },
                                expr_id,
                            );
                        }
                    }
                    _ => {}
                }

                self.infer_binary_op(*op, &lhs_ty, &rhs_ty, expr_id)
            }
            Expr::Unary { op, expr } => {
                let operand_ty = self.infer_expr(*expr, body);
                self.infer_unary_op(*op, &operand_ty, expr_id)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => self.infer_match_expr(expr_id, *scrutinee, arms, body),
            Expr::Is { scrutinee, pattern } => self.infer_is_expr(*scrutinee, *pattern, body),
            Expr::Catch { base, clauses } => {
                self.infer_catch_expr(expr_id, *base, clauses, body, None)
            }
            Expr::Throw { value } => {
                self.infer_expr(*value, body);
                Ty::Never {
                    attr: TyAttr::default(),
                }
            }
            Expr::Object {
                type_name,
                type_args: obj_type_args,
                fields,
                ..
            } => {
                for (_, expr_id) in fields {
                    self.infer_expr(*expr_id, body);
                }
                // Lower explicit type args from `Foo<int> { ... }` syntax.
                let lowered_type_args: Vec<Ty> = obj_type_args
                    .iter()
                    .map(|te| {
                        let mut diags = Vec::new();
                        let ty = crate::lower_type_expr::lower_type_expr_in_ns(
                            self.context.db(),
                            te,
                            self.package_items,
                            &self.ns_context,
                            &self.generic_params,
                            &mut diags,
                        );
                        // Swallow diagnostics silently — errors will be caught during
                        // elaboration and reported with better context.
                        let _ = diags;
                        ty
                    })
                    .collect();
                type_name
                    .as_ref()
                    .and_then(|path| {
                        // Bare names: look up in the local package's namespace
                        // context. Qualified paths (`baml.glob.ScanOptions`,
                        // `root.http.Response`): go through the resolver that
                        // understands cross-namespace and cross-package paths.
                        // Mixing these would let a bare name fall through to
                        // another package, which the project intentionally
                        // forbids — single-segment writes mean "in scope here".
                        let db = self.context.db();
                        if path.is_qualified() {
                            self.res_ctx
                                .resolve_type(db, path.segments(), &self.ns_context)
                                .map(|(_, ty)| ty)
                        } else {
                            let leaf = path.leaf();
                            self.package_items
                                .lookup_type(&self.ns_context, leaf)
                                .map(|def| {
                                    Ty::Class(
                                        crate::lower_type_expr::qualify_def(db, def, leaf),
                                        lowered_type_args,
                                        TyAttr::default(),
                                    )
                                })
                        }
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    })
            }
            Expr::Index { base, index } => {
                let base_ty = self.infer_expr(*base, body);
                self.infer_expr(*index, body);
                let inner = crate::narrowing::remove_null(&base_ty);
                let (resolve_ty, rewrap) = if inner != base_ty
                    && !matches!(base_ty, Ty::Unknown { attr: _ } | Ty::Error { attr: _ })
                {
                    if self.in_optional_chain == 0 {
                        // Outside any chain: indexing a nullable type is an error.
                        let base_text = body.display_expr(*base);
                        let expr_text = body.display_expr(expr_id);
                        self.context.report_simple(
                            TirTypeError::NullableMemberAccess {
                                base: base_text,
                                member: "[...]".to_string(),
                                expr: expr_text,
                            },
                            expr_id,
                        );
                    }
                    (inner, true)
                } else {
                    (base_ty, false)
                };
                let elem_ty = match resolve_ty {
                    Ty::List(elem_ty, _) | Ty::EvolvingList(elem_ty, _) => *elem_ty,
                    Ty::Map(_, val_ty, _) | Ty::EvolvingMap(_, val_ty, _) => *val_ty,
                    Ty::Primitive(PrimitiveType::Uint8Array, _) => {
                        Ty::Primitive(PrimitiveType::Int, TyAttr::default())
                    }
                    Ty::Unknown { attr: _ } | Ty::Error { attr: _ } => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                    _ => {
                        self.context.report_simple(
                            TirTypeError::NotIndexable {
                                ty: resolve_ty.clone(),
                            },
                            expr_id,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    }
                };
                if rewrap {
                    Self::make_optional(elem_ty)
                } else {
                    elem_ty
                }
            }
            Expr::OptionalIndex { base, index } => {
                // Optional chaining: a?.[expr] — short-circuits to null if a is null.
                let base_ty = self.infer_expr(*base, body);
                self.infer_expr(*index, body);
                let base_info = self.analyze_optional_base(&base_ty);
                // E2: warn if base is not nullable
                if !base_info.is_nullable()
                    && !matches!(base_ty, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    let base_text = body.display_expr(*base);
                    let expr_text = body.display_expr(expr_id);
                    self.context.report_simple(
                        TirTypeError::UnnecessaryOptionalChaining {
                            expr: expr_text,
                            base: base_text,
                        },
                        expr_id,
                    );
                }
                if base_info.is_null_only() {
                    Ty::Primitive(PrimitiveType::Null, TyAttr::default())
                } else {
                    let elem_ty = match &base_info.inner {
                        Ty::List(elem_ty, _) | Ty::EvolvingList(elem_ty, _) => {
                            elem_ty.as_ref().clone()
                        }
                        Ty::Map(_, val_ty, _) | Ty::EvolvingMap(_, val_ty, _) => {
                            val_ty.as_ref().clone()
                        }
                        Ty::Unknown { .. } | Ty::Error { .. } => Ty::Unknown {
                            attr: TyAttr::default(),
                        },
                        _ => {
                            self.context.report_simple(
                                TirTypeError::NotIndexable {
                                    ty: base_info.inner.clone(),
                                },
                                expr_id,
                            );
                            Ty::Unknown {
                                attr: TyAttr::default(),
                            }
                        }
                    };
                    Self::make_optional(elem_ty)
                }
            }
            Expr::OptionalCall { .. } => {
                // Keep synthesis mode aligned with `check_expr`: optional calls
                // should still run the shared call checker so argument type
                // validation, generic inference, and higher-order callable
                // checks happen even when no contextual expected type exists.
                self.check_expr(
                    expr_id,
                    body,
                    &Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                )
            }
            Expr::OptionalChain { expr } => {
                // Transparent wrapper — type is the same as the inner expression's type.
                // While inside the chain, FieldAccess/Index auto-unwrap nullable bases
                // (null is caught by the chain's short-circuit scope).
                self.in_optional_chain += 1;
                let ty = self.infer_expr(*expr, body);
                self.in_optional_chain -= 1;
                ty
            }
            Expr::Lambda(func_def) => {
                // Synthesis mode: no expected type available.
                // All param types MUST be annotated; unannotated params produce an error.

                // Combine parent generics with the lambda's own generic params
                // so that `<T>(x: T) -> T { x }` recognizes T as a TypeVar.
                let mut all_generic_params = self.generic_params.clone();
                all_generic_params.extend(func_def.generic_params.iter().cloned());

                let mut param_tys: Vec<FunctionParamTy> = Vec::new();

                for param in &func_def.params {
                    let param_ty = match &param.type_expr {
                        Some(te) => {
                            self.lower_lambda_type_expr(&te.expr, &all_generic_params, te.span)
                        }
                        None => {
                            // No annotation and no expected type → error
                            self.context.report_simple(
                                TirTypeError::CannotInferLambdaParamType {
                                    param_name: param.name.clone(),
                                },
                                expr_id,
                            );
                            Ty::Unknown {
                                attr: TyAttr::default(),
                            }
                        }
                    };
                    param_tys.push(FunctionParamTy::required(
                        Some(param.name.clone()),
                        param_ty,
                    ));
                }

                // Lower optional return type annotation
                let return_annotation = func_def
                    .return_type
                    .as_ref()
                    .map(|te| self.lower_lambda_type_expr(&te.expr, &all_generic_params, te.span));
                let (throws_ty, throws_span, warn_extraneous_throws) =
                    self.choose_lambda_throws_surface(func_def, &all_generic_params, None);

                // Infer the lambda body using save/restore approach
                let (ret_ty, _lambda_expressions, lambda_fsi, lambda_effective_throws) = self
                    .infer_lambda_body(
                        func_def,
                        &param_tys,
                        return_annotation.as_ref(),
                        &throws_ty,
                        throws_span,
                        warn_extraneous_throws,
                    );
                let surface_ret_ty = return_annotation.unwrap_or(ret_ty);

                let result = Ty::Function {
                    params: param_tys,
                    ret: Box::new(surface_ret_ty),
                    throws: Box::new(throws_ty),
                    attr: TyAttr::default(),
                };
                self.lambda_effective_throws
                    .insert(expr_id, lambda_effective_throws);
                if let Some(fsi) = lambda_fsi {
                    self.nested_lambda_types.insert(fsi, result.clone());
                }
                result
            }
            Expr::Spawn {
                name,
                body: spawn_body,
            } => {
                // BEP-034: `spawn name? { body } : Future<T, E>` where
                // `body` has type `T throws E`. After AST lowering the
                // body is wrapped in a synthetic 0-arg lambda; we infer
                // the lambda's type, peel out its return as `T`, and
                // pull its effective throws (computed and stored by
                // `infer_lambda_body`) as `E`.
                if let Some(name_id) = name {
                    let _ = self.infer_expr(*name_id, body);
                }
                let lambda_ty = self.infer_expr(*spawn_body, body);
                let value_ty = match &lambda_ty {
                    Ty::Function { ret, .. } => ret.as_ref().clone(),
                    _ => lambda_ty.clone(),
                };
                // Read the body's effective throws from the side table
                // populated by `infer_lambda_body`. `Never` means the
                // body throws nothing; BAML lacks a `never` type
                // variant, so approximate with `Null` (per the BEP's
                // "Future<T, never> ≈ Future<T, null> in v1" note).
                let throws_ty = self
                    .lambda_effective_throws
                    .get(spawn_body)
                    .cloned()
                    .map_or_else(
                        || Ty::Primitive(PrimitiveType::Null, TyAttr::default()),
                        |t| {
                            if matches!(t, Ty::Never { .. }) {
                                Ty::Primitive(PrimitiveType::Null, TyAttr::default())
                            } else {
                                t
                            }
                        },
                    );
                Ty::Future(Box::new(value_ty), Box::new(throws_ty), TyAttr::default())
            }
            Expr::Await { future } => {
                // BEP-034: `await e : T` where `e : Future<T, E>`.
                let fut_ty = self.infer_expr(*future, body);
                match fut_ty {
                    Ty::Future(value, _error, _) => *value,
                    Ty::Unknown { .. } | Ty::Error { .. } => fut_ty,
                    other => {
                        // `await` requires a Future operand. Emit a
                        // TypeMismatch with `Future<unknown, unknown>` as
                        // the expected shape so the user sees what `await`
                        // wanted instead of silently getting `Unknown`.
                        self.context.report_simple(
                            TirTypeError::TypeMismatch {
                                expected: Ty::Future(
                                    Box::new(Ty::Unknown {
                                        attr: TyAttr::default(),
                                    }),
                                    Box::new(Ty::Unknown {
                                        attr: TyAttr::default(),
                                    }),
                                    TyAttr::default(),
                                ),
                                got: other,
                            },
                            *future,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    }
                }
            }
            Expr::Missing => Ty::Unknown {
                attr: TyAttr::default(),
            },
        };
        self.record_expr_type(expr_id, ty.clone());
        ty
    }

    /// Checking mode: verify an expression against an expected type.
    pub fn check_expr(&mut self, expr_id: ExprId, body: &ExprBody, expected: &Ty) -> Ty {
        let expr = &body.exprs[expr_id];
        match expr {
            // Block: check the tail expression against expected type
            Expr::Block { stmts, tail_expr } => {
                let snapshot = self.snapshot_scoped_locals();
                let mut diverged_at: Option<(usize, StmtId)> = None;
                for (i, stmt_id) in stmts.iter().enumerate() {
                    if self.check_stmt_with_early_return_narrowing(*stmt_id, body) {
                        diverged_at = Some((i, *stmt_id));
                        break;
                    }
                }
                let ty = if let Some((div_idx, div_stmt)) = diverged_at {
                    let remaining = stmts.len() - div_idx - 1 + usize::from(tail_expr.is_some());
                    if remaining > 0 {
                        self.context.report_warning_at_stmt(
                            crate::infer_context::TirTypeError::DeadCode {
                                after: div_stmt,
                                unreachable_count: remaining,
                            },
                            div_stmt,
                        );
                    }
                    Ty::Never {
                        attr: TyAttr::default(),
                    }
                } else if let Some(tail) = tail_expr {
                    if matches!(expected, Ty::Void { .. }) {
                        // Void context: infer the tail for diagnostics but discard its value.
                        let _ = self.infer_expr(*tail, body);
                        Ty::Void {
                            attr: TyAttr::default(),
                        }
                    } else {
                        self.check_expr(*tail, body, expected)
                    }
                } else if !matches!(expected, Ty::Unknown { .. } | Ty::Void { .. }) {
                    // No tail expression, no divergence — block falls through
                    // without producing a value. Report missing return.
                    self.context.report_simple(
                        TirTypeError::MissingReturn {
                            expected: expected.clone(),
                        },
                        expr_id,
                    );
                    expected.clone()
                } else {
                    Ty::Void {
                        attr: TyAttr::default(),
                    }
                };
                self.restore_scoped_locals(&snapshot);
                self.record_expr_type(expr_id, ty.clone());
                ty
            }
            // If: check both branches against expected type
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // Infer the condition first so its type is in `self.expressions`.
                self.infer_expr(*condition, body);

                // Extract narrowings from the condition expression.
                let narrowings = crate::narrowing::extract_narrowings(
                    *condition,
                    body,
                    &self.expressions,
                    &self.pattern_types,
                );

                // Apply then-branch narrowings, saving originals.
                let saved = crate::narrowing::apply_then_narrowings(&narrowings, &mut self.locals);

                let then_ty = self.check_expr(*then_branch, body, expected);

                // Restore originals and apply else-branch narrowings.
                crate::narrowing::restore_and_apply_else(&narrowings, &saved, &mut self.locals);

                let ty = if let Some(else_id) = else_branch {
                    let else_ty = self.check_expr(*else_id, body, expected);
                    Self::join_types(&then_ty, &else_ty)
                } else {
                    if !matches!(expected, Ty::Void { .. } | Ty::Unknown { .. }) {
                        self.context
                            .report_simple(TirTypeError::VoidUsedAsValue, expr_id);
                    }
                    Ty::Void {
                        attr: TyAttr::default(),
                    }
                };

                // Restore original types after the if expression.
                crate::narrowing::restore_narrowings(saved, &mut self.locals);

                self.record_expr_type(expr_id, ty.clone());
                ty
            }
            Expr::Array { elements } => {
                let elem_ty = match expected {
                    Ty::List(e, _) | Ty::EvolvingList(e, _) => Some(e),
                    _ => None,
                };
                if let Some(elem_ty) = elem_ty {
                    for e in elements {
                        self.check_expr(*e, body, elem_ty);
                    }
                    let ty = expected.clone();
                    self.record_expr_type(expr_id, ty.clone());
                    ty
                } else {
                    self.infer_expr(expr_id, body)
                }
            }
            // Object: if expected is Class(name), check fields against declared types.
            Expr::Object { fields, .. } => {
                if let Ty::Class(class_name, type_args, _) = expected {
                    let field_types = self.lookup_class_fields(class_name, type_args);
                    for (field_name, field_expr) in fields {
                        if let Some(declared_ty) = field_types.get(field_name) {
                            self.check_expr(*field_expr, body, declared_ty);
                        } else {
                            self.infer_expr(*field_expr, body);
                        }
                    }
                    let ty = expected.clone();
                    self.record_expr_type(expr_id, ty.clone());
                    ty
                } else {
                    self.infer_expr(expr_id, body)
                }
            }
            Expr::Map { entries } => {
                let kv = match expected {
                    Ty::Map(k, v, _) | Ty::EvolvingMap(k, v, _) => Some((k, v)),
                    _ => None,
                };
                if let Some((key_ty, val_ty)) = kv {
                    for (k, v) in entries {
                        self.check_expr(*k, body, key_ty);
                        self.check_expr(*v, body, val_ty);
                    }
                    let ty = expected.clone();
                    self.record_expr_type(expr_id, ty.clone());
                    ty
                } else {
                    self.infer_expr(expr_id, body)
                }
            }
            // Literal checked against a literal type: compare values directly.
            // On match, strip freshness → Regular. On mismatch, fall through
            // to the default infer-then-check path which will report the error.
            Expr::Literal(lit) if matches!(expected, Ty::Literal(..)) => {
                use crate::ty::Freshness;
                let Ty::Literal(expected_lit, _, _) = expected else {
                    unreachable!()
                };
                if lit == expected_lit {
                    let ty =
                        Ty::Literal(expected_lit.clone(), Freshness::Regular, TyAttr::default());
                    self.record_expr_type(expr_id, ty.clone());
                    ty
                } else {
                    // Value doesn't match — infer (produces fresh literal) and
                    // let the subtype check report the error.
                    let inferred = self.infer_expr(expr_id, body);
                    if !self.is_subtype(&inferred, expected) {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: expected.clone(),
                                got: inferred.clone(),
                            },
                            expr_id,
                            Vec::new(),
                        );
                    }
                    inferred
                }
            }
            // Call expressions: generic type inference + argument checking.
            Expr::Call {
                callee,
                type_args,
                args,
            } => {
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                if matches!(&body.exprs[*callee], Expr::OptionalMemberAccess { .. })
                    && self.in_optional_chain > 0
                {
                    let callee_ty = self.infer_expr(*callee, body);
                    return self.finalize_optional_callee_call(
                        OptionalCallContext {
                            call: CallContext {
                                expr_id,
                                args: &arg_exprs,
                                call_args: Some(args),
                                body,
                                expected,
                            },
                            callee_id: *callee,
                            is_method_call: true,
                        },
                        &callee_ty,
                    );
                }

                // Container mutation fast path (e.g. x.push(val) on EvolvingList).
                // Matches MemberAccess and 2-segment Path (multi-segment paths).
                if Self::is_method_like_callee(&body.exprs[*callee])
                    && let Some(result_ty) =
                        self.try_container_method_call(*callee, &arg_exprs, body)
                {
                    self.report_result_type_mismatch(expr_id, &result_ty, expected);
                    self.record_expr_type(expr_id, result_ty.clone());
                    return result_ty;
                }

                let is_method_call = match &body.exprs[*callee] {
                    Expr::MemberAccess { .. } => true,
                    Expr::Path(segs) if segs.len() >= 2 => {
                        // A multi-segment Path callee is a method call only when the
                        // root is a local variable (e.g. `obj.method()`).  Package-
                        // qualified callees (e.g. `registry.register_test(...)`) are
                        // free-function calls where the first segment is a package name.
                        // We check directly in the TIR local scope rather than going
                        // through the HIR path_resolution_query, because ExprIds are
                        // per-function-body and not globally unique across functions.
                        self.locals.contains_key(&segs[0])
                    }
                    _ => false,
                };
                let callee_ty = self.infer_expr(*callee, body);

                // When explicit type args are written at the call site (e.g. `foo<int, T>(x)`),
                // validate arity and resolve them to a pre-computed bindings map.
                let explicit_type_arg_bindings = if !type_args.is_empty() {
                    self.resolve_explicit_type_args(*callee, type_args, expr_id)
                } else {
                    None
                };

                let checked = self.check_call_inner(CallCheckRequest {
                    context: CallContext {
                        expr_id,
                        args: &arg_exprs,
                        call_args: Some(args),
                        body,
                        expected,
                    },
                    callee_ty,
                    is_method_call,
                    is_optional_call: false,
                    explicit_type_arg_bindings,
                });

                if !checked.bindings_from_inference {
                    self.report_result_type_mismatch(expr_id, &checked.result, expected);
                }

                self.record_function_coercion_if_needed(expr_id, &checked.result, expected);
                self.record_expr_type(expr_id, checked.result.clone());
                checked.result
            }
            Expr::OptionalCall { callee, args } => {
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                let is_method_call = matches!(
                    &body.exprs[*callee],
                    Expr::MemberAccess { .. } | Expr::OptionalMemberAccess { .. }
                );
                let callee_ty = self.infer_expr(*callee, body);

                if !self.analyze_optional_base(&callee_ty).is_nullable()
                    && !matches!(&callee_ty, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    let callee_text = body.display_expr(*callee);
                    let expr_text = body.display_expr(expr_id);
                    self.context.report_simple(
                        TirTypeError::UnnecessaryOptionalChaining {
                            expr: expr_text,
                            base: callee_text,
                        },
                        expr_id,
                    );
                }

                self.finalize_optional_callee_call(
                    OptionalCallContext {
                        call: CallContext {
                            expr_id,
                            args: &arg_exprs,
                            call_args: Some(args),
                            body,
                            expected,
                        },
                        callee_id: *callee,
                        is_method_call,
                    },
                    &callee_ty,
                )
            }
            // Catch: propagate expected type to the base expression
            Expr::Catch { base, clauses } => {
                self.infer_catch_expr(expr_id, *base, clauses, body, Some(expected))
            }
            // Lambda: bidirectional checking against expected function type
            Expr::Lambda(func_def) => {
                let expected_function = self.expected_lambda_function_ty(expected);
                match expected_function.as_ref() {
                    Some(
                        expected_fn_ty @ Ty::Function {
                            params: expected_params,
                            ret: expected_ret,
                            throws: expected_throws,
                            ..
                        },
                    ) => {
                        // Checking mode: decompose expected function type.
                        // Arity check
                        if func_def.params.len() != expected_params.len() {
                            self.context.report_simple(
                                TirTypeError::ArgumentCountMismatch {
                                    expected: expected_params.len(),
                                    got: func_def.params.len(),
                                },
                                expr_id,
                            );
                        }

                        let mut all_generic_params = self.generic_params.clone();
                        all_generic_params.extend(func_def.generic_params.iter().cloned());

                        // Determine param types: annotation takes precedence, else use expected
                        let mut param_tys: Vec<FunctionParamTy> = Vec::new();
                        for (i, param) in func_def.params.iter().enumerate() {
                            let expected_param_ty = expected_params
                                .get(i)
                                .map(|param| param.ty.clone())
                                .unwrap_or(Ty::Unknown {
                                    attr: TyAttr::default(),
                                });

                            let param_ty = match &param.type_expr {
                                Some(te) => {
                                    let annotated = self.lower_lambda_type_expr(
                                        &te.expr,
                                        &all_generic_params,
                                        te.span,
                                    );
                                    // Check annotation is compatible with expected
                                    if !self.is_subtype(&expected_param_ty, &annotated) {
                                        self.context.report(
                                            TirTypeError::TypeMismatch {
                                                expected: expected_param_ty.clone(),
                                                got: annotated.clone(),
                                            },
                                            expr_id,
                                            Vec::new(),
                                        );
                                    }
                                    annotated
                                }
                                None => {
                                    // Bidirectional inference: use expected param type
                                    expected_param_ty
                                }
                            };
                            param_tys.push(FunctionParamTy::required(
                                Some(param.name.clone()),
                                param_ty,
                            ));
                        }

                        // Determine return type: annotation > expected
                        let return_annotation = func_def.return_type.as_ref().map(|te| {
                            self.lower_lambda_type_expr(&te.expr, &all_generic_params, te.span)
                        });
                        let effective_ret =
                            return_annotation.as_ref().unwrap_or(expected_ret.as_ref());
                        let (throws_ty, throws_span, warn_extraneous_throws) = self
                            .choose_lambda_throws_surface(
                                func_def,
                                &all_generic_params,
                                Some(expected_throws.as_ref()),
                            );

                        // Infer/check the lambda body using save/restore approach
                        let (ret_ty, _lambda_expressions, lambda_fsi, lambda_effective_throws) =
                            self.infer_lambda_body(
                                func_def,
                                &param_tys,
                                Some(effective_ret),
                                &throws_ty,
                                throws_span,
                                warn_extraneous_throws,
                            );
                        let surface_ret_ty = return_annotation.unwrap_or_else(|| {
                            if matches!(
                                expected_ret.as_ref(),
                                Ty::Unknown { .. } | Ty::TypeVar(_, _)
                            ) {
                                ret_ty.clone()
                            } else {
                                expected_ret.as_ref().clone()
                            }
                        });

                        let result = Ty::Function {
                            params: param_tys,
                            ret: Box::new(surface_ret_ty),
                            throws: Box::new(throws_ty),
                            attr: TyAttr::default(),
                        };
                        self.lambda_effective_throws
                            .insert(expr_id, lambda_effective_throws);
                        if !crate::generics::contains_typevar(expected_fn_ty)
                            && !self.is_subtype(&result, expected_fn_ty)
                        {
                            self.context.report(
                                TirTypeError::TypeMismatch {
                                    expected: expected_fn_ty.clone(),
                                    got: result.clone(),
                                },
                                expr_id,
                                Vec::new(),
                            );
                        }
                        self.record_function_coercion_if_needed(expr_id, &result, expected_fn_ty);
                        self.record_expr_type(expr_id, result.clone());
                        if let Some(fsi) = lambda_fsi {
                            self.nested_lambda_types.insert(fsi, result.clone());
                        }
                        result
                    }
                    _ => {
                        // Non-function expected type: fall through to infer-then-check
                        let inferred = self.infer_expr(expr_id, body);
                        if !self.is_subtype(&inferred, expected) {
                            self.context.report(
                                TirTypeError::TypeMismatch {
                                    expected: expected.clone(),
                                    got: inferred.clone(),
                                },
                                expr_id,
                                Vec::new(),
                            );
                        }
                        inferred
                    }
                }
            }
            // All other expressions: infer then subtype-check
            _ => {
                let inferred = self.infer_expr(expr_id, body);
                if matches!(inferred, Ty::Void { .. })
                    && !matches!(expected, Ty::Void { .. } | Ty::Unknown { .. })
                {
                    let err = if matches!(
                        body.exprs[expr_id],
                        Expr::Call { .. } | Expr::OptionalCall { .. }
                    ) {
                        TirTypeError::VoidFunctionResultUsed
                    } else {
                        TirTypeError::VoidUsedAsValue
                    };
                    self.context.report_simple(err, expr_id);
                } else if !self.is_subtype(&inferred, expected) {
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: expected.clone(),
                            got: inferred.clone(),
                        },
                        expr_id,
                        Vec::new(),
                    );
                } else {
                    self.record_function_coercion_if_needed(expr_id, &inferred, expected);
                }
                inferred
            }
        }
    }

    /// Type-check a statement. Returns `true` if the statement diverges
    /// (i.e. control flow never reaches the next statement).
    pub fn check_stmt(&mut self, stmt_id: StmtId, body: &ExprBody) -> bool {
        let stmt = &body.stmts[stmt_id];
        match stmt {
            Stmt::Expr(expr_id) => {
                let ty = self.infer_expr(*expr_id, body);
                matches!(ty, Ty::Never { .. })
            }
            Stmt::Let {
                pattern,
                initializer,
                ..
            } => {
                let (init_result_ty, pattern_subject_ty, declared_for_scope) =
                    if let Some(init) = *initializer {
                        let expected = self.pattern_expected_ty(*pattern, body);
                        let is_structural_pattern =
                            Self::pattern_contains_structural_syntax(*pattern, body);
                        let expected_for_check = match expected {
                            Some(PatternExpectedTy::Full(ty)) if !is_structural_pattern => Some(ty),
                            Some(PatternExpectedTy::Partial(ty))
                                if Self::expr_accepts_partial_pattern_expected(init, body) =>
                            {
                                Some(ty)
                            }
                            Some(PatternExpectedTy::Full(_) | PatternExpectedTy::Partial(_))
                            | None => None,
                        };
                        let ty = if let Some(expected) = expected_for_check.as_ref()
                            && !matches!(expected, Ty::Void { .. })
                        {
                            self.check_expr(init, body, expected)
                        } else {
                            self.infer_expr(init, body)
                        };
                        if matches!(ty, Ty::Void { .. }) {
                            let err = if matches!(
                                body.exprs[init],
                                Expr::Call { .. } | Expr::OptionalCall { .. }
                            ) {
                                TirTypeError::VoidFunctionResultUsed
                            } else {
                                TirTypeError::VoidUsedAsValue
                            };
                            self.context.report_simple(err, init);
                        }
                        if let Some(expected) = expected_for_check {
                            (Some(ty), Some(expected.clone()), Some(expected))
                        } else {
                            let flow_ty = ty.clone().widen_fresh().make_evolving();
                            (Some(ty), Some(flow_ty), None)
                        }
                    } else {
                        (None, None, None)
                    };

                let diverges = matches!(init_result_ty, Some(Ty::Never { .. }));
                if !diverges && let Some(flow_ty) = pattern_subject_ty {
                    let result =
                        self.analyze_and_lower(*pattern, &flow_ty, body, initializer.unwrap());
                    self.finalize_pattern_lowering(
                        *pattern,
                        &result,
                        declared_for_scope.as_ref(),
                        Some(IrrefutablePatternContext {
                            context: IrrefutableContextKind::Let,
                            fallback_expr: *initializer,
                        }),
                        &flow_ty,
                    );
                }
                diverges
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    if let Some(ret_ty) = &self.declared_return_ty {
                        let ret_ty = ret_ty.clone();
                        self.check_expr(*e, body, &ret_ty);
                    } else {
                        self.infer_expr(*e, body);
                    }
                }
                true // return always diverges
            }
            Stmt::Throw { value } => {
                self.infer_expr(*value, body);
                true
            }
            Stmt::While {
                condition,
                body: while_body,
                after,
                ..
            } => {
                self.infer_expr(*condition, body);
                // Snapshot scoped locals before the body and restore after,
                // mirroring `Stmt::For`. Without this, a `let x = ...`
                // inside the while body (or any narrowing of an outer name)
                // leaks past the loop, violating Slack rule 1.
                // `restore_scoped_locals` keeps outer-binding mutations from
                // the body — Slack rule 2 — by filtering assignments
                // through binding identity.
                let snapshot = self.snapshot_scoped_locals();
                self.infer_expr(*while_body, body);
                self.restore_scoped_locals(&snapshot);
                // Type-check the C-style for `after` step, if present. It
                // runs at the same lexical level as the body but in the
                // surrounding scope (HIR P1.2.b puts it inside the wrapping
                // While scope but outside the body's block scope), so we
                // check it AFTER restoring the snapshot — body-declared lets
                // are not in scope here.
                if let Some(after_stmt) = after {
                    self.check_stmt(*after_stmt, body);
                }
                false
            }
            // Design note: Stmt::For is kept as a first-class construct (not desugared
            // to While) so we can produce for-loop-specific diagnostics ("cannot iterate
            // over type X") and preserve iteration semantics for downstream codegen.
            // Desugaring to index-based basic blocks happens at MIR lowering time.
            Stmt::For {
                binding,
                collection,
                body: for_body,
            } => {
                // 1. Infer the collection type
                let coll_ty = self.infer_expr(*collection, body);

                // 2. Derive the element type from the collection
                let elem_ty = match &coll_ty {
                    Ty::List(elem, _) => *elem.clone(),
                    Ty::EvolvingList(elem, _) => *elem.clone(),
                    _ => {
                        self.context
                            .report_simple(TirTypeError::NotIterable { ty: coll_ty }, *collection);
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    }
                };

                // 3. Validate the binding pattern against the element type.
                let expected = self.pattern_expected_ty(*binding, body);
                let is_structural_pattern =
                    Self::pattern_contains_structural_syntax(*binding, body);
                let (flow_ty, declared_for_scope) = if let Some(PatternExpectedTy::Full(expected)) =
                    expected
                    && !is_structural_pattern
                {
                    if !self.is_subtype(&elem_ty, &expected) {
                        // Anchor the diagnostic to the binding pattern: the
                        // bad annotation is what's wrong, not the iterable.
                        let err = TirTypeError::TypeMismatch {
                            expected: expected.clone(),
                            got: elem_ty,
                        };
                        self.report_at_pat_or_expr(err, *binding, *collection);
                    }
                    (expected.clone(), Some(expected))
                } else {
                    (elem_ty, None)
                };

                // 4. Bind every Pattern::Bind reachable in the loop binding
                // pattern to the validated flow type.
                let snapshot = self.snapshot_scoped_locals();
                let result = self.analyze_and_lower(*binding, &flow_ty, body, *for_body);
                self.finalize_pattern_lowering(
                    *binding,
                    &result,
                    declared_for_scope.as_ref(),
                    Some(IrrefutablePatternContext {
                        context: IrrefutableContextKind::ForLet,
                        fallback_expr: Some(*collection),
                    }),
                    &flow_ty,
                );

                // 5. Check the body
                self.infer_expr(*for_body, body);
                self.restore_scoped_locals(&snapshot);
                false
            }
            Stmt::Assign { target, value } => {
                // Check for container index mutation: x[i] = val
                if self.try_index_assign_mutation(*target, *value, body) {
                    return false;
                }
                // Assignment targets with Optional* nodes (e.g. `user?.profile.name = val`)
                // are guarded by MIR's lower_safe_chain_guard — treat them as if inside
                // an OptionalChain for type inference to avoid false NullableMemberAccess errors.
                let target_has_optional = Self::expr_contains_optional(*target, body);
                if target_has_optional {
                    self.in_optional_chain += 1;
                }
                // For simple variable assignment (x = val), check against the
                // variable's *declared* type, not its potentially-narrowed type.
                // Narrowing may have refined x: int? → null inside an if-branch,
                // but assignment should still accept any value assignable to int?.
                let declared_ty = self.get_declared_type(*target, body);
                let value_ty = self.infer_expr(*value, body);
                if let Some(ref decl_ty) = declared_ty {
                    if !matches!(decl_ty, Ty::Unknown { .. } | Ty::Error { .. })
                        && !matches!(value_ty, Ty::Unknown { .. } | Ty::Error { .. })
                        && !self.is_subtype(&value_ty, decl_ty)
                    {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: decl_ty.clone(),
                                got: value_ty.clone(),
                            },
                            *value,
                            Vec::new(),
                        );
                    } else {
                        self.record_function_coercion_if_needed(*value, &value_ty, decl_ty);
                    }
                    // Update the local to the assigned value's type (invalidates narrowing)
                    if let Expr::Path(segments) = &body.exprs[*target] {
                        if segments.len() == 1 {
                            self.assign_local(segments[0].clone(), value_ty);
                        }
                    }
                } else {
                    self.infer_expr(*target, body);
                    self.infer_expr(*value, body);
                }
                if target_has_optional {
                    self.in_optional_chain -= 1;
                }
                false
            }
            Stmt::AssignOp { target, op, value } => {
                let target_has_optional = Self::expr_contains_optional(*target, body);
                if target_has_optional {
                    self.in_optional_chain += 1;
                }
                let target_ty = self.infer_expr(*target, body);
                let value_ty = self.infer_expr(*value, body);
                let binary_op = Self::assign_op_to_binary_op(*op);
                // When the target is behind `?.`, the type is T? but the compound
                // op only executes when the chain is non-null, so arithmetic should
                // see T, not T?.
                let effective_target_ty = if target_has_optional {
                    crate::narrowing::remove_null(&target_ty)
                } else {
                    target_ty
                };
                let result_ty =
                    self.infer_binary_op(binary_op, &effective_target_ty, &value_ty, *target);
                // Re-record the value expression with the result type so the
                // display shows the operation result, not the raw RHS literal.
                self.record_expr_type(*value, result_ty);
                if target_has_optional {
                    self.in_optional_chain -= 1;
                }
                false
            }
            Stmt::Break | Stmt::Continue => true, // break/continue diverge
            Stmt::Missing | Stmt::HeaderComment { .. } => false,
        }
    }

    fn expr_accepts_partial_pattern_expected(expr_id: ExprId, body: &ExprBody) -> bool {
        matches!(
            body.exprs[expr_id],
            Expr::Call { .. } | Expr::OptionalCall { .. }
        )
    }

    // ── Early-return narrowing ────────────────────────────────────────────────

    /// Type-check a statement, applying early-return narrowing when applicable.
    ///
    /// This wraps `check_stmt` and adds special handling for the pattern:
    ///
    /// ```baml
    /// if (x == null) { return ...; }
    /// // x is non-null here
    /// ```
    ///
    /// When a `Stmt::Expr(Expr::If { ... })` is processed:
    /// - If the then-branch always diverges (return/break/continue)
    /// - And the overall statement does NOT diverge (no else, or else does not diverge)
    ///
    /// Then the else-branch narrowings are applied to the locals map, narrowing
    /// the variable types for the remainder of the enclosing block.
    ///
    /// For all other statements, delegates to `check_stmt`.
    fn check_stmt_with_early_return_narrowing(&mut self, stmt_id: StmtId, body: &ExprBody) -> bool {
        let stmt = &body.stmts[stmt_id];

        // Only special-case `Stmt::Expr(Expr::If { ... })`
        if let baml_compiler2_ast::Stmt::Expr(if_expr_id) = stmt {
            let if_expr = &body.exprs[*if_expr_id];
            if let Expr::If {
                condition,
                then_branch,
                else_branch,
            } = if_expr
            {
                let condition = *condition;
                let then_branch = *then_branch;
                let else_branch = *else_branch;

                // Infer the condition to populate its type in self.expressions.
                // (infer_expr for the full Expr::If will also do this, but we
                // need narrowings before calling check_stmt.)
                //
                // We call check_stmt normally — it calls infer_expr(Expr::If),
                // which already applies and restores narrowings for the branches.
                // After check_stmt returns, we check if the then-branch diverged
                // and, if so, apply the else-narrowings permanently.

                // Extract narrowings from the condition. We need the condition
                // type to be recorded first, so we infer it here. Note that
                // infer_expr for the Expr::If will re-infer it (idempotent: the
                // type is recorded and cached in self.expressions).
                self.infer_expr(condition, body);
                let narrowings = crate::narrowing::extract_narrowings(
                    condition,
                    body,
                    &self.expressions,
                    &self.pattern_types,
                );

                // Run the normal check_stmt (which handles the full Expr::If
                // including inner narrowing for the branches).
                let stmt_diverges = self.check_stmt(stmt_id, body);

                // After check_stmt, inspect whether the then-branch diverged.
                // If it did diverge AND the overall if didn't (either no else
                // branch, or else also diverged but then the whole stmt would
                // have diverged too), apply the else-narrowings to locals.
                if !narrowings.is_empty() {
                    let then_ty = self.expressions.get(&then_branch);
                    let then_diverged = matches!(then_ty, Some(Ty::Never { .. }));

                    if then_diverged && !stmt_diverges {
                        // The then-branch always diverges but execution can
                        // continue after this statement — so the else-narrowings
                        // now hold for the rest of the block.
                        crate::narrowing::apply_post_diverge_narrowings(
                            &narrowings,
                            &mut self.locals,
                        );
                    }

                    // If there's no else and the then-branch diverges, we also
                    // want to apply the else-narrowing even when the overall if
                    // might not diverge (it diverges only if then always diverges
                    // and there's no else, which is covered above).
                    let _ = else_branch; // already handled via stmt_diverges check
                }

                return stmt_diverges;
            }
        }

        // Default: delegate to check_stmt
        self.check_stmt(stmt_id, body)
    }

    // ── Helper methods ────────────────────────────────────────────────────────

    /// Look up the *declared* type of an assignment target.
    ///
    /// Returns the original type from the parameter annotation or `let` type
    /// annotation — unaffected by narrowing. Returns `None` for unannotated
    /// let-bindings (including evolving containers) or non-simple targets.
    fn get_declared_type(&self, target: ExprId, body: &ExprBody) -> Option<Ty> {
        if let Expr::Path(segments) = &body.exprs[target] {
            if segments.len() == 1 {
                return self
                    .locals
                    .get(&segments[0])
                    .and_then(|binding| binding.declared_ty.clone());
            }
        }
        None
    }

    fn report_refutable_pattern_in_irrefutable_context(
        &mut self,
        pattern: PatId,
        fallback_expr: Option<ExprId>,
        context: IrrefutableContextKind,
    ) {
        let err = TirTypeError::RefutablePatternInLet { context };
        if let Some(sm) = self.body_source_map.as_ref() {
            self.context.report_at_span(err, sm.pattern_span(pattern));
        } else if let Some(expr) = fallback_expr {
            self.context.report_simple(err, expr);
        }
    }

    /// Report `err` at the source span of `pat_id` if a body source map is
    /// available, otherwise fall back to anchoring at `fallback_expr`.
    /// Encapsulates an idiom that recurs throughout pattern lowering.
    fn report_at_pat_or_expr(&mut self, err: TirTypeError, pat_id: PatId, fallback_expr: ExprId) {
        if let Some(sm) = self.body_source_map.as_ref() {
            self.context.report_at_span(err, sm.pattern_span(pat_id));
        } else {
            self.context.report_simple(err, fallback_expr);
        }
    }

    fn infer_match_expr(
        &mut self,
        match_expr_id: ExprId,
        scrutinee_expr_id: ExprId,
        arms: &[baml_compiler2_ast::MatchArmId],
        body: &ExprBody,
    ) -> Ty {
        let scrutinee_ty = self.infer_expr(scrutinee_expr_id, body);
        let scrutinee_name = match &body.exprs[scrutinee_expr_id] {
            Expr::Path(segments) if segments.len() == 1 => Some(segments[0].clone()),
            _ => None,
        };

        // Pass 1: lower each arm's pattern to a DPat, register bindings,
        // narrow the scrutinee for the body, and infer the body's type.
        // Collect DPats for the matrix run in pass 2.
        //
        // We track non-guarded arms separately for usefulness: guarded
        // arms can never cover values themselves (the guard might fail),
        // so they're irrelevant to exhaustiveness coverage.
        let mut arm_types = Vec::with_capacity(arms.len());
        let mut matrix_arms: Vec<crate::exhaustiveness::DPat> = Vec::new();
        // Map matrix index → source arm body ExprId for unreachable-arm
        // diagnostics. We only push non-guarded arms into the matrix.
        let mut matrix_arm_ids: Vec<ExprId> = Vec::new();

        for arm_id in arms {
            let arm = &body.match_arms[*arm_id];
            let pattern_id = arm.pattern;

            let result = self.analyze_and_lower(pattern_id, &scrutinee_ty, body, arm.body);
            let narrowed = result.matched_ty.clone();

            // Snapshot/restore the scope for this arm's bindings.
            let snapshot = self.snapshot_scoped_locals();

            // Narrow the scrutinee local for the arm body.
            if let Some(name) = &scrutinee_name {
                self.narrow_local(name.clone(), narrowed.clone());
            }

            self.finalize_pattern_lowering(pattern_id, &result, None, None, &scrutinee_ty);

            if let Some(guard_expr) = arm.guard {
                self.infer_expr(guard_expr, body);
            }

            let arm_ty = self.infer_expr(arm.body, body);
            arm_types.push(arm_ty);

            self.restore_scoped_locals(&snapshot);

            // Guarded arms don't contribute to coverage but still need to
            // appear in the matrix at their source position so we can
            // detect unreachability of *later* arms (a guard doesn't
            // cover, but it doesn't make the arm unreachable either).
            // For now, drop guarded arms from coverage analysis entirely
            // — matches existing behaviour where guard suppressed both
            // coverage and unreachable detection for that arm.
            if arm.guard.is_none() {
                matrix_arms.push(result.dpat);
                matrix_arm_ids.push(arm.body);
            }
        }

        // Pass 2: run the matrix algorithm once on all non-guarded arms.
        // Normalize the scrutinee for the matrix only — `Optional<T>` is
        // treated as `Union<T, null>` so UnionMember dispatch covers
        // both branches. The `NonExhaustiveMatch` diagnostic below still
        // carries the original `scrutinee_ty` for display.
        let scrutinee_ty_for_matrix = self.matrix_normalize_scrut(&scrutinee_ty);
        let report = crate::pattern_lowering::compute_match_usefulness(
            self,
            &matrix_arms,
            scrutinee_ty_for_matrix,
        );

        // Exhaustiveness diagnostic.
        if report.missing.is_empty() {
            self.exhaustive_matches.insert(match_expr_id);
        } else {
            let missing: Vec<String> = report
                .missing
                .iter()
                .map(|w| self.render_witness_pat(w))
                .collect();
            self.context.report_simple(
                TirTypeError::NonExhaustiveMatch {
                    scrutinee_type: scrutinee_ty,
                    missing_cases: missing,
                },
                match_expr_id,
            );
        }

        // Unreachable-arm diagnostic. ArmId in the report indexes into
        // matrix_arms (not the original arm list), so we look up the
        // source body ExprId via matrix_arm_ids.
        for arm in report.unreachable_arms {
            if let Some(&body_expr) = matrix_arm_ids.get(arm.0) {
                self.context
                    .report_simple(TirTypeError::UnreachableArm, body_expr);
            }
        }

        Self::join_all(&arm_types)
    }

    /// Type-check `<expr> is <pattern>` (Rust `matches!`-style pattern test).
    ///
    /// Always evaluates to `bool`. Unlike `match`:
    ///   - no exhaustiveness check — there's only one pattern, the rest is "false"
    ///   - no pattern-vs-scrutinee subtype check — `v is string` for `v: int` is
    ///     legal, it just always evaluates to `false`
    ///   - pattern bindings are restricted to the pattern itself and discarded,
    ///     so the surrounding scope never sees them (use `match` / `if let` for
    ///     binding semantics)
    ///
    /// We still lower the pattern (records `pattern_types` so LSP/MIR/codegen
    /// can read the per-PatId type) and infer the scrutinee — that's how we
    /// keep "unresolved type" diagnostics inside the pattern working.
    fn infer_is_expr(
        &mut self,
        scrutinee_expr_id: ExprId,
        pattern_id: PatId,
        body: &ExprBody,
    ) -> Ty {
        let scrutinee_ty = self.infer_expr(scrutinee_expr_id, body);

        // Snapshot the scope so pattern bindings don't leak out — `is` is a
        // test, not a binder.
        let snapshot = self.snapshot_scoped_locals();
        let result = self.analyze_and_lower_no_subtype_check(
            pattern_id,
            &scrutinee_ty,
            body,
            scrutinee_expr_id,
        );
        self.finalize_pattern_lowering(pattern_id, &result, None, None, &scrutinee_ty);
        self.restore_scoped_locals(&snapshot);

        Ty::Primitive(PrimitiveType::Bool, TyAttr::default())
    }

    fn infer_catch_expr(
        &mut self,
        catch_expr_id: ExprId,
        base_expr_id: ExprId,
        clauses: &[baml_compiler2_ast::CatchClause],
        body: &ExprBody,
        expected: Option<&Ty>,
    ) -> Ty {
        let base_ty = if let Some(expected) = expected {
            self.check_expr(base_expr_id, body, expected)
        } else {
            self.infer_expr(base_expr_id, body)
        };
        let mut result_members = vec![base_ty];
        let mut residual = self.catch_base_throw_types(base_expr_id, body);

        for clause in clauses {
            // Compute the clause-level binding type from the current residual throw set.
            // This is the type of the error variable (e.g. `e` in `catch (e)`).
            let clause_binding_ty = if residual.is_empty() {
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            } else {
                Self::facts_to_ty(&residual)
            };
            // Record the clause binding type in the bindings map so MIR can read it.
            self.pattern_types
                .insert(clause.binding, clause_binding_ty.clone());

            // Type the optional stack trace binding as baml.errors.StackTrace.
            //
            // The stack-trace binding's lifetime is the catch-clause body.
            // We snapshot scoped locals before introducing it and restore
            // after the clause's arms finish, so the binding does not leak
            // into the rest of the function.
            let st_snapshot = clause
                .stack_trace_binding
                .is_some()
                .then(|| self.snapshot_scoped_locals());
            if let Some(st_binding) = clause.stack_trace_binding {
                let db = self.context.db();
                let baml_name = baml_base::Name::new("baml");
                let st_ty = self
                    .res_ctx
                    .items_for_package(db, &baml_name)
                    .and_then(|items| {
                        let errors_ns = [baml_base::Name::new("errors")];
                        let st_name = baml_base::Name::new("StackTrace");
                        items.lookup_type(&errors_ns, &st_name)
                    })
                    .map(|def| {
                        let st_name = baml_base::Name::new("StackTrace");
                        Ty::Class(
                            crate::lower_type_expr::qualify_def(db, def, &st_name),
                            Vec::new(),
                            TyAttr::default(),
                        )
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                // Register the stack-trace name through declare_scoped_local
                // so name resolution finds it AND so the binding is unwound
                // by the matching restore_scoped_locals at the end of the
                // clause. A prior raw `self.locals.insert` had no paired
                // snapshot/restore at all and leaked the binding into the
                // rest of the function.
                let st_result = self.analyze_and_lower(st_binding, &st_ty, body, base_expr_id);
                self.finalize_pattern_lowering(st_binding, &st_result, None, None, &st_ty);
            }

            // The clause may carry an annotation: `catch (e: SomeError)`.
            // Reject catch-anything bindings (`unknown`, `any`) before per-arm
            // flow analysis records the actual narrowed binding type.
            if let Some(clause_constraint) = self.pattern_expected_ty(clause.binding, body)
                && let Some(banned) =
                    crate::throw_inference::is_banned_catch_binding_type(clause_constraint.ty())
            {
                self.context.report_simple(
                    TirTypeError::InvalidCatchBindingType {
                        type_name: banned.to_string(),
                    },
                    base_expr_id,
                );
            }

            for &arm_id in &clause.arms {
                let arm = &body.catch_arms[arm_id];
                let arm_expected_ty = self
                    .pattern_expected_ty(arm.pattern, body)
                    .map(PatternExpectedTy::into_ty);
                // Probe the arm pattern's matched type for narrowing.
                // Bindings/refutability are handled by the per-arm walk
                // below, after the narrowed type is finalised.
                let arm_probe = self.analyze_and_lower_no_subtype_check(
                    arm.pattern,
                    &clause_binding_ty,
                    body,
                    arm.body,
                );
                let mut narrowed_ty = arm_probe.matched_ty.clone();
                let written_arm_ty = arm_expected_ty
                    .clone()
                    .unwrap_or_else(|| narrowed_ty.clone());

                let panic_subset_ty = arm_expected_ty
                    .as_ref()
                    .and_then(|ty| self.ty_panic_subset(ty))
                    .or_else(|| self.ty_panic_subset(&narrowed_ty));
                if let Some(panic_subset_ty) = panic_subset_ty.clone() {
                    narrowed_ty = if matches!(narrowed_ty, Ty::Never { .. }) {
                        panic_subset_ty
                    } else {
                        Self::join_all(&[narrowed_ty, panic_subset_ty])
                    };
                }

                let throw_matches = Self::throw_matches_from_ty(&narrowed_ty, &residual);
                let panic_subset_ty = self.ty_panic_subset(&narrowed_ty);
                let has_panic_component = panic_subset_ty.is_some();

                if throw_matches.may_match.is_empty() && !has_panic_component {
                    self.context
                        .report_warning_simple(TirTypeError::UnreachableArm, arm.body);
                }

                let mut narrowed_binding_ty = Self::facts_to_ty(&throw_matches.may_match);
                if let Some(panic_subset_ty) = panic_subset_ty {
                    narrowed_binding_ty = if matches!(narrowed_binding_ty, Ty::Never { .. }) {
                        panic_subset_ty
                    } else {
                        Self::join_all(&[narrowed_binding_ty, panic_subset_ty])
                    };
                }

                let catch_binding_ty = |fallback: Ty| -> Ty {
                    if matches!(narrowed_binding_ty, Ty::Never { .. }) {
                        fallback
                    } else {
                        narrowed_binding_ty.clone()
                    }
                };

                // Route catch-arm bindings through
                // snapshot/declare_scoped_local/restore so the arm pattern's
                // PatId is recorded.
                let arm_snapshot = self.snapshot_scoped_locals();

                // Register clause-level binding with the runtime-narrowed
                // type for this arm, then arm-level bindings (if any).
                let clause_flow = catch_binding_ty(written_arm_ty.clone());
                let clause_result =
                    self.analyze_and_lower(clause.binding, &clause_flow, body, arm.body);
                self.finalize_pattern_lowering(
                    clause.binding,
                    &clause_result,
                    None,
                    None,
                    &clause_flow,
                );

                let arm_flow = catch_binding_ty(written_arm_ty);
                let arm_result =
                    self.analyze_and_lower_no_subtype_check(arm.pattern, &arm_flow, body, arm.body);
                self.finalize_pattern_lowering(arm.pattern, &arm_result, None, None, &arm_flow);

                let arm_ty = self.infer_expr(arm.body, body);
                result_members.push(arm_ty);

                self.restore_scoped_locals(&arm_snapshot);
                // `restore_scoped_locals` rolls back `locals`, but
                // `pattern_types` is global state that the per-arm
                // `finalize_pattern_lowering(clause.binding, ...)` mutated.
                // Without this re-insertion, MIR/LSP would see whichever arm
                // ran last as the clause header binding's type, even though
                // the clause-level binding outlives any single arm.
                self.pattern_types
                    .insert(clause.binding, clause_binding_ty.clone());

                for handled in &throw_matches.definitely_handled {
                    residual.remove(handled);
                }
            }

            // Restore the snapshot taken before the clause's stack-trace
            // binding was introduced. This unwinds the stack-trace name from
            // `locals` so it does not leak past the clause.
            if let Some(snapshot) = st_snapshot {
                self.restore_scoped_locals(&snapshot);
            }

            if matches!(
                clause.kind,
                baml_compiler2_ast::CatchClauseKind::CatchAll
                    | baml_compiler2_ast::CatchClauseKind::CatchAllPanics
            ) {
                residual.clear();
            }
        }

        self.catch_residual_throws
            .insert(catch_expr_id, residual.clone());
        Self::join_all(&result_members)
    }

    /// Validate declared `throws` against effective escaping throws from the body.
    ///
    /// `warn_extraneous` controls whether a warning is emitted when the declared
    /// throws clause contains types that never actually escape from the body.
    /// Pass `false` for auto-derived methods whose throws clause is declared
    /// conservatively rather than in response to what the body actually throws.
    pub fn check_throws_contract(
        &mut self,
        body: &ExprBody,
        declared_throws: Option<&TypeExpr>,
        throws_span: Option<TextRange>,
        fallback_span: TextRange,
        warn_extraneous: bool,
    ) {
        let Some(declared_expr) = declared_throws else {
            return;
        };

        let mut diags = Vec::new();
        let declared_ty = crate::lower_type_expr::lower_type_expr_in_ns(
            self.context.db(),
            declared_expr,
            self.package_items,
            &self.ns_context,
            &self.generic_params,
            &mut diags,
        );
        let span = throws_span.unwrap_or(fallback_span);
        for diag in diags {
            self.context.report_at_span(diag, span);
        }
        self.check_throws_surface(body, &declared_ty, span, warn_extraneous);
    }

    // ====================================================================
    // Pattern type-checking primitives
    // ====================================================================
    //
    // Pattern analysis is contextual. `pattern_expected_ty` extracts the
    // informative type pieces for bidirectional checking, then
    // `analyze_pattern` walks once with the incoming value type to compute
    // matched type, bindings, coverage, and PatId -> Ty output.

    /// Resolve a class-pattern head (and any generic args) to a `Ty`.
    ///
    /// `anchor` controls diagnostic placement:
    ///   - `None`: silent (used when computing a pattern's natural type for
    ///     subtype checks — we don't want to double-report).
    ///   - `Some((pat_id, fallback_expr))`: anchor unresolved-name / type-
    ///     mismatch diagnostics at the pattern's source span via
    ///     `report_at_pat_or_expr`, falling back to `fallback_expr` only
    ///     when the source map has no span for `pat_id`.
    fn resolve_class_pattern_type(
        &mut self,
        class: &[Name],
        generic_args: &[TypeExpr],
        anchor: Option<(PatId, ExprId)>,
    ) -> Ty {
        if !generic_args.is_empty() {
            let ty_expr = TypeExpr::Path {
                segments: class.to_vec(),
                generic_args: generic_args.to_vec(),
                attrs: Vec::new(),
            };
            let ty = if let Some((pat_id, fallback)) = anchor {
                self.resolve_type_expr_at_pat(&ty_expr, pat_id, fallback)
            } else {
                self.resolve_type_expr_silent(&ty_expr)
            };
            if matches!(ty, Ty::Class(..) | Ty::Unknown { .. } | Ty::Error { .. }) {
                return ty;
            }
            if let Some((pat_id, fallback)) = anchor {
                self.report_at_pat_or_expr(
                    TirTypeError::TypeMismatch {
                        expected: Ty::Type {
                            attr: TyAttr::default(),
                        },
                        got: ty,
                    },
                    pat_id,
                    fallback,
                );
            }
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }

        if let Some((_source, ty @ Ty::Class(..))) =
            self.res_ctx
                .resolve_type(self.context.db(), class, &self.ns_context)
        {
            return ty;
        }

        if let Some((pat_id, fallback)) = anchor {
            let lookup = class.last().cloned().unwrap_or_else(|| Name::new("_"));
            self.report_at_pat_or_expr(
                TirTypeError::UnresolvedName { name: lookup },
                pat_id,
                fallback,
            );
        }
        Ty::Unknown {
            attr: TyAttr::default(),
        }
    }

    fn class_pattern_missing_generic_args(&self, class_ty: &Ty, generic_args: &[TypeExpr]) -> bool {
        let Ty::Class(qn, args, _) = class_ty else {
            return false;
        };
        if !generic_args.is_empty() {
            return false;
        }
        if !qn.generic_params.is_empty() || args.iter().any(crate::generics::contains_typevar) {
            return true;
        }
        let Some(pkg_items) = self.resolve_class_pkg_items(qn.package()) else {
            return false;
        };
        let Some(Definition::Class(class_loc)) = pkg_items.lookup_type(qn.namespace(), qn.name())
        else {
            return false;
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        !item_tree[class_loc.id(db)].generic_params.is_empty()
    }

    fn ty_contains_recovery_unknown(ty: &Ty) -> bool {
        match ty {
            Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. } => true,
            Ty::Class(_, args, _) | Ty::Union(args, _) => {
                args.iter().any(Self::ty_contains_recovery_unknown)
            }
            Ty::List(elem, _) | Ty::EvolvingList(elem, _) | Ty::Optional(elem, _) => {
                Self::ty_contains_recovery_unknown(elem)
            }
            Ty::Map(key, value, _) | Ty::EvolvingMap(key, value, _) => {
                Self::ty_contains_recovery_unknown(key) || Self::ty_contains_recovery_unknown(value)
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params
                    .iter()
                    .any(|param| Self::ty_contains_recovery_unknown(&param.ty))
                    || Self::ty_contains_recovery_unknown(ret)
                    || Self::ty_contains_recovery_unknown(throws)
            }
            Ty::Future(value, error, _) => {
                Self::ty_contains_recovery_unknown(value)
                    || Self::ty_contains_recovery_unknown(error)
            }
            Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::TypeAlias(..)
            | Ty::Primitive(..)
            | Ty::Literal(..)
            | Ty::TypeVar(..)
            | Ty::Never { .. }
            | Ty::Void { .. }
            | Ty::RustType { .. }
            | Ty::Type { .. } => false,
        }
    }

    fn ty_contains_unfilled_generic_class(ty: &Ty) -> bool {
        match ty {
            Ty::Class(qn, type_args, _) => {
                type_args.is_empty() && !qn.generic_params.is_empty()
                    || type_args
                        .iter()
                        .any(Self::ty_contains_unfilled_generic_class)
            }
            Ty::Union(args, _) => args.iter().any(Self::ty_contains_unfilled_generic_class),
            Ty::List(elem, _) | Ty::EvolvingList(elem, _) | Ty::Optional(elem, _) => {
                Self::ty_contains_unfilled_generic_class(elem)
            }
            Ty::Map(key, value, _) | Ty::EvolvingMap(key, value, _) => {
                Self::ty_contains_unfilled_generic_class(key)
                    || Self::ty_contains_unfilled_generic_class(value)
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params
                    .iter()
                    .any(|param| Self::ty_contains_unfilled_generic_class(&param.ty))
                    || Self::ty_contains_unfilled_generic_class(ret)
                    || Self::ty_contains_unfilled_generic_class(throws)
            }
            Ty::Future(value, error, _) => {
                Self::ty_contains_unfilled_generic_class(value)
                    || Self::ty_contains_unfilled_generic_class(error)
            }
            Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::TypeAlias(..)
            | Ty::Primitive(..)
            | Ty::Literal(..)
            | Ty::TypeVar(..)
            | Ty::Unknown { .. }
            | Ty::BuiltinUnknown { .. }
            | Ty::Never { .. }
            | Ty::Void { .. }
            | Ty::Error { .. }
            | Ty::RustType { .. }
            | Ty::Type { .. } => false,
        }
    }

    fn intersect_pattern_flow_types(&self, incoming: &Ty, constraint: &Ty) -> Ty {
        if matches!(incoming, Ty::Unknown { .. } | Ty::Error { .. }) {
            return constraint.clone();
        }
        if matches!(constraint, Ty::Unknown { .. } | Ty::Error { .. }) {
            return incoming.clone();
        }
        self.intersect_types(incoming, constraint)
    }

    /// Compute the type a pattern expects of its scrutinee, if it can act
    /// as a useful bidirectional hint. Returns `None` for unconstrained
    /// patterns (wildcard / bare bind / generic class missing type args /
    /// array of bare binds), or for derived types polluted by recovery
    /// `Unknown` / unfilled generic class. Returns `Full(ty)` when every
    /// contributing leaf was constrained, `Partial(ty)` when an Or branch
    /// was unconstrained — see [`PatternExpectedTy`] for what the consumer
    /// does with each.
    ///
    /// Recursive sub-pattern lookups go through this same function (and
    /// project to a raw `Ty` via [`PatternExpectedTy::into_ty`]) so the
    /// recovery-malformedness filter applies uniformly per element / per
    /// branch.
    fn pattern_expected_ty(&mut self, pat_id: PatId, body: &ExprBody) -> Option<PatternExpectedTy> {
        let ty = match &body.patterns[pat_id].clone() {
            // No constraint contributed.
            ast::Pattern::Wildcard => return None,
            // `let x` → no constraint; `let x: <subpat>` → defer.
            ast::Pattern::Bind { subpat, .. } => {
                return subpat.and_then(|sp| self.pattern_expected_ty(sp, body));
            }
            ast::Pattern::Type(t) => self.resolve_type_expr_silent(t),
            ast::Pattern::Class {
                class,
                generic_args,
                ..
            } => {
                let class_ty = self.resolve_class_pattern_type(class, generic_args, None);
                if self.class_pattern_missing_generic_args(&class_ty, generic_args) {
                    return None;
                }
                class_ty
            }
            ast::Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            } => {
                // Explicit `: T` ascription on the whole array wins over
                // element-derived inference.
                if let Some(ty_expr) = ascription {
                    self.resolve_type_expr_silent(ty_expr)
                } else {
                    let mut elem_tys = Vec::new();
                    for p in prefix.iter().chain(suffix.iter()) {
                        if let Some(ty) = self
                            .pattern_expected_ty(*p, body)
                            .map(PatternExpectedTy::into_ty)
                        {
                            elem_tys.push(ty);
                        }
                    }
                    if let Some(rp) = rest
                        && let Some(rest_pat) = rp.pat
                        && let Some(ty) = self
                            .pattern_expected_ty(rest_pat, body)
                            .map(PatternExpectedTy::into_ty)
                        && let Ty::List(elem, _) | Ty::EvolvingList(elem, _) = ty
                    {
                        elem_tys.push(*elem);
                    }
                    let elem_ty = (!elem_tys.is_empty()).then(|| Self::join_all(&elem_tys))?;
                    Ty::List(Box::new(elem_ty), TyAttr::default())
                }
            }
            // Or has its own Full/Partial tracking and returns directly —
            // its branches' joined type is not subjected to the recovery
            // filter applied below (every branch already went through the
            // filter individually via the recursive call).
            ast::Pattern::Or(parts) => {
                let mut tys = Vec::new();
                let mut is_full = true;
                for part in parts {
                    match self.pattern_expected_ty(*part, body) {
                        Some(PatternExpectedTy::Full(ty)) => tys.push(ty),
                        Some(PatternExpectedTy::Partial(ty)) => {
                            is_full = false;
                            tys.push(ty);
                        }
                        None => is_full = false,
                    }
                }
                let ty = (!tys.is_empty()).then(|| Self::join_all(&tys))?;
                return Some(if is_full {
                    PatternExpectedTy::Full(ty)
                } else {
                    PatternExpectedTy::Partial(ty)
                });
            }
        };

        if Self::ty_contains_recovery_unknown(&ty) || Self::ty_contains_unfilled_generic_class(&ty)
        {
            None
        } else {
            Some(PatternExpectedTy::Full(ty))
        }
    }

    fn pattern_contains_structural_syntax(pat_id: PatId, body: &ExprBody) -> bool {
        match &body.patterns[pat_id] {
            ast::Pattern::Class { .. } | ast::Pattern::Array { .. } => true,
            ast::Pattern::Or(parts) => parts
                .iter()
                .any(|part| Self::pattern_contains_structural_syntax(*part, body)),
            ast::Pattern::Wildcard | ast::Pattern::Bind { .. } | ast::Pattern::Type(_) => false,
        }
    }

    fn check_or_binding_type_compatibility(
        &mut self,
        bindings_by_name: &FxHashMap<Name, Vec<(PatId, Ty)>>,
        at_expr: ExprId,
    ) {
        for (name, entries) in bindings_by_name {
            let Some((_, first_ty)) = entries.first() else {
                continue;
            };
            for (pat, other_ty) in entries.iter().skip(1) {
                if Self::ty_contains_recovery_unknown(first_ty)
                    || Self::ty_contains_recovery_unknown(other_ty)
                {
                    continue;
                }
                if !self.is_subtype(first_ty, other_ty) || !self.is_subtype(other_ty, first_ty) {
                    let err = TirTypeError::OrPatternBindingTypeMismatch {
                        name: name.clone(),
                        first_type: first_ty.clone(),
                        other_type: other_ty.clone(),
                    };
                    self.report_at_pat_or_expr(err, *pat, at_expr);
                }
            }
        }
    }

    /// Resolve a `TypeExpr` to a `Ty`.  Tries `bare_type_sugar_to_ty` first
    /// (handles `baml.panics.*` types and primitives), falls back to
    /// `lower_pattern_type_expr` for user-defined types.
    fn resolve_type_expr(&mut self, ty: &TypeExpr, at_expr: ExprId) -> Ty {
        if let TypeExpr::Path { segments, .. } = ty {
            if segments.len() == 1 {
                if let Some(resolved) = bare_type_sugar_to_ty(&segments[0]) {
                    return resolved;
                }
            }
        }
        self.lower_pattern_type_expr(ty, at_expr)
    }

    fn resolve_type_expr_silent(&self, ty: &TypeExpr) -> Ty {
        if let TypeExpr::Path { segments, .. } = ty {
            if segments.len() == 1 {
                if let Some(resolved) = bare_type_sugar_to_ty(&segments[0]) {
                    return resolved;
                }
            }
        }
        let mut diags = Vec::new();
        crate::lower_type_expr::lower_type_expr_in_ns(
            self.context.db(),
            ty,
            self.package_items,
            &self.ns_context,
            &self.generic_params,
            &mut diags,
        )
    }

    fn lower_pattern_type_expr(&mut self, expr: &TypeExpr, at_expr: ExprId) -> Ty {
        let mut diags = Vec::new();
        let ty = crate::lower_type_expr::lower_type_expr_in_ns(
            self.context.db(),
            expr,
            self.package_items,
            &self.ns_context,
            &self.generic_params,
            &mut diags,
        );
        for diag in diags {
            self.context.report_simple(diag, at_expr);
        }
        ty
    }

    /// Same as [`Self::resolve_type_expr`], but anchors diagnostics at the
    /// pattern's source span (via [`Self::report_at_pat_or_expr`]) instead
    /// of falling all the way back to the surrounding scrutinee
    /// expression. Used by pattern-lowering call sites where we have a
    /// `PatId` in scope so the squiggle lands on the actual type name.
    fn resolve_type_expr_at_pat(
        &mut self,
        ty: &TypeExpr,
        pat_id: PatId,
        fallback_expr: ExprId,
    ) -> Ty {
        if let TypeExpr::Path { segments, .. } = ty {
            if segments.len() == 1 {
                if let Some(resolved) = bare_type_sugar_to_ty(&segments[0]) {
                    return resolved;
                }
            }
        }
        let mut diags = Vec::new();
        let resolved = crate::lower_type_expr::lower_type_expr_in_ns(
            self.context.db(),
            ty,
            self.package_items,
            &self.ns_context,
            &self.generic_params,
            &mut diags,
        );
        for diag in diags {
            self.report_at_pat_or_expr(diag, pat_id, fallback_expr);
        }
        resolved
    }

    fn ty_panic_subset(&self, ty: &Ty) -> Option<Ty> {
        match ty {
            Ty::Class(qtn, _, _) => qtn.is_panic_type().then(|| ty.clone()),
            Ty::TypeAlias(qtn, _) => {
                if let Some(expanded) = self.aliases.get(qtn) {
                    self.ty_panic_subset(expanded)
                } else if qtn.is_panic_type() {
                    Some(ty.clone())
                } else {
                    None
                }
            }
            Ty::Union(parts, _) => {
                let panic_members: Vec<_> = parts
                    .iter()
                    .filter_map(|part| self.ty_panic_subset(part))
                    .collect();
                if panic_members.is_empty() {
                    None
                } else {
                    Some(Self::join_all(&panic_members))
                }
            }
            _ => None,
        }
    }

    fn catch_base_throw_types(&self, base_expr_id: ExprId, body: &ExprBody) -> BTreeSet<Ty> {
        let mut out = BTreeSet::new();
        self.collect_throw_facts_from_expr(base_expr_id, body, &mut out);
        out
    }

    /// Join a set of throw fact types into a single type.
    fn facts_to_ty(facts: &BTreeSet<Ty>) -> Ty {
        if facts.is_empty() {
            return Ty::Never {
                attr: TyAttr::default(),
            };
        }
        let tys: Vec<Ty> = facts.iter().cloned().collect();
        Self::join_all(&tys)
    }

    /// Check if a pattern type covers a throw fact type.
    fn ty_covers_fact(pattern_ty: &Ty, fact: &Ty) -> bool {
        if pattern_ty == fact {
            return true;
        }
        match pattern_ty {
            Ty::Primitive(p, _) => match fact {
                Ty::Primitive(fp, _) => p == fp,
                Ty::Literal(lit, _, _) => *p == PrimitiveType::from_literal(lit),
                _ => false,
            },
            Ty::Literal(_, _, _) => false,
            Ty::Optional(inner, _) => {
                matches!(fact, Ty::Primitive(PrimitiveType::Null, _))
                    || Self::ty_covers_fact(inner, fact)
            }
            Ty::Union(parts, _) => parts.iter().any(|part| Self::ty_covers_fact(part, fact)),
            Ty::Class(qn, type_args, _) => matches!(
                fact,
                Ty::Class(fqn, fact_args, _) if fqn == qn && fact_args == type_args
            ),
            Ty::Enum(qn, _) => match fact {
                Ty::Enum(fqn, _) => fqn == qn,
                Ty::EnumVariant(fqn, _, _) => fqn == qn,
                _ => false,
            },
            Ty::TypeAlias(qn, _) => matches!(fact, Ty::TypeAlias(fqn, _) if fqn == qn),
            Ty::EnumVariant(qn, variant, _) => {
                matches!(fact, Ty::EnumVariant(fqn, fv, _) if fqn == qn && fv == variant)
            }
            _ => false,
        }
    }

    fn ty_may_match_fact(pattern_ty: &Ty, fact: &Ty) -> bool {
        match pattern_ty {
            Ty::Literal(lit, _, _) => {
                let widened = Ty::Primitive(PrimitiveType::from_literal(lit), TyAttr::default());
                &widened == fact
                    || matches!(fact, Ty::Primitive(p, _) if *p == PrimitiveType::from_literal(lit))
            }
            Ty::EnumVariant(qn, _, _) => {
                matches!(fact, Ty::Enum(fqn, _) if fqn == qn)
            }
            Ty::BuiltinUnknown { .. } | Ty::Unknown { .. } | Ty::Error { .. } => true,
            _ => false,
        }
    }

    fn ty_match_strength(narrowed_ty: &Ty, throw_fact: &Ty) -> PatternMatchStrength {
        let is_unknown = matches!(
            throw_fact,
            Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. }
        );
        if Self::ty_covers_fact(narrowed_ty, throw_fact) {
            PatternMatchStrength::DefiniteMatch
        } else if is_unknown || Self::ty_may_match_fact(narrowed_ty, throw_fact) {
            PatternMatchStrength::MayMatch
        } else {
            PatternMatchStrength::NoMatch
        }
    }

    fn throw_matches_from_ty(narrowed_ty: &Ty, throw_types: &BTreeSet<Ty>) -> ThrowPatternMatches {
        let mut out = ThrowPatternMatches::default();
        for throw_fact in throw_types {
            match Self::ty_match_strength(narrowed_ty, throw_fact) {
                PatternMatchStrength::NoMatch => {}
                PatternMatchStrength::MayMatch => {
                    out.may_match.insert(throw_fact.clone());
                }
                PatternMatchStrength::DefiniteMatch => {
                    out.may_match.insert(throw_fact.clone());
                    out.definitely_handled.insert(throw_fact.clone());
                }
            }
        }
        out
    }

    fn collect_effective_throws(&self, body: &ExprBody) -> BTreeSet<Ty> {
        crate::throws_analysis::collect_escaping_throws(
            &BuilderThrowsAnalysis { builder: self },
            body,
        )
    }

    fn lookup_named_throw_summary(&self, target: &Name) -> Option<BTreeSet<Ty>> {
        let throws =
            crate::throw_inference::function_throw_sets(self.context.db(), self.package_id);
        if let Some(transitive) = throws.transitive_for(target) {
            return Some(transitive.clone());
        }

        for (_dep_name, dep_iface) in &self.res_ctx.dep_interfaces {
            if let Some(transitive) = dep_iface.throw_sets.transitive_for(target) {
                return Some(transitive.clone());
            }
        }

        None
    }

    fn callee_uses_method_call_convention(&self, callee_expr_id: ExprId) -> bool {
        matches!(
            self.resolutions.get(&callee_expr_id),
            Some(crate::inference::MemberResolution::BoundMethod { .. })
        ) || matches!(
            self.path_member_resolutions
                .get(&callee_expr_id)
                .and_then(|resolutions| resolutions.last()),
            Some(crate::inference::MemberResolution::BoundMethod { .. })
        )
    }

    fn instantiated_callee_throws(
        &self,
        callee_expr_id: ExprId,
        args: &[ExprId],
        unwrap_optional_callee: bool,
        call_plan: Option<&crate::inference::CallPlan>,
    ) -> Option<Ty> {
        let callee_ty = self.expressions.get(&callee_expr_id)?;
        let typed_callee = if unwrap_optional_callee {
            self.analyze_optional_base(callee_ty).inner
        } else {
            callee_ty.clone()
        };

        // When the callee has a union type (e.g., a method called on a union-typed
        // field like `(string | int | MyClass).to_json()`), every member of the union
        // is a separate function that might execute.  Conservatively union all of
        // their throws — if every member is a function we can compute a precise
        // answer; otherwise fall through to return `None` (unknown).
        if let Ty::Union(ref members, _) = typed_callee {
            let mut all_throws: BTreeSet<Ty> = BTreeSet::new();
            for member in members {
                if let Ty::Function { throws, .. } = member {
                    all_throws.extend(crate::throw_inference::flatten_ty_to_facts(throws));
                } else {
                    // At least one member is not a function — can't compute throws,
                    // return None to let the fallback handle it.
                    return None;
                }
            }
            // All members were functions; return the union of their throws.
            return Some(
                Self::ty_from_concrete_facts(&all_throws).unwrap_or(Ty::Never {
                    attr: TyAttr::default(),
                }),
            );
        }

        let Ty::Function { params, throws, .. } = typed_callee else {
            return None;
        };

        let effective_params = if self.callee_uses_method_call_convention(callee_expr_id) {
            crate::generics::skip_self_param(&params)
        } else {
            params.as_slice()
        };

        let mut bindings = FxHashMap::default();
        if let Some(call_plan) = call_plan {
            for (param_index, arg_expr_id) in call_plan.provided_param_args() {
                let Some(param) = effective_params.get(param_index) else {
                    continue;
                };
                let arg_ty = self
                    .expressions
                    .get(&arg_expr_id)
                    .cloned()
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                crate::generics::infer_bindings_allow_typevars(&param.ty, &arg_ty, &mut bindings);
            }
        } else {
            for (param, arg_expr_id) in effective_params.iter().zip(args.iter()) {
                let arg_ty = self
                    .expressions
                    .get(arg_expr_id)
                    .cloned()
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                crate::generics::infer_bindings_allow_typevars(&param.ty, &arg_ty, &mut bindings);
            }
        }

        Some(crate::generics::substitute_ty(&throws, &bindings))
    }

    fn collect_throw_facts_from_expr(
        &self,
        expr_id: ExprId,
        body: &ExprBody,
        out: &mut BTreeSet<Ty>,
    ) {
        match &body.exprs[expr_id] {
            Expr::Throw { value } => {
                self.collect_throw_facts_from_expr(*value, body, out);
                self.collect_throw_facts_from_value(*value, out);
            }
            Expr::Call { callee, args, .. } => {
                self.collect_throw_facts_from_expr(*callee, body, out);
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                for arg in args {
                    self.collect_throw_facts_from_expr(arg.expr, body, out);
                }
                crate::throws_analysis::collect_callee_escaping_throws(
                    &BuilderThrowsAnalysis { builder: self },
                    *callee,
                    &arg_exprs,
                    body,
                    false,
                    out,
                );
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.collect_throw_facts_from_expr(*condition, body, out);
                self.collect_throw_facts_from_expr(*then_branch, body, out);
                if let Some(else_expr) = else_branch {
                    self.collect_throw_facts_from_expr(*else_expr, body, out);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.collect_throw_facts_from_expr(*scrutinee, body, out);
                for arm_id in arms {
                    let arm = &body.match_arms[*arm_id];
                    if let Some(guard) = arm.guard {
                        self.collect_throw_facts_from_expr(guard, body, out);
                    }
                    self.collect_throw_facts_from_expr(arm.body, body, out);
                }
            }
            Expr::Is { scrutinee, .. } => {
                self.collect_throw_facts_from_expr(*scrutinee, body, out);
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.collect_throw_facts_from_expr(*lhs, body, out);
                self.collect_throw_facts_from_expr(*rhs, body, out);
            }
            Expr::Unary { expr, .. } => self.collect_throw_facts_from_expr(*expr, body, out),
            Expr::Object {
                fields, spreads, ..
            } => {
                for (_, value) in fields {
                    self.collect_throw_facts_from_expr(*value, body, out);
                }
                for spread in spreads {
                    self.collect_throw_facts_from_expr(spread.expr, body, out);
                }
            }
            Expr::Array { elements } => {
                for elem in elements {
                    self.collect_throw_facts_from_expr(*elem, body, out);
                }
            }
            Expr::Map { entries } => {
                for (key, value) in entries {
                    self.collect_throw_facts_from_expr(*key, body, out);
                    self.collect_throw_facts_from_expr(*value, body, out);
                }
            }
            Expr::Block { stmts, tail_expr } => {
                for stmt in stmts {
                    self.collect_throw_facts_from_stmt(*stmt, body, out);
                }
                if let Some(tail) = tail_expr {
                    self.collect_throw_facts_from_expr(*tail, body, out);
                }
            }
            Expr::MemberAccess { base, .. } | Expr::OptionalMemberAccess { base, .. } => {
                self.collect_throw_facts_from_expr(*base, body, out);
            }
            Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
                self.collect_throw_facts_from_expr(*base, body, out);
                self.collect_throw_facts_from_expr(*index, body, out);
            }
            Expr::OptionalCall { callee, args } => {
                self.collect_throw_facts_from_expr(*callee, body, out);
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                for arg in args {
                    self.collect_throw_facts_from_expr(arg.expr, body, out);
                }
                crate::throws_analysis::collect_callee_escaping_throws(
                    &BuilderThrowsAnalysis { builder: self },
                    *callee,
                    &arg_exprs,
                    body,
                    true,
                    out,
                );
            }
            Expr::Catch { base, .. } => {
                self.collect_throw_facts_from_expr(*base, body, out);
            }
            Expr::OptionalChain { expr } => {
                self.collect_throw_facts_from_expr(*expr, body, out);
            }
            Expr::Spawn {
                name,
                body: spawn_body,
            } => {
                // Spawn-body throws do NOT escape the spawning function
                // — they are captured into the resulting `Future<T, E>`'s
                // E parameter and only re-thrown at an `await` site. The
                // name expression itself can throw, so walk it; do not
                // walk spawn_body.
                if let Some(name_id) = name {
                    self.collect_throw_facts_from_expr(*name_id, body, out);
                }
                let _ = spawn_body;
            }
            Expr::Await { future } => {
                // `await` re-throws the future's error. Walk the future
                // expression (its construction can throw), AND add the
                // future's E parameter to the throws set so the
                // surrounding function's effective throws includes it.
                self.collect_throw_facts_from_expr(*future, body, out);
                if let Some(Ty::Future(_value, error, _)) = self.expressions.get(future) {
                    out.extend(crate::throw_inference::flatten_ty_to_facts(error));
                }
            }
            Expr::Lambda(_)
            | Expr::Literal(_)
            | Expr::ByteStringLiteral(_)
            | Expr::Null
            | Expr::Path(_)
            | Expr::Missing => {}
        }
    }

    fn collect_throw_facts_from_stmt(
        &self,
        stmt_id: StmtId,
        body: &ExprBody,
        out: &mut BTreeSet<Ty>,
    ) {
        match &body.stmts[stmt_id] {
            Stmt::Expr(expr_id) => self.collect_throw_facts_from_expr(*expr_id, body, out),
            Stmt::Let { initializer, .. } => {
                if let Some(init) = initializer {
                    self.collect_throw_facts_from_expr(*init, body, out);
                }
            }
            Stmt::While {
                condition,
                body: while_body,
                after,
                ..
            } => {
                self.collect_throw_facts_from_expr(*condition, body, out);
                self.collect_throw_facts_from_expr(*while_body, body, out);
                if let Some(after_stmt) = after {
                    self.collect_throw_facts_from_stmt(*after_stmt, body, out);
                }
            }
            Stmt::For {
                collection,
                body: for_body,
                ..
            } => {
                self.collect_throw_facts_from_expr(*collection, body, out);
                self.collect_throw_facts_from_expr(*for_body, body, out);
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.collect_throw_facts_from_expr(*expr, body, out);
                }
            }
            Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
                self.collect_throw_facts_from_expr(*target, body, out);
                self.collect_throw_facts_from_expr(*value, body, out);
            }
            Stmt::Throw { value } => {
                self.collect_throw_facts_from_expr(*value, body, out);
                self.collect_throw_facts_from_value(*value, out);
            }
            Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
        }
    }

    fn collect_throw_facts_from_value(&self, value_expr_id: ExprId, out: &mut BTreeSet<Ty>) {
        let unknown_ty = Ty::Unknown {
            attr: TyAttr::default(),
        };
        let thrown_ty = self.expressions.get(&value_expr_id).unwrap_or(&unknown_ty);
        out.extend(crate::throw_inference::flatten_ty_to_facts(thrown_ty));
    }

    fn call_target_name(&self, callee_expr_id: ExprId, body: &ExprBody) -> Option<Name> {
        let segments = crate::throws_analysis::expr_to_path_segments(callee_expr_id, body)?;
        if segments.len() < 2 {
            // Single-segment path (free function) — return as-is.
            return if segments.is_empty() {
                None
            } else {
                Some(segments[0].clone())
            };
        }
        let method = segments.last().unwrap();

        // Check path_member_resolutions — handles 2+ segment paths correctly.
        // The last resolution for the callee path tells us the receiver class.
        if let Some(resolutions) = self.path_member_resolutions.get(&callee_expr_id) {
            if let Some(
                crate::inference::MemberResolution::BoundMethod { class_loc, .. }
                | crate::inference::MemberResolution::UnboundMethod { class_loc, .. },
            ) = resolutions.last()
            {
                let db = self.context.db();
                let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
                let class_data = &item_tree[class_loc.id(db)];
                let pkg_info =
                    baml_compiler2_hir::file_package::file_package(db, class_loc.file(db));
                let ns = &pkg_info.namespace_path;
                let key = if ns.is_empty() {
                    format!("{}.{}", class_data.name, method)
                } else {
                    let ns_str = ns.iter().map(Name::as_str).collect::<Vec<_>>().join(".");
                    format!("{}.{}.{}", ns_str, class_data.name, method)
                };
                return Some(Name::new(key));
            }
        }

        // Fallback: 2-segment path with locals lookup (existing logic)
        let receiver = &segments[0];
        if let Some(Ty::Class(qn, _, _)) =
            self.locals.get(receiver).map(|binding| &binding.current_ty)
        {
            let ns = qn.namespace();
            let key = if ns.is_empty() {
                format!("{}.{}", qn.name(), method)
            } else {
                let ns_str = ns.iter().map(Name::as_str).collect::<Vec<_>>().join(".");
                format!("{}.{}.{}", ns_str, qn.name(), method)
            };
            Some(Name::new(key))
        } else {
            // Receiver not a known local or not a class — fall back to raw path
            Some(Name::new(
                segments
                    .iter()
                    .map(Name::as_str)
                    .collect::<Vec<_>>()
                    .join("."),
            ))
        }
    }

    fn infer_literal(lit: &baml_base::Literal) -> Ty {
        Ty::Literal(lit.clone(), Freshness::Fresh, TyAttr::default())
    }

    fn infer_path(&mut self, segments: &[Name], _body: &ExprBody, expr_id: ExprId) -> Ty {
        if segments.len() == 1 {
            let name = &segments[0];
            let ty = self.infer_single_name(name);
            // Record free-function resolution so that explicit type-arg binding
            // (`resolve_explicit_type_args`) can later retrieve the `FunctionLoc`
            // without re-doing a package lookup.
            if !self.locals.contains_key(name) {
                if let Some(Definition::Function(func_loc)) =
                    self.package_items.lookup_value(&self.ns_context, name)
                {
                    self.resolutions.insert(
                        expr_id,
                        crate::inference::MemberResolution::Free { func_loc },
                    );
                }
            }
            // Don't report "unresolved name" for dependency package names —
            // they'll be resolved by the parent FieldAccess expression.
            let is_dep_package = self.res_ctx.dep_interfaces.iter().any(|(n, _)| n == name);
            if matches!(ty, Ty::Unknown { .. })
                && !self.locals.contains_key(name)
                && self
                    .package_items
                    .lookup_value(&self.ns_context, name)
                    .is_none()
                && self
                    .package_items
                    .lookup_type(&self.ns_context, name)
                    .is_none()
                && !is_dep_package
            {
                self.context
                    .report_simple(TirTypeError::UnresolvedName { name: name.clone() }, expr_id);
            }
            ty
        } else if segments.len() >= 2 {
            // Dispatch based on whether the root segment is a known local variable.
            // We check self.locals directly rather than going through the HIR
            // path_resolution_query, because ExprIds are per-function-body arenas
            // and are not globally unique across functions in a file.
            if self.locals.contains_key(&segments[0]) {
                // Root is a local variable — chain resolve_member for segments[1..].
                // The last segment resolves as a bound method reference.
                self.infer_local_rooted_path(segments, expr_id, true)
            } else {
                // Root is not a known local. Try full package/namespace resolution:
                // 1. Package path (e.g. baml.llm.ClientType.Primitive, baml.env.get)
                let pkg_ty = self.infer_multi_segment_path(segments, expr_id);
                if !matches!(pkg_ty, Ty::Unknown { .. }) {
                    return pkg_ty;
                }
                // 2. Primitive static access (e.g. image.from_url)
                if segments.len() == 2 {
                    let name = segments[0].as_str();
                    let class_path: &[&str] = match name {
                        "image" => &["media", "Image"],
                        "audio" => &["media", "Audio"],
                        "video" => &["media", "Video"],
                        "pdf" => &["media", "Pdf"],
                        "string" => &["String"],
                        "int" => &["Int"],
                        "float" => &["Float"],
                        _ => &[],
                    };
                    if !class_path.is_empty() {
                        if let Some(ty) =
                            self.resolve_builtin_member(class_path, &[], &segments[1], expr_id)
                        {
                            return ty;
                        }
                    }
                }
                // 3. Type-rooted access: root resolves as a type in the namespace
                //    (e.g. `Status.Active` where Status is an enum, or enum static methods,
                //    or unbound method references like `Person.get_name`).
                //    Use infer_local_rooted_path which chains resolve_member for segments[1..].
                //    root_is_value = false → last segment is an unbound method reference.
                let root_ty = self.infer_single_name(&segments[0]);
                if !matches!(root_ty, Ty::Unknown { .. }) {
                    return self.infer_local_rooted_path(segments, expr_id, false);
                }
                // 4. Truly unresolved — report error if not a known namespace/package name.
                let is_dep_package = self
                    .res_ctx
                    .dep_interfaces
                    .iter()
                    .any(|(n, _)| n == &segments[0]);
                if !is_dep_package && !self.locals.contains_key(&segments[0]) {
                    self.context.report_simple(
                        TirTypeError::UnresolvedName {
                            name: segments[0].clone(),
                        },
                        expr_id,
                    );
                }
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
        } else {
            Ty::Unknown {
                attr: TyAttr::default(),
            }
        }
    }

    /// Infer a local-rooted multi-segment path (e.g. `obj.a.b`).
    ///
    /// Resolves `segments[0]` as a local variable, then chains `resolve_member`
    /// for each subsequent segment. Captures per-segment `MemberResolution`
    /// values into `path_member_resolutions` for MIR field-chain lowering and
    /// LSP navigation.
    fn infer_local_rooted_path(
        &mut self,
        segments: &[Name],
        expr_id: ExprId,
        root_is_value: bool,
    ) -> Ty {
        let root_ty = self.infer_single_name(&segments[0]);
        if matches!(root_ty, Ty::Unknown { .. }) {
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }

        // Record the root segment's TIR type for MIR field-chain lowering.
        // MIR catch variables are declared as BuiltinUnknown, so builder.local_ty()
        // would return a coarser type than TIR inferred here.
        self.path_root_types.insert(expr_id, root_ty.clone());
        self.path_segment_types
            .insert((expr_id, 0), root_ty.clone());

        // Chain resolve_member for remaining segments, capturing per-segment resolutions.
        let mut current_ty = root_ty;
        let mut member_resolutions: Vec<crate::inference::MemberResolution<'db>> = Vec::new();
        let total_segments = segments.len();

        for (i, seg) in segments[1..].iter().enumerate() {
            let seg_idx = i + 1; // index into the path segments (0 is the root)

            // For the last segment: bound iff root was a value (local variable or field chain).
            // Intermediate segments are always field accesses (bound doesn't affect fields).
            let is_last_segment = seg_idx == total_segments - 1;
            let bound = root_is_value || !is_last_segment;

            let inner = crate::narrowing::remove_null(&current_ty);
            // `Primitive(Null)` is a concrete non-optional type with a companion
            // class. Calling methods on it directly (e.g. `n.to_json()`) is valid
            // — do NOT require `?.` chaining.
            let is_pure_null = matches!(current_ty, Ty::Primitive(PrimitiveType::Null, _));
            let is_nullable = inner != current_ty
                && !is_pure_null
                && !matches!(current_ty, Ty::Unknown { .. } | Ty::Error { .. });
            // Use `current_ty` for dispatch when the base is Primitive(Null) (or
            // any non-nullable type): `inner` would be `Never` for Null, which has
            // no members.
            let dispatch_ty = if is_nullable { &inner } else { &current_ty };
            let member_ty;
            if is_nullable {
                if self.in_optional_chain > 0 {
                    // Inside an OptionalChain: resolve and re-wrap the result.
                    member_ty = self.resolve_member_for_path_segment(
                        dispatch_ty,
                        seg,
                        expr_id,
                        seg_idx,
                        bound,
                    );
                    current_ty = Self::make_optional(member_ty.clone());
                } else {
                    // Outside any chain: null-safety violation — suggest `?.`.
                    let base_text = segments[..seg_idx]
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join(".");
                    let member_text = format!(".{}", seg.as_str());
                    let expr_text = segments[..=seg_idx]
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join(".");
                    self.context.report_simple(
                        TirTypeError::NullableMemberAccess {
                            base: base_text,
                            member: member_text,
                            expr: expr_text,
                        },
                        expr_id,
                    );
                    member_ty = self.resolve_member_for_path_segment(
                        dispatch_ty,
                        seg,
                        expr_id,
                        seg_idx,
                        bound,
                    );
                    current_ty = Self::make_optional(member_ty.clone());
                }
            } else {
                member_ty =
                    self.resolve_member_for_path_segment(dispatch_ty, seg, expr_id, seg_idx, bound);
                current_ty = member_ty.clone();
            }

            // Record the type of segments[..=seg_idx] so MIR can read the
            // receiver-prefix type of multi-segment method calls (e.g.
            // `holder.box.describe()` — the prefix is `holder.box` at index 1).
            self.path_segment_types
                .insert((expr_id, seg_idx), current_ty.clone());

            // Capture whatever resolution `resolve_member` (called by
            // `resolve_member_for_path_segment`) stored at `expr_id`.
            // We immediately remove it from `resolutions` so consecutive
            // segments don't see stale values from earlier iterations.
            //
            // Note: the Vec is NOT guaranteed to be parallel to segments[1..] —
            // builtin/primitive members (e.g. String.length) don't record a
            // MemberResolution. Consumers must use `.last()` or iterate by
            // value, not index-based correspondence with segments.
            if let Some(res) = self.resolutions.remove(&expr_id) {
                member_resolutions.push(res);
            }

            if matches!(current_ty, Ty::Unknown { .. }) {
                break;
            }
        }

        if !member_resolutions.is_empty() {
            self.path_member_resolutions
                .insert(expr_id, member_resolutions);
        }

        current_ty
    }

    /// Resolve a multi-segment path like `baml.llm.render_prompt` or `root.sys.panic`.
    ///
    /// The first segment is either a literal package name or `root` (maps to the
    /// current file's package).
    fn infer_multi_segment_path(&mut self, segments: &[Name], expr_id: ExprId) -> Ty {
        let first = &segments[0];
        if self.locals.contains_key(first) {
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }

        let db = self.context.db();
        let pkg_name = if first.as_str() == "root" {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, self.scope.file(db));
            pkg_info.package
        } else {
            first.clone()
        };

        let (pkg_items, item_path): (&baml_compiler2_hir::package::PackageItems<'db>, Vec<Name>) =
            if let Some(items) = self.res_ctx.items_for_package(db, &pkg_name) {
                (items, segments[1..].to_vec())
            } else {
                return Ty::Unknown {
                    attr: TyAttr::default(),
                };
            };

        match self.resolve_package_item(pkg_items, &item_path, expr_id) {
            Some(ty) => ty,
            None => {
                // Find the first invalid segment in the path.
                // The path is [ns_0, ns_1, ..., ns_n, item].
                // Check each prefix of the namespace path to find the first invalid segment.
                if !item_path.is_empty() {
                    let namespace_path = &item_path[..item_path.len() - 1];
                    let mut unresolved_segment: Option<&Name> = Some(item_path.last().unwrap());

                    // Check each prefix of the namespace path
                    for i in 0..namespace_path.len() {
                        let prefix = &namespace_path[..=i];
                        if !pkg_items.namespaces.contains_key(prefix) {
                            // This prefix doesn't exist as a namespace.
                            // Check if namespace_path[i] is a valid type or value in the parent namespace.
                            let parent_ns = &namespace_path[..i];
                            let segment_name = &namespace_path[i];

                            let is_type_or_value =
                                pkg_items.lookup_type(parent_ns, segment_name).is_some()
                                    || pkg_items.lookup_value(parent_ns, segment_name).is_some();

                            if is_type_or_value {
                                // namespace_path[i] is a valid type/value, so the unresolved segment
                                // is the next segment (or the final item if there's no next segment)
                                if i + 1 < namespace_path.len() {
                                    unresolved_segment = Some(&namespace_path[i + 1]);
                                }
                                // else: unresolved_segment already points to item_path.last(), keep it
                            } else {
                                // This segment doesn't exist as a namespace, type, or value
                                // so namespace_path[i] is the first invalid segment
                                unresolved_segment = Some(&namespace_path[i]);
                            }
                            break;
                        }
                    }

                    // Don't report an error if the "unresolved" segment is actually a valid namespace.
                    // This handles cases like `baml.events` where `events` is a namespace (user is typing).
                    if unresolved_segment.is_some() {
                        // Check if item_path (e.g., ["log"]) is a valid namespace in this package
                        let is_valid_namespace =
                            pkg_items.namespaces.keys().any(|k| k == &item_path);
                        if is_valid_namespace {
                            unresolved_segment = None;
                        }
                    }

                    if let Some(seg) = unresolved_segment {
                        self.context.report_simple(
                            TirTypeError::UnresolvedName { name: seg.clone() },
                            expr_id,
                        );
                    }
                }
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
        }
    }

    /// Shared helper: resolve a value or type within a package's namespace.
    ///
    /// `path` contains all segments after the package name. The last segment
    /// is the item name; preceding segments are the namespace path.
    ///
    /// Used by `infer_multi_segment_path` (for `Expr::Path`).
    fn resolve_package_item(
        &mut self,
        pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
        path: &[Name],
        expr_id: ExprId,
    ) -> Option<Ty> {
        use baml_compiler2_hir::contributions::Definition;

        if path.is_empty() {
            return None;
        }

        // Try as a value (function) in a nested namespace
        let item = path.last().expect("non-empty path");
        let lookup_val = pkg_items.lookup_value(&path[..path.len() - 1], item);
        if let Some(Definition::Function(func_loc)) = lookup_val {
            let db = self.context.db();
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, func_loc.file(db));
            let ns_context = pkg_info.namespace_path;
            self.resolutions.insert(
                expr_id,
                crate::inference::MemberResolution::Free { func_loc },
            );
            let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
            let function_generic_params: Vec<Name> = sig
                .user_generic_params
                .iter()
                .chain(sig.synthetic_effect_params.iter())
                .cloned()
                .collect();
            let mut diags = Vec::new();
            let ty = Ty::Function {
                params: sig
                    .params
                    .iter()
                    .map(|param| FunctionParamTy {
                        name: Some(param.name.clone()),
                        ty: crate::lower_type_expr::lower_type_expr_in_ns(
                            db,
                            &param.ty,
                            pkg_items,
                            &ns_context,
                            &function_generic_params,
                            &mut diags,
                        ),
                        mode: if param.has_default {
                            FunctionParamMode::Optional
                        } else {
                            FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: Box::new(
                    sig.return_type
                        .as_ref()
                        .map(|te| {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                te,
                                pkg_items,
                                &ns_context,
                                &function_generic_params,
                                &mut diags,
                            )
                        })
                        .unwrap_or(Ty::Unknown {
                            attr: TyAttr::default(),
                        }),
                ),
                throws: Box::new(crate::callable::callable_throws(db, func_loc).clone()),
                attr: TyAttr::default(),
            };
            return Some(ty);
        }

        // Try as a type (class/enum)
        if let Some(def) = pkg_items.lookup_type(&path[..path.len() - 1], item) {
            let db = self.context.db();
            let name = item;
            match def {
                Definition::Class(_) => {
                    let class_qtn = crate::lower_type_expr::qualify_def(db, def, name);
                    return Some(Ty::Class(class_qtn, vec![], TyAttr::default()));
                }
                Definition::Enum(_) => {
                    let enum_qtn = crate::lower_type_expr::qualify_def(db, def, name);
                    return Some(Ty::Enum(enum_qtn, TyAttr::default()));
                }
                _ => {}
            }
        }

        // Try as an enum variant: for paths like ["llm", "ClientType", "Primitive"],
        // check if a shorter prefix resolves to an enum and the remaining segment
        // is a variant name. Walk from split_at=1 up to path.len()-1.
        for split in 1..path.len() {
            let ns = &path[..split - 1];
            let type_name = &path[split - 1];
            if let Some(Definition::Enum(enum_loc)) = pkg_items.lookup_type(ns, type_name) {
                let variant_name = &path[split];
                // Remaining segments after the variant are not supported here
                if split + 1 != path.len() {
                    continue;
                }
                let db = self.context.db();
                let enum_qtn =
                    crate::lower_type_expr::qualify_def(db, Definition::Enum(enum_loc), type_name);
                self.resolutions.insert(
                    expr_id,
                    crate::inference::MemberResolution::Variant {
                        enum_loc,
                        variant_name: variant_name.clone(),
                    },
                );
                return Some(Ty::EnumVariant(
                    enum_qtn,
                    variant_name.clone(),
                    TyAttr::default(),
                ));
            }
        }

        // Try as a class method (UFCS): for paths like ["Array", "length"],
        // check if a prefix resolves to a class and the last segment is a method.
        for split in 1..path.len() {
            let ns = &path[..split - 1];
            let type_name = &path[split - 1];
            if let Some(Definition::Class(class_loc)) = pkg_items.lookup_type(ns, type_name) {
                let method_name = &path[split];
                // Only support single method name after class (no further chaining)
                if split + 1 != path.len() {
                    continue;
                }
                let db = self.context.db();
                let class_qtn = crate::lower_type_expr::qualify_def(
                    db,
                    Definition::Class(class_loc),
                    type_name,
                );
                // Look up method on this class (no type args for UFCS)
                if let Some((method_ty, class_loc, func_loc)) =
                    self.lookup_class_method(&class_qtn, &[], method_name)
                {
                    self.resolutions.insert(
                        expr_id,
                        crate::inference::MemberResolution::UnboundMethod {
                            class_loc,
                            func_loc,
                        },
                    );
                    return Some(method_ty);
                }
            }
        }

        None
    }

    /// Resolve a single name to its type.
    ///
    /// Checks local variables first, then value namespace (functions), then
    /// type namespace (classes, enums).
    fn infer_single_name(&self, name: &Name) -> Ty {
        if let Some(binding) = self.locals.get(name) {
            let ty = &binding.current_ty;
            return match ty {
                Ty::EvolvingList(inner, attr) => Ty::List(inner.clone(), attr.clone()),
                Ty::EvolvingMap(k, v, attr) => Ty::Map(k.clone(), v.clone(), attr.clone()),
                other => other.clone(),
            };
        }
        if let Some(def) = self.package_items.lookup_value(&self.ns_context, name) {
            match def {
                Definition::Function(func_loc) => {
                    // Get function signature to build the function type
                    let db = self.context.db();
                    let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
                    let function_generic_params: Vec<Name> = sig
                        .user_generic_params
                        .iter()
                        .chain(sig.synthetic_effect_params.iter())
                        .cloned()
                        .collect();
                    let sig_ns =
                        baml_compiler2_hir::file_package::file_package(db, func_loc.file(db))
                            .namespace_path;
                    let mut diags = Vec::new();

                    // Note: diags from referenced function signatures are not
                    // reported here — they'll be reported at the definition site.
                    Ty::Function {
                        params: sig
                            .params
                            .iter()
                            .map(|param| FunctionParamTy {
                                name: Some(param.name.clone()),
                                ty: crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    &param.ty,
                                    self.package_items,
                                    &sig_ns,
                                    &function_generic_params,
                                    &mut diags,
                                ),
                                mode: if param.has_default {
                                    FunctionParamMode::Optional
                                } else {
                                    FunctionParamMode::Required
                                },
                            })
                            .collect(),
                        ret: Box::new(
                            sig.return_type
                                .as_ref()
                                .map(|te| {
                                    crate::lower_type_expr::lower_type_expr_in_ns(
                                        db,
                                        te,
                                        self.package_items,
                                        &sig_ns,
                                        &function_generic_params,
                                        &mut diags,
                                    )
                                })
                                .unwrap_or(Ty::Unknown {
                                    attr: TyAttr::default(),
                                }),
                        ),
                        throws: Box::new(crate::callable::callable_throws(db, func_loc).clone()),
                        attr: TyAttr::default(),
                    }
                }
                _ => Ty::Unknown {
                    attr: TyAttr::default(),
                },
            }
        } else if let Some(def) = self.package_items.lookup_type(&self.ns_context, name) {
            let db = self.context.db();
            match def {
                Definition::Class(_) => Ty::Class(
                    crate::lower_type_expr::qualify_def(db, def, name),
                    vec![],
                    TyAttr::default(),
                ),
                Definition::Enum(_) => Ty::Enum(
                    crate::lower_type_expr::qualify_def(db, def, name),
                    TyAttr::default(),
                ),
                Definition::TypeAlias(_) => Ty::TypeAlias(
                    crate::lower_type_expr::qualify_def(db, def, name),
                    TyAttr::default(),
                ),
                _ => Ty::Unknown {
                    attr: TyAttr::default(),
                },
            }
        } else {
            Ty::Unknown {
                attr: TyAttr::default(),
            }
        }
    }

    /// Resolve a member access on a known base type.
    ///
    /// For class types, checks data fields. For enum types, validates variants.
    /// For builtin container types (`Ty::List`, `Ty::Map`) and `Ty::Primitive(String, TyAttr::default())`,
    /// bridges to the `.baml`-declared builtin classes via `resolve_builtin_method`.
    /// Emits `UnresolvedMember` diagnostics when the base type is known but
    /// the member doesn't exist.
    pub fn resolve_member(&mut self, base_ty: &Ty, member: &Name, at: ExprId, bound: bool) -> Ty {
        match base_ty {
            Ty::Class(class_name, type_args, _) => {
                // Check class fields
                let class_fields = self.lookup_class_fields(class_name, type_args);
                if let Some(field_ty) = class_fields.get(member) {
                    // Store field resolution for LSP navigation
                    if let Some(class_loc) = self.resolve_class_loc(class_name) {
                        self.resolutions.insert(
                            at,
                            crate::inference::MemberResolution::Field {
                                class_loc,
                                field_name: member.clone(),
                            },
                        );
                    }
                    return field_ty.clone();
                }

                // Check class methods via the item tree (methods are stored
                // directly on the Class entry, not in the package namespace).
                if let Some((ty, class_loc, func_loc)) =
                    self.lookup_class_method(class_name, type_args, member)
                {
                    if bound {
                        // Bound method reference: strip `self` from the type so the
                        // caller doesn't need to pass the receiver explicitly.
                        self.resolutions.insert(
                            at,
                            crate::inference::MemberResolution::BoundMethod {
                                class_loc,
                                func_loc,
                            },
                        );
                        if let Ty::Function {
                            params,
                            ret,
                            throws,
                            attr,
                        } = ty
                        {
                            let stripped_params =
                                crate::generics::skip_self_param(&params).to_vec();
                            return Ty::Function {
                                params: stripped_params,
                                ret,
                                throws,
                                attr,
                            };
                        }
                    } else {
                        // Unbound method reference: keep `self` as the first parameter.
                        self.resolutions.insert(
                            at,
                            crate::inference::MemberResolution::UnboundMethod {
                                class_loc,
                                func_loc,
                            },
                        );
                    }
                    return ty;
                }

                // Known class but member not found — error
                let class_def = self
                    .package_items
                    .lookup_type(class_name.namespace(), class_name.name());
                let related = class_def
                    .map(|def| {
                        vec![RelatedNote::new(
                            RelatedLocation::Item(def),
                            "class defined here",
                        )]
                    })
                    .unwrap_or_default();
                self.context.report_at_member(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                    related,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            Ty::Enum(enum_name, _) => {
                // `to_json` on an enum: returns the variant name as a JSON string.
                // BEP-038 specifies the enum JSON representation as its variant name string.
                // Throws `never` — enum serialization always succeeds.
                if member.as_str() == "to_json" {
                    return Ty::Function {
                        params: vec![FunctionParamTy::required(
                            Some(Name::new("self")),
                            base_ty.clone(),
                        )],
                        ret: Box::new(json_alias_ty()),
                        throws: Box::new(Ty::Never {
                            attr: TyAttr::default(),
                        }),
                        attr: TyAttr::default(),
                    };
                }

                // Validate that the variant exists
                let variants = self.lookup_enum_variants(enum_name);
                if variants.contains(member) {
                    // Store variant resolution for LSP navigation
                    if let Some(enum_loc) = self.resolve_enum_loc(enum_name) {
                        self.resolutions.insert(
                            at,
                            crate::inference::MemberResolution::Variant {
                                enum_loc,
                                variant_name: member.clone(),
                            },
                        );
                    }
                    return Ty::EnumVariant(enum_name.clone(), member.clone(), TyAttr::default());
                }

                // Known enum but variant not found — error
                let enum_def = self
                    .package_items
                    .lookup_type(enum_name.namespace(), enum_name.name());
                let related = enum_def
                    .map(|def| {
                        vec![RelatedNote::new(
                            RelatedLocation::Item(def),
                            "enum defined here",
                        )]
                    })
                    .unwrap_or_default();
                self.context.report_at_member(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                    related,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            Ty::List(element_ty, _) => {
                // Bridge: int[] → Array<int> — resolve via builtin Array class.
                self.resolve_builtin_member(&["Array"], &[element_ty.as_ref().clone()], member, at)
                    .unwrap_or_else(|| {
                        self.context.report_at_member_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    })
            }
            Ty::Map(key_ty, val_ty, _) => {
                // Bridge: map<string, int> → Map<string, int>
                self.resolve_builtin_member(
                    &["Map"],
                    &[key_ty.as_ref().clone(), val_ty.as_ref().clone()],
                    member,
                    at,
                )
                .unwrap_or_else(|| {
                    self.context.report_at_member_simple(
                        TirTypeError::UnresolvedMember {
                            base_type: base_ty.clone(),
                            member: member.clone(),
                        },
                        at,
                    );
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                })
            }
            Ty::Future(value_ty, error_ty, _) => {
                // Bridge: Future<T, E> → baml.future.Future class
                self.resolve_builtin_member(
                    &["future", "Future"],
                    &[value_ty.as_ref().clone(), error_ty.as_ref().clone()],
                    member,
                    at,
                )
                .unwrap_or_else(|| {
                    self.context.report_at_member_simple(
                        TirTypeError::UnresolvedMember {
                            base_type: base_ty.clone(),
                            member: member.clone(),
                        },
                        at,
                    );
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                })
            }
            Ty::Primitive(PrimitiveType::String, _)
            | Ty::Literal(baml_base::Literal::String(_), _, _) => {
                // Bridge: string / string-literal → String class
                self.resolve_builtin_member(&["String"], &[], member, at)
                    .unwrap_or_else(|| {
                        self.context.report_at_member_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    })
            }
            // int literal / int → Int companion class
            Ty::Primitive(PrimitiveType::Int, _)
            | Ty::Literal(baml_base::Literal::Int(_), _, _) => self
                .resolve_builtin_member(&["Int"], &[], member, at)
                .unwrap_or_else(|| {
                    self.context.report_at_member_simple(
                        TirTypeError::UnresolvedMember {
                            base_type: base_ty.clone(),
                            member: member.clone(),
                        },
                        at,
                    );
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }),
            // float literal / float → Float companion class
            Ty::Primitive(PrimitiveType::Float, _)
            | Ty::Literal(baml_base::Literal::Float(_), _, _) => self
                .resolve_builtin_member(&["Float"], &[], member, at)
                .unwrap_or_else(|| {
                    self.context.report_at_member_simple(
                        TirTypeError::UnresolvedMember {
                            base_type: base_ty.clone(),
                            member: member.clone(),
                        },
                        at,
                    );
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }),
            // bool literal / bool → Bool companion class
            Ty::Primitive(PrimitiveType::Bool, _)
            | Ty::Literal(baml_base::Literal::Bool(_), _, _) => self
                .resolve_builtin_member(&["Bool"], &[], member, at)
                .unwrap_or_else(|| {
                    self.context.report_at_member_simple(
                        TirTypeError::UnresolvedMember {
                            base_type: base_ty.clone(),
                            member: member.clone(),
                        },
                        at,
                    );
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }),
            // null / Null companion class
            Ty::Primitive(PrimitiveType::Null, _) => self
                .resolve_builtin_member(&["Null"], &[], member, at)
                .unwrap_or_else(|| {
                    self.context.report_at_member_simple(
                        TirTypeError::UnresolvedMember {
                            base_type: base_ty.clone(),
                            member: member.clone(),
                        },
                        at,
                    );
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }),
            Ty::Type { .. } => {
                // Bridge: type → TypeValue companion class (provides `.to_string()`, etc.)
                self.resolve_builtin_member(&["TypeValue"], &[], member, at)
                    .unwrap_or_else(|| {
                        self.context.report_at_member_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    })
            }
            Ty::Primitive(
                p @ (PrimitiveType::Uint8Array
                | PrimitiveType::Image
                | PrimitiveType::Audio
                | PrimitiveType::Video
                | PrimitiveType::Pdf),
                _,
            ) => {
                // Bridge: media / binary primitives with builtin companion classes
                self.resolve_builtin_member(p.builtin_class_path(), &[], member, at)
                    .unwrap_or_else(|| {
                        self.context.report_at_member_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    })
            }
            // Universal `to_json` / `from_json` on generic type variables.
            //
            // After Phase 5b.1, every BAML type has `to_json(self) -> json` and
            // `from_json(j: json) -> Self`. When the base type is a type-variable
            // `T` (e.g. inside `class Array<T>`), the compiler cannot look up the
            // concrete companion class; instead we synthesize the expected method
            // signature directly. The throws clause is conservatively widened to
            // `JsonSerializationError | JsonParseError` — the actual throws for any
            // concrete T is a subset, so call-site throw inference stays sound.
            Ty::TypeVar(_, _) if member.as_str() == "to_json" => {
                // Type-check: every BAML type has `to_json(self) -> json` after Phase 5b.1.
                // No MemberResolution stored — the concrete dispatch is deferred to Phase 5b.4
                // (native Array/Map impls) and never runs with an unresolved TypeVar at runtime.
                Ty::Function {
                    params: vec![],
                    ret: Box::new(json_alias_ty()),
                    throws: Box::new(json_serialization_or_parse_error_ty()),
                    attr: TyAttr::default(),
                }
            }
            Ty::TypeVar(name, _) if member.as_str() == "from_json" => {
                // Type-check: every BAML type has `from_json(j: json) -> Self` after Phase 5b.1.
                Ty::Function {
                    params: vec![FunctionParamTy::required(
                        Some(Name::new("j")),
                        json_alias_ty(),
                    )],
                    ret: Box::new(Ty::TypeVar(name.clone(), TyAttr::default())),
                    throws: Box::new(json_parse_or_serialization_error_ty()),
                    attr: TyAttr::default(),
                }
            }
            Ty::Union(members, _) => {
                // For union types, try to resolve the field on each member.
                // If ALL members have the field, return Union(resolved_types).
                // If any member is missing the field, report per-member errors.
                let members = members.clone();
                let resolved: Vec<(Ty, Option<Ty>)> = members
                    .iter()
                    .map(|m| (m.clone(), self.try_resolve_member_on_ty(m, member)))
                    .collect();

                if resolved.iter().all(|(_, r)| r.is_some()) {
                    // All members have the field — return union of resolved types
                    let field_tys: Vec<Ty> =
                        resolved.into_iter().map(|(_, r)| r.unwrap()).collect();
                    // TODO(TyAttr): This union is synthesized from per-member field resolutions,
                    // not a transform of the original union. The original union's attr describes
                    // the union-as-a-whole (e.g. @stream.done), but the result describes field
                    // types which may have different streaming semantics. TyAttr::default() is
                    // probably correct but needs confirmation.
                    Ty::Union(field_tys, TyAttr::default())
                } else {
                    // Report an error for each member that's missing the field
                    for (member_ty, result) in &resolved {
                        if result.is_none() {
                            self.context.report_at_member_simple(
                                TirTypeError::UnresolvedMember {
                                    base_type: member_ty.clone(),
                                    member: member.clone(),
                                },
                                at,
                            );
                        }
                    }
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
            }
            Ty::TypeAlias(_, _) => {
                // Expand the alias chain to its concrete type, then recurse on
                // the result.  `expand_alias_chains` already caps iterations at
                // 64, so cyclic aliases (`type One = Two; type Two = One`)
                // stop expanding without overflowing the call stack.  If the
                // chain bottoms out on another `TypeAlias` (cycle never
                // resolves), fall through to `Unknown`.
                let expanded = self.expand_alias_chains(base_ty.clone());
                if matches!(expanded, Ty::TypeAlias(_, _)) {
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                } else {
                    self.resolve_member(&expanded, member, at, bound)
                }
            }
            Ty::Unknown { .. } => {
                // Base type unknown — can't resolve member, but don't emit error
                // (the base type error was already reported upstream)
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            _ => {
                // Other types (other primitives, etc.) — no members
                self.context.report_at_member_simple(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
        }
    }

    /// Resolve a member access for a specific segment of a multi-segment `Path` expression.
    ///
    /// For simple base types (Class, Enum, Unknown), uses `report_at_segment` so diagnostics
    /// point at the correct segment token. For complex base types (Union, List, Map, String,
    /// primitives), falls through to `resolve_member` which handles them correctly.
    ///
    /// Stores resolutions on `path_id` (the whole path `ExprId`).
    fn resolve_member_for_path_segment(
        &mut self,
        base_ty: &Ty,
        member: &Name,
        path_id: ExprId,
        seg_idx: usize,
        bound: bool,
    ) -> Ty {
        match base_ty {
            Ty::Class(class_name, _, _) => {
                // Use the no-side-effect helper to check membership WITHOUT
                // calling lookup_class_fields (which emits field-type diagnostics).
                if self.class_has_member(class_name, member) {
                    // Field/method found — delegate to resolve_member which stores
                    // the resolution and handles field-type diagnostics once.
                    return self.resolve_member(base_ty, member, path_id, bound);
                }
                // Not found — report at segment for the correct span.
                let class_def = self
                    .package_items
                    .lookup_type(class_name.namespace(), class_name.name());
                let related = class_def
                    .map(|def| {
                        vec![RelatedNote::new(
                            RelatedLocation::Item(def),
                            "class defined here",
                        )]
                    })
                    .unwrap_or_default();
                self.context.report_at_segment(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    path_id,
                    seg_idx,
                    related,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            Ty::Enum(enum_name, _) => {
                // `to_json` on an enum (path-segment form): same as the
                // `resolve_member_on_ty` arm above — variant name as JSON string.
                if member.as_str() == "to_json" {
                    return Ty::Function {
                        params: vec![FunctionParamTy::required(
                            Some(Name::new("self")),
                            base_ty.clone(),
                        )],
                        ret: Box::new(json_alias_ty()),
                        throws: Box::new(Ty::Never {
                            attr: TyAttr::default(),
                        }),
                        attr: TyAttr::default(),
                    };
                }

                // Use the no-side-effect helper.
                if self.enum_has_variant(enum_name, member) {
                    return self.resolve_member(base_ty, member, path_id, bound);
                }
                // Not found — report at segment.
                let enum_def = self
                    .package_items
                    .lookup_type(enum_name.namespace(), enum_name.name());
                let related = enum_def
                    .map(|def| {
                        vec![RelatedNote::new(
                            RelatedLocation::Item(def),
                            "enum defined here",
                        )]
                    })
                    .unwrap_or_default();
                self.context.report_at_segment(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    path_id,
                    seg_idx,
                    related,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            Ty::Union(members, _) => {
                // For union types, try to resolve the member on each constituent.
                // Use report_at_segment for missing members so the span points at the
                // segment token, not the full path.
                let members = members.clone();
                let resolved: Vec<(Ty, Option<Ty>)> = members
                    .iter()
                    .map(|m| (m.clone(), self.try_resolve_member_on_ty(m, member)))
                    .collect();

                if resolved.iter().all(|(_, r)| r.is_some()) {
                    let field_tys: Vec<Ty> =
                        resolved.into_iter().map(|(_, r)| r.unwrap()).collect();
                    Ty::Union(field_tys, TyAttr::default())
                } else {
                    for (member_ty, result) in &resolved {
                        if result.is_none() {
                            self.context.report_at_segment(
                                TirTypeError::UnresolvedMember {
                                    base_type: member_ty.clone(),
                                    member: member.clone(),
                                },
                                path_id,
                                seg_idx,
                                Vec::new(),
                            );
                        }
                    }
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
            }
            Ty::Unknown { .. } => {
                // Base type unknown — don't emit another error.
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            _ => {
                // For List, Map, String, primitives, etc. — fall through
                // to resolve_member which handles them with proper error reporting.
                self.resolve_member(base_ty, member, path_id, bound)
            }
        }
    }

    /// Try to resolve a member on a type without emitting diagnostics.
    ///
    /// Returns `Some(Ty)` if the member exists, `None` if it doesn't.
    /// Used by `resolve_member` for union type handling.
    fn try_resolve_member_on_ty(&self, ty: &Ty, member: &Name) -> Option<Ty> {
        match ty {
            Ty::Class(class_name, type_args, _) => {
                let fields = self.lookup_class_fields(class_name, type_args);
                if let Some(field_ty) = fields.get(member) {
                    return Some(field_ty.clone());
                }
                if let Some((method_ty, _, _)) =
                    self.lookup_class_method(class_name, type_args, member)
                {
                    return Some(method_ty);
                }
                None
            }
            Ty::List(element_ty, _) => self
                .resolve_builtin_method(&["Array"], &[element_ty.as_ref().clone()], member)
                .map(BuiltinResolution::into_ty),
            Ty::Map(key_ty, val_ty, _) => self
                .resolve_builtin_method(
                    &["Map"],
                    &[key_ty.as_ref().clone(), val_ty.as_ref().clone()],
                    member,
                )
                .map(BuiltinResolution::into_ty),
            Ty::Future(value_ty, error_ty, _) => self
                .resolve_builtin_method(
                    &["future", "Future"],
                    &[value_ty.as_ref().clone(), error_ty.as_ref().clone()],
                    member,
                )
                .map(BuiltinResolution::into_ty),
            Ty::Primitive(PrimitiveType::String, _)
            | Ty::Literal(baml_base::Literal::String(_), _, _) => self
                .resolve_builtin_method(&["String"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Primitive(PrimitiveType::Int, _)
            | Ty::Literal(baml_base::Literal::Int(_), _, _) => self
                .resolve_builtin_method(&["Int"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Primitive(PrimitiveType::Float, _)
            | Ty::Literal(baml_base::Literal::Float(_), _, _) => self
                .resolve_builtin_method(&["Float"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Primitive(PrimitiveType::Bool, _)
            | Ty::Literal(baml_base::Literal::Bool(_), _, _) => self
                .resolve_builtin_method(&["Bool"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Primitive(PrimitiveType::Null, _) => self
                .resolve_builtin_method(&["Null"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Primitive(
                p @ (PrimitiveType::Uint8Array
                | PrimitiveType::Image
                | PrimitiveType::Audio
                | PrimitiveType::Video
                | PrimitiveType::Pdf),
                _,
            ) => self
                .resolve_builtin_method(p.builtin_class_path(), &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Type { .. } => self
                .resolve_builtin_method(&["TypeValue"], &[], member)
                .map(BuiltinResolution::into_ty),
            // Universal `to_json` / `from_json` on generic type variables.
            // Mirrors the arm in `resolve_member` — no side effects needed here.
            Ty::TypeVar(_, _) if member.as_str() == "to_json" => Some(Ty::Function {
                params: vec![],
                ret: Box::new(json_alias_ty()),
                throws: Box::new(json_serialization_or_parse_error_ty()),
                attr: TyAttr::default(),
            }),
            Ty::TypeVar(name, _) if member.as_str() == "from_json" => Some(Ty::Function {
                params: vec![FunctionParamTy::required(
                    Some(Name::new("j")),
                    json_alias_ty(),
                )],
                ret: Box::new(Ty::TypeVar(name.clone(), TyAttr::default())),
                throws: Box::new(json_parse_or_serialization_error_ty()),
                attr: TyAttr::default(),
            }),
            Ty::Optional(inner, _) => {
                // Drill through Optional to resolve the member on the inner type
                self.try_resolve_member_on_ty(inner, member)
            }
            Ty::TypeAlias(qtn, _) => {
                if let Some(expanded) = self.aliases.get(qtn) {
                    let expanded = expanded.clone();
                    self.try_resolve_member_on_ty(&expanded, member)
                } else {
                    None
                }
            }
            Ty::Unknown { .. } => {
                // Unknown propagates — treat as if the field exists with Unknown type
                Some(Ty::Unknown {
                    attr: TyAttr::default(),
                })
            }
            _ => None,
        }
    }

    /// Look up class fields from the package items (via item tree).
    ///
    /// `class_type_args` are the concrete type arguments for the class (e.g.
    /// `[Sentiment$stream, Sentiment]` for `Stream<Sentiment$stream, Sentiment>`).
    /// When non-empty, field types are resolved with `lower_type_expr_with_generics`
    /// so that type variables like `TStream` and `TFinal` are substituted with concrete types.
    ///
    /// Returns a map of field name → resolved field type.
    fn lookup_class_fields(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
    ) -> FxHashMap<Name, Ty> {
        let mut result = FxHashMap::default();
        let Some(pkg_items_for_class) = self.resolve_class_pkg_items(class_name.package()) else {
            return result;
        };
        if let Some(Definition::Class(class_loc)) =
            pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())
        {
            let file = class_loc.file(self.context.db());
            let ns_context =
                baml_compiler2_hir::file_package::file_package(self.context.db(), file)
                    .namespace_path;
            let item_tree = baml_compiler2_ppir::file_item_tree(self.context.db(), file);
            let class_data = &item_tree[class_loc.id(self.context.db())];

            // Build bindings from declared generic params → concrete type args.
            let bindings =
                crate::generics::bind_type_vars(&class_data.generic_params, class_type_args);

            for field in &class_data.fields {
                let mut diags = Vec::new();
                let field_ty = field
                    .type_expr
                    .as_ref()
                    .map(|te| {
                        let ty = if bindings.is_empty() {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                self.context.db(),
                                &te.expr,
                                pkg_items_for_class,
                                &ns_context,
                                &class_data.generic_params,
                                &mut diags,
                            )
                        } else {
                            crate::generics::lower_type_expr_with_generics(
                                self.context.db(),
                                &te.expr,
                                pkg_items_for_class,
                                &ns_context,
                                &bindings,
                                &mut diags,
                            )
                        };
                        for diag in diags.drain(..) {
                            self.context.report_at_span(diag, te.span);
                        }
                        ty
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                result.insert(field.name.clone(), field_ty);
            }
        }
        result
    }

    /// Class field `(name, type)` pairs in **declaration order** with
    /// generic substitution applied. Single item-tree walk shared by every
    /// pattern-lowering caller that needs ordered field info — replaces a
    /// previous pair of helpers (`class_field_types_ordered` +
    /// `class_field_names_ordered`) that walked the tree twice.
    fn class_field_infos_ordered(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
    ) -> Vec<(Name, Ty)> {
        let by_name = self.lookup_class_fields(class_name, class_type_args);
        let Some(items) = self.resolve_class_pkg_items(class_name.package()) else {
            return Vec::new();
        };
        let Some(Definition::Class(class_loc)) =
            items.lookup_type(class_name.namespace(), class_name.name())
        else {
            return Vec::new();
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        class_data
            .fields
            .iter()
            .map(|f| {
                let ty = by_name.get(&f.name).cloned().unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                });
                (f.name.clone(), ty)
            })
            .collect()
    }

    /// Class field types in declaration order. Thin projection over
    /// [`Self::class_field_infos_ordered`].
    fn class_field_types_ordered(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
    ) -> Vec<Ty> {
        self.class_field_infos_ordered(class_name, class_type_args)
            .into_iter()
            .map(|(_, ty)| ty)
            .collect()
    }

    /// Check whether a class has a field or method with the given name.
    ///
    /// Unlike `lookup_class_fields`, this does NOT call `lower_type_expr_in_ns`
    /// so it has no diagnostic side-effects.  Used by `resolve_member_for_path_segment`
    /// to decide which error location to use without double-emitting field-type
    /// diagnostics.
    fn class_has_member(&self, class_name: &crate::ty::QualifiedTypeName, member: &Name) -> bool {
        let Some(pkg_items) = self.resolve_class_pkg_items(class_name.package()) else {
            return false;
        };
        let Some(Definition::Class(class_loc)) =
            pkg_items.lookup_type(class_name.namespace(), class_name.name())
        else {
            return false;
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        // Check fields.
        if class_data.fields.iter().any(|f| &f.name == member) {
            return true;
        }
        // Check methods.
        self.lookup_class_method(class_name, &[], member).is_some()
    }

    /// Check whether an enum has a variant with the given name.
    ///
    /// No diagnostic side-effects.
    fn enum_has_variant(&self, enum_name: &crate::ty::QualifiedTypeName, member: &Name) -> bool {
        self.lookup_enum_variants(enum_name).contains(member)
    }

    /// Fetch `PackageItems` for the package that owns a class type.
    ///
    /// Returns `self.package_items` when the class is in the current package,
    /// or loads the correct foreign package's items when the class lives in a
    /// declared dependency package.
    fn resolve_class_pkg_items(
        &self,
        class_pkg: &baml_base::Name,
    ) -> Option<&'db baml_compiler2_hir::package::PackageItems<'db>> {
        let db = self.context.db();
        self.res_ctx.items_for_package(db, class_pkg)
    }

    /// Resolve a `QualifiedTypeName` to a `ClassLoc` via `package_items` lookup.
    fn resolve_class_loc(
        &self,
        qtn: &crate::ty::QualifiedTypeName,
    ) -> Option<baml_compiler2_hir::loc::ClassLoc<'db>> {
        let pkg_items = self.resolve_class_pkg_items(qtn.package())?;
        match pkg_items.lookup_type(qtn.namespace(), qtn.name())? {
            Definition::Class(class_loc) => Some(class_loc),
            _ => None,
        }
    }

    /// Resolve a `QualifiedTypeName` to an `EnumLoc` via `package_items` lookup.
    fn resolve_enum_loc(
        &self,
        qtn: &crate::ty::QualifiedTypeName,
    ) -> Option<baml_compiler2_hir::loc::EnumLoc<'db>> {
        let db = self.context.db();
        let items = if *qtn.package() == self.package_id.name(db) {
            self.package_items
        } else {
            self.res_ctx.items_for_package(db, qtn.package())?
        };
        match items.lookup_type(qtn.namespace(), qtn.name())? {
            Definition::Enum(enum_loc) => Some(enum_loc),
            _ => None,
        }
    }

    /// Look up a class method by name from the item tree.
    ///
    /// Methods are stored on the `Class` entry directly (not in the package
    /// namespace), so we resolve the class, iterate its method IDs, and match
    /// by name. Returns the method type along with the class and function locs
    /// so callers can record a `MemberResolution`.
    ///
    /// `class_type_args` are the concrete type arguments for the class (e.g.
    /// `[Sentiment$stream, Sentiment]` for `Stream<Sentiment$stream, Sentiment>`).
    /// When non-empty, return types are resolved with `lower_type_expr_with_generics`
    /// so that type variables like `TStream` and `TFinal` are substituted with concrete types.
    fn lookup_class_method(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
        method_name: &Name,
    ) -> Option<(
        Ty,
        baml_compiler2_hir::loc::ClassLoc<'db>,
        baml_compiler2_hir::loc::FunctionLoc<'db>,
    )> {
        let pkg_items_for_class = self.resolve_class_pkg_items(class_name.package())?;
        let def = pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())?;
        let Definition::Class(class_loc) = def else {
            return None;
        };
        let db = self.context.db();
        let file = class_loc.file(db);
        let ns_context = baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];

        for &method_id in &class_data.methods {
            let method_data = &item_tree[method_id];
            if method_data.name == *method_name {
                // Build bindings from class-level generic params → concrete args.
                let mut bindings =
                    crate::generics::bind_type_vars(&class_data.generic_params, class_type_args);
                // Seed class-level generics as TypeVar entries when no concrete args
                // were provided (e.g., UFCS calls like `Array.length(arr)`).
                for gp in &class_data.generic_params {
                    bindings
                        .entry(gp.clone())
                        .or_insert_with(|| Ty::TypeVar(gp.clone(), TyAttr::default()));
                }
                // Seed method-level generics as TypeVar entries so they survive
                // lowering and can be resolved by call-site inference.
                for gp in &method_data.generic_params {
                    bindings
                        .entry(gp.clone())
                        .or_insert_with(|| Ty::TypeVar(gp.clone(), TyAttr::default()));
                }

                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
                let mut diags = Vec::new();

                // Build the self type WITH concrete type args, or TypeVars for
                // unbound generics (UFCS case).
                let class_ty_args: Vec<Ty> = if class_type_args.is_empty() {
                    class_data
                        .generic_params
                        .iter()
                        .map(|gp| Ty::TypeVar(gp.clone(), TyAttr::default()))
                        .collect()
                } else {
                    class_type_args.to_vec()
                };
                let class_ty = Ty::Class(class_name.clone(), class_ty_args, TyAttr::default());

                // All generic params for fallback lowering (class + method).
                let mut all_generic_params = class_data.generic_params.clone();
                all_generic_params.extend(sig.user_generic_params.iter().cloned());
                all_generic_params.extend(sig.synthetic_effect_params.iter().cloned());

                let callable_throws = crate::callable::callable_throws(db, func_loc).clone();

                let ty = Ty::Function {
                    params: sig
                        .params
                        .iter()
                        .map(|param| {
                            let param_ty = if param.name.as_str() == "self"
                                && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
                            {
                                // self with no annotation → use the enclosing class type
                                class_ty.clone()
                            } else if bindings.is_empty() {
                                crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    &param.ty,
                                    pkg_items_for_class,
                                    &ns_context,
                                    &all_generic_params,
                                    &mut diags,
                                )
                            } else {
                                crate::generics::lower_type_expr_with_generics(
                                    db,
                                    &param.ty,
                                    pkg_items_for_class,
                                    &ns_context,
                                    &bindings,
                                    &mut diags,
                                )
                            };
                            FunctionParamTy {
                                name: Some(param.name.clone()),
                                ty: param_ty,
                                mode: if param.has_default {
                                    FunctionParamMode::Optional
                                } else {
                                    FunctionParamMode::Required
                                },
                            }
                        })
                        .collect(),
                    ret: Box::new(
                        sig.return_type
                            .as_ref()
                            .map(|te| {
                                if bindings.is_empty() {
                                    crate::lower_type_expr::lower_type_expr_in_ns(
                                        db,
                                        te,
                                        pkg_items_for_class,
                                        &ns_context,
                                        &all_generic_params,
                                        &mut diags,
                                    )
                                } else {
                                    crate::generics::lower_type_expr_with_generics(
                                        db,
                                        te,
                                        pkg_items_for_class,
                                        &ns_context,
                                        &bindings,
                                        &mut diags,
                                    )
                                }
                            })
                            .unwrap_or(Ty::Unknown {
                                attr: TyAttr::default(),
                            }),
                    ),
                    throws: Box::new(if bindings.is_empty() {
                        callable_throws
                    } else {
                        crate::generics::substitute_ty(&callable_throws, &bindings)
                    }),
                    attr: TyAttr::default(),
                };
                // Note: diags from method signatures are reported at definition site.
                return Some((ty, class_loc, func_loc));
            }
        }
        None
    }

    /// Check if a `FieldAccess` base is a primitive type name used for static
    /// method access (e.g. `image.from_url(...)`, `pdf.from_base64(...)`).
    ///
    /// Returns `Some(method_ty)` if the base is a recognized primitive type name
    /// and the field is a valid static method on the corresponding builtin class.
    /// Returns `None` to fall through to normal `FieldAccess` resolution.
    /// Try to resolve a `FieldAccess` chain rooted at `Path(["baml"])` as a
    /// builtin package access: `baml.Array.length`, `baml.media.Image.from_url`.
    ///
    /// Walks the `FieldAccess` chain to extract the class path and member name,
    /// then delegates to `resolve_builtin_member`.
    /// Try to resolve a `FieldAccess` chain as a package access path.
    ///
    fn try_primitive_static_access(
        &mut self,
        at: ExprId,
        base_id: ExprId,
        field: &Name,
        body: &ExprBody,
    ) -> Option<Ty> {
        let base_expr = &body.exprs[base_id];
        let Expr::Path(segments) = base_expr else {
            return None;
        };
        if segments.len() != 1 {
            return None;
        }
        let name = segments[0].as_str();

        // Map lowercase primitive type names to their builtin class paths.
        let class_path: &[&str] = match name {
            "image" => &["media", "Image"],
            "audio" => &["media", "Audio"],
            "video" => &["media", "Video"],
            "pdf" => &["media", "Pdf"],
            "string" => &["String"],
            "int" => &["Int"],
            "float" => &["Float"],
            _ => return None,
        };

        self.resolve_builtin_member(class_path, &[], field, at)
    }

    /// Resolve a method or field on a builtin class declared in the `"baml"` package.
    ///
    /// 1. Fetches `package_items(db, "baml")`.
    /// 2. Looks up `class_name` in the root namespace.
    /// 3. Binds the class's `generic_params` to `type_args` (e.g. `{T → int}`).
    /// 4. Searches the class methods for `member_name`, lowering the method's
    ///    parameter and return types with type variable substitution applied.
    /// 5. Falls back to checking class fields.
    ///
    /// Returns `None` if the class or member is not found.
    /// Wrapper around `resolve_builtin_method` that also stores a `MemberResolution`
    /// when the result is a method (not a field).
    fn resolve_builtin_member(
        &mut self,
        class_path: &[&str],
        type_args: &[Ty],
        member_name: &Name,
        at: ExprId,
    ) -> Option<Ty> {
        let result = self.resolve_builtin_method(class_path, type_args, member_name)?;
        match result {
            BuiltinResolution::Method {
                ty,
                class_loc,
                func_loc,
            } => {
                // Builtin methods are always accessed on value bases → BoundMethod.
                self.resolutions.insert(
                    at,
                    crate::inference::MemberResolution::BoundMethod {
                        class_loc,
                        func_loc,
                    },
                );
                Some(ty)
            }
            BuiltinResolution::Field(ty) => Some(ty),
        }
    }

    fn resolve_builtin_method(
        &self,
        class_path: &[&str],
        type_args: &[Ty],
        member_name: &Name,
    ) -> Option<BuiltinResolution<'db>> {
        let db = self.context.db();
        let baml_items = self
            .res_ctx
            .items_for_package(db, &baml_base::Name::new("baml"))?;

        // Look up the class by path (e.g. &["Array"] or &["media", "Image"]).
        let path: Vec<Name> = class_path.iter().map(baml_base::Name::new).collect();
        let item = path.last().expect("non-empty class_path");
        let def = baml_items.lookup_type(&path[..path.len() - 1], item)?;
        let baml_compiler2_hir::contributions::Definition::Class(class_loc) = def else {
            return None;
        };

        let file = class_loc.file(db);
        let stub_pkg = baml_compiler2_hir::file_package::file_package(db, file);
        let stub_ns: &[Name] = &stub_pkg.namespace_path;
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];

        // Bind generic type variables: e.g. {T → int} for Array<int>.
        let mut bindings = crate::generics::bind_type_vars(&class_data.generic_params, type_args);

        // Search methods first.
        for &method_id in &class_data.methods {
            let method_data = &item_tree[method_id];
            if method_data.name == *member_name {
                // Add method-level generics as TypeVar entries so they survive
                // lowering and can be resolved by call-site inference.
                for gp in &method_data.generic_params {
                    bindings
                        .entry(gp.clone())
                        .or_insert_with(|| Ty::TypeVar(gp.clone(), TyAttr::default()));
                }
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
                let mut diags = Vec::new();
                // Build the class type for self parameter resolution.
                // For generics, apply type_args (e.g. Array<int>).
                let builtin_class_ty = if type_args.is_empty() {
                    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                    Ty::Class(
                        crate::ty::QualifiedTypeName::new_with_generic_params(
                            pkg_info.package,
                            pkg_info.namespace_path,
                            class_data.name.clone(),
                            class_data.generic_params.clone(),
                        ),
                        vec![],
                        TyAttr::default(),
                    )
                } else if type_args.len() == 1 {
                    // Single type arg: Array<T> → List(T), special-case common containers
                    match class_data.name.as_str() {
                        "Array" => Ty::List(Box::new(type_args[0].clone()), TyAttr::default()),
                        _ => {
                            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                            Ty::Class(
                                crate::ty::QualifiedTypeName::new(
                                    pkg_info.package,
                                    pkg_info.namespace_path,
                                    class_data.name.clone(),
                                ),
                                type_args.to_vec(),
                                TyAttr::default(),
                            )
                        }
                    }
                } else if type_args.len() == 2 {
                    match class_data.name.as_str() {
                        "Map" => Ty::Map(
                            Box::new(type_args[0].clone()),
                            Box::new(type_args[1].clone()),
                            TyAttr::default(),
                        ),
                        _ => {
                            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                            Ty::Class(
                                crate::ty::QualifiedTypeName::new(
                                    pkg_info.package,
                                    pkg_info.namespace_path,
                                    class_data.name.clone(),
                                ),
                                type_args.to_vec(),
                                TyAttr::default(),
                            )
                        }
                    }
                } else {
                    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                    Ty::Class(
                        crate::ty::QualifiedTypeName::new(
                            pkg_info.package,
                            pkg_info.namespace_path,
                            class_data.name.clone(),
                        ),
                        type_args.to_vec(),
                        TyAttr::default(),
                    )
                };

                let params: Vec<FunctionParamTy> = sig
                    .params
                    .iter()
                    .map(|param| {
                        let ty = if param.name.as_str() == "self"
                            && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
                        {
                            builtin_class_ty.clone()
                        } else {
                            crate::generics::lower_type_expr_with_generics(
                                db,
                                &param.ty,
                                self.package_items,
                                stub_ns,
                                &bindings,
                                &mut diags,
                            )
                        };
                        FunctionParamTy {
                            name: Some(param.name.clone()),
                            ty,
                            mode: if param.has_default {
                                FunctionParamMode::Optional
                            } else {
                                FunctionParamMode::Required
                            },
                        }
                    })
                    .collect();
                let ret = sig
                    .return_type
                    .as_ref()
                    .map(|te| {
                        crate::generics::lower_type_expr_with_generics(
                            db,
                            te,
                            self.package_items,
                            stub_ns,
                            &bindings,
                            &mut diags,
                        )
                    })
                    .unwrap_or(Ty::Void {
                        attr: TyAttr::default(),
                    });
                let callable_throws = crate::callable::callable_throws(db, func_loc).clone();
                let throws = if bindings.is_empty() {
                    callable_throws
                } else {
                    crate::generics::substitute_ty(&callable_throws, &bindings)
                };
                // Discard diags — they will be reported at the definition site
                // (the builtin .baml stub). We don't want to spam user code
                // with unresolved-type errors from builtin signatures.
                drop(diags);
                return Some(BuiltinResolution::Method {
                    ty: Ty::Function {
                        params,
                        ret: Box::new(ret),
                        throws: Box::new(throws),
                        attr: TyAttr::default(),
                    },
                    class_loc,
                    func_loc,
                });
            }
        }

        // Fall back to fields (e.g. Request.method, Request.url).
        for field in &class_data.fields {
            if field.name == *member_name {
                let mut diags = Vec::new();
                let field_ty = field
                    .type_expr
                    .as_ref()
                    .map(|te| {
                        crate::generics::lower_type_expr_with_generics(
                            db,
                            &te.expr,
                            self.package_items,
                            stub_ns,
                            &bindings,
                            &mut diags,
                        )
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                drop(diags);
                return Some(BuiltinResolution::Field(field_ty));
            }
        }

        None
    }

    /// Look up enum variants from the package items (via item tree).
    ///
    /// Uses the enum's qualified package to find it in the correct package,
    /// not just the current file's package.
    fn lookup_enum_variants(&self, enum_name: &crate::ty::QualifiedTypeName) -> Vec<Name> {
        let db = self.context.db();

        // Resolve the package that owns the enum via res_ctx.
        let items = if *enum_name.package() == self.package_id.name(db) {
            self.package_items
        } else {
            match self.res_ctx.items_for_package(db, enum_name.package()) {
                Some(items) => items,
                None => return Vec::new(),
            }
        };

        if let Some(Definition::Enum(enum_loc)) =
            items.lookup_type(enum_name.namespace(), enum_name.name())
        {
            let file = enum_loc.file(db);
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            let enum_data = &item_tree[enum_loc.id(db)];
            return enum_data.variants.iter().map(|v| v.name.clone()).collect();
        }
        Vec::new()
    }

    // ── Evolving Container Mutations ─────────────────────────────────────────

    /// Extract the local variable name from an expression, if it's a simple
    /// single-segment path that refers to a known local.
    /// Check if a callee expression looks like a method call (`MemberAccess` or
    /// 2-segment `Path` like `["x", "push"]`).
    fn is_method_like_callee(callee: &Expr) -> bool {
        match callee {
            Expr::MemberAccess { .. } => true,
            Expr::Path(segs) if segs.len() == 2 => true,
            _ => false,
        }
    }

    fn expr_local_name(&self, expr_id: ExprId, body: &ExprBody) -> Option<Name> {
        match &body.exprs[expr_id] {
            Expr::Path(segments) if segments.len() == 1 => {
                let name = &segments[0];
                if self.locals.contains_key(name) {
                    Some(name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Try to handle a container mutation method call: `x.push(val)` or `x?.push?.(val)`.
    ///
    /// This is the "evolving path" for container mutations — it intercepts
    /// `.push()` / `.append()` and index assignment *before* normal method
    /// resolution via `resolve_builtin_method`. See the doc comment on
    /// `Ty::EvolvingList` for why two paths exist.
    ///
    /// If the callee is `base.push(arg)` or `base.append(arg)` where base is a
    /// local with type `List(T)` or `EvolvingList(T)`:
    /// - If `T == Never`: first establishment → update local to `[Evolving]List(arg_ty)`
    /// - If `arg_ty <: T`: ok
    /// - Otherwise: type error
    ///
    /// Returns the inner method result type (e.g. `int` for `Array.push`) if
    /// handled. Callers own the enclosing expression semantics such as optional
    /// wrapping, final subtype checks, and recording the call expression type.
    fn try_container_method_call(
        &mut self,
        callee_id: ExprId,
        args: &[ExprId],
        body: &ExprBody,
    ) -> Option<Ty> {
        // After AST lowering, mutating container calls arrive through either a
        // MemberAccess (`x.push(...)`), an OptionalMemberAccess (`x?.push(...)`),
        // or a 2-segment Path (`["x", "push"]`) when multi-segment paths are preserved.
        let (base_id, local_name, method_name) = match &body.exprs[callee_id] {
            Expr::MemberAccess { base, member } => {
                let name = self.expr_local_name(*base, body)?;
                (*base, name, member.clone())
            }
            Expr::OptionalMemberAccess { base, member } => {
                let name = self.expr_local_name(*base, body)?;
                (*base, name, member.clone())
            }
            Expr::Path(segments) if segments.len() == 2 => {
                let receiver = &segments[0];
                if !self.locals.contains_key(receiver) {
                    return None;
                }
                (callee_id, receiver.clone(), segments[1].clone())
            }
            _ => return None,
        };

        let local_ty = self.locals.get(&local_name)?.current_ty.clone();

        match method_name.as_str() {
            "push" | "append" if args.len() == 1 => {
                let (elem_ty, is_evolving, container_attr) = match &local_ty {
                    Ty::EvolvingList(elem, attr) => (elem, true, attr.clone()),
                    Ty::List(elem, attr) => (elem, false, attr.clone()),
                    _ => return None,
                };

                let arg_ty = self.infer_expr(args[0], body);
                let widened_arg = arg_ty.widen_fresh();

                let effective_local_ty = if matches!(**elem_ty, Ty::Never { .. }) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingList(Box::new(widened_arg), container_attr)
                    } else {
                        Ty::List(Box::new(widened_arg), container_attr)
                    };
                    self.assign_local(local_name.clone(), new_ty.clone());
                    self.sync_let_binding_type(&local_name, new_ty.clone());
                    new_ty
                } else if !self.is_subtype(&widened_arg, elem_ty) {
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: *elem_ty.clone(),
                            got: widened_arg,
                        },
                        args[0],
                        Vec::new(),
                    );
                    local_ty.clone()
                } else {
                    local_ty.clone()
                };

                // Record a MemberResolution so MIR emits a proper method call
                // instead of a dynamic map lookup.
                let effective_elem = match &effective_local_ty {
                    Ty::EvolvingList(e, _) | Ty::List(e, _) => e.as_ref().clone(),
                    _ => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                };
                let method_ty = self
                    .resolve_builtin_member(&["Array"], &[effective_elem], &method_name, callee_id)
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                let result = match &method_ty {
                    Ty::Function { ret, .. } => ret.as_ref().clone(),
                    _ => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                };
                let callee_expr_ty = match &body.exprs[callee_id] {
                    Expr::OptionalMemberAccess { .. } => Self::make_optional(method_ty),
                    _ => method_ty,
                };

                self.record_expr_type(base_id, effective_local_ty);
                self.record_expr_type(callee_id, callee_expr_ty);
                Some(result)
            }
            _ => None,
        }
    }

    /// Try to handle index assignment mutation: x[i] = val on List or Map locals.
    ///
    /// For `List(Never)`: first element establishes element type.
    /// For `Map(Never, Never)`: first entry establishes key and value types.
    ///
    /// Returns `true` if handled, `false` to fall through to general case.
    fn try_index_assign_mutation(
        &mut self,
        target_id: ExprId,
        value_id: ExprId,
        body: &ExprBody,
    ) -> bool {
        let (base_id, index_id) = match &body.exprs[target_id] {
            Expr::Index { base, index } => (*base, *index),
            _ => return false,
        };

        let Some(local_name) = self.expr_local_name(base_id, body) else {
            return false;
        };
        let local_ty = match self.locals.get(&local_name) {
            Some(binding) => binding.current_ty.clone(),
            None => return false,
        };

        match &local_ty {
            Ty::List(elem_ty, container_attr) | Ty::EvolvingList(elem_ty, container_attr) => {
                let is_evolving = matches!(local_ty, Ty::EvolvingList(_, _));
                let container_attr = container_attr.clone();
                let index_ty = self.infer_expr(index_id, body);
                let val_ty = self.infer_expr(value_id, body);
                let widened_val = val_ty.clone().widen_fresh();

                if matches!(**elem_ty, Ty::Never { .. }) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingList(Box::new(widened_val.clone()), container_attr)
                    } else {
                        Ty::List(Box::new(widened_val.clone()), container_attr)
                    };
                    self.assign_local(local_name.clone(), new_ty.clone());
                    self.sync_let_binding_type(&local_name, new_ty);
                } else if !self.is_subtype(&widened_val, elem_ty) {
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: *elem_ty.clone(),
                            got: widened_val.clone(),
                        },
                        value_id,
                        Vec::new(),
                    );
                }

                self.record_expr_type(base_id, local_ty);
                self.record_expr_type(index_id, index_ty);
                self.record_expr_type(target_id, widened_val);
                self.record_expr_type(value_id, val_ty);
                true
            }
            Ty::Map(key_ty, val_ty, container_attr)
            | Ty::EvolvingMap(key_ty, val_ty, container_attr) => {
                let is_evolving = matches!(local_ty, Ty::EvolvingMap(_, _, _));
                let container_attr = container_attr.clone();
                let index_ty = self.infer_expr(index_id, body);
                let value_ty = self.infer_expr(value_id, body);
                let widened_key = index_ty.clone().widen_fresh();
                let widened_val = value_ty.clone().widen_fresh();

                if matches!(**key_ty, Ty::Never { .. }) && matches!(**val_ty, Ty::Never { .. }) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingMap(
                            Box::new(widened_key),
                            Box::new(widened_val.clone()),
                            container_attr,
                        )
                    } else {
                        Ty::Map(
                            Box::new(widened_key),
                            Box::new(widened_val.clone()),
                            container_attr,
                        )
                    };
                    self.assign_local(local_name.clone(), new_ty.clone());
                    self.sync_let_binding_type(&local_name, new_ty);
                } else {
                    if !self.is_subtype(&widened_key, key_ty) {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: *key_ty.clone(),
                                got: widened_key,
                            },
                            index_id,
                            Vec::new(),
                        );
                    }
                    if !self.is_subtype(&widened_val, val_ty) {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: *val_ty.clone(),
                                got: widened_val.clone(),
                            },
                            value_id,
                            Vec::new(),
                        );
                    }
                }

                self.record_expr_type(base_id, local_ty);
                self.record_expr_type(index_id, index_ty);
                self.record_expr_type(target_id, widened_val);
                self.record_expr_type(value_id, value_ty);
                true
            }
            _ => false,
        }
    }

    /// Wrap a type in `Optional` unless it is already nullable.
    /// Check if an expression tree contains any Optional* nodes (`OptionalMemberAccess`,
    /// `OptionalIndex`, `OptionalCall`). Used to detect safe-chain assignment targets.
    fn expr_contains_optional(expr_id: ExprId, body: &ExprBody) -> bool {
        match &body.exprs[expr_id] {
            Expr::OptionalMemberAccess { .. }
            | Expr::OptionalIndex { .. }
            | Expr::OptionalCall { .. } => true,
            Expr::OptionalChain { expr } => Self::expr_contains_optional(*expr, body),
            Expr::MemberAccess { base, .. } | Expr::Index { base, .. } => {
                Self::expr_contains_optional(*base, body)
            }
            _ => false,
        }
    }

    fn make_optional(ty: Ty) -> Ty {
        match &ty {
            Ty::Optional(..) | Ty::Primitive(PrimitiveType::Null, _) => ty,
            Ty::Union(members, _)
                if members
                    .iter()
                    .any(|m| matches!(m, Ty::Primitive(PrimitiveType::Null, _))) =>
            {
                ty
            }
            _ => Ty::Optional(Box::new(ty), TyAttr::default()),
        }
    }

    fn join_types(a: &Ty, b: &Ty) -> Ty {
        if matches!(a, Ty::Never { .. }) {
            return b.clone();
        }
        if matches!(b, Ty::Never { .. }) {
            return a.clone();
        }
        if matches!(a, Ty::Void { .. }) || matches!(b, Ty::Void { .. }) {
            // TODO(TyAttr): Control-flow merge where either branch is Void — neither
            // branch's attr is obviously "the right one" to keep. May need a merge
            // operation on TyAttr, or default may be correct for synthesized types.
            return Ty::Void {
                attr: TyAttr::default(),
            };
        }
        if a == b {
            return a.clone();
        }
        // Same literal value, different freshness → normalize to Regular
        if let (Ty::Literal(lit_a, _, _), Ty::Literal(lit_b, _, _)) = (a, b) {
            if lit_a == lit_b {
                // TODO(TyAttr): Same literal from two branches with different freshness.
                // Could pick either side's attr or merge them; unclear which is correct.
                return Ty::Literal(
                    lit_a.clone(),
                    crate::ty::Freshness::Regular,
                    TyAttr::default(),
                );
            }
        }
        // Build a flat union, deduplicating members
        let mut members = Vec::new();
        let mut push = |ty: &Ty| {
            if let Ty::Union(inner, _) = ty {
                for m in inner {
                    if !members.contains(m) {
                        members.push(m.clone());
                    }
                }
            } else if !members.contains(ty) {
                members.push(ty.clone());
            }
        };
        push(a);
        push(b);
        if members.len() == 1 {
            members.into_iter().next().unwrap()
        } else {
            // TODO(TyAttr): Novel union synthesized from two different branch types.
            // No single original attr to preserve. Same question as union_ty in generics.rs.
            Ty::Union(members, TyAttr::default())
        }
    }

    fn join_all(types: &[Ty]) -> Ty {
        if types.is_empty() {
            return Ty::Never {
                attr: TyAttr::default(),
            };
        }
        types
            .iter()
            .skip(1)
            .fold(types[0].clone(), |acc, t| Self::join_types(&acc, t))
    }

    /// Subtype check — delegates to the normalizer which resolves type aliases
    /// and performs equirecursive structural subtyping.
    fn is_subtype(&self, sub: &Ty, sup: &Ty) -> bool {
        crate::normalize::is_subtype_of(sub, sup, &self.aliases)
    }

    /// Type intersection (greatest common subtype). Used by match-arm
    /// narrowing to compute the actual reachable type for an arm body —
    /// the values that satisfy both the scrutinee's type AND the arm
    /// pattern's natural type.
    fn intersect_types(&self, a: &Ty, b: &Ty) -> Ty {
        // Expand aliases up front so two aliases over overlapping unions
        // (e.g. `AliasA = int | string` ∩ `AliasB = string | bool`) reach
        // the union-distribution branch and pick out the shared member,
        // instead of falling through to `Never`.
        let a = self.expand_alias_chains(a.clone());
        let b = self.expand_alias_chains(b.clone());

        // Subtype shortcuts: if either side already covers the other, the
        // intersection is the narrower side.
        if self.is_subtype(&a, &b) {
            return a;
        }
        if self.is_subtype(&b, &a) {
            return b;
        }
        // Distribute over unions: (X | Y) ∩ T = (X ∩ T) | (Y ∩ T).
        if let Ty::Union(members, _) = &a {
            let intersected: Vec<Ty> = members
                .iter()
                .map(|m| self.intersect_types(m, &b))
                .filter(|t| !matches!(t, Ty::Never { .. }))
                .collect();
            return match intersected.len() {
                0 => Ty::Never {
                    attr: TyAttr::default(),
                },
                1 => intersected.into_iter().next().unwrap(),
                _ => Ty::Union(intersected, TyAttr::default()),
            };
        }
        if matches!(&b, Ty::Union(_, _)) {
            return self.intersect_types(&b, &a);
        }
        // No overlap — disjoint types intersect to Never.
        Ty::Never {
            attr: TyAttr::default(),
        }
    }

    fn infer_binary_op(
        &mut self,
        op: baml_compiler2_ast::BinaryOp,
        lhs: &Ty,
        rhs: &Ty,
        at: ExprId,
    ) -> Ty {
        use baml_compiler2_ast::BinaryOp;
        // Try constant folding on two literals first.
        if let Some(folded) = Self::try_fold_binary(op, lhs, rhs) {
            return folded;
        }
        match op {
            // Comparison / equality → bool
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => Ty::Primitive(PrimitiveType::Bool, TyAttr::default()),

            // Logical → bool
            BinaryOp::And | BinaryOp::Or => Ty::Primitive(PrimitiveType::Bool, TyAttr::default()),

            // Arithmetic: result type depends on operands
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                let result = Self::infer_arithmetic(op, lhs, rhs);
                if matches!(result, Ty::Unknown { .. })
                    && !matches!(lhs, Ty::Unknown { .. } | Ty::Error { .. })
                    && !matches!(rhs, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    self.context.report_simple(
                        TirTypeError::InvalidBinaryOp {
                            op,
                            lhs: lhs.clone(),
                            rhs: rhs.clone(),
                        },
                        at,
                    );
                }
                result
            }

            // Bitwise → int
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => Ty::Primitive(PrimitiveType::Int, TyAttr::default()),

            // Null coalescing: a ?? b
            // If a: T?, result is T | typeof(b).
            // Canonical unwrap: a ?? b where a: T? and b: T → T (non-nullable).
            BinaryOp::NullCoalesce => {
                let inner_lhs = crate::narrowing::remove_null(lhs);
                // If RHS is a subtype of the unwrapped LHS (e.g. int? ?? 1 → int),
                // return the unwrapped LHS directly instead of building a union.
                if self.is_subtype(rhs, &inner_lhs) {
                    inner_lhs
                } else if self.is_subtype(&inner_lhs, rhs) {
                    rhs.clone()
                } else {
                    Self::join_types(&inner_lhs, rhs)
                }
            }
        }
    }

    /// Determine the result type of an arithmetic operation (non-literal fallback).
    ///
    /// String concatenation is only valid for `Add`; other arithmetic ops on
    /// strings are invalid and return `Unknown` (triggering an error upstream).
    fn infer_arithmetic(op: baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Ty {
        fn promote(a: PrimitiveType, b: &PrimitiveType) -> Option<PrimitiveType> {
            if a == *b {
                return Some(a);
            }
            match (&a, &b) {
                (PrimitiveType::Int, PrimitiveType::Float)
                | (PrimitiveType::Float, PrimitiveType::Int) => Some(PrimitiveType::Float),
                _ => None,
            }
        }

        fn base_ty(ty: &Ty) -> Option<PrimitiveType> {
            match ty {
                Ty::Primitive(p, _) => Some(p.clone()),
                Ty::Literal(lit, _, _) => Some(PrimitiveType::from_literal(lit)),
                Ty::Union(members, _) => {
                    let mut result: Option<PrimitiveType> = None;
                    for m in members {
                        let p = base_ty(m)?;
                        result = Some(match result {
                            None => p,
                            Some(existing) => promote(existing, &p)?,
                        });
                    }
                    result
                }
                _ => None,
            }
        }

        match (base_ty(lhs), base_ty(rhs)) {
            (Some(PrimitiveType::Float), _) | (_, Some(PrimitiveType::Float)) => {
                Ty::Primitive(PrimitiveType::Float, TyAttr::default())
            }
            (Some(PrimitiveType::Int), Some(PrimitiveType::Int)) => {
                Ty::Primitive(PrimitiveType::Int, TyAttr::default())
            }
            (Some(PrimitiveType::String), _) | (_, Some(PrimitiveType::String)) => {
                // String concatenation only for Add
                if matches!(op, baml_compiler2_ast::BinaryOp::Add) {
                    Ty::Primitive(PrimitiveType::String, TyAttr::default())
                } else {
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
            }
            _ => Ty::Unknown {
                attr: TyAttr::default(),
            },
        }
    }

    fn infer_unary_op(&mut self, op: baml_compiler2_ast::UnaryOp, operand: &Ty, at: ExprId) -> Ty {
        // Try constant folding on a literal first.
        if let Some(folded) = Self::try_fold_unary(op, operand) {
            return folded;
        }
        let operand_attr = operand.attr().clone();
        match op {
            baml_compiler2_ast::UnaryOp::Not => Ty::Primitive(PrimitiveType::Bool, operand_attr),
            baml_compiler2_ast::UnaryOp::Neg => match operand {
                Ty::Primitive(PrimitiveType::Int, attr) => {
                    Ty::Primitive(PrimitiveType::Int, attr.clone())
                }
                Ty::Primitive(PrimitiveType::Float, attr) => {
                    Ty::Primitive(PrimitiveType::Float, attr.clone())
                }
                Ty::Unknown { attr } | Ty::Error { attr } => Ty::Unknown { attr: attr.clone() },
                _ => {
                    self.context.report_simple(
                        TirTypeError::InvalidUnaryOp {
                            op,
                            operand: operand.clone(),
                        },
                        at,
                    );
                    Ty::Unknown { attr: operand_attr }
                }
            },
        }
    }

    // ── Constant Folding ─────────────────────────────────────────────────────

    fn merge_freshness(a: crate::ty::Freshness, b: crate::ty::Freshness) -> crate::ty::Freshness {
        use crate::ty::Freshness;
        match (a, b) {
            (Freshness::Regular, Freshness::Regular) => Freshness::Regular,
            _ => Freshness::Fresh,
        }
    }

    /// Try to fold a unary operation on a literal into a new literal.
    fn try_fold_unary(op: baml_compiler2_ast::UnaryOp, operand: &Ty) -> Option<Ty> {
        use crate::ty::LiteralValue;
        let (lit, f) = match operand {
            Ty::Literal(lit, f, _) => (lit, *f),
            _ => return None,
        };
        match op {
            baml_compiler2_ast::UnaryOp::Neg => match lit {
                LiteralValue::Int(n) => Some(Ty::Literal(
                    LiteralValue::Int(n.checked_neg()?),
                    f,
                    TyAttr::default(),
                )),
                LiteralValue::Float(s) => {
                    let v: f64 = s.parse().ok()?;
                    Some(Ty::Literal(
                        LiteralValue::Float(format_float(-v)?),
                        f,
                        TyAttr::default(),
                    ))
                }
                _ => None,
            },
            baml_compiler2_ast::UnaryOp::Not => match lit {
                LiteralValue::Bool(b) => {
                    Some(Ty::Literal(LiteralValue::Bool(!b), f, TyAttr::default()))
                }
                _ => None,
            },
        }
    }

    /// Try to fold a binary operation on two literals into a new literal.
    fn try_fold_binary(op: baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Option<Ty> {
        use baml_compiler2_ast::BinaryOp;

        use crate::ty::LiteralValue;

        let (lhs_lit, lhs_f) = match lhs {
            Ty::Literal(lit, f, _) => (lit, *f),
            _ => return None,
        };
        let (rhs_lit, rhs_f) = match rhs {
            Ty::Literal(lit, f, _) => (lit, *f),
            _ => return None,
        };
        let f = Self::merge_freshness(lhs_f, rhs_f);

        // Int × Int
        if let (LiteralValue::Int(a), LiteralValue::Int(b)) = (lhs_lit, rhs_lit) {
            let (a, b) = (*a, *b);
            return match op {
                BinaryOp::Add => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_add(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Sub => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_sub(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Mul => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_mul(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Div => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_div(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Mod => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_rem(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::BitAnd => {
                    Some(Ty::Literal(LiteralValue::Int(a & b), f, TyAttr::default()))
                }
                BinaryOp::BitOr => {
                    Some(Ty::Literal(LiteralValue::Int(a | b), f, TyAttr::default()))
                }
                BinaryOp::BitXor => {
                    Some(Ty::Literal(LiteralValue::Int(a ^ b), f, TyAttr::default()))
                }
                BinaryOp::Shl => {
                    let shift = u32::try_from(b).ok()?;
                    Some(Ty::Literal(
                        LiteralValue::Int(a.checked_shl(shift)?),
                        f,
                        TyAttr::default(),
                    ))
                }
                BinaryOp::Shr => {
                    let shift = u32::try_from(b).ok()?;
                    Some(Ty::Literal(
                        LiteralValue::Int(a.checked_shr(shift)?),
                        f,
                        TyAttr::default(),
                    ))
                }
                BinaryOp::Eq => Some(Ty::Literal(
                    LiteralValue::Bool(a == b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Ne => Some(Ty::Literal(
                    LiteralValue::Bool(a != b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f, TyAttr::default())),
                BinaryOp::Le => Some(Ty::Literal(
                    LiteralValue::Bool(a <= b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f, TyAttr::default())),
                BinaryOp::Ge => Some(Ty::Literal(
                    LiteralValue::Bool(a >= b),
                    f,
                    TyAttr::default(),
                )),
                _ => None,
            };
        }

        // Float × Float
        if let (LiteralValue::Float(a_s), LiteralValue::Float(b_s)) = (lhs_lit, rhs_lit) {
            let a: f64 = a_s.parse().ok()?;
            let b: f64 = b_s.parse().ok()?;
            return match op {
                BinaryOp::Add => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a + b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Sub => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a - b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Mul => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a * b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Div if b != 0.0 => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a / b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Mod if b != 0.0 => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a % b)?),
                    f,
                    TyAttr::default(),
                )),
                #[allow(clippy::float_cmp)] // Intentional: folding literal float equality
                BinaryOp::Eq => Some(Ty::Literal(
                    LiteralValue::Bool(a == b),
                    f,
                    TyAttr::default(),
                )),
                #[allow(clippy::float_cmp)] // Intentional: folding literal float inequality
                BinaryOp::Ne => Some(Ty::Literal(
                    LiteralValue::Bool(a != b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f, TyAttr::default())),
                BinaryOp::Le => Some(Ty::Literal(
                    LiteralValue::Bool(a <= b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f, TyAttr::default())),
                BinaryOp::Ge => Some(Ty::Literal(
                    LiteralValue::Bool(a >= b),
                    f,
                    TyAttr::default(),
                )),
                _ => None,
            };
        }

        // Bool × Bool
        if let (LiteralValue::Bool(a), LiteralValue::Bool(b)) = (lhs_lit, rhs_lit) {
            let (a, b) = (*a, *b);
            return match op {
                BinaryOp::And => Some(Ty::Literal(
                    LiteralValue::Bool(a && b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Or => Some(Ty::Literal(
                    LiteralValue::Bool(a || b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Eq => Some(Ty::Literal(
                    LiteralValue::Bool(a == b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Ne => Some(Ty::Literal(
                    LiteralValue::Bool(a != b),
                    f,
                    TyAttr::default(),
                )),
                _ => None,
            };
        }

        // String × String
        if let (LiteralValue::String(a), LiteralValue::String(b)) = (lhs_lit, rhs_lit) {
            return match op {
                BinaryOp::Add => Some(Ty::Literal(
                    LiteralValue::String(format!("{a}{b}")),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Eq => Some(Ty::Literal(
                    LiteralValue::Bool(a == b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Ne => Some(Ty::Literal(
                    LiteralValue::Bool(a != b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f, TyAttr::default())),
                BinaryOp::Le => Some(Ty::Literal(
                    LiteralValue::Bool(a <= b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f, TyAttr::default())),
                BinaryOp::Ge => Some(Ty::Literal(
                    LiteralValue::Bool(a >= b),
                    f,
                    TyAttr::default(),
                )),
                _ => None,
            };
        }

        None
    }

    fn assign_op_to_binary_op(op: baml_compiler2_ast::AssignOp) -> baml_compiler2_ast::BinaryOp {
        use baml_compiler2_ast::{AssignOp, BinaryOp};
        match op {
            AssignOp::Add => BinaryOp::Add,
            AssignOp::Sub => BinaryOp::Sub,
            AssignOp::Mul => BinaryOp::Mul,
            AssignOp::Div => BinaryOp::Div,
            AssignOp::Mod => BinaryOp::Mod,
            AssignOp::BitAnd => BinaryOp::BitAnd,
            AssignOp::BitOr => BinaryOp::BitOr,
            AssignOp::BitXor => BinaryOp::BitXor,
            AssignOp::Shl => BinaryOp::Shl,
            AssignOp::Shr => BinaryOp::Shr,
        }
    }

    /// Infer/check a lambda body using a save/restore approach.
    ///
    /// Saves the current locals, `declared_return_ty`, `generic_params`, and
    /// `expressions` (to avoid `ExprId` collisions between
    /// the lambda's arena and the parent's arena). After inference, restores all
    /// saved state and returns the lambda's expression types separately.
    ///
    /// Returns `(inferred_return_ty, lambda_expressions)` where
    /// `lambda_expressions` contains the expression types for the lambda body
    /// only (keyed by the lambda's own `ExprId`s, which start at 0).
    pub fn infer_lambda_body(
        &mut self,
        func_def: &baml_compiler2_ast::FunctionDef,
        param_tys: &[FunctionParamTy],
        expected_ret: Option<&Ty>,
        chosen_throws: &Ty,
        throws_report_span: TextRange,
        warn_extraneous_throws: bool,
    ) -> (Ty, FxHashMap<ExprId, Ty>, Option<FileScopeId>, Ty) {
        use baml_compiler2_ast::FunctionBodyDef;

        // Get the lambda's ExprBody
        let Some(FunctionBodyDef::Expr(lambda_body, lambda_source_map)) = &func_def.body else {
            return (
                Ty::Unknown {
                    attr: TyAttr::default(),
                },
                FxHashMap::default(),
                None,
                Ty::Never {
                    attr: TyAttr::default(),
                },
            );
        };

        let Some(root_expr) = lambda_body.root_expr else {
            return (
                Ty::Void {
                    attr: TyAttr::default(),
                },
                FxHashMap::default(),
                None,
                Ty::Never {
                    attr: TyAttr::default(),
                },
            );
        };

        // Save current state (including expressions to prevent ExprId collisions)
        let saved_locals = self.locals.clone();
        let saved_scoped_local_declarations = std::mem::take(&mut self.scoped_local_declarations);
        let saved_scoped_local_assignments = std::mem::take(&mut self.scoped_local_assignments);
        let saved_return_ty = self.declared_return_ty.clone();
        let saved_generic_params = self.generic_params.clone();
        let saved_expressions = std::mem::take(&mut self.expressions);
        let saved_bindings = std::mem::take(&mut self.pattern_types);
        let saved_pattern_natural_cache = std::mem::take(&mut self.pattern_natural_cache);
        let saved_resolutions = std::mem::take(&mut self.resolutions);
        let saved_exhaustive_matches = std::mem::take(&mut self.exhaustive_matches);
        let saved_catch_residual_throws = std::mem::take(&mut self.catch_residual_throws);
        let saved_path_root_types = std::mem::take(&mut self.path_root_types);
        let saved_path_segment_types = std::mem::take(&mut self.path_segment_types);
        let saved_path_member_resolutions = std::mem::take(&mut self.path_member_resolutions);
        let saved_lambda_effective_throws = std::mem::take(&mut self.lambda_effective_throws);
        let saved_call_plans = std::mem::take(&mut self.call_plans);
        let saved_function_coercions = std::mem::take(&mut self.function_coercions);
        let saved_body_source_map = self.body_source_map.clone();
        self.body_source_map = Some(lambda_source_map.clone());

        // Extend generic params with the lambda's own generic params
        let mut new_generic_params = self.generic_params.clone();
        new_generic_params.extend(func_def.generic_params.iter().cloned());
        self.generic_params = new_generic_params;

        // Seed lambda params (captures remain accessible via parent locals).
        //
        // Directly overwrite `locals` rather than going through `add_local`:
        // that helper preserves an existing declared contract, but lambda
        // params shadow outer lets. The lambda param's declared type must
        // replace any outer declaration, and params carry no let-pattern
        // identity.
        for param in param_tys {
            if let Some(name) = &param.name {
                self.locals.insert(
                    name.clone(),
                    LocalBinding {
                        current_ty: param.ty.clone(),
                        declared_ty: Some(param.ty.clone()),
                        pattern: None,
                    },
                );
            }
        }

        // Seed captures from HIR semantic index as Ty::Unknown.
        //
        // When `infer_lambda_body` is called from a parent scope (e.g. when the outer
        // lambda scope infers its body and encounters an inner lambda), captures from
        // grandparent scopes are NOT visible in `saved_locals`. Look up the lambda
        // scope by its span and seed its captures as `Ty::Unknown` to suppress false
        // "unresolved name" diagnostics. Names already in locals are left unchanged.
        //
        // Also captures the lambda's `FileScopeId` for use as a position-independent
        // key in `nested_lambda_types` (avoids TextRange in Salsa-cached output).
        let lambda_file_scope_id = {
            let db = self.context.db();
            let file = self.context.scope().file(db);
            let index = baml_compiler2_ppir::file_semantic_index(db, file);
            // Captures are seeded only if the lambda scope is located (it
            // always should be, but be defensive).
            let found_fsi = index.lambda_scope_for(func_def.span);
            if let Some(fsi) = found_fsi {
                for (capture_name, _def_site) in
                    &index.scope_bindings[fsi.index() as usize].captures
                {
                    if !self.locals.contains_key(capture_name) {
                        self.seed_capture_unknown(capture_name.clone());
                    }
                }
            }
            found_fsi
        };

        // Set return type context for return statement checking inside lambda
        if let Some(ret) = expected_ret {
            self.declared_return_ty = Some(ret.clone());
        } else {
            self.declared_return_ty = None;
        }

        // Infer or check the lambda body
        let ret_ty = if let Some(expected) = expected_ret {
            if matches!(expected, Ty::Unknown { .. } | Ty::TypeVar(_, _)) {
                // Expected return is unknown or a type var — just infer
                self.infer_expr(root_expr, lambda_body)
            } else {
                self.check_expr(root_expr, lambda_body, expected)
            }
        } else {
            self.infer_expr(root_expr, lambda_body)
        };

        self.check_throws_surface(
            lambda_body,
            chosen_throws,
            throws_report_span,
            warn_extraneous_throws,
        );
        let lambda_effective_throws = Self::ty_from_concrete_facts(
            &self.collect_effective_throws(lambda_body),
        )
        .unwrap_or(Ty::Never {
            attr: TyAttr::default(),
        });

        // Collect the lambda's expression types and restore parent state
        let lambda_expressions = std::mem::replace(&mut self.expressions, saved_expressions);
        self.pattern_types = saved_bindings;
        self.pattern_natural_cache = saved_pattern_natural_cache;
        self.resolutions = saved_resolutions;
        self.exhaustive_matches = saved_exhaustive_matches;
        self.catch_residual_throws = saved_catch_residual_throws;
        self.path_root_types = saved_path_root_types;
        self.path_segment_types = saved_path_segment_types;
        self.path_member_resolutions = saved_path_member_resolutions;
        self.lambda_effective_throws = saved_lambda_effective_throws;
        self.call_plans = saved_call_plans;
        self.function_coercions = saved_function_coercions;
        self.locals = saved_locals;
        self.scoped_local_declarations = saved_scoped_local_declarations;
        self.scoped_local_assignments = saved_scoped_local_assignments;
        self.declared_return_ty = saved_return_ty;
        self.generic_params = saved_generic_params;
        self.body_source_map = saved_body_source_map;

        (
            ret_ty,
            lambda_expressions,
            lambda_file_scope_id,
            lambda_effective_throws,
        )
    }
}

// ── PatCtx integration ────────────────────────────────────────────────────────
//
// The exhaustiveness algorithm in `crate::exhaustiveness` is parameterized
// over a `PatCtx` trait that asks the type system four questions:
// "what ctors inhabit this type", "what field types does this class have",
// "what's the element type of this list", "is this type inhabited".
//
// The natural place to answer those questions is the builder, since it
// already holds the alias map, package items, and Salsa db. We just impl
// the trait directly.

impl crate::exhaustiveness::PatCtx for TypeInferenceBuilder<'_> {
    fn enumerate_ctors(&self, ty: &Ty) -> Vec<crate::exhaustiveness::Ctor> {
        use crate::exhaustiveness::Ctor;
        // Always peel aliases first — the algorithm shouldn't see
        // `Ty::TypeAlias` as an opaque ctor when the alias has a target.
        let ty = self.expand_alias_chains(ty.clone());
        match &ty {
            Ty::Primitive(PrimitiveType::Bool, _) => vec![
                Ctor::Single(Ty::Literal(
                    baml_base::Literal::Bool(true),
                    Freshness::Regular,
                    TyAttr::default(),
                )),
                Ctor::Single(Ty::Literal(
                    baml_base::Literal::Bool(false),
                    Freshness::Regular,
                    TyAttr::default(),
                )),
            ],
            Ty::Primitive(PrimitiveType::Null, _) => vec![Ctor::Single(ty.clone())],
            // Infinite-alphabet / opaque primitives and types — all
            // require a wildcard arm for exhaustiveness.
            Ty::Primitive(_, _)
            | Ty::Map(..)
            | Ty::EvolvingMap(..)
            | Ty::Function { .. }
            | Ty::Type { .. }
            | Ty::RustType { .. }
            | Ty::Void { .. }
            | Ty::BuiltinUnknown { .. }
            | Ty::Unknown { .. }
            | Ty::Error { .. }
            | Ty::TypeVar(_, _) => vec![Ctor::NonExhaustive],
            Ty::Never { .. } => vec![],
            Ty::Optional(inner, _) => {
                let mut out = self.enumerate_ctors(inner);
                out.push(Ctor::Single(Ty::Primitive(
                    PrimitiveType::Null,
                    TyAttr::default(),
                )));
                out
            }
            // Each union member becomes a `UnionMember` tag, mirroring
            // rustc's `Variant` ctor for enums. Specializing on
            // `UnionMember(M)` recurses into a column of type `M`, so
            // structural patterns (slices, classes) inside a union are
            // analysed with their full ctor set at that depth — slice
            // splitting fires for `[]` + `[_, ..]` to recognise full
            // coverage of a list branch, etc.
            Ty::Union(members, _) => members
                .iter()
                .map(|m| Ctor::UnionMember(m.clone()))
                .collect(),
            Ty::Literal(_, _, _) | Ty::EnumVariant(_, _, _) => vec![Ctor::Single(ty.clone())],
            Ty::Enum(qtn, _) => self
                .lookup_enum_variants(qtn)
                .into_iter()
                .map(|variant| {
                    Ctor::Single(Ty::EnumVariant(qtn.clone(), variant, TyAttr::default()))
                })
                .collect(),
            Ty::Class(qtn, _, _) => vec![Ctor::Class(qtn.clone())],
            // Futures are non-exhaustive at the pattern level: there is
            // no surface syntax to match against `Future<T, E>` other
            // than a wildcard.
            Ty::Future(..) => vec![Ctor::NonExhaustive],
            // Slice path in `split_ctors` enumerates length classes from
            // the matrix; this branch is only reached if the algorithm
            // calls into us with a List directly, in which case empty
            // means "let the slice path handle it."
            Ty::List(_, _) | Ty::EvolvingList(_, _) => vec![],
            // After expand_alias_chains, no remaining `TypeAlias` should
            // appear; if it does (cycle), treat as opaque.
            Ty::TypeAlias(_, _) => vec![Ctor::NonExhaustive],
        }
    }

    fn class_field_types(&self, qtn: &crate::ty::QualifiedTypeName, ty: &Ty) -> Vec<Ty> {
        let args = match ty {
            Ty::Class(_, args, _) => args.clone(),
            _ => Vec::new(),
        };
        // Normalize each field type for matrix consumption — matches the
        // normalization the dispatcher applied to each field pattern's
        // dpat scrut tag, so column types and row ctors stay aligned.
        // Without this, an `Optional<T>` field column would enumerate only
        // `Single(null)` (List/Function inner ctors are missing here) and
        // wildcard rows for `T`-shaped patterns would shadow concrete
        // `null` arms.
        self.class_field_types_ordered(qtn, &args)
            .into_iter()
            .map(|ft| self.matrix_normalize_scrut(&ft))
            .collect()
    }

    fn list_element_type(&self, ty: &Ty) -> Ty {
        let elem = match self.expand_alias_chains(ty.clone()) {
            Ty::List(elem, _) | Ty::EvolvingList(elem, _) => *elem,
            other => other,
        };
        // Same rationale as class_field_types: list element columns flow
        // into slice-pattern specialization with normalized scrut tags.
        self.matrix_normalize_scrut(&elem)
    }

    // is_inhabited uses the trait's default impl (recursive walk over
    // class fields with cycle protection). Override later with a
    // Salsa-cached query if profiling shows it matters.
}

// ── New pattern integration helpers ──────────────────────────────────────────

impl TypeInferenceBuilder<'_> {
    /// After `analyze_and_lower`, register the pattern's bindings and
    /// `pattern_types` entries, and emit a refutable-let diagnostic if
    /// the pattern is refutable in an irrefutable context.
    ///
    /// Replaces the binding-registration / refutability portion of the
    /// old `apply_pattern_result`. Unlike that version, this one trusts
    /// `check_irrefutable` directly — no structural-pattern workaround.
    fn finalize_pattern_lowering(
        &mut self,
        pattern: PatId,
        result: &crate::pattern_lowering::PatternResult,
        declared_for_scope: Option<&Ty>,
        irrefutable: Option<IrrefutablePatternContext>,
        scrut_ty: &Ty,
    ) {
        // Per-binding pattern_types entry — keyed by source PatId so
        // LSP/codegen can look up the binding's type at any of its source
        // positions (or-pattern alternatives, chain alias binds, etc.).
        for binding in &result.bindings {
            self.pattern_types
                .insert(binding.pat_id, binding.ty.clone());
        }

        // Scope registration. `locals` is by-name so duplicate names
        // (or-pattern alternatives) collapse to the last declared.
        for binding in &result.bindings {
            self.declare_scoped_local(
                binding.name.clone(),
                binding.pat_id,
                binding.ty.clone(),
                declared_for_scope.cloned(),
            );
        }

        // Refutable-pattern check for irrefutable contexts (let, for,
        // catch). Now consistent for every pattern shape — the matrix
        // tells us definitively whether the pattern covers all values
        // of the scrutinee type. Normalize the scrut for the matrix so
        // the column type matches the dpat scrut tags produced by
        // `analyze_and_lower_no_subtype_check`.
        if let Some(ctx) = irrefutable
            && crate::pattern_lowering::check_irrefutable(
                self,
                &result.dpat,
                self.matrix_normalize_scrut(scrut_ty),
            )
            .is_err()
        {
            self.report_refutable_pattern_in_irrefutable_context(
                pattern,
                ctx.fallback_expr,
                ctx.context,
            );
        }
    }
}

// ── Pattern lowering walk ────────────────────────────────────────────────────
//
// `analyze_and_lower` is the per-pattern recursion that produces a
// `PatternResult` (DPat for the matrix + matched_ty + required_ty +
// bindings).

impl TypeInferenceBuilder<'_> {
    /// Recursively lower a source pattern to a [`PatternResult`].
    ///
    /// Bidirectional inference: `scrut_ty` flows DOWN; `required_ty` flows
    /// UP for the surrounding context to validate or refine its scrutinee
    /// expectation; `matched_ty` is the narrowed type for the pattern's
    /// arm body (downward flow into the body's expression inference).
    ///
    /// As a side effect, populates `self.pattern_types[pat_id]` with the
    /// pattern's `matched_ty`. MIR's `pat_ty` looks up the type for
    /// *every* pattern `PatId` during structural destructure lowering — not
    /// just the binding `PatIds` — so this insertion at every recursion
    /// level is load-bearing.
    fn analyze_and_lower(
        &mut self,
        pat_id: PatId,
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        self.check_pattern_vs_scrut_subtype(pat_id, scrut_ty, body, at_expr);
        self.analyze_and_lower_no_subtype_check(pat_id, scrut_ty, body, at_expr)
    }

    /// Strict pattern-vs-scrut type check (rustc-style). A pattern's
    /// natural type must be a subtype of the scrutinee — otherwise the
    /// arm could never fire for any value of the scrut. This catches
    /// cases like `match x: int[] { []: string[] => … }` (string[] is
    /// not a subtype of int[]) and `match x: Foo { Bar::D => … }` (Bar
    /// and Foo disjoint), where we'd otherwise silently produce an
    /// unreachable arm with `matched_ty = Never`.
    ///
    /// Skipped during error-recovery / generics to avoid cascading
    /// diagnostics. Bind/Wildcard patterns get the `Never` unconstrained
    /// natural here, so they trivially pass via `Never <: anything`.
    ///
    /// Match/let/for callers go through `analyze_and_lower` and get this
    /// check for free. Catch arms call `analyze_and_lower_no_subtype_check`
    /// directly because catch dispatch is a runtime type test — an `int`
    /// arm against a `string`-only throw set is unreachable, not ill-typed,
    /// and that case is already reported as an unreachable-arm warning by
    /// `infer_catch_expr`'s per-arm `throw_matches.may_match.is_empty()`
    /// check.
    fn check_pattern_vs_scrut_subtype(
        &mut self,
        pat_id: PatId,
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) {
        // Hot-path short-circuit: bare wildcard / bare bind have `Never`
        // natural type by construction, and `Never <: anything` makes the
        // subtype check trivially pass. Skip the natural-type walk and the
        // two `is_subtype` calls — these are the common forms in `let x =
        // …` / `for x in …` so the savings are material.
        if matches!(
            body.patterns[pat_id],
            ast::Pattern::Wildcard | ast::Pattern::Bind { subpat: None, .. }
        ) {
            return;
        }
        // `Never` for unconstrained leaves (Wildcard / bare Bind / empty
        // Array): plain strict subtype works without carve-outs because
        // `Never <: anything`.
        let pat_natural = self.pattern_natural_type(
            pat_id,
            body,
            &Ty::Never {
                attr: TyAttr::default(),
            },
        );
        let scrut_for_check = self.expand_alias_chains(scrut_ty.clone());
        if !Self::ty_contains_recovery_unknown(&pat_natural)
            && !Self::ty_contains_recovery_unknown(&scrut_for_check)
            && !crate::generics::contains_typevar(&pat_natural)
            && !crate::generics::contains_typevar(&scrut_for_check)
            && !self.is_subtype(&pat_natural, &scrut_for_check)
            && !self.is_subtype(&scrut_for_check, &pat_natural)
        {
            let err = TirTypeError::TypeMismatch {
                expected: scrut_ty.clone(),
                got: pat_natural,
            };
            self.report_at_pat_or_expr(err, pat_id, at_expr);
        }
    }

    fn analyze_and_lower_no_subtype_check(
        &mut self,
        pat_id: PatId,
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        let result = self.lower_pat_dispatch(pat_id, scrut_ty, body, at_expr);
        // Single point of truth for `pattern_types[pat_id]`. All dispatch
        // branches above (union-member wrap, opaque-scrut wrap, fallthrough
        // to `analyze_and_lower_inner`) flow through here, so MIR/LSP see
        // exactly the matched_ty that this call returned to its caller —
        // including the wrapped/joined types from the union/opaque paths.
        self.pattern_types.insert(pat_id, result.matched_ty.clone());
        result
    }

    /// Pre-`pattern_types` dispatch step. See `analyze_and_lower_no_subtype_check`
    /// for the wrapper that records the pattern's matched type; this function
    /// owns the union/opaque/inner branching and never writes to
    /// `pattern_types[pat_id]` directly.
    ///
    /// If the scrutinee is a Union, decide which members this pattern
    /// targets. Most patterns target exactly one member and get wrapped in
    /// `UnionMember(M)` so the matrix can specialise on the union branch —
    /// matching rustc's `Variant`-based approach for enums.
    ///
    /// An array pattern with no element-type constraint targets *every*
    /// list-typed member (consider `[..]` against `List(int) | List(Never)`
    /// — the pattern matches every value of either member). In that case
    /// we wrap as an Or across one `UnionMember` per matching member, with
    /// the same inner `DPat` lowered for each member's element type.
    ///
    /// `Optional<T>` is normalized to `Union<T, null>` for matrix purposes
    /// only (`matrix_normalize_scrut`). The dpat scrut tags attached here
    /// use the normalized form so the matrix's `UnionMember` specialization
    /// can find the member inside the column type. `matched_ty` keeps the
    /// original `Optional` representation.
    fn lower_pat_dispatch(
        &mut self,
        pat_id: PatId,
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        let normalized_scrut = self.matrix_normalize_scrut(scrut_ty);
        if let Ty::Union(members, _) = &normalized_scrut {
            let targets = self.union_targets_for_pattern(pat_id, body, members);
            if targets.len() == 1 {
                let member_ty = targets.into_iter().next().unwrap();
                let inner = self.analyze_and_lower_inner(pat_id, &member_ty, body, at_expr);
                let wrapped = crate::exhaustiveness::DPat::union_member(
                    member_ty,
                    inner.dpat,
                    normalized_scrut,
                );
                // Use `inner.matched_ty` (the deepest narrowed type) rather
                // than `member_ty` (the union-member projection). They match
                // for shallow unions, but for nested cases like
                // `Optional<Union<A, B>>` the outer projection is
                // `Union<A, B>` while the inner narrows further to `A` —
                // bindings and arm-body narrowing want the latter. The
                // multi-target branch below already does this via
                // `join_all(&matched_tys)`.
                return crate::pattern_lowering::PatternResult {
                    dpat: wrapped,
                    required_ty: inner.required_ty,
                    matched_ty: inner.matched_ty,
                    bindings: inner.bindings,
                };
            }
            if targets.len() > 1 {
                let mut alts: Vec<crate::exhaustiveness::DPat> = Vec::with_capacity(targets.len());
                let mut required_tys: Vec<Ty> = Vec::new();
                let mut matched_tys: Vec<Ty> = Vec::new();
                // Per-branch bindings: same name appears once per branch
                // (with a per-member type). After all branches lower, we
                // collapse same-named bindings into one entry typed at
                // the join of the per-branch types — so e.g. `parsed`
                // bound across `S` and `StreamNoYield` ends up typed at
                // `S | StreamNoYield` rather than last-write-wins.
                let mut bindings_by_name: indexmap::IndexMap<Name, (PatId, Vec<Ty>)> =
                    indexmap::IndexMap::new();
                for member_ty in &targets {
                    let inner = self.analyze_and_lower_inner(pat_id, member_ty, body, at_expr);
                    let wrapped = crate::exhaustiveness::DPat::union_member(
                        member_ty.clone(),
                        inner.dpat,
                        normalized_scrut.clone(),
                    );
                    alts.push(wrapped);
                    if let Some(req) = inner.required_ty {
                        required_tys.push(req);
                    }
                    matched_tys.push(inner.matched_ty);
                    for b in inner.bindings {
                        let entry = bindings_by_name
                            .entry(b.name.clone())
                            .or_insert_with(|| (b.pat_id, Vec::new()));
                        entry.1.push(b.ty);
                    }
                }
                let joined_bindings: Vec<crate::pattern_lowering::PatternBinding> =
                    bindings_by_name
                        .into_iter()
                        .map(
                            |(name, (pat_id, tys))| crate::pattern_lowering::PatternBinding {
                                name,
                                pat_id,
                                ty: Self::join_all(&tys),
                            },
                        )
                        .collect();
                return crate::pattern_lowering::PatternResult {
                    dpat: crate::exhaustiveness::DPat::or(alts, normalized_scrut),
                    required_ty: if required_tys.is_empty() {
                        None
                    } else {
                        Some(Self::join_all(&required_tys))
                    },
                    matched_ty: Self::join_all(&matched_tys),
                    bindings: joined_bindings,
                };
            }
        }
        // Opaque scrutinee (`unknown` / `BuiltinUnknown`) with a pattern
        // that has a strict-narrower natural type: dispatch onto a virtual
        // single-member union. Without this, `let n: int` against scrut
        // `unknown` would lower to a column-wide wildcard and shadow any
        // sibling `_ => …` arm — which is wrong, because at runtime the
        // arm only fires when the value's type tag is `int`. Wrapping in
        // `UnionMember(int)` lets the matrix split this row from a
        // sibling wildcard via the same `NonExhaustive`-vs-concrete-ctor
        // mechanism it uses for `int` literals.
        if matches!(scrut_ty, Ty::Unknown { .. } | Ty::BuiltinUnknown { .. }) {
            let natural = self.pattern_natural_type(
                pat_id,
                body,
                &Ty::Unknown {
                    attr: TyAttr::default(),
                },
            );
            if !matches!(
                natural,
                Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. } | Ty::TypeVar(..)
            ) {
                let inner = self.analyze_and_lower_inner(pat_id, &natural, body, at_expr);
                let wrapped = crate::exhaustiveness::DPat::union_member(
                    natural,
                    inner.dpat,
                    scrut_ty.clone(),
                );
                return crate::pattern_lowering::PatternResult {
                    dpat: wrapped,
                    required_ty: inner.required_ty,
                    matched_ty: inner.matched_ty,
                    bindings: inner.bindings,
                };
            }
        }
        self.analyze_and_lower_inner(pat_id, scrut_ty, body, at_expr)
    }

    /// Compute every union member that a pattern can possibly match.
    ///
    /// Type-system-driven: derive the pattern's *natural type* from the
    /// AST (e.g. `[Bucket {}]` ⇒ `List<Bucket>`, `[..]` ⇒ `List<Unknown>`,
    /// `Box {}` ⇒ `Box<Unknown>`) and keep every union member that's
    /// compatible with it via subtype in either direction. No per-pattern
    /// AST-shape rules — the same compatibility check drives every shape.
    ///
    /// The result drives the dispatcher: 0 → no wrap (fall through),
    /// 1 → single `UnionMember` wrap, >1 → Or-of-UnionMember.
    fn union_targets_for_pattern(
        &mut self,
        pat_id: PatId,
        body: &ExprBody,
        union_members: &[Ty],
    ) -> Vec<Ty> {
        // Or-patterns are handled per-branch by the recursive walk, so
        // the outer Or itself never wraps.
        if matches!(&body.patterns[pat_id], ast::Pattern::Or(_)) {
            return Vec::new();
        }
        let natural = self.pattern_natural_type(
            pat_id,
            body,
            &Ty::Unknown {
                attr: TyAttr::default(),
            },
        );
        // Pure `Unknown` natural type means a wildcard/bind: targets
        // every member, but the dispatcher prefers no-wrap in that case.
        if matches!(natural, Ty::Unknown { .. }) {
            return Vec::new();
        }
        // Pattern matching dispatches on runtime type identity, not the
        // type-system subtype relation. Subtype is too liberal because of
        // cross-tag bridges like `Int <: Float` (the numeric tower) — a
        // value tagged `int` at runtime is never matched by a `float`
        // pattern, despite the type system saying `Int <: Float`. Use a
        // structural overlap check that respects runtime tag identity:
        // primitives must agree on head, classes must agree on qtn AND
        // overlap pairwise on their generic args.
        union_members
            .iter()
            .filter(|m| self.types_overlap(&natural, m))
            .cloned()
            .collect()
    }

    /// Two types overlap (for pattern dispatch) iff some atom of `a`
    /// shares a runtime identity with some atom of `b`. Unions/Optionals
    /// decompose into atoms; everything else is a single atom matched by
    /// [`Self::atoms_overlap`].
    fn types_overlap(&self, a: &Ty, b: &Ty) -> bool {
        let mut a_atoms: Vec<Ty> = Vec::new();
        self.collect_overlap_atoms(a, &mut a_atoms);
        let mut b_atoms: Vec<Ty> = Vec::new();
        self.collect_overlap_atoms(b, &mut b_atoms);
        a_atoms
            .iter()
            .any(|aa| b_atoms.iter().any(|bb| self.atoms_overlap(aa, bb)))
    }

    fn collect_overlap_atoms(&self, t: &Ty, out: &mut Vec<Ty>) {
        let expanded = self.expand_alias_chains(t.clone());
        match expanded {
            Ty::Union(members, _) => {
                for m in members {
                    self.collect_overlap_atoms(&m, out);
                }
            }
            Ty::Optional(inner, _) => {
                self.collect_overlap_atoms(&inner, out);
                out.push(Ty::Primitive(PrimitiveType::Null, TyAttr::default()));
            }
            other => out.push(other),
        }
    }

    /// Structural identity check used by [`Self::types_overlap`]. Two
    /// atoms overlap iff they could share at least one runtime value.
    fn atoms_overlap(&self, a: &Ty, b: &Ty) -> bool {
        // Error/recovery types are bidirectionally compatible with
        // anything to suppress cascading diagnostics.
        if matches!(a, Ty::Unknown { .. } | Ty::Error { .. })
            || matches!(b, Ty::Unknown { .. } | Ty::Error { .. })
        {
            return true;
        }
        // Unconstrained type variables target every atom.
        if matches!(a, Ty::TypeVar(..)) || matches!(b, Ty::TypeVar(..)) {
            return true;
        }
        match (a, b) {
            // Primitives: strict head equality (no `Int <: Float`).
            (Ty::Primitive(p1, _), Ty::Primitive(p2, _)) => p1 == p2,
            // Literal vs primitive: literal's primitive head must match.
            (Ty::Literal(lit, _, _), Ty::Primitive(p, _))
            | (Ty::Primitive(p, _), Ty::Literal(lit, _, _)) => {
                PrimitiveType::from_literal(lit) == *p
            }
            // Two literals: same value (modulo float canonicalization).
            (Ty::Literal(l1, _, _), Ty::Literal(l2, _, _)) => l1 == l2,
            // Class: same qtn AND every type-arg pair overlaps.
            (Ty::Class(q1, args1, _), Ty::Class(q2, args2, _)) => {
                q1 == q2
                    && args1.len() == args2.len()
                    && args1
                        .iter()
                        .zip(args2.iter())
                        .all(|(x, y)| self.types_overlap(x, y))
            }
            // Enum/EnumVariant: same enum qtn. Variants of the same enum
            // overlap with each other and with the bare enum.
            (Ty::Enum(q1, _), Ty::Enum(q2, _)) => q1 == q2,
            (Ty::EnumVariant(q1, v1, _), Ty::EnumVariant(q2, v2, _)) => q1 == q2 && v1 == v2,
            (Ty::Enum(q1, _), Ty::EnumVariant(q2, _, _))
            | (Ty::EnumVariant(q2, _, _), Ty::Enum(q1, _)) => q1 == q2,
            // List/Map: same head AND inner types overlap. The element /
            // key / value types are part of the pattern's natural shape
            // (e.g. `[let x: int]` has natural `List<int>`), so a pattern
            // targeting `List<int>` does not target `List<string>`.
            (
                Ty::List(a_elem, _) | Ty::EvolvingList(a_elem, _),
                Ty::List(b_elem, _) | Ty::EvolvingList(b_elem, _),
            ) => self.types_overlap(a_elem, b_elem),
            (
                Ty::Map(a_k, a_v, _) | Ty::EvolvingMap(a_k, a_v, _),
                Ty::Map(b_k, b_v, _) | Ty::EvolvingMap(b_k, b_v, _),
            ) => self.types_overlap(a_k, b_k) && self.types_overlap(a_v, b_v),
            // Function: arities match AND every param pair overlaps AND
            // returns overlap. A `(int) -> int` pattern can never match a
            // `(string) -> int` value, so they must not be reported as
            // overlapping for union dispatch.
            (
                Ty::Function {
                    params: a_params,
                    ret: a_ret,
                    ..
                },
                Ty::Function {
                    params: b_params,
                    ret: b_ret,
                    ..
                },
            ) => {
                a_params.len() == b_params.len()
                    && a_params
                        .iter()
                        .zip(b_params.iter())
                        .all(|(x, y)| self.types_overlap(&x.ty, &y.ty))
                    && self.types_overlap(a_ret, b_ret)
            }
            // Never has no values, so it doesn't overlap with anything.
            (Ty::Never { .. }, _) | (_, Ty::Never { .. }) => false,
            // Self-overlap for atoms that aren't covered above. These have a
            // single inhabited "kind" that's identified by discriminant —
            // `type` (the type-of-types), `void`, builtin-unknown sentinel,
            // etc. Without these, e.g. `Optional<type>` looks like it
            // overlaps only its `null` branch, breaking refutability checks
            // for `let t: type? = null`.
            (Ty::Type { .. }, Ty::Type { .. })
            | (Ty::Void { .. }, Ty::Void { .. })
            | (Ty::BuiltinUnknown { .. }, Ty::BuiltinUnknown { .. })
            | (Ty::RustType { .. }, Ty::RustType { .. }) => true,
            // Anything else: distinct heads.
            _ => false,
        }
    }

    /// The pattern's natural type — what it would match without a
    /// surrounding scrutinee context.
    ///
    /// `unconstrained` is the `Ty` substituted for leaves with no type
    /// information (bare `Wildcard`, `Bind` without subpat, empty
    /// `Array`):
    /// - Pass `Ty::Unknown` (the "matches anything" sentinel) for the
    ///   union dispatcher, where unconstrained patterns target every
    ///   member.
    /// - Pass `Ty::Never` (the bottom type) for the chain-widening and
    ///   pattern-vs-scrut subtype checks, where unconstrained patterns
    ///   should trivially widen / be a subtype of anything.
    ///
    /// Conventions:
    /// - `Type(t)` ⇒ resolved `t`
    /// - `Class { name, generic_args, .. }` ⇒ `Class<args>`, with
    ///   `Unknown` for any unspecified `T`
    /// - `Array { prefix, rest, suffix }` ⇒ `List<elem>` where `elem` is
    ///   the join of each sub-position's natural type (`unconstrained`
    ///   if all unconstrained); `: T` ascription wins
    /// - `Or(parts)` ⇒ join of each part's natural type
    fn pattern_natural_type(&mut self, pat_id: PatId, body: &ExprBody, unconstrained: &Ty) -> Ty {
        let kind = match unconstrained {
            Ty::Never { .. } => NaturalKind::Never,
            _ => NaturalKind::Unknown,
        };
        if let Some(cached) = self.pattern_natural_cache.get(&(pat_id, kind)) {
            return cached.clone();
        }
        let result = match &body.patterns[pat_id].clone() {
            ast::Pattern::Wildcard => unconstrained.clone(),
            // `let x` → unconstrained; `let x: <pattern>` → recurse.
            ast::Pattern::Bind { subpat, .. } => match subpat {
                Some(sp) => self.pattern_natural_type(*sp, body, unconstrained),
                None => unconstrained.clone(),
            },
            ast::Pattern::Type(t) => self.resolve_type_expr_silent(t),
            ast::Pattern::Class {
                class,
                generic_args,
                ..
            } => self.resolve_class_pattern_type(class, generic_args, None),
            ast::Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            } => {
                if let Some(t) = ascription {
                    self.resolve_type_expr_silent(t)
                } else {
                    let mut elem_tys: Vec<Ty> = prefix
                        .iter()
                        .chain(suffix.iter())
                        .map(|&p| self.pattern_natural_type(p, body, unconstrained))
                        .collect();
                    if let Some(rp) = rest
                        && let Some(rest_pat) = rp.pat
                        && let ast::Pattern::Array { .. } = &body.patterns[rest_pat]
                    {
                        let rest_natural = self.pattern_natural_type(rest_pat, body, unconstrained);
                        if let Ty::List(inner, _) | Ty::EvolvingList(inner, _) = rest_natural {
                            elem_tys.push(*inner);
                        }
                    }
                    // Drop unconstrained (`Unknown`/`Never`) contributions
                    // from the elem-type join when at least one position
                    // is concrete. A wildcard or bare bind doesn't
                    // constrain the element type, so it shouldn't dilute
                    // the join — otherwise `[_, [...let v: int], ..]`
                    // ends up with elem `Union<Unknown, List<int>>`,
                    // making the outer pattern look like it overlaps any
                    // list (`Unknown` bypasses `atoms_overlap`) and wrongly
                    // targets unrelated union members.
                    let any_concrete = elem_tys
                        .iter()
                        .any(|t| !matches!(t, Ty::Unknown { .. } | Ty::Never { .. }));
                    if any_concrete {
                        elem_tys.retain(|t| !matches!(t, Ty::Unknown { .. } | Ty::Never { .. }));
                    }
                    let elem = if elem_tys.is_empty() {
                        unconstrained.clone()
                    } else {
                        Self::join_all(&elem_tys)
                    };
                    Ty::List(Box::new(elem), TyAttr::default())
                }
            }
            ast::Pattern::Or(parts) => {
                let part_tys: Vec<Ty> = parts
                    .iter()
                    .map(|&p| self.pattern_natural_type(p, body, unconstrained))
                    .collect();
                Self::join_all(&part_tys)
            }
        };
        self.pattern_natural_cache
            .insert((pat_id, kind), result.clone());
        result
    }

    /// Inner dispatch (no union-member wrapping). The wrapper
    /// `analyze_and_lower_no_subtype_check` records `pattern_types[pat_id]`
    /// after this returns, so callers do not need to.
    fn analyze_and_lower_inner(
        &mut self,
        pat_id: PatId,
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        match &body.patterns[pat_id].clone() {
            ast::Pattern::Wildcard => Self::lower_wildcard_pat(scrut_ty),
            ast::Pattern::Bind { name, subpat } => {
                self.lower_bind_pat(pat_id, name.clone(), *subpat, scrut_ty, body, at_expr)
            }
            ast::Pattern::Type(t) => self.lower_type_pat(t, pat_id, scrut_ty, at_expr),
            ast::Pattern::Class {
                class,
                generic_args,
                fields,
            } => self.lower_class_pat(class, generic_args, fields, pat_id, scrut_ty, body, at_expr),
            ast::Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            } => self.lower_array_pat(
                prefix,
                rest.as_ref(),
                suffix,
                ascription.as_ref(),
                scrut_ty,
                body,
                at_expr,
            ),
            ast::Pattern::Or(parts) => self.lower_or_pat(parts, scrut_ty, body, at_expr),
        }
    }

    fn lower_wildcard_pat(scrut_ty: &Ty) -> crate::pattern_lowering::PatternResult {
        crate::pattern_lowering::PatternResult {
            dpat: crate::exhaustiveness::DPat::wildcard(scrut_ty.clone()),
            required_ty: None,
            matched_ty: scrut_ty.clone(),
            bindings: Vec::new(),
        }
    }

    fn lower_bind_pat(
        &mut self,
        pat_id: PatId,
        name: Name,
        subpat: Option<PatId>,
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        // Bare bind without sub-pattern: matches anything, binds at scrut.
        let Some(sp) = subpat else {
            return crate::pattern_lowering::PatternResult {
                dpat: crate::exhaustiveness::DPat::wildcard(scrut_ty.clone()),
                required_ty: None,
                matched_ty: scrut_ty.clone(),
                bindings: vec![crate::pattern_lowering::PatternBinding {
                    name,
                    pat_id,
                    ty: scrut_ty.clone(),
                }],
            };
        };
        // `let x: <pattern>` — lower the sub-pattern (which can be a type
        // ascription, another binding, an array/class destructure, etc.),
        // then prepend the outer binding at the sub-pattern's narrowed type.
        // Skip the per-subpat strict subtype check: the parent
        // analyze_and_lower already checked the bind's natural type
        // (which is the subpat's natural type per `pattern_natural_type`)
        // against `scrut_ty`, so re-running it would double-diagnose
        // match arms and would also break catch arms that bypass the
        // outer check intentionally.
        let inner = self.analyze_and_lower_no_subtype_check(sp, scrut_ty, body, at_expr);
        let mut bindings = vec![crate::pattern_lowering::PatternBinding {
            name,
            pat_id,
            ty: inner.matched_ty.clone(),
        }];
        bindings.extend(inner.bindings);
        crate::pattern_lowering::PatternResult {
            dpat: inner.dpat,
            required_ty: inner.required_ty,
            matched_ty: inner.matched_ty,
            bindings,
        }
    }

    fn lower_type_pat(
        &mut self,
        ty_expr: &TypeExpr,
        pat_id: PatId,
        scrut_ty: &Ty,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        // Anchor "unresolved type" / "type mismatch" diagnostics at the
        // pattern's own span rather than the surrounding expression so the
        // squiggle lands on the type name (e.g. `Frobnitz`), not the
        // scrutinee.
        let resolved = self.resolve_type_expr_at_pat(ty_expr, pat_id, at_expr);
        let dpat = self.dpat_for_type(&resolved, scrut_ty);
        let matched = self.intersect_pattern_flow_types(scrut_ty, &resolved);
        crate::pattern_lowering::PatternResult {
            dpat,
            required_ty: Some(resolved),
            matched_ty: matched,
            bindings: Vec::new(),
        }
    }

    /// Build a `DPat` that matches every value of `t`. Used by `Type(t)`
    /// patterns. Four regimes:
    ///
    /// - Singleton types (`Literal`, `EnumVariant`, `null`) →
    ///   `DPat::single(t, scrut_ty)`.
    /// - Finite-alphabet types (`bool`, `Enum`, finite literal unions,
    ///   `Optional<T>`, unions of finites) → `DPat::or` of the
    ///   singletons; the algorithm explodes Or rows during specialization.
    /// - Class types → `DPat::class(qtn, [Wildcard for each field])`.
    /// - Opaque alphabets (raw int/string/float, generics, lists, maps,
    ///   functions) → `DPat::wildcard`.
    ///
    /// Callers are expected to project union scrutinees onto a single
    /// member before calling this (`analyze_and_lower` does that via
    /// `Ctor::UnionMember`), so `scrut_ty` here is the column type at the
    /// matched branch.
    fn dpat_for_type(&self, t: &Ty, scrut_ty: &Ty) -> crate::exhaustiveness::DPat {
        use crate::exhaustiveness::DPat;
        let expanded = self.expand_alias_chains(t.clone());
        match &expanded {
            // Singletons.
            Ty::Literal(_, _, _) | Ty::EnumVariant(_, _, _) => {
                DPat::single(expanded.clone(), scrut_ty.clone())
            }
            Ty::Primitive(PrimitiveType::Null, _) => {
                DPat::single(expanded.clone(), scrut_ty.clone())
            }
            // Finite enumerations: build an Or of singletons.
            Ty::Primitive(PrimitiveType::Bool, _) => Self::or_of_singletons(
                vec![
                    Ty::Literal(
                        baml_base::Literal::Bool(true),
                        Freshness::Regular,
                        TyAttr::default(),
                    ),
                    Ty::Literal(
                        baml_base::Literal::Bool(false),
                        Freshness::Regular,
                        TyAttr::default(),
                    ),
                ],
                scrut_ty,
            ),
            Ty::Enum(qtn, _) => {
                let variants: Vec<Ty> = self
                    .lookup_enum_variants(qtn)
                    .into_iter()
                    .map(|v| Ty::EnumVariant(qtn.clone(), v, TyAttr::default()))
                    .collect();
                Self::or_of_singletons(variants, scrut_ty)
            }
            Ty::Optional(inner, _) => {
                let inner_dpat = self.dpat_for_type(inner, scrut_ty);
                let null_dpat = DPat::single(
                    Ty::Primitive(PrimitiveType::Null, TyAttr::default()),
                    scrut_ty.clone(),
                );
                Self::or_combine(vec![inner_dpat, null_dpat], scrut_ty)
            }
            Ty::Union(members, _) => {
                let alts: Vec<DPat> = members
                    .iter()
                    .map(|m| self.dpat_for_type(m, scrut_ty))
                    .collect();
                Self::or_combine(alts, scrut_ty)
            }
            // Classes: structural ctor with all fields wildcarded.
            Ty::Class(qtn, args, _) => {
                let field_tys = self.class_field_types_ordered(qtn, args);
                let fields = field_tys.into_iter().map(DPat::wildcard).collect();
                DPat::class(qtn.clone(), fields, scrut_ty.clone())
            }
            // Opaque alphabets — best-effort: wildcard. Imprecise when
            // the scrutinee is a union containing this type plus other
            // members; documented above.
            _ => DPat::wildcard(scrut_ty.clone()),
        }
    }

    /// Build a `DPat::or(...)` of `Single(ty)` ctors over a list of
    /// types. Single-element lists collapse to a plain `DPat::single`.
    fn or_of_singletons(tys: Vec<Ty>, scrut_ty: &Ty) -> crate::exhaustiveness::DPat {
        use crate::exhaustiveness::DPat;
        match tys.len() {
            0 => DPat::wildcard(scrut_ty.clone()),
            1 => DPat::single(tys.into_iter().next().unwrap(), scrut_ty.clone()),
            _ => {
                let alts: Vec<DPat> = tys
                    .into_iter()
                    .map(|t| DPat::single(t, scrut_ty.clone()))
                    .collect();
                DPat::or(alts, scrut_ty.clone())
            }
        }
    }

    /// Combine a set of `DPats` into one. Single → as-is; multiple → Or;
    /// empty → wildcard.
    fn or_combine(
        alts: Vec<crate::exhaustiveness::DPat>,
        scrut_ty: &Ty,
    ) -> crate::exhaustiveness::DPat {
        use crate::exhaustiveness::DPat;
        match alts.len() {
            0 => DPat::wildcard(scrut_ty.clone()),
            1 => alts.into_iter().next().unwrap(),
            _ => DPat::or(alts, scrut_ty.clone()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_class_pat(
        &mut self,
        class: &[Name],
        generic_args: &[TypeExpr],
        fields: &[ast::FieldPat],
        pat_id: PatId,
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        use crate::{
            exhaustiveness::DPat,
            pattern_lowering::{PatternBinding, PatternResult},
        };

        // Anchor unresolved-name / type-mismatch diagnostics for the class
        // head and its generic args at the pattern's span (same treatment
        // as `Pattern::Type` in `lower_type_pat`). `at_expr` stays in the
        // tuple as a fallback for `report_at_pat_or_expr`.
        let class_ty =
            self.resolve_class_pattern_type(class, generic_args, Some((pat_id, at_expr)));
        if !matches!(class_ty, Ty::Class(..)) {
            // Resolution failed; bail out with a wildcard so downstream
            // can keep going. Diagnostics already emitted by resolver.
            return PatternResult {
                dpat: DPat::wildcard(scrut_ty.clone()),
                required_ty: Some(class_ty.clone()),
                matched_ty: class_ty,
                bindings: Vec::new(),
            };
        }
        // Missing generic args on a destructure of a generic class:
        // always require them, even for empty destructures. The
        // pattern's natural type is otherwise `Class<Unknown, ...>`,
        // which can't pick a unique union member when the scrutinee is
        // `Box<int> | Box<string>`. Emit the diagnostic but continue
        // lowering so field bindings still get registered — otherwise
        // body references to those bindings fail with "unresolved
        // name" instead of the specific generic-args error.
        if self.class_pattern_missing_generic_args(&class_ty, generic_args) {
            let err = TirTypeError::GenericClassDestructureRequiresTypeArgs {
                class_name: class.last().cloned().unwrap_or_else(|| Name::new("_")),
            };
            self.context.report_simple(err, at_expr);
        }

        let (qtn, args) = match &class_ty {
            Ty::Class(qtn, args, _) => (qtn.clone(), args.clone()),
            _ => unreachable!("class_ty is Class by check above"),
        };

        // Build a name → source-FieldPat lookup so we can walk in
        // declaration order.
        let mut by_name: FxHashMap<Name, &ast::FieldPat> = FxHashMap::default();
        for fp in fields {
            by_name.insert(fp.field.clone(), fp);
        }

        // Walk the class definition's fields in declaration order. One
        // item-tree walk gives both names and types — see
        // `class_field_infos_ordered`.
        let field_infos = self.class_field_infos_ordered(&qtn, &args);

        let mut sub_dpats: Vec<DPat> = Vec::with_capacity(field_infos.len());
        let mut bindings: Vec<PatternBinding> = Vec::new();
        for (field_name, field_ty) in field_infos {
            match by_name.get(&field_name) {
                Some(fp) => {
                    let r = self.analyze_and_lower(fp.pat, &field_ty, body, at_expr);
                    sub_dpats.push(r.dpat);
                    bindings.extend(r.bindings);
                }
                None => {
                    // Elided field: implicitly wildcard.
                    sub_dpats.push(DPat::wildcard(field_ty));
                }
            }
        }

        PatternResult {
            dpat: DPat::class(qtn, sub_dpats, scrut_ty.clone()),
            required_ty: Some(class_ty.clone()),
            matched_ty: class_ty,
            bindings,
        }
    }

    /// Render a [`crate::exhaustiveness::WitnessPat`] for the
    /// `non-exhaustive match` diagnostic. Mirrors `WitnessPat`'s `Display`
    /// impl but looks up class field names so witnesses like
    /// `Mixed { value: int }` say which field is which instead of just
    /// `Mixed { int }`.
    fn render_witness_pat(&self, w: &crate::exhaustiveness::WitnessPat) -> String {
        use std::fmt::Write as _;

        use crate::exhaustiveness::Ctor;
        match &w.ctor {
            Ctor::Class(qtn) => {
                let names = self.class_field_names_ordered(qtn);
                if w.fields.is_empty() {
                    return format!("{qtn} {{}}");
                }
                let mut out = format!("{qtn} {{ ");
                for (i, fld) in w.fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let rendered = self.render_witness_pat(fld);
                    if let Some(name) = names.get(i) {
                        let _ = write!(out, "{name}: {rendered}");
                    } else {
                        out.push_str(&rendered);
                    }
                }
                out.push_str(" }");
                out
            }
            // For everything else, fall through to the standard Display
            // impl. UnionMember inner pats cascade through this method too,
            // so a Class nested inside a UnionMember still gets field
            // labels.
            Ctor::UnionMember(_) => match w.fields.first() {
                Some(inner) => {
                    let s = self.render_witness_pat(inner);
                    if matches!(s.as_str(), "_") {
                        // Inner collapsed to placeholder; fall back to the
                        // member type name (`int`, `Foo`, etc.).
                        w.to_string()
                    } else {
                        s
                    }
                }
                None => w.to_string(),
            },
            _ => w.to_string(),
        }
    }

    /// Class field NAMES in declaration order. Companion to
    /// `class_field_types_ordered`.
    fn class_field_names_ordered(&self, qtn: &crate::ty::QualifiedTypeName) -> Vec<Name> {
        let Some(items) = self.resolve_class_pkg_items(qtn.package()) else {
            return Vec::new();
        };
        let Some(Definition::Class(class_loc)) = items.lookup_type(qtn.namespace(), qtn.name())
        else {
            return Vec::new();
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        class_data.fields.iter().map(|f| f.name.clone()).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_array_pat(
        &mut self,
        prefix: &[PatId],
        rest: Option<&ast::ArrayRestPat>,
        suffix: &[PatId],
        ascription: Option<&TypeExpr>,
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        use crate::{
            exhaustiveness::{DPat, SliceShape},
            pattern_lowering::{PatternBinding, PatternResult},
        };

        // If the array carries a `: T` ascription, narrow the scrutinee
        // through the ascribed type before walking elements. Mirrors the
        // narrowing flow used by `Pattern::Bind { ascription }`.
        let ascription_resolved = ascription.map(|t| self.resolve_type_expr(t, at_expr));
        let effective_scrut = match &ascription_resolved {
            Some(ty) => self.intersect_pattern_flow_types(scrut_ty, ty),
            None => scrut_ty.clone(),
        };

        // Determine element type from the (possibly narrowed) scrutinee.
        let elem_ty = match self.expand_alias_chains(effective_scrut.clone()) {
            Ty::List(elem, _) | Ty::EvolvingList(elem, _) => *elem,
            _ => Ty::Unknown {
                attr: TyAttr::default(),
            },
        };

        // Reject any non-trivial rest sub-pattern: only bare `..` is
        // allowed for now. Bindings (`..let r`), structural rests
        // (`..[a, b]`), class destructures (`..Box {}`), and chain
        // ascriptions (`..pat: T`) are all disallowed while we settle
        // the rest-vs-slice typing semantics. The chain widening rule
        // only catches mismatches *between* chain links, not the
        // implicit "rest is a slice" constraint, so partial support
        // would let contradictory annotations slip through silently.
        if let Some(rp) = rest
            && let Some(rest_pat) = rp.pat
        {
            self.report_at_pat_or_expr(TirTypeError::RestSubPatternNotSupported, rest_pat, at_expr);
        }

        // Pre-flatten nested rest sub-patterns: `[a, ..[b, ..c, d], e]`
        // collapses to `[a, b, ..c, d, e]`. baml has no equivalent of this
        // in rustc — rustc's `..` is a position marker and can only carry a
        // binding (`name @ ..`). To stay faithful to rustc's flat slice
        // model in the matrix algorithm we rewrite to flat shape here.
        //
        // Returns (flat_prefix, has_rest, rest_binding_pat, flat_suffix):
        //   - has_rest=false → Fixed shape, no rest binding
        //   - has_rest=true, rest_binding_pat=None → bare `..` (Variable, no binding)
        //   - has_rest=true, rest_binding_pat=Some(p) → `..p` where p is a
        //     non-flattenable sub-pattern (binding/wildcard/etc.)
        let (flat_prefix, has_rest, rest_binding_pat, flat_suffix) =
            Self::flatten_array_rest(body, prefix.to_vec(), rest, suffix.to_vec());

        let mut sub_dpats: Vec<DPat> = Vec::with_capacity(flat_prefix.len() + flat_suffix.len());
        let mut bindings: Vec<PatternBinding> = Vec::new();
        let mut element_required_tys: Vec<Ty> = Vec::new();

        for &p in &flat_prefix {
            let r = self.analyze_and_lower(p, &elem_ty, body, at_expr);
            sub_dpats.push(r.dpat);
            bindings.extend(r.bindings);
            if let Some(req) = r.required_ty {
                element_required_tys.push(req);
            }
        }
        for &p in &flat_suffix {
            let r = self.analyze_and_lower(p, &elem_ty, body, at_expr);
            sub_dpats.push(r.dpat);
            bindings.extend(r.bindings);
            if let Some(req) = r.required_ty {
                element_required_tys.push(req);
            }
        }
        // Rest binding: contributes a binding (typed at List<elem_ty>) but
        // doesn't consume a slot in the slice shape.
        if let Some(rest_pat) = rest_binding_pat {
            let rest_ty = Ty::List(Box::new(elem_ty.clone()), TyAttr::default());
            // The rest sub-pattern matches against the *slice* (a list),
            // not against an element. If the user annotates it with a
            // non-list type (e.g. `..let r: int`, or `..Box { .. }`, or
            // `..[x]: int`), that's a type mismatch — the annotation
            // claims something incompatible with `List<elem>`.
            if let Some(expected) = self.pattern_expected_ty(rest_pat, body)
                && !Self::ty_contains_recovery_unknown(&rest_ty)
                && !crate::generics::contains_typevar(&rest_ty)
                && !self.is_subtype(expected.ty(), &rest_ty)
            {
                let got = expected.into_ty();
                let err = TirTypeError::TypeMismatch {
                    expected: rest_ty.clone(),
                    got,
                };
                self.report_at_pat_or_expr(err, rest_pat, at_expr);
            }
            let r = self.analyze_and_lower(rest_pat, &rest_ty, body, at_expr);
            bindings.extend(r.bindings);
        }

        let shape = if has_rest {
            SliceShape::Variable {
                prefix: flat_prefix.len(),
                suffix: flat_suffix.len(),
            }
        } else {
            SliceShape::Fixed(flat_prefix.len() + flat_suffix.len())
        };

        // Required type: List<join of element required tys>, or List<elem>
        // if no element constrained the type. If the array carries a `: T`
        // ascription, that takes precedence as the required type.
        let required = if let Some(ty) = ascription_resolved {
            Some(ty)
        } else if element_required_tys.is_empty() {
            None
        } else {
            let joined = Self::join_all(&element_required_tys);
            Some(Ty::List(Box::new(joined), TyAttr::default()))
        };

        PatternResult {
            dpat: DPat::slice(shape, sub_dpats, effective_scrut.clone()),
            required_ty: required,
            matched_ty: effective_scrut,
            bindings,
        }
    }

    /// Recursively merge nested rest sub-patterns into a flat slice shape.
    ///
    /// Returns `(flat_prefix, has_rest, rest_binding_pat, flat_suffix)`. See
    /// [`Self::lower_array_pat`] for the encoding.
    ///
    /// When the rest sub-pattern is itself an array (possibly wrapped in a
    /// chain — only the leftmost link is structural), its prefix/suffix get
    /// merged into the outer prefix/suffix and we recurse into its inner
    /// rest. When the rest sub-pattern is anything else (binding, wildcard,
    /// or-pattern, etc.), we keep it as the variable-shape's binding slot.
    fn flatten_array_rest(
        body: &ExprBody,
        prefix: Vec<PatId>,
        rest: Option<&ast::ArrayRestPat>,
        suffix: Vec<PatId>,
    ) -> (Vec<PatId>, bool, Option<PatId>, Vec<PatId>) {
        let Some(rp) = rest else {
            return (prefix, false, None, suffix);
        };
        let Some(rest_pat) = rp.pat else {
            return (prefix, true, None, suffix);
        };

        // Rest sub-patterns are disabled at TIR (we emit
        // `RestSubPatternNotSupported` elsewhere). For error recovery
        // here we simply treat the rest as a bare binding-like slot —
        // no nested array flattening, no chain unwrapping.
        match &body.patterns[rest_pat] {
            ast::Pattern::Array {
                prefix: ip,
                rest: ir,
                suffix: is,
                ascription: _,
            } => {
                let mut new_prefix = prefix;
                new_prefix.extend(ip.iter().copied());
                let mut new_suffix: Vec<PatId> = is.clone();
                new_suffix.extend(suffix);
                let ir_owned = ir.clone();
                Self::flatten_array_rest(body, new_prefix, ir_owned.as_ref(), new_suffix)
            }
            _ => (prefix, true, Some(rest_pat), suffix),
        }
    }

    fn lower_or_pat(
        &mut self,
        parts: &[PatId],
        scrut_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> crate::pattern_lowering::PatternResult {
        use crate::{
            exhaustiveness::DPat,
            pattern_lowering::{PatternBinding, PatternResult},
        };

        let mut alts: Vec<DPat> = Vec::with_capacity(parts.len());
        let mut required_tys: Vec<Ty> = Vec::new();
        let mut matched_tys: Vec<Ty> = Vec::new();
        // Keep ALL per-branch bindings — each PatId is a distinct source
        // location and downstream consumers (LSP/codegen) look up types
        // per PatId via `pattern_types: FxHashMap<PatId, Ty>`. Scope
        // registration collapses by name on its own (`locals` is a map).
        let mut bindings: Vec<PatternBinding> = Vec::new();
        // Group by name solely for the cross-branch type-equality check.
        // HIR already ensures every branch binds the same set of names,
        // so this map's job is purely "do the types match across branches?"
        let mut bindings_by_name: FxHashMap<Name, Vec<(PatId, Ty)>> = FxHashMap::default();

        for &p in parts {
            // Skip the per-branch strict subtype check: the parent
            // analyze_and_lower call already checked the joined natural
            // type (`foo | bar`) against `scrut_ty`, and re-running it
            // per-branch would double-diagnose match arms and would also
            // break catch arms that bypass the outer check intentionally.
            let r = self.analyze_and_lower_no_subtype_check(p, scrut_ty, body, at_expr);
            alts.push(r.dpat);
            for b in r.bindings {
                bindings_by_name
                    .entry(b.name.clone())
                    .or_default()
                    .push((b.pat_id, b.ty.clone()));
                bindings.push(b);
            }
            if let Some(req) = r.required_ty {
                required_tys.push(req);
            }
            matched_tys.push(r.matched_ty);
        }

        // Emit `OrPatternBindingTypeMismatch` per offending branch if a
        // bound name has different types across alternatives.
        self.check_or_binding_type_compatibility(&bindings_by_name, at_expr);

        let required = if required_tys.is_empty() {
            None
        } else {
            Some(Self::join_all(&required_tys))
        };
        let matched = Self::join_all(&matched_tys);

        let dpat = if alts.len() == 1 {
            alts.into_iter().next().unwrap()
        } else {
            DPat::or(alts, scrut_ty.clone())
        };

        PatternResult {
            dpat,
            required_ty: required,
            matched_ty: matched,
            bindings,
        }
    }
}

/// Map a bare type sugar name to a `Ty` for primitive and media types.
///
/// Returns `Some(Ty)` for names like `int`, `string`, `image`, etc.
/// Returns `None` for class/enum names that need full resolution.
fn bare_type_sugar_to_ty(name: &Name) -> Option<Ty> {
    match name.as_str() {
        "int" => Some(Ty::Primitive(PrimitiveType::Int, TyAttr::default())),
        "float" => Some(Ty::Primitive(PrimitiveType::Float, TyAttr::default())),
        "string" => Some(Ty::Primitive(PrimitiveType::String, TyAttr::default())),
        "bool" => Some(Ty::Primitive(PrimitiveType::Bool, TyAttr::default())),
        "null" => Some(Ty::Primitive(PrimitiveType::Null, TyAttr::default())),
        "image" => Some(Ty::Primitive(PrimitiveType::Image, TyAttr::default())),
        "audio" => Some(Ty::Primitive(PrimitiveType::Audio, TyAttr::default())),
        "video" => Some(Ty::Primitive(PrimitiveType::Video, TyAttr::default())),
        "pdf" => Some(Ty::Primitive(PrimitiveType::Pdf, TyAttr::default())),
        _ => None,
    }
}
