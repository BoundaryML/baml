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

use baml_base::{Name, SourceFile};
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
    inference::MemberResolution,
    package_interface::PackageResolutionContext,
    throws_analysis::ThrowsAnalysisContext,
    ty::{Freshness, FunctionParamMode, FunctionParamTy, MediaKind, PrimitiveType, Ty, TyAttr},
};

// ── Well-known type constructors ──────────────────────────────────────────────
//
// These helpers construct `Ty` values for well-known types that appear in
// synthesized method signatures (e.g., the universal `to_json`/`from_json` on
// `Ty::TypeVar`). They are free functions so they can be called from both
// `resolve_member` (mutable context) and `try_resolve_member_on_ty` (shared).

/// Construct `Ty::TypeAlias` for `baml.json.json`.
/// Render an interface instantiation for diagnostics, e.g. `Box` (no args) or
/// `Box<int>`. Used so ambiguity/projection hints name the exact instantiation
/// (`.as<Box<int>>`) rather than the bare interface name.
fn format_interface_display(name: &Name, args: &[Ty]) -> String {
    if args.is_empty() {
        name.to_string()
    } else {
        let rendered: Vec<String> = args.iter().map(std::string::ToString::to_string).collect();
        format!("{name}<{}>", rendered.join(", "))
    }
}

#[derive(Clone, Copy)]
struct InterfaceBindingInputs<'a, 'db> {
    iface_name: &'a crate::ty::QualifiedTypeName,
    iface_data: &'a baml_compiler2_hir::item_tree::Interface,
    iface_type_args: &'a [Ty],
    associated_bindings: &'a [(Name, Ty)],
    pkg_items: &'a PackageItems<'db>,
    iface_ns: &'a [Name],
    receiver_projection_base: Option<&'a Ty>,
    qualify_symbolic_projection: bool,
    prefer_symbolic_projections: bool,
}

#[derive(Clone, Copy)]
struct InterfaceMemberLookup<'a> {
    iface_name: &'a crate::ty::QualifiedTypeName,
    iface_type_args: &'a [Ty],
    associated_bindings: &'a [(Name, Ty)],
    member: &'a Name,
    at: ExprId,
    bound: bool,
    receiver_projection_base: Option<&'a Ty>,
    /// How the receiver pins `Self` (rigid type var / concrete value /
    /// existential `dyn`). Decides object-safety and `Self` substitution.
    self_recv: SelfReceiver<'a>,
}

/// Construct `Ty::Class` for `baml.spawn.SpawnParams<value, error>` (BEP-034
/// middleware: the value a `spawn ... with` pipeline transforms).
fn spawn_params_ty(value: Ty, error: Ty) -> Ty {
    Ty::Class(
        crate::ty::QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("spawn")],
            Name::new("SpawnParams"),
        ),
        vec![value, error],
        TyAttr::default(),
    )
}

/// `true` when `qn` names `baml.spawn.SpawnParams`.
fn is_spawn_params_qtn(qn: &crate::ty::QualifiedTypeName) -> bool {
    qn.package().as_str() == "baml"
        && qn.namespace().len() == 1
        && qn.namespace()[0].as_str() == "spawn"
        && qn.name().as_str() == "SpawnParams"
}

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

fn baml_iter_interface_qtn(name: &str) -> crate::ty::QualifiedTypeName {
    crate::ty::QualifiedTypeName::new(Name::new("baml"), vec![Name::new("iter")], Name::new(name))
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

fn function_generic_param_bounds_exprs(
    db: &dyn crate::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> Vec<Option<TypeExpr>> {
    let item_tree = baml_compiler2_ppir::file_item_tree(db, func_loc.file(db));
    item_tree[func_loc.id(db)].generic_param_bounds.clone()
}

pub(crate) fn lower_generic_param_bounds(
    db: &dyn crate::Db,
    bounds: &[Option<TypeExpr>],
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    generic_params: &[Name],
    bindings: Option<&FxHashMap<Name, Ty>>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Vec<Option<Ty>> {
    bounds
        .iter()
        .map(|bound| {
            bound.as_ref().map(|bound| {
                if let Some(bindings) = bindings {
                    crate::generics::lower_type_expr_with_generics(
                        db,
                        bound,
                        pkg_items,
                        ns_context,
                        bindings,
                        diagnostics,
                    )
                } else {
                    crate::lower_type_expr::lower_type_expr_in_ns(
                        db,
                        bound,
                        pkg_items,
                        ns_context,
                        generic_params,
                        diagnostics,
                    )
                }
            })
        })
        .collect()
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

/// How the receiver of an interface-member resolution pins `Self`.
///
/// Decides whether the object-safety restriction applies and what `Self`-typed
/// parameters resolve to. See [`TypeInferenceBuilder::resolve_interface_member`].
#[derive(Clone, Copy)]
enum SelfReceiver<'a> {
    /// Bare interface ("dyn"/existential) receiver: `Self`-parameter methods are
    /// not callable (object safety).
    Existential,
    /// `Self` is a single rigid type variable — a generic bound `T extends I`, or
    /// `self` inside a default method. Pinned; never inferred from an argument,
    /// checked by identity.
    RigidVar(&'a Name),
    /// `Self` is pinned to the receiver's exact type. This includes concrete
    /// classes/primitives and abstract-but-rigid associated projections such as
    /// `H.Item`.
    ExactTy(&'a Ty),
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
    /// True when the return type contains callee generics that could not be
    /// inferred and were erased for recovery. The call itself already emitted
    /// the relevant inference diagnostic, so callers should not treat `result`
    /// as a concrete type for contextual mismatch reporting.
    recovered_unresolved_generics: bool,
}

type RegistryInterfaceMethodSource = (crate::ty::QualifiedTypeName, Vec<Ty>, Vec<(Name, Ty)>);

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
    /// `true` when the callee is a function *value* (a local/param holding a
    /// function), as opposed to a direct reference to a function/method
    /// declaration. For value callees the callee type's `generic_params`
    /// accurately lists the still-inferable params, so call-site inference is
    /// restricted to them — keeping rigid ambient type vars (e.g. from an
    /// instantiation value `let f = foo<T>`) from being re-inferred. Declaration
    /// callees keep the existing behavior (their `generic_params` is cleared by
    /// receiver/class substitution, so the restriction would be wrong there).
    is_value_call: bool,
    is_optional_call: bool,
    /// Pre-computed type-arg bindings when explicit `<T1, T2, ...>` were written at the call
    /// site. `Some(map)` means the caller already validated arity and resolved each `TypeExpr`;
    /// `None` means use the existing forward/reverse inference paths.
    explicit_type_arg_bindings: Option<FxHashMap<Name, Ty>>,
    /// The callee expression, when one exists. Used to resolve the callee's
    /// declared generic params so the call's final type-arg bindings can be
    /// recorded (in declared order) in `call_type_instantiations` for MIR.
    callee_expr: Option<ExprId>,
    /// Generic params in the order the callee's runtime frame expects them.
    /// For static methods on generic classes this includes owner class params
    /// before method params; for bound methods the receiver seeds owner params,
    /// so this is just the method params.
    runtime_type_arg_params: Vec<Name>,
    /// Pre-bound runtime type args that were substituted out of the callable
    /// type before ordinary call inference ran.
    runtime_type_arg_binding_seed: Vec<(Name, Ty)>,
    /// The rigid `Self` type variable for a Self-pinned interface method call —
    /// argument inference never binds it and the argument is checked against it
    /// by identity (rustc's `ty::Param`). `None` for every ordinary call, which
    /// leaves their inference completely unchanged.
    rigid_self_var: Option<Name>,
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
    /// BEP-044: when this body is the override of an interface method
    /// inside an `implements I { ... }` block, this names `I`. Used to
    /// resolve `default.<method>(...)` calls to `I`'s default body.
    implements_block_interface: Option<crate::ty::QualifiedTypeName>,
    /// Residual throw facts for each catch expression after applying all clauses.
    catch_residual_throws: FxHashMap<ExprId, BTreeSet<Ty>>,
    /// Match expressions that the exhaustiveness checker determined cover all cases.
    exhaustive_matches: FxHashSet<ExprId>,
    /// Generic type parameters in scope for this function (e.g. `["T"]` for
    /// `function foo<T>(...)`). Used when lowering type annotations inside the
    /// function body so that `T` resolves to `Ty::TypeVar("T", TyAttr::default())` rather than
    /// `Ty::Unknown`.
    pub generic_params: Vec<Name>,
    /// Type aliases/bindings visible only while checking this body. Interface
    /// default methods use this for associated type names like `Item` and
    /// `Error`, which must lower to `Self.Item` / `Self.Error` in expression
    /// type positions as well as in signatures.
    pub type_bindings: FxHashMap<Name, Ty>,
    /// BEP-044 generic bounds: `T → bound_ty`. Populated alongside
    /// `generic_params` when a function is declared with `<T extends I>`.
    /// Used by `resolve_member` to expose `I`'s contract on values of
    /// type `T`, and by call-site enforcement when a `T` is replaced by
    /// a concrete type that must satisfy its bound.
    pub generic_param_bounds: rustc_hash::FxHashMap<Name, Ty>,
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
    /// Interface method generic params keyed by the callee expression. Interface
    /// required methods do not have a `FunctionLoc`, so this supplements
    /// `resolutions` for explicit call-site type-arg checking.
    interface_method_generic_params: FxHashMap<ExprId, (Name, Vec<Name>)>,
    /// Concrete owner-interface generic bindings for interface default methods,
    /// keyed by the callee expression. The callable type substitutes these out
    /// of its parameter/return types, but the VM frame still expects owner
    /// params before method params.
    interface_default_owner_type_arg_bindings: FxHashMap<ExprId, Vec<(Name, Ty)>>,
    /// For a Self-pinned interface method call (resolved through a type-variable
    /// receiver — `self` in a default method, or a generic `T extends I`), the
    /// rigid `Self` type variable, keyed by the callee (member-access) expr.
    /// The call site treats it like rustc's `ty::Param`: argument inference
    /// never binds it, and the argument is checked against it by identity. Empty
    /// for every non-Self-pinned call, so ordinary inference is unaffected.
    self_pinned_rigid_var: FxHashMap<ExprId, Name>,
    /// Parameter types for this scope (populated for lambda/function scopes).
    /// Used by LSP to resolve lambda parameter types.
    pub param_types: Vec<(Name, Ty)>,
    /// Full parameter binding plans for checked call expressions.
    pub call_plans: FxHashMap<ExprId, crate::inference::CallPlan>,
    /// Generic instantiation per checked call whose callee declares type
    /// params, in declared De Bruijn order. See
    /// `ScopeInference::call_type_instantiations`.
    pub call_type_instantiations: FxHashMap<ExprId, Vec<Ty>>,
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
    fn baml_iter_iterable_ty() -> Ty {
        Ty::Interface(
            baml_iter_interface_qtn("Iterable"),
            vec![],
            vec![],
            TyAttr::default(),
        )
    }

    fn iterable_view_for_ty(&self, ty: &Ty) -> Option<Ty> {
        self.actual_interface_view_for_formal(&Self::baml_iter_iterable_ty(), ty)
    }

    fn iterable_associated_ty(&self, ty: &Ty, name: &str) -> Option<Ty> {
        let Ty::Interface(_, _, associated_bindings, _) = self.iterable_view_for_ty(ty)? else {
            return None;
        };
        associated_bindings
            .into_iter()
            .find(|(binding_name, _)| binding_name.as_str() == name)
            .map(|(_, ty)| ty)
    }

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

    /// Hard rollback to a previous snapshot — discards EVERYTHING introduced
    /// since the snapshot, including assignments to outer bindings.
    ///
    /// Use this only for branches whose effects are not observable in the
    /// continuation (i.e. diverging branches like `let … else`'s else
    /// block). The normal `restore_scoped_locals` is a join-style merge —
    /// it preserves outer-binding writes for branches that *do* return
    /// control to the surrounding scope (`if`, `if let`, match arms).
    fn discard_scoped_locals(&mut self, snapshot: ScopedLocalsSnapshot) {
        self.locals = snapshot.locals;
        self.scoped_local_declarations
            .truncate(snapshot.scoped_local_declarations_len);
        self.scoped_local_assignments
            .truncate(snapshot.scoped_local_assignments_len);
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
            implements_block_interface: None,
            generic_param_bounds: rustc_hash::FxHashMap::default(),
            catch_residual_throws: FxHashMap::default(),
            exhaustive_matches: FxHashSet::default(),
            generic_params: Vec::new(),
            type_bindings: FxHashMap::default(),
            in_optional_chain: 0,
            path_root_types: FxHashMap::default(),
            path_segment_types: FxHashMap::default(),
            path_member_resolutions: FxHashMap::default(),
            interface_method_generic_params: FxHashMap::default(),
            interface_default_owner_type_arg_bindings: FxHashMap::default(),
            self_pinned_rigid_var: FxHashMap::default(),
            param_types: Vec::new(),
            call_plans: FxHashMap::default(),
            call_type_instantiations: FxHashMap::default(),
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

    pub fn set_type_bindings(&mut self, bindings: FxHashMap<Name, Ty>) {
        self.type_bindings = bindings;
    }

    fn lower_type_expr_in_current_body(
        &self,
        ty_expr: &TypeExpr,
        diags: &mut Vec<TirTypeError>,
    ) -> Ty {
        if self.type_bindings.is_empty() {
            crate::lower_type_expr::lower_type_expr_in_ns(
                self.context.db(),
                ty_expr,
                self.package_items,
                &self.ns_context,
                &self.generic_params,
                diags,
            )
        } else {
            crate::generics::lower_type_expr_with_generics(
                self.context.db(),
                ty_expr,
                self.package_items,
                &self.ns_context,
                &self.type_bindings,
                diags,
            )
        }
    }

    /// BEP-044: register the bound for each generic parameter visible
    /// inside this body. Map keys are the parameter names, values are
    /// the lowered `Ty` of the `extends` clause. Type-vars without an
    /// entry are unbounded.
    pub fn set_generic_param_bounds(&mut self, bounds: rustc_hash::FxHashMap<Name, Ty>) {
        self.generic_param_bounds = bounds;
    }

    /// BEP-044: when this builder is analyzing a method body declared
    /// inside an `implements I { ... }` block, record the interface's
    /// `QualifiedTypeName` so `default.<method>()` expressions in the
    /// body can resolve back to it. `None` for class-level methods, free
    /// functions, and interface default-method bodies (which can also use
    /// `self` but never `default`).
    pub fn set_implements_block_interface(
        &mut self,
        iface_qtn: Option<crate::ty::QualifiedTypeName>,
    ) {
        self.implements_block_interface = iface_qtn;
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
        FxHashMap<ExprId, Vec<Ty>>,
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
            self.call_type_instantiations,
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
    /// Flattens a union so the matrix's `UnionMember` dispatch applies
    /// uniformly (so `int | null | string` keeps a single `null` member),
    /// deduplicating `null` members.
    fn matrix_normalize_scrut(&self, ty: &Ty) -> Ty {
        let expanded = self.expand_alias_chains(ty.clone());
        match expanded {
            Ty::Union(members, attr) => {
                let mut flat: Vec<Ty> = Vec::with_capacity(members.len());
                let mut has_null = false;
                for m in members {
                    match self.expand_alias_chains(m) {
                        Ty::Null { .. } => {
                            has_null = true;
                        }
                        other => flat.push(other),
                    }
                }
                if has_null {
                    flat.push(Ty::Null {
                        attr: TyAttr::default(),
                    });
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
                Ty::Union(members, _) => {
                    let mut function_member = None;
                    for member in members {
                        let expanded_member = builder.expand_alias_chains(member);
                        if matches!(expanded_member, Ty::Null { .. }) {
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
        self.validate_type_generic_bounds_at_span(span, &ty);
        ty
    }

    fn lower_lambda_return_annotation(&mut self, func_def: &ast::FunctionDef) -> Option<Ty> {
        let te = func_def.return_type.as_ref()?;
        let mut all_generic_params = self.generic_params.clone();
        all_generic_params.extend(func_def.generic_params.iter().cloned());
        Some(self.lower_lambda_type_expr(&te.expr, &all_generic_params, te.span))
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

    fn synthetic_effect_param_name(fact: &Ty) -> Option<&Name> {
        match fact {
            Ty::TypeVar(name, _) if crate::ty::is_synthetic_effect_param(name) => Some(name),
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
            if facts.is_empty() {
                return Some(Ty::Never {
                    attr: TyAttr::default(),
                });
            }
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
        if facts.is_empty() {
            return Some(Ty::Never {
                attr: TyAttr::default(),
            });
        }
        Self::ty_from_concrete_facts(&facts)
    }

    fn replace_callable_throws(ty: Ty, concrete_throws: &Ty) -> Ty {
        match ty {
            Ty::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                attr,
                ..
            } => Ty::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                throws: Box::new(concrete_throws.clone()),
                attr,
            },
            Ty::Union(members, attr) => Ty::Union(
                members
                    .into_iter()
                    .map(|member| Self::replace_callable_throws(member, concrete_throws))
                    .collect(),
                attr,
            ),
            other => other,
        }
    }

    fn call_arg_ty_for_generic_inference(&self, expr_id: ExprId, ty: Ty) -> Ty {
        self.callback_concrete_throws_from_expr(expr_id)
            .map(|throws| Self::replace_callable_throws(ty.clone(), &throws))
            .unwrap_or(ty)
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

        // Throws tracking is otherwise *exact* (so e.g. enum-variant throws stay
        // precise — `throws Errors` does not absorb `throws Errors.AuthError`).
        // BEP-044 adds exactly one widening: a thrown class is covered by a
        // declared *interface* it nominally implements. We therefore keep the
        // original exact set difference and only additionally exempt
        // interface-implementor coverage.
        let covered_by_declared = |eff: &Ty| {
            declared.contains(eff)
                || declared
                    .iter()
                    .any(|decl| matches!(decl, Ty::Interface(..)) && self.is_subtype(eff, decl))
        };
        let extra_facts: BTreeSet<Ty> = if has_open_slot {
            BTreeSet::new()
        } else {
            effective
                .iter()
                .filter(|eff| !covered_by_declared(eff))
                .cloned()
                .collect()
        };
        let mut extra: Vec<String> = extra_facts
            .iter()
            .map(crate::ty::Ty::render_user_facing)
            .collect();
        let mut extraneous: Vec<String> = if warn_extraneous && !has_open_slot {
            // A declared fact is extraneous when nothing thrown matches it
            // exactly — except a declared interface that some thrown class
            // implements is genuinely used (so `throws IError` with a thrown
            // `NetworkError` is not reported as unused).
            declared
                .iter()
                .filter(|decl| {
                    !(effective.contains(*decl)
                        || matches!(decl, Ty::Interface(..))
                            && effective.iter().any(|eff| self.is_subtype(eff, decl)))
                })
                .map(crate::ty::Ty::render_user_facing)
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
                            concrete_throws.render_user_facing()
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

        // The arms below bridge between the builtin `Class<Array, [T]>` /
        // `Class<Map, [K, V]>` wrapper types and their raw `List<T>` /
        // `Map<K, V>` shapes, recursing structurally on the element types.
        // `List`/`Map` are invariant, so an `int[]` argument does not satisfy
        // an `(int | string)[]` slot.
        match (&expanded_expected, &expanded_got) {
            (Ty::Class(class_name, expected_args, _), Ty::List(actual_inner, _))
                if class_name.is_builtin_root_type("Array") && expected_args.len() == 1 =>
            {
                self.container_arg_subtype_without_nominal(actual_inner, &expected_args[0])
            }
            (Ty::Class(class_name, expected_args, _), Ty::EvolvingList(actual_inner, _))
                if class_name.is_builtin_root_type("Array") && expected_args.len() == 1 =>
            {
                self.container_arg_subtype_without_nominal(actual_inner, &expected_args[0])
            }
            (
                Ty::Class(class_name, expected_args, _),
                Ty::Map {
                    key: actual_key,
                    value: actual_val,
                    ..
                }
                | Ty::EvolvingMap(actual_key, actual_val, _),
            ) if class_name.is_builtin_root_type("Map") && expected_args.len() == 2 => {
                self.container_arg_subtype_without_nominal(actual_key, &expected_args[0])
                    && self.container_arg_subtype_without_nominal(actual_val, &expected_args[1])
            }
            _ => false,
        }
    }

    fn container_arg_subtype_without_nominal(&self, actual: &Ty, expected: &Ty) -> bool {
        crate::normalize::is_subtype_of(actual, expected, &self.aliases)
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

    /// The callee's declared generic-param names in De Bruijn order
    /// (`[class params...] ++ [user fn params...]`), plus the callee's name
    /// for diagnostics. Resolved from the callee expression's recorded
    /// `MemberResolution`; `None` when the callee is not a declared function
    /// (lambda values, unresolved callees).
    fn callee_declared_generic_params(&self, callee_id: ExprId) -> Option<(Vec<Name>, Name)> {
        let Some(resolution) = self.resolutions.get(&callee_id).cloned() else {
            // Interface methods aren't in `resolutions`; their declared generic
            // params are recorded separately during interface checking.
            let (callee_name, declared_params) = self
                .interface_method_generic_params
                .get(&callee_id)
                .cloned()?;
            return Some((declared_params, callee_name));
        };
        let (func_loc, treat_as_static_method) = match resolution {
            crate::inference::MemberResolution::Free { func_loc } => (func_loc, true),
            // `UnboundMethod` covers `Class.method` / `Class<...>.method` call
            // sites where the receiver is a type name.  When the call writes
            // `Class<...>.method(...)`, the receiver-type's `<...>` is parsed
            // as the call's type-args by `find_callee_generic_args` in
            // `lower_expr_body.rs`; those args fill the *enclosing class's*
            // generic params (BEP-039), so we include them in the declared
            // list.
            crate::inference::MemberResolution::UnboundMethod { func_loc, .. } => (func_loc, true),
            // BoundMethod calls (`inst.method(args)`) get class type-args
            // from the receiver instance's `class_type_args` at runtime, not
            // from the call site.
            crate::inference::MemberResolution::BoundMethod { func_loc, .. } => (func_loc, false),
            crate::inference::MemberResolution::InterfaceDefaultMethod { func_loc, .. } => {
                (func_loc, false)
            }
            _ => return None,
        };
        let db = self.context.db();
        let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
        // Only user-declared generic params are supplied at the call site;
        // synthetic effect params are always inferred.  For
        // static-method-on-generic-class calls, prepend the class's generic
        // params: type-args fill `[class_params..., function_params...]`.
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
        Some((declared_params, sig.name.clone()))
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
    /// Infer `foo<int>` — a generic callable referenced with explicit type args
    /// but NOT called (`Expr::GenericApply`). Produces the specialized function
    /// type with the type params bound and **cleared** to `[]`, so the value is
    /// a concrete function: a later call checks args against the substituted
    /// param types (e.g. `let f = foo<int>; f("s")` is a type error).
    fn infer_generic_apply(
        &mut self,
        base: ExprId,
        type_args: &[TypeExpr],
        body: &ExprBody,
        expr_id: ExprId,
    ) -> Ty {
        let base_ty = self.infer_expr(base, body);
        // If the base already failed to type-check, propagate its
        // `Unknown`/`Error` instead of cascading a second `TypeIsNotGeneric`
        // diagnostic (mirrors the normal call path).
        if matches!(base_ty, Ty::Unknown { .. } | Ty::Error { .. }) {
            return base_ty;
        }
        let Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } = base_ty
        else {
            // Type args applied to something that is not a generic callable.
            self.context.report_simple(
                TirTypeError::TypeIsNotGeneric {
                    type_name: Self::generic_apply_base_name(base, body),
                    kind: "value",
                },
                expr_id,
            );
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        };

        if type_args.len() != generic_params.len() {
            self.context.report_simple(
                TirTypeError::WrongTypeArgArity {
                    callee_name: Self::generic_apply_base_name(base, body),
                    expected: generic_params.len(),
                    got: type_args.len(),
                },
                expr_id,
            );
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }

        // Resolve each explicit type argument in the current namespace.
        let db = self.context.db();
        let ns = self.ns_context.clone();
        let caller_generic_params = self.generic_params.clone();
        let mut resolved: Vec<Ty> = Vec::with_capacity(type_args.len());
        for type_arg_expr in type_args {
            let mut diags = Vec::new();
            let ty = if self.type_bindings.is_empty() {
                crate::lower_type_expr::lower_type_expr_in_ns(
                    db,
                    type_arg_expr,
                    self.package_items,
                    &ns,
                    &caller_generic_params,
                    &mut diags,
                )
            } else {
                crate::generics::lower_type_expr_with_generics(
                    db,
                    type_arg_expr,
                    self.package_items,
                    &ns,
                    &self.type_bindings,
                    &mut diags,
                )
            };
            for d in diags {
                self.context.report_simple(d, expr_id);
            }
            resolved.push(ty);
        }

        let bindings = crate::generics::bind_type_vars(&generic_params, &resolved);

        // BEP-044 generic-bound enforcement: each supplied type arg must satisfy
        // its param's bound (mirrors the call-site check in
        // `resolve_explicit_type_args`). The bounds are already lowered on
        // `base_ty`; substitute the bindings so self-referential bounds
        // (`<T: Container<T>>`) resolve before the subtype check.
        for (idx, resolved_arg) in resolved.iter().enumerate() {
            if let Some(Some(bound)) = generic_param_bounds.get(idx) {
                let bound_ty = crate::generics::substitute_ty(bound, &bindings);
                if !self.is_subtype(resolved_arg, &bound_ty) {
                    self.context.report_simple(
                        TirTypeError::TypeMismatch {
                            expected: bound_ty,
                            got: resolved_arg.clone(),
                        },
                        expr_id,
                    );
                }
            }
        }

        // Build the specialized signature. Substitute the bound params into
        // each param/ret/throws and clear `generic_params` so the result is a
        // concrete (non-generic) function value.
        Ty::Function {
            generic_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            params: params
                .iter()
                .map(|param| FunctionParamTy {
                    name: param.name.clone(),
                    ty: crate::generics::substitute_ty(&param.ty, &bindings),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(crate::generics::substitute_ty(&ret, &bindings)),
            throws: Box::new(crate::generics::substitute_ty(&throws, &bindings)),
            attr: TyAttr::default(),
        }
    }

    /// Best-effort display name for a `GenericApply` base, for diagnostics.
    fn generic_apply_base_name(base: ExprId, body: &ExprBody) -> Name {
        match &body.exprs[base] {
            Expr::Path(segments) => segments
                .last()
                .cloned()
                .unwrap_or_else(|| Name::new("<value>")),
            _ => Name::new("<value>"),
        }
    }

    fn resolve_explicit_type_args(
        &mut self,
        callee_id: ExprId,
        type_args: &[TypeExpr],
        call_expr_id: ExprId,
    ) -> Option<FxHashMap<Name, Ty>> {
        let (declared_params, callee_name) = self.callee_declared_generic_params(callee_id)?;

        if type_args.len() != declared_params.len() {
            self.context.report_simple(
                TirTypeError::WrongTypeArgArity {
                    callee_name,
                    expected: declared_params.len(),
                    got: type_args.len(),
                },
                call_expr_id,
            );
            return None;
        }

        // Resolve each type argument in the current namespace context.
        let db = self.context.db();
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
                type_args: Vec::new(),
            },
        );

        pairs
    }

    fn runtime_call_type_args(
        &self,
        generic_params: &[Name],
        bindings: &FxHashMap<Name, Ty>,
    ) -> Vec<Ty> {
        generic_params
            .iter()
            .map(|param| {
                // Widen FRESH literal types out of inferred bindings: `mk("hi")`
                // must thread `T = string`, not `T = literal "hi"` — the runtime
                // compares class type args invariantly, so an escaped instance
                // carrying the literal would never match `is Box<string>`.
                // (Explicit type args never reach here; recording is gated on
                // `!explicit_args_used`.)
                let ty = bindings
                    .get(param)
                    .cloned()
                    .map(Ty::widen_fresh)
                    .unwrap_or_else(|| Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    });
                let resolved = self.resolve_associated_projections_deep(&ty);
                if crate::generics::contains_typevar_where(&resolved, &|name| {
                    !self.generic_params.iter().any(|param| param == name)
                }) {
                    Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    }
                } else {
                    resolved
                }
            })
            .collect()
    }

    fn callee_member_resolution(
        &self,
        callee_id: ExprId,
    ) -> Option<crate::inference::MemberResolution<'db>> {
        self.resolutions.get(&callee_id).cloned().or_else(|| {
            self.path_member_resolutions
                .get(&callee_id)
                .and_then(|resolutions| resolutions.last().cloned())
        })
    }

    fn callee_frame_generic_params(
        &self,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) -> (Vec<Name>, Vec<Name>) {
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, func_loc.file(db));
        let func_id = func_loc.id(db);
        if let Some(imp) = item_tree
            .implements_for
            .iter()
            .find(|imp| imp.methods.contains(&func_id))
        {
            return (
                imp.generic_params.clone(),
                item_tree[func_id].generic_params.clone(),
            );
        }

        let fn_params = item_tree[func_id].generic_params.clone();
        if let Some(iface_data) = item_tree
            .interfaces
            .values()
            .find(|iface_data| iface_data.default_methods.contains(&func_id))
        {
            return (iface_data.generic_params.clone(), fn_params);
        }

        let owner_params = item_tree
            .classes
            .values()
            .find(|class_data| class_data.methods.contains(&func_id))
            .map(|class_data| class_data.generic_params.clone())
            .unwrap_or_default();
        (owner_params, fn_params)
    }

    fn runtime_type_arg_params_for_call(
        &self,
        callee_id: ExprId,
        callee_generic_params: &[Name],
        _is_method_call: bool,
        is_value_call: bool,
    ) -> Vec<Name> {
        if is_value_call {
            return callee_generic_params.to_vec();
        }

        let Some(resolution) = self.callee_member_resolution(callee_id) else {
            return callee_generic_params.to_vec();
        };

        let func_loc = match resolution {
            MemberResolution::Free { func_loc }
            | MemberResolution::BoundMethod { func_loc, .. }
            | MemberResolution::UnboundMethod { func_loc, .. }
            | MemberResolution::InterfaceDefaultMethod { func_loc, .. } => func_loc,
            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                return callee_generic_params.to_vec();
            }
        };

        let (owner_params, fn_params) = self.callee_frame_generic_params(func_loc);
        match resolution {
            MemberResolution::Free { .. } | MemberResolution::UnboundMethod { .. } => {
                owner_params.into_iter().chain(fn_params).collect()
            }
            MemberResolution::BoundMethod { .. } => fn_params,
            MemberResolution::InterfaceDefaultMethod { .. } => {
                owner_params.into_iter().chain(fn_params).collect()
            }
            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                callee_generic_params.to_vec()
            }
        }
    }

    fn record_call_type_args(&mut self, expr_id: ExprId, type_args: Vec<Ty>) {
        if type_args.is_empty() {
            return;
        }
        self.call_plans
            .entry(expr_id)
            .and_modify(|plan| plan.type_args.clone_from(&type_args))
            .or_insert_with(|| crate::inference::CallPlan {
                bindings: Vec::new(),
                type_args,
            });
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
        let saved_interface_method_generic_params =
            std::mem::take(&mut self.interface_method_generic_params);
        let saved_interface_default_owner_type_arg_bindings =
            std::mem::take(&mut self.interface_default_owner_type_arg_bindings);
        let saved_self_pinned_rigid_var = std::mem::take(&mut self.self_pinned_rigid_var);
        let saved_call_plans = std::mem::take(&mut self.call_plans);
        let saved_call_type_instantiations = std::mem::take(&mut self.call_type_instantiations);
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
            call_type_instantiations: std::mem::take(&mut self.call_type_instantiations),
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
        self.interface_method_generic_params = saved_interface_method_generic_params;
        self.interface_default_owner_type_arg_bindings =
            saved_interface_default_owner_type_arg_bindings;
        self.self_pinned_rigid_var = saved_self_pinned_rigid_var;
        self.call_plans = saved_call_plans;
        self.call_type_instantiations = saved_call_type_instantiations;
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
            Expr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            } => {
                Self::collect_default_expr_forward_references(
                    *scrutinee,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                let saved_len = shadowed.len();
                Self::push_pattern_bindings(*pattern, body, shadowed);
                Self::collect_default_expr_forward_references(
                    *then_branch,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                shadowed.truncate(saved_len);
                if let Some(else_branch) = else_branch {
                    Self::collect_default_expr_forward_references(
                        *else_branch,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
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
            | Expr::Upcast { base, .. }
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
                with_exprs,
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
                for with_id in with_exprs {
                    Self::collect_default_expr_forward_references(
                        *with_id,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                // The spawn BODY is deferred (wrapped in a synthetic lambda
                // and evaluated on the spawned task) — only `name` and the
                // `with` transformers are evaluated eagerly, so a default
                // capturing a later parameter inside `spawn { ... }` is fine.
                let _ = spawn_body;
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
            Expr::GenericApply { base, .. } => {
                Self::collect_default_expr_forward_references(
                    *base,
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
                else_branch,
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
                if let Some(else_expr) = else_branch {
                    // The else branch runs before the pattern's bindings
                    // exist, so it can't see them — recurse with the
                    // pre-binding `shadowed` set, then re-truncate after.
                    let saved_len = shadowed.len();
                    Self::collect_default_expr_forward_references(
                        *else_expr,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    shadowed.truncate(saved_len);
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
            Stmt::WhileLet {
                pattern,
                scrutinee,
                body: loop_body,
            } => {
                // Scrutinee is evaluated outside the pattern's binding scope;
                // the pattern's names shadow within the body only — mirrors
                // `Stmt::For` (collection then pattern then body).
                Self::collect_default_expr_forward_references(
                    *scrutinee,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                let saved_len = shadowed.len();
                Self::push_pattern_bindings(*pattern, body, shadowed);
                Self::collect_default_expr_forward_references(
                    *loop_body,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
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
            is_value_call,
            is_optional_call,
            explicit_type_arg_bindings,
            callee_expr,
            runtime_type_arg_params,
            runtime_type_arg_binding_seed,
            rigid_self_var,
        } = request;
        let explicit_args_used = explicit_type_arg_bindings.is_some();
        let callee_ty = self.expand_alias_chains(callee_ty);

        match &callee_ty {
            Ty::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                ..
            } => {
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
                for (name, ty) in &runtime_type_arg_binding_seed {
                    bindings.entry(name.clone()).or_insert_with(|| ty.clone());
                }

                // Only explicit call-site type args suppress inference. Owner
                // interface args seeded for default methods are already known,
                // but method generics still need normal argument/return inference.
                let run_inference_phases = !explicit_args_used;

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
                        let arg_ty = self.call_arg_ty_for_generic_inference(*arg, arg_ty);
                        self.infer_call_bindings_rigid_self(
                            param_ty,
                            &arg_ty,
                            &mut bindings,
                            rigid_self_var.as_ref(),
                        );
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
                            // Drive bidirectional checking as long as every
                            // callback parameter is concrete *modulo the
                            // enclosing function's own generics*. Those outer
                            // generics are rigid here, so a param like
                            // `Future<T, E>` (T/E being the caller's generics)
                            // still gives an unannotated lambda param a concrete
                            // shape; only a callee inference var the call hasn't
                            // bound yet forces synthesis.
                            let all_params_inferable = fn_params.iter().all(|param| {
                                !crate::generics::contains_non_rigid_typevar(
                                    &param.ty,
                                    &self.generic_params,
                                )
                            });
                            if all_params_inferable {
                                self.check_expr(*arg, body, &substituted)
                            } else {
                                self.infer_expr(*arg, body)
                            }
                        } else {
                            self.infer_expr(*arg, body)
                        };
                        let arg_ty = self.call_arg_ty_for_generic_inference(*arg, arg_ty);
                        self.infer_call_bindings_rigid_self(
                            param_ty,
                            &arg_ty,
                            &mut bindings,
                            rigid_self_var.as_ref(),
                        );
                        if let Expr::Lambda(func_def) = &body.exprs[*arg]
                            && let Some(Ty::Function {
                                ret: formal_ret, ..
                            }) = self.expected_lambda_function_ty(param_ty)
                            && let Some(actual_ret) = self.lower_lambda_return_annotation(func_def)
                        {
                            self.infer_call_bindings_rigid_self(
                                &formal_ret,
                                &actual_ret,
                                &mut bindings,
                                rigid_self_var.as_ref(),
                            );
                        }
                    }

                    // Backfill: a callee generic matched ONLY against the
                    // caller's own rigid TypeVars (e.g. `map`'s `U`/`E` against
                    // the enclosing `all<T, E>`'s `T`/`E` in
                    // `futures.map((f) -> { await f })`) is bound to those
                    // TypeVars. Plain inference skips TypeVar actuals — fresh
                    // inference vars carry no information — but a rigid caller
                    // generic is as concrete as any type inside this body, and
                    // leaving the callee generic unbound makes the call's
                    // result type fail strict rigid-var checking at its use
                    // site (`U[]` ≢ `T[]`).
                    let unbound: Vec<&FunctionParamTy> = param_arg_pairs
                        .iter()
                        .map(|(param, _)| *param)
                        .filter(|param| {
                            crate::generics::contains_typevar(&crate::generics::substitute_ty(
                                &param.ty, &bindings,
                            ))
                        })
                        .collect();
                    if !unbound.is_empty() {
                        let mut typevar_bindings: FxHashMap<Name, Ty> = FxHashMap::default();
                        for (param, arg) in &param_arg_pairs {
                            if let Some(arg_ty) = self.expressions.get(arg) {
                                crate::generics::infer_bindings_allow_typevars(
                                    &param.ty,
                                    arg_ty,
                                    &mut typevar_bindings,
                                );
                            }
                        }
                        for (name, ty) in typevar_bindings {
                            // Only a CALLEE-inferable generic may be backfilled.
                            // A formal TypeVar that names one of the CALLER's
                            // own generics is RIGID in this body (receiver
                            // substitution leaves e.g. a Self-pinned `T` in
                            // the signature): binding it from an argument
                            // would silently collapse `x.eq(5)` to `T = int`
                            // instead of rejecting the literal against the
                            // rigid `T`. Same for the pinned `Self` var.
                            let callee_inferable = !self.generic_params.contains(&name)
                                && rigid_self_var.as_ref() != Some(&name);
                            let all_rigid = !matches!(ty, Ty::Unknown { .. } | Ty::Error { .. })
                                && !crate::generics::contains_non_rigid_typevar(
                                    &ty,
                                    &self.generic_params,
                                );
                            if callee_inferable && all_rigid {
                                bindings.entry(name).or_insert(ty);
                            }
                        }
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

                if run_inference_phases && !self.generic_params.is_empty() {
                    let mut caller_typevar_bindings = FxHashMap::default();
                    for (param, arg) in &param_arg_pairs {
                        let arg_ty = self
                            .expressions
                            .get(arg)
                            .cloned()
                            .unwrap_or_else(|| self.infer_expr(*arg, body));
                        let arg_ty = self.call_arg_ty_for_generic_inference(*arg, arg_ty);
                        self.infer_call_bindings_allow_typevars(
                            &param.ty,
                            &arg_ty,
                            &mut caller_typevar_bindings,
                        );
                    }
                    for generic_param in generic_params {
                        if bindings.contains_key(generic_param) {
                            continue;
                        }
                        let Some(candidate) = caller_typevar_bindings.get(generic_param) else {
                            continue;
                        };
                        let references_caller_generic =
                            crate::generics::contains_typevar_where(candidate, &|name| {
                                self.generic_params.iter().any(|param| param == name)
                            });
                        let references_non_caller_generic =
                            crate::generics::contains_typevar_where(candidate, &|name| {
                                !self.generic_params.iter().any(|param| param == name)
                            });
                        if references_caller_generic && !references_non_caller_generic {
                            bindings.insert(generic_param.clone(), candidate.clone());
                        }
                    }
                }

                if run_inference_phases {
                    for _ in 0..generic_params.len() {
                        let before = bindings.clone();
                        for (idx, param) in generic_params.iter().enumerate() {
                            let Some(actual) = bindings.get(param).cloned() else {
                                continue;
                            };
                            let Some(bound) =
                                generic_param_bounds.get(idx).and_then(Option::as_ref)
                            else {
                                continue;
                            };
                            let bound = crate::generics::substitute_ty(bound, &bindings);
                            self.infer_call_bindings_rigid_self(
                                &bound,
                                &actual,
                                &mut bindings,
                                rigid_self_var.as_ref(),
                            );
                        }
                        if bindings == before {
                            break;
                        }
                    }
                }

                // Soundness for *value* callees: a function value's type may
                // mention rigid type vars from the enclosing scope that are NOT
                // among its still-inferable params — e.g. an instantiation value
                // `let f = foo<T>; f(1)` (type `(T) -> T`, `generic_params`
                // cleared) or a higher-order param `g: (T) -> T`. Inference above
                // may have bound such a `T` from an argument; drop anything not
                // in the value's own `generic_params` so it stays rigid and the
                // call is checked structurally (`int` is not a subtype of a rigid
                // `T` → mismatch) instead of silently collapsing `foo<T>` to
                // `foo<int>`. This is gated on value calls because a *declaration*
                // callee (free/method/static) reaches here with `generic_params`
                // cleared by receiver/class substitution yet its own params still
                // inferable — restricting there would wrongly freeze `arr.map`'s
                // `U` or `StreamCache.new`'s class params.
                if is_value_call {
                    bindings.retain(|name, _| {
                        crate::generics::is_value_call_inferable(name, generic_params)
                    });
                }

                // Capture caller type-variable correspondences so generic
                // *bounds* are checked even when a callee parameter is matched
                // against another type variable. Ordinary inference (`bindings`)
                // skips TypeVar→TypeVar binds, so without this a callee
                // `T extends Equatable` matched against a caller `U` would never
                // have `U`'s bound verified — letting an unbounded `U` reach a
                // bounded position and trap at runtime. These binds are used
                // solely for the bound check — never for the call's result type.
                //
                // The seed `bindings` are inserted first; `infer_bindings` then
                // *unions* any additional actuals into a binding (it does not skip
                // when one is already present), so a param matched against both a
                // concrete `C` and a caller `U` yields `C | U` here. That stays
                // sound for the *interface* bounds we check: `C | U <: I` iff every
                // member is a subtype of `I`, so unioning neither invents nor hides
                // a bound violation.
                let mut bound_check_bindings = bindings.clone();
                let mut runtime_type_arg_bindings = bindings.clone();
                for (name, ty) in runtime_type_arg_binding_seed {
                    runtime_type_arg_bindings.entry(name).or_insert(ty);
                }
                if run_inference_phases {
                    if let Some(phase0_expected) = phase0_expected.as_ref() {
                        if crate::generics::contains_typevar(ret)
                            && !matches!(phase0_expected, Ty::Unknown { .. } | Ty::Error { .. })
                        {
                            crate::generics::infer_bindings_allow_typevars(
                                ret,
                                phase0_expected,
                                &mut runtime_type_arg_bindings,
                            );
                        }
                    }
                }
                for (param, arg) in &param_arg_pairs {
                    let arg_ty = self
                        .expressions
                        .get(arg)
                        .cloned()
                        .unwrap_or_else(|| self.infer_expr(*arg, body));
                    let arg_ty = self.call_arg_ty_for_generic_inference(*arg, arg_ty);
                    self.infer_call_bindings_allow_typevars(
                        &param.ty,
                        &arg_ty,
                        &mut bound_check_bindings,
                    );
                    self.infer_call_bindings_allow_typevars(
                        &param.ty,
                        &arg_ty,
                        &mut runtime_type_arg_bindings,
                    );
                }
                self.validate_function_generic_bounds(
                    expr_id,
                    generic_params,
                    generic_param_bounds,
                    &bound_check_bindings,
                );
                let runtime_type_arg_params = if runtime_type_arg_params.is_empty() {
                    generic_params.as_slice()
                } else {
                    runtime_type_arg_params.as_slice()
                };
                if !explicit_args_used && !runtime_type_arg_params.is_empty() {
                    let type_args = self.runtime_call_type_args(
                        runtime_type_arg_params,
                        &runtime_type_arg_bindings,
                    );
                    self.record_call_type_args(expr_id, type_args);
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

                    // Defer validation only for *genuinely uninferred* type
                    // variables — a callee generic param still being solved, or a
                    // free inference / effect variable. CHECK (don't defer) the
                    // *rigid* ones:
                    //   - the pinned `Self` of a Self-pinned call (never inferred,
                    //     survives substitution as a `TypeVar`, possibly nested as
                    //     `Self[]` / `Self?`), validated by identity (Unit-A
                    //     reflexivity) — this is what makes `other: Self` sound; and
                    //   - any caller-scope generic param (bounded *or not*) that is
                    //     not shadowed by a callee generic, so a bound-method value
                    //     `let f = x.eq` keeps `Self` pinned to the caller's `T`,
                    //     and a function value `g: (T) -> _` applied to a `U` is
                    //     still rejected rather than silently accepted. (The
                    //     *concrete*-arg case `f(1)` is handled by the value-call
                    //     binding-retention above, which keeps `T` rigid here.)
                    // A callee generic of the same name (a shadow) stays deferred,
                    // which avoids confusing it with an identically-named caller
                    // param. The bounds map (`self.generic_param_bounds`) only holds
                    // *bounded* generics, so we consult the full `self.generic_params`
                    // list to catch an unbounded caller `T` too.
                    let defers_typevar =
                        crate::generics::contains_typevar_where(&expected_arg_ty, &|name| {
                            let rigid = rigid_self_var.as_ref() == Some(name)
                                || (self.generic_params.iter().any(|gp| gp == name)
                                    && !generic_params.iter().any(|g| g == name));
                            !rigid
                        });
                    if matches!(expected_arg_ty, Ty::Unknown { .. } | Ty::Error { .. })
                        || defers_typevar
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

                // Record the call's final generic instantiation (declared
                // De Bruijn order) so MIR can thread it into the callee's
                // `frame.type_args` at runtime. Values are recorded BEFORE
                // typevar erasure: a binding to the *caller's* rigid `TypeVar`
                // must survive so MIR can lower it to a `TypeArgRef` into the
                // caller's own frame (generic→generic calls). Fresh literal
                // types widen to their base primitive (`"hi"` infers
                // `T = Literal("hi")`, but the runtime tag must be `string`
                // so `is Box<string>` compares equal).
                if let Some(callee_id) = callee_expr {
                    if let Some((declared_params, _)) =
                        self.callee_declared_generic_params(callee_id)
                    {
                        if !declared_params.is_empty() {
                            // The type-checking `bindings` pass refuses
                            // TypeVar actuals (`infer_bindings` passes
                            // allow_typevar_actuals=false), so a generic→
                            // generic call (`any(fs)` inside `helper<E>`,
                            // where `fs: Future<int, E>[]`) leaves E2 unbound
                            // there. For RECORDING those rigid caller
                            // TypeVars are exactly what we want (MIR lowers
                            // them to TypeArgRef) — fill the gaps with an
                            // allow-typevars pass over the checked arg types.
                            let mut typevar_bindings: FxHashMap<Name, Ty> = FxHashMap::default();
                            for (param, arg) in &param_arg_pairs {
                                if let Some(arg_ty) = self.expressions.get(arg) {
                                    crate::generics::infer_bindings_allow_typevars(
                                        &param.ty,
                                        arg_ty,
                                        &mut typevar_bindings,
                                    );
                                }
                            }
                            let instantiation: Vec<Ty> = declared_params
                                .iter()
                                .map(|name| {
                                    bindings
                                        .get(name)
                                        .or_else(|| typevar_bindings.get(name))
                                        .cloned()
                                        .map(Ty::widen_fresh)
                                        .unwrap_or(Ty::Unknown {
                                            attr: TyAttr::default(),
                                        })
                                })
                                .collect();
                            self.call_type_instantiations.insert(expr_id, instantiation);
                        }
                    }
                }

                let substituted_ret = crate::generics::substitute_ty(ret, &bindings);
                let substituted_ret = if crate::generics::contains_typevar(&substituted_ret) {
                    substituted_ret
                } else {
                    self.resolve_associated_projections_deep(&substituted_ret)
                };
                let unresolved_callee_typevars: FxHashSet<Name> = generic_params
                    .iter()
                    .filter(|name| {
                        !bindings.contains_key(*name)
                            && !self.generic_params.iter().any(|param| param == *name)
                            && crate::generics::contains_typevar_where(
                                &substituted_ret,
                                &|candidate| candidate == *name,
                            )
                    })
                    .cloned()
                    .collect();
                if !matches!(expected, Ty::Unknown { .. } | Ty::Error { .. }) {
                    for name in &unresolved_callee_typevars {
                        self.context.report_simple(
                            TirTypeError::CannotInferTypeParameter { name: name.clone() },
                            expr_id,
                        );
                    }
                }
                let substituted_ret = if unresolved_callee_typevars.is_empty() {
                    substituted_ret
                } else {
                    crate::generics::erase_typevars_matching(&substituted_ret, &|name| {
                        unresolved_callee_typevars.contains(name)
                    })
                };
                let mut erase_diags = Vec::new();
                let result =
                    crate::generics::erase_unresolved_typevars(&substituted_ret, &mut erase_diags);
                let recovered_unresolved_generics =
                    !unresolved_callee_typevars.is_empty() || !erase_diags.is_empty();
                for d in erase_diags {
                    self.context.report_simple(d, expr_id);
                }

                CheckedCallInner {
                    result,
                    recovered_unresolved_generics,
                }
            }
            Ty::Unknown { .. } | Ty::Error { .. } => {
                self.infer_args_for_recovery(args, body);
                CheckedCallInner {
                    result: Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                    recovered_unresolved_generics: false,
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
                            recovered_unresolved_generics: false,
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
                        generic_params: Vec::new(),
                        generic_param_bounds: Vec::new(),
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
                        is_value_call,
                        is_optional_call,
                        explicit_type_arg_bindings,
                        callee_expr,
                        runtime_type_arg_params: Vec::new(),
                        runtime_type_arg_binding_seed: Vec::new(),
                        rigid_self_var,
                    });
                }

                // When *every* non-function arm is already in recovery
                // (Unknown/Error), a member's resolution already reported the
                // real error (e.g. an ambiguous interface method → E0121), so
                // don't also emit a misleading `unknown | unknown is not a
                // function`. But if a concrete non-function arm remains (e.g.
                // `int | unknown`), that arm is genuinely not callable and must
                // still be diagnosed.
                let has_recovery_arm = expanded
                    .iter()
                    .any(|a| matches!(a, Ty::Unknown { .. } | Ty::Error { .. }));
                let has_concrete_non_function_arm = expanded.iter().any(|a| {
                    !matches!(
                        a,
                        Ty::Function { .. } | Ty::Unknown { .. } | Ty::Error { .. }
                    )
                });
                if has_recovery_arm && !has_concrete_non_function_arm {
                    self.infer_args_for_recovery(args, body);
                    return CheckedCallInner {
                        result: Ty::Unknown {
                            attr: TyAttr::default(),
                        },
                        recovered_unresolved_generics: false,
                    };
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
                    recovered_unresolved_generics: false,
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
                    recovered_unresolved_generics: false,
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
            let final_ty = Ty::optional(result_ty);
            self.report_result_type_mismatch(expr_id, &final_ty, expected);
            self.record_expr_type(expr_id, final_ty.clone());
            return final_ty;
        }

        if callee_info.is_null_only() {
            self.infer_args_for_recovery(args, body);
            let ty = Ty::Null {
                attr: TyAttr::default(),
            };
            self.report_result_type_mismatch(expr_id, &ty, expected);
            self.record_expr_type(expr_id, ty.clone());
            return ty;
        }

        let callee_generic_params = match &callee_info.inner {
            Ty::Function { generic_params, .. } => generic_params.clone(),
            _ => Vec::new(),
        };
        let runtime_type_arg_params = self.runtime_type_arg_params_for_call(
            callee_id,
            &callee_generic_params,
            is_method_call,
            false,
        );

        let checked = self.check_call_inner(CallCheckRequest {
            context: call,
            callee_ty: callee_info.inner,
            is_method_call,
            // Optional calls are always member accesses (`x?.foo(...)`), never a
            // bare-local value callee.
            is_value_call: false,
            is_optional_call: true,
            explicit_type_arg_bindings: None,
            callee_expr: Some(callee_id),
            runtime_type_arg_params,
            runtime_type_arg_binding_seed: self
                .interface_default_owner_type_arg_bindings
                .get(&callee_id)
                .cloned()
                .unwrap_or_default(),
            rigid_self_var: self.self_pinned_rigid_var.get(&callee_id).cloned(),
        });
        let final_ty = Ty::optional(checked.result);
        if !checked.recovered_unresolved_generics {
            self.report_result_type_mismatch(expr_id, &final_ty, expected);
        }
        self.record_expr_type(expr_id, final_ty.clone());
        final_ty
    }

    // ── Bidirectional Type Checking ─────────────────────────────────────────

    /// Synthesis mode: compute the type of an expression bottom-up.
    pub fn infer_expr(&mut self, expr_id: ExprId, body: &ExprBody) -> Ty {
        let expr = &body.exprs[expr_id];
        let ty = match expr {
            Expr::Literal(lit) => Self::infer_literal(lit),
            Expr::ByteStringLiteral(_) => Ty::Uint8Array {
                attr: TyAttr::default(),
            },
            Expr::Null => Ty::Null {
                attr: TyAttr::default(),
            },
            Expr::Path(segments) => self.infer_path(segments.as_slice(), body, expr_id),
            Expr::GenericApply { base, type_args } => {
                self.infer_generic_apply(*base, type_args, body, expr_id)
            }
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
            Expr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            } => self.infer_if_let_expr(
                expr_id,
                *pattern,
                *scrutinee,
                *then_branch,
                *else_branch,
                body,
            ),
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
                self.infer_member_access_expr(expr_id, body, *base, member)
            }
            Expr::Upcast { base, target } => self.infer_upcast_expr(expr_id, body, *base, target),
            Expr::OptionalMemberAccess { base, member } => {
                self.infer_optional_member_access_expr(expr_id, body, *base, member)
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
                Ty::Map {
                    key: Box::new(key_ty),
                    value: Box::new(val_ty),
                    attr: TyAttr::default(),
                }
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
                spreads,
                ..
            } if Self::is_map_object_literal(type_name.as_ref(), obj_type_args, spreads) => {
                self.infer_map_object_expr(body, fields)
            }
            Expr::Object {
                type_name,
                type_args: obj_type_args,
                fields,
                ..
            } => self.infer_object_expr(expr_id, body, type_name.as_ref(), obj_type_args, fields),
            Expr::Index { base, index } => self.infer_index_expr(expr_id, body, *base, *index),
            Expr::OptionalIndex { base, index } => {
                self.infer_optional_index_expr(expr_id, body, *base, *index)
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
            Expr::Lambda(func_def) => self.infer_lambda_expr(expr_id, func_def),
            Expr::Spawn {
                name,
                with_exprs,
                body: spawn_body,
            } => self.infer_spawn_expr(body, *name, with_exprs, *spawn_body),
            Expr::Await { future } => self.infer_await_expr(body, *future),
            Expr::Missing => Ty::Unknown {
                attr: TyAttr::default(),
            },
        };
        self.record_expr_type(expr_id, ty.clone());
        ty
    }

    #[inline(never)]
    fn infer_member_access_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        base: ExprId,
        member: &Name,
    ) -> Ty {
        // `MemberAccess` now only comes from `FIELD_ACCESS_EXPR` (complex base
        // expressions like `f().a`, `arr[0].x`). Package-qualified paths are
        // always `Expr::Path` nodes (never `MemberAccess`) after Phase 1.
        //
        // Still handle primitive-type static method access (e.g. an expression
        // evaluating to an image type followed by `.from_url`) via the existing
        // try_primitive_static_access helper.
        if let Some(ty) = self.try_primitive_static_access(expr_id, base, member, body) {
            return ty;
        }

        let base_ty = self.infer_expr(base, body);

        // Determine if the base is a runtime value (local variable, function
        // result, etc.) or a bare type name used as a namespace (e.g.
        // `Factory<int>` in `Factory<int>.create(42)`).
        // A base is a type name if it's a `Path` whose root segment is NOT
        // a local variable — multi-segment paths like
        // `root.pkg.inner.Box<int>.from_json(j)` resolve to a class type at
        // the base position, so the access is an "unbound" static-method
        // reference (bound = false).
        let base_is_value = match &body.exprs[base] {
            Expr::Path(segments) if !segments.is_empty() => self.locals.contains_key(&segments[0]),
            _ => true, // complex expressions are always values
        };

        let inner = crate::narrowing::remove_null(&base_ty);
        // `Primitive(Null)` is a concrete non-optional type (the null value
        // itself) with its own companion class. Treat it like any other
        // primitive — do NOT require `?.` chaining for direct method calls.
        let is_pure_null = matches!(base_ty, Ty::Null { .. });
        if inner != base_ty
            && !is_pure_null
            && !matches!(base_ty, Ty::Unknown { .. } | Ty::Error { .. })
        {
            if self.in_optional_chain > 0 {
                // Inside an OptionalChain: auto-unwrap nullable base,
                // resolve the member, and re-wrap in Optional.
                // This allows `a?.b.c` where `a?.b` returns `T?`.
                let member_ty = self.resolve_member(&inner, member, expr_id, base_is_value);
                Ty::optional(member_ty)
            } else {
                // Outside any chain: accessing `.member` on a nullable type
                // is an error (e.g. `(a?.b).c`). Use `?.` instead.
                let base_text = body.display_expr(base);
                self.context.report_simple(
                    TirTypeError::NullableMemberAccess {
                        base: base_text.clone(),
                        member: format!(".{member}"),
                        expr: format!("{base_text}.{member}"),
                    },
                    expr_id,
                );
                // Still resolve for downstream inference
                let member_ty = self.resolve_member(&inner, member, expr_id, base_is_value);
                Ty::optional(member_ty)
            }
        } else {
            self.resolve_member(&base_ty, member, expr_id, base_is_value)
        }
    }

    #[inline(never)]
    fn infer_upcast_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        base: ExprId,
        target: &TypeExpr,
    ) -> Ty {
        let base_ty = self.infer_expr(base, body);
        let mut diags = Vec::new();
        let mut target_ty = crate::lower_type_expr::lower_type_expr_in_ns(
            self.context.db(),
            target,
            self.package_items,
            &self.ns_context,
            &self.generic_params,
            &mut diags,
        );
        if let Ty::Interface(iface_qtn, iface_args, associated_bindings, attr) = &target_ty
            && associated_bindings.is_empty()
        {
            let projected_interface =
                Ty::Interface(iface_qtn.clone(), iface_args.clone(), vec![], attr.clone());
            let projection_resolver =
                crate::associated_projection::AssociatedProjectionResolver::with_resolution_context(
                    self.context.db(),
                    self.res_ctx,
                    &self.aliases,
                    &self.generic_param_bounds,
                );
            let completed_bindings: Vec<(Name, Ty)> = self
                .interface_associated_type_names(iface_qtn)
                .into_iter()
                .filter_map(|name| {
                    let projected = Ty::AssociatedTypeProjection {
                        base: Box::new(base_ty.clone()),
                        interface: Some(Box::new(projected_interface.clone())),
                        member: name.clone(),
                        attr: TyAttr::default(),
                    };
                    let resolved = projection_resolver.resolve_deep(&projected);
                    if matches!(resolved, Ty::AssociatedTypeProjection { .. }) {
                        None
                    } else {
                        Some((name, resolved))
                    }
                })
                .collect();
            if !completed_bindings.is_empty() {
                target_ty = Ty::Interface(
                    iface_qtn.clone(),
                    iface_args.clone(),
                    completed_bindings,
                    attr.clone(),
                );
            }
        }
        for diag in diags {
            self.context.report_simple(diag, expr_id);
        }
        let valid_target = matches!(target_ty, Ty::Interface(_, _, _, _));
        if !valid_target && !matches!(target_ty, Ty::Unknown { .. } | Ty::Error { .. }) {
            self.context.report_simple(
                TirTypeError::InvalidInterfaceUpcastTarget {
                    target: target_ty.clone(),
                },
                expr_id,
            );
        } else if !self.is_subtype(&base_ty, &target_ty)
            && !matches!(base_ty, Ty::Unknown { .. } | Ty::Error { .. })
            && !matches!(target_ty, Ty::Unknown { .. } | Ty::Error { .. })
        {
            // BEP-044 wf3 #G9d: when a *concrete* value is projected to
            // an interface it doesn't implement, say so directly rather
            // than emitting a generic `type mismatch`. (Interface→sibling
            // projections keep the type-mismatch form, which names both
            // interfaces.)
            if let (Ty::Interface(_, _, _, _), false) =
                (&target_ty, matches!(base_ty, Ty::Interface(..)))
            {
                self.context.report_simple(
                    TirTypeError::TypeDoesNotImplementInterface {
                        value_type: base_ty,
                        interface: target_ty.clone(),
                    },
                    expr_id,
                );
            } else {
                self.context.report_simple(
                    TirTypeError::TypeMismatch {
                        expected: target_ty.clone(),
                        got: base_ty,
                    },
                    expr_id,
                );
            }
        }
        target_ty
    }

    #[inline(never)]
    fn infer_optional_member_access_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        base: ExprId,
        member: &Name,
    ) -> Ty {
        // Optional chaining: a?.b — if a is null, short-circuits to null.
        // Type: if a: T?, resolve member on T, wrap result in Optional.
        let base_ty = self.infer_expr(base, body);
        let base_info = self.analyze_optional_base(&base_ty);
        // E2: warn if base is not nullable (?.  is unnecessary)
        if !base_info.is_nullable() && !matches!(base_ty, Ty::Unknown { .. } | Ty::Error { .. }) {
            let base_text = body.display_expr(base);
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
            Ty::Null {
                attr: TyAttr::default(),
            }
        } else {
            // OptionalMemberAccess always has a value base → bound = true.
            let member_ty = self.resolve_member(&base_info.inner, member, expr_id, true);
            Ty::optional(member_ty)
        }
    }

    #[inline(never)]
    fn infer_index_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        base: ExprId,
        index: ExprId,
    ) -> Ty {
        let base_ty = self.infer_expr(base, body);
        self.infer_expr(index, body);
        let inner = crate::narrowing::remove_null(&base_ty);
        let (resolve_ty, rewrap) = if inner != base_ty
            && !matches!(base_ty, Ty::Unknown { attr: _ } | Ty::Error { attr: _ })
        {
            if self.in_optional_chain == 0 {
                // Outside any chain: indexing a nullable type is an error.
                let base_text = body.display_expr(base);
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
            Ty::Map {
                key: _,
                value: val_ty,
                ..
            }
            | Ty::EvolvingMap(_, val_ty, _) => *val_ty,
            Ty::Uint8Array { .. } => Ty::Int {
                attr: TyAttr::default(),
            },
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
            Ty::optional(elem_ty)
        } else {
            elem_ty
        }
    }

    #[inline(never)]
    fn infer_optional_index_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        base: ExprId,
        index: ExprId,
    ) -> Ty {
        // Optional chaining: a?.[expr] — short-circuits to null if a is null.
        let base_ty = self.infer_expr(base, body);
        self.infer_expr(index, body);
        let base_info = self.analyze_optional_base(&base_ty);
        // E2: warn if base is not nullable
        if !base_info.is_nullable() && !matches!(base_ty, Ty::Unknown { .. } | Ty::Error { .. }) {
            let base_text = body.display_expr(base);
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
            Ty::Null {
                attr: TyAttr::default(),
            }
        } else {
            let elem_ty = match &base_info.inner {
                Ty::List(elem_ty, _) | Ty::EvolvingList(elem_ty, _) => elem_ty.as_ref().clone(),
                Ty::Map {
                    key: _,
                    value: val_ty,
                    ..
                }
                | Ty::EvolvingMap(_, val_ty, _) => val_ty.as_ref().clone(),
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
            Ty::optional(elem_ty)
        }
    }

    #[inline(never)]
    fn infer_spawn_expr(
        &mut self,
        body: &ExprBody,
        name: Option<ExprId>,
        with_exprs: &[ExprId],
        spawn_body: ExprId,
    ) -> Ty {
        // BEP-034: `spawn name? with? { body } : Future<T, E>` where
        // `body` has type `T throws E`. After AST lowering the
        // body is wrapped in a synthetic 0-arg lambda; we infer
        // the lambda's type, peel out its return as `T`, and
        // pull its effective throws (computed and stored by
        // `infer_lambda_body`) as `E`.
        if let Some(name_id) = name {
            let _ = self.infer_expr(name_id, body);
        }
        let lambda_ty = self.infer_expr(spawn_body, body);
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
            .get(&spawn_body)
            .cloned()
            .map_or_else(
                || Ty::Null {
                    attr: TyAttr::default(),
                },
                |t| {
                    if matches!(t, Ty::Never { .. }) {
                        Ty::Null {
                            attr: TyAttr::default(),
                        }
                    } else {
                        t
                    }
                },
            );

        // BEP-034 middleware: fold the `with` transformers left-to-right.
        // The body seeds an implicit `SpawnParams<T0, E0>`; each transformer
        // must check against `(SpawnParams<cur>) -> SpawnParams<?, ?>` —
        // checking with that expected type lets phase-0 reverse inference
        // bind a generic transformer's own type params from the parameter
        // position (e.g. `withRetry(3)` whose declared type is
        // `(SpawnParams<T, E>) -> SpawnParams<T, E>`). The transformer's
        // OUTPUT type args feed the next link; the final pair types the
        // spawn's `Future`. Type-changing transformers (e.g. a fallback
        // erasing the error type) fall out naturally.
        // Widen fresh literal types out of the seed (`spawn with t { 1 }`
        // must read `SpawnParams<int, null>` in diagnostics and bindings,
        // not `SpawnParams<1, null>`).
        let mut cur_value = value_ty.widen_fresh();
        let mut cur_error = throws_ty.widen_fresh();
        for with_id in with_exprs {
            let params_in = spawn_params_ty(cur_value.clone(), cur_error.clone());
            // The expected RETURN is `SpawnParams<unknown, unknown>` (not a
            // bare `Unknown`): a non-transformer return type then fails the
            // check with a readable mismatch instead of coercing into the
            // open slot.
            let expected = Ty::Function {
                generic_params: Vec::new(),
                generic_param_bounds: Vec::new(),
                params: vec![FunctionParamTy {
                    name: None,
                    ty: params_in,
                    mode: FunctionParamMode::Required,
                }],
                ret: Box::new(spawn_params_ty(
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                )),
                throws: Box::new(Ty::Unknown {
                    attr: TyAttr::default(),
                }),
                attr: TyAttr::default(),
            };
            // A VARIABLE-BOUND transformer (`let t = withRetry(3); spawn
            // with t { .. }`) is a bare path whose type still carries the
            // transformer's unbound generics — `check_expr` would reject
            // `SpawnParams<int, E>` against the link's concrete input. Infer
            // it instead and instantiate its generics from the input, the
            // same binding a direct call gets from phase-0.
            let is_value_ref = matches!(
                &body.exprs[*with_id],
                Expr::Path(_) | Expr::MemberAccess { .. }
            );
            let got = if is_value_ref {
                let inferred = self.infer_expr(*with_id, body);
                if let Ty::Function { params, .. } = &inferred
                    && params.len() == 1
                    && crate::generics::contains_typevar(&inferred)
                {
                    let mut bindings = FxHashMap::default();
                    crate::generics::infer_bindings(
                        &params[0].ty,
                        &spawn_params_ty(cur_value.clone(), cur_error.clone()),
                        &mut bindings,
                    );
                    crate::generics::substitute_ty(&inferred, &bindings)
                } else {
                    inferred
                }
            } else {
                self.check_expr(*with_id, body, &expected)
            };
            match &got {
                Ty::Function { params, ret, .. } if params.len() == 1 => {
                    // Value-refs skipped `check_expr`, so validate the input
                    // side here: the transformer's parameter must accept the
                    // link's `SpawnParams`.
                    let input_ok = !is_value_ref
                        || self.is_subtype(
                            &spawn_params_ty(cur_value.clone(), cur_error.clone()),
                            &params[0].ty,
                        );
                    if let Ty::Class(qn, args, _) = ret.as_ref()
                        && input_ok
                        && is_spawn_params_qtn(qn)
                        && args.len() == 2
                    {
                        cur_value = args[0].clone();
                        cur_error = args[1].clone();
                    } else if !matches!(ret.as_ref(), Ty::Unknown { .. } | Ty::Error { .. }) {
                        self.context.report_simple(
                            TirTypeError::SpawnWithNotATransformer {
                                expected_input: spawn_params_ty(
                                    cur_value.clone(),
                                    cur_error.clone(),
                                ),
                                got: got.clone(),
                            },
                            *with_id,
                        );
                    }
                }
                // Already diagnosed (unresolved name, failed call, ...).
                Ty::Unknown { .. } | Ty::Error { .. } => {}
                other => {
                    // Checked routes already got `check_expr`'s concrete
                    // mismatch; value-refs skipped it and must report here.
                    if is_value_ref {
                        self.context.report_simple(
                            TirTypeError::SpawnWithNotATransformer {
                                expected_input: spawn_params_ty(
                                    cur_value.clone(),
                                    cur_error.clone(),
                                ),
                                got: other.clone(),
                            },
                            *with_id,
                        );
                    }
                }
            }
        }

        Ty::Future(Box::new(cur_value), Box::new(cur_error), TyAttr::default())
    }

    #[inline(never)]
    fn infer_await_expr(&mut self, body: &ExprBody, future: ExprId) -> Ty {
        // BEP-034: `await e : T` where `e : Future<T, E>`.
        let fut_ty = self.infer_expr(future, body);
        match fut_ty {
            Ty::Future(value, _error, _) => *value,
            // `await` DISTRIBUTES over a union of futures (BEP-034): `Future`
            // is invariant, so combining differently-typed futures (if/else,
            // array elements) yields `Future<A, E1> | Future<B, E2>` — not a
            // future of a union. Awaiting it gives value `A | B`; the error
            // side (`E1 | E2`) is contributed by the throws analysis.
            Ty::Union(ref members, _)
                if !members.is_empty() && members.iter().all(|m| matches!(m, Ty::Future(..))) =>
            {
                let mut values = members.iter().filter_map(|m| match m {
                    Ty::Future(value, _, _) => Some(value.as_ref().clone()),
                    _ => None,
                });
                let first = values.next().expect("non-empty checked above");
                values.fold(first, |acc, v| crate::generics::union_ty(&acc, &v))
            }
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
                    future,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
        }
    }

    #[inline(never)]
    fn infer_lambda_expr(&mut self, expr_id: ExprId, func_def: &ast::FunctionDef) -> Ty {
        // Synthesis mode: no expected type available.
        // All param types MUST be annotated; unannotated params produce an error.

        // Combine parent generics with the lambda's own generic params
        // so that `<T>(x: T) -> T { x }` recognizes T as a TypeVar.
        let mut all_generic_params = self.generic_params.clone();
        all_generic_params.extend(func_def.generic_params.iter().cloned());

        let mut param_tys: Vec<FunctionParamTy> = Vec::new();

        for param in &func_def.params {
            let param_ty = match &param.type_expr {
                Some(te) => self.lower_lambda_type_expr(&te.expr, &all_generic_params, te.span),
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
        // An UNANNOTATED lambda in synthesis mode (no contextual throws)
        // INFERS its throws surface from the body — a lambda throws what it
        // throws. Pass `Unknown` to the declared-vs-effective check (open
        // slot → skipped; there is nothing declared to violate) and take the
        // effective set as the function type's throws below. E.g. a
        // middleware body wrap `() -> { original() * 2 }` where `original:
        // () -> T throws E` types as `() -> T throws E`, not `throws never`.
        let infer_throws_from_body =
            func_def.throws.is_none() && !Self::is_spawn_body_lambda(func_def);
        let throws_ty = if infer_throws_from_body {
            Ty::Unknown {
                attr: TyAttr::default(),
            }
        } else {
            throws_ty
        };

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

        let surface_throws = if infer_throws_from_body {
            lambda_effective_throws.clone()
        } else {
            throws_ty
        };
        let result = Ty::Function {
            generic_params: func_def.generic_params.clone(),
            generic_param_bounds: Vec::new(),
            params: param_tys,
            ret: Box::new(surface_ret_ty),
            throws: Box::new(surface_throws),
            attr: TyAttr::default(),
        };
        self.lambda_effective_throws
            .insert(expr_id, lambda_effective_throws);
        if let Some(fsi) = lambda_fsi {
            self.nested_lambda_types.insert(fsi, result.clone());
        }
        result
    }

    fn lower_object_type_name(
        &mut self,
        expr_id: ExprId,
        path: &baml_base::core_types::TypePath,
        obj_type_args: &[TypeExpr],
    ) -> Ty {
        let mut diags = Vec::new();
        let ty_expr = TypeExpr::Path {
            segments: path.segments().to_vec(),
            generic_args: obj_type_args.to_vec(),
            associated_type_bindings: Vec::new(),
            attrs: Vec::new(),
        };
        let ty = self.lower_type_expr_in_current_body(&ty_expr, &mut diags);
        for diag in diags {
            self.context.report_simple(diag, expr_id);
        }
        ty
    }

    fn is_map_object_literal(
        type_name: Option<&baml_base::core_types::TypePath>,
        obj_type_args: &[TypeExpr],
        spreads: &[ast::SpreadField],
    ) -> bool {
        obj_type_args.is_empty()
            && spreads.is_empty()
            && matches!(
                type_name.map(|path| path.segments()),
                Some([name]) if name.as_str() == "map"
            )
    }

    fn infer_map_object_expr(&mut self, body: &ExprBody, fields: &[(Name, ExprId)]) -> Ty {
        let key_ty = if fields.is_empty() {
            Ty::Never {
                attr: TyAttr::default(),
            }
        } else {
            Ty::Primitive(PrimitiveType::String, TyAttr::default())
        };
        let val_types: Vec<Ty> = fields
            .iter()
            .map(|(_, value)| self.infer_expr(*value, body))
            .collect();
        let val_ty = Self::join_all(&val_types).widen_fresh();
        Ty::Map(Box::new(key_ty), Box::new(val_ty), TyAttr::default())
    }

    fn check_map_object_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        expected: &Ty,
        fields: &[(Name, ExprId)],
    ) -> Ty {
        if let Ty::Map(key_ty, val_ty, _) | Ty::EvolvingMap(key_ty, val_ty, _) = expected {
            let string_ty = Ty::Primitive(PrimitiveType::String, TyAttr::default());
            if !fields.is_empty() && !self.is_subtype(&string_ty, key_ty) {
                self.context.report(
                    TirTypeError::TypeMismatch {
                        expected: key_ty.as_ref().clone(),
                        got: string_ty,
                    },
                    expr_id,
                    Vec::new(),
                );
            }
            for (_, value) in fields {
                self.check_expr(*value, body, val_ty);
            }
            let ty = expected.clone();
            self.record_expr_type(expr_id, ty.clone());
            ty
        } else {
            let inferred = self.infer_map_object_expr(body, fields);
            if !matches!(expected, Ty::Unknown { .. } | Ty::Error { .. })
                && !self.is_subtype(&inferred, expected)
            {
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

    #[inline(never)]
    fn infer_object_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        type_name: Option<&baml_base::core_types::TypePath>,
        obj_type_args: &[TypeExpr],
        fields: &[(Name, ExprId)],
    ) -> Ty {
        let ty = type_name
            .map(|path| self.lower_object_type_name(expr_id, path, obj_type_args))
            .unwrap_or(Ty::Unknown {
                attr: TyAttr::default(),
            });
        let ty = match ty {
            Ty::Class(class_name, type_args, attr) => {
                if type_args.is_empty()
                    && obj_type_args.is_empty()
                    && let Some(class_loc) = self.resolve_class_loc(&class_name)
                {
                    let class_tree = baml_compiler2_hir::file_item_tree(
                        self.context.db(),
                        class_loc.file(self.context.db()),
                    );
                    let class_data = &class_tree[class_loc.id(self.context.db())];
                    if !class_data.generic_params.is_empty() {
                        let field_types: FxHashMap<Name, Ty> = self
                            .class_actual_fields_ordered(&class_name, &[])
                            .into_iter()
                            .collect();
                        let mut bindings = FxHashMap::default();
                        for (field_name, field_expr) in fields {
                            let Some(declared_ty) = field_types.get(field_name) else {
                                continue;
                            };
                            if !crate::generics::contains_typevar(declared_ty) {
                                continue;
                            }
                            let field_ty = self.infer_expr(*field_expr, body).widen_fresh();
                            crate::generics::infer_bindings(declared_ty, &field_ty, &mut bindings);
                        }
                        let inferred_type_args: Vec<Ty> = class_data
                            .generic_params
                            .iter()
                            .map(|param| {
                                bindings.get(param).cloned().unwrap_or(Ty::Unknown {
                                    attr: TyAttr::default(),
                                })
                            })
                            .collect();
                        if inferred_type_args
                            .iter()
                            .any(|ty| !matches!(ty, Ty::Unknown { .. }))
                        {
                            Ty::Class(class_name, inferred_type_args, attr)
                        } else {
                            Ty::Class(class_name, type_args, attr)
                        }
                    } else {
                        Ty::Class(class_name, type_args, attr)
                    }
                } else {
                    Ty::Class(class_name, type_args, attr)
                }
            }
            ty => ty,
        };
        self.validate_type_generic_bounds(expr_id, &ty);
        if let Ty::Class(class_name, type_args, _) = &ty {
            let field_types: FxHashMap<Name, Ty> = self
                .class_actual_fields_ordered(class_name, type_args)
                .into_iter()
                .collect();
            for (field_name, field_expr) in fields {
                if field_name.as_str().contains('.') {
                    let suggested = field_name
                        .as_str()
                        .rsplit('.')
                        .next()
                        .unwrap_or(field_name.as_str());
                    self.context.report_simple(
                        TirTypeError::InterfaceFieldRequiresQualifiedConstruction {
                            field_name: field_name.clone(),
                            qualified_name: Name::new(suggested),
                        },
                        expr_id,
                    );
                    self.infer_expr(*field_expr, body);
                } else if let Some(declared_ty) = field_types.get(field_name) {
                    if type_args.is_empty() && crate::generics::contains_typevar(declared_ty) {
                        self.infer_expr(*field_expr, body);
                        continue;
                    }
                    self.check_expr(*field_expr, body, declared_ty);
                } else if !field_name.as_str().contains('.')
                    && let Some((qualified_name, declared_ty)) = self
                        .qualified_interface_field_for_construction(
                            class_name, type_args, field_name,
                        )
                {
                    self.context.report_simple(
                        TirTypeError::InterfaceFieldRequiresQualifiedConstruction {
                            field_name: field_name.clone(),
                            qualified_name,
                        },
                        expr_id,
                    );
                    self.check_expr(*field_expr, body, &declared_ty);
                } else {
                    self.infer_expr(*field_expr, body);
                }
            }
        } else {
            for (_, expr_id) in fields {
                self.infer_expr(*expr_id, body);
            }
        }
        ty
    }

    /// Checking mode: verify an expression against an expected type.
    fn check_object_literal_declared_class_mismatch(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        expected: &Ty,
        type_name: Option<&baml_base::core_types::TypePath>,
        obj_type_args: &[TypeExpr],
    ) -> Option<Ty> {
        // BEP-044 wf3 #G15: when the literal explicitly names a concrete
        // class that differs from the expected class, it must be a subtype.
        // Keep this out of `check_expr` proper so its temporary lowering
        // state doesn't bloat every recursive checking frame in debug builds.
        let (Ty::Class(expected_qtn, _, _), Some(path)) = (expected, type_name) else {
            return None;
        };

        let lit_ty = self.lower_object_type_name(expr_id, path, obj_type_args);
        if let Ty::Class(lit_qtn, _, _) = &lit_ty
            && lit_qtn != expected_qtn
        {
            let inferred = self.infer_expr(expr_id, body);
            if !matches!(inferred, Ty::Unknown { .. } | Ty::Error { .. })
                && !self.is_subtype(&inferred, expected)
            {
                self.context.report(
                    TirTypeError::TypeMismatch {
                        expected: expected.clone(),
                        got: inferred.clone(),
                    },
                    expr_id,
                    Vec::new(),
                );
            }
            Some(inferred)
        } else {
            None
        }
    }

    #[inline(never)]
    fn check_object_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        expected: &Ty,
        fields: &[(Name, ExprId)],
        type_name: Option<&baml_base::core_types::TypePath>,
        obj_type_args: &[TypeExpr],
    ) -> Ty {
        if let Some(inferred) = self.check_object_literal_declared_class_mismatch(
            expr_id,
            body,
            expected,
            type_name,
            obj_type_args,
        ) {
            return inferred;
        }
        self.validate_type_generic_bounds(expr_id, expected);
        if let Ty::Class(class_name, type_args, _) = expected {
            let field_types: FxHashMap<Name, Ty> = self
                .class_actual_fields_ordered(class_name, type_args)
                .into_iter()
                .collect();
            for (field_name, field_expr) in fields {
                if field_name.as_str().contains('.') {
                    let suggested = field_name
                        .as_str()
                        .rsplit('.')
                        .next()
                        .unwrap_or(field_name.as_str());
                    self.context.report_simple(
                        TirTypeError::InterfaceFieldRequiresQualifiedConstruction {
                            field_name: field_name.clone(),
                            qualified_name: Name::new(suggested),
                        },
                        expr_id,
                    );
                    self.infer_expr(*field_expr, body);
                } else if let Some(declared_ty) = field_types.get(field_name) {
                    if type_args.is_empty() && crate::generics::contains_typevar(declared_ty) {
                        self.infer_expr(*field_expr, body);
                        continue;
                    }
                    self.check_expr(*field_expr, body, declared_ty);
                } else if !field_name.as_str().contains('.')
                    && let Some((qualified_name, declared_ty)) = self
                        .qualified_interface_field_for_construction(
                            class_name, type_args, field_name,
                        )
                {
                    self.context.report_simple(
                        TirTypeError::InterfaceFieldRequiresQualifiedConstruction {
                            field_name: field_name.clone(),
                            qualified_name,
                        },
                        expr_id,
                    );
                    self.check_expr(*field_expr, body, &declared_ty);
                } else {
                    self.infer_expr(*field_expr, body);
                }
            }
            let ty = expected.clone();
            self.record_expr_type(expr_id, ty.clone());
            ty
        } else {
            let inferred = self.infer_expr(expr_id, body);
            if !matches!(expected, Ty::Unknown { .. } | Ty::Error { .. })
                && !self.is_subtype(&inferred, expected)
            {
                // BEP-044 wf3 #G18: if the value almost implements the
                // expected interface via a blanket rule but a generic
                // bound fails, name the unsatisfied bound rather than
                // emitting a bare `type mismatch`.
                let bound_failure = if matches!(expected, Ty::Interface(..)) {
                    let db = self.context.db();
                    let registry =
                        crate::interfaces::package_implements_registry(db, self.package_id);
                    registry.first_failing_bound(&inferred, expected, &self.aliases, |a, b| {
                        self.is_subtype(a, b)
                    })
                } else {
                    None
                };
                if let Some((_param, bound, _actual_arg)) = bound_failure {
                    self.context.report(
                        TirTypeError::BlanketBoundNotSatisfied {
                            value_type: inferred.clone(),
                            bound,
                        },
                        expr_id,
                        Vec::new(),
                    );
                } else {
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: expected.clone(),
                            got: inferred.clone(),
                        },
                        expr_id,
                        Vec::new(),
                    );
                }
            }
            inferred
        }
    }

    #[inline(never)]
    fn check_call_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        expected: &Ty,
        callee: ExprId,
        type_args: &[TypeExpr],
        args: &[ast::CallArg],
    ) -> Ty {
        let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
        if matches!(&body.exprs[callee], Expr::OptionalMemberAccess { .. })
            && self.in_optional_chain > 0
        {
            let callee_ty = self.infer_expr(callee, body);
            return self.finalize_optional_callee_call(
                OptionalCallContext {
                    call: CallContext {
                        expr_id,
                        args: &arg_exprs,
                        call_args: Some(args),
                        body,
                        expected,
                    },
                    callee_id: callee,
                    is_method_call: true,
                },
                &callee_ty,
            );
        }

        // Container mutation fast path (e.g. x.push(val) on EvolvingList).
        // Matches MemberAccess and 2-segment Path (multi-segment paths).
        if Self::is_method_like_callee(&body.exprs[callee])
            && let Some(result_ty) = self.try_container_method_call(callee, &arg_exprs, body)
        {
            self.report_result_type_mismatch(expr_id, &result_ty, expected);
            self.record_expr_type(expr_id, result_ty.clone());
            return result_ty;
        }

        let is_method_call = match &body.exprs[callee] {
            Expr::MemberAccess { base, .. } => match &body.exprs[*base] {
                Expr::Path(segments) if !segments.is_empty() => {
                    self.locals.contains_key(&segments[0])
                }
                _ => true,
            },
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
        // A "value call" is one whose callee is a function *value* held in a
        // local/param (e.g. `let f = foo<int>; f(x)` or a higher-order param
        // `g`), as opposed to a direct reference to a function/method
        // declaration. Only a bare single-segment path bound in the local scope
        // qualifies — member accesses, qualified paths, and references to
        // top-level functions are declaration calls. For value callees the
        // callee type's `generic_params` is an accurate list of the still-
        // inferable params, so inference can be restricted to them.
        let is_value_call = matches!(
            &body.exprs[callee],
            Expr::Path(segs) if segs.len() == 1 && self.locals.contains_key(&segs[0])
        );
        let callee_ty = self.infer_expr(callee, body);

        // When explicit type args are written at the call site (e.g. `foo<int, T>(x)`),
        // validate arity and resolve them to a pre-computed bindings map.
        let explicit_type_arg_bindings = if !type_args.is_empty() {
            self.resolve_explicit_type_args(callee, type_args, expr_id)
        } else {
            None
        };
        let callee_generic_params = match &callee_ty {
            Ty::Function { generic_params, .. } => generic_params.clone(),
            _ => Vec::new(),
        };
        let runtime_type_arg_params = self.runtime_type_arg_params_for_call(
            callee,
            &callee_generic_params,
            is_method_call,
            is_value_call,
        );

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
            is_value_call,
            is_optional_call: false,
            explicit_type_arg_bindings,
            callee_expr: Some(callee),
            runtime_type_arg_params,
            runtime_type_arg_binding_seed: self
                .interface_default_owner_type_arg_bindings
                .get(&callee)
                .cloned()
                .unwrap_or_default(),
            rigid_self_var: self.self_pinned_rigid_var.get(&callee).cloned(),
        });

        if !checked.recovered_unresolved_generics {
            self.report_result_type_mismatch(expr_id, &checked.result, expected);
            self.record_function_coercion_if_needed(expr_id, &checked.result, expected);
        }
        self.record_expr_type(expr_id, checked.result.clone());
        checked.result
    }

    #[inline(never)]
    fn check_optional_call_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        expected: &Ty,
        callee: ExprId,
        args: &[ast::CallArg],
    ) -> Ty {
        let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
        let is_method_call = matches!(
            &body.exprs[callee],
            Expr::MemberAccess { .. } | Expr::OptionalMemberAccess { .. }
        );
        let callee_ty = self.infer_expr(callee, body);

        if !self.analyze_optional_base(&callee_ty).is_nullable()
            && !matches!(&callee_ty, Ty::Unknown { .. } | Ty::Error { .. })
        {
            let callee_text = body.display_expr(callee);
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
                callee_id: callee,
                is_method_call,
            },
            &callee_ty,
        )
    }

    #[inline(never)]
    fn check_lambda_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        expected: &Ty,
        func_def: &ast::FunctionDef,
    ) -> Ty {
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
                            let annotated =
                                self.lower_lambda_type_expr(&te.expr, &all_generic_params, te.span);
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
                let return_annotation = func_def
                    .return_type
                    .as_ref()
                    .map(|te| self.lower_lambda_type_expr(&te.expr, &all_generic_params, te.span));
                let effective_ret = return_annotation.as_ref().unwrap_or(expected_ret.as_ref());
                let (throws_ty, throws_span, warn_extraneous_throws) = self
                    .choose_lambda_throws_surface(
                        func_def,
                        &all_generic_params,
                        Some(expected_throws.as_ref()),
                    );

                // Infer/check the lambda body using save/restore approach
                let (ret_ty, _lambda_expressions, lambda_fsi, lambda_effective_throws) = self
                    .infer_lambda_body(
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
                    generic_params: func_def.generic_params.clone(),
                    generic_param_bounds: Vec::new(),
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

    pub fn check_expr(&mut self, expr_id: ExprId, body: &ExprBody, expected: &Ty) -> Ty {
        // This function is deeply recursive during builtin throws inference.
        // Keep bulky match-arm temporaries in helpers so debug builds don't
        // reserve them in every `check_expr` frame (Windows build.rs stacks are
        // tight enough for a few extra KiB per frame to matter).
        // Fix the element type of an empty evolving container the first time
        // it is used in a typed context (see `retype_evolving_empty`).
        self.retype_evolving_empty(expr_id, body, expected);
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
            // IfLet: like `If`, push the expected type into both branches —
            // critical for the function-body tail-expression path so that
            // `if let pat = e { v1 } else { v2 }` doesn't report
            // "missing return" when the if-let *is* the return value.
            Expr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            } => self.check_if_let_expr(
                expr_id,
                *pattern,
                *scrutinee,
                *then_branch,
                *else_branch,
                body,
                expected,
            ),
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
                    let inferred = self.infer_expr(expr_id, body);
                    if !matches!(expected, Ty::Unknown { .. } | Ty::Error { .. })
                        && !self.is_subtype(&inferred, expected)
                    {
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
            // Object: if expected is Class(name), check fields against declared types.
            //
            // Class fields are invariant, like `Array`/`Map` elements. Checking
            // each field against its declared type rejects a mismatched value
            // (e.g. an `int` field expression against a `bigint` field — `int`
            // is not a subtype of `bigint`, and the runtime does no field-level
            // widening, so it must be written `1n`).
            Expr::Object {
                fields,
                type_name,
                type_args,
                spreads,
                ..
            } if Self::is_map_object_literal(type_name.as_ref(), type_args, spreads) => {
                self.check_map_object_expr(expr_id, body, expected, fields)
            }
            Expr::Object {
                fields,
                type_name,
                type_args,
                ..
            } => self.check_object_expr(
                expr_id,
                body,
                expected,
                fields,
                type_name.as_ref(),
                type_args,
            ),
            Expr::Map { entries } => {
                let kv = match expected {
                    Ty::Map {
                        key: k, value: v, ..
                    }
                    | Ty::EvolvingMap(k, v, _) => Some((k, v)),
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
            } => self.check_call_expr(expr_id, body, expected, *callee, type_args, args),
            Expr::OptionalCall { callee, args } => {
                self.check_optional_call_expr(expr_id, body, expected, *callee, args)
            }
            // Catch: propagate expected type to the base expression
            Expr::Catch { base, clauses } => {
                self.infer_catch_expr(expr_id, *base, clauses, body, Some(expected))
            }
            // Lambda: bidirectional checking against expected function type
            Expr::Lambda(func_def) => self.check_lambda_expr(expr_id, body, expected, func_def),
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
                    // BEP-044 wf3 #18: coercing a concrete class to an interface
                    // it provides via >1 in-body block at this instantiation is
                    // ambiguous (`Getter<L>`+`Getter<R>` collapse at `Pair<int,int>`).
                    if let (Ty::Class(cqtn, ctargs, _), Ty::Interface(iqtn, iargs, _, _)) =
                        (&inferred, expected)
                        && self.class_interface_instantiation_count(cqtn, ctargs, iqtn, iargs) > 1
                    {
                        self.context.report(
                            TirTypeError::AmbiguousInterfaceInstantiation {
                                class_name: cqtn.name().clone(),
                                interface: expected.clone(),
                            },
                            expr_id,
                            Vec::new(),
                        );
                    }
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
                else_branch,
                ..
            } => {
                let (init_result_ty, pattern_subject_ty, declared_for_scope) = if let Some(init) =
                    *initializer
                {
                    let expected = self.pattern_expected_ty(*pattern, body);
                    let is_structural_pattern =
                        Self::pattern_contains_structural_syntax(*pattern, body);
                    // For `let … else`, the pattern is refutable: the
                    // declared type narrows the binding ON A MATCH, but
                    // the initializer is allowed to be wider (the else
                    // branch handles the residual). Don't push the
                    // pattern's expected type into the init — infer it
                    // freely instead so the refutability matrix sees
                    // the full scrutinee type.
                    let expected_for_check = if else_branch.is_some() {
                        None
                    } else {
                        match expected {
                            Some(PatternExpectedTy::Full(ty)) if !is_structural_pattern => Some(ty),
                            Some(PatternExpectedTy::Partial(ty))
                                if Self::expr_accepts_partial_pattern_expected(init, body) =>
                            {
                                Some(ty)
                            }
                            Some(PatternExpectedTy::Full(_) | PatternExpectedTy::Partial(_))
                            | None => None,
                        }
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
                    // For `let … else`, check the else branch FIRST, in the
                    // scope before pattern bindings are introduced. The else
                    // path runs when the pattern did not match, so it must
                    // not see the names the pattern would bind. We snapshot
                    // scoped locals and restore so any inner let-bindings in
                    // the else block don't leak into the enclosing scope.
                    if let Some(else_expr) = else_branch {
                        let snapshot = self.snapshot_scoped_locals();
                        let else_ty = self.infer_expr(*else_expr, body);
                        // The else branch diverges by construction, so its
                        // writes — including assignments to outer bindings —
                        // are not observable in the success continuation.
                        // Use the hard-rollback path, not the join-style
                        // `restore_scoped_locals` that an `if`/`if let`
                        // branch would use.
                        self.discard_scoped_locals(snapshot);
                        if !matches!(else_ty, Ty::Never { .. } | Ty::Unknown { .. }) {
                            let err = TirTypeError::LetElseMustDiverge { got: else_ty };
                            self.context.report_simple(err, *else_expr);
                        }
                    }

                    let result =
                        self.analyze_and_lower(*pattern, &flow_ty, body, initializer.unwrap());
                    // Irrefutable-pattern check differs by binding form:
                    //   - plain `let`: refutable patterns are an error
                    //     (RefutablePatternInLet) — they'd fail at runtime
                    //     with nowhere to go.
                    //   - `let … else`: refutable is the whole point, but an
                    //     irrefutable pattern makes the else branch dead, so
                    //     warn (IrrefutablePatternInLetElse) and suggest
                    //     dropping the else.
                    let irrefutable_ctx = if else_branch.is_some() {
                        None
                    } else {
                        Some(IrrefutablePatternContext {
                            context: IrrefutableContextKind::Let,
                            fallback_expr: *initializer,
                        })
                    };
                    self.finalize_pattern_lowering(
                        *pattern,
                        &result,
                        declared_for_scope.as_ref(),
                        irrefutable_ctx,
                        &flow_ty,
                    );
                    if else_branch.is_some() {
                        let scrut_for_matrix = self.matrix_normalize_scrut(&flow_ty);
                        let report = crate::pattern_lowering::compute_match_usefulness(
                            self,
                            std::slice::from_ref(&result.dpat),
                            scrut_for_matrix,
                        );
                        if report.missing.is_empty() {
                            let err = TirTypeError::IrrefutablePatternInLetElse;
                            if let Some(sm) = self.body_source_map.as_ref() {
                                self.context
                                    .report_warning_at_span(err, sm.pattern_span(*pattern));
                            } else if let Some(init) = *initializer {
                                self.context.report_warning_simple(err, init);
                            }
                        }
                    }
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
            Stmt::WhileLet {
                pattern,
                scrutinee,
                body: while_body,
            } => {
                // Refutable pattern in a loop header: mirrors `if_let_expr_common`
                // (refutable expected, warn-not-reject if irrefutable) crossed
                // with `Stmt::While`'s scoping (snapshot/restore so body lets and
                // narrowings don't leak past the loop). Produces unit and never
                // diverges — the body may run zero times.
                let scrutinee_ty = self.infer_expr(*scrutinee, body);
                let scrutinee_name = match &body.exprs[*scrutinee] {
                    Expr::Path(segments) if segments.len() == 1 => Some(segments[0].clone()),
                    _ => None,
                };

                // Lower the refutable pattern against the scrutinee — same
                // machinery as `match` arms / `if let`. Populates
                // `pattern_types` and yields `matched_ty` + `dpat`.
                let result = self.analyze_and_lower(*pattern, &scrutinee_ty, body, *while_body);

                // Body scope: narrow the scrutinee to the matched type and
                // register the pattern bindings for the body only, then restore.
                let snapshot = self.snapshot_scoped_locals();
                if let Some(name) = &scrutinee_name {
                    self.narrow_local(name.clone(), result.matched_ty.clone());
                }
                self.finalize_pattern_lowering(*pattern, &result, None, None, &scrutinee_ty);
                self.infer_expr(*while_body, body);
                self.restore_scoped_locals(&snapshot);

                // Irrefutability warning — same policy as `if let`. An
                // irrefutable `while let` never exits via pattern failure, so it
                // is an unconditional infinite loop with a pointless pattern;
                // warn and suggest a plain `while`/`loop`.
                let scrutinee_ty_for_matrix = self.matrix_normalize_scrut(&scrutinee_ty);
                let report = crate::pattern_lowering::compute_match_usefulness(
                    self,
                    std::slice::from_ref(&result.dpat),
                    scrutinee_ty_for_matrix,
                );
                if report.missing.is_empty() {
                    let err = crate::infer_context::TirTypeError::IrrefutablePatternInWhileLet;
                    if let Some(sm) = self.body_source_map.as_ref() {
                        self.context
                            .report_warning_at_span(err, sm.pattern_span(*pattern));
                    } else {
                        self.context.report_warning_simple(err, *scrutinee);
                    }
                }

                false
            }
            // Design note: Stmt::For is kept as a first-class construct (not desugared
            // to While) so we can produce for-loop-specific diagnostics ("cannot iterate
            // over type X") and lower through the Iterable interface in MIR.
            Stmt::For {
                binding,
                collection,
                body: for_body,
            } => {
                // 1. Infer the collection type
                let coll_ty = self.infer_expr(*collection, body);

                // 2. Derive the element type through Iterable.Item.
                let elem_ty = if let Some(item_ty) = self.iterable_associated_ty(&coll_ty, "Item") {
                    item_ty
                } else {
                    self.context
                        .report_simple(TirTypeError::NotIterable { ty: coll_ty }, *collection);
                    Ty::Unknown {
                        attr: TyAttr::default(),
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

        // `Stmt::Expr(Expr::IfLet { ... })` — same shape as the `Expr::If`
        // case below, but the narrowing is implicit in the pattern rather
        // than explicit in a boolean condition. When the then-branch
        // diverges and execution can continue past the statement, the
        // scrutinee narrows to the complement of the matched type for the
        // rest of the enclosing block.
        if let baml_compiler2_ast::Stmt::Expr(if_let_expr_id) = stmt {
            let if_let_expr = &body.exprs[*if_let_expr_id];
            if let Expr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                ..
            } = if_let_expr
            {
                let pattern = *pattern;
                let scrutinee = *scrutinee;
                let then_branch = *then_branch;

                // Resolve the scrutinee name + matched type *before*
                // running check_stmt so we can apply the residual
                // narrowing afterward without re-walking the if-let.
                let scrutinee_ty = self.infer_expr(scrutinee, body);
                let scrutinee_name = match &body.exprs[scrutinee] {
                    Expr::Path(segments) if segments.len() == 1 => Some(segments[0].clone()),
                    _ => None,
                };

                let stmt_diverges = self.check_stmt(stmt_id, body);

                if let Some(name) = scrutinee_name {
                    let then_ty = self.expressions.get(&then_branch);
                    let then_diverged = matches!(then_ty, Some(Ty::Never { .. }));
                    if then_diverged && !stmt_diverges {
                        // Look up the matched type recorded by
                        // `analyze_and_lower_no_subtype_check` keyed on the
                        // pattern's PatId. The if-let's inference already
                        // populated this; we just subtract to get the
                        // complement.
                        if let Some(matched_ty) = self.pattern_types.get(&pattern).cloned() {
                            let complement =
                                crate::narrowing::subtract_pattern_type(&scrutinee_ty, &matched_ty);
                            self.narrow_local(name, complement);
                        }
                    }
                }

                return stmt_diverges;
            }
        }

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

    /// Type-check `if let PATTERN = SCRUTINEE { THEN } else { ELSE }`.
    ///
    /// Refutable pattern match in condition position. Modeled on
    /// [`Self::infer_match_expr`] with effectively two arms:
    ///   - the user-written pattern (binds names in the then-branch)
    ///   - an implicit wildcard arm (the else-branch)
    ///
    /// Side effects:
    /// - Pattern bindings registered in `self.locals` for the then-branch only
    ///   (snapshot/restore around the body).
    /// - Scrutinee local narrowed to `matched_ty` in the then-branch, and to
    ///   the complement type in the else-branch (when the scrutinee is a
    ///   simple-path local).
    /// - If the matrix says the pattern is irrefutable, emit a warning at the
    ///   pattern span suggesting `let` instead.
    /// - Joined branch types returned as the if-let's value; if there is no
    ///   else, the type is `Void`.
    fn infer_if_let_expr(
        &mut self,
        if_let_expr_id: ExprId,
        pattern_id: PatId,
        scrutinee_expr_id: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
        body: &ExprBody,
    ) -> Ty {
        self.if_let_expr_common(
            if_let_expr_id,
            pattern_id,
            scrutinee_expr_id,
            then_branch,
            else_branch,
            body,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn check_if_let_expr(
        &mut self,
        if_let_expr_id: ExprId,
        pattern_id: PatId,
        scrutinee_expr_id: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
        body: &ExprBody,
        expected: &Ty,
    ) -> Ty {
        let ty = self.if_let_expr_common(
            if_let_expr_id,
            pattern_id,
            scrutinee_expr_id,
            then_branch,
            else_branch,
            body,
            Some(expected),
        );
        // Without an else, the if-let evaluates to `Void`. Using it in
        // value position is the same error as `if cond { ... }` without
        // an else — `Expr::If`'s check arm reports `VoidUsedAsValue`
        // here, and we match that.
        if else_branch.is_none() && !matches!(expected, Ty::Void { .. } | Ty::Unknown { .. }) {
            self.context
                .report_simple(TirTypeError::VoidUsedAsValue, if_let_expr_id);
        }
        self.record_expr_type(if_let_expr_id, ty.clone());
        ty
    }

    /// Shared inference/checking core for `Expr::IfLet`. When `expected` is
    /// `Some`, branches are propagated through `check_expr` (giving the
    /// function-body tail-expression path the same expected-type flow as
    /// `if`/`match`); when `None`, they're inferred bottom-up.
    #[allow(clippy::too_many_arguments)]
    fn if_let_expr_common(
        &mut self,
        if_let_expr_id: ExprId,
        pattern_id: PatId,
        scrutinee_expr_id: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
        body: &ExprBody,
        expected: Option<&Ty>,
    ) -> Ty {
        let scrutinee_ty = self.infer_expr(scrutinee_expr_id, body);
        let scrutinee_name = match &body.exprs[scrutinee_expr_id] {
            Expr::Path(segments) if segments.len() == 1 => Some(segments[0].clone()),
            _ => None,
        };

        // Lower the pattern against the scrutinee type. Same machinery as
        // `match` arms — does subtype check, populates `pattern_types`.
        let result = self.analyze_and_lower(pattern_id, &scrutinee_ty, body, then_branch);
        let matched_ty = result.matched_ty.clone();

        // Then-branch: push a fresh scope, narrow scrutinee, register
        // pattern bindings, then infer/check the body.
        let snapshot = self.snapshot_scoped_locals();
        if let Some(name) = &scrutinee_name {
            self.narrow_local(name.clone(), matched_ty.clone());
        }
        self.finalize_pattern_lowering(pattern_id, &result, None, None, &scrutinee_ty);
        let then_ty = match expected {
            Some(exp) => self.check_expr(then_branch, body, exp),
            None => self.infer_expr(then_branch, body),
        };
        self.restore_scoped_locals(&snapshot);

        // Else-branch: narrow scrutinee to the complement (residual type
        // after subtracting matched_ty), then infer/check.
        let else_ty = if let Some(else_expr) = else_branch {
            let else_snapshot = self.snapshot_scoped_locals();
            if let Some(name) = &scrutinee_name {
                let complement =
                    crate::narrowing::subtract_pattern_type(&scrutinee_ty, &matched_ty);
                self.narrow_local(name.clone(), complement);
            }
            let ty = match expected {
                Some(exp) => self.check_expr(else_expr, body, exp),
                None => self.infer_expr(else_expr, body),
            };
            self.restore_scoped_locals(&else_snapshot);
            Some(ty)
        } else {
            None
        };

        // Refutability check: run the matrix with a single arm. If nothing
        // is missing, the pattern covers every value of the scrutinee — the
        // else branch is dead.
        let scrutinee_ty_for_matrix = self.matrix_normalize_scrut(&scrutinee_ty);
        let report = crate::pattern_lowering::compute_match_usefulness(
            self,
            std::slice::from_ref(&result.dpat),
            scrutinee_ty_for_matrix,
        );
        if report.missing.is_empty() {
            let err = crate::infer_context::TirTypeError::IrrefutablePatternInIfLet;
            if let Some(sm) = self.body_source_map.as_ref() {
                self.context
                    .report_warning_at_span(err, sm.pattern_span(pattern_id));
            } else {
                self.context.report_warning_simple(err, if_let_expr_id);
            }
        }

        match else_ty {
            Some(else_ty) => Self::join_types(&then_ty, &else_ty),
            None => Ty::Void {
                attr: TyAttr::default(),
            },
        }
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

        Ty::Bool {
            attr: TyAttr::default(),
        }
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
        self.validate_type_generic_bounds_at_span(span, &declared_ty);
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
        associated_type_bindings: &[baml_compiler2_ast::AssociatedTypeBinding],
        anchor: Option<(PatId, ExprId)>,
    ) -> Ty {
        if !generic_args.is_empty() || !associated_type_bindings.is_empty() {
            let ty_expr = TypeExpr::Path {
                segments: class.to_vec(),
                generic_args: generic_args.to_vec(),
                associated_type_bindings: associated_type_bindings.to_vec(),
                attrs: Vec::new(),
            };
            let ty = if let Some((pat_id, fallback)) = anchor {
                self.resolve_type_expr_at_pat(&ty_expr, pat_id, fallback)
            } else {
                self.resolve_type_expr_silent(&ty_expr)
            };
            if matches!(
                ty,
                Ty::Class(..) | Ty::Interface(..) | Ty::Unknown { .. } | Ty::Error { .. }
            ) {
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

        // Accept both class heads (`Dog { ... }`) and interface heads
        // (`Animal { ... }`). An interface destructure matches any implementor
        // and binds the interface's fields through their views — see the
        // interface branch in `lower_class_pat`.
        if let Some((_source, ty @ (Ty::Class(..) | Ty::Interface(..)))) = self
            .res_ctx
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
        // Fast path: an unspecialized generic carries its declaration's params as
        // non-rigid `TypeVar` args. The enclosing function's own rigid params
        // (e.g. `Err` in a thrown `AllFailed<Err>`) are bound, not missing, so
        // they fall through to the declaration-arity check below.
        if args
            .iter()
            .any(|a| crate::generics::contains_non_rigid_typevar(a, &self.generic_params))
        {
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
            Ty::Class(_, args, _) | Ty::Interface(_, args, _, _) | Ty::Union(args, _) => {
                args.iter().any(Self::ty_contains_recovery_unknown)
            }
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => {
                Self::ty_contains_recovery_unknown(base)
                    || interface
                        .as_ref()
                        .is_some_and(|interface| Self::ty_contains_recovery_unknown(interface))
            }
            Ty::List(elem, _) | Ty::EvolvingList(elem, _) => {
                Self::ty_contains_recovery_unknown(elem)
            }
            Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => {
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
            // `WatchAccessor` is never built by TIR; the arm exists only so the
            // match stays exhaustive over the shared `baml_type::Ty`.
            Ty::WatchAccessor(inner, _) => Self::ty_contains_recovery_unknown(inner),
            Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::TypeAlias(..)
            | Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Literal(..)
            | Ty::TypeVar(..)
            | Ty::Never { .. }
            | Ty::Void { .. }
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. } => false,
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
                associated_type_bindings,
                ..
            } => {
                let class_ty = self.resolve_class_pattern_type(
                    class,
                    generic_args,
                    associated_type_bindings,
                    None,
                );
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

        // A derived type is a useful bidirectional hint only when it is fully
        // resolved: not polluted by recovery `Unknown`, and not still carrying
        // an *unspecialized* generic. Declared generics live on the type as
        // `TypeVar` args now, so an unspecialized class shows up as a non-rigid
        // type var; the enclosing function's own rigid params (e.g. the `Err`
        // in `AllFailed<Err>`) are already bound and so don't disqualify it.
        if Self::ty_contains_recovery_unknown(&ty)
            || crate::generics::contains_non_rigid_typevar(&ty, &self.generic_params)
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
        self.lower_type_expr_in_current_body(ty, &mut diags)
    }

    fn lower_pattern_type_expr(&mut self, expr: &TypeExpr, at_expr: ExprId) -> Ty {
        let mut diags = Vec::new();
        let ty = self.lower_type_expr_in_current_body(expr, &mut diags);
        for diag in diags {
            self.context.report_simple(diag, at_expr);
        }
        self.validate_type_generic_bounds(at_expr, &ty);
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
        if let Some(sm) = self.body_source_map.as_ref() {
            self.validate_type_generic_bounds_at_span(sm.pattern_span(pat_id), &resolved);
        } else {
            self.validate_type_generic_bounds(fallback_expr, &resolved);
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
            Ty::Int { .. } => {
                matches!(
                    fact,
                    Ty::Int { .. } | Ty::Literal(baml_base::Literal::Int(_), _, _)
                )
            }
            Ty::Bigint { .. } => matches!(
                fact,
                Ty::Bigint { .. } | Ty::Literal(baml_base::Literal::Bigint(_), _, _)
            ),
            Ty::Float { .. } => matches!(
                fact,
                Ty::Float { .. } | Ty::Literal(baml_base::Literal::Float(_), _, _)
            ),
            Ty::String { .. } => matches!(
                fact,
                Ty::String { .. } | Ty::Literal(baml_base::Literal::String(_), _, _)
            ),
            Ty::Bool { .. } => matches!(
                fact,
                Ty::Bool { .. } | Ty::Literal(baml_base::Literal::Bool(_), _, _)
            ),
            Ty::Null { .. } => matches!(fact, Ty::Null { .. }),
            Ty::Uint8Array { .. } => matches!(fact, Ty::Uint8Array { .. }),
            Ty::Media(k, _) => matches!(fact, Ty::Media(fk, _) if k == fk),
            Ty::Literal(_, _, _) => false,
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
            Ty::Literal(lit, _, _) => match lit {
                baml_base::Literal::Int(_) => matches!(fact, Ty::Int { .. }),
                baml_base::Literal::Bigint(_) => matches!(fact, Ty::Bigint { .. }),
                baml_base::Literal::Float(_) => matches!(fact, Ty::Float { .. }),
                baml_base::Literal::String(_) => matches!(fact, Ty::String { .. }),
                baml_base::Literal::Bool(_) => matches!(fact, Ty::Bool { .. }),
            },
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

    /// The argument type used for throws instantiation. For a lambda argument
    /// whose EFFECTIVE throws is known (side table populated when the lambda
    /// body was checked), override the recorded surface throws with it: an
    /// omitted-throws lambda's surface is open (`Unknown`), which would leave
    /// the callee's throws generic unbound — `map`'s `E` would then leak as a
    /// raw `TypeVar` (a non-throwing callback) or stay symbolic where the body
    /// demonstrably throws. The effective value gives the truth either way:
    /// `Never` for a non-throwing callback, the thrown type otherwise.
    fn callee_throws_arg_ty(&self, arg_expr_id: ExprId) -> Ty {
        let arg_ty = self
            .expressions
            .get(&arg_expr_id)
            .cloned()
            .unwrap_or(Ty::Unknown {
                attr: TyAttr::default(),
            });
        if let Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attr,
        } = &arg_ty
            && let Some(effective) = self.lambda_effective_throws.get(&arg_expr_id)
            // Only an OPEN surface is replaced: an omitted-throws lambda's
            // surface is `Unknown`, and a lambda checked against an
            // effect-polymorphic callback param carries the unbound effect
            // TypeVar. An EXPLICIT (or contextual-concrete) `throws Foo | Bar`
            // is a deliberate contract wider than the current body facts —
            // narrowing it to today's effective set would instantiate a
            // higher-order `E` too tightly.
            && (matches!(throws.as_ref(), Ty::Unknown { .. })
                || crate::generics::contains_typevar(throws))
        {
            return Ty::Function {
                generic_params: generic_params.clone(),
                generic_param_bounds: generic_param_bounds.clone(),
                params: params.clone(),
                ret: ret.clone(),
                throws: Box::new(effective.clone()),
                attr: attr.clone(),
            };
        }
        arg_ty
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
                let arg_ty = self.callee_throws_arg_ty(arg_expr_id);
                crate::generics::infer_bindings_allow_typevars(&param.ty, &arg_ty, &mut bindings);
            }
        } else {
            for (param, arg_expr_id) in effective_params.iter().zip(args.iter()) {
                let arg_ty = self.callee_throws_arg_ty(*arg_expr_id);
                crate::generics::infer_bindings_allow_typevars(&param.ty, &arg_ty, &mut bindings);
            }
        }

        let substituted = crate::generics::substitute_ty(&throws, &bindings);
        Some(self.resolve_associated_projections_deep(&substituted))
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
            Expr::IfLet {
                scrutinee,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_throw_facts_from_expr(*scrutinee, body, out);
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
            Expr::MemberAccess { base, .. }
            | Expr::Upcast { base, .. }
            | Expr::OptionalMemberAccess { base, .. } => {
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
                with_exprs,
                body: spawn_body,
            } => {
                // Spawn-body throws do NOT escape the spawning function
                // — they are captured into the resulting `Future<T, E>`'s
                // E parameter and only re-thrown at an `await` site. The
                // name and `with` expressions are evaluated eagerly in the
                // spawning function, so their throws DO escape — walk them;
                // do not walk spawn_body.
                if let Some(name_id) = name {
                    self.collect_throw_facts_from_expr(*name_id, body, out);
                }
                for with_id in with_exprs {
                    self.collect_throw_facts_from_expr(*with_id, body, out);
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
            Expr::GenericApply { base, .. } => {
                // Referencing a generic callable as a value cannot throw; walk
                // the base for completeness (it is a path, a no-op).
                self.collect_throw_facts_from_expr(*base, body, out);
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
            Stmt::Let {
                initializer,
                else_branch,
                ..
            } => {
                if let Some(init) = initializer {
                    self.collect_throw_facts_from_expr(*init, body, out);
                }
                if let Some(else_expr) = else_branch {
                    self.collect_throw_facts_from_expr(*else_expr, body, out);
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
            Stmt::WhileLet {
                scrutinee,
                body: while_body,
                ..
            } => {
                self.collect_throw_facts_from_expr(*scrutinee, body, out);
                self.collect_throw_facts_from_expr(*while_body, body, out);
            }
            Stmt::For {
                collection,
                body: for_body,
                ..
            } => {
                self.collect_throw_facts_from_expr(*collection, body, out);
                if let Some(error_ty) = self
                    .expressions
                    .get(collection)
                    .and_then(|ty| self.iterable_associated_ty(ty, "Error"))
                    && !matches!(error_ty, Ty::Never { .. })
                {
                    out.insert(error_ty);
                }
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

    /// Whether `segments` is rooted at the BEP-044 `default` receiver keyword
    /// and that keyword is not shadowed by a local of the same name. See
    /// [`baml_compiler2_ast::DEFAULT_RECEIVER_KEYWORD`].
    fn is_default_receiver_root(&self, segments: &[Name]) -> bool {
        segments
            .first()
            .is_some_and(|s| s.as_str() == baml_compiler2_ast::DEFAULT_RECEIVER_KEYWORD)
            && !self.locals.contains_key(&segments[0])
    }

    fn infer_path(&mut self, segments: &[Name], _body: &ExprBody, expr_id: ExprId) -> Ty {
        // BEP-044: `default.<method>(...)` inside an `implements I { ... }`
        // block resolves to `I`'s default body. Treat `default` as a
        // syntactic alias for an `I`-typed receiver and chain the rest of
        // the path through the interface's member contract.
        if self.is_default_receiver_root(segments)
            && let Some(iface_qtn) = self.implements_block_interface.clone()
        {
            if segments.len() == 1 {
                // BEP-044 wf3: bare `default` as a value is not allowed; it is
                // only meaningful in call position (`default.method(...)`).
                self.context
                    .report(TirTypeError::BareDefaultKeyword, expr_id, Vec::new());
                return Ty::Unknown {
                    attr: TyAttr::default(),
                };
            }
            // BEP-044 §"default keyword scoping rules": `default.method()`
            // on a required method (no default body) is a compile error.
            if segments.len() == 2 {
                let method_name = &segments[1];
                if self.is_required_interface_method(&iface_qtn, method_name) {
                    self.context.report(
                        TirTypeError::DefaultOnRequiredMethod {
                            interface_name: iface_qtn.name().clone(),
                            method_name: method_name.clone(),
                        },
                        expr_id,
                        Vec::new(),
                    );
                }
            }
            // Multi-segment: thread the interface through the segment-
            // resolver so each suffix segment dispatches against `I`.
            let iface_ty = Ty::Interface(iface_qtn, Vec::new(), vec![], TyAttr::default());
            self.path_root_types.insert(expr_id, iface_ty.clone());
            let mut current_ty = iface_ty;
            for (idx, seg) in segments[1..].iter().enumerate() {
                let seg_idx = idx + 1;
                let bound_segment = seg_idx + 1 == segments.len();
                current_ty = self.resolve_member_for_path_segment(
                    &current_ty,
                    seg,
                    expr_id,
                    seg_idx,
                    bound_segment,
                );
                self.path_segment_types
                    .insert((expr_id, seg_idx), current_ty.clone());
            }
            return current_ty;
        }
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
                        "bigint" => &["Bigint"],
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
            let is_pure_null = matches!(current_ty, Ty::Null { .. });
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
                    current_ty = Ty::optional(member_ty.clone());
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
                    current_ty = Ty::optional(member_ty.clone());
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
            let generic_param_bounds = lower_generic_param_bounds(
                db,
                &function_generic_param_bounds_exprs(db, func_loc),
                pkg_items,
                &ns_context,
                &function_generic_params,
                None,
                &mut diags,
            );
            let ty = Ty::Function {
                generic_params: sig.user_generic_params.clone(),
                generic_param_bounds,
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
                Definition::Interface(_) => {
                    let iface_qtn = crate::lower_type_expr::qualify_def(db, def, name);
                    return Some(Ty::Interface(iface_qtn, vec![], vec![], TyAttr::default()));
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
                Ty::EvolvingMap(k, v, attr) => Ty::Map {
                    key: k.clone(),
                    value: v.clone(),
                    attr: attr.clone(),
                },
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
                    let generic_param_bounds = lower_generic_param_bounds(
                        db,
                        &function_generic_param_bounds_exprs(db, func_loc),
                        self.package_items,
                        &sig_ns,
                        &function_generic_params,
                        None,
                        &mut diags,
                    );

                    // Note: diags from referenced function signatures are not
                    // reported here — they'll be reported at the definition site.
                    Ty::Function {
                        generic_params: sig.user_generic_params.clone(),
                        generic_param_bounds,
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
                Definition::Interface(_) => Ty::Interface(
                    crate::lower_type_expr::qualify_def(db, def, name),
                    vec![],
                    vec![],
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
    /// For builtin container types (`Ty::List`, `Ty::Map`) and `Ty::String { attr: TyAttr::default() }`,
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

                let field_sources = self.class_interface_field_sources(class_name, member);
                // A class's own method of the same name wins over an aliased
                // interface *field view* — `p.name()` calls the method even when
                // an `implements I { name as _name }` view also exists. Only
                // surface the "needs projection" error when there's no such
                // method to fall through to.
                if let [interface_name] = field_sources.as_slice()
                    && self
                        .lookup_class_method(class_name, type_args, member)
                        .is_none()
                {
                    self.context.report_at_member(
                        TirTypeError::InterfaceFieldRequiresProjection {
                            class_name: class_name.name().clone(),
                            field_name: member.clone(),
                            interface_name: interface_name.clone(),
                        },
                        at,
                        Vec::new(),
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
                }
                if field_sources.len() > 1 {
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
                        TirTypeError::AmbiguousInterfaceField {
                            class_name: class_name.name().clone(),
                            field_name: member.clone(),
                            sources: field_sources,
                        },
                        at,
                        related,
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
                }

                // BEP-044 §"Method Disambiguation": if more than one
                // implemented interface contributes a method of this name —
                // whether overridden in an `implements` block, inherited as a
                // default, or declared as a required method — the unqualified
                // call site is ambiguous. Emit E0121 listing every contributing
                // interface. The class declaration itself is allowed; only the
                // call errors.
                // Render each contributing interface with its namespace
                // (e.g. `zoo.Animal`) so two same-simple-name interfaces from
                // different namespaces are distinguishable and the suggested
                // `as<…>` projection actually resolves. Root-namespace
                // interfaces stay bare (`Named`).
                let method_sources: Vec<String> = self.format_interface_method_sources(
                    self.implemented_interface_method_sources(class_name, type_args, member)
                        .into_iter()
                        .map(|(_, qtn, args)| (qtn, args)),
                );
                if method_sources.len() >= 2 {
                    let sources = method_sources;
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
                        TirTypeError::AmbiguousInterfaceMethod {
                            class_name: class_name.name().clone(),
                            method_name: member.clone(),
                            sources,
                        },
                        at,
                        related,
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
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
                            generic_params,
                            generic_param_bounds,
                            params,
                            ret,
                            throws,
                            attr,
                        } = ty
                        {
                            let stripped_params =
                                crate::generics::skip_self_param(&params).to_vec();
                            return Ty::Function {
                                generic_params,
                                generic_param_bounds,
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

                // BEP-044: a default method inherited via an empty (or partial)
                // `implements I {}` block is callable directly on the concrete
                // class. The method isn't in `class_data.methods` (only
                // overrides are), so resolve it through the interface machinery
                // — exactly as `obj.as<I>.method()` would — which records an
                // `InterfaceDefaultMethod` resolution and dispatches to the
                // default body. Ambiguity (≥2 sources) was already rejected
                // above, so at most one source remains here.
                if let [(_, iface_qtn, iface_args)] = self
                    .implemented_interface_method_sources(class_name, type_args, member)
                    .as_slice()
                {
                    let iface_qtn = iface_qtn.clone();
                    let iface_args = iface_args.clone();
                    if let Some(ty) = self.resolve_interface_member(InterfaceMemberLookup {
                        iface_name: &iface_qtn,
                        iface_type_args: &iface_args,
                        associated_bindings: &[],
                        member,
                        at,
                        bound,
                        receiver_projection_base: Some(base_ty),
                        self_recv: SelfReceiver::ExactTy(base_ty),
                    }) {
                        return ty;
                    }
                }

                // BEP-044 wf3 #G7: blanket / out-of-body impls aren't in the
                // class's in-body `implements` blocks, so a direct
                // `obj.method()` call misses them above. Consult the registry.
                if let Some(ty) =
                    self.try_registry_member(base_ty, class_name.name().clone(), member, at, bound)
                {
                    return ty;
                }

                if self
                    .lookup_implemented_interface_by_name(class_name, member)
                    .is_some()
                {
                    let as_target = self.implemented_interface_display(class_name, member);
                    self.context.report_at_member(
                        TirTypeError::DeprecatedInterfaceProjection {
                            interface_name: member.clone(),
                            as_target,
                        },
                        at,
                        Vec::new(),
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
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
            Ty::Interface(iface_name, type_args, associated_bindings, _) => {
                if let Some(ty) = self.resolve_interface_member(InterfaceMemberLookup {
                    iface_name,
                    iface_type_args: type_args,
                    associated_bindings,
                    member,
                    at,
                    bound,
                    receiver_projection_base: Some(base_ty),
                    self_recv: SelfReceiver::Existential,
                }) {
                    return ty;
                }
                // Known interface but member not found — error.
                let iface_def = self
                    .package_items
                    .lookup_type(iface_name.namespace(), iface_name.name());
                let related = iface_def
                    .map(|def| {
                        vec![RelatedNote::new(
                            RelatedLocation::Item(def),
                            "interface defined here",
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
                        generic_params: Vec::new(),
                        generic_param_bounds: Vec::new(),
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
                    .or_else(|| {
                        self.try_registry_member(base_ty, Name::new("array"), member, at, bound)
                    })
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
            Ty::Map {
                key: key_ty,
                value: val_ty,
                ..
            } => {
                // Bridge: map<string, int> → Map<string, int>
                self.resolve_builtin_member(
                    &["Map"],
                    &[key_ty.as_ref().clone(), val_ty.as_ref().clone()],
                    member,
                    at,
                )
                .or_else(|| self.try_registry_member(base_ty, Name::new("map"), member, at, bound))
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
                .or_else(|| {
                    self.try_registry_member(base_ty, Name::new("future"), member, at, bound)
                })
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
            Ty::String { .. } | Ty::Literal(baml_base::Literal::String(_), _, _) => {
                // Bridge: string / string-literal → String class
                self.resolve_builtin_member(&["String"], &[], member, at)
                    .or_else(|| {
                        self.try_registry_member(base_ty, Name::new("string"), member, at, bound)
                    })
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
            Ty::Int { .. } | Ty::Literal(baml_base::Literal::Int(_), _, _) => self
                .resolve_builtin_member(&["Int"], &[], member, at)
                .or_else(|| self.try_registry_member(base_ty, Name::new("int"), member, at, bound))
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
            // bigint literal / bigint → Bigint companion class
            Ty::Bigint { .. } | Ty::Literal(baml_base::Literal::Bigint(_), _, _) => self
                .resolve_builtin_member(&["Bigint"], &[], member, at)
                .or_else(|| {
                    self.try_registry_member(base_ty, Name::new("bigint"), member, at, bound)
                })
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
            Ty::Float { .. } | Ty::Literal(baml_base::Literal::Float(_), _, _) => self
                .resolve_builtin_member(&["Float"], &[], member, at)
                .or_else(|| {
                    self.try_registry_member(base_ty, Name::new("float"), member, at, bound)
                })
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
            Ty::Bool { .. } | Ty::Literal(baml_base::Literal::Bool(_), _, _) => self
                .resolve_builtin_member(&["Bool"], &[], member, at)
                .or_else(|| self.try_registry_member(base_ty, Name::new("bool"), member, at, bound))
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
            Ty::Null { .. } => self
                .resolve_builtin_member(&["Null"], &[], member, at)
                .or_else(|| self.try_registry_member(base_ty, Name::new("null"), member, at, bound))
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
                    .or_else(|| {
                        self.try_registry_member(base_ty, Name::new("type"), member, at, bound)
                    })
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
            Ty::Uint8Array { .. } | Ty::Media(_, _) => {
                // Bridge: media / binary primitives with builtin companion classes
                let p = match base_ty {
                    Ty::Uint8Array { .. } => PrimitiveType::Uint8Array,
                    Ty::Media(MediaKind::Image, _) => PrimitiveType::Image,
                    Ty::Media(MediaKind::Audio, _) => PrimitiveType::Audio,
                    Ty::Media(MediaKind::Video, _) => PrimitiveType::Video,
                    Ty::Media(MediaKind::Pdf, _) => PrimitiveType::Pdf,
                    _ => unreachable!("matched Uint8Array or Media above"),
                };
                self.resolve_builtin_member(p.builtin_class_path(), &[], member, at)
                    .or_else(|| {
                        self.try_registry_member(base_ty, Name::new(p.alias()), member, at, bound)
                    })
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
            // BEP-044 generic bound: when `T extends I` is in scope and
            // the member isn't a universal builtin (`to_json` /
            // `from_json`), delegate to `I`'s contract.
            Ty::TypeVar(name, _)
                if member.as_str() != "to_json"
                    && member.as_str() != "from_json"
                    && self.generic_param_bounds.contains_key(name) =>
            {
                let bound_ty = self.generic_param_bounds[name].clone();
                // The receiver is a single concrete type (the type variable),
                // so an interface-bound member resolves with `Self` pinned to
                // that variable — `Self`-typed parameters are sound here. This
                // is what lets a generic `T extends Equals` (and an interface's
                // own `self`) call `Self`-param methods, while a bare interface
                // (existential) receiver still cannot.
                if let Ty::Interface(iface_qtn, iface_args, associated_bindings, _) = &bound_ty {
                    let receiver_ty = Ty::TypeVar(name.clone(), TyAttr::default());
                    if let Some(ty) = self.resolve_interface_member(InterfaceMemberLookup {
                        iface_name: iface_qtn,
                        iface_type_args: iface_args,
                        associated_bindings,
                        member,
                        at,
                        bound,
                        receiver_projection_base: Some(&receiver_ty),
                        self_recv: SelfReceiver::RigidVar(name),
                    }) {
                        return ty;
                    }
                }
                self.resolve_member(&bound_ty, member, at, bound)
            }
            Ty::TypeVar(_, _) if member.as_str() == "to_json" => {
                // Type-check: every BAML type has `to_json(self) -> json` after Phase 5b.1.
                // No MemberResolution stored — the concrete dispatch is deferred to Phase 5b.4
                // (native Array/Map impls) and never runs with an unresolved TypeVar at runtime.
                Ty::Function {
                    generic_params: Vec::new(),
                    generic_param_bounds: Vec::new(),
                    params: vec![],
                    ret: Box::new(json_alias_ty()),
                    throws: Box::new(json_serialization_or_parse_error_ty()),
                    attr: TyAttr::default(),
                }
            }
            Ty::TypeVar(name, _) if member.as_str() == "from_json" => {
                // Type-check: every BAML type has `from_json(j: json) -> Self` after Phase 5b.1.
                Ty::Function {
                    generic_params: Vec::new(),
                    generic_param_bounds: Vec::new(),
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
                // A shared implementor of two of the union's interfaces makes an
                // unqualified method call ambiguous (E0121) — reject it instead
                // of silently picking the first interface's default.
                if let Some((class_name, sources)) =
                    self.union_interface_method_ambiguity(&members, member)
                {
                    self.context.report_at_member(
                        TirTypeError::AmbiguousInterfaceMethod {
                            class_name,
                            method_name: member.clone(),
                            sources,
                        },
                        at,
                        Vec::new(),
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
                }
                let mut resolved: Vec<(Ty, Option<Ty>)> = Vec::with_capacity(members.len());
                for m in &members {
                    let r = self.resolve_union_member_ty(m, member, at, bound);
                    resolved.push((m.clone(), r));
                }

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
            Ty::AssociatedTypeProjection { .. } => {
                let resolved = self.resolve_associated_projections_deep(base_ty);
                if &resolved != base_ty {
                    return self.resolve_member(&resolved, member, at, bound);
                }
                let projection_resolver =
                    crate::associated_projection::AssociatedProjectionResolver::with_resolution_context(
                        self.context.db(),
                        self.res_ctx,
                        &self.aliases,
                        &self.generic_param_bounds,
                    );
                if let Some(projection_bound) =
                    projection_resolver.resolve_projection_bound(base_ty)
                {
                    if &projection_bound != base_ty {
                        if let Ty::Interface(iface_qtn, iface_args, associated_bindings, _) =
                            &projection_bound
                        {
                            if let Some(ty) = self.resolve_interface_member(InterfaceMemberLookup {
                                iface_name: iface_qtn,
                                iface_type_args: iface_args,
                                associated_bindings,
                                member,
                                at,
                                bound,
                                receiver_projection_base: Some(base_ty),
                                self_recv: SelfReceiver::ExactTy(base_ty),
                            }) {
                                return ty;
                            }
                        } else {
                            return self.resolve_member(&projection_bound, member, at, bound);
                        }
                    }
                }
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

    fn types_equivalent(&self, a: &Ty, b: &Ty) -> bool {
        crate::associated_projection::AssociatedProjectionResolver::with_resolution_context(
            self.context.db(),
            self.res_ctx,
            &self.aliases,
            &self.generic_param_bounds,
        )
        .types_equivalent(a, b)
    }

    fn interface_type_with_default_associated_bindings(&self, ty: Ty) -> Ty {
        let Ty::Interface(iface_qtn, iface_args, associated_bindings, attr) = ty else {
            return ty;
        };
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_qtn.package()) else {
            return Ty::Interface(iface_qtn, iface_args, associated_bindings, attr);
        };
        let Some(Definition::Interface(iface_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return Ty::Interface(iface_qtn, iface_args, associated_bindings, attr);
        };
        let db = self.context.db();
        let pkg = baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
        let completed = crate::interfaces::interface_closure_locs_with_args_and_assoc(
            db,
            iface_loc,
            &iface_args,
            &associated_bindings,
            pkg_items,
            &pkg.namespace_path,
        )
        .into_iter()
        .next()
        .map(|(_, _, assoc)| assoc)
        .unwrap_or_else(|| associated_bindings.clone());
        Ty::Interface(iface_qtn, iface_args, completed, attr)
    }

    fn interface_bound_with_self_associated_bindings(
        &self,
        self_name: &Name,
        bound: &Ty,
    ) -> Option<Ty> {
        let Ty::Interface(iface_qtn, iface_args, associated_bindings, attr) = bound else {
            return None;
        };
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_qtn.package()) else {
            return Some(bound.clone());
        };
        let Some(Definition::Interface(iface_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return Some(bound.clone());
        };
        let db = self.context.db();
        let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
        let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
            return Some(bound.clone());
        };
        let mut completed = associated_bindings.clone();
        for assoc in &iface_data.associated_types {
            if completed.iter().any(|(name, _)| name == &assoc.name) {
                continue;
            }
            completed.push((
                assoc.name.clone(),
                Ty::AssociatedTypeProjection {
                    base: Box::new(Ty::TypeVar(self_name.clone(), TyAttr::default())),
                    interface: None,
                    member: assoc.name.clone(),
                    attr: TyAttr::default(),
                },
            ));
        }
        Some(Ty::Interface(
            iface_qtn.clone(),
            iface_args.clone(),
            completed,
            attr.clone(),
        ))
    }

    fn interface_requires_instantiation(
        &self,
        sub_qtn: &crate::ty::QualifiedTypeName,
        sub_args: &[Ty],
        sub_associated_bindings: &[(Name, Ty)],
        sup_qtn: &crate::ty::QualifiedTypeName,
        sup_args: &[Ty],
        sup_associated_bindings: &[(Name, Ty)],
    ) -> bool {
        let Some(pkg_items) = self.resolve_class_pkg_items(sub_qtn.package()) else {
            return false;
        };
        let Some(Definition::Interface(sub_loc)) =
            pkg_items.lookup_type(sub_qtn.namespace(), sub_qtn.name())
        else {
            return false;
        };
        let db = self.context.db();
        for (iface_loc, iface_args, iface_assoc) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                db,
                sub_loc,
                sub_args,
                sub_associated_bindings,
                pkg_items,
                sub_qtn.namespace(),
            )
        {
            let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
            let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                continue;
            };
            let iface_qtn = crate::lower_type_expr::qualify_def(
                db,
                Definition::Interface(iface_loc),
                &iface_data.name,
            );
            if &iface_qtn == sup_qtn
                && iface_args.len() == sup_args.len()
                && iface_args
                    .iter()
                    .zip(sup_args.iter())
                    .all(|(a, b)| self.types_equivalent(a, b))
                && sup_associated_bindings.iter().all(|(sup_name, sup_ty)| {
                    iface_assoc
                        .iter()
                        .find(|(iface_name, _)| iface_name == sup_name)
                        .is_some_and(|(_, iface_ty)| self.types_equivalent(iface_ty, sup_ty))
                })
            {
                return true;
            }
        }
        false
    }

    fn interface_view_in_requires_closure(
        &self,
        root_interface_ty: &Ty,
        target_qtn: &crate::ty::QualifiedTypeName,
    ) -> Option<Ty> {
        let Ty::Interface(root_qtn, root_args, root_associated_bindings, _) = root_interface_ty
        else {
            return None;
        };
        let pkg_items = self.resolve_class_pkg_items(root_qtn.package())?;
        let Some(Definition::Interface(root_loc)) =
            pkg_items.lookup_type(root_qtn.namespace(), root_qtn.name())
        else {
            return None;
        };
        let db = self.context.db();
        let pkg = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db));
        for (iface_loc, iface_args, iface_associated_bindings) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                db,
                root_loc,
                root_args,
                root_associated_bindings,
                pkg_items,
                &pkg.namespace_path,
            )
        {
            let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
            let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                continue;
            };
            let iface_qtn = crate::lower_type_expr::qualify_def(
                db,
                Definition::Interface(iface_loc),
                &iface_data.name,
            );
            if &iface_qtn == target_qtn {
                return Some(Ty::Interface(
                    iface_qtn,
                    iface_args,
                    iface_associated_bindings,
                    TyAttr::default(),
                ));
            }
        }
        None
    }

    fn merge_interface_inference_candidate(&self, current: &mut Option<Ty>, candidate: Ty) -> bool {
        match current {
            Some(existing) => self.types_equivalent(existing, &candidate),
            slot @ None => {
                *slot = Some(candidate);
                true
            }
        }
    }

    /// View `actual_ty` through the concrete interface instantiation required
    /// by `formal_ty`.
    ///
    /// Generic call inference normally matches parameter and argument types by
    /// structure. Interface implementations are nominal, so a concrete class
    /// like `ArrayIterator<int>` must first be viewed as its implemented
    /// `Iterator<int, never>` before a formal `Iterator<T, E>` can bind
    /// `T = int` and `E = never`.
    fn actual_interface_view_for_formal(&self, formal_ty: &Ty, actual_ty: &Ty) -> Option<Ty> {
        let formal_ty = self.interface_type_with_default_associated_bindings(
            self.expand_alias_chains(formal_ty.clone()),
        );
        let Ty::Interface(formal_qtn, _, _, _) = &formal_ty else {
            return None;
        };
        let actual_ty = self.expand_alias_chains(actual_ty.clone());
        let mut candidate = None;

        if let Some(view) = self.interface_view_in_requires_closure(&actual_ty, formal_qtn)
            && !self.merge_interface_inference_candidate(&mut candidate, view)
        {
            return None;
        }

        let db = self.context.db();
        for pkg_id in
            self.registry_packages_for_interface_lookup(Some(&actual_ty), Some(formal_qtn))
        {
            let registry = crate::interfaces::package_implements_registry(db, pkg_id);
            for rule in &registry.interface_impl_rules {
                let Some(bindings) = crate::interfaces::match_ty_pattern(
                    &rule.for_ty_pattern,
                    &actual_ty,
                    &rule.generic_params,
                    &self.aliases,
                ) else {
                    continue;
                };
                let implemented_iface =
                    crate::generics::substitute_ty(&rule.interface_ty, &bindings);
                let Some(view) =
                    self.interface_view_in_requires_closure(&implemented_iface, formal_qtn)
                else {
                    continue;
                };
                if !registry.type_implements_interface_via_rule(
                    &actual_ty,
                    &implemented_iface,
                    &self.aliases,
                    |actual, bound| self.is_subtype(actual, bound),
                ) {
                    continue;
                }
                if !self.merge_interface_inference_candidate(&mut candidate, view) {
                    return None;
                }
            }
        }

        candidate
    }

    fn infer_call_bindings_rigid_self(
        &self,
        formal: &Ty,
        actual: &Ty,
        bindings: &mut FxHashMap<Name, Ty>,
        rigid: Option<&Name>,
    ) {
        crate::generics::infer_bindings_rigid_self(formal, actual, bindings, rigid);
        self.infer_call_bindings_via_interface_views_rigid(formal, actual, bindings, rigid);
    }

    fn infer_call_bindings_allow_typevars(
        &self,
        formal: &Ty,
        actual: &Ty,
        bindings: &mut FxHashMap<Name, Ty>,
    ) {
        crate::generics::infer_bindings_allow_typevars(formal, actual, bindings);
        self.infer_call_bindings_via_interface_views_allow_typevars(formal, actual, bindings);
    }

    fn infer_call_bindings_via_interface_views_rigid(
        &self,
        formal: &Ty,
        actual: &Ty,
        bindings: &mut FxHashMap<Name, Ty>,
        rigid: Option<&Name>,
    ) {
        if let Some(view) = self.actual_interface_view_for_formal(formal, actual) {
            crate::generics::infer_bindings_rigid_self(formal, &view, bindings, rigid);
            self.infer_call_bindings_via_matching_shape(formal, &view, bindings, rigid, false);
        }
        self.infer_call_bindings_via_matching_shape(formal, actual, bindings, rigid, false);
    }

    fn infer_call_bindings_via_interface_views_allow_typevars(
        &self,
        formal: &Ty,
        actual: &Ty,
        bindings: &mut FxHashMap<Name, Ty>,
    ) {
        if let Some(view) = self.actual_interface_view_for_formal(formal, actual) {
            crate::generics::infer_bindings_allow_typevars(formal, &view, bindings);
            self.infer_call_bindings_via_matching_shape(formal, &view, bindings, None, true);
        }
        self.infer_call_bindings_via_matching_shape(formal, actual, bindings, None, true);
    }

    fn nullable_non_null_part(ty: &Ty) -> Option<Ty> {
        let Ty::Union(members, attr) = ty else {
            return None;
        };
        if !members.iter().any(Ty::is_null) {
            return None;
        }
        let non_null: Vec<Ty> = members
            .iter()
            .filter(|member| !member.is_null())
            .cloned()
            .collect();
        match non_null.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(Ty::Union(non_null, attr.clone())),
        }
    }

    fn infer_call_bindings_via_matching_shape(
        &self,
        formal: &Ty,
        actual: &Ty,
        bindings: &mut FxHashMap<Name, Ty>,
        rigid: Option<&Name>,
        allow_typevar_actuals: bool,
    ) {
        match (formal, actual) {
            (Ty::List(f, _), Ty::List(a, _)) | (Ty::EvolvingList(f, _), Ty::EvolvingList(a, _)) => {
                if allow_typevar_actuals {
                    self.infer_call_bindings_via_interface_views_allow_typevars(f, a, bindings);
                } else {
                    self.infer_call_bindings_via_interface_views_rigid(f, a, bindings, rigid);
                }
            }
            (
                Ty::Map {
                    key: fk, value: fv, ..
                },
                Ty::Map {
                    key: ak, value: av, ..
                },
            )
            | (Ty::EvolvingMap(fk, fv, _), Ty::EvolvingMap(ak, av, _)) => {
                if allow_typevar_actuals {
                    self.infer_call_bindings_via_interface_views_allow_typevars(fk, ak, bindings);
                    self.infer_call_bindings_via_interface_views_allow_typevars(fv, av, bindings);
                } else {
                    self.infer_call_bindings_via_interface_views_rigid(fk, ak, bindings, rigid);
                    self.infer_call_bindings_via_interface_views_rigid(fv, av, bindings, rigid);
                }
            }
            (Ty::Future(fv, fe, _), Ty::Future(av, ae, _)) => {
                if allow_typevar_actuals {
                    self.infer_call_bindings_via_interface_views_allow_typevars(fv, av, bindings);
                    self.infer_call_bindings_via_interface_views_allow_typevars(fe, ae, bindings);
                } else {
                    self.infer_call_bindings_via_interface_views_rigid(fv, av, bindings, rigid);
                    self.infer_call_bindings_via_interface_views_rigid(fe, ae, bindings, rigid);
                }
            }
            (
                Ty::Function {
                    params: fp,
                    ret: fr,
                    throws: fth,
                    ..
                },
                Ty::Function {
                    params: ap,
                    ret: ar,
                    throws: ath,
                    ..
                },
            ) => {
                for (fp, ap) in fp.iter().zip(ap.iter()) {
                    if allow_typevar_actuals {
                        self.infer_call_bindings_via_interface_views_allow_typevars(
                            &fp.ty, &ap.ty, bindings,
                        );
                    } else {
                        self.infer_call_bindings_via_interface_views_rigid(
                            &fp.ty, &ap.ty, bindings, rigid,
                        );
                    }
                }
                if allow_typevar_actuals {
                    self.infer_call_bindings_via_interface_views_allow_typevars(fr, ar, bindings);
                    self.infer_call_bindings_via_interface_views_allow_typevars(fth, ath, bindings);
                } else {
                    self.infer_call_bindings_via_interface_views_rigid(fr, ar, bindings, rigid);
                    self.infer_call_bindings_via_interface_views_rigid(fth, ath, bindings, rigid);
                }
            }
            (Ty::Union(_, _), _) if Self::nullable_non_null_part(formal).is_some() => {
                let formal_inner = Self::nullable_non_null_part(formal).expect("checked above");
                let actual_inner =
                    Self::nullable_non_null_part(actual).unwrap_or_else(|| actual.clone());
                if allow_typevar_actuals {
                    self.infer_call_bindings_via_interface_views_allow_typevars(
                        &formal_inner,
                        &actual_inner,
                        bindings,
                    );
                } else {
                    self.infer_call_bindings_via_interface_views_rigid(
                        &formal_inner,
                        &actual_inner,
                        bindings,
                        rigid,
                    );
                }
            }
            (Ty::Union(f_members, _), Ty::Union(a_members, _))
                if f_members.len() == a_members.len() =>
            {
                for (formal_member, actual_member) in f_members.iter().zip(a_members.iter()) {
                    if allow_typevar_actuals {
                        self.infer_call_bindings_via_interface_views_allow_typevars(
                            formal_member,
                            actual_member,
                            bindings,
                        );
                    } else {
                        self.infer_call_bindings_via_interface_views_rigid(
                            formal_member,
                            actual_member,
                            bindings,
                            rigid,
                        );
                    }
                }
            }
            (Ty::Class(f_name, f_args, _), Ty::Class(a_name, a_args, _)) if f_name == a_name => {
                for (formal_arg, actual_arg) in f_args.iter().zip(a_args.iter()) {
                    if allow_typevar_actuals {
                        self.infer_call_bindings_via_interface_views_allow_typevars(
                            formal_arg, actual_arg, bindings,
                        );
                    } else {
                        self.infer_call_bindings_via_interface_views_rigid(
                            formal_arg, actual_arg, bindings, rigid,
                        );
                    }
                }
            }
            (
                Ty::Interface(f_name, f_args, f_assoc, _),
                Ty::Interface(a_name, a_args, a_assoc, _),
            ) if f_name == a_name => {
                for (formal_arg, actual_arg) in f_args.iter().zip(a_args.iter()) {
                    if allow_typevar_actuals {
                        self.infer_call_bindings_via_interface_views_allow_typevars(
                            formal_arg, actual_arg, bindings,
                        );
                    } else {
                        self.infer_call_bindings_via_interface_views_rigid(
                            formal_arg, actual_arg, bindings, rigid,
                        );
                    }
                }
                for (formal_name, formal_ty) in f_assoc {
                    let Some((_, actual_ty)) = a_assoc
                        .iter()
                        .find(|(actual_name, _)| actual_name == formal_name)
                    else {
                        continue;
                    };
                    if allow_typevar_actuals {
                        self.infer_call_bindings_via_interface_views_allow_typevars(
                            formal_ty, actual_ty, bindings,
                        );
                    } else {
                        self.infer_call_bindings_via_interface_views_rigid(
                            formal_ty, actual_ty, bindings, rigid,
                        );
                    }
                }
            }
            _ => {}
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
                // BEP-044 wf3 #G7: blanket / out-of-body impls aren't class
                // members, but a direct `obj.method()` should still resolve.
                // resolve_member's registry fallback handles single/ambiguous.
                if !self
                    .registry_interface_method_sources(base_ty, member)
                    .is_empty()
                {
                    return self.resolve_member(base_ty, member, path_id, bound);
                }
                if self
                    .lookup_implemented_interface_by_name(class_name, member)
                    .is_some()
                {
                    let as_target = self.implemented_interface_display(class_name, member);
                    self.context.report_at_segment(
                        TirTypeError::DeprecatedInterfaceProjection {
                            interface_name: member.clone(),
                            as_target,
                        },
                        path_id,
                        seg_idx,
                        Vec::new(),
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
                }
                let field_sources = self.class_interface_field_sources(class_name, member);
                if let [interface_name] = field_sources.as_slice() {
                    self.context.report_at_segment(
                        TirTypeError::InterfaceFieldRequiresProjection {
                            class_name: class_name.name().clone(),
                            field_name: member.clone(),
                            interface_name: interface_name.clone(),
                        },
                        path_id,
                        seg_idx,
                        Vec::new(),
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
                }
                if field_sources.len() > 1 {
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
                        TirTypeError::AmbiguousInterfaceField {
                            class_name: class_name.name().clone(),
                            field_name: member.clone(),
                            sources: field_sources,
                        },
                        path_id,
                        seg_idx,
                        related,
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
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
            Ty::Interface(iface_name, type_args, associated_bindings, _) => {
                if let Some(ty) = self.resolve_interface_member(InterfaceMemberLookup {
                    iface_name,
                    iface_type_args: type_args,
                    associated_bindings,
                    member,
                    at: path_id,
                    bound,
                    receiver_projection_base: Some(base_ty),
                    self_recv: SelfReceiver::Existential,
                }) {
                    return ty;
                }
                let iface_def = self
                    .package_items
                    .lookup_type(iface_name.namespace(), iface_name.name());
                let related = iface_def
                    .map(|def| {
                        vec![RelatedNote::new(
                            RelatedLocation::Item(def),
                            "interface defined here",
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
                        generic_params: Vec::new(),
                        generic_param_bounds: Vec::new(),
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
                // A shared implementor of two of the union's interfaces makes an
                // unqualified method call ambiguous (E0121) — reject it instead
                // of silently picking the first interface's default.
                if let Some((class_name, sources)) =
                    self.union_interface_method_ambiguity(&members, member)
                {
                    self.context.report_at_segment(
                        TirTypeError::AmbiguousInterfaceMethod {
                            class_name,
                            method_name: member.clone(),
                            sources,
                        },
                        path_id,
                        seg_idx,
                        Vec::new(),
                    );
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
                }
                let mut resolved: Vec<(Ty, Option<Ty>)> = Vec::with_capacity(members.len());
                for m in &members {
                    let r = self.resolve_union_member_ty(m, member, path_id, bound);
                    resolved.push((m.clone(), r));
                }

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

    /// Conservative ambiguity check for accessing `member` (a method) on a union
    /// whose members include interfaces. If some class implements two or more of
    /// the union's interfaces and `member` is declared by ≥2 of them, then a
    /// value of the union could be that class — for which a direct
    /// `value.member()` is rejected as ambiguous (E0121). The static union type
    /// admits such a class, so the call is genuinely ambiguous; report it rather
    /// than silently dispatching to the first interface's default.
    ///
    /// Returns `(class_name, formatted_sources)` for the E0121 diagnostic, or
    /// `None` when no shared implementor makes the call ambiguous.
    fn union_interface_method_ambiguity(
        &self,
        members: &[Ty],
        member: &Name,
    ) -> Option<(Name, Vec<String>)> {
        let iface_qtns: FxHashSet<crate::ty::QualifiedTypeName> = members
            .iter()
            .filter_map(|m| match m {
                Ty::Interface(qtn, _, _, _) => Some(qtn.clone()),
                _ => None,
            })
            .collect();
        // A single interface can't have a cross-member shared-implementor clash.
        if iface_qtns.len() < 2 {
            return None;
        }
        let db = self.context.db();
        let registry = crate::interfaces::package_implements_registry(db, self.package_id);
        let mut seen: FxHashSet<crate::ty::QualifiedTypeName> = FxHashSet::default();
        for rule in &registry.interface_impl_rules {
            let Ty::Interface(rule_iface, _, _, _) = &rule.interface_ty else {
                continue;
            };
            if !iface_qtns.contains(rule_iface) {
                continue;
            }
            let Ty::Class(class_qtn, _, _) = &rule.for_ty_pattern else {
                continue;
            };
            if !seen.insert(class_qtn.clone()) {
                continue;
            }
            // Which of THIS union's interfaces declare `member` on the class.
            let sources: Vec<(crate::ty::QualifiedTypeName, Vec<Ty>)> = self
                .implemented_interface_method_sources(class_qtn, &[], member)
                .into_iter()
                .filter(|(_, qtn, _)| iface_qtns.contains(qtn))
                .map(|(_, qtn, args)| (qtn, args))
                .collect();
            if sources.len() >= 2 {
                return Some((
                    class_qtn.name().clone(),
                    self.format_interface_method_sources(sources.into_iter()),
                ));
            }
        }
        None
    }

    /// Resolve `member` on one constituent `m` of a union receiver.
    ///
    /// Interface members can't be resolved by the read-only
    /// [`try_resolve_member_on_ty`](Self::try_resolve_member_on_ty) probe — it
    /// returns the `Ty::Unknown` sentinel, which makes a method present on
    /// every member collapse into a non-callable `unknown | unknown` union
    /// (BEP-044 union dispatch, formerly E0006). Resolve interface members
    /// through the full interface machinery instead, but suppress the
    /// diagnostics and the single-interface `MemberResolution` it records: a
    /// union method/field access dispatches on the runtime class (see MIR
    /// `try_lower_union_iface_dispatch` / `try_lower_interface_field_access`),
    /// not a stored per-member resolution.
    fn resolve_union_member_ty(
        &mut self,
        m: &Ty,
        member: &Name,
        at: ExprId,
        bound: bool,
    ) -> Option<Ty> {
        if let Ty::Interface(iface_name, type_args, associated_bindings, _) = m {
            let iface_name = iface_name.clone();
            let type_args = type_args.clone();
            let associated_bindings = associated_bindings.clone();
            let self_ty = Ty::Interface(
                iface_name.clone(),
                type_args.clone(),
                associated_bindings.clone(),
                TyAttr::default(),
            );
            // A union member that is a bare interface is an existential ("dyn")
            // receiver, so a method with an extra `Self` parameter is not callable
            // on it (object safety). `resolve_interface_member` reports this, but
            // the call below suppresses its diagnostics — so surface it here,
            // outside the suppressed region (mirrors `union_interface_method_ambiguity`).
            if bound && self.interface_method_has_extra_self_param(&iface_name, member) {
                self.context.report_simple(
                    TirTypeError::InvalidSelfCallThroughInterface {
                        interface_name: iface_name.name().clone(),
                        method_name: member.clone(),
                    },
                    at,
                );
            }
            let ty = self.resolve_member_suppressing_side_effects(at, |this| {
                // The object-safety restriction (reported above) still applies;
                // resolution proceeds to recover the member's shape for the union.
                this.resolve_interface_member(InterfaceMemberLookup {
                    iface_name: &iface_name,
                    iface_type_args: &type_args,
                    associated_bindings: &associated_bindings,
                    member,
                    at,
                    bound,
                    receiver_projection_base: Some(&self_ty),
                    self_recv: SelfReceiver::Existential,
                })
            });
            // A bound interface *method* comes back self-stripped, but class
            // method members of the same union keep `self` (the unbound form).
            // The union-callee fold requires every arm to share a shape and
            // skips `self` for method calls, so re-attach a `self` param here
            // to match the class arms (`(self: Animal) -> R`, not `() -> R`).
            // Only for actual methods — a function-typed interface *field* must
            // keep its declared signature rather than gain a phantom `self`.
            let is_method = self.interface_closure_declares_method(&iface_name, member);
            return ty.map(|t| {
                if is_method {
                    Self::prepend_self_param_if_method(t, self_ty.clone(), bound)
                } else {
                    t
                }
            });
        }
        // Class / primitive member: the read-only probe handles its own fields
        // and methods.
        if let Some(ty) = self.try_resolve_member_on_ty(m, member) {
            return Some(ty);
        }
        // A class member contributed by two or more interfaces is genuinely
        // ambiguous — emit the same E0131 (field) / E0121 (method) the
        // single-class receiver path emits, with the `.as<I>` projection hint.
        // Surface it directly (not suppressed) and treat the member as resolved,
        // so the union doesn't instead collapse to a misleading
        // `unknown | unknown` (E0006) or blame the member with a false E0007.
        if let Ty::Class(class_name, type_args, _) = m {
            let related = || {
                self.package_items
                    .lookup_type(class_name.namespace(), class_name.name())
                    .map(|def| {
                        vec![RelatedNote::new(
                            RelatedLocation::Item(def),
                            "class defined here",
                        )]
                    })
                    .unwrap_or_default()
            };
            let field_sources = self.class_interface_field_sources(class_name, member);
            if field_sources.len() > 1 {
                self.context.report_at_member(
                    TirTypeError::AmbiguousInterfaceField {
                        class_name: class_name.name().clone(),
                        field_name: member.clone(),
                        sources: field_sources,
                    },
                    at,
                    related(),
                );
                return Some(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            }
            let method_sources: Vec<String> = self.format_interface_method_sources(
                self.implemented_interface_method_sources(class_name, type_args, member)
                    .into_iter()
                    .map(|(_, qtn, args)| (qtn, args)),
            );
            if method_sources.len() >= 2 {
                self.context.report_at_member(
                    TirTypeError::AmbiguousInterfaceMethod {
                        class_name: class_name.name().clone(),
                        method_name: member.clone(),
                        sources: method_sources,
                    },
                    at,
                    related(),
                );
                return Some(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            }
        }
        // Out-of-body / blanket `implements I for T` rules aren't in the type's
        // in-body members, so a union member backed only by such a rule (e.g.
        // `int` in `int | Dog` with `implements Debuggable for int`) is missed
        // above. Consult the registry — suppressing diagnostics + the recorded
        // resolution — so the member resolves (and a genuinely-lacking member
        // like `Dog` is the only one blamed).
        let receiver_name = match m {
            Ty::Class(qtn, _, _) => qtn.name().clone(),
            _ => Name::new("value"),
        };
        let member_ty = m.clone();
        let self_ty = m.clone();
        let ty = self.resolve_member_suppressing_side_effects(at, |this| {
            this.try_registry_member(&member_ty, receiver_name.clone(), member, at, bound)
        });
        // Only re-attach `self` for methods (mirrors the interface branch). The
        // registry resolves `member` as a method exactly when some out-of-body
        // interface declares it as one; a function-typed interface *field*
        // satisfied out-of-body must keep its declared signature rather than
        // gain a phantom `self`.
        let is_method = !self
            .registry_interface_method_sources(&member_ty, member)
            .is_empty();
        ty.map(|t| {
            if is_method {
                Self::prepend_self_param_if_method(t, self_ty.clone(), bound)
            } else {
                t
            }
        })
    }

    /// Run `f` (a member-resolution probe that may report diagnostics and record
    /// a single `MemberResolution` at `at`) and roll back both side effects.
    /// Used by union-member resolution, where the real diagnostics/dispatch are
    /// decided across all members (and at the MIR layer), not per-member.
    fn resolve_member_suppressing_side_effects(
        &mut self,
        at: ExprId,
        f: impl FnOnce(&mut Self) -> Option<Ty>,
    ) -> Option<Ty> {
        let diag_snapshot = self.context.diagnostic_count();
        let saved_resolution = self.resolutions.remove(&at);
        let saved_generics = self.interface_method_generic_params.remove(&at);
        let ty = f(self);
        self.context.truncate_diagnostics(diag_snapshot);
        self.resolutions.remove(&at);
        self.interface_method_generic_params.remove(&at);
        if let Some(r) = saved_resolution {
            self.resolutions.insert(at, r);
        }
        if let Some(g) = saved_generics {
            self.interface_method_generic_params.insert(at, g);
        }
        ty
    }

    /// Re-attach a `self` parameter (typed `self_ty`) to a self-stripped bound
    /// method type, so it shares the self-included shape of class method members
    /// in a union callee. No-op for non-function types or unbound calls.
    fn prepend_self_param_if_method(ty: Ty, self_ty: Ty, bound: bool) -> Ty {
        match ty {
            Ty::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                throws,
                attr,
            } if bound => {
                let mut with_self = Vec::with_capacity(params.len() + 1);
                with_self.push(FunctionParamTy {
                    name: Some(Name::new("self")),
                    ty: self_ty,
                    mode: FunctionParamMode::Required,
                });
                with_self.extend(params);
                Ty::Function {
                    generic_params,
                    generic_param_bounds,
                    params: with_self,
                    ret,
                    throws,
                    attr,
                }
            }
            other => other,
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
            Ty::Map {
                key: key_ty,
                value: val_ty,
                ..
            } => self
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
            Ty::String { .. } | Ty::Literal(baml_base::Literal::String(_), _, _) => self
                .resolve_builtin_method(&["String"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Int { .. } | Ty::Literal(baml_base::Literal::Int(_), _, _) => self
                .resolve_builtin_method(&["Int"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Bigint { .. } | Ty::Literal(baml_base::Literal::Bigint(_), _, _) => self
                .resolve_builtin_method(&["Bigint"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Float { .. } | Ty::Literal(baml_base::Literal::Float(_), _, _) => self
                .resolve_builtin_method(&["Float"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Bool { .. } | Ty::Literal(baml_base::Literal::Bool(_), _, _) => self
                .resolve_builtin_method(&["Bool"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Null { .. } => self
                .resolve_builtin_method(&["Null"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Uint8Array { .. } | Ty::Media(_, _) => {
                let p = match ty {
                    Ty::Uint8Array { .. } => PrimitiveType::Uint8Array,
                    Ty::Media(MediaKind::Image, _) => PrimitiveType::Image,
                    Ty::Media(MediaKind::Audio, _) => PrimitiveType::Audio,
                    Ty::Media(MediaKind::Video, _) => PrimitiveType::Video,
                    Ty::Media(MediaKind::Pdf, _) => PrimitiveType::Pdf,
                    _ => unreachable!("matched Uint8Array or Media above"),
                };
                self.resolve_builtin_method(p.builtin_class_path(), &[], member)
                    .map(BuiltinResolution::into_ty)
            }
            Ty::Type { .. } => self
                .resolve_builtin_method(&["TypeValue"], &[], member)
                .map(BuiltinResolution::into_ty),
            // Universal `to_json` / `from_json` on generic type variables.
            // Mirrors the arm in `resolve_member` — no side effects needed here.
            Ty::TypeVar(_, _) if member.as_str() == "to_json" => Some(Ty::Function {
                generic_params: Vec::new(),
                generic_param_bounds: Vec::new(),
                params: vec![],
                ret: Box::new(json_alias_ty()),
                throws: Box::new(json_serialization_or_parse_error_ty()),
                attr: TyAttr::default(),
            }),
            Ty::TypeVar(name, _) if member.as_str() == "from_json" => Some(Ty::Function {
                generic_params: Vec::new(),
                generic_param_bounds: Vec::new(),
                params: vec![FunctionParamTy::required(
                    Some(Name::new("j")),
                    json_alias_ty(),
                )],
                ret: Box::new(Ty::TypeVar(name.clone(), TyAttr::default())),
                throws: Box::new(json_parse_or_serialization_error_ty()),
                attr: TyAttr::default(),
            }),
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
            // BEP-044 wf3 #G9c: read-only probe for an interface member, so a
            // union like `Animal | Swimmer` attributes a missing `.speak()` only
            // to the arm that actually lacks it (`Swimmer`) instead of falsely
            // blaming `Animal` too. The only caller is union-member resolution,
            // which needs Some/None; a precise member type isn't required here.
            Ty::Interface(iface_qtn, _, _, _) => (self
                .interface_closure_declares_method(iface_qtn, member)
                || self.interface_closure_declares_field(iface_qtn, member))
            .then(|| Ty::Unknown {
                attr: TyAttr::default(),
            }),
            _ => None,
        }
    }

    /// Returns the QTN of an interface that `class_name` directly or
    /// transitively implements and that has the short name `member`. Used to
    /// emit a targeted diagnostic for the removed `obj.InterfaceName` projection
    /// syntax.
    fn lookup_implemented_interface_by_name(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        member: &Name,
    ) -> Option<crate::ty::QualifiedTypeName> {
        let db = self.context.db();
        let pkg_items = self.resolve_class_pkg_items(class_name.package())?;
        let Definition::Class(class_loc) =
            pkg_items.lookup_type(class_name.namespace(), class_name.name())?
        else {
            return None;
        };
        let class_tree = baml_compiler2_hir::file_item_tree(db, class_loc.file(db));
        let class_data = class_tree.classes.get(&class_loc.id(db))?;
        let class_pkg = baml_compiler2_hir::file_package::file_package(db, class_loc.file(db));
        let class_ns = &class_pkg.namespace_path;
        for impl_target in &class_data.implements {
            let Some(iface_loc) = crate::interfaces::resolve_path_to_interface(
                db,
                &impl_target.target.expr,
                pkg_items,
                class_ns,
            ) else {
                continue;
            };
            for iface_loc in
                crate::interfaces::interface_closure_locs(db, iface_loc, pkg_items, class_ns)
            {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                    continue;
                };
                if &iface_data.name == member {
                    return Some(crate::lower_type_expr::qualify_def(
                        db,
                        Definition::Interface(iface_loc),
                        &iface_data.name,
                    ));
                }
            }
        }
        None
    }

    /// Resolve a member access on an interface-typed receiver (BEP-044).
    ///
    /// Walks the interface and its transitive `extends` closure looking for
    /// Returns `true` if `method_name` is a required method (no default body)
    /// on the interface identified by `iface_qtn`.
    fn is_required_interface_method(
        &self,
        iface_qtn: &crate::ty::QualifiedTypeName,
        method_name: &Name,
    ) -> bool {
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_qtn.package()) else {
            return false;
        };
        let Some(def) = pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name()) else {
            return false;
        };
        let baml_compiler2_hir::contributions::Definition::Interface(root_loc) = def else {
            return false;
        };
        let db = self.context.db();
        let root_pkg = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db));
        let pkg_ns = &root_pkg.namespace_path;
        for iface_loc in crate::interfaces::interface_closure_locs(db, root_loc, pkg_items, pkg_ns)
        {
            let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
            let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                continue;
            };
            if iface_data
                .required_methods
                .iter()
                .any(|s| s.name == *method_name)
            {
                return true;
            }
            if iface_data.default_methods.iter().any(|&fn_id| {
                iface_tree
                    .functions
                    .get(&fn_id)
                    .is_some_and(|f| f.name == *method_name)
            }) {
                return false;
            }
        }
        false
    }

    fn add_interface_associated_type_bindings(
        &self,
        inputs: InterfaceBindingInputs<'_, '_>,
        bindings: &mut FxHashMap<Name, Ty>,
        diagnostics: &mut Vec<TirTypeError>,
    ) {
        for assoc in &inputs.iface_data.associated_types {
            if let Some((_, ty)) = inputs
                .associated_bindings
                .iter()
                .find(|(name, _)| name == &assoc.name)
            {
                bindings.insert(assoc.name.clone(), ty.clone());
                continue;
            }
            if inputs.prefer_symbolic_projections
                && let Some(base) = inputs.receiver_projection_base
            {
                let projection_interface = Ty::Interface(
                    inputs.iface_name.clone(),
                    if inputs.iface_type_args.is_empty() {
                        inputs
                            .iface_data
                            .generic_params
                            .iter()
                            .map(|generic| Ty::TypeVar(generic.clone(), TyAttr::default()))
                            .collect()
                    } else {
                        inputs.iface_type_args.to_vec()
                    },
                    inputs.associated_bindings.to_vec(),
                    TyAttr::default(),
                );
                bindings.insert(
                    assoc.name.clone(),
                    Ty::AssociatedTypeProjection {
                        base: Box::new(base.clone()),
                        interface: inputs
                            .qualify_symbolic_projection
                            .then(|| Box::new(projection_interface)),
                        member: assoc.name.clone(),
                        attr: TyAttr::default(),
                    },
                );
                continue;
            }
            if let Some(default) = &assoc.default {
                let ty = crate::generics::lower_type_expr_with_generics(
                    self.context.db(),
                    &default.expr,
                    inputs.pkg_items,
                    inputs.iface_ns,
                    bindings,
                    diagnostics,
                );
                bindings.insert(assoc.name.clone(), ty);
                continue;
            }
            if let Some(base) = inputs.receiver_projection_base {
                let projection_interface = Ty::Interface(
                    inputs.iface_name.clone(),
                    if inputs.iface_type_args.is_empty() {
                        inputs
                            .iface_data
                            .generic_params
                            .iter()
                            .map(|generic| Ty::TypeVar(generic.clone(), TyAttr::default()))
                            .collect()
                    } else {
                        inputs.iface_type_args.to_vec()
                    },
                    inputs.associated_bindings.to_vec(),
                    TyAttr::default(),
                );
                bindings.insert(
                    assoc.name.clone(),
                    Ty::AssociatedTypeProjection {
                        base: Box::new(base.clone()),
                        interface: inputs
                            .qualify_symbolic_projection
                            .then(|| Box::new(projection_interface)),
                        member: assoc.name.clone(),
                        attr: TyAttr::default(),
                    },
                );
            }
        }
    }

    /// a matching field or method. Default methods (with bodies) are
    /// resolved via `FunctionLoc` like class methods. Required methods are
    /// lowered straight from the `InterfaceMethodSig` since they don't have
    /// function locs. Returns `None` when the member isn't found anywhere
    /// in the chain (the caller emits the `UnresolvedMember` diagnostic).
    /// Resolve `member` on interface `iface_name`.
    ///
    /// `self_recv` describes how the receiver pins `Self`, which decides whether
    /// the object-safety restriction (`InvalidSelfCallThroughInterface`) applies:
    ///
    /// - [`SelfReceiver::RigidVar`]: the call reaches the interface through a
    ///   *type variable* bound by it — `self` inside the interface's own default
    ///   method, or a generic `T extends Equals`. `Self` is that rigid variable;
    ///   a `Self`-typed argument is checked against it by identity.
    /// - [`SelfReceiver::ExactTy`]: the receiver is a single known type. This
    ///   includes concrete values and rigid projections such as `H.Item`;
    ///   `Self` resolves to that type.
    /// - [`SelfReceiver::Existential`]: a bare `Ty::Interface` ("dyn") receiver.
    ///   A method is callable if and only if `Self` appears in exactly one
    ///   parameter — the `self` receiver itself. Any *additional* `Self`-typed
    ///   parameter (e.g. `other: Self`) makes the method uncallable on the
    ///   existential, because the second `Self` would have to be the same hidden
    ///   concrete type as the receiver, which a "dyn" value cannot guarantee.
    ///   (Return and `throws` positions don't count — they collapse to the
    ///   interface.) This mirrors Rust's `Self`-vs-`dyn Trait` object-safety
    ///   split and Swift's `Self`-vs-`any Protocol`. Interface methods are
    ///   instance-only (every call goes through a receiver), so "the one `Self`
    ///   parameter" is always the `self` receiver; if static interface methods
    ///   are ever added, a non-receiver `Self` parameter is still caught here.
    fn resolve_interface_member(&mut self, lookup: InterfaceMemberLookup<'_>) -> Option<Ty> {
        let InterfaceMemberLookup {
            iface_name,
            iface_type_args,
            associated_bindings,
            member,
            at,
            bound,
            receiver_projection_base,
            self_recv,
        } = lookup;

        let pkg_items = self.resolve_class_pkg_items(iface_name.package())?;
        let def = pkg_items.lookup_type(iface_name.namespace(), iface_name.name())?;
        let Definition::Interface(root_loc) = def else {
            return None;
        };
        // A named rigid type variable needs call-site identity checking because
        // it remains a `Ty::TypeVar` in the method type. Exact receiver types
        // substitute `Self` directly and are checked by ordinary type matching.
        let rigid_pin: Option<Name> = match self_recv {
            SelfReceiver::RigidVar(pin) => Some(pin.clone()),
            _ => None,
        };
        let preserve_self_associated_projections =
            matches!(self_recv, SelfReceiver::Existential) && bound;
        let db = self.context.db();
        let root_pkg = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db));
        let pkg_ns = &root_pkg.namespace_path;

        for (iface_loc, iface_type_args, iface_associated_bindings) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                db,
                root_loc,
                iface_type_args,
                associated_bindings,
                pkg_items,
                pkg_ns,
            )
        {
            let file = iface_loc.file(db);
            let iface_tree = baml_compiler2_hir::file_item_tree(db, file);
            let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                continue;
            };
            let iface_ns = baml_compiler2_hir::file_package::file_package(db, file)
                .namespace_path
                .clone();
            let current_iface_qtn = crate::lower_type_expr::qualify_def(
                db,
                Definition::Interface(iface_loc),
                &iface_data.name,
            );
            let qualify_symbolic_projection = current_iface_qtn != *iface_name;
            let prefer_symbolic_projections = matches!(self_recv, SelfReceiver::RigidVar(_));

            // Field lookup: walk this interface's own fields.
            for field in &iface_data.fields {
                if &field.name != member {
                    continue;
                }
                if !bound {
                    self.context.report_simple(
                        TirTypeError::InterfaceMemberRequiresReceiver {
                            interface_name: iface_data.name.clone(),
                            member_name: member.clone(),
                        },
                        at,
                    );
                    return Some(Ty::Error {
                        attr: TyAttr::default(),
                    });
                }
                let ty = field
                    .type_expr
                    .as_ref()
                    .map(|te| {
                        let mut diags = Vec::new();
                        let mut bindings = crate::generics::bind_type_vars(
                            &iface_data.generic_params,
                            &iface_type_args,
                        );
                        for generic_param in &iface_data.generic_params {
                            bindings.entry(generic_param.clone()).or_insert_with(|| {
                                Ty::TypeVar(generic_param.clone(), TyAttr::default())
                            });
                        }
                        self.add_interface_associated_type_bindings(
                            InterfaceBindingInputs {
                                iface_name: &current_iface_qtn,
                                iface_data,
                                iface_type_args: &iface_type_args,
                                associated_bindings: &iface_associated_bindings,
                                pkg_items,
                                iface_ns: &iface_ns,
                                receiver_projection_base,
                                qualify_symbolic_projection,
                                prefer_symbolic_projections,
                            },
                            &mut bindings,
                            &mut diags,
                        );
                        let ty = crate::generics::lower_type_expr_with_generics(
                            db, &te.expr, pkg_items, &iface_ns, &bindings, &mut diags,
                        );
                        for diag in diags {
                            self.context.report_at_span(diag, te.span);
                        }
                        ty
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                return Some(ty);
            }

            // Default method lookup: FunctionLoc-based.
            for &fn_id in &iface_data.default_methods {
                let method_data = &iface_tree[fn_id];
                if method_data.name != *member {
                    continue;
                }
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, fn_id);
                let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
                // An exact receiver pins `Self` to its own type, not to a fresh
                // method generic, so suppress the unbound-reference generic there.
                let receiver_generic = (!bound && !matches!(self_recv, SelfReceiver::ExactTy(_)))
                    .then(|| {
                        Self::fresh_interface_method_receiver_generic(
                            iface_data,
                            &sig.user_generic_params,
                        )
                    });
                let mut diags = Vec::new();
                let mut bindings = if iface_type_args.is_empty() {
                    rustc_hash::FxHashMap::default()
                } else {
                    crate::generics::bind_type_vars(&iface_data.generic_params, &iface_type_args)
                };
                for generic_param in &iface_data.generic_params {
                    bindings
                        .entry(generic_param.clone())
                        .or_insert_with(|| Ty::TypeVar(generic_param.clone(), TyAttr::default()));
                }
                self.add_interface_associated_type_bindings(
                    InterfaceBindingInputs {
                        iface_name: &current_iface_qtn,
                        iface_data,
                        iface_type_args: &iface_type_args,
                        associated_bindings: &iface_associated_bindings,
                        pkg_items,
                        iface_ns: &iface_ns,
                        receiver_projection_base,
                        qualify_symbolic_projection,
                        prefer_symbolic_projections,
                    },
                    &mut bindings,
                    &mut diags,
                );
                for generic_param in &sig.user_generic_params {
                    bindings
                        .entry(generic_param.clone())
                        .or_insert_with(|| Ty::TypeVar(generic_param.clone(), TyAttr::default()));
                }
                let mut all_generic_params = iface_data.generic_params.clone();
                // The receiver's `Self` placeholder: a pinned receiver (a rigid
                // type variable for `self`/`T extends I`, or an exact receiver
                // type) takes precedence over the fresh generic used for an
                // unbound existential reference. It is registered as a known type
                // variable so `Self` lowers to it, and bound with `or_insert` so
                // it can never clobber an identically-named interface generic
                // parameter. An exact receiver binds the placeholder to its own
                // type, so `Self` fully resolves to that type.
                let (self_ty_var, self_exact) = Self::self_substitution(
                    self_recv,
                    iface_data,
                    &sig.user_generic_params,
                    receiver_generic.as_ref(),
                );
                if let Some(name) = &self_ty_var {
                    let binding = self_exact
                        .clone()
                        .unwrap_or_else(|| Ty::TypeVar(name.clone(), TyAttr::default()));
                    bindings.entry(name.clone()).or_insert(binding);
                    if !all_generic_params.contains(name) {
                        all_generic_params.push(name.clone());
                    }
                }
                all_generic_params.extend(sig.user_generic_params.iter().cloned());
                let iface_ty = Ty::Interface(
                    current_iface_qtn.clone(),
                    if iface_type_args.is_empty() {
                        iface_data
                            .generic_params
                            .iter()
                            .map(|gp| Ty::TypeVar(gp.clone(), TyAttr::default()))
                            .collect()
                    } else {
                        iface_type_args
                    },
                    iface_associated_bindings.clone(),
                    TyAttr::default(),
                );
                let callable_throws = crate::callable::callable_throws(db, func_loc).clone();
                let receiver_ty = self_exact
                    .or_else(|| {
                        self_ty_var
                            .as_ref()
                            .map(|name| Ty::TypeVar(name.clone(), TyAttr::default()))
                    })
                    .unwrap_or_else(|| iface_ty.clone());
                let self_replacement = self_ty_var
                    .as_ref()
                    .map(|name| crate::lower_type_expr::type_expr_for_name(name.clone()))
                    .unwrap_or_else(|| Self::interface_self_type_expr(iface_data));
                // A pinned `Self` (rigid type variable or exact receiver type)
                // is a single type, so `Self`-typed parameters are sound; the
                // object-safety restriction only applies to a bare interface
                // (existential) receiver.
                if matches!(self_recv, SelfReceiver::Existential)
                    && bound
                    && sig
                        .params
                        .iter()
                        .filter(|param| param.name.as_str() != "self")
                        .any(|param| Self::type_expr_contains_self(&param.ty))
                {
                    self.context.report_simple(
                        TirTypeError::InvalidSelfCallThroughInterface {
                            interface_name: iface_data.name.clone(),
                            method_name: member.clone(),
                        },
                        at,
                    );
                }
                let params: Vec<FunctionParamTy> = sig
                    .params
                    .iter()
                    .map(|param| {
                        let param_ty = if param.name.as_str() == "self"
                            && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
                        {
                            receiver_ty.clone()
                        } else if bindings.is_empty() {
                            let ty_expr = Self::substitute_interface_self_in(
                                &param.ty,
                                &self_replacement,
                                preserve_self_associated_projections,
                            );
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                &ty_expr,
                                pkg_items,
                                &iface_ns,
                                &all_generic_params,
                                &mut diags,
                            )
                        } else {
                            let ty_expr = Self::substitute_interface_self_in(
                                &param.ty,
                                &self_replacement,
                                preserve_self_associated_projections,
                            );
                            crate::generics::lower_type_expr_with_generics(
                                db, &ty_expr, pkg_items, &iface_ns, &bindings, &mut diags,
                            )
                        };
                        FunctionParamTy {
                            name: Some(param.name.clone()),
                            ty: self.resolve_associated_projections_deep(&param_ty),
                            mode: if param.has_default {
                                FunctionParamMode::Optional
                            } else {
                                FunctionParamMode::Required
                            },
                        }
                    })
                    .collect();
                let ret_ty = sig
                    .return_type
                    .as_ref()
                    .map(|te| {
                        let ty_expr = Self::substitute_interface_self_in(
                            te,
                            &self_replacement,
                            preserve_self_associated_projections,
                        );
                        if bindings.is_empty() {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                &ty_expr,
                                pkg_items,
                                &iface_ns,
                                &all_generic_params,
                                &mut diags,
                            )
                        } else {
                            crate::generics::lower_type_expr_with_generics(
                                db, &ty_expr, pkg_items, &iface_ns, &bindings, &mut diags,
                            )
                        }
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                let ret_ty = self.resolve_associated_projections_deep(&ret_ty);
                let throws_ty = sig
                    .throws
                    .as_ref()
                    .map(|te| {
                        let ty_expr = Self::substitute_interface_self_in(
                            te,
                            &self_replacement,
                            preserve_self_associated_projections,
                        );
                        if bindings.is_empty() {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                &ty_expr,
                                pkg_items,
                                &iface_ns,
                                &all_generic_params,
                                &mut diags,
                            )
                        } else {
                            crate::generics::lower_type_expr_with_generics(
                                db, &ty_expr, pkg_items, &iface_ns, &bindings, &mut diags,
                            )
                        }
                    })
                    .unwrap_or_else(|| callable_throws.clone());
                let throws_ty = self.resolve_associated_projections_deep(&throws_ty);
                let mut function_generic_params = Vec::new();
                let mut function_generic_param_bounds = Vec::new();
                if let Some(receiver_generic) = &receiver_generic {
                    function_generic_params.push(receiver_generic.clone());
                    function_generic_param_bounds.push(Some(iface_ty));
                }
                function_generic_params.extend(sig.user_generic_params.iter().cloned());
                function_generic_param_bounds.extend(lower_generic_param_bounds(
                    db,
                    &function_generic_param_bounds_exprs(db, func_loc),
                    pkg_items,
                    &iface_ns,
                    &all_generic_params,
                    Some(&bindings),
                    &mut diags,
                ));
                for diag in diags {
                    self.context.report(diag, at, Vec::new());
                }
                let mut fn_ty = Ty::Function {
                    generic_params: function_generic_params.clone(),
                    generic_param_bounds: function_generic_param_bounds,
                    params,
                    ret: Box::new(ret_ty),
                    throws: Box::new(throws_ty),
                    attr: TyAttr::default(),
                };
                if bound {
                    if let Ty::Function {
                        generic_params,
                        generic_param_bounds,
                        params,
                        ret,
                        throws,
                        attr,
                    } = fn_ty
                    {
                        let stripped = crate::generics::skip_self_param(&params).to_vec();
                        fn_ty = Ty::Function {
                            generic_params,
                            generic_param_bounds,
                            params: stripped,
                            ret,
                            throws,
                            attr,
                        };
                    }
                }
                self.resolutions.insert(
                    at,
                    crate::inference::MemberResolution::InterfaceDefaultMethod {
                        iface_loc,
                        func_loc,
                    },
                );
                if let Some(pin) = &rigid_pin {
                    self.self_pinned_rigid_var.insert(at, pin.clone());
                }
                self.interface_method_generic_params
                    .insert(at, (member.clone(), function_generic_params));
                let owner_type_arg_bindings = iface_data
                    .generic_params
                    .iter()
                    .filter_map(|param| bindings.get(param).cloned().map(|ty| (param.clone(), ty)))
                    .collect::<Vec<_>>();
                if !owner_type_arg_bindings.is_empty() {
                    self.interface_default_owner_type_arg_bindings
                        .insert(at, owner_type_arg_bindings);
                }
                return Some(fn_ty);
            }

            // Required method lookup: built straight from InterfaceMethodSig.
            for sig in &iface_data.required_methods {
                if sig.name != *member {
                    continue;
                }
                // An exact receiver pins `Self` to its own type, not to a fresh
                // method generic, so suppress the unbound-reference generic there.
                let receiver_generic = (!bound && !matches!(self_recv, SelfReceiver::ExactTy(_)))
                    .then(|| {
                        Self::fresh_interface_method_receiver_generic(
                            iface_data,
                            &sig.generic_params,
                        )
                    });
                let mut bindings = if iface_type_args.is_empty() {
                    rustc_hash::FxHashMap::default()
                } else {
                    crate::generics::bind_type_vars(&iface_data.generic_params, &iface_type_args)
                };
                for generic_param in &iface_data.generic_params {
                    bindings
                        .entry(generic_param.clone())
                        .or_insert_with(|| Ty::TypeVar(generic_param.clone(), TyAttr::default()));
                }
                let mut diags = Vec::new();
                self.add_interface_associated_type_bindings(
                    InterfaceBindingInputs {
                        iface_name: &current_iface_qtn,
                        iface_data,
                        iface_type_args: &iface_type_args,
                        associated_bindings: &iface_associated_bindings,
                        pkg_items,
                        iface_ns: &iface_ns,
                        receiver_projection_base,
                        qualify_symbolic_projection,
                        prefer_symbolic_projections,
                    },
                    &mut bindings,
                    &mut diags,
                );
                for generic_param in &sig.generic_params {
                    bindings
                        .entry(generic_param.clone())
                        .or_insert_with(|| Ty::TypeVar(generic_param.clone(), TyAttr::default()));
                }
                let mut all_generic_params = iface_data.generic_params.clone();
                // See the default-method loop above: a pinned `Self` (rigid type
                // variable or exact receiver type) is registered without
                // clobbering an identically-named interface generic parameter; an
                // exact receiver binds the placeholder to its own type.
                let (self_ty_var, self_exact) = Self::self_substitution(
                    self_recv,
                    iface_data,
                    &sig.generic_params,
                    receiver_generic.as_ref(),
                );
                if let Some(name) = &self_ty_var {
                    let binding = self_exact
                        .clone()
                        .unwrap_or_else(|| Ty::TypeVar(name.clone(), TyAttr::default()));
                    bindings.entry(name.clone()).or_insert(binding);
                    if !all_generic_params.contains(name) {
                        all_generic_params.push(name.clone());
                    }
                }
                all_generic_params.extend(sig.generic_params.iter().cloned());
                let iface_ty = Ty::Interface(
                    current_iface_qtn.clone(),
                    if iface_type_args.is_empty() {
                        iface_data
                            .generic_params
                            .iter()
                            .map(|gp| Ty::TypeVar(gp.clone(), TyAttr::default()))
                            .collect()
                    } else {
                        iface_type_args
                    },
                    iface_associated_bindings.clone(),
                    TyAttr::default(),
                );
                // NB: reuse the `diags` from `add_interface_associated_type_bindings`
                // above (do NOT shadow it) so diagnostics from lowering a malformed
                // associated-type default are still reported at the end — mirroring
                // the default-method loop, which threads a single `diags` through.
                let receiver_ty = self_exact
                    .or_else(|| {
                        self_ty_var
                            .as_ref()
                            .map(|name| Ty::TypeVar(name.clone(), TyAttr::default()))
                    })
                    .unwrap_or_else(|| iface_ty.clone());
                let self_replacement = self_ty_var
                    .as_ref()
                    .map(|name| crate::lower_type_expr::type_expr_for_name(name.clone()))
                    .unwrap_or_else(|| Self::interface_self_type_expr(iface_data));
                if matches!(self_recv, SelfReceiver::Existential)
                    && bound
                    && sig
                        .params
                        .iter()
                        .filter(|param| param.name.as_str() != "self")
                        .filter_map(|param| param.type_expr.as_ref())
                        .any(|te| Self::type_expr_contains_self(&te.expr))
                {
                    self.context.report_simple(
                        TirTypeError::InvalidSelfCallThroughInterface {
                            interface_name: iface_data.name.clone(),
                            method_name: member.clone(),
                        },
                        at,
                    );
                }
                let params: Vec<FunctionParamTy> = sig
                    .params
                    .iter()
                    .map(|param| {
                        let param_ty = if param.name.as_str() == "self" && param.type_expr.is_none()
                        {
                            receiver_ty.clone()
                        } else if let Some(te) = &param.type_expr {
                            let ty_expr = Self::substitute_interface_self_in(
                                &te.expr,
                                &self_replacement,
                                preserve_self_associated_projections,
                            );
                            if bindings.is_empty() {
                                crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    &ty_expr,
                                    pkg_items,
                                    &iface_ns,
                                    &all_generic_params,
                                    &mut diags,
                                )
                            } else {
                                crate::generics::lower_type_expr_with_generics(
                                    db, &ty_expr, pkg_items, &iface_ns, &bindings, &mut diags,
                                )
                            }
                        } else {
                            Ty::Unknown {
                                attr: TyAttr::default(),
                            }
                        };
                        FunctionParamTy {
                            name: Some(param.name.clone()),
                            ty: self.resolve_associated_projections_deep(&param_ty),
                            mode: if param.default.is_some() {
                                FunctionParamMode::Optional
                            } else {
                                FunctionParamMode::Required
                            },
                        }
                    })
                    .collect();
                let ret_ty = sig
                    .return_type
                    .as_ref()
                    .map(|te| {
                        let ty_expr = Self::substitute_interface_self_in(
                            &te.expr,
                            &self_replacement,
                            preserve_self_associated_projections,
                        );
                        if bindings.is_empty() {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                &ty_expr,
                                pkg_items,
                                &iface_ns,
                                &all_generic_params,
                                &mut diags,
                            )
                        } else {
                            crate::generics::lower_type_expr_with_generics(
                                db, &ty_expr, pkg_items, &iface_ns, &bindings, &mut diags,
                            )
                        }
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                let ret_ty = self.resolve_associated_projections_deep(&ret_ty);
                let throws_ty = sig
                    .throws
                    .as_ref()
                    .map(|te| {
                        let ty_expr = Self::substitute_interface_self_in(
                            &te.expr,
                            &self_replacement,
                            preserve_self_associated_projections,
                        );
                        if bindings.is_empty() {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                &ty_expr,
                                pkg_items,
                                &iface_ns,
                                &all_generic_params,
                                &mut diags,
                            )
                        } else {
                            crate::generics::lower_type_expr_with_generics(
                                db, &ty_expr, pkg_items, &iface_ns, &bindings, &mut diags,
                            )
                        }
                    })
                    .unwrap_or(Ty::Never {
                        attr: TyAttr::default(),
                    });
                let throws_ty = self.resolve_associated_projections_deep(&throws_ty);
                let mut function_generic_params = Vec::new();
                let mut function_generic_param_bounds = Vec::new();
                if let Some(receiver_generic) = &receiver_generic {
                    function_generic_params.push(receiver_generic.clone());
                    function_generic_param_bounds.push(Some(iface_ty));
                }
                function_generic_params.extend(sig.generic_params.iter().cloned());
                function_generic_param_bounds.extend(lower_generic_param_bounds(
                    db,
                    &sig.generic_param_bounds,
                    pkg_items,
                    &iface_ns,
                    &all_generic_params,
                    Some(&bindings),
                    &mut diags,
                ));
                for diag in diags {
                    self.context.report(diag, at, Vec::new());
                }
                let mut fn_ty = Ty::Function {
                    generic_params: function_generic_params.clone(),
                    generic_param_bounds: function_generic_param_bounds,
                    params,
                    ret: Box::new(ret_ty),
                    throws: Box::new(throws_ty),
                    attr: TyAttr::default(),
                };
                if bound {
                    if let Ty::Function {
                        generic_params,
                        generic_param_bounds,
                        params,
                        ret,
                        throws,
                        attr,
                    } = fn_ty
                    {
                        let stripped = crate::generics::skip_self_param(&params).to_vec();
                        fn_ty = Ty::Function {
                            generic_params,
                            generic_param_bounds,
                            params: stripped,
                            ret,
                            throws,
                            attr,
                        };
                    }
                }
                if let Some(pin) = &rigid_pin {
                    self.self_pinned_rigid_var.insert(at, pin.clone());
                }
                self.interface_method_generic_params
                    .insert(at, (member.clone(), function_generic_params));
                return Some(fn_ty);
            }
        }
        None
    }

    fn interface_self_type_expr(iface_data: &baml_compiler2_hir::item_tree::Interface) -> TypeExpr {
        TypeExpr::Path {
            segments: vec![iface_data.name.clone()],
            generic_args: iface_data
                .generic_params
                .iter()
                .cloned()
                .map(crate::lower_type_expr::type_expr_for_name)
                .collect(),
            associated_type_bindings: Vec::new(),
            attrs: Vec::new(),
        }
    }

    fn substitute_interface_self_in(
        ty: &TypeExpr,
        self_replacement: &TypeExpr,
        preserve_associated_projections: bool,
    ) -> TypeExpr {
        if preserve_associated_projections {
            crate::lower_type_expr::substitute_self_in_preserving_associated_projections(
                ty,
                self_replacement,
            )
        } else {
            crate::lower_type_expr::substitute_self_in(ty, self_replacement)
        }
    }

    /// Resolve how `Self` substitutes for an interface-member resolution.
    ///
    /// Returns the `Self` placeholder type variable (if any) and the exact
    /// type it stands for. A `None` exact type means the placeholder is itself
    /// a type variable — a rigid generic bound, or the fresh generic used for an
    /// unbound existential reference. A `Some(ty)` means `Self` (and the
    /// placeholder) lowers to `ty`. For an exact receiver the
    /// placeholder is a fresh name bound to `ty` in the substitution map, so it
    /// fully resolves away rather than leaking as a type variable.
    fn self_substitution(
        self_recv: SelfReceiver<'_>,
        iface_data: &baml_compiler2_hir::item_tree::Interface,
        method_generic_params: &[Name],
        receiver_generic: Option<&Name>,
    ) -> (Option<Name>, Option<Ty>) {
        match self_recv {
            SelfReceiver::RigidVar(name) => (Some(name.clone()), None),
            SelfReceiver::ExactTy(ty) => (
                Some(Self::fresh_interface_method_receiver_generic(
                    iface_data,
                    method_generic_params,
                )),
                Some(ty.clone()),
            ),
            SelfReceiver::Existential => (receiver_generic.cloned(), None),
        }
    }

    fn fresh_interface_method_receiver_generic(
        iface_data: &baml_compiler2_hir::item_tree::Interface,
        method_generic_params: &[Name],
    ) -> Name {
        let used: FxHashSet<Name> = iface_data
            .generic_params
            .iter()
            .chain(method_generic_params.iter())
            .cloned()
            .collect();
        if !used.contains(&Name::new("T")) {
            return Name::new("T");
        }
        let base = "TImpl";
        if !used.contains(&Name::new(base)) {
            return Name::new(base);
        }
        let mut idx = 0usize;
        loop {
            let candidate = Name::new(format!("{base}{idx}"));
            if !used.contains(&candidate) {
                return candidate;
            }
            idx += 1;
        }
    }

    fn type_expr_contains_self(ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Path {
                segments,
                generic_args,
                ..
            } => {
                segments.iter().any(|segment| segment.as_str() == "Self")
                    || generic_args.iter().any(Self::type_expr_contains_self)
            }
            TypeExpr::List { inner, .. } | TypeExpr::Optional { inner, .. } => {
                Self::type_expr_contains_self(inner)
            }
            TypeExpr::Map { key, value, .. } => {
                Self::type_expr_contains_self(key) || Self::type_expr_contains_self(value)
            }
            TypeExpr::Union { variants, .. } => variants.iter().any(Self::type_expr_contains_self),
            TypeExpr::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params
                    .iter()
                    .any(|param| Self::type_expr_contains_self(&param.ty))
                    || Self::type_expr_contains_self(ret)
                    || throws
                        .as_ref()
                        .is_some_and(|throws| Self::type_expr_contains_self(throws))
            }
            _ => false,
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
        self.class_all_fields_ordered(class_name, class_type_args, true)
            .into_iter()
            .collect()
    }

    fn class_actual_fields_ordered(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
    ) -> Vec<(Name, Ty)> {
        self.class_all_fields_ordered(class_name, class_type_args, false)
    }

    /// Compute the full ordered field list for `class_name`.
    ///
    /// Actual runtime fields are the class's own bare fields. Interface fields
    /// are views over those fields and never add qualified runtime slots.
    ///
    /// Field type diagnostics are owned by the structural
    /// `resolve_class_fields` query and collected once at file level. This
    /// helper is used from expression/pattern checking, so it must not
    /// re-emit those diagnostics at every class use site.
    fn class_all_fields_ordered(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
        _include_aliases: bool,
    ) -> Vec<(Name, Ty)> {
        let mut out: Vec<(Name, Ty)> = Vec::new();
        let Some(pkg_items_for_class) = self.resolve_class_pkg_items(class_name.package()) else {
            return out;
        };
        let Some(Definition::Class(class_loc)) =
            pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())
        else {
            return out;
        };
        let db = self.context.db();
        let file = class_loc.file(db);
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];

        let resolved = crate::inference::resolve_class_fields(db, class_loc);

        // Build bindings from declared generic params → concrete type args.
        let bindings = crate::generics::bind_type_vars(&class_data.generic_params, class_type_args);

        for (name, ty, _attrs) in &resolved.fields {
            let field_ty = if bindings.is_empty() {
                ty.clone()
            } else {
                crate::generics::substitute_ty(ty, &bindings)
            };
            out.push((name.clone(), field_ty));
        }

        out
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
        self.class_actual_fields_ordered(class_name, class_type_args)
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

    /// Interface field `(name, type)` pairs in requires-closure order with
    /// generic substitution applied. This mirrors `resolve_interface_member`
    /// but is side-effect-free for matrix construction.
    fn interface_field_infos_ordered_for_ty(&self, iface_ty: &Ty) -> Vec<(Name, Ty)> {
        let mut out = Vec::new();
        let mut seen = FxHashSet::default();
        let Ty::Interface(iface_name, iface_type_args, associated_bindings, _) = iface_ty else {
            return out;
        };
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_name.package()) else {
            return out;
        };
        let Some(Definition::Interface(root_loc)) =
            pkg_items.lookup_type(iface_name.namespace(), iface_name.name())
        else {
            return out;
        };

        let db = self.context.db();
        let root_pkg = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db));
        for (iface_loc, closure_args, closure_assoc) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                db,
                root_loc,
                iface_type_args,
                associated_bindings,
                pkg_items,
                &root_pkg.namespace_path,
            )
        {
            let file = iface_loc.file(db);
            let iface_tree = baml_compiler2_ppir::file_item_tree(db, file);
            let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                continue;
            };
            let iface_ns = baml_compiler2_hir::file_package::file_package(db, file)
                .namespace_path
                .clone();
            let current_iface_qtn = crate::lower_type_expr::qualify_def(
                db,
                Definition::Interface(iface_loc),
                &iface_data.name,
            );
            let qualify_symbolic_projection = current_iface_qtn != *iface_name;
            let mut bindings =
                crate::generics::bind_type_vars(&iface_data.generic_params, &closure_args);
            for generic_param in &iface_data.generic_params {
                bindings
                    .entry(generic_param.clone())
                    .or_insert_with(|| Ty::TypeVar(generic_param.clone(), TyAttr::default()));
            }
            let mut diags = Vec::new();
            self.add_interface_associated_type_bindings(
                InterfaceBindingInputs {
                    iface_name: &current_iface_qtn,
                    iface_data,
                    iface_type_args: &closure_args,
                    associated_bindings: &closure_assoc,
                    pkg_items,
                    iface_ns: &iface_ns,
                    receiver_projection_base: Some(iface_ty),
                    qualify_symbolic_projection,
                    prefer_symbolic_projections: false,
                },
                &mut bindings,
                &mut diags,
            );

            for field in &iface_data.fields {
                if !seen.insert(field.name.clone()) {
                    continue;
                }
                let ty = field
                    .type_expr
                    .as_ref()
                    .map(|te| {
                        crate::generics::lower_type_expr_with_generics(
                            db, &te.expr, pkg_items, &iface_ns, &bindings, &mut diags,
                        )
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                out.push((field.name.clone(), ty));
            }
        }

        out
    }

    /// For an exact interface instantiation that a class implements, find the
    /// concrete class field backing one interface field. Side-effect-free
    /// counterpart to `qualified_interface_field_for_construction`.
    fn class_field_name_for_interface_field(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
        target_iface_name: &crate::ty::QualifiedTypeName,
        target_iface_args: &[Ty],
        field_name: &Name,
    ) -> Option<Name> {
        let pkg_items = self.resolve_class_pkg_items(class_name.package())?;
        let Definition::Class(class_loc) =
            pkg_items.lookup_type(class_name.namespace(), class_name.name())?
        else {
            return None;
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let class_ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        let class_bindings =
            crate::generics::bind_type_vars(&class_data.generic_params, class_type_args);

        for impl_target in &class_data.implements {
            let Some(root_iface_loc) = crate::interfaces::resolve_path_to_interface(
                db,
                &impl_target.target.expr,
                pkg_items,
                &class_ns,
            ) else {
                continue;
            };

            let root_iface_type_args = match &impl_target.target.expr {
                TypeExpr::Path { generic_args, .. } => {
                    let mut diags = Vec::new();
                    generic_args
                        .iter()
                        .map(|arg| {
                            if class_bindings.is_empty() {
                                crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    arg,
                                    pkg_items,
                                    &class_ns,
                                    &class_data.generic_params,
                                    &mut diags,
                                )
                            } else {
                                crate::generics::lower_type_expr_with_generics(
                                    db,
                                    arg,
                                    pkg_items,
                                    &class_ns,
                                    &class_bindings,
                                    &mut diags,
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                }
                _ => Vec::new(),
            };

            for (iface_loc, iface_type_args, _iface_assoc) in
                crate::interfaces::interface_closure_locs_with_args_and_assoc(
                    db,
                    root_iface_loc,
                    &root_iface_type_args,
                    &[],
                    pkg_items,
                    &class_ns,
                )
            {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                    continue;
                };
                let iface_qtn = crate::lower_type_expr::qualify_def(
                    db,
                    Definition::Interface(iface_loc),
                    &iface_data.name,
                );
                if &iface_qtn != target_iface_name
                    || iface_type_args.len() != target_iface_args.len()
                    || !iface_type_args
                        .iter()
                        .zip(target_iface_args.iter())
                        .all(|(a, b)| self.types_equivalent(a, b))
                {
                    continue;
                }
                let Some(field) = iface_data
                    .fields
                    .iter()
                    .find(|field| field.name == *field_name)
                else {
                    continue;
                };
                return Some(
                    impl_target
                        .field_links
                        .iter()
                        .find(|link| link.interface_field == field.name)
                        .map(|link| link.class_field.clone())
                        .unwrap_or_else(|| field.name.clone()),
                );
            }
        }

        None
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
        let ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        // Check class's own fields.
        if class_data.fields.iter().any(|f| &f.name == member) {
            return true;
        }
        // Removed BEP-044 interface projection syntax (`obj.Interface.field`):
        // treat the interface name as known so resolution can emit a targeted
        // diagnostic instead of a generic missing-member error.
        for impl_target in &class_data.implements {
            let Some(iface_loc) = crate::interfaces::resolve_path_to_interface(
                db,
                &impl_target.target.expr,
                pkg_items,
                &ns,
            ) else {
                continue;
            };
            for iface_loc in
                crate::interfaces::interface_closure_locs(db, iface_loc, pkg_items, &ns)
            {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                    continue;
                };
                // The interface's own name (legacy projection diagnostic),
                // plus any field or method it declares — so members inherited
                // through `implements I {}` (e.g. default methods, interface
                // fields) count as present on the class and route to
                // `resolve_member` for proper resolution / disambiguation.
                if &iface_data.name == member
                    || iface_data.fields.iter().any(|f| &f.name == member)
                    || iface_data
                        .required_methods
                        .iter()
                        .any(|s| s.name == *member)
                    || iface_data
                        .default_methods
                        .iter()
                        .any(|&fn_id| iface_tree[fn_id].name == *member)
                {
                    return true;
                }
            }
        }
        // Check methods.
        self.lookup_class_method(class_name, &[], member).is_some()
    }

    fn qualified_interface_field_for_construction(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
        field_name: &Name,
    ) -> Option<(Name, Ty)> {
        let pkg_items = self.resolve_class_pkg_items(class_name.package())?;
        let Definition::Class(class_loc) =
            pkg_items.lookup_type(class_name.namespace(), class_name.name())?
        else {
            return None;
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let class_ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        let class_bindings =
            crate::generics::bind_type_vars(&class_data.generic_params, class_type_args);

        for impl_target in &class_data.implements {
            let Some(root_iface_loc) = crate::interfaces::resolve_path_to_interface(
                db,
                &impl_target.target.expr,
                pkg_items,
                &class_ns,
            ) else {
                continue;
            };

            let root_iface_type_args = match &impl_target.target.expr {
                TypeExpr::Path { generic_args, .. } => {
                    let mut diags = Vec::new();
                    generic_args
                        .iter()
                        .map(|arg| {
                            if class_bindings.is_empty() {
                                crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    arg,
                                    pkg_items,
                                    &class_ns,
                                    &class_data.generic_params,
                                    &mut diags,
                                )
                            } else {
                                crate::generics::lower_type_expr_with_generics(
                                    db,
                                    arg,
                                    pkg_items,
                                    &class_ns,
                                    &class_bindings,
                                    &mut diags,
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                }
                _ => Vec::new(),
            };

            for (iface_loc, iface_type_args, _iface_assoc) in
                crate::interfaces::interface_closure_locs_with_args_and_assoc(
                    db,
                    root_iface_loc,
                    &root_iface_type_args,
                    &[],
                    pkg_items,
                    &class_ns,
                )
            {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                    continue;
                };
                let Some(field) = iface_data
                    .fields
                    .iter()
                    .find(|field| field.name == *field_name)
                else {
                    continue;
                };
                let class_field_name = impl_target
                    .field_links
                    .iter()
                    .find(|link| link.interface_field == field.name)
                    .map(|link| link.class_field.clone())
                    .unwrap_or_else(|| field.name.clone());
                let iface_ns =
                    baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db))
                        .namespace_path;
                let bindings =
                    crate::generics::bind_type_vars(&iface_data.generic_params, &iface_type_args);
                let declared_ty = field
                    .type_expr
                    .as_ref()
                    .map(|te| {
                        let mut diags = Vec::new();
                        let ty = if bindings.is_empty() {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                &te.expr,
                                pkg_items,
                                &iface_ns,
                                &iface_data.generic_params,
                                &mut diags,
                            )
                        } else {
                            crate::generics::lower_type_expr_with_generics(
                                db, &te.expr, pkg_items, &iface_ns, &bindings, &mut diags,
                            )
                        };
                        for diag in diags {
                            self.context.report_at_span(diag, te.span);
                        }
                        ty
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                return Some((class_field_name, declared_ty));
            }
        }
        None
    }

    fn class_interface_field_sources(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        member: &Name,
    ) -> Vec<Name> {
        let Some(pkg_items) = self.resolve_class_pkg_items(class_name.package()) else {
            return Vec::new();
        };
        let Some(Definition::Class(class_loc)) =
            pkg_items.lookup_type(class_name.namespace(), class_name.name())
        else {
            return Vec::new();
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        let mut seen = FxHashSet::default();
        let mut sources = Vec::new();

        for impl_target in &class_data.implements {
            let Some(root_iface_loc) = crate::interfaces::resolve_path_to_interface(
                db,
                &impl_target.target.expr,
                pkg_items,
                &ns,
            ) else {
                continue;
            };
            // Recover the instantiation the user wrote (e.g. `Box<int>`) so the
            // source carries its type args. Two instantiations of one generic
            // interface (`Box<int>` vs `Box<string>`) then stay distinct — the
            // access is ambiguous, and diagnostics can suggest `as<Box<int>>`.
            let mut diags = Vec::new();
            let root_args = match crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                &impl_target.target.expr,
                pkg_items,
                &ns,
                &class_data.generic_params,
                &mut diags,
            ) {
                Ty::Interface(_, args, _, _) => args,
                _ => Vec::new(),
            };
            for (iface_loc, iface_args, _iface_assoc) in
                crate::interfaces::interface_closure_locs_with_args_and_assoc(
                    db,
                    root_iface_loc,
                    &root_args,
                    &[],
                    pkg_items,
                    &ns,
                )
            {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                    continue;
                };
                if iface_data.fields.iter().any(|field| &field.name == member) {
                    let display = format_interface_display(&iface_data.name, &iface_args);
                    if seen.insert(display.clone()) {
                        sources.push(Name::new(display));
                    }
                }
            }
        }

        sources
    }

    /// Render the instantiated projection target for an interface the class
    /// implements by simple name — e.g. `Container<int>` for `implements
    /// Container<int>`. Falls back to the bare name when no matching `implements`
    /// is found or it carries no type args. Used to make the deprecated
    /// `.Interface` projection hint name the exact instantiation.
    fn implemented_interface_display(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        iface_simple_name: &Name,
    ) -> String {
        let fallback = || iface_simple_name.to_string();
        let Some(pkg_items) = self.resolve_class_pkg_items(class_name.package()) else {
            return fallback();
        };
        let Some(Definition::Class(class_loc)) =
            pkg_items.lookup_type(class_name.namespace(), class_name.name())
        else {
            return fallback();
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        for impl_target in &class_data.implements {
            let mut diags = Vec::new();
            if let Ty::Interface(qtn, args, _, _) = crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                &impl_target.target.expr,
                pkg_items,
                &ns,
                &class_data.generic_params,
                &mut diags,
            ) && qtn.name() == iface_simple_name
            {
                return format_interface_display(iface_simple_name, &args);
            }
        }
        fallback()
    }

    /// Directly-implemented interfaces (named by the `implements` clause) whose
    /// `requires`-closure declares a method `member`, as a default *or* a
    /// required method. Returns, per such interface, its simple name (for
    /// diagnostics), root qualified name, and lowered type args (so an
    /// inherited default can be resolved through the interface machinery).
    ///
    /// This is the method analogue of [`Self::class_interface_field_sources`].
    /// It powers two BEP-044 rules at once: unqualified-call ambiguity (when
    /// two interfaces contribute the same method name → E0121) and inherited
    /// default-method visibility on the concrete class (when exactly one does).
    fn implemented_interface_method_sources(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        type_args: &[Ty],
        member: &Name,
    ) -> Vec<(Name, crate::ty::QualifiedTypeName, Vec<Ty>)> {
        let Some(pkg_items) = self.resolve_class_pkg_items(class_name.package()) else {
            return Vec::new();
        };
        let Some(Definition::Class(class_loc)) =
            pkg_items.lookup_type(class_name.namespace(), class_name.name())
        else {
            return Vec::new();
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        // Map the receiver's concrete class type args onto the class's generic
        // params so a declaring interface's args (which carry the class's type
        // vars, e.g. `Getter<L>`) render and dispatch with the concrete
        // instantiation (`Getter<int>`). Dedup still keys on the pre-substitution
        // form so `Pair<int,int>`'s `Getter<L>`/`Getter<R>` stay two sources
        // (genuine ambiguity) rather than collapsing (BEP-044 wf3 #20).
        let bindings = crate::generics::bind_type_vars(&class_data.generic_params, type_args);
        let mut seen = FxHashSet::default();
        let mut sources = Vec::new();

        for impl_target in &class_data.implements {
            let Some(root_iface_loc) = crate::interfaces::resolve_path_to_interface(
                db,
                &impl_target.target.expr,
                pkg_items,
                &ns,
            ) else {
                continue;
            };

            // Recover the block root's qtn + type args by lowering the path the
            // user wrote (e.g. `Container<int>`); class generics stay as type
            // vars, which is what `resolve_interface_member` expects.
            let mut diags = Vec::new();
            let lowered = crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                &impl_target.target.expr,
                pkg_items,
                &ns,
                &class_data.generic_params,
                &mut diags,
            );
            let Ty::Interface(_root_qtn, root_args, _, _) = lowered else {
                continue;
            };

            // BEP-044 wf3 #11: record the interface that actually DECLARES
            // `member`, not the block root. Walk the `requires` closure in
            // BFS order (most-derived first) and take the first declarer — its
            // own declaration shadows inherited ones, and a method declared
            // once but reached via several `requires` paths (`Left`/`Right`
            // both `requires Base`) collapses to a single source instead of
            // spuriously appearing as N (which produced a false E0121). A real
            // override (e.g. both `Base` and `Left` declare `id`) still yields
            // two distinct declarers across the two blocks → genuine E0121.
            let closure = crate::interfaces::interface_closure_locs_with_args_and_assoc(
                db,
                root_iface_loc,
                &root_args,
                &[],
                pkg_items,
                &ns,
            );
            let mut declarer: Option<(Name, crate::ty::QualifiedTypeName, Vec<Ty>)> = None;
            for (iface_loc, iface_args, _iface_assoc) in &closure {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                    continue;
                };
                let declares_here = iface_data
                    .required_methods
                    .iter()
                    .any(|s| s.name == *member)
                    || iface_data
                        .default_methods
                        .iter()
                        .any(|&fn_id| iface_tree[fn_id].name == *member);
                if declares_here {
                    let iface_pkg =
                        baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
                    let qtn = crate::ty::QualifiedTypeName::new(
                        iface_pkg.package.clone(),
                        iface_pkg.namespace_path.clone(),
                        iface_data.name.clone(),
                    );
                    declarer = Some((iface_data.name.clone(), qtn, iface_args.clone()));
                    break;
                }
            }

            // Dedup by (declaring interface, type args): the same declaration
            // reached via multiple `requires` paths collapses, but two distinct
            // instantiations of one generic interface (`Converter<int>` vs
            // `Converter<float>`) stay separate so an unqualified call is
            // correctly ambiguous (E0121).
            if let Some((name, qtn, args)) = declarer
                && seen.insert((qtn.clone(), args.clone()))
            {
                let subst_args = args
                    .iter()
                    .map(|a| crate::generics::substitute_ty(a, &bindings))
                    .collect();
                sources.push((name, qtn, subst_args));
            }
        }

        sources
    }

    /// Whether `iface_qtn` or any interface in its `requires` closure declares a
    /// method named `member` (required or default). Side-effect-free; used by the
    /// registry fallback in `resolve_member` to find blanket / out-of-body impls.
    fn interface_closure_declares_method(
        &self,
        iface_qtn: &crate::ty::QualifiedTypeName,
        member: &Name,
    ) -> bool {
        let db = self.context.db();
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_qtn.package()) else {
            return false;
        };
        let Some(Definition::Interface(root_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return false;
        };
        let root_pkg = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db));
        crate::interfaces::interface_closure_locs(db, root_loc, pkg_items, &root_pkg.namespace_path)
            .into_iter()
            .any(|iface_loc| {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                iface_tree
                    .interfaces
                    .get(&iface_loc.id(db))
                    .is_some_and(|iface_data| {
                        iface_data
                            .required_methods
                            .iter()
                            .any(|s| s.name == *member)
                            || iface_data
                                .default_methods
                                .iter()
                                .any(|&fn_id| iface_tree[fn_id].name == *member)
                    })
            })
    }

    /// Whether `member`, resolved through `iface_qtn`'s closure, has a non-`self`
    /// parameter typed with `Self` — i.e. it is *not* callable on a bare interface
    /// ("dyn"/existential) receiver (object safety). Mirrors the existential guard
    /// in [`Self::resolve_interface_member`], for callers that need the answer
    /// where that resolution's diagnostics are suppressed (e.g. the union-member
    /// path).
    fn interface_method_has_extra_self_param(
        &self,
        iface_qtn: &crate::ty::QualifiedTypeName,
        member: &Name,
    ) -> bool {
        let db = self.context.db();
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_qtn.package()) else {
            return false;
        };
        let Some(Definition::Interface(root_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return false;
        };
        let root_pkg = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db));
        crate::interfaces::interface_closure_locs(db, root_loc, pkg_items, &root_pkg.namespace_path)
            .into_iter()
            .any(|iface_loc| {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                    return false;
                };
                // Required method: params carry `type_expr: Option<SpannedTypeExpr>`.
                if let Some(sig) = iface_data
                    .required_methods
                    .iter()
                    .find(|s| s.name == *member)
                {
                    return sig
                        .params
                        .iter()
                        .filter(|param| param.name.as_str() != "self")
                        .filter_map(|param| param.type_expr.as_ref())
                        .any(|te| Self::type_expr_contains_self(&te.expr));
                }
                // Default method: an elaborated function signature with `ty: TypeExpr`.
                if let Some(&fn_id) = iface_data
                    .default_methods
                    .iter()
                    .find(|&&fn_id| iface_tree[fn_id].name == *member)
                {
                    let func_loc =
                        baml_compiler2_hir::loc::FunctionLoc::new(db, iface_loc.file(db), fn_id);
                    let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
                    return sig
                        .params
                        .iter()
                        .filter(|param| param.name.as_str() != "self")
                        .any(|param| Self::type_expr_contains_self(&param.ty));
                }
                false
            })
    }

    /// Count how many of `class`'s in-body `implements` blocks resolve to the
    /// SAME interface instantiation `target` once the class's concrete
    /// `type_args` are substituted. `>1` means distinct generic blocks collapsed
    /// (e.g. `Getter<L>`+`Getter<R>` at `Pair<int,int>`), so coercing to that
    /// interface is ambiguous (BEP-044 wf3 #18). Per-instantiation, so
    /// `Pair<int,string>` with `Slot<L,R>`/`Slot<R,L>` is fine (counts 1).
    fn class_interface_instantiation_count(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        type_args: &[Ty],
        target_iface_qtn: &crate::ty::QualifiedTypeName,
        target_iface_args: &[Ty],
    ) -> usize {
        let Some(pkg_items) = self.resolve_class_pkg_items(class_name.package()) else {
            return 0;
        };
        let Some(Definition::Class(class_loc)) =
            pkg_items.lookup_type(class_name.namespace(), class_name.name())
        else {
            return 0;
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        let bindings = crate::generics::bind_type_vars(&class_data.generic_params, type_args);
        let mut count = 0;
        for impl_target in &class_data.implements {
            let mut diags = Vec::new();
            let lowered = crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                &impl_target.target.expr,
                pkg_items,
                &ns,
                &class_data.generic_params,
                &mut diags,
            );
            if let Ty::Interface(qtn, args, _, _) = lowered
                && &qtn == target_iface_qtn
            {
                let subst: Vec<Ty> = args
                    .iter()
                    .map(|a| crate::generics::substitute_ty(a, &bindings))
                    .collect();
                if subst.len() == target_iface_args.len()
                    && subst.iter().zip(target_iface_args).all(|(a, b)| {
                        crate::normalize::is_same_normalized_type(a, b, &self.aliases)
                    })
                {
                    count += 1;
                }
            }
        }
        count
    }

    /// Whether `iface_qtn` or any interface in its `requires` closure declares a
    /// field named `member`. Side-effect-free (G9c union-member probing).
    fn interface_closure_declares_field(
        &self,
        iface_qtn: &crate::ty::QualifiedTypeName,
        member: &Name,
    ) -> bool {
        let db = self.context.db();
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_qtn.package()) else {
            return false;
        };
        let Some(Definition::Interface(root_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return false;
        };
        let root_pkg = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db));
        crate::interfaces::interface_closure_locs(db, root_loc, pkg_items, &root_pkg.namespace_path)
            .into_iter()
            .any(|iface_loc| {
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                iface_tree
                    .interfaces
                    .get(&iface_loc.id(db))
                    .is_some_and(|iface_data| iface_data.fields.iter().any(|f| f.name == *member))
            })
    }

    fn type_package_name(ty: &Ty) -> Option<Name> {
        match ty {
            Ty::Class(qtn, _, _)
            | Ty::Enum(qtn, _)
            | Ty::Interface(qtn, _, _, _)
            | Ty::TypeAlias(qtn, _) => Some(qtn.package().clone()),
            Ty::Union(members, _) => {
                let mut out = None;
                for member in members {
                    let Some(pkg) = Self::type_package_name(member) else {
                        continue;
                    };
                    match &out {
                        Some(existing) if existing != &pkg => return None,
                        None => out = Some(pkg),
                        _ => {}
                    }
                }
                out
            }
            _ => None,
        }
    }

    fn registry_packages_for_interface_lookup(
        &self,
        actual_ty: Option<&Ty>,
        target_iface_qtn: Option<&crate::ty::QualifiedTypeName>,
    ) -> Vec<PackageId<'db>> {
        let db = self.context.db();
        let mut names = Vec::new();
        let mut seen = FxHashSet::default();
        let mut push_name = |name: Name| {
            if seen.insert(name.clone()) {
                names.push(name);
            }
        };

        push_name(self.package_id.name(db));
        for (dep_name, _) in &self.res_ctx.dep_interfaces {
            push_name(dep_name.clone());
        }
        if let Some(pkg) = actual_ty.and_then(Self::type_package_name) {
            push_name(pkg);
        }
        if let Some(qtn) = target_iface_qtn {
            push_name(qtn.package().clone());
        }

        names
            .into_iter()
            .filter(|name| self.res_ctx.items_for_package(db, name).is_some())
            .map(|name| PackageId::new(db, name))
            .collect()
    }

    /// Find interfaces the receiver `base_ty` implements via registry rules
    /// that declare `member`. This includes rules from visible dependency
    /// packages, so stdlib blanket impls like `implements<T> Iterable for T[]`
    /// are available in user code.
    fn registry_interface_method_sources(
        &self,
        base_ty: &Ty,
        member: &Name,
    ) -> Vec<RegistryInterfaceMethodSource> {
        let db = self.context.db();
        let mut seen = FxHashSet::default();
        let mut out: Vec<RegistryInterfaceMethodSource> = Vec::new();
        for pkg_id in self.registry_packages_for_interface_lookup(Some(base_ty), None) {
            let registry = crate::interfaces::package_implements_registry(db, pkg_id);
            for rule in &registry.interface_impl_rules {
                let Some(b) = crate::interfaces::match_ty_pattern(
                    &rule.for_ty_pattern,
                    base_ty,
                    &rule.generic_params,
                    &self.aliases,
                ) else {
                    continue;
                };
                let iface_ty = crate::generics::substitute_ty(&rule.interface_ty, &b);
                let Ty::Interface(iface_qtn, iface_args, associated_bindings, _) = iface_ty else {
                    continue;
                };
                if !self.interface_closure_declares_method(&iface_qtn, member) {
                    continue;
                }
                // Confirm via the full rule check so generic bounds are honored
                // (e.g. `implements<T extends Named> Printable for Box<T>`).
                let requested = Ty::Interface(
                    iface_qtn.clone(),
                    iface_args.clone(),
                    vec![],
                    TyAttr::default(),
                );
                if !registry.type_implements_interface_via_rule(
                    base_ty,
                    &requested,
                    &self.aliases,
                    |a, c| self.is_subtype(a, c),
                ) {
                    continue;
                }
                if seen.insert((iface_qtn.clone(), iface_args.clone())) {
                    out.push((iface_qtn, iface_args, associated_bindings));
                }
            }
        }
        out
    }

    /// Render interface method/field sources for an E0121 message: each as a
    /// namespace-qualified, type-arg-instantiated display (`zoo.Animal`,
    /// `Getter<int>`) so two same-simple-name interfaces are distinguishable and
    /// the suggested `as<…>` projection actually compiles. Root-namespace
    /// interfaces stay bare. Shared by the in-body and registry E0121 sites (C6).
    fn format_interface_method_sources(
        &self,
        sources: impl Iterator<Item = (crate::ty::QualifiedTypeName, Vec<Ty>)>,
    ) -> Vec<String> {
        sources
            .map(|(qtn, args)| {
                let base = format_interface_display(qtn.name(), &args);
                if qtn.namespace().is_empty() {
                    base
                } else {
                    let ns = qtn
                        .namespace()
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(".");
                    // BEP-044 wf3 #10: when the call site is itself inside a
                    // namespace, a bare `zoo.Animal` resolves relative-first
                    // (→ `birds.zoo.Animal`) and the suggested `as<…>` fix
                    // wouldn't compile. Use the absolute `root.`-qualified form
                    // there; at the root namespace keep the bare relative form.
                    if self.ns_context.is_empty() {
                        format!("{ns}.{base}")
                    } else {
                        format!("root.{ns}.{base}")
                    }
                }
            })
            .collect()
    }

    /// Resolve `member` on `base_ty` through blanket / out-of-body registry
    /// impls (BEP-044 wf3 #G7). Returns `Some(ty)` when exactly one matching
    /// interface declares it (resolved as `obj.as<I>.member`), `Some(Unknown)`
    /// after emitting E0121 when two or more do, and `None` when none match
    /// (caller falls through to its own "no member" error). `receiver_name` is
    /// used only for the E0121 message.
    fn try_registry_member(
        &mut self,
        base_ty: &Ty,
        receiver_name: Name,
        member: &Name,
        at: ExprId,
        bound: bool,
    ) -> Option<Ty> {
        let reg = self.registry_interface_method_sources(base_ty, member);
        match reg.as_slice() {
            [] => None,
            [(iface_qtn, iface_args, associated_bindings)] => {
                let iface_qtn = iface_qtn.clone();
                let iface_args = iface_args.clone();
                let associated_bindings = associated_bindings.clone();
                let receiver_ty = Ty::Interface(
                    iface_qtn.clone(),
                    iface_args.clone(),
                    associated_bindings.clone(),
                    TyAttr::default(),
                );
                self.resolve_interface_member(InterfaceMemberLookup {
                    iface_name: &iface_qtn,
                    iface_type_args: &iface_args,
                    associated_bindings: &associated_bindings,
                    member,
                    at,
                    bound,
                    receiver_projection_base: Some(&receiver_ty),
                    self_recv: SelfReceiver::ExactTy(base_ty),
                })
            }
            _ => {
                let sources = self.format_interface_method_sources(
                    reg.iter().map(|(qtn, args, _)| (qtn.clone(), args.clone())),
                );
                self.context.report_at_member(
                    TirTypeError::AmbiguousInterfaceMethod {
                        class_name: receiver_name,
                        method_name: member.clone(),
                        sources,
                    },
                    at,
                    Vec::new(),
                );
                Some(Ty::Unknown {
                    attr: TyAttr::default(),
                })
            }
        }
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

    fn resolve_interface_loc(
        &self,
        qtn: &crate::ty::QualifiedTypeName,
    ) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
        let pkg_items = self.resolve_class_pkg_items(qtn.package())?;
        match pkg_items.lookup_type(qtn.namespace(), qtn.name())? {
            Definition::Interface(interface_loc) => Some(interface_loc),
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

    /// BEP-044 §"Method Disambiguation": when a class's flattened
    /// method list contains two or more methods sharing `method_name`
    /// (typically because the class declares them in different
    /// `implements I {}` blocks), return the list of contributing
    /// interface names. Returns `None` when the call is unambiguous.
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

                // BEP-044: pre-substitute `Self` to the enclosing class
                // name so signatures like `function clone() -> Self`
                // surface as `Ty::Class(<class>)`.
                let self_replacement =
                    crate::lower_type_expr::type_expr_for_name(class_data.name.clone());

                if let Some(target) = item_tree.method_to_iface_target.get(&method_id)
                    && let Some(iface_loc) = crate::interfaces::resolve_path_to_interface(
                        db,
                        &target.expr,
                        pkg_items_for_class,
                        &ns_context,
                    )
                {
                    let iface_file = iface_loc.file(db);
                    let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_file);
                    if let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) {
                        let iface_ns =
                            baml_compiler2_hir::file_package::file_package(db, iface_file)
                                .namespace_path;
                        if let baml_compiler2_ast::TypeExpr::Path { generic_args, .. } =
                            &target.expr
                        {
                            for (param, arg) in iface_data.generic_params.iter().zip(generic_args) {
                                let ty = crate::generics::lower_type_expr_with_generics(
                                    db,
                                    arg,
                                    pkg_items_for_class,
                                    &ns_context,
                                    &bindings,
                                    &mut diags,
                                );
                                bindings.insert(param.clone(), ty);
                            }
                        }

                        let explicit_bindings = item_tree
                            .method_to_iface_associated_type_bindings
                            .get(&method_id)
                            .cloned()
                            .unwrap_or_default();
                        for assoc in &iface_data.associated_types {
                            if explicit_bindings.iter().any(|b| b.name == assoc.name) {
                                continue;
                            }
                            if let Some(default) = &assoc.default {
                                let ty = crate::generics::lower_type_expr_with_generics(
                                    db,
                                    &default.expr,
                                    pkg_items_for_class,
                                    &iface_ns,
                                    &bindings,
                                    &mut diags,
                                );
                                bindings.insert(assoc.name.clone(), ty);
                            }
                        }
                        for binding in &explicit_bindings {
                            let Some(te) = &binding.type_expr else {
                                continue;
                            };
                            let resolved = crate::lower_type_expr::substitute_self_in(
                                &te.expr,
                                &self_replacement,
                            );
                            let ty = crate::generics::lower_type_expr_with_generics(
                                db,
                                &resolved,
                                pkg_items_for_class,
                                &ns_context,
                                &bindings,
                                &mut diags,
                            );
                            bindings.insert(binding.name.clone(), ty);
                        }
                    }
                }

                let callable_throws = crate::callable::callable_throws(db, func_loc).clone();
                let generic_param_bounds = lower_generic_param_bounds(
                    db,
                    &function_generic_param_bounds_exprs(db, func_loc),
                    pkg_items_for_class,
                    &ns_context,
                    &all_generic_params,
                    Some(&bindings),
                    &mut diags,
                );

                let ty = Ty::Function {
                    generic_params: sig.user_generic_params.clone(),
                    generic_param_bounds,
                    params: sig
                        .params
                        .iter()
                        .map(|param| {
                            let param_ty = if param.name.as_str() == "self"
                                && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
                            {
                                // self with no annotation → use the enclosing class type
                                class_ty.clone()
                            } else {
                                let resolved = crate::lower_type_expr::substitute_self_in(
                                    &param.ty,
                                    &self_replacement,
                                );
                                if bindings.is_empty() {
                                    crate::lower_type_expr::lower_type_expr_in_ns(
                                        db,
                                        &resolved,
                                        pkg_items_for_class,
                                        &ns_context,
                                        &all_generic_params,
                                        &mut diags,
                                    )
                                } else {
                                    crate::generics::lower_type_expr_with_generics(
                                        db,
                                        &resolved,
                                        pkg_items_for_class,
                                        &ns_context,
                                        &bindings,
                                        &mut diags,
                                    )
                                }
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
                                let resolved = crate::lower_type_expr::substitute_self_in(
                                    te,
                                    &self_replacement,
                                );
                                if bindings.is_empty() {
                                    crate::lower_type_expr::lower_type_expr_in_ns(
                                        db,
                                        &resolved,
                                        pkg_items_for_class,
                                        &ns_context,
                                        &all_generic_params,
                                        &mut diags,
                                    )
                                } else {
                                    crate::generics::lower_type_expr_with_generics(
                                        db,
                                        &resolved,
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
            "bigint" => &["Bigint"],
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
                        crate::ty::QualifiedTypeName::new(
                            pkg_info.package,
                            pkg_info.namespace_path,
                            class_data.name.clone(),
                        ),
                        // Declared generics live on the type as `TypeVar` args,
                        // not on the name.
                        class_data
                            .generic_params
                            .iter()
                            .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                            .collect(),
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
                        "Map" => Ty::Map {
                            key: Box::new(type_args[0].clone()),
                            value: Box::new(type_args[1].clone()),
                            attr: TyAttr::default(),
                        },
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
                        generic_params: Vec::new(),
                        generic_param_bounds: Vec::new(),
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

    /// An unannotated `let a = []` produces an `EvolvingList(Never)` whose
    /// element type is meant to be fixed by usage — the same way `.push`
    /// evolves it. When such an *empty* evolving container is read in a
    /// context that expects a concrete `List`/`Map`, adopt that element type
    /// so the binding (and any later mutation) stays consistent with the use.
    ///
    /// A later, conflicting typed use no longer matches the `Never` guard, so
    /// it falls through to a normal invariance mismatch.
    fn retype_evolving_empty(&mut self, expr_id: ExprId, body: &ExprBody, expected: &Ty) {
        let Some(name) = self.expr_local_name(expr_id, body) else {
            return;
        };
        let Some(binding) = self.locals.get(&name) else {
            return;
        };
        let new_ty = match (&binding.current_ty, expected) {
            (Ty::EvolvingList(inner, attr), Ty::List(exp, _) | Ty::EvolvingList(exp, _))
                if matches!(**inner, Ty::Never { .. }) =>
            {
                Ty::EvolvingList(exp.clone(), attr.clone())
            }
            (
                Ty::EvolvingMap(k, v, attr),
                Ty::Map {
                    key: exp_k,
                    value: exp_v,
                    ..
                }
                | Ty::EvolvingMap(exp_k, exp_v, _),
            ) if matches!(**k, Ty::Never { .. }) && matches!(**v, Ty::Never { .. }) => {
                Ty::EvolvingMap(exp_k.clone(), exp_v.clone(), attr.clone())
            }
            _ => return,
        };
        self.assign_local(name.clone(), new_ty.clone());
        self.sync_let_binding_type(&name, new_ty);
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
                    Expr::OptionalMemberAccess { .. } => Ty::optional(method_ty),
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
            Ty::Map {
                key: key_ty,
                value: val_ty,
                attr: container_attr,
            }
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
                        Ty::Map {
                            key: Box::new(widened_key),
                            value: Box::new(widened_val.clone()),
                            attr: container_attr,
                        }
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

    fn resolve_associated_projections_deep(&self, ty: &Ty) -> Ty {
        crate::associated_projection::AssociatedProjectionResolver::with_resolution_context(
            self.context.db(),
            self.res_ctx,
            &self.aliases,
            &self.generic_param_bounds,
        )
        .resolve_deep(ty)
    }

    fn interface_associated_type_names(
        &self,
        iface_qtn: &crate::ty::QualifiedTypeName,
    ) -> Vec<Name> {
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_qtn.package()) else {
            return Vec::new();
        };
        let Some(Definition::Interface(iface_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return Vec::new();
        };
        let db = self.context.db();
        let iface_tree = baml_compiler2_ppir::file_item_tree(db, iface_loc.file(db));
        iface_tree
            .interfaces
            .get(&iface_loc.id(db))
            .map(|iface| {
                iface
                    .associated_types
                    .iter()
                    .map(|assoc| assoc.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn package_id_for_qtn(&self, qtn: &crate::ty::QualifiedTypeName) -> PackageId<'db> {
        PackageId::new(self.context.db(), qtn.package().clone())
    }

    fn registry_package_for_interface_check(
        &self,
        sub: &Ty,
        sup_iface_qtn: &crate::ty::QualifiedTypeName,
    ) -> PackageId<'db> {
        match sub {
            Ty::Class(class_qtn, _, _) => self.package_id_for_qtn(class_qtn),
            _ => self.package_id_for_qtn(sup_iface_qtn),
        }
    }

    fn associated_projection_views_equivalent(&self, a: &Ty, b: &Ty) -> bool {
        crate::associated_projection::AssociatedProjectionResolver::with_resolution_context(
            self.context.db(),
            self.res_ctx,
            &self.aliases,
            &self.generic_param_bounds,
        )
        .projection_views_equivalent(a, b)
    }

    /// Subtype check — delegates to the normalizer which resolves type aliases
    /// and performs equirecursive structural subtyping.
    ///
    /// **Nominal interface subtyping (BEP-044).** Before falling through to
    /// structural subtyping, we short-circuit on `Class C <: Interface I`:
    /// `C` is a subtype of `I` iff the implements registry says it is. This
    /// preserves BAML's nominal semantics — a class without an explicit
    /// `implements I` block never satisfies `I`, even if it has matching
    /// fields and methods.
    /// Whether the type variable `sub_name` is a subtype of `sup` by its own
    /// identity (independent of its bound): `T <: T`, `T <: T?`, and
    /// `T <: (T | …)`. Used to make type-variable reflexivity take precedence
    /// over bound-expansion in [`Self::is_subtype`].
    fn typevar_is_reflexive_subtype(sub_name: &Name, sup: &Ty) -> bool {
        match sup {
            Ty::TypeVar(sup_name, _) => sup_name == sub_name,
            Ty::Union(members, _) => members
                .iter()
                .any(|m| Self::typevar_is_reflexive_subtype(sub_name, m)),
            _ => false,
        }
    }

    fn is_subtype(&self, sub: &Ty, sup: &Ty) -> bool {
        if sub == sup {
            return true;
        }
        // Type-variable reflexivity must be checked BEFORE expanding the
        // variable's bound: a type variable is a subtype of *itself* (and of an
        // optional/union containing itself), which holds for the variable's own
        // identity regardless of its bound. Expanding the bound first would turn
        // `T <: T` into `bound(T) <: T` (false) and wrongly reject legitimate
        // same-variable uses like a `Self`-typed argument inside a default
        // method, or `same<T extends Eq>(x: T, y: T) { x.eq(y) }`.
        if let Ty::TypeVar(sub_name, _) = sub
            && Self::typevar_is_reflexive_subtype(sub_name, sup)
        {
            return true;
        }
        if self.associated_projection_views_equivalent(sub, sup) {
            return true;
        }
        if let (Ty::TypeVar(sub_name, _), Ty::Interface(sup_qtn, sup_args, sup_assoc, _)) =
            (sub, sup)
            && let Some(bound) = self.generic_param_bounds.get(sub_name)
            && let Some(symbolic_bound) =
                self.interface_bound_with_self_associated_bindings(sub_name, bound)
            && let Ty::Interface(bound_qtn, bound_args, bound_assoc, _) = symbolic_bound
        {
            if &bound_qtn == sup_qtn
                && bound_args.len() == sup_args.len()
                && bound_args
                    .iter()
                    .zip(sup_args.iter())
                    .all(|(a, b)| self.types_equivalent(a, b))
                && sup_assoc.iter().all(|(sup_name, sup_ty)| {
                    bound_assoc
                        .iter()
                        .find(|(bound_name, _)| bound_name == sup_name)
                        .is_some_and(|(_, bound_ty)| self.types_equivalent(bound_ty, sup_ty))
                })
            {
                return true;
            }
            if self.interface_requires_instantiation(
                &bound_qtn,
                &bound_args,
                &bound_assoc,
                sup_qtn,
                sup_args,
                sup_assoc,
            ) {
                return true;
            }
        }
        let expanded_sub = self
            .interface_type_with_default_associated_bindings(self.expand_alias_chains(sub.clone()));
        let expanded_sup = self
            .interface_type_with_default_associated_bindings(self.expand_alias_chains(sup.clone()));
        let resolved_sub = self.resolve_associated_projections_deep(&expanded_sub);
        let resolved_sup = self.resolve_associated_projections_deep(&expanded_sup);
        if resolved_sub != *sub || resolved_sup != *sup {
            return self.is_subtype(&resolved_sub, &resolved_sup);
        }
        if let Ty::TypeVar(name, _) = sub
            && let Some(bound) = self.generic_param_bounds.get(name)
        {
            return self.is_subtype(&bound.clone(), sup);
        }
        if let Some(result) = self.function_subtype_with_generic_binders(sub, sup) {
            return result;
        }
        // BEP-044 wf3 #12: a union is a subtype of `sup` iff *every* member is
        // (join-of-subtypes). Checked before the interface/structural branches
        // because those assume a non-union `sub` and would reject `Dog | Cat`
        // against `Animal` early. `Dog?` (= `Dog | null`) is still rejected
        // because `null` is not a subtype of the interface.
        if let Ty::Union(members, _) = sub
            && !members.is_empty()
            && members.iter().all(|m| self.is_subtype(m, sup))
        {
            return true;
        }
        // Same-class generic args are invariant. The normalizer implements
        // that invariance as structural EQUALITY, which is both too strict
        // and too weak for two cases the spawn `with`-chain (and any
        // expected-type-driven generic instantiation) hits:
        //   1. HOLES — `Ty::Unknown` (error recovery / not-yet-constrained,
        //      bidirectionally compatible by the top-level rule in
        //      `normalize::is_subtype_of`) and `Ty::BuiltinUnknown` (what
        //      unresolved callee typevars are ERASED to, e.g.
        //      `let t = withDouble();` leaving `SpawnParams<int, unknown>`).
        //      Equality rejects `SpawnParams<int, E> <:
        //      SpawnParams<unknown, unknown>` even though the hole means
        //      "anything goes here", not "a different type".
        //   2. EQUIVALENT SPELLINGS — phase-0 can bind `T = int | 99` (arg
        //      literal joined with the expected type); that denotes the same
        //      type as `int` but is not structurally equal to it.
        // Compare args pairwise instead: holes match anything, concrete args
        // must be MUTUAL subtypes (the proper definition of invariant
        // compatibility, of which structural equality is a special case).
        if let (Ty::Class(sub_qtn, sub_args, _), Ty::Class(sup_qtn, sup_args, _)) = (sub, sup)
            && sub_qtn == sup_qtn
            && sub_args.len() == sup_args.len()
        {
            return sub_args.iter().zip(sup_args.iter()).all(|(a, b)| {
                matches!(a, Ty::Unknown { .. } | Ty::BuiltinUnknown { .. })
                    || matches!(b, Ty::Unknown { .. } | Ty::BuiltinUnknown { .. })
                    || (self.is_subtype(a, b) && self.is_subtype(b, a))
            });
        }
        if let Ty::Interface(iface_qtn, iface_args, associated_bindings, _) = sup
            && !matches!(sub, Ty::Interface(..))
        {
            let db = self.context.db();
            let registry_pkg = self.registry_package_for_interface_check(sub, iface_qtn);
            let registry = crate::interfaces::package_implements_registry(db, registry_pkg);
            let requested_iface_ty = Ty::Interface(
                iface_qtn.clone(),
                iface_args.clone(),
                associated_bindings.clone(),
                TyAttr::default(),
            );
            return registry.type_implements_interface_via_rule(
                sub,
                &requested_iface_ty,
                &self.aliases,
                |actual, bound| self.is_subtype(actual, bound),
            );
        }
        // BEP-044 interface-to-interface subtyping: `Interface A <: Interface B`
        // iff A == B or A requires B (transitively). Two unrelated interfaces
        // that happen to share an implementor are NOT subtypes — the user must
        // narrow via `match`/`is` first.
        if let (
            Ty::Interface(a_qtn, a_args, a_associated_bindings, _),
            Ty::Interface(b_qtn, b_args, b_associated_bindings, _),
        ) = (sub, sup)
        {
            let db = self.context.db();
            let registry = crate::interfaces::package_implements_registry(db, self.package_id);
            if a_qtn == b_qtn
                && a_args.len() == b_args.len()
                && a_args
                    .iter()
                    .zip(b_args.iter())
                    .all(|(a, b)| self.types_equivalent(a, b))
                && b_associated_bindings.iter().all(|(b_name, b_ty)| {
                    a_associated_bindings
                        .iter()
                        .find(|(a_name, _)| a_name == b_name)
                        .is_some_and(|(_, a_ty)| self.types_equivalent(a_ty, b_ty))
                })
            {
                return true;
            }
            if a_qtn != b_qtn
                && registry.interface_requires(a_qtn, b_qtn)
                && b_args.is_empty()
                && b_associated_bindings.is_empty()
            {
                return true;
            }
            if a_qtn != b_qtn
                && self.interface_requires_instantiation(
                    a_qtn,
                    a_args,
                    a_associated_bindings,
                    b_qtn,
                    b_args,
                    b_associated_bindings,
                )
            {
                return true;
            }
        }
        // BEP-044: nominal interface subtyping must also hold when the target
        // is a *union* type (including a nullable `T | null`).
        // `normalize::is_subtype_of` has no interface arm, so without this a
        // `Dog` (which implements `Animal`) would be rejected for `Animal | null`
        // or `Animal | string` even though it is a subtype of a wrapped
        // interface. We only short-circuit on a positive result here — a
        // negative falls through to the structural check below, so non-interface
        // union behaviour is unchanged.
        if let Ty::Union(members, _) = sup {
            // Decompose `sub` into members too — a nullable `T?` is now the
            // union `T | null` — and require every `sub` member to be a
            // subtype of some `sup` member, using the recursive (nominal
            // interface aware) `is_subtype`. This preserves `Dog? <: Animal?`
            // (and `Dog <: Animal?`), which `normalize::is_subtype_of` cannot
            // decide because it has no interface arm. Strictly adds positives.
            let sub_members: Vec<&Ty> = match sub {
                Ty::Union(sub_members, _) => sub_members.iter().collect(),
                other => vec![other],
            };
            if sub_members
                .iter()
                .all(|sm| members.iter().any(|m| self.is_subtype(sm, m)))
            {
                return true;
            }
        }
        if let Some(result) = self.structural_subtype_with_nominal_interfaces(sub, sup) {
            return result;
        }
        // Interface-to-interface subtyping: `Interface A <: Interface B` iff
        // A extends B (transitively). The registry doesn't carry that
        // directly, but every class that implements A also implements B, so
        // we don't need a separate index — the normalizer handles equality
        // and the `extends` chain is reflected in the per-class data already.
        // For a pure interface-to-interface check, fall through to structural
        // equality (matches today's behaviour for unrelated interfaces).
        crate::normalize::is_subtype_of(sub, sup, &self.aliases)
    }

    fn structural_subtype_with_nominal_interfaces(&self, sub: &Ty, sup: &Ty) -> Option<bool> {
        let expanded_sub = self.expand_alias_chains(sub.clone());
        let expanded_sup = self.expand_alias_chains(sup.clone());
        if &expanded_sub != sub || &expanded_sup != sup {
            return self.structural_subtype_with_nominal_interfaces(&expanded_sub, &expanded_sup);
        }

        match (sub, sup) {
            (Ty::List(..) | Ty::EvolvingList(..), Ty::List(..) | Ty::EvolvingList(..)) => {
                Some(crate::normalize::is_subtype_of(sub, sup, &self.aliases))
            }
            (Ty::Map { .. } | Ty::EvolvingMap(..), Ty::Map { .. } | Ty::EvolvingMap(..)) => {
                Some(crate::normalize::is_subtype_of(sub, sup, &self.aliases))
            }
            (Ty::Future(sub_value, sub_error, _), Ty::Future(sup_value, sup_error, _)) => {
                Some(self.is_subtype(sub_value, sup_value) && self.is_subtype(sub_error, sup_error))
            }
            (
                Ty::Function {
                    generic_params: sub_generic_params,
                    params: sub_params,
                    ret: sub_ret,
                    throws: sub_throws,
                    ..
                },
                Ty::Function {
                    generic_params: sup_generic_params,
                    params: sup_params,
                    ret: sup_ret,
                    throws: sup_throws,
                    ..
                },
            ) if sub_generic_params.is_empty() && sup_generic_params.is_empty() => Some(
                self.is_subtype(sub_ret, sup_ret)
                    && self.is_subtype(sub_throws, sup_throws)
                    && self.function_params_subtype_with_nominal_interfaces(sub_params, sup_params),
            ),
            _ => None,
        }
    }

    fn function_params_subtype_with_nominal_interfaces(
        &self,
        sub_params: &[FunctionParamTy],
        sup_params: &[FunctionParamTy],
    ) -> bool {
        let sub_required: Vec<_> = sub_params
            .iter()
            .filter(|param| matches!(param.mode, FunctionParamMode::Required))
            .collect();
        let sup_required: Vec<_> = sup_params
            .iter()
            .filter(|param| matches!(param.mode, FunctionParamMode::Required))
            .collect();

        if sub_required.len() != sup_required.len() {
            return false;
        }

        for (sub, sup) in sub_required.iter().zip(sup_required.iter()) {
            if !self.is_subtype(&sup.ty, &sub.ty) {
                return false;
            }
        }

        for sup in sup_params
            .iter()
            .filter(|param| matches!(param.mode, FunctionParamMode::Optional))
        {
            let Some(name) = &sup.name else {
                return false;
            };
            let Some(sub) = sub_params.iter().find(|param| {
                matches!(param.mode, FunctionParamMode::Optional)
                    && param.name.as_ref() == Some(name)
            }) else {
                return false;
            };
            if !self.is_subtype(&sup.ty, &sub.ty) {
                return false;
            }
        }

        true
    }

    fn validate_function_generic_bounds(
        &mut self,
        expr_id: ExprId,
        generic_params: &[Name],
        generic_param_bounds: &[Option<Ty>],
        bindings: &FxHashMap<Name, Ty>,
    ) {
        for (idx, param) in generic_params.iter().enumerate() {
            let Some(bound) = generic_param_bounds.get(idx).and_then(Option::as_ref) else {
                continue;
            };
            let Some(actual) = bindings.get(param) else {
                continue;
            };
            let bound = crate::generics::substitute_ty(bound, bindings);
            if !self.is_subtype(actual, &bound) {
                self.context.report(
                    TirTypeError::TypeMismatch {
                        expected: bound,
                        got: actual.clone(),
                    },
                    expr_id,
                    Vec::new(),
                );
            }
        }
    }

    pub(crate) fn validate_type_generic_bounds_at_span(&mut self, span: TextRange, ty: &Ty) {
        for error in self.collect_type_generic_bound_errors(ty) {
            self.context.report_at_span(error, span);
        }
    }

    fn validate_type_generic_bounds(&mut self, expr_id: ExprId, ty: &Ty) {
        for error in self.collect_type_generic_bound_errors(ty) {
            self.context.report(error, expr_id, Vec::new());
        }
    }

    fn collect_type_generic_bound_errors(&mut self, ty: &Ty) -> Vec<TirTypeError> {
        let mut seen_aliases = FxHashSet::default();
        let mut errors = Vec::new();
        self.collect_type_generic_bound_errors_inner(ty, &mut seen_aliases, &mut errors);
        errors
    }

    fn collect_type_generic_bound_errors_inner(
        &mut self,
        ty: &Ty,
        seen_aliases: &mut FxHashSet<crate::ty::QualifiedTypeName>,
        errors: &mut Vec<TirTypeError>,
    ) {
        match ty {
            Ty::Class(qtn, type_args, _) => {
                for arg in type_args {
                    self.collect_type_generic_bound_errors_inner(arg, seen_aliases, errors);
                }
                self.collect_class_generic_bound_errors(qtn, type_args, errors);
            }
            Ty::Interface(qtn, type_args, associated_bindings, _) => {
                for arg in type_args {
                    self.collect_type_generic_bound_errors_inner(arg, seen_aliases, errors);
                }
                for (_, arg) in associated_bindings {
                    self.collect_type_generic_bound_errors_inner(arg, seen_aliases, errors);
                }
                self.collect_interface_generic_bound_errors(qtn, type_args, errors);
            }
            Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
                self.collect_type_generic_bound_errors_inner(inner, seen_aliases, errors);
            }
            Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => {
                self.collect_type_generic_bound_errors_inner(key, seen_aliases, errors);
                self.collect_type_generic_bound_errors_inner(value, seen_aliases, errors);
            }
            Ty::Union(members, _) => {
                for member in members {
                    self.collect_type_generic_bound_errors_inner(member, seen_aliases, errors);
                }
            }
            Ty::Function {
                generic_param_bounds,
                params,
                ret,
                throws,
                ..
            } => {
                for bound in generic_param_bounds.iter().flatten() {
                    self.collect_type_generic_bound_errors_inner(bound, seen_aliases, errors);
                }
                for param in params {
                    self.collect_type_generic_bound_errors_inner(&param.ty, seen_aliases, errors);
                }
                self.collect_type_generic_bound_errors_inner(ret, seen_aliases, errors);
                self.collect_type_generic_bound_errors_inner(throws, seen_aliases, errors);
            }
            Ty::Future(value, error, _) => {
                self.collect_type_generic_bound_errors_inner(value, seen_aliases, errors);
                self.collect_type_generic_bound_errors_inner(error, seen_aliases, errors);
            }
            Ty::TypeAlias(qtn, _) => {
                if !seen_aliases.insert(qtn.clone()) {
                    return;
                }
                let expanded = self.expand_alias_chains(ty.clone());
                if !matches!(expanded, Ty::TypeAlias(_, _)) {
                    self.collect_type_generic_bound_errors_inner(&expanded, seen_aliases, errors);
                }
                seen_aliases.remove(qtn);
            }
            _ => {}
        }
    }

    fn collect_class_generic_bound_errors(
        &mut self,
        qtn: &crate::ty::QualifiedTypeName,
        type_args: &[Ty],
        errors: &mut Vec<TirTypeError>,
    ) {
        let Some(class_loc) = self.resolve_class_loc(qtn) else {
            return;
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_hir::file_item_tree(db, class_loc.file(db));
        let Some(class_data) = item_tree.classes.get(&class_loc.id(db)) else {
            return;
        };
        self.collect_named_generic_bound_errors(
            &class_data.generic_params,
            &class_data.generic_param_bounds,
            class_loc.file(db),
            type_args,
            errors,
        );
    }

    fn collect_interface_generic_bound_errors(
        &mut self,
        qtn: &crate::ty::QualifiedTypeName,
        type_args: &[Ty],
        errors: &mut Vec<TirTypeError>,
    ) {
        let Some(interface_loc) = self.resolve_interface_loc(qtn) else {
            return;
        };
        let db = self.context.db();
        let item_tree = baml_compiler2_hir::file_item_tree(db, interface_loc.file(db));
        let Some(interface_data) = item_tree.interfaces.get(&interface_loc.id(db)) else {
            return;
        };
        self.collect_named_generic_bound_errors(
            &interface_data.generic_params,
            &interface_data.generic_param_bounds,
            interface_loc.file(db),
            type_args,
            errors,
        );
    }

    fn collect_named_generic_bound_errors(
        &mut self,
        generic_params: &[Name],
        generic_param_bounds: &[Option<TypeExpr>],
        file: SourceFile,
        type_args: &[Ty],
        errors: &mut Vec<TirTypeError>,
    ) {
        if generic_params.is_empty() || type_args.is_empty() {
            return;
        }
        let db = self.context.db();
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let mut diags = Vec::new();
        let lowered_bounds = lower_generic_param_bounds(
            db,
            generic_param_bounds,
            pkg_items,
            &pkg_info.namespace_path,
            generic_params,
            None,
            &mut diags,
        );
        for diag in diags {
            errors.push(diag);
        }

        let bindings = crate::generics::bind_type_vars(generic_params, type_args);
        for idx in 0..generic_params.len() {
            let Some(actual) = type_args.get(idx) else {
                continue;
            };
            let Some(bound) = lowered_bounds.get(idx).and_then(Option::as_ref) else {
                continue;
            };
            let bound = crate::generics::substitute_ty(bound, &bindings);
            if !self.is_subtype(actual, &bound) {
                errors.push(TirTypeError::TypeMismatch {
                    expected: bound,
                    got: actual.clone(),
                });
            }
        }
    }

    fn function_subtype_with_generic_binders(&self, sub: &Ty, sup: &Ty) -> Option<bool> {
        let Ty::Function {
            generic_params: sub_generic_params,
            generic_param_bounds: sub_generic_param_bounds,
            params: sub_params,
            ret: sub_ret,
            throws: sub_throws,
            ..
        } = sub
        else {
            return None;
        };
        let Ty::Function {
            generic_params: sup_generic_params,
            generic_param_bounds: sup_generic_param_bounds,
            params: sup_params,
            ret: sup_ret,
            throws: sup_throws,
            ..
        } = sup
        else {
            return None;
        };

        if sub_generic_params.is_empty() && sup_generic_params.is_empty() {
            return None;
        }
        if sub_generic_params.len() != sup_generic_params.len() {
            return Some(false);
        }

        let canonical_params: Vec<Name> = (0..sub_generic_params.len())
            .map(|idx| Name::new(format!("__fn_generic_{idx}")))
            .collect();
        let (sub_bounds, sub_fn) = Self::canonicalize_generic_function_for_subtyping(
            sub_generic_params,
            sub_generic_param_bounds,
            sub_params,
            sub_ret,
            sub_throws,
            &canonical_params,
        );
        let (sup_bounds, sup_fn) = Self::canonicalize_generic_function_for_subtyping(
            sup_generic_params,
            sup_generic_param_bounds,
            sup_params,
            sup_ret,
            sup_throws,
            &canonical_params,
        );

        for (sub_bound, sup_bound) in sub_bounds.iter().zip(sup_bounds.iter()) {
            match (sub_bound.as_ref(), sup_bound.as_ref()) {
                (None, _) => {}
                (Some(_), None) => return Some(false),
                (Some(sub_bound), Some(sup_bound)) => {
                    if !self.is_subtype(sup_bound, sub_bound) {
                        return Some(false);
                    }
                }
            }
        }

        Some(crate::normalize::is_subtype_of(
            &sub_fn,
            &sup_fn,
            &self.aliases,
        ))
    }

    fn canonicalize_generic_function_for_subtyping(
        generic_params: &[Name],
        generic_param_bounds: &[Option<Ty>],
        params: &[FunctionParamTy],
        ret: &Ty,
        throws: &Ty,
        canonical_params: &[Name],
    ) -> (Vec<Option<Ty>>, Ty) {
        let mut bindings = FxHashMap::default();
        for (param, canonical) in generic_params.iter().zip(canonical_params.iter()) {
            bindings.insert(
                param.clone(),
                Ty::TypeVar(canonical.clone(), TyAttr::default()),
            );
        }
        let bounds = generic_param_bounds
            .iter()
            .map(|bound| {
                bound
                    .as_ref()
                    .map(|bound| crate::generics::substitute_ty(bound, &bindings))
            })
            .collect();
        let function = Ty::Function {
            generic_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            params: params
                .iter()
                .map(|param| FunctionParamTy {
                    name: param.name.clone(),
                    ty: crate::generics::substitute_ty(&param.ty, &bindings),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(crate::generics::substitute_ty(ret, &bindings)),
            throws: Box::new(crate::generics::substitute_ty(throws, &bindings)),
            attr: TyAttr::default(),
        };
        (bounds, function)
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
        // Peel type aliases once at the entry so downstream classifiers
        // (`infer_arithmetic`, `infer_bitwise`, `is_float_bigint_mix`) only
        // need to recognise the underlying primitive shapes. Mirrors how
        // `is_subtype` and other type-aware sites expand at their entry.
        let expanded_lhs = self.expand_alias_chains(lhs.clone());
        let expanded_rhs = self.expand_alias_chains(rhs.clone());
        let lhs = &expanded_lhs;
        let rhs = &expanded_rhs;
        match op {
            // Equality (`==`, `!=`): permissive — any two operands are
            // accepted and the result is `bool`. This intentionally allows
            // `x == null` (the canonical null check), `int? == int`
            // (nullable equality), and numeric cross-type comparisons like
            // `int == float`. The only rejected pairing is float-vs-bigint:
            // a `bigint` beyond f64's exactly-representable range cannot be
            // compared to a `float` without precision loss, so
            // `is_float_bigint_mix` flags it (matching arithmetic/ordering).
            BinaryOp::Eq | BinaryOp::Ne => {
                if Self::is_float_bigint_mix(lhs, rhs)
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
                Ty::Bool {
                    attr: TyAttr::default(),
                }
            }

            // Ordering (`<`, `<=`, `>`, `>=`): unlike equality, there is no
            // meaningful ordering between `null` and a real value. Reject
            // any operand that could be null (Optional / Union containing
            // `null` / bare `null`) before the float-bigint mix check.
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let invalid_null = (Self::may_be_null(lhs) || Self::may_be_null(rhs))
                    && !matches!(lhs, Ty::Unknown { .. } | Ty::Error { .. })
                    && !matches!(rhs, Ty::Unknown { .. } | Ty::Error { .. });
                let invalid_mix = Self::is_float_bigint_mix(lhs, rhs)
                    && !matches!(lhs, Ty::Unknown { .. } | Ty::Error { .. })
                    && !matches!(rhs, Ty::Unknown { .. } | Ty::Error { .. });
                if invalid_null || invalid_mix {
                    self.context.report_simple(
                        TirTypeError::InvalidBinaryOp {
                            op,
                            lhs: lhs.clone(),
                            rhs: rhs.clone(),
                        },
                        at,
                    );
                }
                Ty::Bool {
                    attr: TyAttr::default(),
                }
            }

            // Logical → bool
            BinaryOp::And | BinaryOp::Or => Ty::Bool {
                attr: TyAttr::default(),
            },

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

            // Bitwise: result type depends on operands (int or bigint).
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                let result = Self::infer_bitwise(lhs, rhs);
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
    /// Whether `ty` is a non-object primitive — `int`/`float`/`bigint`/`bool`/
    /// `null` (and their literals). These are tagged values with no heap object,
    /// so the VM cannot string-concatenate them (`exec_binop` only concatenates
    /// two objects). String-typed and `uint8array`/media values ARE objects.
    fn is_non_object_primitive(ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Int { .. }
                | Ty::Float { .. }
                | Ty::Bigint { .. }
                | Ty::Bool { .. }
                | Ty::Null { .. }
                | Ty::Literal(
                    baml_base::Literal::Int(_)
                        | baml_base::Literal::Float(_)
                        | baml_base::Literal::Bigint(_)
                        | baml_base::Literal::Bool(_),
                    _,
                    _,
                )
        )
    }

    fn infer_arithmetic(op: baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Ty {
        fn promote(a: PrimitiveType, b: &PrimitiveType) -> Option<PrimitiveType> {
            if a == *b {
                return Some(a);
            }
            match (&a, &b) {
                (PrimitiveType::Int, PrimitiveType::Float)
                | (PrimitiveType::Float, PrimitiveType::Int) => Some(PrimitiveType::Float),
                (PrimitiveType::Int, PrimitiveType::Bigint)
                | (PrimitiveType::Bigint, PrimitiveType::Int) => Some(PrimitiveType::Bigint),
                _ => None,
            }
        }

        fn base_ty(ty: &Ty) -> Option<PrimitiveType> {
            // Enumerate the specific primitives this op accepts (Int, Bigint,
            // Float, String). Anything else returns `None`, which makes the
            // outer match fall through to `Ty::Unknown` and surface as an
            // `InvalidBinaryOp` diagnostic. Adding a new `PrimitiveType` or
            // `Literal` variant forces a deliberate opt-in here.
            match ty {
                Ty::Int { .. } => Some(PrimitiveType::Int),
                Ty::Bigint { .. } => Some(PrimitiveType::Bigint),
                Ty::Float { .. } => Some(PrimitiveType::Float),
                Ty::String { .. } => Some(PrimitiveType::String),
                Ty::Literal(baml_base::Literal::Int(_), _, _) => Some(PrimitiveType::Int),
                Ty::Literal(baml_base::Literal::Bigint(_), _, _) => Some(PrimitiveType::Bigint),
                Ty::Literal(baml_base::Literal::Float(_), _, _) => Some(PrimitiveType::Float),
                Ty::Literal(baml_base::Literal::String(_), _, _) => Some(PrimitiveType::String),
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
            // Float / bigint mixing is rejected — bigint values past 2^53 don't
            // round-trip through f64. Users must explicitly convert.
            (Some(PrimitiveType::Float), Some(PrimitiveType::Bigint))
            | (Some(PrimitiveType::Bigint), Some(PrimitiveType::Float)) => Ty::Unknown {
                attr: TyAttr::default(),
            },
            (Some(PrimitiveType::Float), _) | (_, Some(PrimitiveType::Float)) => Ty::Float {
                attr: TyAttr::default(),
            },
            (Some(PrimitiveType::Bigint | PrimitiveType::Int), Some(PrimitiveType::Bigint))
            | (Some(PrimitiveType::Bigint), Some(PrimitiveType::Int)) => Ty::Bigint {
                attr: TyAttr::default(),
            },
            (Some(PrimitiveType::Int), Some(PrimitiveType::Int)) => Ty::Int {
                attr: TyAttr::default(),
            },
            (Some(PrimitiveType::String), _) | (_, Some(PrimitiveType::String)) => {
                // String concatenation, Add only. The VM concatenates any two
                // *objects* via `as_string` (string, uint8array, …), but a
                // non-object primitive (int/float/bigint/bool/null) has no object
                // representation and aborts at runtime. So `string + int` must be
                // a type error here rather than inferring `string` and crashing
                // the VM (F4), while `string + uint8array` stays valid.
                if matches!(op, baml_compiler2_ast::BinaryOp::Add)
                    && !Self::is_non_object_primitive(lhs)
                    && !Self::is_non_object_primitive(rhs)
                {
                    Ty::String {
                        attr: TyAttr::default(),
                    }
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

    /// Returns true if any runtime value of `ty` could be `null` — i.e. the
    /// type is the bare `null` primitive or a union that contains it (a
    /// nullable `T?` lowers to `T | null`).
    ///
    /// Used by the ordering arms of [`infer_binary_op`] to reject operands
    /// whose runtime value might be `null`. Arithmetic / bitwise rely on
    /// their respective `base_ty` classifiers (which return `None` for
    /// `null` and short-circuit via the catch-all to `Unknown`),
    /// so they do not need this helper.
    fn may_be_null(ty: &Ty) -> bool {
        match ty {
            Ty::Null { .. } => true,
            Ty::Union(members, _) => members.iter().any(Self::may_be_null),
            _ => false,
        }
    }

    /// Returns true if the common upcast of `lhs` and `rhs` would contain
    /// both `float` and `bigint`, which has no sound comparison (bigint
    /// values past 2^53 don't round-trip through f64).
    ///
    /// Models the "is there a valid common type for these two values?"
    /// question used by the equality and ordering arms of
    /// [`infer_binary_op`]. `int? == int` upcasts both sides to `int?` and
    /// is fine; `float? == bigint` upcasts to `float | null | bigint`,
    /// which still pairs float with bigint inside the upcast — so even
    /// though only one runtime branch realizes the bad pairing, the type
    /// itself is unsound.
    ///
    /// Note this is conservative for a single union that already contains
    /// both: `(float | bigint) == (float | bigint)` (and even `x == x` for
    /// such an `x`) is rejected, because the type admits a float-vs-bigint
    /// pairing even though any one concrete value is only ever one
    /// representation. Narrow the operand first if you hit this.
    fn is_float_bigint_mix(lhs: &Ty, rhs: &Ty) -> bool {
        /// `(could_be_float, could_be_bigint)` for a primitive/literal/union
        /// type — i.e. the set of primitive shapes any runtime branch could
        /// carry. A nullable `T | null` is unwrapped through its members here
        /// because this helper asks about the *upcast* type, not about whether
        /// the operand itself is a valid scalar (the latter check belongs at
        /// the operator's arm entry, e.g. ordering rejecting nullable operands
        /// via `may_be_null`).
        fn shape(ty: &Ty) -> (bool, bool) {
            match ty {
                Ty::Float { .. } | Ty::Literal(baml_base::Literal::Float(_), _, _) => (true, false),
                Ty::Bigint { .. } | Ty::Literal(baml_base::Literal::Bigint(_), _, _) => {
                    (false, true)
                }
                Ty::Union(members, _) => members.iter().fold((false, false), |(f, b), m| {
                    let (mf, mb) = shape(m);
                    (f || mf, b || mb)
                }),
                _ => (false, false),
            }
        }
        let (lhs_float, lhs_bigint) = shape(lhs);
        let (rhs_float, rhs_bigint) = shape(rhs);
        // The upcast of the two operands contains both `float` and `bigint`
        // iff either side could be float **and** either side could be bigint.
        // (A single side containing both — e.g. `float | bigint` — is just as
        // unsound as the classic cross-side case `float vs bigint`.)
        (lhs_float || rhs_float) && (lhs_bigint || rhs_bigint)
    }

    /// Determine the result type of a bitwise operation.
    ///
    /// `(int, int) -> int`, `(bigint, bigint) -> bigint`, `(int, bigint) -> bigint`.
    fn infer_bitwise(lhs: &Ty, rhs: &Ty) -> Ty {
        fn base_ty(ty: &Ty) -> Option<PrimitiveType> {
            // Bitwise only accepts Int and Bigint. Float / String / Bool
            // return `None` so the outer match falls through to `Unknown`
            // (which surfaces as `InvalidBinaryOp`).
            match ty {
                Ty::Int { .. } => Some(PrimitiveType::Int),
                Ty::Bigint { .. } => Some(PrimitiveType::Bigint),
                Ty::Literal(baml_base::Literal::Int(_), _, _) => Some(PrimitiveType::Int),
                Ty::Literal(baml_base::Literal::Bigint(_), _, _) => Some(PrimitiveType::Bigint),
                Ty::Union(members, _) => {
                    let mut result: Option<PrimitiveType> = None;
                    for m in members {
                        let p = base_ty(m)?;
                        result = Some(match (result, p) {
                            (None, p) => p,
                            (Some(a), b) if a == b => a,
                            // `int | bigint` members widen to `bigint`, matching
                            // the mixed-operand rule in the outer match below.
                            (Some(PrimitiveType::Int), PrimitiveType::Bigint)
                            | (Some(PrimitiveType::Bigint), PrimitiveType::Int) => {
                                PrimitiveType::Bigint
                            }
                            _ => return None,
                        });
                    }
                    result
                }
                _ => None,
            }
        }

        match (base_ty(lhs), base_ty(rhs)) {
            (Some(PrimitiveType::Int), Some(PrimitiveType::Int)) => Ty::Int {
                attr: TyAttr::default(),
            },
            (Some(PrimitiveType::Bigint | PrimitiveType::Int), Some(PrimitiveType::Bigint))
            | (Some(PrimitiveType::Bigint), Some(PrimitiveType::Int)) => Ty::Bigint {
                attr: TyAttr::default(),
            },
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
            baml_compiler2_ast::UnaryOp::Not => Ty::Bool { attr: operand_attr },
            baml_compiler2_ast::UnaryOp::Neg => match operand {
                Ty::Int { attr } => Ty::Int { attr: attr.clone() },
                Ty::Float { attr } => Ty::Float { attr: attr.clone() },
                Ty::Bigint { attr } => Ty::Bigint { attr: attr.clone() },
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
                LiteralValue::Bigint(n) => Some(Ty::Literal(
                    LiteralValue::Bigint(-n.clone()),
                    f,
                    TyAttr::default(),
                )),
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

        // Bigint × Bigint
        if let (LiteralValue::Bigint(a), LiteralValue::Bigint(b)) = (lhs_lit, rhs_lit) {
            return Self::try_fold_bigint_binary(op, a, b, f);
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

    /// Fold a `Bigint × Bigint` binary operation at the literal-type level.
    ///
    /// Returns `None` when the operation would not survive at runtime — division
    /// or modulo by zero, negative shift counts, or a result whose bit-length
    /// exceeds `baml_type::MAX_BIGINT_BITS` (the VM's allocation cap). Treating
    /// those as non-foldable matches the runtime behavior: the user-visible
    /// effect is the same as if the literal types had never been narrowed,
    /// rather than producing a literal type the VM could not actually
    /// materialize.
    fn try_fold_bigint_binary(
        op: baml_compiler2_ast::BinaryOp,
        a: &num_bigint::BigInt,
        b: &num_bigint::BigInt,
        f: crate::ty::Freshness,
    ) -> Option<Ty> {
        use baml_compiler2_ast::BinaryOp;
        use num_bigint::Sign;

        use crate::ty::LiteralValue;

        // Reject if the bit-length of `n` would trip the VM's allocation cap.
        let within_cap =
            |n: &num_bigint::BigInt| -> bool { n.bits() <= baml_type::MAX_BIGINT_BITS };

        let lit_bigint = |n: num_bigint::BigInt| -> Option<Ty> {
            if !within_cap(&n) {
                return None;
            }
            Some(Ty::Literal(LiteralValue::Bigint(n), f, TyAttr::default()))
        };
        let lit_bool = |v: bool| -> Option<Ty> {
            Some(Ty::Literal(LiteralValue::Bool(v), f, TyAttr::default()))
        };

        match op {
            BinaryOp::Add => lit_bigint(a + b),
            BinaryOp::Sub => lit_bigint(a - b),
            BinaryOp::Mul => {
                // Pre-flight check matching `Instruction::MulBigint`.
                if a.bits().saturating_add(b.bits()) > baml_type::MAX_BIGINT_BITS {
                    return None;
                }
                lit_bigint(a * b)
            }
            BinaryOp::Div => {
                if b.sign() == Sign::NoSign {
                    return None;
                }
                lit_bigint(a / b)
            }
            BinaryOp::Mod => {
                if b.sign() == Sign::NoSign {
                    return None;
                }
                lit_bigint(a % b)
            }
            BinaryOp::BitAnd => lit_bigint(a & b),
            BinaryOp::BitOr => lit_bigint(a | b),
            BinaryOp::BitXor => lit_bigint(a ^ b),
            BinaryOp::Shl => {
                if b.sign() == Sign::Minus {
                    return None;
                }
                // Shl and Shr handle huge counts asymmetrically *on purpose*.
                // Shl growth is bounded by `MAX_BIGINT_BITS`; if the count
                // overflows `usize` (or would push the result past the cap)
                // we refuse to fold, deferring to the runtime which raises
                // `AllocFailure` rather than producing a value the VM
                // couldn't materialise. Shr cannot grow the value, so a
                // count past `usize::MAX` simply saturates to `0n` / `-1n`
                // — see the comment in the `Shr` arm.
                let shift = usize::try_from(b).ok()?;
                let shift_u64 = u64::try_from(shift).ok()?;
                if a.bits().saturating_add(shift_u64) > baml_type::MAX_BIGINT_BITS {
                    return None;
                }
                lit_bigint(a << shift)
            }
            BinaryOp::Shr => {
                if b.sign() == Sign::Minus {
                    return None;
                }
                // Mirror `Instruction::ShrBigint`: counts that don't fit in
                // `usize` saturate to 0n (or -1n for negative operands).
                // (The asymmetry with `Shl` is intentional — see that arm.)
                match usize::try_from(b) {
                    Ok(shift) => lit_bigint(a >> shift),
                    Err(_) => lit_bigint(if a.sign() == Sign::Minus {
                        num_bigint::BigInt::from(-1)
                    } else {
                        num_bigint::BigInt::ZERO
                    }),
                }
            }
            BinaryOp::Eq => lit_bool(a == b),
            BinaryOp::Ne => lit_bool(a != b),
            BinaryOp::Lt => lit_bool(a < b),
            BinaryOp::Le => lit_bool(a <= b),
            BinaryOp::Gt => lit_bool(a > b),
            BinaryOp::Ge => lit_bool(a >= b),
            _ => None,
        }
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
        let saved_interface_method_generic_params =
            std::mem::take(&mut self.interface_method_generic_params);
        let saved_interface_default_owner_type_arg_bindings =
            std::mem::take(&mut self.interface_default_owner_type_arg_bindings);
        let saved_self_pinned_rigid_var = std::mem::take(&mut self.self_pinned_rigid_var);
        let saved_lambda_effective_throws = std::mem::take(&mut self.lambda_effective_throws);
        let saved_call_plans = std::mem::take(&mut self.call_plans);
        let saved_call_type_instantiations = std::mem::take(&mut self.call_type_instantiations);
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

        let lambda_diagnostics_start = self.context.diagnostic_count();

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
        self.context
            .remap_diagnostics_after(lambda_diagnostics_start, lambda_source_map);

        let effective_facts = self.collect_effective_throws(lambda_body);
        let lambda_effective_throws = Self::ty_from_concrete_facts(&effective_facts)
            .or_else(|| {
                // A rigid TypeVar fact — an ENCLOSING function's generic param,
                // e.g. the `E` contributed by `await f` (`f: Future<T, E>`)
                // inside a std combinator's lambda — is concrete-enough for
                // the effective-throws surface: it resolves at the outer call
                // site like any rigid type. Only genuinely open facts (fresh
                // effect slots, Unknown/recovery types) keep the surface open
                // and collapse to `Never` below.
                let all_rigid_or_concrete = !effective_facts.is_empty()
                    && effective_facts.iter().all(|fact| match fact {
                        Ty::TypeVar(name, _) => {
                            self.generic_params.contains(name)
                                && !crate::ty::is_synthetic_effect_param(name)
                        }
                        Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. } => false,
                        _ => true,
                    });
                all_rigid_or_concrete.then(|| {
                    let mut iter = effective_facts.iter();
                    let first = iter.next().expect("non-empty checked above").clone();
                    iter.fold(first, |acc, fact| crate::generics::union_ty(&acc, fact))
                })
            })
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
        self.interface_method_generic_params = saved_interface_method_generic_params;
        self.interface_default_owner_type_arg_bindings =
            saved_interface_default_owner_type_arg_bindings;
        self.self_pinned_rigid_var = saved_self_pinned_rigid_var;
        self.lambda_effective_throws = saved_lambda_effective_throws;
        self.call_plans = saved_call_plans;
        self.call_type_instantiations = saved_call_type_instantiations;
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
            Ty::Bool { .. } => vec![
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
            Ty::Null { .. } => vec![Ctor::Single(ty.clone())],
            // Infinite-alphabet / opaque primitives and types — all
            // require a wildcard arm for exhaustiveness.
            Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Map { .. }
            | Ty::EvolvingMap(..)
            | Ty::Function { .. }
            | Ty::Type { .. }
            | Ty::RustType { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Void { .. }
            | Ty::BuiltinUnknown { .. }
            | Ty::Unknown { .. }
            | Ty::Error { .. }
            | Ty::WatchAccessor(..)
            | Ty::TypeVar(_, _)
            | Ty::AssociatedTypeProjection { .. } => vec![Ctor::NonExhaustive],
            Ty::Never { .. } => vec![],
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
            Ty::Class(qtn, args, _) => vec![Ctor::Class(qtn.clone(), args.clone())],
            // Open interfaces always require a wildcard — new implementors
            // can appear in any file. See BEP-044 §"Interaction with match".
            Ty::Interface(_, _, _, _) => vec![Ctor::NonExhaustive],
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

    fn interface_field_types(&self, ty: &Ty) -> Vec<Ty> {
        self.interface_field_infos_ordered_for_ty(ty)
            .into_iter()
            .map(|(_, ft)| self.matrix_normalize_scrut(&ft))
            .collect()
    }

    fn interface_field_projection_for_class(
        &self,
        iface_ty: &Ty,
        class_qtn: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
    ) -> Option<Vec<usize>> {
        let Ty::Interface(iface_qtn, iface_args, _, _) = iface_ty else {
            return None;
        };
        let class_fields = self.class_field_infos_ordered(class_qtn, class_type_args);
        let class_indices: FxHashMap<Name, usize> = class_fields
            .into_iter()
            .enumerate()
            .map(|(idx, (name, _))| (name, idx))
            .collect();
        let interface_fields = self.interface_field_infos_ordered_for_ty(iface_ty);
        let mut projection = Vec::with_capacity(interface_fields.len());
        for (interface_field, _) in interface_fields {
            let class_field = self.class_field_name_for_interface_field(
                class_qtn,
                class_type_args,
                iface_qtn,
                iface_args,
                &interface_field,
            )?;
            projection.push(*class_indices.get(&class_field)?);
        }
        Some(projection)
    }

    fn interface_ctor_covers_column(&self, iface_ty: &Ty, col_ty: &Ty) -> bool {
        self.is_subtype(col_ty, iface_ty)
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
            let current_ty = if matches!(binding.ty, Ty::Never { .. }) {
                declared_for_scope
                    .cloned()
                    .unwrap_or_else(|| binding.ty.clone())
            } else {
                binding.ty.clone()
            };
            self.declare_scoped_local(
                binding.name.clone(),
                binding.pat_id,
                current_ty,
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
            && !self.pattern_overlaps_scrut_member(&pat_natural, &scrut_for_check)
        {
            let err = TirTypeError::TypeMismatch {
                expected: scrut_ty.clone(),
                got: pat_natural,
            };
            self.report_at_pat_or_expr(err, pat_id, at_expr);
        }
    }

    /// A match arm is valid if its pattern overlaps *any* member of a
    /// union/optional scrutinee — the arm matches that member's values even
    /// when other members don't (`null`, or an unrelated class). Without this,
    /// `let a: Animal => …` over `(Dog | Cat)?` is wrongly rejected because the
    /// whole `Dog | Cat | null` isn't a subtype of `Animal` (the `null` arm) and
    /// `Animal` isn't a subtype of the union either.
    fn pattern_overlaps_scrut_member(&self, pat: &Ty, scrut: &Ty) -> bool {
        let members = self.flatten_union_optional_members(scrut);
        // Only a genuine union/optional contributes >1 member; a scalar scrut
        // yields itself and was already covered by the two `is_subtype` checks.
        members.len() > 1
            && members
                .iter()
                .any(|m| self.is_subtype(pat, m) || self.is_subtype(m, pat))
    }

    /// Flatten a (possibly nested) union type into its leaf members. A nullable
    /// `T?` lowers to `T | null`, so its `null` member surfaces here too.
    fn flatten_union_optional_members(&self, ty: &Ty) -> Vec<Ty> {
        match self.expand_alias_chains(ty.clone()) {
            Ty::Union(members, _) => members
                .iter()
                .flat_map(|m| self.flatten_union_optional_members(m))
                .collect(),
            other => vec![other],
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
                // `pattern_types` is single-valued per PatId, but lowering the
                // same nested sub-patterns once per union member overwrites
                // their entries last-write-wins (the `insert` in
                // `analyze_and_lower_no_subtype_check`). MIR reads those
                // per-PatId entries to emit each pattern's runtime `is`-type
                // test, so keeping only the last member makes e.g. `x: A | B`
                // test only `B` — values of earlier members never match. Join
                // the per-member writes here so the recorded type spans every
                // member, mirroring how `matched_ty` / `bindings` are joined
                // across members below. (MIR already ORs a union's members.)
                let pt_before = self.pattern_types.clone();
                let mut pt_joined: FxHashMap<PatId, Ty> = FxHashMap::default();
                for member_ty in &targets {
                    let inner = self.analyze_and_lower_inner(pat_id, member_ty, body, at_expr);
                    for (k, v) in &self.pattern_types {
                        if pt_before.get(k) != Some(v) {
                            pt_joined
                                .entry(*k)
                                .and_modify(|acc| {
                                    *acc = Self::join_all(&[acc.clone(), v.clone()]);
                                })
                                .or_insert_with(|| v.clone());
                        }
                    }
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
                // Publish the per-member-joined nested pattern types, undoing
                // the last-write-wins clobber from the loop above.
                for (k, v) in pt_joined {
                    self.pattern_types.insert(k, v);
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
    /// shares a runtime identity with some atom of `b`. Unions (including a
    /// nullable `T | null`) decompose into atoms; everything else is a single
    /// atom matched by [`Self::atoms_overlap`].
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
            (Ty::Int { .. }, Ty::Int { .. })
            | (Ty::Bigint { .. }, Ty::Bigint { .. })
            | (Ty::Float { .. }, Ty::Float { .. })
            | (Ty::String { .. }, Ty::String { .. })
            | (Ty::Bool { .. }, Ty::Bool { .. })
            | (Ty::Null { .. }, Ty::Null { .. })
            | (Ty::Uint8Array { .. }, Ty::Uint8Array { .. }) => true,
            (Ty::Media(k1, _), Ty::Media(k2, _)) => k1 == k2,
            // Literal vs primitive: literal's primitive head must match.
            (Ty::Literal(baml_base::Literal::Int(_), _, _), Ty::Int { .. })
            | (Ty::Int { .. }, Ty::Literal(baml_base::Literal::Int(_), _, _))
            | (Ty::Literal(baml_base::Literal::Bigint(_), _, _), Ty::Bigint { .. })
            | (Ty::Bigint { .. }, Ty::Literal(baml_base::Literal::Bigint(_), _, _))
            | (Ty::Literal(baml_base::Literal::Float(_), _, _), Ty::Float { .. })
            | (Ty::Float { .. }, Ty::Literal(baml_base::Literal::Float(_), _, _))
            | (Ty::Literal(baml_base::Literal::String(_), _, _), Ty::String { .. })
            | (Ty::String { .. }, Ty::Literal(baml_base::Literal::String(_), _, _))
            | (Ty::Literal(baml_base::Literal::Bool(_), _, _), Ty::Bool { .. })
            | (Ty::Bool { .. }, Ty::Literal(baml_base::Literal::Bool(_), _, _)) => true,
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
                Ty::Map {
                    key: a_k,
                    value: a_v,
                    ..
                }
                | Ty::EvolvingMap(a_k, a_v, _),
                Ty::Map {
                    key: b_k,
                    value: b_v,
                    ..
                }
                | Ty::EvolvingMap(b_k, b_v, _),
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
            // BEP-044: an interface atom overlaps a value whose runtime class
            // could satisfy it — the same/`requires`-related interface, or a
            // class that nominally implements it. This is genuine runtime-tag
            // overlap (a `Dog` value IS matched by an `Animal` pattern), so it's
            // sound here unlike the numeric-tower subtype bridge guarded against
            // above. Without it, `let a: Animal` against `Animal | string`
            // targets no union member and degrades to a catch-all.
            (Ty::Interface(..), _) | (_, Ty::Interface(..)) => {
                self.is_subtype(a, b) || self.is_subtype(b, a)
            }
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
                associated_type_bindings,
                ..
            } => {
                self.resolve_class_pattern_type(class, generic_args, associated_type_bindings, None)
            }
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
                associated_type_bindings,
                fields,
            } => self.lower_class_pat(
                class,
                generic_args,
                associated_type_bindings,
                fields,
                pat_id,
                scrut_ty,
                body,
                at_expr,
            ),
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
            Ty::Null { .. } => DPat::single(expanded.clone(), scrut_ty.clone()),
            // Finite enumerations: build an Or of singletons.
            Ty::Bool { .. } => Self::or_of_singletons(
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
                DPat::class_inst(qtn.clone(), args.clone(), fields, scrut_ty.clone())
            }
            Ty::Interface(_, _, _, _) => {
                let field_tys = self
                    .interface_field_infos_ordered_for_ty(&expanded)
                    .into_iter()
                    .map(|(_, ty)| ty);
                let fields = field_tys.map(DPat::wildcard).collect();
                DPat::interface(expanded.clone(), fields, scrut_ty.clone())
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
        associated_type_bindings: &[baml_compiler2_ast::AssociatedTypeBinding],
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
        let class_ty = self.resolve_class_pattern_type(
            class,
            generic_args,
            associated_type_bindings,
            Some((pat_id, at_expr)),
        );

        // BEP-044: destructuring an interface head (`Animal { name, age }`).
        // The interface has no positional field layout — it matches any
        // implementor and binds each named field through the interface's
        // field view (the same projection used by `iface_value.field`). Field
        // *types* come from the interface field view; the runtime extraction
        // is wired by `project_interface_pattern_field` in MIR. Because every
        // implementor necessarily provides the interface's declared fields,
        // elided fields are wildcarded, while named fields preserve their
        // lowered subpatterns for exhaustiveness/refutability.
        if let Ty::Interface(pattern_iface_qtn, pattern_args, pattern_assoc, attr) = &class_ty {
            let effective_interface_ty = match (
                pattern_assoc.is_empty(),
                self.expand_alias_chains(scrut_ty.clone()),
            ) {
                (true, Ty::Interface(scrut_iface_qtn, scrut_args, scrut_assoc, scrut_attr))
                    if pattern_iface_qtn == &scrut_iface_qtn
                        && (pattern_args.is_empty()
                            || (pattern_args.len() == scrut_args.len()
                                && pattern_args
                                    .iter()
                                    .zip(scrut_args.iter())
                                    .all(|(a, b)| self.types_equivalent(a, b)))) =>
                {
                    Ty::Interface(
                        pattern_iface_qtn.clone(),
                        if pattern_args.is_empty() {
                            scrut_args
                        } else {
                            pattern_args.clone()
                        },
                        scrut_assoc,
                        scrut_attr,
                    )
                }
                _ => Ty::Interface(
                    pattern_iface_qtn.clone(),
                    pattern_args.clone(),
                    pattern_assoc.clone(),
                    attr.clone(),
                ),
            };
            let mut by_name: FxHashMap<Name, &ast::FieldPat> = FxHashMap::default();
            for fp in fields {
                by_name.insert(fp.field.clone(), fp);
            }

            let field_infos = self.interface_field_infos_ordered_for_ty(&effective_interface_ty);
            let mut declared_fields: FxHashSet<Name> = FxHashSet::default();
            let mut sub_dpats: Vec<DPat> = Vec::with_capacity(field_infos.len());
            let mut bindings: Vec<PatternBinding> = Vec::new();
            for (field_name, field_ty) in field_infos {
                declared_fields.insert(field_name.clone());
                match by_name.get(&field_name) {
                    Some(fp) => {
                        let r = self.analyze_and_lower(fp.pat, &field_ty, body, at_expr);
                        sub_dpats.push(r.dpat);
                        bindings.extend(r.bindings);
                    }
                    None => sub_dpats.push(DPat::wildcard(field_ty)),
                }
            }

            for fp in fields {
                if declared_fields.contains(&fp.field) {
                    continue;
                }
                self.report_at_pat_or_expr(
                    TirTypeError::UnresolvedMember {
                        base_type: effective_interface_ty.clone(),
                        member: fp.field.clone(),
                    },
                    pat_id,
                    at_expr,
                );
                let unknown = Ty::Unknown {
                    attr: TyAttr::default(),
                };
                let r = self.analyze_and_lower(fp.pat, &unknown, body, at_expr);
                bindings.extend(r.bindings);
            }
            let dpat = DPat::interface(effective_interface_ty.clone(), sub_dpats, scrut_ty.clone());
            let matched_ty = self.intersect_pattern_flow_types(scrut_ty, &effective_interface_ty);
            return PatternResult {
                dpat,
                required_ty: Some(effective_interface_ty),
                matched_ty,
                bindings,
            };
        }

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
            dpat: DPat::class_inst(qtn, args, sub_dpats, scrut_ty.clone()),
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
            Ctor::Class(qtn, _) => {
                let names = self.class_field_names_ordered(qtn);
                let qtn_str = qtn.render_user_facing();
                if w.fields.is_empty() {
                    return format!("{qtn_str} {{}}");
                }
                let mut out = format!("{qtn_str} {{ ");
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
        "int" => Some(Ty::Int {
            attr: TyAttr::default(),
        }),
        "bigint" => Some(Ty::Bigint {
            attr: TyAttr::default(),
        }),
        "float" => Some(Ty::Float {
            attr: TyAttr::default(),
        }),
        "string" => Some(Ty::String {
            attr: TyAttr::default(),
        }),
        "bool" => Some(Ty::Bool {
            attr: TyAttr::default(),
        }),
        "null" => Some(Ty::Null {
            attr: TyAttr::default(),
        }),
        "image" => Some(Ty::Media(MediaKind::Image, TyAttr::default())),
        "audio" => Some(Ty::Media(MediaKind::Audio, TyAttr::default())),
        "video" => Some(Ty::Media(MediaKind::Video, TyAttr::default())),
        "pdf" => Some(Ty::Media(MediaKind::Pdf, TyAttr::default())),
        _ => None,
    }
}
