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
    self as ast, AstSourceMap, Expr, ExprBody, ExprId, PatId, Stmt, StmtId, TypeExpr, TypeExprKind,
};
use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, FunctionLoc},
    package::{PackageId, PackageItems},
    scope::{FileScopeId, ScopeId},
    semantic_index::{ExprMetadataKey, ExprMetadataScope, PathResolution},
};
// The trait must be in scope so the builder (which implements it) can call the
// defaulted type-algebra methods on itself — `self.is_subtype(a, b)`.
use baml_type::normalize::TypeContext;
use rustc_hash::{FxHashMap, FxHashSet};
use text_size::TextRange;

use crate::{
    infer_context::{
        InferContext, RelatedLocation, RelatedNote, TirTypeError, TypeCheckDiagnostics,
    },
    inference::MemberResolution,
    lower_type_expr::TypeVarBoundsMap,
    package_interface::PackageResolutionContext,
    throws_analysis::ThrowsAnalysisContext,
    ty::{Freshness, FunctionParamMode, FunctionParamTy, MediaKind, PrimitiveType, Ty, TyAttr},
    type_context::GlobalTypeContext,
};

pub(crate) mod associated_projection;
/// Interface member/method resolution for existential / type-var receivers.
pub(crate) mod interface_resolution;
use interface_resolution::UnionMemberResolution;

pub(crate) fn duplicate_parameter_names<'a>(
    names: impl IntoIterator<Item = &'a Name>,
) -> FxHashSet<Name> {
    let mut seen = FxHashSet::default();
    let mut duplicates = FxHashSet::default();
    for name in names {
        if !seen.insert(name.clone()) {
            duplicates.insert(name.clone());
        }
    }
    duplicates
}

pub(crate) fn parameter_binding_ty(
    name: &Name,
    declared_ty: &Ty,
    duplicate_names: &FxHashSet<Name>,
) -> Ty {
    if duplicate_names.contains(name) {
        Ty::Error {
            attr: TyAttr::default(),
        }
    } else {
        declared_ty.clone()
    }
}

// ── Well-known type constructors ──────────────────────────────────────────────
//
// These helpers construct `Ty` values for well-known types that appear in
// synthesized method signatures (e.g., the universal `to_json`/`from_json` on
// `Ty::TypeVar`). They are free functions so they can be called from both
// `resolve_member` (mutable context) and `try_resolve_member_on_ty` (shared).

/// The interface bound to search for a member — the receiver's declared interface, whose
/// `requires`-closure is walked. For an existential receiver this is the receiver's own
/// interface; for a `T extends I` / `H.Item` receiver it's the constraint that bounds it.
#[derive(Clone, Copy)]
struct InterfaceBound<'a> {
    name: &'a crate::ty::QualifiedTypeName,
    type_args: &'a [Ty],
    associated_bindings: &'a [(Name, Ty)],
}

/// A member access to resolve: which member, its source expression (for diagnostics), and
/// whether it's a bound access (`recv.m()` / field read — `self` stripped) versus an
/// unbound method value (`recv.m` — `self` kept).
#[derive(Clone, Copy)]
struct MemberAccess<'a> {
    member: &'a Name,
    at: ExprId,
    bound: bool,
}

enum ClassFieldLookup {
    Found(Ty),
    Duplicate,
    NotFound,
}

enum ClassMethodLookup<'db> {
    Found {
        ty: Ty,
        class_loc: ClassLoc<'db>,
        func_loc: FunctionLoc<'db>,
    },
    DuplicateInherent,
    DeferToInterfaces,
    NotFound,
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

/// Construct `Ty::Class` for `baml.json.JsonDecodeError` — the throws of the
/// `from_json` / `baml.json.to` decode family (decoding never re-parses).
fn json_decode_error_ty() -> Ty {
    Ty::Class(
        crate::ty::QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("json")],
            Name::new("JsonDecodeError"),
        ),
        vec![],
        TyAttr::default(),
    )
}

/// The exact builtin type accepted by the call-site `$id` side channel.
fn boundary_local_id_ty() -> Ty {
    Ty::Class(
        crate::ty::QualifiedTypeName::new(
            Name::new(baml_builtins2::PACKAGE_BOUNDARY),
            vec![],
            Name::new("LocalId"),
        ),
        vec![],
        TyAttr::default(),
    )
}

fn baml_iter_interface_qtn(name: &str) -> crate::ty::QualifiedTypeName {
    crate::ty::QualifiedTypeName::new(Name::new("baml"), vec![Name::new("iter")], Name::new(name))
}

/// Per-callee generic-parameter facts consumed at every call site:
/// the enclosing class's generic params (empty for free functions), the
/// callee's user-declared params, their lowered interface bounds, and the
/// callee's name for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CalleeGenerics {
    pub(crate) class_params: Vec<crate::ty::ParamTy>,
    pub(crate) user_params: Vec<crate::ty::ParamTy>,
    /// Parallel to `user_params`: each param's full `extends A & B` bound
    /// conjunction, empty when the param is unbounded.
    pub(crate) user_bounds: Vec<Vec<Ty>>,
    pub(crate) name: Name,
}

// Safety: `CalleeGenerics` holds only plain (non-`'db`) data. Manual `Update`
// impl uses `PartialEq` for Salsa early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for CalleeGenerics {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// The declared generic params and lowered bounds of a callable, memoized per
/// function. Every call site used to redo this from scratch — an item-tree
/// scan for the enclosing class plus a re-lowering of the bound type
/// expressions — even though it is a pure function of the callee's
/// declaration, so memoizing per `FunctionLoc` is safe.
#[salsa::tracked(returns(ref))]
pub(crate) fn callee_generics_for_func<'db>(
    db: &'db dyn crate::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> CalleeGenerics {
    let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
    // The enclosing class's generic params (`[]` for a free function). These
    // are always in scope when lowering the method's *bounds* — a bound may
    // reference a class generic (`<U extends Eq<C>>` on a method of
    // `class Box<C>`) regardless of how the class args are supplied — so they
    // must be visible or `C` would resolve to `unknown`.
    let file = func_loc.file(db);
    // Enclosing class params via the firewall `method_owner` index (O(1)) rather
    // than an O(classes) `methods.contains` scan — identical result (a class
    // method's owner class's params, else none).
    let env = crate::generic_env::function_generic_env(db, func_loc);
    let class_params: Vec<crate::ty::ParamTy> =
        match baml_compiler2_ppir::item_data::method_owner(db, func_loc) {
            Some(baml_compiler2_ppir::item_data::MethodOwner::Class(class_loc)) => {
                crate::generic_env::class_generic_env(db, class_loc)
                    .params()
                    .to_vec()
            }
            _ => Vec::new(),
        };
    let user_params = sig
        .user_generic_params
        .iter()
        .map(|name| {
            env.resolve_param(name)
                .expect("function generic parameter is in its environment")
                .clone()
        })
        .collect::<Vec<_>>();

    // Lower the user generic params' interface bounds in the callee's own
    // package/namespace, with the class params in scope (see above). The
    // bounds were already validated at the declaration site, so discard
    // re-lowering diagnostics here.
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let mut bound_scope = class_params.clone();
    bound_scope.extend(user_params.iter().cloned());
    let mut diags = Vec::new();
    let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
    let user_bounds = lower_generic_param_bound_refs(
        db,
        &func_data.type_refs,
        &func_data.generic_params,
        pkg_items,
        &pkg_info.namespace_path,
        &bound_scope,
        None,
        &mut diags,
    );

    CalleeGenerics {
        class_params,
        user_params,
        user_bounds,
        name: sig.name.clone(),
    }
}

/// Lower each declared generic parameter's bound *conjunction* (`TypeRefId`s
/// into `store`, from firewall data — `function_data` / `class_data` / …) to its
/// `Ty`s, in declaration order.
///
/// The result is parallel to `params`; the inner `Vec` holds every
/// `&`-separated conjunct the parameter declared and is empty when it is
/// unbounded.
#[expect(clippy::too_many_arguments)]
pub(crate) fn lower_generic_param_bound_refs(
    db: &dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    params: &[baml_compiler2_ppir::item_data::GenericParamData],
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    generic_params: &[crate::ty::ParamTy],
    bindings: Option<&FxHashMap<crate::ty::ParamTy, Ty>>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Vec<Vec<Ty>> {
    // The in-scope names under `bindings` are the same for every bound — snapshot once.
    let binding_params: Vec<crate::ty::ParamTy> = bindings
        .map(|bindings| bindings.keys().cloned().collect())
        .unwrap_or_default();
    params
        .iter()
        .map(|param| {
            param
                .bounds
                .iter()
                .map(|&bound| {
                    if let Some(bindings) = bindings {
                        crate::generics::substitute_ty(
                            &crate::lower_type_expr::lower_constraint_head_type_ref(
                                store,
                                bound,
                                &crate::lower_type_expr::ScopeCtx {
                                    db,
                                    package_items: pkg_items,
                                    ns_context,
                                    generic_params: &binding_params,
                                    bounds: &TypeVarBoundsMap::default(),
                                    self_ty: None,
                                },
                                diagnostics,
                            ),
                            bindings,
                        )
                    } else {
                        crate::lower_type_expr::lower_constraint_head_type_ref(
                            store,
                            bound,
                            &crate::lower_type_expr::ScopeCtx {
                                db,
                                package_items: pkg_items,
                                ns_context,
                                generic_params,
                                bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
                                self_ty: None,
                            },
                            diagnostics,
                        )
                    }
                })
                .collect()
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

/// Which container kind a literal in checking position is — selects the shape
/// [`TypeInferenceBuilder::adopted_container_for_literal`] looks for in the
/// expected type.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ContainerLiteralKind {
    List,
    Map,
}

impl ContainerLiteralKind {
    fn matches(self, ty: &Ty) -> bool {
        match self {
            ContainerLiteralKind::List => matches!(ty, Ty::List(..) | Ty::EvolvingList(..)),
            ContainerLiteralKind::Map => matches!(ty, Ty::Map { .. } | Ty::EvolvingMap(..)),
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

#[derive(Clone, Copy)]
enum NamedPatternFieldOwner<'a> {
    Class(&'a crate::ty::QualifiedTypeName),
    Interface(&'a Ty),
}

struct LoweredNamedPatternFields {
    sub_dpats: Vec<crate::exhaustiveness::DPat>,
    bindings: Vec<crate::pattern_lowering::PatternBinding>,
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
        call_expr_id: ExprId,
        callee_expr_id: ExprId,
        args: &[ExprId],
        unwrap_optional_callee: bool,
    ) -> Option<Ty> {
        let call_plan = self.builder.call_plans.get(&call_expr_id);
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

    fn runtime_id_set_throws(&self) -> Option<BTreeSet<Ty>> {
        // Throw-set keys are namespace-relative within their package: the
        // builtin lives in package `baml`, namespace `id`, so its key is
        // `id.set` (own-package lookup covers compiling the std lib itself).
        self.builder
            .lookup_named_throw_summary(&Name::new("id.set"))
    }

    fn to_json_fallback_throws(&self) -> Option<BTreeSet<Ty>> {
        // The `recv.to_json()` sugar lowers to `baml.json.from(recv)`; charge
        // that function's throws (`JsonSerializationError`). Namespace-relative
        // key `json.from` within package `baml`, like `runtime_id_set_throws`.
        self.builder
            .lookup_named_throw_summary(&Name::new("json.from"))
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_json_fallback_throws(&self) -> Option<BTreeSet<Ty>> {
        // The `Type.from_json(j)` sugar lowers to `baml.json.to<Type>(j)`; charge
        // that function's throws (`JsonDecodeError`). Namespace-relative key
        // `json.to` within package `baml`.
        self.builder
            .lookup_named_throw_summary(&Name::new("json.to"))
    }
}

/// How the receiver of an interface-member resolution pins `Self`.
///
/// Decides whether the object-safety restriction applies and what `Self`-typed
/// parameters resolve to. See [`TypeInferenceBuilder::resolve_interface_member`].
#[derive(Clone, Copy)]
enum SelfReceiver<'a> {
    /// Bare interface ("dyn"/existential) receiver, carrying its interface existential type —
    /// used as `Self` for a bound call (a complete `Ty::Interface`, unlike a reconstructed
    /// constraint). `Self`-parameter methods are not callable on it (object safety).
    Existential(&'a Ty),
    /// `Self` is a single rigid type variable — a generic bound `T extends I`, or
    /// `self` inside a default method. Pinned; never inferred from an argument,
    /// checked by identity.
    RigidVar(&'a crate::ty::ParamTy),
    /// `Self` is pinned to the receiver's exact type. This includes concrete
    /// classes/primitives and abstract-but-rigid associated projections such as
    /// `H.Item`.
    ExactTy(&'a Ty),
    /// `Self` is a union receiver reached through one shared interface of an
    /// intersection-existential union (`union.foo()` ≡ `union.as<I>.foo()`). `Self` binds to
    /// the union — the subtype of the interface existential — so `Self`-returning methods
    /// yield the union, not the erased interface; but like [`SelfReceiver::Existential`] the
    /// runtime arm is unknown, so `Self`-typed *parameters* are not callable (object safety).
    Union(&'a Ty),
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

#[derive(Clone, Copy)]
struct CallContext<'a> {
    expr_id: ExprId,
    args: &'a [ExprId],
    call_args: Option<&'a [ast::CallArg]>,
    body: &'a ExprBody,
    expected: &'a Ty,
}

/// The call site's explicit `<T1, T2, ...>` type args, resolved ahead of
/// [`TypeInferenceBuilder::check_call_inner`].
enum ExplicitTypeArgs {
    /// No explicit type args were written — use the forward/reverse inference paths.
    NotProvided,
    /// Written but malformed (wrong arity) — already diagnosed (`WrongTypeArgArity`), so
    /// the unresolved-parameter check must not cascade a `CannotInferTypeParameter` on
    /// top of it for the params the malformed list failed to fill.
    Errored,
    /// Validated and resolved: arity checked, each `TypeExpr` lowered. Inference phases
    /// are skipped in favor of these bindings.
    Resolved(FxHashMap<crate::ty::ParamTy, Ty>),
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
    /// The call site's explicit `<T1, T2, ...>` type args, if any (see
    /// [`ExplicitTypeArgs`]).
    explicit_type_args: ExplicitTypeArgs,
    /// The callee expression, when one exists. Used to resolve the callee's
    /// declared generic params so the call's final type-arg bindings can be
    /// recorded (in declared order) in `call_type_instantiations` for MIR.
    callee_expr: Option<ExprId>,
    /// Generic params in the order the callee's runtime frame expects them.
    /// For static methods on generic classes this includes owner class params
    /// before method params; for bound methods the receiver seeds owner params,
    /// so this is just the method params.
    runtime_generic_layout: crate::ty::RuntimeGenericLayout,
    /// Pre-bound runtime type args that were substituted out of the callable
    /// type before ordinary call inference ran.
    runtime_type_arg_binding_seed: Vec<(crate::ty::ParamTy, Ty)>,
    /// The rigid `Self` type variable for a Self-pinned interface method call —
    /// argument inference never binds it and the argument is checked against it
    /// by identity (rustc's `ty::Param`). `None` for every ordinary call, which
    /// leaves their inference completely unchanged.
    rigid_self_var: Option<crate::ty::ParamTy>,
}

#[derive(Clone, Copy)]
struct OptionalCallContext<'a> {
    call: CallContext<'a>,
    callee_id: ExprId,
    is_method_call: bool,
}

impl<'db> TypeInferenceBuilder<'db> {
    /// The builder-free view of this scope, borrowing the builder's global
    /// inputs — including its type-variable bounds, held as interface-constraint
    /// conjunctions (`T: A & B`), the same representation type-expression lowering
    /// uses.
    fn as_global(&self) -> GlobalTypeContext<'_, 'db> {
        GlobalTypeContext {
            db: self.context.db(),
            res_ctx: self.res_ctx,
            aliases: self.aliases,
            bounds: &self.generic_param_bounds,
        }
    }
}

/// The builder *is* a [`baml_type::normalize::TypeContext`]: the
/// nominal facts the type algebra needs are entirely global (see
/// [`GlobalTypeContext`]), so each method delegates to a [`GlobalTypeContext`]
/// over the builder's scope via `Self::as_global`. Implementing the trait
/// directly lets value-checking call the defaulted algebra methods on the builder
/// — `self.is_subtype(a, b)` — with no wrapper.
impl baml_type::normalize::TypeContext for TypeInferenceBuilder<'_> {
    fn alias_def(&self, name: &crate::ty::QualifiedTypeName) -> Option<Ty> {
        self.as_global().alias_def(name)
    }

    fn implements_interface(&self, concrete: &Ty, interface: &baml_type::Interface) -> bool {
        self.as_global().implements_interface(concrete, interface)
    }

    fn type_var_bound(&self, param: &crate::ty::ParamTy) -> Vec<baml_type::Interface> {
        self.as_global().type_var_bound(param)
    }

    fn interface_requires(&self, sub: &baml_type::Interface, sup: &baml_type::Interface) -> bool {
        self.as_global().interface_requires(sub, sup)
    }

    fn enum_variants(&self, name: &crate::ty::QualifiedTypeName) -> Option<Vec<Name>> {
        self.as_global().enum_variants(name)
    }

    fn associated_type_bound(
        &self,
        interface: &baml_type::Interface,
        assoc: Name,
    ) -> Vec<baml_type::Interface> {
        self.as_global().associated_type_bound(interface, assoc)
    }

    fn project(
        &self,
        base: &Ty,
        interface: &baml_type::Interface,
        member: &Name,
        fuel: u32,
    ) -> baml_type::normalize::ProjectionStep {
        self.as_global().project(base, interface, member, fuel)
    }
}

/// A declared interface method's generics for call-site checking, keyed by the
/// callee expression: the method name (for diagnostics), its generic-param names
/// in declaration order, and their positional interface bounds — one entry per
/// param holding the full `extends A & B` conjunction (empty for an unbounded
/// param, including the receiver/`Self` generic when it is unbounded).
type InterfaceMethodGenerics = (Name, Vec<crate::ty::ParamTy>, Vec<Vec<Ty>>);

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
    /// Arena namespace for the expressions currently being inferred.
    expr_metadata_scope: ExprMetadataScope,
    /// Declared return type for the function (used to check return statements).
    declared_return_ty: Option<Ty>,
    /// Resolved type alias map: alias qualified name → expanded Ty.
    /// Used by the normalizer for structural subtype checking. Held by
    /// reference to the Salsa-cached `package_resolved_aliases` value so it is
    /// not cloned per scope (it used to be rebuilt for every scope inference).
    aliases: &'db HashMap<crate::ty::QualifiedTypeName, Ty>,
    /// Alias map in `unify::nf` canonical union form, for the pattern
    /// reachability oracle ([`crate::pattern_overlap`]) — the raw `aliases`
    /// bodies would mis-decide alias-obscured unions at invariant positions.
    /// Built lazily on the first oracle consultation: most scopes have no
    /// rigid-carrying `Never` narrowings, so they never pay for it.
    normalized_overlap_aliases: std::cell::OnceCell<HashMap<crate::ty::QualifiedTypeName, Ty>>,
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
    pub generic_params: Vec<crate::ty::ParamTy>,
    /// Type aliases/bindings visible only while checking this body. Interface
    /// default methods use this for associated type names like `Item` and
    /// `Error`, which must lower to `Self.Item` / `Self.Error` in expression
    /// type positions as well as in signatures.
    pub type_bindings: FxHashMap<crate::ty::ParamTy, Ty>,
    /// What a *body-position* `Self` (annotations, patterns, explicit type
    /// arguments) lowers to in this body: the rigid `Self` type variable for
    /// an interface's own method, or — statically substituted — the enclosing
    /// implements-block's `for` target (the class at its own params for an
    /// in-body block). `None` for plain class methods and free functions,
    /// where `Self` stays an unresolved name (a plain class body cannot yet
    /// name its own instantiation — pinned in the `self_in_body` diagnostics
    /// project). Signature positions resolve `Self` separately, for every
    /// body kind (`lower_with_self` in `inference.rs`).
    body_self_ty: Option<Ty>,
    /// BEP-044 generic bounds: `T → bound_ty`. Populated alongside
    /// `generic_params` when a function is declared with `<T extends I>`.
    /// Used by `resolve_member` to expose `I`'s contract on values of
    /// type `T`, and by call-site enforcement when a `T` is replaced by
    /// a concrete type that must satisfy its bound. Each entry is the bound's
    /// interface-constraint conjunction (`T: A & B`); an empty/absent entry is
    /// unbounded (or bounded only by a non-interface type, already diagnosed).
    pub generic_param_bounds: crate::lower_type_expr::TypeVarBoundsMap,
    /// Source map for the body being analyzed. Set by `infer_scope_types`
    /// before checking. Used to resolve `PatId` → `TextRange` when emitting
    /// pattern-position diagnostics.
    body_source_map: Option<AstSourceMap>,
    /// Depth counter for `OptionalChain` scopes. When > 0, `FieldAccess` and
    /// `Index` auto-unwrap nullable bases (null is caught by the chain wrapper).
    /// When 0, accessing a member on a nullable type is a type error.
    in_optional_chain: usize,
    /// Number of enclosing loops in the CURRENT `ExprBody` (reset across lambda
    /// boundaries). Combined with `defer_loop_floors` to allow `break`/
    /// `continue` that target a loop declared inside a `defer` body while
    /// rejecting control flow that would escape the defer (BEP-042).
    loop_depth: usize,
    /// Stack of `loop_depth` values captured when entering each active `defer`
    /// body (BEP-042). Non-empty ⇒ currently checking inside a defer body. A
    /// `break`/`continue` escapes the innermost defer iff `loop_depth` equals
    /// the top floor; a `return` escapes whenever the stack is non-empty.
    /// Reset across lambda boundaries.
    defer_loop_floors: Vec<usize>,
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
    interface_method_generic_params: FxHashMap<ExprId, InterfaceMethodGenerics>,
    /// Concrete owner (class/interface) generic bindings for a member call,
    /// keyed by the callee expression. The callable type substitutes these out
    /// of its parameter/return types. They seed the call's type-arg bindings for
    /// two reasons: an interface default method's VM frame still expects owner
    /// params before method params, and a bound method's generic *bounds* may
    /// reference an owner param (`<U extends Eq<C>>` on `class Box<C>`) that is
    /// otherwise absent from the call-site bindings.
    owner_type_arg_binding_seed: FxHashMap<ExprId, Vec<(crate::ty::ParamTy, Ty)>>,
    /// For a Self-pinned interface method call (resolved through a type-variable
    /// receiver — `self` in a default method, or a generic `T extends I`), the
    /// rigid `Self` type variable, keyed by the callee (member-access) expr.
    /// The call site treats it like rustc's `ty::Param`: argument inference
    /// never binds it, and the argument is checked against it by identity. Empty
    /// for every non-Self-pinned call, so ordinary inference is unaffected.
    self_pinned_rigid_var: FxHashMap<ExprId, crate::ty::ParamTy>,
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
    /// Tagged-template body Lambda scope (`is_template_body`) → its tag's
    /// body-lambda params (BEP-049 §10). These params are injected into
    /// `self.locals` while typing the desugared `tag_body`, but they have no
    /// HIR binding (the tag resolves only at TIR time), so a real lambda nested
    /// inside the interpolations cannot see them when its scope is type-checked
    /// standalone. Recording them here (like `nested_lambda_types`, bubbling up
    /// to the owning Function/Let scope) lets that standalone inference seed
    /// them — see `infer_scope_types`'s `ScopeKind::Lambda` arm.
    pub template_body_params: FxHashMap<FileScopeId, Vec<FunctionParamTy>>,
    /// Accumulates each nested lambda's full inline-inference tables, keyed by
    /// the lambda's `FileScopeId`. Captured at the end of `infer_lambda_body`
    /// (moved, not cloned) before parent state is restored. Like
    /// `nested_lambda_types`, NOT saved/restored by `infer_lambda_body`, so
    /// entries for arbitrarily nested lambdas bubble up to the owning
    /// Function/Let scope, where the standalone `ScopeKind::Lambda` query
    /// projects them out instead of re-inferring the body (and re-emitting its
    /// diagnostics).
    pub nested_lambda_inference:
        FxHashMap<FileScopeId, crate::inference::NestedLambdaInference<'db>>,
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
        aliases: &'db HashMap<crate::ty::QualifiedTypeName, Ty>,
    ) -> Self {
        let db = context.db();
        let package_items = &res_ctx.own_items;
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, scope.file(db));
        let ns_context = pkg_info.namespace_path;
        let expr_metadata_scope = ExprMetadataScope::Body(scope.file_scope_id(db));
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
            expr_metadata_scope,
            declared_return_ty: None,
            aliases,
            normalized_overlap_aliases: std::cell::OnceCell::new(),
            ns_context,
            implements_block_interface: None,
            generic_param_bounds: rustc_hash::FxHashMap::default(),
            catch_residual_throws: FxHashMap::default(),
            exhaustive_matches: FxHashSet::default(),
            generic_params: Vec::new(),
            type_bindings: FxHashMap::default(),
            body_self_ty: None,
            in_optional_chain: 0,
            loop_depth: 0,
            defer_loop_floors: Vec::new(),
            path_root_types: FxHashMap::default(),
            path_segment_types: FxHashMap::default(),
            path_member_resolutions: FxHashMap::default(),
            interface_method_generic_params: FxHashMap::default(),
            owner_type_arg_binding_seed: FxHashMap::default(),
            self_pinned_rigid_var: FxHashMap::default(),
            param_types: Vec::new(),
            call_plans: FxHashMap::default(),
            call_type_instantiations: FxHashMap::default(),
            function_coercions: FxHashMap::default(),
            default_parameter_inference: crate::inference::DefaultParameterInference::empty(),
            nested_lambda_types: FxHashMap::default(),
            template_body_params: FxHashMap::default(),
            nested_lambda_inference: FxHashMap::default(),
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

    pub fn set_generic_params(&mut self, params: Vec<crate::ty::ParamTy>) {
        self.generic_params = params;
    }

    pub fn set_type_bindings(&mut self, bindings: FxHashMap<crate::ty::ParamTy, Ty>) {
        self.type_bindings = bindings;
    }

    /// See `Self::body_self_ty`'s field docs. Set by `infer_scope_types`
    /// during body setup for interface-owned and implements-block methods.
    pub fn set_body_self_ty(&mut self, self_ty: Option<Ty>) {
        self.body_self_ty = self_ty;
    }

    /// The scope's installed generic-parameter bounds as interface constraints,
    /// for type-expression lowering (`T.member` projection resolution). The
    /// enforcement table ([`Self::set_generic_param_bounds`]) already holds this
    /// representation.
    fn scope_type_var_bounds(&self) -> TypeVarBoundsMap {
        self.generic_param_bounds.clone()
    }

    fn lower_type_expr_in_current_body(
        &self,
        ty_expr: &TypeExpr,
        diags: &mut Vec<TirTypeError>,
    ) -> Ty {
        self.lower_type_expr_in_current_body_at(
            ty_expr,
            diags,
            crate::lower_type_expr::TypePosition::Existential,
        )
    }

    fn lower_type_expr_in_current_body_at(
        &self,
        ty_expr: &TypeExpr,
        diags: &mut Vec<TirTypeError>,
        position: crate::lower_type_expr::TypePosition,
    ) -> Ty {
        let bounds = self.scope_type_var_bounds();
        if self.type_bindings.is_empty() {
            crate::lower_type_expr::lower_type_expr_at(
                ty_expr,
                &crate::lower_type_expr::ScopeCtx {
                    db: self.context.db(),
                    package_items: self.package_items,
                    ns_context: &self.ns_context,
                    generic_params: &self.generic_params,
                    bounds: &bounds,
                    self_ty: self.body_self_ty.clone(),
                },
                diags,
                position,
            )
        } else {
            let generic_params: Vec<_> = self.type_bindings.keys().cloned().collect();
            crate::generics::substitute_ty(
                &crate::lower_type_expr::lower_type_expr_at(
                    ty_expr,
                    &crate::lower_type_expr::ScopeCtx {
                        db: self.context.db(),
                        package_items: self.package_items,
                        ns_context: &self.ns_context,
                        generic_params: &generic_params,
                        bounds: &bounds,
                        self_ty: self.body_self_ty.clone(),
                    },
                    diags,
                    position,
                ),
                &self.type_bindings,
            )
        }
    }

    /// BEP-044: register the bound for each generic parameter visible inside this
    /// body. Keys are the parameter names, values are the `extends` clause's
    /// interface-constraint conjunction. Type-vars without an entry are unbounded.
    pub fn set_generic_param_bounds(&mut self, bounds: crate::lower_type_expr::TypeVarBoundsMap) {
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
        FxHashMap<FileScopeId, Vec<FunctionParamTy>>,
        crate::inference::DefaultParameterInference<'db>,
        FxHashMap<FileScopeId, crate::inference::NestedLambdaInference<'db>>,
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
            self.template_body_params,
            self.default_parameter_inference,
            self.nested_lambda_inference,
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

    /// True when type-checking inside at least one `defer` body (BEP-042).
    fn in_defer(&self) -> bool {
        !self.defer_loop_floors.is_empty()
    }

    /// True when a `break`/`continue` here would escape the innermost active
    /// `defer` body — no loop has been entered since that defer began, so the
    /// only loop it could target lies outside the defer.
    fn break_escapes_defer(&self) -> bool {
        self.defer_loop_floors.last() == Some(&self.loop_depth)
    }

    /// Report a "control flow cannot leave a `defer` body" error at `stmt_id`.
    fn report_defer_escape(&self, keyword: &'static str, stmt_id: StmtId) {
        if let Some(span) = self
            .body_source_map
            .as_ref()
            .map(|sm| sm.stmt_span(stmt_id))
        {
            self.report_at_span(
                crate::infer_context::TirTypeError::DeferControlFlowEscape { keyword },
                span,
            );
        }
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

    fn local_is_uncaptured(&self, subject: ExprId, name: &Name) -> bool {
        if !self.locals.contains_key(name) {
            return false;
        }
        let db = self.context.db();
        let file = self.context.scope().file(db);
        let index = baml_compiler2_ppir::file_semantic_index(db, file);
        let key = ExprMetadataKey::new(self.expr_metadata_scope, subject);
        let Some(PathResolution::Local(binding_id)) = index.path_resolution(key) else {
            return false;
        };
        index
            .scope_bindings
            .get(binding_id.scope.index() as usize)
            .is_some_and(|bindings| !bindings.captured_bindings.contains(&binding_id))
    }

    fn narrow_uncaptured_local(&mut self, subject: ExprId, name: &Name, ty: Ty) {
        if self.local_is_uncaptured(subject, name) {
            self.narrow_local(name.clone(), ty);
        }
    }

    fn uncaptured_condition_narrowings(
        &self,
        condition: ExprId,
        body: &ExprBody,
    ) -> Vec<crate::narrowing::Narrowing> {
        crate::narrowing::extract_narrowings(
            condition,
            body,
            &self.expressions,
            &self.pattern_types,
        )
        .into_iter()
        .filter(|narrowing| self.local_is_uncaptured(narrowing.subject, &narrowing.name))
        .collect()
    }

    /// Seed a captured-name marker as `Ty::Unknown` to suppress false
    /// "unresolved name" diagnostics inside a lambda body. This is NOT a
    /// binding; the actual capture's type is resolved by the parent scope.
    ///
    /// Exists so all `self.locals` writes are named.
    fn seed_capture(&mut self, name: Name, ty: Ty) {
        self.locals.insert(
            name,
            LocalBinding {
                current_ty: ty,
                declared_ty: None,
                pattern: None,
            },
        );
    }

    /// Resolve the real type of a captured binding when it is not visible in
    /// `locals` (a grandparent capture seen while inferring a lambda body
    /// without its outer scopes in scope).
    ///
    /// The capture's `BindingId` names the scope that declares it, but that
    /// scope may not be independently inferred (catch clause/arm, block). Its
    /// type is recorded by the nearest enclosing inferred scope (function /
    /// lambda / let), so resolve there — mirroring the lambda capture-seeding in
    /// `infer_scope_types`.
    ///
    /// A capture only reaches here once name resolution has bound it (an
    /// unresolved capture is a diagnostic, never an entry in `captures`). In the
    /// common case the owner scope's inference is complete and yields the real
    /// type — the de-erasure win this resolution exists for. During a Salsa
    /// cycle through the owner scope, or while its inference is still partial,
    /// `infer_scope_types` has no entry yet; that transient recovery state falls
    /// back to `Unknown` rather than panicking. This matches the pre-resolution
    /// behavior (captures were seeded `Unknown` universally) and is safe: the
    /// owner's *converged* inference — which the runtime-lowering boundary
    /// actually consumes — sees the real type.
    fn resolve_capture_type(
        &self,
        binding_id: baml_compiler2_hir::semantic_index::BindingId,
    ) -> Ty {
        use baml_compiler2_hir::semantic_index::BindingKind;
        let db = self.context.db();
        let file = self.context.scope().file(db);
        let index = baml_compiler2_ppir::file_semantic_index(db, file);
        let scope_idx = binding_id.scope.index() as usize;

        let inference_fsi = crate::inference::inference_owner_scope(index, binding_id.scope);
        let inference_scope_id = index.scope_ids[inference_fsi.index() as usize];
        let inference = crate::inference::infer_scope_types(db, inference_scope_id);
        let resolved = match binding_id.kind {
            BindingKind::Parameter(idx) => inference.param_type(idx).cloned(),
            BindingKind::Local(idx) => index
                .scope_bindings
                .get(scope_idx)
                .and_then(|bindings| bindings.bindings.get(idx as usize))
                .and_then(|binding| inference.binding_type(binding.pattern).cloned()),
        };
        let resolved = resolved.unwrap_or(Ty::Unknown {
            attr: TyAttr::default(),
        });
        // A captured binding is no longer open, so freeze evolving empties just
        // like an ordinary local reference does.
        match resolved {
            Ty::EvolvingList(inner, attr) => Ty::List(inner, attr),
            Ty::EvolvingMap(key, value, attr) => Ty::Map { key, value, attr },
            other => other,
        }
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
        crate::inference::expand_alias_chains(ty, self.aliases)
    }

    /// Pattern-matrix-internal normalization of a scrutinee/column type: the
    /// canonical form from the one type algebra (`baml_type::normalize`).
    /// Canonicalization recursively flattens alias-nested unions, sorts and
    /// deduplicates members, absorbs subsumed ones (`1 | int` → `int`, an
    /// implementor into its interface existential), drops `never`, and
    /// unwraps singletons — so the matrix's `UnionMember` dispatch sees
    /// exactly the canonical member *set* the value belongs to, the same set
    /// the pattern side targets: columns, rows, and witnesses agree by
    /// construction (`type DoubleMaybe = MaybeResult?` becomes
    /// `Success | Failure | null`; `type A = B | int` with
    /// `type B = int | string` contributes one `int` member).
    ///
    /// The normalized form flows into binding and narrowing types (members
    /// are the types arms are analyzed against — `analyze_and_lower_inner`),
    /// so canonical spellings become program-visible in diagnostics and
    /// reflection. RULED intended (2026-07-24): types render canonically
    /// wherever users see them, and SAP attributes — which canonicalization
    /// does not preserve — carry meaning only directly in an LLM function's
    /// return type position, never through the type algebra. This requires
    /// order-insensitive union inference at call sites (a binding's
    /// canonically sorted spelling must solve generics exactly like the
    /// declared order — `heads_correspond` in `baml_type_runtime`).
    fn matrix_normalize_scrut(&self, ty: &Ty) -> Ty {
        self.normalize(ty)
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
        generic_params: &[crate::ty::ParamTy],
        span: TextRange,
    ) -> Ty {
        let mut diags = Vec::new();
        let ty = crate::lower_type_expr::lower_type_expr(
            type_expr,
            &crate::lower_type_expr::ScopeCtx {
                db: self.context.db(),
                package_items: self.package_items,
                ns_context: &self.ns_context,
                generic_params,
                bounds: &self.scope_type_var_bounds(),
                self_ty: None,
            },
            &mut diags,
        );
        for diag in diags {
            self.context.report_at_span(diag, span);
        }
        self.validate_type_generic_bounds_at_span(span, &ty);
        ty
    }

    fn lower_lambda_return_annotation(&mut self, lambda: &ast::LambdaDef) -> Option<Ty> {
        let te = lambda.return_type.as_ref()?;
        // A lambda declares no generics of its own, so the enclosing scope's
        // parameters are the whole environment.
        Some(self.lower_lambda_type_expr(te, &self.generic_params.clone(), te.span))
    }

    fn choose_lambda_throws_surface(
        &mut self,
        func_def: &baml_compiler2_ast::LambdaDef,
        generic_params: &[crate::ty::ParamTy],
        contextual_throws: Option<&Ty>,
    ) -> (Ty, TextRange, bool) {
        if let Some(throws) = &func_def.throws {
            let ty = self.lower_lambda_type_expr(throws, generic_params, throws.span);
            (ty, throws.span, true)
        } else if let Some(contextual) = contextual_throws {
            (contextual.clone(), func_def.span, false)
        } else if func_def.kind == baml_compiler2_ast::LambdaKind::Spawn {
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

    fn synthetic_effect_param_name(fact: &Ty) -> Option<&crate::ty::ParamTy> {
        match fact {
            Ty::TypeVar(param, _) if crate::ty::is_synthetic_effect_param(param.name()) => {
                Some(param)
            }
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

    fn callback_throws_for_generic_inference(&self, expr_id: ExprId) -> Option<Ty> {
        if let Some(throws) = self.lambda_effective_throws.get(&expr_id) {
            let facts = crate::throw_inference::flatten_ty_to_facts(throws);
            let all_rigid = facts.iter().all(|fact| {
                !matches!(
                    fact,
                    Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. }
                ) && !crate::generics::contains_non_rigid_typevar(fact, &self.generic_params)
            });
            if all_rigid {
                return Some(throws.clone());
            }
        }
        self.callback_concrete_throws_from_expr(expr_id)
    }

    fn replace_callable_throws(ty: Ty, concrete_throws: &Ty) -> Ty {
        match ty {
            Ty::Function {
                params, ret, attr, ..
            } => Ty::Function {
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
        self.callback_throws_for_generic_inference(expr_id)
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
        root: Option<ExprId>,
        throws_ty: &Ty,
        span: TextRange,
        warn_extraneous: bool,
    ) {
        // Normalize both sides before comparing so a concrete-base associated
        // projection reduces to its binding: `(int as Comparable).CompareError`
        // becomes `never` (int pins it) and drops out, instead of being flagged as
        // an extra throw the declaration doesn't cover. A *symbolic* projection
        // (`(T as Comparable).CompareError` in a generic body) does not reduce and
        // stays a fact — correctly, since the body genuinely throws it.
        let declared = crate::throw_inference::flatten_ty_to_facts(&self.normalize(throws_ty));
        let effective: BTreeSet<Ty> = self
            .collect_effective_throws(body, root)
            .iter()
            .flat_map(|fact| crate::throw_inference::flatten_ty_to_facts(&self.normalize(fact)))
            .collect();
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
        // `Map<K, V>` shapes. `List`/`Map` are invariant (TYPE_SYSTEM.md, Subtyping
        // Rules → Variance), so the element types must be *equivalent*, not merely
        // subtypes — an `int[]` argument does not satisfy an `(int | string)[]` slot.
        match (&expanded_expected, &expanded_got) {
            (Ty::Class(class_name, expected_args, _), Ty::List(actual_inner, _))
                if class_name.is_builtin_root_type("Array") && expected_args.len() == 1 =>
            {
                self.equivalent(actual_inner, &expected_args[0])
            }
            (Ty::Class(class_name, expected_args, _), Ty::EvolvingList(actual_inner, _))
                if class_name.is_builtin_root_type("Array") && expected_args.len() == 1 =>
            {
                self.equivalent(actual_inner, &expected_args[0])
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
                self.equivalent(actual_key, &expected_args[0])
                    && self.equivalent(actual_val, &expected_args[1])
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

    /// Whether `expr` is a bare reference to a generic *free function* — an
    /// unrealized function value that must be specialized (`identity` →
    /// `identity<int>`). Only `Free` resolutions with declared user generic params
    /// qualify: method references (whose receiver/`Self` is inferred at the call)
    /// are exempt, and a `GenericApply` (`foo<int>`) records no `Free` resolution
    /// on its own node, so it is already realized.
    fn references_unspecialized_generic_function(&self, expr: ExprId) -> bool {
        if let Some(crate::inference::MemberResolution::Free { func_loc }) =
            self.resolutions.get(&expr)
        {
            !baml_compiler2_ppir::item_data::elaborated_function_data(self.context.db(), *func_loc)
                .user_generic_params
                .is_empty()
        } else {
            false
        }
    }

    /// The callee's declared generic-param names in De Bruijn order, plus the
    /// callee's name for diagnostics. See [`Self::callee_declared_generics`].
    fn callee_declared_generic_params(
        &self,
        callee_id: ExprId,
    ) -> Option<(Vec<crate::ty::ParamTy>, Name)> {
        self.callee_declared_generics(callee_id)
            .map(|(params, _bounds, name)| (params, name))
    }

    /// The callee's declared generic params in De Bruijn order
    /// (`[class params...] ++ [user fn params...]`), each paired (positionally)
    /// with its lowered interface bound (`None` for unbounded params, and for the
    /// prepended class params, which are unbounded *at this call position*), plus
    /// the callee's name for diagnostics. Resolved from the callee expression's
    /// recorded `MemberResolution`; `None` when the callee is not a declared
    /// function (lambda values, unresolved callees). The bounds drive call-site
    /// generic-bound enforcement now that a function *type* no longer carries
    /// them (function values are realized).
    fn callee_declared_generics(
        &self,
        callee_id: ExprId,
    ) -> Option<(Vec<crate::ty::ParamTy>, Vec<Vec<Ty>>, Name)> {
        // Method calls on a local receiver (`rec.get<int>(...)`) parse as a
        // multi-segment Path callee, whose member resolution is recorded in
        // `path_member_resolutions` rather than `resolutions`. Only trust a
        // method-like final entry there: the vec is NOT parallel to the path
        // segments (builtin/primitive members and abstract interface methods
        // record no entry), so a Field/Variant tail from an earlier segment
        // must fall through to the interface-method fallback below instead of
        // short-circuiting the lookup.
        let resolved = self.resolutions.get(&callee_id).cloned().or_else(|| {
            self.path_member_resolutions
                .get(&callee_id)
                .and_then(|resolutions| resolutions.last().cloned())
                .filter(|resolution| {
                    matches!(
                        resolution,
                        crate::inference::MemberResolution::Free { .. }
                            | crate::inference::MemberResolution::UnboundMethod { .. }
                            | crate::inference::MemberResolution::BoundMethod { .. }
                            | crate::inference::MemberResolution::InterfaceVirtualMethod { .. }
                            | crate::inference::MemberResolution::InterfaceConcreteMethod { .. }
                    )
                })
        });
        let Some(resolution) = resolved else {
            // Interface methods aren't in `resolutions`; their declared generic
            // params and bounds are recorded separately during interface checking.
            let (callee_name, declared_params, declared_bounds) = self
                .interface_method_generic_params
                .get(&callee_id)
                .cloned()?;
            return Some((declared_params, declared_bounds, callee_name));
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
            // Interface methods carry no callee-frame `func_loc`; their declared generic
            // params and bounds come from the side table, like the no-resolution case above.
            crate::inference::MemberResolution::InterfaceVirtualMethod { .. }
            | crate::inference::MemberResolution::InterfaceConcreteMethod { .. } => {
                let (callee_name, declared_params, declared_bounds) = self
                    .interface_method_generic_params
                    .get(&callee_id)
                    .cloned()?;
                return Some((declared_params, declared_bounds, callee_name));
            }
            _ => return None,
        };
        let db = self.context.db();
        // The callee's declared params and lowered bounds are a pure function
        // of its declaration — previously re-derived here (item-tree scan +
        // bound re-lowering) at every call site; now Salsa-memoized per callee.
        let data = callee_generics_for_func(db, func_loc);
        // Only user-declared generic params are *supplied at the call site*;
        // synthetic effect params are always inferred. For static-method-on-
        // generic-class calls, the class params are also supplied (`Class<...>.m`)
        // and are prepended to the declared list; for bound/instance calls the
        // class args come from the receiver, so they are not declared call-site
        // params (their own bounds, where any, are enforced at receiver
        // specialization).
        let class_params: &[crate::ty::ParamTy] = if treat_as_static_method {
            &data.class_params
        } else {
            &[]
        };

        let mut declared_params: Vec<crate::ty::ParamTy> = class_params.to_vec();
        declared_params.extend(data.user_params.iter().cloned());
        let mut declared_bounds: Vec<Vec<Ty>> = vec![Vec::new(); class_params.len()];
        declared_bounds.extend(data.user_bounds.iter().cloned());
        // A user bound that references an enclosing class param (`<U extends
        // Eq<C>>` on a method of `class Box<C>`) is lowered with `C` as a type
        // variable here; on a bound-method call the receiver's value for `C` is
        // seeded into the call-site bindings (see `owner_type_arg_binding_seed`) so
        // the bound resolves to e.g. `Eq<int>` before it is checked.
        Some((declared_params, declared_bounds, data.name.clone()))
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

        // Function values are realized: the type no longer carries its own
        // generics, so the params to specialize — and their bounds — come from the
        // callee's *declaration* (resolved via the base expr). A non-declared
        // callee (a plain function value) has none.
        let (generic_params, generic_param_bounds): (Vec<crate::ty::ParamTy>, Vec<Vec<Ty>>) = self
            .callee_declared_generics(base)
            .map(|(params, bounds, _)| (params, bounds))
            .unwrap_or_default();

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
        let scope_bounds = self.scope_type_var_bounds();
        let self_ty = self.body_self_ty.clone();
        // `type_bindings` is not mutated by this loop — snapshot its keys once.
        let binding_params: Vec<crate::ty::ParamTy> = self.type_bindings.keys().cloned().collect();
        let mut resolved: Vec<Ty> = Vec::with_capacity(type_args.len());
        for type_arg_expr in type_args {
            let mut diags = Vec::new();
            let ty = if self.type_bindings.is_empty() {
                crate::lower_type_expr::lower_type_expr(
                    type_arg_expr,
                    &crate::lower_type_expr::ScopeCtx {
                        db,
                        package_items: self.package_items,
                        ns_context: &ns,
                        generic_params: &caller_generic_params,
                        bounds: &scope_bounds,
                        self_ty: self_ty.clone(),
                    },
                    &mut diags,
                )
            } else {
                crate::generics::substitute_ty(
                    &crate::lower_type_expr::lower_type_expr(
                        type_arg_expr,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: self.package_items,
                            ns_context: &ns,
                            generic_params: &binding_params,
                            bounds: &scope_bounds,
                            self_ty: self_ty.clone(),
                        },
                        &mut diags,
                    ),
                    &self.type_bindings,
                )
            };
            for d in diags {
                self.context.report_simple(d, expr_id);
            }
            resolved.push(ty);
        }

        let bindings = crate::generics::bind_type_vars(&generic_params, &resolved);

        // Generic-bound enforcement: each supplied type arg must satisfy *every*
        // bound its param declared (`T extends A & B` is a conjunction).
        // Substitute the bindings first so self-referential bounds
        // (`<T extends Container<T>>`) resolve before the subtype check.
        for (idx, resolved_arg) in resolved.iter().enumerate() {
            for bound in generic_param_bounds.get(idx).into_iter().flatten() {
                let Some(bound) = crate::generics::substitute_ty(bound, &bindings).as_interface()
                else {
                    continue;
                };
                if let Some(error) = self.bounded_type_arg_error(resolved_arg, &bound) {
                    self.context.report_simple(error, expr_id);
                }
            }
        }

        // Build the specialized signature: substitute the bound params into each
        // param/ret/throws. The result is a realized (non-generic) function value.
        Ty::Function {
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
    ) -> Option<FxHashMap<crate::ty::ParamTy, Ty>> {
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
        let scope_bounds = self.scope_type_var_bounds();
        let self_ty = self.body_self_ty.clone();
        let suppress_diags = self.is_auto_derived_body;
        for (param_name, type_arg_expr) in declared_params.iter().zip(type_args.iter()) {
            let mut diags = Vec::new();
            let ty = crate::lower_type_expr::lower_type_expr(
                type_arg_expr,
                &crate::lower_type_expr::ScopeCtx {
                    db,
                    package_items: self.package_items,
                    ns_context: &ns,
                    generic_params: &caller_generic_params,
                    bounds: &scope_bounds,
                    self_ty: self_ty.clone(),
                },
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
        let mut runtime_id = None;
        let mut reported_overflow_arity = false;
        let ordinary_arg_count = call_args.map_or(args.len(), |call_args| {
            call_args
                .iter()
                .filter(|arg| {
                    arg.label
                        .as_ref()
                        .is_none_or(|label| label.as_str() != "$id")
                })
                .count()
        });
        let has_named_args = call_args.is_some_and(|call_args| {
            call_args.iter().any(|arg| {
                arg.label
                    .as_ref()
                    .is_some_and(|label| label.as_str() != "$id")
            })
        });
        for (arg_index, arg_expr) in args.iter().copied().enumerate() {
            let label = call_args
                .and_then(|call_args| call_args.get(arg_index))
                .and_then(|arg| arg.label.as_ref());

            if label.is_some_and(|label| label.as_str() == "$id") {
                if runtime_id.is_some() {
                    self.context
                        .report_simple(TirTypeError::DuplicateRuntimeIdArgument, arg_expr);
                    self.infer_expr(arg_expr, body);
                    provided_args.insert(arg_expr);
                    continue;
                }

                let got = self.infer_expr(arg_expr, body);
                if !self.is_subtype(&got, &boundary_local_id_ty()) {
                    self.context.report_simple(
                        TirTypeError::RuntimeIdArgumentTypeMismatch { got },
                        arg_expr,
                    );
                }
                runtime_id = Some(arg_expr);
                provided_args.insert(arg_expr);
                continue;
            }

            if runtime_id.is_some() {
                self.context
                    .report_simple(TirTypeError::RuntimeIdArgumentMustBeLast, arg_expr);
                self.infer_expr(arg_expr, body);
                provided_args.insert(arg_expr);
                continue;
            }

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
                            got: ordinary_arg_count,
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
        let reported_positional_arity = !has_named_args && ordinary_arg_count < required_count;
        if reported_positional_arity {
            self.context.report_simple(
                TirTypeError::ArgumentCountMismatch {
                    expected: required_count,
                    got: ordinary_arg_count,
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
                            got: ordinary_arg_count,
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
                instantiated_throws: None,
                side_channels: crate::inference::CallSideChannels { runtime_id },
            },
        );

        pairs
    }

    fn runtime_call_type_args(
        &self,
        generic_params: &[crate::ty::ParamTy],
        bindings: &FxHashMap<crate::ty::ParamTy, Ty>,
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
                //
                // An uninferable parameter records `Ty::Error`, NOT `unknown`: erasing it to
                // the top type is silent unsoundness (observable via `reflect.type_of<T>()`),
                // so the recorded arg stays a loud error rather than a compatible-with-anything
                // sentinel (the diagnostic is emitted by the call-site inference).
                let ty = bindings
                    .get(param)
                    .cloned()
                    .map(Ty::widen_fresh)
                    .unwrap_or_else(|| Ty::Error {
                        attr: TyAttr::default(),
                    });
                let resolved = ty;
                if crate::generics::contains_typevar_where(&resolved, &|name| {
                    !self.generic_params.iter().any(|param| param == name)
                }) {
                    Ty::Error {
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

    fn callee_runtime_generic_layout(
        &self,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
        include_owner: bool,
    ) -> crate::ty::RuntimeGenericLayout {
        let env = crate::generic_env::function_generic_env(self.context.db(), func_loc);
        let params = if include_owner {
            env.params()
        } else {
            env.own_params()
        };
        crate::ty::RuntimeGenericLayout::new(params)
    }

    fn runtime_generic_layout_for_call(
        &self,
        callee_id: ExprId,
        callee_generic_params: &[crate::ty::ParamTy],
        _is_method_call: bool,
        is_value_call: bool,
    ) -> crate::ty::RuntimeGenericLayout {
        if is_value_call {
            crate::ty::RuntimeGenericLayout::new(callee_generic_params)
        } else if let Some(resolution) = self.callee_member_resolution(callee_id) {
            // Interface methods carry no callee-frame `func_loc`; their declared generic params
            // are recorded in the `interface_method_generic_params` side table during interface
            // checking. Everything else derives its frame from the function loc.
            match resolution {
                MemberResolution::Free { func_loc }
                | MemberResolution::UnboundMethod { func_loc, .. } => {
                    self.callee_runtime_generic_layout(func_loc, true)
                }
                MemberResolution::BoundMethod { func_loc, .. } => {
                    self.callee_runtime_generic_layout(func_loc, false)
                }
                MemberResolution::InterfaceVirtualMethod { .. }
                | MemberResolution::InterfaceConcreteMethod { .. } => self
                    .interface_method_generic_params
                    .get(&callee_id)
                    .map(|(_, params, _)| crate::ty::RuntimeGenericLayout::new(params))
                    .unwrap_or_else(|| crate::ty::RuntimeGenericLayout::new(callee_generic_params)),
                MemberResolution::Field { .. }
                | MemberResolution::Variant { .. }
                | MemberResolution::InterfaceVirtualField { .. } => {
                    crate::ty::RuntimeGenericLayout::new(callee_generic_params)
                }
            }
        } else {
            crate::ty::RuntimeGenericLayout::new(callee_generic_params)
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
                instantiated_throws: None,
                side_channels: crate::inference::CallSideChannels::default(),
            });
    }

    fn record_call_throws(&mut self, expr_id: ExprId, throws: Ty) {
        self.call_plans
            .entry(expr_id)
            .and_modify(|plan| plan.instantiated_throws = Some(throws.clone()))
            .or_insert_with(|| crate::inference::CallPlan {
                bindings: Vec::new(),
                type_args: Vec::new(),
                instantiated_throws: Some(throws),
                side_channels: crate::inference::CallSideChannels::default(),
            });
    }

    pub fn check_function_parameter_defaults(
        &mut self,
        params: &[baml_compiler2_ppir::item_data::FunctionParamData],
        // One span per parameter, parallel to `params`
        // (`FunctionSourceMap::param_spans`).
        param_spans: &[text_size::TextRange],
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
        let saved_owner_type_arg_binding_seed =
            std::mem::take(&mut self.owner_type_arg_binding_seed);
        let saved_self_pinned_rigid_var = std::mem::take(&mut self.self_pinned_rigid_var);
        let saved_call_plans = std::mem::take(&mut self.call_plans);
        let saved_call_type_instantiations = std::mem::take(&mut self.call_type_instantiations);
        let saved_function_coercions = std::mem::take(&mut self.function_coercions);
        let saved_lambda_effective_throws = std::mem::take(&mut self.lambda_effective_throws);
        let saved_expr_metadata_scope = self.expr_metadata_scope;
        let owner_scope = match saved_expr_metadata_scope {
            ExprMetadataScope::Body(scope) | ExprMetadataScope::ParameterDefault(scope) => scope,
        };
        self.expr_metadata_scope = ExprMetadataScope::ParameterDefault(owner_scope);
        let defaults = &parameter_defaults.defaults;
        let saved_body_source_map = self.body_source_map.replace(defaults.source_map.clone());
        let saved_locals = self.locals.clone();
        let saved_scoped_local_declarations_len = self.scoped_local_declarations.len();
        let saved_scoped_local_assignments_len = self.scoped_local_assignments.len();
        // Diagnostics emitted while checking defaults carry defaults-arena
        // `ExprId`s; freeze their spans against the defaults source map after the
        // loop so they don't resolve against the function body's map (wrong
        // offsets — see the `freeze_diagnostic_spans_from` call below).
        let defaults_diag_start = self.context.diagnostic_count();

        for (index, param) in params.iter().enumerate() {
            let Some(default_ref) = parameter_defaults.param_default(index) else {
                if seen_default {
                    self.report_at_span(
                        TirTypeError::RequiredParamAfterDefault {
                            name: param.name.clone(),
                        },
                        param_spans[index],
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
            // Type each default exactly once against the parameter's declared
            // type. A container *literal* whose kind matches a structural
            // container declared type goes through `check_expr`, which both
            // adopts empty `[]`/`{}` (even nested, e.g. `int[][] = [[]]`) and
            // reports any element mismatch — once, at the offending element.
            // Everything else infers and validates with
            // `argument_matches_expected` (looser than `check_expr`'s
            // `is_subtype`, so e.g. an `int[]` default against the
            // `baml.Array<int>` spelling of the same parameter is not falsely
            // rejected). Both paths' spans are fixed against the defaults source
            // map after the loop.
            let adopt_container_literal = matches!(
                (&defaults.exprs.exprs[default_expr], &expected_ty),
                (Expr::Array { .. }, Ty::List(..) | Ty::EvolvingList(..))
                    | (Expr::Map { .. }, Ty::Map { .. } | Ty::EvolvingMap(..))
            );
            if matches!(expected_ty, Ty::Unknown { .. } | Ty::Error { .. }) {
                // No declared type to check against or adopt — just record a type.
                self.infer_expr(default_expr, &defaults.exprs);
            } else if adopt_container_literal {
                self.check_expr(default_expr, &defaults.exprs, &expected_ty);
            } else {
                let got_ty = self.infer_expr(default_expr, &defaults.exprs);
                if !self.argument_matches_expected(&got_ty, &expected_ty) {
                    self.report_at_span(
                        TirTypeError::TypeMismatch {
                            expected: expected_ty,
                            got: got_ty,
                        },
                        default_span,
                    );
                }
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
        self.owner_type_arg_binding_seed = saved_owner_type_arg_binding_seed;
        self.self_pinned_rigid_var = saved_self_pinned_rigid_var;
        self.call_plans = saved_call_plans;
        self.call_type_instantiations = saved_call_type_instantiations;
        self.function_coercions = saved_function_coercions;
        self.lambda_effective_throws = saved_lambda_effective_throws;
        self.expr_metadata_scope = saved_expr_metadata_scope;
        // Resolve all default-checking diagnostics against the defaults source
        // map before restoring the function body's map (otherwise their
        // defaults-arena `ExprId`s render at the wrong offsets).
        self.context
            .freeze_diagnostic_spans_from(defaults_diag_start, &defaults.source_map);
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
            Expr::Return { value } => {
                if let Some(value) = value {
                    Self::collect_default_expr_forward_references(
                        *value,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
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
                if let Some(lambda_root) = func_def.body {
                    Self::collect_default_expr_forward_references(
                        lambda_root,
                        body,
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
            Expr::Template { tag, segments } => {
                if let ast::TemplateTag::Custom { tag, .. } = tag {
                    Self::collect_default_expr_forward_references(
                        *tag,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                Self::collect_default_expr_forward_references_in_template_segments(
                    segments,
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

    /// Recursive walk over a tagged-template segment tree. Same forward-reference
    /// collection as `collect_default_expr_forward_references` but threads
    /// through nested for-bodies and if-branches, pushing for-bindings onto the
    /// shadowed stack.
    fn collect_default_expr_forward_references_in_template_segments(
        segments: &[ast::TemplateSegment],
        body: &ExprBody,
        later_params: &FxHashSet<Name>,
        shadowed: &mut Vec<Name>,
        refs: &mut Vec<Name>,
    ) {
        for seg in segments {
            match seg {
                ast::TemplateSegment::Text(_) => {}
                ast::TemplateSegment::Interp(e) => {
                    Self::collect_default_expr_forward_references(
                        *e,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                ast::TemplateSegment::For {
                    binding,
                    collection,
                    body: inner,
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
                    Self::collect_default_expr_forward_references_in_template_segments(
                        inner,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    shadowed.truncate(saved_len);
                }
                ast::TemplateSegment::CStyleFor {
                    init,
                    cond,
                    body: inner,
                    ..
                } => {
                    // Pull the loop var's pattern + initializer out of the `init`
                    // `let` (releases the borrow before the collect calls below).
                    let (init_initializer, init_pattern) = match &body.stmts[*init] {
                        ast::Stmt::Let {
                            initializer,
                            pattern,
                            ..
                        } => (*initializer, Some(*pattern)),
                        _ => (None, None),
                    };
                    // The initializer runs before the loop var is bound, so it
                    // may reference outer names but never the loop var itself —
                    // process it before shadowing.
                    if let Some(e) = init_initializer {
                        Self::collect_default_expr_forward_references(
                            e,
                            body,
                            later_params,
                            shadowed,
                            refs,
                        );
                    }
                    // Shadow the loop var, then process `cond` and the body — both
                    // see the binding (e.g. `i` in `for (let i = 0; i < n; …)`), so
                    // a later param sharing the name must not flag them as
                    // forward references.
                    let saved_len = shadowed.len();
                    if let Some(p) = init_pattern {
                        Self::push_pattern_bindings(p, body, shadowed);
                    }
                    Self::collect_default_expr_forward_references(
                        *cond,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    Self::collect_default_expr_forward_references_in_template_segments(
                        inner,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    shadowed.truncate(saved_len);
                }
                ast::TemplateSegment::If {
                    branches,
                    else_body,
                } => {
                    for branch in branches {
                        Self::collect_default_expr_forward_references(
                            branch.condition,
                            body,
                            later_params,
                            shadowed,
                            refs,
                        );
                        Self::collect_default_expr_forward_references_in_template_segments(
                            &branch.body,
                            body,
                            later_params,
                            shadowed,
                            refs,
                        );
                    }
                    if let Some(eb) = else_body {
                        Self::collect_default_expr_forward_references_in_template_segments(
                            eb,
                            body,
                            later_params,
                            shadowed,
                            refs,
                        );
                    }
                }
            }
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
            Stmt::Defer { body: defer_body } => {
                Self::collect_default_expr_forward_references(
                    *defer_body,
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
            explicit_type_args,
            callee_expr,
            runtime_generic_layout,
            runtime_type_arg_binding_seed,
            rigid_self_var,
        } = request;
        let explicit_type_args_errored = matches!(explicit_type_args, ExplicitTypeArgs::Errored);
        let explicit_type_arg_bindings = match explicit_type_args {
            ExplicitTypeArgs::Resolved(bindings) => Some(bindings),
            ExplicitTypeArgs::NotProvided | ExplicitTypeArgs::Errored => None,
        };
        let explicit_args_used = explicit_type_arg_bindings.is_some();
        let callee_ty = self.expand_alias_chains(callee_ty);

        match &callee_ty {
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                // Function values are realized: the type no longer carries its own
                // generics, so the callee's inferable params *and their bounds* come
                // from its *declaration* (resolved via the callee expr). A plain
                // function value (lambda/local) has none.
                let (generic_params, generic_param_bounds): (
                    Vec<crate::ty::ParamTy>,
                    Vec<Vec<Ty>>,
                ) = callee_expr
                    .and_then(|id| self.callee_declared_generics(id))
                    .map(|(params, bounds, _)| (params, bounds))
                    .unwrap_or_default();

                let effective_params = if is_method_call {
                    crate::generics::skip_self_param(params)
                } else {
                    params.as_slice()
                };

                // When explicit type args were provided at the call site (e.g. `foo<int>(x)`),
                // skip Phase 0/1a/1b inference and use the pre-computed bindings directly.
                // This avoids ambiguity when the user has been explicit about type instantiation.
                let mut bindings: FxHashMap<crate::ty::ParamTy, Ty> =
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
                        let mut typevar_bindings: FxHashMap<crate::ty::ParamTy, Ty> =
                            FxHashMap::default();
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
                    for generic_param in &generic_params {
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
                            // Every conjunct can contribute bindings, so drive
                            // inference through each. Cloned up front: the loop
                            // mutates `bindings`, which `generic_param_bounds`
                            // does not borrow but `substitute_ty` reads.
                            let declared: Vec<Ty> =
                                generic_param_bounds.get(idx).cloned().unwrap_or_default();
                            for bound in &declared {
                                let bound = crate::generics::substitute_ty(bound, &bindings);
                                self.infer_call_bindings_rigid_self(
                                    &bound,
                                    &actual,
                                    &mut bindings,
                                    rigid_self_var.as_ref(),
                                );
                            }
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
                // `U` or `ParseCache.new`'s class params.
                if is_value_call {
                    bindings.retain(|name, _| {
                        crate::generics::is_value_call_inferable(name, &generic_params)
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
                    &generic_params,
                    &generic_param_bounds,
                    &bound_check_bindings,
                );
                // A value call (e.g. an interface-method value held in a local)
                // carries no declared generics to check above, but the callee's
                // free typevars may be synthetic receiver/`Self` generics whose
                // interface bound was recorded in the caller scope. Enforce those
                // against the inferred bindings. Skip the caller's own generics
                // (`self.generic_params`) — they are rigid here, not value generics.
                for (name, actual) in &bound_check_bindings {
                    if self.generic_params.iter().any(|p| p == name)
                        || generic_params.iter().any(|p| p == name)
                    {
                        continue;
                    }
                    for bound in self
                        .generic_param_bounds
                        .get(name)
                        .cloned()
                        .unwrap_or_default()
                    {
                        let bound =
                            crate::interfaces::substitute_interface(&bound, &bound_check_bindings);
                        if let Some(error) = self.bounded_type_arg_error(actual, &bound) {
                            self.context.report(error, expr_id, Vec::new());
                        }
                    }
                }
                if !explicit_args_used && !runtime_generic_layout.params().is_empty() {
                    let type_args = self.runtime_call_type_args(
                        runtime_generic_layout.params(),
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
                    } else if matches!(body.exprs[*arg], Expr::Array { .. } | Expr::Map { .. })
                        && matches!(
                            expected_arg_ty,
                            Ty::List(..)
                                | Ty::EvolvingList(..)
                                | Ty::Map { .. }
                                | Ty::EvolvingMap(..)
                        )
                    {
                        // An empty container *literal* (`[]`/`map {}`) adopts the
                        // now generic-substituted parameter type, so its
                        // element/key/value types come from the call instead of
                        // committing to `never`. Restricted to literals with a
                        // structural-container expected so `check_expr`'s array/map
                        // arm is guaranteed to adopt — never the `is_subtype`
                        // fallback, which (unlike `argument_matches_expected`)
                        // would reject e.g. an `int[]` arg against the explicit
                        // `baml.Array<int>` spelling of the same parameter.
                        self.check_expr(*arg, body, &expected_arg_ty);
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
                            let mut typevar_bindings: FxHashMap<crate::ty::ParamTy, Ty> =
                                FxHashMap::default();
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
                                    // An uninferable parameter records `Ty::Error` (loud), not an
                                    // erased `unknown`; the call-site inference reports it.
                                    bindings
                                        .get(name)
                                        .or_else(|| typevar_bindings.get(name))
                                        .cloned()
                                        .map(Ty::widen_fresh)
                                        .unwrap_or(Ty::Error {
                                            attr: TyAttr::default(),
                                        })
                                })
                                .collect();
                            self.call_type_instantiations.insert(expr_id, instantiation);
                        }
                    }
                }

                let instantiated_throws = crate::generics::substitute_ty(throws, &bindings);
                let instantiated_throws =
                    if crate::generics::contains_concrete_base_projection(&instantiated_throws) {
                        self.normalize(&instantiated_throws)
                    } else {
                        instantiated_throws
                    };
                self.record_call_throws(expr_id, instantiated_throws);

                let substituted_ret = crate::generics::substitute_ty(ret, &bindings);
                // *Every* declared type parameter must be resolved after inference — not only
                // those occurring in the return type. A parameter the caller cannot infer
                // (`f<T>() -> void`, or `f<T>() -> T?` with no expected type) is an error, never
                // a silently-erased `unknown` (the realization contract, TYPE_SYSTEM.md L152).
                let unresolved_callee_typevars: FxHashSet<crate::ty::ParamTy> = generic_params
                    .iter()
                    .filter(|name| {
                        !bindings.contains_key(*name)
                            && !self.generic_params.iter().any(|param| param == *name)
                    })
                    .cloned()
                    .collect();
                // Cascade suppression: an uninferable parameter is reported only when every
                // inference source it had was sound. An `Error` expected type is an upstream
                // failure, so nothing is reported; an `Unknown` expected (a synthesis position
                // with no annotation) is exactly where inference was meant to succeed, so a
                // failure there IS reported. Malformed explicit type args (wrong arity) were
                // already diagnosed at their own site, so the params they failed to fill must
                // not also report as uninferable. Likewise per parameter: when the variable
                // occurs in a param whose argument already failed to type (its recorded type
                // carries an error-recovery sentinel; unlike an expected type, an argument is
                // only ever `Unknown`/`Error` through its own already-diagnosed failure), the
                // cannot-infer is a figment of the argument's error, so only the argument's own
                // diagnostic is kept (B-836).
                if !matches!(expected, Ty::Error { .. }) && !explicit_type_args_errored {
                    for name in &unresolved_callee_typevars {
                        let tainted_by_errored_arg = param_arg_pairs.iter().any(|(param, arg)| {
                            crate::generics::contains_typevar_where(&param.ty, &|n| n == name)
                                && self
                                    .expressions
                                    .get(arg)
                                    .is_some_and(crate::generics::contains_error_recovery)
                        });
                        if tainted_by_errored_arg {
                            continue;
                        }
                        self.context.report_simple(
                            TirTypeError::CannotInferTypeParameter {
                                name: name.name().clone(),
                            },
                            expr_id,
                        );
                    }
                }
                // The result carries erased generics only when an unresolved var actually occurs
                // in the return type; a phantom parameter leaves the result untouched.
                let result_carries_erased = unresolved_callee_typevars.iter().any(|name| {
                    crate::generics::contains_typevar_where(&substituted_ret, &|c| c == name)
                });
                let substituted_ret =
                    crate::generics::erase_typevars_matching(&substituted_ret, &|name| {
                        unresolved_callee_typevars.contains(name)
                    });
                let mut erase_diags = Vec::new();
                let result =
                    crate::generics::erase_unresolved_typevars(&substituted_ret, &mut erase_diags);
                let recovered_unresolved_generics =
                    result_carries_erased || !erase_diags.is_empty();
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
                        explicit_type_args: match explicit_type_arg_bindings {
                            Some(bindings) => ExplicitTypeArgs::Resolved(bindings),
                            None if explicit_type_args_errored => ExplicitTypeArgs::Errored,
                            None => ExplicitTypeArgs::NotProvided,
                        },
                        callee_expr,
                        runtime_generic_layout,
                        runtime_type_arg_binding_seed,
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

        // Function values are realized — generics come from the callee's
        // declaration, not the (realized) function type.
        let callee_generic_params = self
            .callee_declared_generic_params(callee_id)
            .map(|(params, _)| params)
            .unwrap_or_default();
        let runtime_generic_layout = self.runtime_generic_layout_for_call(
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
            explicit_type_args: ExplicitTypeArgs::NotProvided,
            callee_expr: Some(callee_id),
            runtime_generic_layout,
            runtime_type_arg_binding_seed: self
                .owner_type_arg_binding_seed
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
            Expr::Literal(lit) => self.infer_literal(lit, expr_id),
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
                let narrowings = self.uncaptured_condition_narrowings(*condition, body);

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
                // An empty array literal evolves to fit its use rather than
                // committing to the unsound `never[]` (see `empty_evolving_list`).
                if elements.is_empty() {
                    Self::empty_evolving_list()
                } else {
                    let elem_types: Vec<Ty> =
                        elements.iter().map(|e| self.infer_expr(*e, body)).collect();
                    let elem_ty = Self::join_all(&elem_types).widen_fresh();
                    Ty::List(Box::new(elem_ty), TyAttr::default())
                }
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
                self.report_chaining_lints(*op, &lhs_ty, *lhs, *rhs, expr_id, body);
                self.infer_binary_op(*op, &lhs_ty, &rhs_ty, expr_id)
            }
            Expr::Unary { op, expr } => {
                // Note: `-<int literal>` is folded into a single negative literal
                // during AST lowering, so INT_MIN (`-4611686018427387904`) never
                // produces an out-of-range `+2^62` operand here. A bare `+2^62`
                // is rejected by `infer_literal`.
                let operand_ty = self.infer_expr(*expr, body);
                self.infer_unary_op(*op, &operand_ty, expr_id)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => self.infer_match_expr(expr_id, *scrutinee, arms, body, None),
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
            Expr::Return { value } => {
                // A `return` expression (e.g. a braceless `catch`/`match` arm
                // value) diverges, so it has type `never` — exactly like
                // `throw`. The returned value is still checked against the
                // enclosing function's declared return type, mirroring
                // `Stmt::Return` so the typing is identical to the block form.
                if self.in_defer()
                    && let Some(span) = self
                        .body_source_map
                        .as_ref()
                        .map(|sm| sm.expr_span(expr_id))
                {
                    self.report_at_span(
                        crate::infer_context::TirTypeError::DeferControlFlowEscape {
                            keyword: "return",
                        },
                        span,
                    );
                }
                if let Some(value) = value {
                    if let Some(ret_ty) = &self.declared_return_ty {
                        let ret_ty = ret_ty.clone();
                        self.check_expr(*value, body, &ret_ty);
                    } else {
                        self.infer_expr(*value, body);
                    }
                }
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
            } if Self::is_map_object_literal(type_name, obj_type_args, spreads) => {
                self.infer_map_object_expr(body, fields)
            }
            Expr::Object {
                type_name,
                type_args: obj_type_args,
                fields,
                spreads,
                ..
            } => self.infer_object_expr(expr_id, body, type_name, obj_type_args, fields, spreads),
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
            Expr::Lambda(func_def) => self.infer_lambda_expr(expr_id, body, func_def),
            Expr::Spawn {
                name,
                with_exprs,
                body: spawn_body,
            } => self.infer_spawn_expr(body, *name, with_exprs, *spawn_body),
            Expr::Await { future } => self.infer_await_expr(body, *future),
            Expr::Template {
                tag:
                    ast::TemplateTag::Custom {
                        tag,
                        body: tag_body,
                    },
                ..
            } => {
                // Tagged template (BEP §10). TIR validates the tag (a
                // `//baml:tagged_string` function whose first parameter is
                // `body: (...) -> baml.TaggedString`) and type-checks the
                // desugared `tag_body` flatten block with the tag's body-lambda
                // parameters — and any `${for}` bindings — in scope.
                //
                // The template's RESULT type is the tag fn's return type
                // (`Unknown` on any tag error). Typing `tag_body` is
                // side-effecting only — it surfaces interpolation diagnostics,
                // records each `${expr}` type, and gives MIR the `push` /
                // `baml.TaggedString` resolutions it needs to lower the closure.
                // We always type it (even on a tag error) so interps are still
                // checked; the body-lambda params are bound only when the tag
                // validated. Interps are NOT strictly coerced —
                // `TaggedString.values` is `unknown[]`, so values pass through
                // with their original types (§10/§11).
                let tag_ty = self.infer_expr(*tag, body);
                let tag_name = match &body.exprs[*tag] {
                    Expr::Path(segs) => segs.last().cloned(),
                    _ => None,
                }
                .unwrap_or_else(|| Name::new("<tag>"));

                let (result_ty, body_lambda_params): (Ty, Vec<FunctionParamTy>) = match &tag_ty {
                    // Unresolved: `infer_path` already reported `UnresolvedName`
                    // for a bare-path tag — stay quiet to avoid double-reporting.
                    Ty::Unknown { .. } => (
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        },
                        Vec::new(),
                    ),
                    Ty::Function { params, ret, .. } => {
                        let validated = self.validate_tagged_tag(*tag, &tag_name, params, ret);
                        // A non-`Unknown` return means validation succeeded, so
                        // the first param is `body: (lambda_params) -> ...`; those
                        // lambda params scope into the interpolations.
                        let lambda_params = if matches!(validated, Ty::Unknown { .. }) {
                            Vec::new()
                        } else {
                            match params.first().map(|p| &p.ty) {
                                Some(Ty::Function { params: lp, .. }) => lp.clone(),
                                _ => Vec::new(),
                            }
                        };
                        (validated, lambda_params)
                    }
                    _ => {
                        self.context.report_simple(
                            TirTypeError::TaggedTagNotAFunction { name: tag_name },
                            *tag,
                        );
                        (
                            Ty::Unknown {
                                attr: TyAttr::default(),
                            },
                            Vec::new(),
                        )
                    }
                };

                // Record the body-lambda params against the synthetic
                // template-body Lambda scope(s) (range == this tagged-template
                // expr's span) so a real lambda nested in the interpolations can
                // seed them when its own scope is type-checked standalone — there
                // the params have no HIR binding and would otherwise be
                // "unresolved name". Companion functions (an LLM fn and its
                // `$stream`) can share the span, so record into every match.
                if !body_lambda_params.is_empty()
                    && let Some(sm) = self.body_source_map.as_ref()
                {
                    let span = sm.expr_span(expr_id);
                    let db = self.context.db();
                    let file = self.context.scope().file(db);
                    let index = baml_compiler2_ppir::file_semantic_index(db, file);
                    for (i, scope) in index.scopes.iter().enumerate() {
                        if scope.is_template_body && scope.range == span {
                            #[allow(clippy::cast_possible_truncation)]
                            let fsi = FileScopeId::new(i as u32);
                            self.template_body_params
                                .insert(fsi, body_lambda_params.clone());
                        }
                    }
                }

                // Bind the body-lambda params, walk the segments, then restore.
                // Mirrors `infer_lambda_body`'s param binding (direct insert,
                // `pattern: None`) wrapped in `Stmt::For`-style scope save/restore.
                let snapshot = self.snapshot_scoped_locals();
                for param in &body_lambda_params {
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
                self.infer_expr(*tag_body, body);
                self.restore_scoped_locals(&snapshot);

                result_ty
            }
            Expr::Template {
                tag: ast::TemplateTag::Default { elaborated },
                segments,
            } => {
                // Untagged backtick (BEP §11). The value is realized by the
                // desugared `elaborated` concat: each `${expr}` is wrapped in
                // the total renderer `string.from(...)` and the parts are
                // folded with `+`. Inferring that tree types every `${expr}`
                // sub-expression in place (the segment `ExprId`s are shared
                // with the tree), so every diagnostic it produces is the
                // user's own, anchored on original spans, and all of them are
                // KEPT. Discarding any of them lets the sub-expression's
                // error-recovery type reach MIR, where runtime lowering ICEs
                // (B-836). The synthetic wrapping itself cannot fail:
                // `string.from` accepts every type, and its `T` never cascades
                // a cannot-infer error off an already-errored argument
                // (`check_call_inner` suppresses that like any other upstream
                // failure).
                //
                // The template's type IS the elaborated tree's type: always a
                // string, but literal-preserving for a constant template (e.g.
                // `` `abc` `` infers `Ty::Literal("abc")`), so constant folding
                // of comparisons on literal backticks (BEP §9) still fires.
                let elaborated_ty = self.infer_expr(*elaborated, body);
                // The one interpolation rule the elaborated tree cannot
                // express: each `${expr}` must be non-null (strict stringify,
                // BEP §7), reported on the original `${…}` spans.
                self.check_template_interps_stringable(segments, body);
                elaborated_ty
            }
            // A `Missing` node is an unparseable expression: MIR lowers it to a
            // `panic` and the runtime never produces a value, so it diverges.
            // `Never` (bottom) models that precisely — and, unlike the
            // error-recovery `Unknown`, has a faithful runtime representation,
            // so it doesn't trip the runtime lowering boundary.
            Expr::Missing => Ty::Never {
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
        let target_ty = crate::lower_type_expr::lower_type_expr(
            target,
            &crate::lower_type_expr::ScopeCtx {
                db: self.context.db(),
                package_items: self.package_items,
                ns_context: &self.ns_context,
                generic_params: &self.generic_params,
                bounds: &self.scope_type_var_bounds(),
                self_ty: None,
            },
            &mut diags,
        );
        for diag in diags {
            self.context.report_simple(diag, expr_id);
        }
        // The upcast target is an interface-existential value type, so it must pin every
        // non-defaulted associated type (`MissingAssociatedTypeBindings`) — a bare
        // `x.as<HasKey>` where `HasKey` has an unpinned `Key` is ill-formed exactly as a
        // `let y: HasKey` annotation would be. The pin makes the existential a single
        // concrete-enough type, so its `Self.Key` members reduce instead of staying symbolic.
        self.validate_type_generic_bounds(expr_id, &target_ty);
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
            return target_ty;
        }
        // A concrete (or otherwise-implementing) base carries its realized associated
        // bindings into the view: `doc.as<Codec<TextFormat>>` types as
        // `Codec<TextFormat, Output = string>`, not the bare existential — so a
        // subsequent `(… as I).member` or a method returning `Self.member` reduces
        // through the pin. Fall back to the written interface when no single realized
        // view exists (an `unknown`/error base, or an unpinnable existential).
        self.actual_interface_view_for_formal(&target_ty, &base_ty)
            .unwrap_or(target_ty)
    }

    #[inline(never)]
    fn infer_optional_member_access_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        base: ExprId,
        member: &Name,
    ) -> Ty {
        // Member access rooted at `$id` gets the same targeted diagnostic as
        // the plain-path form (`infer_path`'s multi-segment branch); without
        // this, the generic machinery suggests rewriting `$id?.m` to `$id.m`
        // — which is itself rejected.
        if Self::is_runtime_id_path(body, base) {
            self.context.report_simple(
                TirTypeError::RuntimeIdMemberAccess {
                    member: member.clone(),
                },
                expr_id,
            );
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }
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
        let index_ty = self.infer_expr(index, body);
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
        self.check_index_key_type(&resolve_ty, &index_ty, index, false);
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

    /// Validate that `index_ty` is an acceptable subscript for `container`: the
    /// runtime indexes a list/byte-array by `int` and a map by its key type
    /// (`string`). There is no key coercion — `list["a"]` or `map[0]` aborts the
    /// VM at runtime — so reject a mistyped subscript at compile time. A no-op
    /// for non-containers and for error-recovery index types.
    fn check_index_key_type(
        &mut self,
        container: &Ty,
        index_ty: &Ty,
        index_id: ExprId,
        allow_null: bool,
    ) {
        // The runtime has no null subscript (`a[null]` aborts the VM with the
        // confusing `got any`), so a plain `a[i]` requires a non-null index.
        // The optional form `a?.[i]` only guards the *base*, but its index is
        // typically nullable-by-type yet non-null in the narrowed chain context
        // (e.g. `node?.children?.[node?.value]`); strip null there to avoid a
        // false positive.
        let widened = index_ty.clone().widen_fresh();
        let index_ty = if allow_null {
            crate::narrowing::remove_null(&widened)
        } else {
            widened
        };
        if matches!(index_ty, Ty::Unknown { .. } | Ty::Error { .. }) {
            return;
        }
        let expected_key = match container {
            Ty::List(..) | Ty::EvolvingList(..) | Ty::Uint8Array { .. } => Ty::Int {
                attr: TyAttr::default(),
            },
            // An empty evolving map (`{}`, key `never`) adopts string keys; any
            // other map carries its declared (string) key type.
            Ty::Map { key, .. } | Ty::EvolvingMap(key, _, _) => {
                if matches!(**key, Ty::Never { .. }) {
                    Ty::string()
                } else {
                    (**key).clone()
                }
            }
            _ => return,
        };
        if !self.is_subtype(&index_ty, &expected_key) {
            self.context.report(
                TirTypeError::TypeMismatch {
                    expected: expected_key,
                    got: index_ty,
                },
                index_id,
                Vec::new(),
            );
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
        let index_ty = self.infer_expr(index, body);
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
            self.check_index_key_type(&base_info.inner, &index_ty, index, true);
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
        // Read the body's effective throws from the side table populated by
        // `infer_lambda_body`, which computes it for every lambda it infers and
        // collapses "throws nothing" to `Never` — so a body that statically
        // cannot throw types the future as `Future<T, never>`. `Expr::Spawn`'s
        // body is always the synthetic `<spawn>` lambda (`lower_spawn_expr`
        // lowers a block-less `spawn` to `Expr::Missing` outright rather than
        // building a `Spawn` around a non-lambda body), so the entry exists.
        let throws_ty = self
            .lambda_effective_throws
            .get(&spawn_body)
            .cloned()
            .unwrap_or_else(|| unreachable!("Unknown effective lambda throws"));

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
        // must read `SpawnParams<int, never>` in diagnostics and bindings,
        // not `SpawnParams<1, never>`).
        let mut cur_value = value_ty.widen_fresh();
        let mut cur_error = throws_ty.widen_fresh();
        for with_id in with_exprs {
            let params_in = spawn_params_ty(cur_value.clone(), cur_error.clone());
            // The expected RETURN is `SpawnParams<unknown, unknown>` (not a
            // bare `Unknown`): a non-transformer return type then fails the
            // check with a readable mismatch instead of coercing into the
            // open slot.
            let expected = Ty::Function {
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
            // A `never`-typed operand never yields a value to await: it
            // diverges (`await (throw e)`, `await (return x)`) or is a
            // malformed/error-recovery expression. `never` is the bottom type
            // and is assignable to any `Future<_, _>`, so `await never : never`
            // — the await is unreachable. Yield `never` rather than reporting a
            // spurious "expected Future" mismatch on already-unreachable code.
            Ty::Never { .. } => fut_ty,
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
    fn infer_lambda_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        func_def: &ast::LambdaDef,
    ) -> Ty {
        // Synthesis mode: no expected type available.
        // All param types MUST be annotated; unannotated params produce an error.

        // Lambdas cannot declare generic parameters (rejected by the parser), so
        // they only see the enclosing generic scope.
        let all_generic_params = self.generic_params.clone();

        let mut param_tys: Vec<FunctionParamTy> = Vec::new();

        for param in &func_def.params {
            let param_ty = match &param.type_expr {
                Some(te) => self.lower_lambda_type_expr(te, &all_generic_params, te.span),
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
            .map(|te| self.lower_lambda_type_expr(te, &all_generic_params, te.span));
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
            func_def.throws.is_none() && func_def.kind != baml_compiler2_ast::LambdaKind::Spawn;
        let throws_ty = if infer_throws_from_body {
            Ty::Unknown {
                attr: TyAttr::default(),
            }
        } else {
            throws_ty
        };

        // Infer the lambda body using save/restore approach
        let (ret_ty, lambda_fsi, lambda_effective_throws) = self.infer_lambda_body(
            func_def,
            body,
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
        let ty_expr = TypeExprKind::Path {
            segments: path.segments().to_vec(),
            generic_args: obj_type_args.to_vec(),
            associated_type_bindings: Vec::new(),
            attrs: Vec::new(),
        }
        .at(text_size::TextRange::default());
        // A construction head: a bare generic name is legal here — its
        // arguments are inferred from the construction's fields below.
        let ty = self.lower_type_expr_in_current_body_at(
            &ty_expr,
            &mut diags,
            crate::lower_type_expr::TypePosition::ConstructorHead,
        );
        for diag in diags {
            self.context.report_simple(diag, expr_id);
        }
        ty
    }

    /// `map {}` parses as an object literal named by the reserved `map` keyword
    /// (the keyword form lets an *empty* map be written where a bare `{}` would
    /// read as an empty block). Non-empty maps parse as `Expr::Map`, so an
    /// object literal named `map` is recognized here and routed to map — not
    /// class — inference rather than a construction of a non-existent class.
    fn is_map_object_literal(
        type_name: &baml_base::core_types::TypePath,
        obj_type_args: &[TypeExpr],
        spreads: &[ast::SpreadField],
    ) -> bool {
        obj_type_args.is_empty()
            && spreads.is_empty()
            && matches!(type_name.segments(), [name] if name.as_str() == "map")
    }

    /// The type of an empty array literal: an `EvolvingList` over `never`. An
    /// empty array has no elements to fix its element type, so — like the empty
    /// map `{}` — it evolves to fit its use (annotation or later mutation)
    /// rather than committing to the unsound `never[]`: generics are invariant,
    /// so `never[]` is a subtype of no other array type.
    fn empty_evolving_list() -> Ty {
        Ty::EvolvingList(
            Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        )
    }

    /// The type of an empty map literal: an `EvolvingMap` over `never`. An empty
    /// map has no entries to fix its key/value types, so — like the empty array
    /// `[]` — it evolves to fit its use (annotation or later mutation) rather
    /// than committing to the unsound `map<never, never>`: generics are
    /// invariant, so `map<never, never>` is a subtype of no other map type.
    fn empty_evolving_map() -> Ty {
        Ty::EvolvingMap(
            Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            Box::new(Ty::Never {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        )
    }

    fn infer_map_object_expr(&mut self, body: &ExprBody, fields: &[(Name, ExprId)]) -> Ty {
        // An empty map literal evolves to fit its use rather than committing to
        // the unsound `map<never, never>` (see `empty_evolving_map`).
        if fields.is_empty() {
            return Self::empty_evolving_map();
        }
        let val_types: Vec<Ty> = fields
            .iter()
            .map(|(_, value)| self.infer_expr(*value, body))
            .collect();
        let val_ty = Self::join_all(&val_types).widen_fresh();
        Ty::Map {
            key: Box::new(Ty::string()),
            value: Box::new(val_ty),
            attr: TyAttr::default(),
        }
    }

    fn check_map_object_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        expected: &Ty,
        fields: &[(Name, ExprId)],
    ) -> Ty {
        // Same adoption rule as the `Expr::Map` checking arm: aliases, nullable
        // wrappers, and unions with a unique map member all determine the
        // declared key/value types (containers are invariant, so each entry is
        // checked bidirectionally rather than synthesize-then-subtype).
        if let Some(container) =
            self.adopted_container_for_literal(expected, ContainerLiteralKind::Map)
        {
            let (Ty::Map {
                key: key_ty,
                value: val_ty,
                ..
            }
            | Ty::EvolvingMap(key_ty, val_ty, _)) = &container
            else {
                unreachable!("container is Map/EvolvingMap by construction")
            };
            let string_ty = Ty::string();
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
            self.record_expr_type(expr_id, container.clone());
            container
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

    fn report_unknown_class_property(
        &mut self,
        object_expr: ExprId,
        class_name: &crate::ty::QualifiedTypeName,
        field_name: &Name,
        field_expr: ExprId,
        declared_fields: &FxHashMap<Name, Ty>,
    ) {
        let (is_shorthand, is_synthetic_object) = self
            .body_source_map
            .as_ref()
            .map(|source_map| {
                (
                    source_map.is_property_shorthand_expr(field_expr),
                    source_map.is_synthetic_expr(object_expr),
                )
            })
            .unwrap_or_default();
        if !is_shorthand && is_synthetic_object {
            return;
        }

        let suggestions = Self::similar_name_suggestions(field_name, declared_fields.keys());
        let error = if is_shorthand {
            TirTypeError::UnknownClassPropertyShorthand {
                class_name: class_name.clone(),
                name: field_name.clone(),
                suggestions,
            }
        } else {
            TirTypeError::UnknownClassField {
                class_name: class_name.clone(),
                field_name: field_name.clone(),
                suggestions,
            }
        };

        if let Some(source_map) = &self.body_source_map {
            self.context.report_at_span(
                error,
                source_map.object_field_name_span(object_expr, field_expr),
            );
        } else {
            self.context.report_simple(error, field_expr);
        }
    }

    #[inline(never)]
    fn infer_object_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        type_name: &baml_base::core_types::TypePath,
        obj_type_args: &[TypeExpr],
        fields: &[(Name, ExprId)],
        spreads: &[ast::SpreadField],
    ) -> Ty {
        // Spread expressions are ordinary expressions: infer them even before
        // resolving the destination class so calls nested inside a spread get
        // their TIR call plans (including omitted-default bindings). Skipping
        // these used to let MIR fall back to the source argument list, so a
        // five-parameter factory called with two values emitted no OmittedArg
        // sentinels and consumed three slots from its caller's VM frame.
        let spread_types: Vec<(ExprId, Ty)> = spreads
            .iter()
            .map(|spread| (spread.expr, self.infer_expr(spread.expr, body)))
            .collect();

        // Class-instance literals only: map literals (`map { .. }`) are routed
        // to `infer_map_object_expr` by the `is_map_object_literal` guard in the
        // `Expr::Object` arm before reaching here. The parser only emits an
        // object literal when a type name precedes the brace, so the name is
        // always present, and `lower_object_type_name` surfaces any
        // type-resolution diagnostics (undefined class, wrong arg count, …) as
        // user-visible compiler errors rather than a silently-discarded
        // `Unknown` that later erases to `Void` or trips the runtime boundary.
        let ty = self.lower_object_type_name(expr_id, type_name, obj_type_args);
        let ty = match ty {
            Ty::Class(class_name, type_args, attr) => {
                if type_args.is_empty()
                    && obj_type_args.is_empty()
                    && let Some(class_loc) = self.resolve_class_loc(&class_name)
                {
                    let class_data =
                        baml_compiler2_ppir::item_data::class_data(self.context.db(), class_loc);
                    let class_generic_params =
                        crate::generic_env::class_generic_env(self.context.db(), class_loc)
                            .params()
                            .to_vec();
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
                            // Allow typevar actuals so a field that carries an
                            // enclosing generic (e.g. a `body: () -> T throws E`
                            // field whose value is `() -> int throws E`) binds the
                            // class param to that faithful typevar instead of
                            // leaving it unbound — which would erase to `Unknown`
                            // and trip the runtime boundary.
                            crate::generics::infer_bindings_allow_typevars(
                                declared_ty,
                                &field_ty,
                                &mut bindings,
                            );
                        }
                        // An exact-class spread can determine otherwise omitted
                        // class arguments (`Box { ...box_int }` => `Box<int>`).
                        for (_, spread_ty) in &spread_types {
                            if let Ty::Class(spread_name, spread_args, _) = spread_ty
                                && spread_name == &class_name
                                && spread_args.len() == class_data.generic_params.len()
                            {
                                for (param, arg) in class_generic_params.iter().zip(spread_args) {
                                    bindings.entry(param.clone()).or_insert_with(|| arg.clone());
                                }
                            }
                        }
                        // Params that appear in some field's declared type are
                        // inferable in principle, so leaving one unbound (e.g. `T`
                        // from `Box { items: [] }`) is a real `CannotInfer` error.
                        // A *phantom* param used by no field can never be
                        // determined by construction and is not an error — it is
                        // recovered silently as `BuiltinUnknown` below.
                        let field_constrained_params: FxHashSet<crate::ty::ParamTy> =
                            class_generic_params
                                .iter()
                                .filter(|param| {
                                    field_types.values().any(|field_ty| {
                                        crate::generics::contains_typevar_where(field_ty, &|name| {
                                            name == *param
                                        })
                                    })
                                })
                                .cloned()
                                .collect();
                        // Bind each class parameter from the fields. A field-used
                        // parameter no field determines is reported like an unbound
                        // callee generic in the call path, then recovered with
                        // `BuiltinUnknown` so this under-specialized class never
                        // reaches MIR lowering carrying a bare type variable — which
                        // would trip `tir2_to_template`'s `unreachable!`.
                        let inferred_type_args: Vec<Ty> = class_generic_params
                            .iter()
                            .map(|param| match bindings.get(param) {
                                Some(bound) => bound.clone(),
                                None => {
                                    if field_constrained_params.contains(param) {
                                        self.context.report_simple(
                                            TirTypeError::CannotInferTypeParameter {
                                                name: param.name().clone(),
                                            },
                                            expr_id,
                                        );
                                    }
                                    Ty::BuiltinUnknown {
                                        attr: TyAttr::default(),
                                    }
                                }
                            })
                            .collect();
                        Ty::Class(class_name, inferred_type_args, attr)
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
        // Class spread is nominal and invariant: the source must be the same
        // resolved class with compatible generic arguments. Besides preventing
        // runtime field-layout violations, checking here ensures every nested
        // expression is fully typed before MIR lowering.
        for (spread_expr, spread_ty) in &spread_types {
            if !matches!(spread_ty, Ty::Unknown { .. } | Ty::Error { .. })
                && !self.is_subtype(spread_ty, &ty)
            {
                self.context.report(
                    TirTypeError::TypeMismatch {
                        expected: ty.clone(),
                        got: spread_ty.clone(),
                    },
                    *spread_expr,
                    Vec::new(),
                );
            }
        }
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
                    self.report_unknown_class_property(
                        expr_id,
                        class_name,
                        field_name,
                        *field_expr,
                        &field_types,
                    );
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
        type_name: &baml_base::core_types::TypePath,
        obj_type_args: &[TypeExpr],
    ) -> Option<Ty> {
        // BEP-044 wf3 #G15: when the literal explicitly names a concrete
        // class that differs from the expected class, it must be a subtype.
        // Keep this out of `check_expr` proper so its temporary lowering
        // state doesn't bloat every recursive checking frame in debug builds.
        let Ty::Class(expected_qtn, _, _) = expected else {
            return None;
        };
        let path = type_name;

        let lit_ty = self.lower_object_type_name(expr_id, path, obj_type_args);
        let declared_mismatch = if let Ty::Class(lit_qtn, _, _) = &lit_ty {
            lit_qtn != expected_qtn
                || (!obj_type_args.is_empty() && !self.is_subtype(&lit_ty, expected))
        } else {
            false
        };
        if declared_mismatch {
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

    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn check_object_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        expected: &Ty,
        fields: &[(Name, ExprId)],
        spreads: &[ast::SpreadField],
        type_name: &baml_base::core_types::TypePath,
        obj_type_args: &[TypeExpr],
    ) -> Ty {
        // Class-instance literals only: map literals (`map { .. }`) are routed
        // to `check_map_object_expr` by the `is_map_object_literal` guard in the
        // `Expr::Object` arm of `check_expr` before reaching here — including the
        // empty `map {}`, which bidirectionally adopts the expected map type.
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
            for spread in spreads {
                self.check_expr(spread.expr, body, expected);
            }
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
                    self.report_unknown_class_property(
                        expr_id,
                        class_name,
                        field_name,
                        *field_expr,
                        &field_types,
                    );
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
                    crate::interfaces::first_failing_impl_bound(
                        db,
                        self.package_id,
                        &inferred,
                        expected,
                        self.aliases,
                        |a, b| self.is_subtype(a, b),
                    )
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

        // Operator-style `recv.to_string()` -> `string.from(recv)` fallback. The
        // AST stays a `Call` (so diagnostics keep saying `.to_string()`, never
        // `string.from`); MIR lowers it. A 0-arg/0-type-arg `to_string` member or
        // local-rooted-path call whose receiver has NO real `baml.ToString` method
        // is typed `string` here. A bare `to_string` method is banned, so the only
        // source of a real one is an `implements baml.ToString` block — which
        // resolves cleanly and is left to normal method dispatch. `string.from` is
        // total (`throws never`, any `T`) and honors overrides via a runtime shim.
        let to_string_callee = type_args.is_empty()
            && arg_exprs.is_empty()
            && crate::throws_analysis::is_to_string_call_callee(&body.exprs[callee])
            // A dotted-path callee is the sugar only when its root is an in-scope
            // value (a local), not a package/module path like `Mod.to_string()`.
            && match &body.exprs[callee] {
                Expr::Path(segs) => self.locals.contains_key(&segs[0]),
                _ => true,
            };
        if to_string_callee {
            // Probe whether a real `to_string` resolves. Snapshot the diagnostic
            // count at the start and again after the receiver: a MemberAccess base
            // may be a complex expr (`f(bad).to_string()`) that legitimately errors
            // while still typing to a known type, so infer it first and keep its
            // errors across a fallback. A Path receiver is a pure dotted name
            // (errors only by failing to resolve), so it shares the start baseline.
            let start = self.context.diagnostic_count();
            let after_recv = match &body.exprs[callee] {
                Expr::MemberAccess { base, .. } => {
                    self.infer_expr(*base, body);
                    self.context.diagnostic_count()
                }
                _ => start,
            };
            let probe_ty = self.infer_expr(callee, body);
            // No real `to_string` (only `implements baml.ToString` provides one)
            // leaves the callee `Unknown`/`Error`; a nullable receiver makes the
            // member access `Unknown | null`, so test the non-null part.
            let unresolved = matches!(
                crate::narrowing::remove_null(&probe_ty),
                Ty::Unknown { .. } | Ty::Error { .. }
            );
            // Fire on any known receiver type, including nullable, since
            // `string.from` is total and renders `null` as `"null"`. A broken
            // (`Unknown`/`Error`) receiver is left to its own error.
            let recv_ty = match &body.exprs[callee] {
                Expr::MemberAccess { base, .. } => self.expressions.get(base).cloned(),
                Expr::Path(segs) => self
                    .path_segment_types
                    .get(&(callee, segs.len() - 2))
                    .cloned(),
                _ => None,
            };
            let recv_known = recv_ty
                .as_ref()
                .is_some_and(|t| !matches!(t, Ty::Unknown { .. } | Ty::Error { .. }));
            if unresolved && recv_known {
                // Drop the `to_string` member-resolution errors (the
                // `UnresolvedMember`, plus the "handle null first" hint for a
                // nullable receiver); keep the receiver's own errors (before
                // `after_recv`).
                self.context.truncate_diagnostics(after_recv);
                let ty = Ty::String {
                    attr: TyAttr::default(),
                };
                self.report_result_type_mismatch(expr_id, &ty, expected);
                self.record_expr_type(expr_id, ty.clone());
                return ty;
            }
            // Not the sugar (real method, or broken receiver): discard everything
            // the probe emitted and let the normal call machinery below re-infer
            // the callee once, so a complex receiver's errors are not double-
            // reported (the explicit receiver infer above would otherwise duplicate
            // them).
            self.context.truncate_diagnostics(start);
        }

        // Operator-style `recv.to_json()` -> `baml.json.from(recv)` fallback, the
        // json analog of the `to_string` sugar above. The AST stays a `Call`; MIR
        // lowers it. A 0-arg/0-type-arg `to_json` member or local-rooted-path call
        // whose receiver has NO real `to_json` method (a bare one is banned; only
        // `implements baml.ToJson` provides one) is typed `json` here.
        // `baml.json.from` is total over any `T` (honoring overrides via a runtime
        // shim) but, unlike `string.from`, throws `JsonSerializationError`.
        let to_json_callee = type_args.is_empty()
            && arg_exprs.is_empty()
            && crate::throws_analysis::is_to_json_call_callee(&body.exprs[callee])
            && match &body.exprs[callee] {
                Expr::Path(segs) => self.locals.contains_key(&segs[0]),
                _ => true,
            };
        if to_json_callee {
            let start = self.context.diagnostic_count();
            let after_recv = match &body.exprs[callee] {
                Expr::MemberAccess { base, .. } => {
                    self.infer_expr(*base, body);
                    self.context.diagnostic_count()
                }
                _ => start,
            };
            let probe_ty = self.infer_expr(callee, body);
            let unresolved = matches!(
                crate::narrowing::remove_null(&probe_ty),
                Ty::Unknown { .. } | Ty::Error { .. }
            );
            let recv_ty = match &body.exprs[callee] {
                Expr::MemberAccess { base, .. } => self.expressions.get(base).cloned(),
                Expr::Path(segs) => self
                    .path_segment_types
                    .get(&(callee, segs.len() - 2))
                    .cloned(),
                _ => None,
            };
            let recv_known = recv_ty
                .as_ref()
                .is_some_and(|t| !matches!(t, Ty::Unknown { .. } | Ty::Error { .. }));
            if unresolved && recv_known {
                self.context.truncate_diagnostics(after_recv);
                let ty = json_alias_ty();
                self.report_result_type_mismatch(expr_id, &ty, expected);
                self.record_expr_type(expr_id, ty.clone());
                return ty;
            }
            self.context.truncate_diagnostics(start);
        }

        // Static-constructor `Type.from_json(j)` -> `baml.json.to<Type>(j)` sugar,
        // the deserialize analog of the `recv.to_json()` fallback above. A 1-arg /
        // 0-type-arg `from_json` call whose receiver is a TYPE NAME with NO real
        // `from_json` method (a bare one is banned; only `implements baml.FromJson`
        // provides one) is typed as the receiver type here; MIR lowers it to
        // `baml.json.to<receiver>(j)` (which dispatches an override or decodes
        // structurally). The receiver type IS the call's result type, so the
        // type-arg threads concretely (`Box<int>` decodes to `Box<int>`).
        let from_json_callee = arg_exprs.len() == 1
            && crate::throws_analysis::is_from_json_call_callee(&body.exprs[callee])
            && match &body.exprs[callee] {
                // Receiver must be a type name (unbound static call), not a value.
                Expr::MemberAccess { base, .. } => match &body.exprs[*base] {
                    Expr::Path(segs) if !segs.is_empty() => !self.locals.contains_key(&segs[0]),
                    _ => false,
                },
                Expr::Path(segs) => segs.len() >= 2 && !self.locals.contains_key(&segs[0]),
                _ => false,
            };
        if from_json_callee {
            let start = self.context.diagnostic_count();
            // Infer the receiver type expression and the json argument (both kept)
            // before probing the callee, so only the callee's `UnresolvedMember`
            // is dropped when the sugar fires.
            if let Expr::MemberAccess { base, .. } = &body.exprs[callee] {
                self.infer_expr(*base, body);
            }
            self.infer_expr(arg_exprs[0], body);
            let after_setup = self.context.diagnostic_count();
            let probe_ty = self.infer_expr(callee, body);
            let unresolved = matches!(
                crate::narrowing::remove_null(&probe_ty),
                Ty::Unknown { .. } | Ty::Error { .. }
            );
            if unresolved {
                // The receiver is the type named by the callee minus `from_json`.
                // The `<int>` in `Box<int>.from_json` parses as the call's type
                // args, applied to the (raw) receiver class so the decoded type is
                // `Box<int>`, not `Box`.
                let recv_ty = match &body.exprs[callee] {
                    Expr::MemberAccess { base, .. } => match self.expressions.get(base).cloned() {
                        Some(Ty::Class(qtn, _, attr)) if !type_args.is_empty() => {
                            let resolved: Vec<Ty> = type_args
                                .iter()
                                .map(|te| self.resolve_type_expr(te, expr_id))
                                .collect();
                            Some(Ty::Class(qtn, resolved, attr))
                        }
                        other => other,
                    },
                    // `path_segment_types` is not populated for package-qualified
                    // type paths (`root.pkg.Type.from_json`), so resolve the
                    // receiver segments as a type directly, threading the call's
                    // type args in as the receiver's generic args.
                    Expr::Path(segs) => {
                        let recv_expr = TypeExprKind::Path {
                            segments: segs[..segs.len() - 1].to_vec(),
                            generic_args: type_args.to_vec(),
                            associated_type_bindings: vec![],
                            attrs: vec![],
                        }
                        .at(text_size::TextRange::default());
                        Some(self.resolve_type_expr(&recv_expr, expr_id))
                    }
                    _ => None,
                };
                let recv_known = recv_ty
                    .as_ref()
                    .is_some_and(|t| !matches!(t, Ty::Unknown { .. } | Ty::Error { .. }));
                if recv_known {
                    let ty = recv_ty.expect("recv_known implies Some");
                    self.context.truncate_diagnostics(after_setup);
                    self.report_result_type_mismatch(expr_id, &ty, expected);
                    self.record_expr_type(expr_id, ty.clone());
                    return ty;
                }
            }
            self.context.truncate_diagnostics(start);
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
        // top-level functions are declaration calls. A value callee is a realized
        // function (no generics of its own); a declaration callee's inferable
        // params come from its declaration, resolved below.
        let is_value_call = matches!(
            &body.exprs[callee],
            Expr::Path(segs) if segs.len() == 1 && self.locals.contains_key(&segs[0])
        );
        let callee_ty = self.infer_expr(callee, body);

        // When explicit type args are written at the call site (e.g. `foo<int, T>(x)`),
        // validate arity and resolve them to a pre-computed bindings map. A `None` from a
        // *written* arg list means it was malformed (wrong arity, already diagnosed) — carried
        // as `Errored` so the unresolved-parameter check doesn't cascade on the params it
        // failed to fill.
        let explicit_type_args = if !type_args.is_empty() {
            match self.resolve_explicit_type_args(callee, type_args, expr_id) {
                Some(bindings) => ExplicitTypeArgs::Resolved(bindings),
                None => ExplicitTypeArgs::Errored,
            }
        } else {
            ExplicitTypeArgs::NotProvided
        };
        let callee_generic_params = self
            .callee_declared_generic_params(callee)
            .map(|(params, _)| params)
            .unwrap_or_default();
        let runtime_generic_layout = self.runtime_generic_layout_for_call(
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
            explicit_type_args,
            callee_expr: Some(callee),
            runtime_generic_layout,
            runtime_type_arg_binding_seed: self
                .owner_type_arg_binding_seed
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
        func_def: &ast::LambdaDef,
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

                // Lambdas cannot declare generic parameters (rejected by the
                // parser), so they only see the enclosing generic scope.
                let all_generic_params = self.generic_params.clone();

                // Determine param types: annotation takes precedence, else use expected
                let mut param_tys: Vec<FunctionParamTy> = Vec::new();
                let mut parameter_mismatch = false;
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
                                self.lower_lambda_type_expr(te, &all_generic_params, te.span);
                            // Check annotation is compatible with expected
                            if !self.is_subtype(&expected_param_ty, &annotated) {
                                parameter_mismatch = true;
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
                    .map(|te| self.lower_lambda_type_expr(te, &all_generic_params, te.span));
                let effective_ret = return_annotation
                    .as_ref()
                    .or_else(|| (!parameter_mismatch).then_some(expected_ret.as_ref()));
                let (throws_ty, throws_span, warn_extraneous_throws) = self
                    .choose_lambda_throws_surface(
                        func_def,
                        &all_generic_params,
                        Some(expected_throws.as_ref()),
                    );

                // Infer/check the lambda body using save/restore approach
                let (ret_ty, lambda_fsi, lambda_effective_throws) = self.infer_lambda_body(
                    func_def,
                    body,
                    &param_tys,
                    effective_ret,
                    &throws_ty,
                    throws_span,
                    warn_extraneous_throws,
                );
                let surface_ret_ty = return_annotation.unwrap_or_else(|| {
                    if parameter_mismatch
                        || matches!(
                            expected_ret.as_ref(),
                            Ty::Unknown { .. } | Ty::TypeVar(_, _)
                        )
                    {
                        ret_ty.clone()
                    } else {
                        expected_ret.as_ref().clone()
                    }
                });
                let surface_throws_ty = throws_ty;

                let result = Ty::Function {
                    params: param_tys,
                    ret: Box::new(surface_ret_ty),
                    throws: Box::new(surface_throws_ty),
                    attr: TyAttr::default(),
                };
                self.lambda_effective_throws
                    .insert(expr_id, lambda_effective_throws.clone());
                if parameter_mismatch {
                    // Callback effect generics are inferred after lambda checking.
                    // Resolve them for this diagnostic while keeping outer generics rigid.
                    let mut diagnostic_bindings = FxHashMap::default();
                    crate::generics::infer_bindings(
                        expected_throws,
                        &lambda_effective_throws,
                        &mut diagnostic_bindings,
                    );
                    diagnostic_bindings
                        .retain(|name, _| !self.generic_params.iter().any(|param| param == name));
                    let diagnostic_expected =
                        crate::generics::substitute_ty(expected_fn_ty, &diagnostic_bindings);
                    let diagnostic_got =
                        crate::generics::substitute_ty(&result, &diagnostic_bindings);
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: diagnostic_expected,
                            got: diagnostic_got,
                        },
                        expr_id,
                        Vec::new(),
                    );
                } else if !crate::generics::contains_typevar(expected_fn_ty)
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
                let expression_ty = if parameter_mismatch {
                    Ty::Error {
                        attr: TyAttr::default(),
                    }
                } else {
                    self.record_function_coercion_if_needed(expr_id, &result, expected_fn_ty);
                    result.clone()
                };
                self.record_expr_type(expr_id, expression_ty.clone());
                if let Some(fsi) = lambda_fsi {
                    self.nested_lambda_types.insert(fsi, result.clone());
                }
                expression_ty
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
                let narrowings = self.uncaptured_condition_narrowings(*condition, body);

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
                // Adopt the declared element type, looking through aliases, a
                // nullable wrapper (`int[]?` is `int[] | null`), and a union with
                // a unique list member (`json` is `… | json[] | …`), so each
                // element checks against the declared element type — containers
                // are invariant, so synthesize-then-subtype would wrongly reject
                // `[1]` against `json[]`. The literal is recorded as the adopted
                // container — the array itself is `int[]`, assignable to `int[]?`.
                let container =
                    self.adopted_container_for_literal(expected, ContainerLiteralKind::List);
                if let Some(container) = container {
                    let (Ty::List(elem_ty, _) | Ty::EvolvingList(elem_ty, _)) = &container else {
                        unreachable!("container is List/EvolvingList by construction")
                    };
                    for e in elements {
                        self.check_expr(*e, body, elem_ty);
                    }
                    self.record_expr_type(expr_id, container.clone());
                    container
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
            } if Self::is_map_object_literal(type_name, type_args, spreads) => {
                self.check_map_object_expr(expr_id, body, expected, fields)
            }
            Expr::Object {
                fields,
                type_name,
                type_args,
                spreads,
                ..
            } => self.check_object_expr(
                expr_id, body, expected, fields, spreads, type_name, type_args,
            ),
            Expr::Map { entries } => {
                // Look through aliases, a nullable wrapper (`map<string, int>?`),
                // and a union with a unique map member (`json` is
                // `… | map<string, json>`), so each entry checks against the
                // declared key/value types — containers are invariant, so
                // synthesize-then-subtype would wrongly reject `{"a": 1}` against
                // `map<string, json>`. The literal is recorded as the adopted
                // container.
                let container =
                    self.adopted_container_for_literal(expected, ContainerLiteralKind::Map);
                if let Some(container) = container {
                    let (Ty::Map {
                        key: key_ty,
                        value: val_ty,
                        ..
                    }
                    | Ty::EvolvingMap(key_ty, val_ty, _)) = &container
                    else {
                        unreachable!("container is Map/EvolvingMap by construction")
                    };
                    for (k, v) in entries {
                        self.check_expr(*k, body, key_ty);
                        self.check_expr(*v, body, val_ty);
                    }
                    self.record_expr_type(expr_id, container.clone());
                    container
                } else {
                    // Expected type is not a map: infer, then report the kind
                    // mismatch like the `Expr::Array` arm does, so a map literal
                    // checked against a non-map type fails closed instead of
                    // silently passing as its inferred `EvolvingMap`/`Map`.
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
            Expr::Match {
                scrutinee, arms, ..
            } => {
                // Checking position: each arm body is checked against the
                // expected type (so an empty `[]`/`{}` arm adopts it), then the
                // joined result is recorded — mirroring the `if`/`if let` arms.
                let ty = self.infer_match_expr(expr_id, *scrutinee, arms, body, Some(expected));
                self.record_expr_type(expr_id, ty.clone());
                ty
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
            // `a ?? b` in checking position: the expected type propagates into the
            // fallback arm — `b` is checked against `expected` directly, so an empty
            // container literal (`xs ?? []`) adopts the expected element type instead of
            // surviving as an evolving container that unions into the result. The LHS
            // stays synthesized (its own nullability drives the chaining lints), and the
            // combined result is still subtype-checked against `expected` (the LHS's
            // non-null part may mismatch on its own).
            Expr::Binary {
                op: baml_compiler2_ast::BinaryOp::NullCoalesce,
                lhs,
                rhs,
            } => {
                let lhs_ty = self.infer_expr(*lhs, body);
                self.report_chaining_lints(
                    baml_compiler2_ast::BinaryOp::NullCoalesce,
                    &lhs_ty,
                    *lhs,
                    *rhs,
                    expr_id,
                    body,
                );
                let rhs_ty = self.check_expr(*rhs, body, expected);
                let ty = self.infer_binary_op(
                    baml_compiler2_ast::BinaryOp::NullCoalesce,
                    &lhs_ty,
                    &rhs_ty,
                    expr_id,
                );
                if !self.is_subtype(&ty, expected) {
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: expected.clone(),
                            got: ty.clone(),
                        },
                        expr_id,
                        Vec::new(),
                    );
                }
                self.record_expr_type(expr_id, ty.clone());
                ty
            }
            Expr::Catch { base, clauses } => {
                // Record the result like the other check_expr arms — `infer_catch_expr`
                // does not record, and unlike the synthesis path there is no
                // `infer_expr` wrapper here to do it, so the catch expr type would
                // otherwise be left unrecorded (and render as `unknown`).
                let ty = self.infer_catch_expr(expr_id, *base, clauses, body, Some(expected));
                self.record_expr_type(expr_id, ty.clone());
                ty
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
                    // A bare reference to a generic *free function* (`let f =
                    // identity`) is an unrealized value: it must be specialized
                    // (`identity<int>`) before use. Reject it with a hard error and
                    // bind the error type (`!error`, not an erased `unknown`) so its
                    // uses don't cascade. Method references are exempt — their
                    // receiver/`Self` is inferred at the call via dynamic dispatch —
                    // and `foo<int>` (a `GenericApply`) is already realized. Fires whenever
                    // the annotation does not *constrain* the specialization — no annotation,
                    // or a non-constraining `unknown`/`error` one (`let x: unknown = identity`);
                    // a real expected type drives expected-type specialization instead (§3.1).
                    // `unknown` (the top type `BuiltinUnknown`) and the recovery `Unknown`
                    // sentinel do not constrain the specialization; a real expected type does.
                    // (`Error` is left to constrain so an already-errored annotation doesn't
                    // cascade a second diagnostic here.)
                    let expected_constrains_specialization =
                        expected_for_check.as_ref().is_some_and(|e| {
                            !matches!(e, Ty::BuiltinUnknown { .. } | Ty::Unknown { .. })
                        });
                    let ty = if !expected_constrains_specialization
                        && self.references_unspecialized_generic_function(init)
                    {
                        self.context.report_simple(
                            TirTypeError::GenericFunctionValueNotSpecialized {
                                name: Self::generic_apply_base_name(init, body),
                            },
                            init,
                        );
                        Ty::Error {
                            attr: TyAttr::default(),
                        }
                    } else {
                        ty
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

                    let diag_count_before_pattern = self.context.diagnostic_count();
                    let result =
                        self.analyze_and_lower(*pattern, &flow_ty, body, initializer.unwrap());
                    // A pattern that already errored (e.g. an unknown destructure
                    // field) must not feed the refutability/reachability passes —
                    // they'd add misleading secondary diagnostics on top of the
                    // real one.
                    let pattern_had_error =
                        self.context.diagnostic_count() > diag_count_before_pattern;
                    // Irrefutable-pattern check differs by binding form:
                    //   - plain `let`: refutable patterns are an error
                    //     (RefutablePatternInLet) — they'd fail at runtime
                    //     with nowhere to go.
                    //   - `let … else`: refutable is the whole point, but an
                    //     irrefutable pattern makes the else branch dead, so
                    //     warn (IrrefutablePatternInLetElse) and suggest
                    //     dropping the else.
                    let irrefutable_ctx = if pattern_had_error || else_branch.is_some() {
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
                    if else_branch.is_some() && !pattern_had_error {
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
                if self.in_defer() {
                    self.report_defer_escape("return", stmt_id);
                }
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
            Stmt::Defer { body: defer_body } => {
                // BEP-042: check the inline defer body as an effect. Push the
                // current loop depth so escaping `break`/`continue`/`return`
                // inside the body are rejected (loop-aware: a `break` targeting
                // a loop declared *inside* the defer is allowed).
                self.defer_loop_floors.push(self.loop_depth);
                // The defer body runs at scope exit, not at the `defer` site, so
                // isolate its scoped-local narrowing/assignments — they must not
                // leak into the statements between the `defer` and the scope
                // exit (mirrors the per-body snapshot the loop arms take).
                let snapshot = self.snapshot_scoped_locals();
                self.infer_expr(*defer_body, body);
                self.restore_scoped_locals(&snapshot);
                self.defer_loop_floors.pop();
                false // a `defer` statement does not diverge
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
                self.loop_depth += 1;
                self.infer_expr(*while_body, body);
                self.loop_depth -= 1;
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
                let diag_count_before_pattern = self.context.diagnostic_count();
                let result = self.analyze_and_lower(*pattern, &scrutinee_ty, body, *while_body);
                let pattern_had_error = self.context.diagnostic_count() > diag_count_before_pattern;

                // Body scope: narrow the scrutinee to the matched type and
                // register the pattern bindings for the body only, then restore.
                let snapshot = self.snapshot_scoped_locals();
                if let Some(name) = &scrutinee_name {
                    self.narrow_uncaptured_local(*scrutinee, name, result.matched_ty.clone());
                }
                self.finalize_pattern_lowering(*pattern, &result, None, None, &scrutinee_ty);
                self.loop_depth += 1;
                self.infer_expr(*while_body, body);
                self.loop_depth -= 1;
                self.restore_scoped_locals(&snapshot);

                // Irrefutability warning — same policy as `if let`. An
                // irrefutable `while let` never exits via pattern failure, so it
                // is an unconditional infinite loop with a pointless pattern;
                // warn and suggest a plain `while`/`loop`. Skipped when the
                // pattern already errored — the invalid pattern's matrix row
                // would make the warning meaningless.
                if !pattern_had_error {
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
                let diag_count_before_pattern = self.context.diagnostic_count();
                let result = self.analyze_and_lower(*binding, &flow_ty, body, *for_body);
                let pattern_had_error = self.context.diagnostic_count() > diag_count_before_pattern;
                self.finalize_pattern_lowering(
                    *binding,
                    &result,
                    declared_for_scope.as_ref(),
                    // A pattern that already errored must not also be judged for
                    // refutability — the real diagnostic covers it.
                    if pattern_had_error {
                        None
                    } else {
                        Some(IrrefutablePatternContext {
                            context: IrrefutableContextKind::ForLet,
                            fallback_expr: Some(*collection),
                        })
                    },
                    &flow_ty,
                );

                // 5. Check the body
                self.loop_depth += 1;
                self.infer_expr(*for_body, body);
                self.loop_depth -= 1;
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
                //
                // `$id = e` is a special form (MIR lowers it to `baml.id.set(e)`,
                // see lower.rs); `$id` is not a local, so type-check the RHS
                // against the builtin's `string` parameter here.
                let declared_ty = if Self::is_runtime_id_path(body, *target) {
                    Some(Ty::String {
                        attr: TyAttr::default(),
                    })
                } else {
                    self.get_declared_type(*target, body)
                };
                // A container literal assigned to a matching structural declared
                // type is typed by `check_expr`, which adopts the declared
                // element/key/value types *recursively* (so nested `[[]]` does not
                // leak `EvolvingList(Never)` under an `int[][]` declaration) and
                // reports any mismatch once. Everything else infers and
                // subtype-checks. Typing exactly once avoids duplicate diagnostics.
                let adopt_container_literal = declared_ty.as_ref().is_some_and(|decl_ty| {
                    !matches!(decl_ty, Ty::Unknown { .. } | Ty::Error { .. })
                        && matches!(
                            (&body.exprs[*value], decl_ty),
                            (Expr::Array { .. }, Ty::List(..) | Ty::EvolvingList(..))
                                | (Expr::Map { .. }, Ty::Map { .. } | Ty::EvolvingMap(..))
                        )
                });
                let value_ty = if adopt_container_literal {
                    let decl_ty = declared_ty
                        .as_ref()
                        .expect("adopt_container_literal implies a declared type");
                    self.check_expr(*value, body, decl_ty);
                    decl_ty.clone()
                } else {
                    self.infer_expr(*value, body)
                };
                if let Some(ref decl_ty) = declared_ty {
                    if !adopt_container_literal {
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
                    }
                    // Update the local to the assigned value's type (invalidates
                    // narrowing). `$id` is not a local — don't create one.
                    //
                    // A non-literal evolving-empty value (`x = y` where `y` stayed
                    // `EvolvingList(Never)`) still mirrors the declared type so the
                    // local keeps its declaration; otherwise a later `x.push(v)`
                    // would re-establish the element type from `v`, accepting a
                    // wrong-typed value under the declared one (unsound). The
                    // literal case was already adopted recursively above.
                    let assigned_ty = if matches!(decl_ty, Ty::Error { .. }) {
                        decl_ty.clone()
                    } else if adopt_container_literal {
                        value_ty
                    } else {
                        match (&value_ty, decl_ty) {
                            (Ty::EvolvingList(..), Ty::List(..) | Ty::EvolvingList(..))
                            | (Ty::EvolvingMap(..), Ty::Map { .. } | Ty::EvolvingMap(..)) => {
                                self.record_expr_type(*value, decl_ty.clone());
                                decl_ty.clone()
                            }
                            _ => value_ty,
                        }
                    };
                    if let Expr::Path(segments) = &body.exprs[*target] {
                        if segments.len() == 1 && segments[0].as_str() != "$id" {
                            self.assign_local(segments[0].clone(), assigned_ty);
                        }
                    }
                } else {
                    // No annotated declared type. Member/index/`$id` targets are
                    // handled structurally elsewhere; an *unannotated local*,
                    // though, still carries a flow type in `current_ty` that a
                    // reassignment must stay compatible with. Without this guard
                    // `let x = {}; x = []` keeps `x` map-typed while it holds an
                    // array at runtime, so indexing it aborts the VM with
                    // "expected map, got array" (B-236).
                    if let Expr::Path(segments) = &body.exprs[*target]
                        && segments.len() == 1
                        && segments[0].as_str() != "$id"
                        && let Some(current_ty) =
                            self.locals.get(&segments[0]).map(|b| b.current_ty.clone())
                        && !self.reassignment_is_compatible(&value_ty, &current_ty)
                    {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: current_ty,
                                got: value_ty.clone(),
                            },
                            *value,
                            Vec::new(),
                        );
                    }
                    self.infer_expr(*target, body);
                    self.infer_expr(*value, body);
                }
                if target_has_optional {
                    self.in_optional_chain -= 1;
                }
                false
            }
            Stmt::AssignOp { target, op, value } => {
                // `$id OP= e` can never be meaningful: `baml.id.set` only
                // accepts fresh overrides from `baml.id.new()`, so a derived
                // value (e.g. `$id + "-suffix"`) is always rejected at
                // runtime. Fail at compile time instead of silently no-oping
                // (MIR's lvalue lowering has no `$id` write target).
                if Self::is_runtime_id_path(body, *target) {
                    self.context
                        .report_simple(TirTypeError::RuntimeIdCompoundAssignment, *target);
                    self.infer_expr(*value, body);
                    return false;
                }
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
                // `t OP= v` stores the operator result back into `t`, so the
                // result must be assignable to the target — not a given once
                // operators dispatch through `baml.ops` impls whose `Output`
                // can differ from `Self` (`Counter += 1` with `Output = int`
                // would leave an `int` in a `Counter` slot). The target is
                // widened to its base first so a literal-flow-typed local
                // (`let x = 1; x += 2`) compares against `int`, not `1`.
                let widened_target = Self::widen_literal_base(&effective_target_ty);
                if !self.reassignment_is_compatible(&result_ty, &widened_target) {
                    self.context.report_simple(
                        TirTypeError::TypeMismatch {
                            expected: widened_target,
                            got: result_ty.clone(),
                        },
                        *value,
                    );
                }
                // Re-record the value expression with the result type so the
                // display shows the operation result, not the raw RHS literal.
                self.record_expr_type(*value, result_ty);
                if target_has_optional {
                    self.in_optional_chain -= 1;
                }
                false
            }
            Stmt::Break => {
                if self.break_escapes_defer() {
                    self.report_defer_escape("break", stmt_id);
                }
                true // break diverges
            }
            Stmt::Continue => {
                if self.break_escapes_defer() {
                    self.report_defer_escape("continue", stmt_id);
                }
                true // continue diverges
            }
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
                            self.narrow_uncaptured_local(scrutinee, &name, complement);
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
                let narrowings = self.uncaptured_condition_narrowings(condition, body);

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

    /// Whether reassigning a value of type `value_ty` to an *unannotated* local
    /// currently typed `current_ty` is sound (B-236).
    ///
    /// A concrete local requires the new value to be a subtype — its static
    /// type and the runtime value must not diverge. An *empty evolving*
    /// container (`[]` / `{}`, whose element/key/value types are `never`) is
    /// special: it has not yet committed to an element type, so it may still
    /// adopt any value of the *same container kind* — `let a = []; a = [1, 2,
    /// 3]` keeps working — but never across kinds (`let x = {}; x = []` is the
    /// crash). Cross-kind and unrelated-type reassignments are rejected.
    fn reassignment_is_compatible(&self, value_ty: &Ty, current_ty: &Ty) -> bool {
        // Error recovery / a diverging RHS: don't pile on additional errors.
        if matches!(
            value_ty,
            Ty::Unknown { .. } | Ty::Error { .. } | Ty::Never { .. }
        ) || matches!(current_ty, Ty::Unknown { .. } | Ty::Error { .. })
        {
            return true;
        }
        if self.is_subtype(value_ty, current_ty) {
            return true;
        }
        match current_ty {
            // Empty evolving list: accepts any list-shaped value.
            Ty::List(inner, _) | Ty::EvolvingList(inner, _)
                if matches!(**inner, Ty::Never { .. }) =>
            {
                matches!(value_ty, Ty::List(..) | Ty::EvolvingList(..))
            }
            // Empty evolving map: accepts any map-shaped value.
            Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _)
                if matches!(**key, Ty::Never { .. }) && matches!(**value, Ty::Never { .. }) =>
            {
                matches!(value_ty, Ty::Map { .. } | Ty::EvolvingMap(..))
            }
            _ => false,
        }
    }

    /// True when `expr` is the bare `$id` special form (the runtime-identity
    /// read/write target — see the `$id` stub in `infer_path` and the MIR
    /// lowering in lower.rs).
    fn is_runtime_id_path(body: &ExprBody, expr: ExprId) -> bool {
        matches!(&body.exprs[expr], Expr::Path(segments)
            if segments.len() == 1 && segments[0].as_str() == "$id")
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

    /// Infer and validate a `match` expression against the scrutinee type.
    ///
    /// If any pattern reports an error, usefulness analysis is skipped for the
    /// whole match so it cannot emit dependent exhaustiveness or reachability
    /// diagnostics.
    fn infer_match_expr(
        &mut self,
        match_expr_id: ExprId,
        scrutinee_expr_id: ExprId,
        arms: &[baml_compiler2_ast::MatchArmId],
        body: &ExprBody,
        // When the match is in a checking position, the expected type each arm
        // body is checked against — so e.g. an empty `[]` arm adopts the
        // declared element type (like `if`'s branches), instead of leaving the
        // arm `never[]`. `None` in synthesis position.
        expected: Option<&Ty>,
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
        let mut matrix_arm_ids: Vec<ExprId> = Vec::new();
        let mut match_had_pattern_error = false;

        for arm_id in arms {
            let arm = &body.match_arms[*arm_id];
            let pattern_id = arm.pattern;

            let diag_count_before_pattern = self.context.diagnostic_count();
            let result = self.analyze_and_lower(pattern_id, &scrutinee_ty, body, arm.body);
            let pattern_had_error = self.context.diagnostic_count() > diag_count_before_pattern;
            match_had_pattern_error |= pattern_had_error;
            let narrowed = result.matched_ty.clone();

            // Snapshot/restore the scope for this arm's bindings.
            let snapshot = self.snapshot_scoped_locals();

            // Narrow the scrutinee local for the arm body.
            if let Some(name) = &scrutinee_name {
                self.narrow_uncaptured_local(scrutinee_expr_id, name, narrowed.clone());
            }

            self.finalize_pattern_lowering(pattern_id, &result, None, None, &scrutinee_ty);

            if let Some(guard_expr) = arm.guard {
                self.infer_expr(guard_expr, body);
            }

            let arm_ty = match expected {
                Some(expected) => self.check_expr(arm.body, body, expected),
                None => self.infer_expr(arm.body, body),
            };
            arm_types.push(arm_ty);

            self.restore_scoped_locals(&snapshot);

            // Guarded arms don't contribute to coverage.
            if arm.guard.is_none() {
                matrix_arms.push(result.dpat);
                matrix_arm_ids.push(arm.body);
            }
        }

        if match_had_pattern_error {
            return Self::join_all(&arm_types);
        }

        // Pass 2: run matrix analysis for exhaustiveness and reachability.
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
        let exhaustive = report.missing.is_empty();
        if exhaustive {
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
        let diag_count_before_pattern = self.context.diagnostic_count();
        let result = self.analyze_and_lower(pattern_id, &scrutinee_ty, body, then_branch);
        let pattern_had_error = self.context.diagnostic_count() > diag_count_before_pattern;
        let matched_ty = result.matched_ty.clone();

        // Then-branch: push a fresh scope, narrow scrutinee, register
        // pattern bindings, then infer/check the body.
        let snapshot = self.snapshot_scoped_locals();
        if let Some(name) = &scrutinee_name {
            self.narrow_uncaptured_local(scrutinee_expr_id, name, matched_ty.clone());
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
                self.narrow_uncaptured_local(scrutinee_expr_id, name, complement);
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
        if !pattern_had_error {
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
            //
            // An empty residual means the base throws nothing the type system
            // tracks, so the binding can never be assigned a (user-thrown) value:
            // its type is the bottom type `Never`, not the error-recovery
            // `Unknown`. Any runtime panic a `let`-binding arm catches is folded
            // in per-arm via `ty_panic_subset` below.
            let clause_binding_ty = if residual.is_empty() {
                Ty::Never {
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
                        let st_name = baml_base::Name::new("ErrorContext");
                        items.lookup_type(&errors_ns, &st_name)
                    })
                    .map(|def| {
                        let st_name = baml_base::Name::new("ErrorContext");
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
                let probe_diag_start = self.context.diagnostic_count();
                let arm_probe = self.analyze_and_lower_no_subtype_check(
                    arm.pattern,
                    &clause_binding_ty,
                    body,
                    arm.body,
                );
                self.context.truncate_diagnostics(probe_diag_start);
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

                let throw_matches = self.throw_matches_from_ty(&narrowed_ty, &residual);
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

                // In a checking position, adopt the expected type into the
                // handler body too (not just the catch base) — so e.g. an empty
                // `[]` handler adopts the declared element type rather than
                // leaking `EvolvingList(Never)`. Mirrors the base above.
                let arm_ty = match expected {
                    Some(expected) => self.check_expr(arm.body, body, expected),
                    None => self.infer_expr(arm.body, body),
                };
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
                if !residual.is_empty() {
                    let missing = residual
                        .iter()
                        .map(Ty::render_user_facing)
                        .collect::<Vec<_>>();
                    self.context.report_simple(
                        TirTypeError::NonExhaustiveCatchAll {
                            caught_type: clause_binding_ty,
                            missing_cases: missing,
                        },
                        catch_expr_id,
                    );
                }
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
        type_refs: &baml_compiler2_hir::type_ref::TypeRefStore,
        declared_throws: Option<baml_compiler2_hir::type_ref::TypeRefId>,
        throws_span: Option<TextRange>,
        fallback_span: TextRange,
        warn_extraneous: bool,
    ) {
        let Some(declared_id) = declared_throws else {
            return;
        };

        let mut diags = Vec::new();
        let declared_ty = crate::lower_type_expr::lower_type_ref(
            type_refs,
            declared_id,
            &crate::lower_type_expr::ScopeCtx {
                db: self.context.db(),
                package_items: self.package_items,
                ns_context: &self.ns_context,
                generic_params: &self.generic_params,
                bounds: &self.scope_type_var_bounds(),
                self_ty: None,
            },
            &mut diags,
        );
        let span = throws_span.unwrap_or(fallback_span);
        for diag in diags {
            self.context.report_at_span(diag, span);
        }
        self.validate_type_generic_bounds_at_span(span, &declared_ty);
        self.check_throws_surface(body, body.root_expr, &declared_ty, span, warn_extraneous);
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
            let ty_expr = TypeExprKind::Path {
                segments: class.to_vec(),
                generic_args: generic_args.to_vec(),
                associated_type_bindings: associated_type_bindings.to_vec(),
                attrs: Vec::new(),
            }
            .at(text_size::TextRange::default());
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
        !baml_compiler2_ppir::item_data::class_data(db, class_loc)
            .generic_params
            .is_empty()
    }

    /// Does `ty` contain an *unresolved* leaf — an error-recovery sentinel
    /// (`Ty::Unknown`/`Ty::Error`, minted where resolution already failed and
    /// was diagnosed at its own site) or a still-in-flight inference
    /// placeholder (`Ty::Infer`)? Callers skip checks whose subject is not
    /// (yet) a determined type, so diagnostics don't cascade off an
    /// already-failed or undetermined input.
    ///
    /// The builtin `unknown` top type (`Ty::BuiltinUnknown`) deliberately
    /// does NOT count: user-written `unknown` is a real, fully-determined
    /// type — the top of the subtype lattice, invariant in argument positions
    /// like any other type — and must never behave like a hole. A `let`
    /// annotation like `unknown[]` is a usable expected type that pins its
    /// binding (so a later heterogeneous `push` type-checks); an `unknown[]`
    /// pattern covers exactly the `unknown[]` member.
    fn ty_contains_unresolved(ty: &Ty) -> bool {
        match ty {
            Ty::Unknown { .. } | Ty::Error { .. } | Ty::Infer { .. } => true,
            Ty::BuiltinUnknown { .. } => false,
            Ty::Class(_, args, _) | Ty::Interface(_, args, _, _) | Ty::Union(args, _) => {
                args.iter().any(Self::ty_contains_unresolved)
            }
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => {
                Self::ty_contains_unresolved(base)
                    || interface.tys().any(Self::ty_contains_unresolved)
            }
            Ty::List(elem, _) | Ty::EvolvingList(elem, _) => Self::ty_contains_unresolved(elem),
            Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => {
                Self::ty_contains_unresolved(key) || Self::ty_contains_unresolved(value)
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params
                    .iter()
                    .any(|param| Self::ty_contains_unresolved(&param.ty))
                    || Self::ty_contains_unresolved(ret)
                    || Self::ty_contains_unresolved(throws)
            }
            Ty::Future(value, error, _) => {
                Self::ty_contains_unresolved(value) || Self::ty_contains_unresolved(error)
            }
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

    /// Narrow a scrutinee's flow type (`incoming`) through a pattern's type
    /// (`constraint`) — the pattern-side entry point used for match/`is` arm
    /// narrowing and pattern ascriptions. The result is simultaneously the
    /// binding's type, the arm-body narrowing, AND the type MIR emits the
    /// arm's runtime `IsType` test from — so it must never *under*-approximate
    /// the set of values the written pattern admits, or the arm silently stops
    /// matching values the spec says it matches.
    ///
    /// Two regimes:
    ///
    /// - **Concrete (no in-scope rigid vars / projections on either side):**
    ///   [`Self::intersect_types`] — exact, since concrete meets are decidable.
    ///
    /// - **Rigid-involving:** a rigid variable is only *potentially* unifiable
    ///   with other types, so the equality/subtype meet under-approximates
    ///   (dropping possibly-inhabited members). The arm therefore matches and
    ///   binds exactly its **written** pattern type: the runtime test is
    ///   invariant and frame-realized, so any value that passes belongs to the
    ///   pattern type as written. Exceptions: an irrefutable arm
    ///   (`scrutinee <: pattern`) keeps the (narrower, exact) scrutinee type,
    ///   and a provably-impossible pair — the overlap oracle's `No`, trusted
    ///   only when every type variable on both sides is in scope — stays
    ///   `Never` (reported as a type error by `check_pattern_vs_scrut_subtype`).
    fn intersect_pattern_flow_types(&self, incoming: &Ty, constraint: &Ty) -> Ty {
        if matches!(incoming, Ty::Unknown { .. } | Ty::Error { .. }) {
            return constraint.clone();
        }
        if matches!(constraint, Ty::Unknown { .. } | Ty::Error { .. }) {
            return incoming.clone();
        }
        if self.contains_in_scope_rigid_or_projection(incoming)
            || self.contains_in_scope_rigid_or_projection(constraint)
        {
            let incoming_expanded = self.expand_alias_chains(incoming.clone());
            let constraint_expanded = self.expand_alias_chains(constraint.clone());
            // Irrefutable: every scrutinee value matches, and the scrutinee
            // type is the exact (possibly narrower) description of them.
            if self.is_subtype(&incoming_expanded, &constraint_expanded) {
                return incoming.clone();
            }
            // Provably dead — only when the oracle's verdict is trustworthy:
            // a type variable outside `generic_params` is an opaque atom to
            // the oracle, so a `No` over such a type would be judging a
            // variable it cannot see (a false dead-arm error).
            if self.all_type_vars_in_scope(&incoming_expanded)
                && self.all_type_vars_in_scope(&constraint_expanded)
                && self.pattern_overlap_verdict(constraint, incoming) == crate::unify::Overlap::No
            {
                return Ty::Never {
                    attr: TyAttr::default(),
                };
            }
            return constraint.clone();
        }
        self.intersect_types(incoming, constraint)
    }

    /// Whether every type variable occurring in `ty` is one of this scope's
    /// rigid params — the completeness precondition for trusting the overlap
    /// oracle's `No` verdicts (see [`crate::pattern_overlap::PatternOverlapEnv`]).
    fn all_type_vars_in_scope(&self, ty: &Ty) -> bool {
        !crate::generics::contains_typevar_where(ty, &|name| !self.generic_params.contains(name))
    }

    /// Whether `ty` mentions an in-scope rigid type parameter
    /// (`self.generic_params`, which includes `Self` in interface-owned bodies)
    /// or an associated-type projection — the inputs whose reachability the
    /// equality/subtype relations cannot decide and the overlap oracle can.
    ///
    /// Synthetic effect parameters (`__effect_param_N`, rendered `callback`)
    /// count like any other rigid: they ride in a function type's `throws`
    /// position, where matching follows function-type variance (`throws` is
    /// covariant), so a pattern that does not care about the effect coarsens
    /// it — `throws unknown` is a supertype of every `throws E` and the
    /// ordinary subtype checks decide such pairs without special-casing.
    fn contains_in_scope_rigid_or_projection(&self, ty: &Ty) -> bool {
        crate::generics::contains_ty_where(ty, &|t| match t {
            Ty::TypeVar(name, _) => self.generic_params.contains(name),
            Ty::AssociatedTypeProjection { .. } => true,
            _ => false,
        })
    }

    /// The pattern reachability oracle over this scope: can `pat` and `member`
    /// share a value under some realization of the in-scope rigid params? See
    /// [`crate::pattern_overlap::pattern_overlap`] for the semantics
    /// (`Yes`/`Unknown` = possible, `No` = provably dead).
    fn pattern_overlap_verdict(&self, pat: &Ty, member: &Ty) -> crate::unify::Overlap {
        let db = self.context.db();
        let aliases = self
            .normalized_overlap_aliases
            .get_or_init(|| crate::unify::normalized_alias_map(db, self.package_id));
        let enum_variants =
            |qtn: &crate::ty::QualifiedTypeName| crate::unify::enum_variant_names(db, qtn);
        let implements = |ty: &Ty, iface: &baml_type::Interface| {
            crate::interfaces::get_implements_block(db, self.package_id, ty, iface, aliases)
                .is_some()
        };
        crate::pattern_overlap::pattern_overlap(
            pat,
            member,
            &crate::pattern_overlap::PatternOverlapEnv {
                vars: &self.generic_params,
                bounds: &self.generic_param_bounds,
                aliases,
                enum_variants: &enum_variants,
                implements: &implements,
            },
        )
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
        // resolved: not polluted by an unresolved leaf (`Unknown`/`Error`
        // recovery, in-flight `Infer`), and not still carrying an
        // *unspecialized* generic. Declared generics live on the type as
        // `TypeVar` args now, so an unspecialized class shows up as a
        // non-rigid type var; the enclosing function's own rigid params (e.g.
        // the `Err` in `AllFailed<Err>`) are already bound and so don't
        // disqualify it. A genuine `unknown` annotation (e.g. `unknown[]`) is
        // a usable expected type — the builtin `unknown` top type never
        // disqualifies.
        if Self::ty_contains_unresolved(&ty)
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
    ) -> FxHashSet<Name> {
        let mut conflicting_names = FxHashSet::default();
        for (name, entries) in bindings_by_name {
            let Some((_, first_ty)) = entries.first() else {
                continue;
            };
            for (pat, other_ty) in entries.iter().skip(1) {
                if Self::ty_contains_unresolved(first_ty) || Self::ty_contains_unresolved(other_ty)
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
                    conflicting_names.insert(name.clone());
                }
            }
        }
        conflicting_names
    }

    /// Resolve a `TypeExpr` to a `Ty`.  Tries `bare_type_sugar_to_ty` first
    /// (handles `baml.panics.*` types and primitives), falls back to
    /// `lower_pattern_type_expr` for user-defined types.
    fn resolve_type_expr(&mut self, ty: &TypeExpr, at_expr: ExprId) -> Ty {
        if let TypeExprKind::Path { segments, .. } = &ty.kind {
            if segments.len() == 1 {
                if let Some(resolved) = bare_type_sugar_to_ty(&segments[0]) {
                    return resolved;
                }
            }
        }
        self.lower_pattern_type_expr(ty, at_expr)
    }

    fn resolve_type_expr_silent(&self, ty: &TypeExpr) -> Ty {
        if let TypeExprKind::Path { segments, .. } = &ty.kind {
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
        if let TypeExprKind::Path { segments, .. } = &ty.kind {
            if segments.len() == 1 {
                if let Some(resolved) = bare_type_sugar_to_ty(&segments[0]) {
                    return resolved;
                }
            }
        }
        let mut diags = Vec::new();
        let resolved = crate::lower_type_expr::lower_type_expr(
            ty,
            &crate::lower_type_expr::ScopeCtx {
                db: self.context.db(),
                package_items: self.package_items,
                ns_context: &self.ns_context,
                generic_params: &self.generic_params,
                bounds: &self.scope_type_var_bounds(),
                self_ty: self.body_self_ty.clone(),
            },
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
    fn ty_covers_fact(&self, pattern_ty: &Ty, fact: &Ty) -> bool {
        if pattern_ty == fact {
            return true;
        }
        // An interface pattern covers a fact when every value of the fact is
        // an implementer — e.g. `let f: ai.Failure` over a concrete throw fact
        // whose class implements `ai.Failure`.
        if matches!(pattern_ty, Ty::Interface(..)) {
            return self.is_subtype(fact, pattern_ty);
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
            Ty::Union(parts, _) => parts.iter().any(|part| self.ty_covers_fact(part, fact)),
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

    fn ty_may_match_fact(&self, pattern_ty: &Ty, fact: &Ty) -> bool {
        // A concrete pattern can match a value thrown under an interface fact
        // when the pattern's type implements that interface: the runtime value
        // behind `throws ai.Failure` can be any implementing class, so a
        // `let q: MyQuotaError` arm is a reachable refinement, not dead code.
        if matches!(fact, Ty::Interface(..)) && self.is_subtype(pattern_ty, fact) {
            return true;
        }
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

    fn ty_match_strength(&self, narrowed_ty: &Ty, throw_fact: &Ty) -> PatternMatchStrength {
        let is_unknown = matches!(
            throw_fact,
            Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. }
        );
        if self.ty_covers_fact(narrowed_ty, throw_fact) {
            PatternMatchStrength::DefiniteMatch
        } else if is_unknown || self.ty_may_match_fact(narrowed_ty, throw_fact) {
            PatternMatchStrength::MayMatch
        } else {
            PatternMatchStrength::NoMatch
        }
    }

    fn throw_matches_from_ty(
        &self,
        narrowed_ty: &Ty,
        throw_types: &BTreeSet<Ty>,
    ) -> ThrowPatternMatches {
        let mut out = ThrowPatternMatches::default();
        for throw_fact in throw_types {
            match self.ty_match_strength(narrowed_ty, throw_fact) {
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

    fn collect_effective_throws(&self, body: &ExprBody, root: Option<ExprId>) -> BTreeSet<Ty> {
        crate::throws_analysis::collect_escaping_throws_from(
            &BuilderThrowsAnalysis { builder: self },
            body,
            root,
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
        if let Some(throws) = call_plan.and_then(|plan| plan.instantiated_throws.clone()) {
            return Some(throws);
        }
        let callee_ty = self.expressions.get(&callee_expr_id)?;
        let typed_callee = if unwrap_optional_callee {
            self.analyze_optional_base(callee_ty).inner
        } else {
            callee_ty.clone()
        };
        // Resolve a function-type alias callee to its underlying `Ty::Function`
        // so its `throws` is read rather than dropped to `None`.
        let typed_callee = self.expand_alias_chains(typed_callee);

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
        // Post-substitution collapse: a projection whose base just became concrete
        // reduces to its realization (`(T as HasErr).E` at `T = Risky` IS `Kaboom`),
        // so the throw facts — and everything downstream of them: catch narrowing,
        // exhaustiveness, the runtime pattern tests MIR lowers — see the real type.
        // Gated so types without such a projection are left exactly as written.
        if crate::generics::contains_concrete_base_projection(&substituted) {
            return Some(self.normalize(&substituted));
        }
        Some(substituted)
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
            Expr::Return { value } => {
                // Evaluating the returned value may itself throw, so walk it —
                // but unlike `throw`, the value is returned normally, not
                // raised as an error (no `collect_throw_facts_from_value`).
                if let Some(value) = value {
                    self.collect_throw_facts_from_expr(*value, body, out);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.collect_throw_facts_from_expr(*callee, body, out);
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                for arg in args {
                    self.collect_throw_facts_from_expr(arg.expr, body, out);
                }
                let analysis = BuilderThrowsAnalysis { builder: self };
                // A `to_string`/`to_json`/`from_json` sugar fallback resolves to no
                // real method, so `collect_callee_escaping_throws` would fall through
                // to its unaccounted-callee default and charge a bogus `Ty::Unknown`
                // fact. Charge what the rewritten stdlib call actually throws instead
                // — same guard the inferred-`throws` walker applies
                // (`throws_analysis::collect_from_expr`). Without it a catch binder
                // over e.g. `f().to_string()` was typed `... | Unknown`, and that
                // error-recovery sentinel ICE'd MIR's runtime-type lowering.
                if let Some(sugar) = crate::throws_analysis::sugar_fallback_call_throws(
                    &analysis, *callee, &arg_exprs, body,
                ) {
                    out.extend(sugar);
                    return;
                }
                crate::throws_analysis::collect_callee_escaping_throws(
                    &analysis, expr_id, *callee, &arg_exprs, body, false, out,
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
                    expr_id,
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
            Expr::Template { tag, segments } => {
                if let ast::TemplateTag::Custom { tag, .. } = tag {
                    self.collect_throw_facts_from_expr(*tag, body, out);
                }
                Self::collect_throw_facts_from_template_segments(self, segments, body, out);
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

    /// Recursive walk of a tagged-template segment tree collecting throw
    /// facts from interpolated/condition/iter expressions and any nested
    /// for/if bodies.
    fn collect_throw_facts_from_template_segments(
        &self,
        segments: &[ast::TemplateSegment],
        body: &ExprBody,
        out: &mut BTreeSet<Ty>,
    ) {
        for seg in segments {
            match seg {
                ast::TemplateSegment::Text(_) => {}
                ast::TemplateSegment::Interp(e) => {
                    self.collect_throw_facts_from_expr(*e, body, out);
                }
                ast::TemplateSegment::For {
                    collection,
                    body: inner,
                    ..
                } => {
                    self.collect_throw_facts_from_expr(*collection, body, out);
                    self.collect_throw_facts_from_template_segments(inner, body, out);
                }
                ast::TemplateSegment::CStyleFor {
                    init,
                    cond,
                    step,
                    body: inner,
                } => {
                    // `init` (a `let`) and `step` (an assignment) are statements
                    // whose initializer/RHS can throw, so walk them too.
                    self.collect_throw_facts_from_stmt(*init, body, out);
                    self.collect_throw_facts_from_expr(*cond, body, out);
                    if let Some(step_stmt) = step {
                        self.collect_throw_facts_from_stmt(*step_stmt, body, out);
                    }
                    self.collect_throw_facts_from_template_segments(inner, body, out);
                }
                ast::TemplateSegment::If {
                    branches,
                    else_body,
                } => {
                    for branch in branches {
                        self.collect_throw_facts_from_expr(branch.condition, body, out);
                        self.collect_throw_facts_from_template_segments(&branch.body, body, out);
                    }
                    if let Some(eb) = else_body {
                        self.collect_throw_facts_from_template_segments(eb, body, out);
                    }
                }
            }
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
                // `$id = e` is an implicit `baml.id.set(e)` call (MIR
                // `lower_set_runtime_id`); its declared throws escape here
                // exactly as a direct call's would.
                if Self::is_runtime_id_path(body, *target) {
                    match self.lookup_named_throw_summary(&Name::new("id.set")) {
                        Some(summary) => out.extend(summary),
                        None => {
                            out.insert(Ty::Unknown {
                                attr: TyAttr::default(),
                            });
                        }
                    }
                }
            }
            Stmt::Throw { value } => {
                self.collect_throw_facts_from_expr(*value, body, out);
                self.collect_throw_facts_from_value(*value, out);
            }
            Stmt::Defer { body: defer_body } => {
                // A `throw` inside a defer body propagates (replace-semantics),
                // so the defer body's throws are part of the enclosing
                // function's throw surface.
                self.collect_throw_facts_from_expr(*defer_body, body, out);
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
                let class_data = baml_compiler2_ppir::item_data::class_data(db, *class_loc);
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

    fn infer_literal(&mut self, lit: &baml_base::Literal, expr_id: ExprId) -> Ty {
        // An `int` literal whose value doesn't fit i63 (e.g. 2^62, a valid i64
        // but out of `int` range) would otherwise reach the VM and panic at
        // engine load. Reject it here with a diagnostic pointing at `bigint`,
        // and substitute an in-range placeholder so a failed compile can't carry
        // the bad value forward.
        if let baml_base::Literal::Int(v) = lit
            && !(crate::INT_MIN..=crate::INT_MAX).contains(v)
        {
            self.context.report(
                TirTypeError::IntegerLiteralOutOfRange { value: *v },
                expr_id,
                Vec::new(),
            );
            return Ty::Literal(
                baml_base::Literal::Int(0),
                Freshness::Fresh,
                TyAttr::default(),
            );
        }
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
            // `$id` is the runtime-identity special form: it types as
            // `string` here, and MIR lowers the read to `baml.id.current()`
            // (lower.rs `lower_path`) and `$id = e` to `baml.id.set(e)`
            // (lower.rs `AstStmt::Assign`). Keep the three sites in sync.
            if name.as_str() == "$id" {
                return Ty::String {
                    attr: TyAttr::default(),
                };
            }
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
                let error = if self
                    .body_source_map
                    .as_ref()
                    .is_some_and(|source_map| source_map.is_property_shorthand_expr(expr_id))
                {
                    TirTypeError::UnresolvedPropertyShorthand {
                        name: name.clone(),
                        suggestions: Self::similar_name_suggestions(name, self.locals.keys()),
                    }
                } else {
                    TirTypeError::UnresolvedName { name: name.clone() }
                };
                self.context.report_simple(error, expr_id);
            }
            ty
        } else if segments.len() >= 2 {
            // Member access rooted at `$id` (`$id.len()`): `$id` is a value,
            // not a binding, and the member machinery below would report a
            // misleading "unresolved name: $id". Give the targeted fix.
            if segments[0].as_str() == "$id" {
                self.context.report_simple(
                    TirTypeError::RuntimeIdMemberAccess {
                        member: segments[1].clone(),
                    },
                    expr_id,
                );
                return Ty::Unknown {
                    attr: TyAttr::default(),
                };
            }
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
                // 1. Package path (e.g. ai.Prompt, baml.env.get)
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
                    // Narrow the span to the offending root segment (`o`), not
                    // the whole `o.value` access (B-539).
                    self.context.report_at_segment(
                        TirTypeError::UnresolvedName {
                            name: segments[0].clone(),
                        },
                        expr_id,
                        0,
                        Vec::new(),
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

    /// Resolve a multi-segment path like `baml.http.fetch` or `root.sys.panic`.
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
            self.resolutions.insert(
                expr_id,
                crate::inference::MemberResolution::Free { func_loc },
            );
            // Signature diags are reported at the definition site, not here.
            let sig = crate::callable::function_signature_ty(db, func_loc);
            return Some(
                sig.to_function_ty(crate::callable::callable_throws(db, func_loc).clone()),
            );
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
                match self.lookup_class_method(&class_qtn, &[], method_name) {
                    ClassMethodLookup::Found {
                        ty,
                        class_loc,
                        func_loc,
                    } => {
                        self.resolutions.insert(
                            expr_id,
                            crate::inference::MemberResolution::UnboundMethod {
                                class_loc,
                                func_loc,
                            },
                        );
                        return Some(ty);
                    }
                    ClassMethodLookup::DuplicateInherent => {
                        return Some(Ty::Error {
                            attr: TyAttr::default(),
                        });
                    }
                    ClassMethodLookup::DeferToInterfaces | ClassMethodLookup::NotFound => {}
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
                    let db = self.context.db();
                    // Note: diags from referenced function signatures are not
                    // reported here — they'll be reported at the definition site.
                    let sig = crate::callable::function_signature_ty(db, func_loc);
                    sig.to_function_ty(crate::callable::callable_throws(db, func_loc).clone())
                }
                // A top-level `let` value reference — including the synthetic
                // binding that a `client<llm> ...` declaration lowers to — has
                // the type of its inferred initializer. Resolve it through the
                // let's own scope inference so the reference doesn't fall through
                // to `Ty::Unknown` (which would trip the runtime lowering
                // boundary when the value is used, e.g. a client passed to
                // `stream_llm_function`).
                Definition::Let(let_loc) => {
                    let db = self.context.db();
                    let Some(scope_id) = baml_compiler2_ppir::item_data::let_scope(db, let_loc)
                    else {
                        return Ty::Unknown {
                            attr: TyAttr::default(),
                        };
                    };
                    let inference = crate::inference::infer_scope_types(db, scope_id);
                    let body = baml_compiler2_hir::body::let_body(db, let_loc);
                    if let baml_compiler2_hir::body::LetBody::Expr(expr_body) = body.as_ref()
                        && let Some(root) = expr_body.root_expr
                        && let Some(ty) = inference.expression_type(root)
                    {
                        // Freeze evolving empties the same way a local reference
                        // does — a referenced binding is no longer open.
                        match ty {
                            Ty::EvolvingList(inner, attr) => Ty::List(inner.clone(), attr.clone()),
                            Ty::EvolvingMap(k, v, attr) => Ty::Map {
                                key: k.clone(),
                                value: v.clone(),
                                attr: attr.clone(),
                            },
                            other => other.clone(),
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

    /// BEP-049 §10 (M4d.3): validate a tagged-template tag that resolved to a
    /// function type, and return the template's type (the tag fn's return type
    /// on success, `Unknown` on error).
    ///
    /// `params`/`ret` are the tag function's already-lowered signature. The
    /// tag must (1) carry a `//baml:tagged_string` marker and (2) declare a
    /// first parameter `body: (...) -> baml.TaggedString`. Diagnostics point
    /// at the tag expression, with a secondary note at the function/param def.
    fn validate_tagged_tag(
        &self,
        tag_expr: ExprId,
        tag_name: &Name,
        params: &[FunctionParamTy],
        ret: &Ty,
    ) -> Ty {
        let db = self.context.db();

        // Reach the marker flag via the recorded free-function resolution.
        let func_loc = match self.resolutions.get(&tag_expr) {
            Some(crate::inference::MemberResolution::Free { func_loc }) => Some(*func_loc),
            _ => None,
        };
        let is_tagged = func_loc.is_some_and(|fl| {
            baml_compiler2_ppir::item_data::function_data(db, fl).is_tagged_template_tag
        });

        // (c) resolves to a function, but it isn't a tagged-string tag.
        if !is_tagged {
            let related = func_loc
                .map(|fl| {
                    vec![RelatedNote::new(
                        RelatedLocation::Item(Definition::Function(fl)),
                        "add a `//baml:tagged_string` marker comment above this function",
                    )]
                })
                .unwrap_or_default();
            self.context.report(
                TirTypeError::TaggedTagNotMarked {
                    name: tag_name.clone(),
                },
                tag_expr,
                related,
            );
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }

        // (d) marked, but the first parameter must be a well-formed
        //     `body: (...) -> baml.TaggedString`.
        let body_param_ok = params.first().is_some_and(|p| {
            let name_ok = p.name.as_ref().is_some_and(|n| n.as_str() == "body");
            let ret_ok = matches!(
                &p.ty,
                Ty::Function { ret: body_ret, .. }
                    if matches!(
                        body_ret.as_ref(),
                        Ty::Class(qtn, _, _) if qtn.is_builtin_root_type("TaggedString")
                    )
            );
            name_ok && ret_ok
        });
        if !body_param_ok {
            let related = func_loc
                .map(|fl| {
                    vec![RelatedNote::new(
                        RelatedLocation::Param(fl, 0),
                        "the first parameter must be `body: (...) -> baml.TaggedString`",
                    )]
                })
                .unwrap_or_default();
            self.context.report(
                TirTypeError::TaggedTagBadBodyParam {
                    name: tag_name.clone(),
                },
                tag_expr,
                related,
            );
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }

        // Valid tag: the template evaluates to the tag fn's return type.
        ret.clone()
    }

    /// BEP §11 strict-interpolation check for an untagged template. Recurses
    /// the structured `segments` and, for each `${expr}`, inspects the type
    /// already recorded by typing the desugared `elaborated` tree — reporting a
    /// purpose-built error on the interp's own span (no synthetic `.to_string()`
    /// call for the error to leak through). Read-only: it never re-infers, so
    /// it can't double-report diagnostics from inside the interpolations.
    fn check_template_interps_stringable(
        &mut self,
        segments: &[ast::TemplateSegment],
        body: &ExprBody,
    ) {
        for seg in segments {
            match seg {
                ast::TemplateSegment::Text(_) => {}
                ast::TemplateSegment::Interp(expr_id) => {
                    self.check_interp_stringable(*expr_id, body);
                }
                ast::TemplateSegment::For { body: inner, .. }
                | ast::TemplateSegment::CStyleFor { body: inner, .. } => {
                    self.check_template_interps_stringable(inner, body);
                }
                ast::TemplateSegment::If {
                    branches,
                    else_body,
                } => {
                    for branch in branches {
                        self.check_template_interps_stringable(&branch.body, body);
                    }
                    if let Some(eb) = else_body {
                        self.check_template_interps_stringable(eb, body);
                    }
                }
            }
        }
    }

    /// Is `expr` a syntactically unit-valued (`Ty::Void`) block tail? An
    /// `if`/`if let` with no `else`, or a nested block whose own tail is unit.
    /// Mirrors `elaborate_default_interp`'s predicate in the AST crate so the
    /// §11 segment check and the elaboration agree on which `${…}` render `""`.
    fn is_unit_tail(body: &ExprBody, expr: ExprId) -> bool {
        match &body.exprs[expr] {
            Expr::If {
                else_branch: None, ..
            }
            | Expr::IfLet {
                else_branch: None, ..
            } => true,
            Expr::Block { tail_expr, .. } => tail_expr
                .map(|t| Self::is_unit_tail(body, t))
                .unwrap_or(true),
            _ => false,
        }
    }

    /// The per-`${expr}` half of [`check_template_interps_stringable`]. Reads
    /// the recorded type (from typing the elaborated tree) and requires it to
    /// be non-null and expose a `to_string` method.
    fn check_interp_stringable(&mut self, expr_id: ExprId, body: &ExprBody) {
        let Some(ty) = self.expressions.get(&expr_id).cloned() else {
            return;
        };
        // Unknown/Error already produced their own diagnostics upstream.
        if matches!(ty, Ty::Unknown { .. } | Ty::Error { .. }) {
            return;
        }
        // A unit-valued block (`${ let x = 1 }`, or one whose tail is an
        // `if`/`if let` with no `else`) renders as the empty string (BEP §4) —
        // no `to_string` required. Mirrors the widened unit-tail detection in
        // the untagged-template elaborator (`elaborate_default_interp`); the
        // recorded type here is the ORIGINAL segment's, typed independently of
        // the elaborated `""`, so it must apply the same predicate.
        if let Expr::Block { tail_expr, .. } = &body.exprs[expr_id] {
            let tail_is_unit = tail_expr
                .map(|t| Self::is_unit_tail(body, t))
                .unwrap_or(true);
            if tail_is_unit {
                return;
            }
        }
        // Nullable values can't be stringified directly — the user must
        // coalesce (`${x ?? "…"}`) or unwrap first. Applies the §7-style
        // "surface null bugs at type-check time" rule to interpolation.
        let inner = crate::narrowing::remove_null(&ty);
        if inner != ty {
            self.context
                .report_simple(TirTypeError::InterpolatedValueMaybeNull { ty }, expr_id);
        }
        // No `to_string` requirement: a non-null value renders via
        // `string.from(...)` (BEP-049 §11), which is total — it dispatches the
        // `baml.ToString` override when the value's runtime class implements it
        // and otherwise falls back to a structural rendering, so every type is
        // interpolatable.
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
                let field_ty = match self.lookup_class_field(class_name, type_args, member) {
                    ClassFieldLookup::Found(ty) => Some(ty),
                    ClassFieldLookup::Duplicate => {
                        return Ty::Error {
                            attr: TyAttr::default(),
                        };
                    }
                    ClassFieldLookup::NotFound => None,
                };
                if let Some(field_ty) = field_ty {
                    if self.class_has_inherent_method(class_name, member) {
                        return Ty::Error {
                            attr: TyAttr::default(),
                        };
                    }
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
                    return field_ty;
                }

                // Check class methods via the item tree (methods are stored
                // directly on the Class entry, not in the package namespace).
                let class_method = match self.lookup_class_method(class_name, type_args, member) {
                    ClassMethodLookup::Found {
                        ty,
                        class_loc,
                        func_loc,
                    } => Some((ty, class_loc, func_loc)),
                    ClassMethodLookup::DuplicateInherent => {
                        return Ty::Error {
                            attr: TyAttr::default(),
                        };
                    }
                    ClassMethodLookup::DeferToInterfaces | ClassMethodLookup::NotFound => None,
                };
                if let Some((ty, class_loc, func_loc)) = class_method {
                    if bound {
                        // Record the receiver's class type args (owner class
                        // generics → concrete args) keyed by this callee, so the
                        // generic-bound check can resolve a method bound that
                        // references a class param (`<U extends Eq<C>>` on
                        // `class Box<C>`). The callable type substituted these out
                        // of its signature, so they are otherwise absent from the
                        // call-site bindings; this mirrors the interface-default
                        // path. (The bound-method VM frame expects only method
                        // params, so the extra entries are inert for runtime args.)
                        let owner_bindings: Vec<(crate::ty::ParamTy, Ty)> = {
                            let db = self.context.db();
                            crate::generic_env::class_generic_env(db, class_loc)
                                .params()
                                .iter()
                                .cloned()
                                .zip(type_args.iter().cloned())
                                .collect()
                        };
                        if !owner_bindings.is_empty() {
                            self.owner_type_arg_binding_seed.insert(at, owner_bindings);
                        }
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

                // Interface members come from the class's own impl blocks (rustc-style
                // extension resolution) — consulted only after inherent class field/method
                // lookup misses above, so an inherent member always wins.
                if let Some(ty) = self.resolve_member_from_impls(
                    base_ty,
                    class_name.name().clone(),
                    member,
                    at,
                    bound,
                ) {
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
            Ty::Interface(iface_name, type_args, associated_bindings, _) => {
                if let Some(ty) = self.resolve_interface_member(
                    InterfaceBound {
                        name: iface_name,
                        type_args,
                        associated_bindings,
                    },
                    SelfReceiver::Existential(base_ty),
                    MemberAccess { member, at, bound },
                ) {
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
                // `to_json` is no longer a magic enum method — `enum.to_json()`
                // desugars to `baml.json.from(enum)` (rendered as the variant name
                // by the walker). Left unresolved here so the sugar fires.

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
                        self.resolve_member_from_impls(
                            base_ty,
                            Name::new("array"),
                            member,
                            at,
                            bound,
                        )
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
                .or_else(|| {
                    self.resolve_member_from_impls(base_ty, Name::new("map"), member, at, bound)
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
            Ty::Future(value_ty, error_ty, _) => {
                // Bridge: Future<T, E> → baml.future.Future class
                self.resolve_builtin_member(
                    &["future", "Future"],
                    &[value_ty.as_ref().clone(), error_ty.as_ref().clone()],
                    member,
                    at,
                )
                .or_else(|| {
                    self.resolve_member_from_impls(base_ty, Name::new("future"), member, at, bound)
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
                        self.resolve_member_from_impls(
                            base_ty,
                            Name::new("string"),
                            member,
                            at,
                            bound,
                        )
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
                .or_else(|| {
                    self.resolve_member_from_impls(base_ty, Name::new("int"), member, at, bound)
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
            // bigint literal / bigint → Bigint companion class
            Ty::Bigint { .. } | Ty::Literal(baml_base::Literal::Bigint(_), _, _) => self
                .resolve_builtin_member(&["Bigint"], &[], member, at)
                .or_else(|| {
                    self.resolve_member_from_impls(base_ty, Name::new("bigint"), member, at, bound)
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
                    self.resolve_member_from_impls(base_ty, Name::new("float"), member, at, bound)
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
                .or_else(|| {
                    self.resolve_member_from_impls(base_ty, Name::new("bool"), member, at, bound)
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
            // null / Null companion class
            Ty::Null { .. } => self
                .resolve_builtin_member(&["Null"], &[], member, at)
                .or_else(|| {
                    self.resolve_member_from_impls(base_ty, Name::new("null"), member, at, bound)
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
            Ty::Type { .. } => {
                // Bridge: type → TypeValue companion class (provides `.to_string()`, etc.)
                self.resolve_builtin_member(&["TypeValue"], &[], member, at)
                    .or_else(|| {
                        self.resolve_member_from_impls(
                            base_ty,
                            Name::new("type"),
                            member,
                            at,
                            bound,
                        )
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
                        self.resolve_member_from_impls(
                            base_ty,
                            Name::new(p.alias()),
                            member,
                            at,
                            bound,
                        )
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
            // signature directly. (`to_json` is no longer universal — it desugars
            // to `baml.json.from`; only `from_json` remains a synthesized builtin.)
            // BEP-044 generic bound: when `T extends I` is in scope and the member
            // isn't the synthesized builtin `from_json`, delegate to `I`'s contract
            // — including `to_json` when `T extends baml.ToJson`.
            Ty::TypeVar(name, _)
                if member.as_str() != "from_json"
                    && self
                        .generic_param_bounds
                        .get(name)
                        .is_some_and(|bounds| !bounds.is_empty()) =>
            {
                let bounds = self.generic_param_bounds[name].clone();
                // The receiver is a single concrete type (the type variable),
                // so an interface-bound member resolves with `Self` pinned to
                // that variable — `Self`-typed parameters are sound here. This
                // is what lets a generic `T extends Equals` (and an interface's
                // own `self`) call `Self`-param methods, while a bare interface
                // (existential) receiver still cannot. Each bound of an
                // intersection (`T: A & B`) is tried in turn.
                if let Some(ty) = self.resolve_interface_member_over_conjunction(
                    &bounds,
                    SelfReceiver::RigidVar(name),
                    MemberAccess { member, at, bound },
                ) {
                    return ty;
                }
                // No conjunct declares the member. Reporting it here — rather than
                // re-resolving against one arbitrarily chosen conjunct's existential
                // view — keeps the answer independent of the order the bounds were
                // written, and matches the projection arm below.
                self.context.report_at_member_simple(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                );
                Ty::Error {
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
                    throws: Box::new(json_decode_error_ty()),
                    attr: TyAttr::default(),
                }
            }
            Ty::AssociatedTypeProjection {
                interface: projection_iface,
                member: assoc,
                ..
            } if member.as_str() != "from_json" => {
                // A projection over a determinable base IS its realization (`(Risky as
                // HasErr).E` with `type E = Kaboom` *is* `Kaboom`) — reduce first and
                // resolve the member on the reduced type. Only a still-opaque
                // (symbolic-base) projection dispatches through its declared bound
                // below. Terminates: `normalize` is idempotent, so a second visit
                // compares equal and falls through.
                let reduced = self.normalize(base_ty);
                if reduced != *base_ty && !matches!(reduced, Ty::Error { .. } | Ty::Unknown { .. })
                {
                    return self.resolve_member(&reduced, member, at, bound);
                }
                // A value of an associated-type projection type (`(base as I).Assoc`)
                // is rigid but abstract, like a bounded type variable: it dispatches
                // members through the projection's declared bound (`type Assoc extends
                // J`), with `Self` pinned to the projection itself (`ExactTy`). Mirrors
                // the bounded `Ty::TypeVar` arm above; the declared bound is the same
                // oracle that powers the `(base as I).Assoc <: J` subtype rule.
                let bounds = crate::builder::associated_projection::associated_type_declared_bound(
                    self.context.db(),
                    projection_iface,
                    assoc,
                );
                if let Some(ty) = self.resolve_interface_member_over_conjunction(
                    &bounds,
                    SelfReceiver::ExactTy(base_ty),
                    MemberAccess { member, at, bound },
                ) {
                    return ty;
                }
                // No declared bound resolves the member — an unbounded (or wrongly
                // bounded) projection has no members, same as any receiver missing it.
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
            Ty::Union(members, _) => {
                // A union resolves a member through the single interface shared by every arm
                // that declares it — sugar for `union.as<I>.member` (a virtual access/call).
                let members = members.clone();
                match self.resolve_union_member(base_ty, &members, member, at, bound) {
                    UnionMemberResolution::Resolved(ty) => ty,
                    UnionMemberResolution::Unresolved(err)
                    | UnionMemberResolution::Ambiguous(err) => {
                        self.context.report_at_member(err, at, Vec::new());
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
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
            Ty::Unknown { .. } | Ty::Error { .. } => {
                // Base type unknown or already errored — can't resolve the
                // member, but don't emit an error: the base type's own failure
                // was already reported upstream, and a `!error` receiver must
                // not cascade a second "has no member" diagnostic on top of it.
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
        let completed = crate::interfaces::interface_closure_locs_with_args_and_assoc(
            db,
            iface_loc,
            &iface_args,
            &associated_bindings,
            true,
        )
        .into_iter()
        .next()
        .map(|(_, _, assoc)| assoc)
        .unwrap_or_else(|| associated_bindings.clone());
        Ty::Interface(iface_qtn, iface_args, completed, attr)
    }

    /// Whether a realized interface view's args are consistent with the formal
    /// the caller asked for. A formal arg that mentions a type variable is a
    /// wildcard — the caller's binding inference resolves it — while a ground
    /// formal arg must match exactly. This is what lets a concrete
    /// `Codec<TextFormat>` formal select only the `Codec<TextFormat>` view when
    /// the receiver also implements `Codec<CodeFormat>`, instead of matching both
    /// on the shared `Codec` head and collapsing to an ambiguous None.
    fn interface_view_args_match_formal(&self, formal_args: &[Ty], view_args: &[Ty]) -> bool {
        formal_args.len() == view_args.len()
            && formal_args
                .iter()
                .zip(view_args)
                .all(|(f, v)| crate::generics::contains_typevar(f) || self.equivalent(f, v))
    }

    fn interface_view_in_requires_closure(
        &self,
        root_interface_ty: &Ty,
        target_qtn: &crate::ty::QualifiedTypeName,
        target_args: &[Ty],
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
        for (iface_loc, iface_args, iface_associated_bindings) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                db,
                root_loc,
                root_args,
                root_associated_bindings,
                true,
            )
        {
            let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            let iface_qtn = crate::lower_type_expr::qualify_def(
                db,
                Definition::Interface(iface_loc),
                &iface_data.name,
            );
            if &iface_qtn == target_qtn
                && self.interface_view_args_match_formal(target_args, &iface_args)
            {
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
            Some(existing) => self.equivalent(existing, &candidate),
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
        let Ty::Interface(formal_qtn, formal_args, _, _) = &formal_ty else {
            return None;
        };
        let actual_ty = self.expand_alias_chains(actual_ty.clone());
        let mut candidate = None;

        // The actual may itself be an interface / type-var whose `requires` closure reaches the
        // formal interface.
        if let Some(view) =
            self.interface_view_in_requires_closure(&actual_ty, formal_qtn, formal_args)
            && !self.merge_interface_inference_candidate(&mut candidate, view)
        {
            return None;
        }

        // A concrete actual's interfaces come from its own impls. `type_impls` matches the impl
        // head and discharges its bounds, so each `implemented_interface` is the realized view
        // (`ArrayIterator<int>` provides `Iterator<int, never>`) — no manual pattern-match or
        // `implements_interface` recheck needed. Keep the one whose `requires` closure reaches
        // the formal interface.
        let db = self.context.db();
        for resolved_impl in self.type_impls(&actual_ty) {
            let implemented = resolved_impl.implemented_interface(db).to_ty();
            let Some(view) =
                self.interface_view_in_requires_closure(&implemented, formal_qtn, formal_args)
            else {
                continue;
            };
            if !self.merge_interface_inference_candidate(&mut candidate, view) {
                return None;
            }
        }

        candidate
    }

    fn infer_call_bindings_rigid_self(
        &self,
        formal: &Ty,
        actual: &Ty,
        bindings: &mut FxHashMap<crate::ty::ParamTy, Ty>,
        rigid: Option<&crate::ty::ParamTy>,
    ) {
        crate::generics::infer_bindings_rigid_self(formal, actual, bindings, rigid);
        self.infer_call_bindings_via_interface_views_rigid(formal, actual, bindings, rigid);
    }

    fn infer_call_bindings_allow_typevars(
        &self,
        formal: &Ty,
        actual: &Ty,
        bindings: &mut FxHashMap<crate::ty::ParamTy, Ty>,
    ) {
        crate::generics::infer_bindings_allow_typevars(formal, actual, bindings);
        self.infer_call_bindings_via_interface_views_allow_typevars(formal, actual, bindings);
    }

    fn infer_call_bindings_via_interface_views_rigid(
        &self,
        formal: &Ty,
        actual: &Ty,
        bindings: &mut FxHashMap<crate::ty::ParamTy, Ty>,
        rigid: Option<&crate::ty::ParamTy>,
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
        bindings: &mut FxHashMap<crate::ty::ParamTy, Ty>,
    ) {
        if let Some(view) = self.actual_interface_view_for_formal(formal, actual) {
            crate::generics::infer_bindings_allow_typevars(formal, &view, bindings);
            self.infer_call_bindings_via_matching_shape(formal, &view, bindings, None, true);
        }
        self.infer_call_bindings_via_matching_shape(formal, actual, bindings, None, true);
    }

    /// The container type a list/map literal in checking position adopts from
    /// `expected`, or `None` when `expected` does not determine one.
    ///
    /// Containers are invariant, so a container literal is checked
    /// *bidirectionally* — each element against the declared element type —
    /// rather than synthesized and subtype-checked, which would wrongly reject
    /// `{"a": 1}` against `map<string, json>` (`map<string, int>` is not a
    /// subtype of it under invariance). The expected type is alias-expanded at
    /// the top level (`json` → `… | json[] | map<string, json>`); a nullable
    /// wrapper or a wider union determines the container only when exactly one
    /// distinct member (itself alias-expanded, nested unions flattened) is of
    /// the literal's kind — with two or more the adoption target is ambiguous,
    /// so the caller falls back to synthesize + subtype.
    fn adopted_container_for_literal(
        &self,
        expected: &Ty,
        kind: ContainerLiteralKind,
    ) -> Option<Ty> {
        // Top-level expansion only: the nested occurrences inside a recursive
        // alias like `json` stay symbolic, which is what terminates this.
        let expanded = self.expand_alias_chains(expected.clone());
        if kind.matches(&expanded) {
            return Some(expanded);
        }
        let Ty::Union(members, _) = expanded else {
            return None;
        };
        // Collect kind-matching members across nested unions (a `json` member of
        // a wider union contributes its own `json[]` / `map<string, json>`).
        // Fuel-bounded so a pathological self-referential union alias cannot spin.
        let mut worklist: Vec<Ty> = members;
        let mut adopted: Option<Ty> = None;
        let mut fuel = 64usize;
        while let Some(member) = worklist.pop() {
            if fuel == 0 {
                return None;
            }
            fuel -= 1;
            let member = self.expand_alias_chains(member);
            if kind.matches(&member) {
                if adopted.as_ref().is_some_and(|already| already != &member) {
                    return None;
                }
                adopted = Some(member);
            } else if let Ty::Union(nested, _) = member {
                worklist.extend(nested);
            }
        }
        adopted
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
        bindings: &mut FxHashMap<crate::ty::ParamTy, Ty>,
        rigid: Option<&crate::ty::ParamTy>,
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
            // A class's own impl blocks (interface members), its class members, and the
            // associated diagnostics are all handled by `resolve_member`, which now routes
            // concrete receivers through their impls. (Diagnostics land at the path expr
            // rather than the precise segment token — acceptable.)
            Ty::Class(..) => self.resolve_member(base_ty, member, path_id, bound),
            Ty::Interface(iface_name, type_args, associated_bindings, _) => {
                if let Some(ty) = self.resolve_interface_member(
                    InterfaceBound {
                        name: iface_name,
                        type_args,
                        associated_bindings,
                    },
                    SelfReceiver::Existential(base_ty),
                    MemberAccess {
                        member,
                        at: path_id,
                        bound,
                    },
                ) {
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
                // `to_json` is no longer a magic enum method (path-segment form);
                // `enum.to_json()` desugars to `baml.json.from(enum)`. Left
                // unresolved so the sugar fires.

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
                // A union resolves a member through the single interface shared by every arm
                // that declares it (`union.as<I>.member`). `report_at_segment` points the span
                // at the segment token rather than the whole path.
                let members = members.clone();
                match self.resolve_union_member(base_ty, &members, member, path_id, bound) {
                    UnionMemberResolution::Resolved(ty) => ty,
                    UnionMemberResolution::Unresolved(err)
                    | UnionMemberResolution::Ambiguous(err) => {
                        self.context
                            .report_at_segment(err, path_id, seg_idx, Vec::new());
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    }
                }
            }
            Ty::Unknown { .. } => {
                // Base type unknown — don't emit another error.
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            Ty::BuiltinUnknown { .. } => {
                let receiver_seg_idx = seg_idx.saturating_sub(1);
                self.context.report_at_segment(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    path_id,
                    seg_idx,
                    vec![RelatedNote::new(
                        RelatedLocation::ExprSegment(path_id, receiver_seg_idx),
                        "this value has type `unknown`",
                    )],
                );
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
        for iface_loc in crate::interfaces::interface_closure_locs(db, root_loc) {
            let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            if iface_data
                .required_methods
                .iter()
                .any(|s| s.name == *method_name)
            {
                return true;
            }
            if iface_data.default_methods.iter().any(|&fn_loc| {
                baml_compiler2_ppir::item_data::function_data(db, fn_loc).name == *method_name
            }) {
                return false;
            }
        }
        false
    }

    /// Whether `id` is *exactly* the bare `Self` path — the sole shape the
    /// `Self.Assoc` exemption in [`Self::type_ref_contains_bare_self`] keys on.
    fn type_ref_is_bare_self(
        store: &baml_compiler2_hir::type_ref::TypeRefStore,
        id: baml_compiler2_hir::type_ref::TypeRefId,
    ) -> bool {
        use baml_compiler2_hir::type_ref::TypeRefKind;
        matches!(
            &store[id].kind,
            TypeRefKind::Path { segments, .. }
                if segments.len() == 1 && segments[0].as_str() == "Self"
        )
    }

    /// Whether `id` references the *bare* `Self` type anywhere — a path of exactly
    /// `[Self]`, recursing structurally. A `Self.Assoc` projection is NOT bare `Self`:
    /// on an existential receiver every associated type is pinned (or defaulted), so the
    /// projection denotes one type shared by every member of the existential — none of
    /// the `Self`-identity problems apply. Both object safety and the interface-field
    /// ban (E0136) key on this: a `value: Self.Item` field is legal (it denotes the
    /// implementor's bound associated type), a `value: Self` field is not.
    ///
    /// That exemption is justified by the *pins*, so it extends only to a base of exactly
    /// `Self` (or a chain rooted at one, `Self.Item.Sub`). Any other base wraps `Self` in
    /// a fresh constructor the pins say nothing about — `(Self[] as I).Item` reduces
    /// straight back to `Self` — so such a base is inspected like any other type. The
    /// qualifying interface and any associated-type *bindings* are inspected for the same
    /// reason: `I<Item = Self>` pins a member to bare `Self`.
    pub(crate) fn type_ref_contains_bare_self(
        store: &baml_compiler2_hir::type_ref::TypeRefStore,
        id: baml_compiler2_hir::type_ref::TypeRefId,
    ) -> bool {
        use baml_compiler2_hir::type_ref::TypeRefKind;
        match &store[id].kind {
            TypeRefKind::Path {
                generic_args,
                associated_type_bindings,
                ..
            } => {
                Self::type_ref_is_bare_self(store, id)
                    || generic_args
                        .iter()
                        .any(|&arg| Self::type_ref_contains_bare_self(store, arg))
                    || associated_type_bindings
                        .iter()
                        .any(|binding| Self::type_ref_contains_bare_self(store, binding.ty))
            }
            TypeRefKind::AssociatedTypeProjection {
                base, interface, ..
            } => {
                (!Self::type_ref_is_bare_self(store, *base)
                    && Self::type_ref_contains_bare_self(store, *base))
                    || (*interface)
                        .is_some_and(|iface| Self::type_ref_contains_bare_self(store, iface))
            }
            TypeRefKind::List { inner } | TypeRefKind::Optional { inner } => {
                Self::type_ref_contains_bare_self(store, *inner)
            }
            TypeRefKind::Map { key, value } => {
                Self::type_ref_contains_bare_self(store, *key)
                    || Self::type_ref_contains_bare_self(store, *value)
            }
            TypeRefKind::Union { variants } => variants
                .iter()
                .any(|&v| Self::type_ref_contains_bare_self(store, v)),
            TypeRefKind::Function {
                params,
                ret,
                throws,
            } => {
                params
                    .iter()
                    .any(|param| Self::type_ref_contains_bare_self(store, param.ty))
                    || Self::type_ref_contains_bare_self(store, *ret)
                    || throws.is_some_and(|throws| Self::type_ref_contains_bare_self(store, throws))
            }
            _ => false,
        }
    }

    /// Whether `id` references bare `Self` in an **invariant** position — inside a generic
    /// argument, a list/map element, or a function type. Such a `Self` is unsound to
    /// call through an interface-existential receiver: the impl returns a
    /// concretely-tagged container (`Concrete[]`) that is NOT a subtype of the
    /// existential-tagged one (`dyn I[]`), because containers are invariant.
    ///
    /// A *bare* top-level `Self` — or one reachable only through covariant wrappers
    /// (`Self?`, `A | Self`) — is NOT flagged: it collapses covariantly to the receiver
    /// (`dyn I`), which the impl's concrete return subtypes nominally. A `Self.Assoc`
    /// projection is never flagged, even nested (`Self.Item[]`): the existential's pins
    /// make it one concrete type for every member (see
    /// [`Self::type_ref_contains_bare_self`]). Used for a method's return/throws type;
    /// parameters use the stricter any-position bare-`Self` check (the multi-`Self`
    /// problem applies in every parameter position).
    pub(crate) fn type_ref_self_in_invariant_position(
        store: &baml_compiler2_hir::type_ref::TypeRefStore,
        id: baml_compiler2_hir::type_ref::TypeRefId,
    ) -> bool {
        use baml_compiler2_hir::type_ref::TypeRefKind;
        match &store[id].kind {
            // A class/generic head is invariant in its arguments; a bare `Self` or a
            // `Self.Assoc` projection (the segments) is not itself an invariant nesting.
            TypeRefKind::Path { generic_args, .. } => generic_args
                .iter()
                .any(|&arg| Self::type_ref_contains_bare_self(store, arg)),
            // Invariant containers: any bare `Self` inside is unsound.
            TypeRefKind::List { inner } => Self::type_ref_contains_bare_self(store, *inner),
            TypeRefKind::Map { key, value } => {
                Self::type_ref_contains_bare_self(store, *key)
                    || Self::type_ref_contains_bare_self(store, *value)
            }
            TypeRefKind::Function {
                params,
                ret,
                throws,
            } => {
                params
                    .iter()
                    .any(|param| Self::type_ref_contains_bare_self(store, param.ty))
                    || Self::type_ref_contains_bare_self(store, *ret)
                    || throws.is_some_and(|throws| Self::type_ref_contains_bare_self(store, throws))
            }
            // Covariant-transparent wrappers: recurse, so a bare `Self` under them is
            // fine but a `Self[]` under them is not.
            TypeRefKind::Optional { inner } => {
                Self::type_ref_self_in_invariant_position(store, *inner)
            }
            TypeRefKind::Union { variants } => variants
                .iter()
                .any(|&v| Self::type_ref_self_in_invariant_position(store, v)),
            _ => false,
        }
    }

    /// Look up a class field from the package items (via item tree).
    ///
    /// `class_type_args` are the concrete type arguments for the class (e.g.
    /// `[Sentiment$stream, Sentiment]` for `Stream<Sentiment$stream, Sentiment>`).
    /// When non-empty, field types are resolved with the binding keys in scope and then substituted
    /// so that type variables like `TStream` and `TFinal` are substituted with concrete types.
    ///
    fn lookup_class_field(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
        field_name: &Name,
    ) -> ClassFieldLookup {
        let mut matches = self
            .class_all_fields_ordered(class_name, class_type_args, true)
            .into_iter()
            .filter(|(name, _)| name == field_name);
        let Some((_, ty)) = matches.next() else {
            return ClassFieldLookup::NotFound;
        };
        if matches.next().is_some() {
            ClassFieldLookup::Duplicate
        } else {
            ClassFieldLookup::Found(ty)
        }
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
        let class_generic_env = crate::generic_env::class_generic_env(db, class_loc);

        let resolved = crate::inference::resolve_class_fields(db, class_loc);

        // Build bindings from declared generic params → concrete type args.
        let bindings = crate::generics::bind_type_vars(class_generic_env.params(), class_type_args);

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

    /// The class field that an interface field (`field_name` on the realized interface
    /// `target_iface_name<target_iface_args>`) links to, via the class's own impl of that
    /// interface (`ImplData.field_links`, defaulting to the same name). Side-effect-free
    /// counterpart to `qualified_interface_field_for_construction`.
    fn class_field_name_for_interface_field(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
        target_iface_name: &crate::ty::QualifiedTypeName,
        target_iface_args: &[Ty],
        field_name: &Name,
    ) -> Option<Name> {
        let concrete = Ty::Class(
            class_name.clone(),
            class_type_args.to_vec(),
            TyAttr::default(),
        );
        let impls = self.type_impls(&concrete);
        self.concrete_interface_field_sources(&impls, field_name)
            .into_iter()
            .find(|source| {
                &source.interface.name == target_iface_name
                    && source.interface.generics.len() == target_iface_args.len()
                    && source
                        .interface
                        .generics
                        .iter()
                        .zip(target_iface_args)
                        .all(|(a, b)| self.equivalent(a, b))
            })
            .map(|source| source.class_field)
    }

    /// Resolve a constructor field `field_name` to the `(class field, declared type)` it
    /// denotes through an interface the class implements: the interface declares the field,
    /// `ImplData.field_links` maps it to a class field, and the type is the interface's
    /// declaration with its generic args applied. `None` when no implemented interface
    /// declares `field_name`.
    fn qualified_interface_field_for_construction(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
        field_name: &Name,
    ) -> Option<(Name, Ty)> {
        let concrete = Ty::Class(
            class_name.clone(),
            class_type_args.to_vec(),
            TyAttr::default(),
        );
        let impls = self.type_impls(&concrete);
        let source = self
            .concrete_interface_field_sources(&impls, field_name)
            .into_iter()
            .next()?;
        // The field's declared type lives in the interface's scope, its generic params bound
        // to the realized interface args.
        let db = self.context.db();
        // `iface_loc` reads the field; `interface` supplies the realized args. Derived from one
        // interface at construction — assert they haven't drifted apart.
        debug_assert_eq!(
            crate::interfaces::interface_loc_qtn(db, source.iface_loc).as_ref(),
            Some(&source.interface.name),
            "ConcreteFieldSource.iface_loc and .interface must name the same interface",
        );
        let iface_data = baml_compiler2_ppir::item_data::interface_data(db, source.iface_loc);
        let iface_env = crate::generic_env::interface_generic_env(db, source.iface_loc);
        let (_, iface_generic_params) = iface_env.interface_param_parts();
        let field = iface_data.fields.iter().find(|f| f.name == *field_name)?;
        let iface_pkg_items = self.resolve_class_pkg_items(source.interface.name.package())?;
        let iface_ns =
            baml_compiler2_hir::file_package::file_package(db, source.iface_loc.file(db))
                .namespace_path;
        let bindings =
            crate::generics::bind_type_vars(iface_generic_params, &source.interface.generics);
        // The declaring interface's parameter bounds, so a `T.member` projection
        // in the field type resolves `T`'s declaring interface.
        let iface_bounds =
            crate::lower_type_expr::interface_generic_param_bounds(db, source.iface_loc);
        // BUG(interface-field-self-scope): both branches below lower with `self_ty: None`,
        // so a field type naming `Self`/`Self.Assoc` (`key: Self.Key`) takes the unresolved
        // path and lands as an error-recovery type instead of a `TypeVar` /
        // `AssociatedTypeProjection`. The fix is the scope emit's `build_interface_def`
        // builds: `Self` as a rigid type variable bounded by the interface itself, so a
        // projection through it resolves. Low impact here: the caller has already reported
        // `InterfaceFieldRequiresQualifiedConstruction`, and this type only feeds the
        // follow-on `check_expr` that suppresses cascading errors.
        let declared_ty = {
            let type_ref = field.type_ref;
            let mut diags = Vec::new();
            let ty = if bindings.is_empty() {
                crate::lower_type_expr::lower_type_ref(
                    &iface_data.type_refs,
                    type_ref,
                    &crate::lower_type_expr::ScopeCtx {
                        db,
                        package_items: iface_pkg_items,
                        ns_context: &iface_ns,
                        generic_params: iface_generic_params,
                        bounds: iface_bounds,
                        self_ty: None,
                    },
                    &mut diags,
                )
            } else {
                let generic_params: Vec<_> = bindings.keys().cloned().collect();
                crate::generics::substitute_ty(
                    &crate::lower_type_expr::lower_type_ref(
                        &iface_data.type_refs,
                        type_ref,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: iface_pkg_items,
                            ns_context: &iface_ns,
                            generic_params: &generic_params,
                            bounds: iface_bounds,
                            self_ty: None,
                        },
                        &mut diags,
                    ),
                    &bindings,
                )
            };
            if !diags.is_empty() {
                let span =
                    baml_compiler2_ppir::item_data::interface_source_map(db, source.iface_loc)
                        .type_refs
                        .span(type_ref);
                for diag in diags {
                    self.context.report_at_span(diag, span);
                }
            }
            ty
        };
        Some((source.class_field, declared_ty))
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

    fn class_has_inherent_method(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        method_name: &Name,
    ) -> bool {
        let Some(class_loc) = self.resolve_class_loc(class_name) else {
            return false;
        };
        let db = self.context.db();
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);

        class_data.methods.iter().copied().any(|method| {
            baml_compiler2_ppir::item_data::function_data(db, method).name == *method_name
                && baml_compiler2_ppir::item_data::method_interface_target(db, method).is_none()
        })
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
    /// When non-empty, return types are resolved with the binding keys in scope and then substituted
    /// so that type variables like `TStream` and `TFinal` are substituted with concrete types.
    fn lookup_class_method(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        class_type_args: &[Ty],
        method_name: &Name,
    ) -> ClassMethodLookup<'db> {
        let Some(pkg_items_for_class) = self.resolve_class_pkg_items(class_name.package()) else {
            return ClassMethodLookup::NotFound;
        };
        let Some(def) = pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())
        else {
            return ClassMethodLookup::NotFound;
        };
        let Definition::Class(class_loc) = def else {
            return ClassMethodLookup::NotFound;
        };
        let db = self.context.db();
        let file = class_loc.file(db);
        let ns_context = baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
        let class_generic_env = crate::generic_env::class_generic_env(db, class_loc);

        // `class_data.methods` flattens inherent methods and interface implementations.
        // Duplicate inherent methods were already diagnosed by HIR, while multiple interface
        // methods must be resolved through their realized interfaces.
        let materialized: Vec<_> = class_data
            .methods
            .iter()
            .copied()
            .filter(|&method| {
                baml_compiler2_ppir::item_data::function_data(db, method).name == *method_name
            })
            .collect();
        let inherent_count = materialized
            .iter()
            .filter(|&&method| {
                baml_compiler2_ppir::item_data::method_interface_target(db, method).is_none()
            })
            .count();
        if inherent_count > 1 {
            return ClassMethodLookup::DuplicateInherent;
        }
        if materialized.len() > 1 {
            return ClassMethodLookup::DeferToInterfaces;
        }
        // Even a *single* materialized match can be ambiguous: an impl-block override of one
        // interface's method does not shadow a *different* interface's same-named method that
        // the class inherits as an un-overridden default — that default lives only in the
        // interface's `default_methods` and is never materialized on the class. Defer when the
        // sole match is an impl-block method and another implemented interface also provides the
        // member (enumerated by `type_impls`, defaults included). A *class-level* method still
        // shadows interface methods (BEP-044), and a uniquely-provided one keeps the fast path.
        if let [only] = materialized.as_slice()
            && baml_compiler2_ppir::item_data::method_interface_target(db, *only).is_some()
        {
            let receiver_args: Vec<Ty> = if class_type_args.is_empty() {
                class_generic_env
                    .params()
                    .iter()
                    .map(|gp| Ty::TypeVar(gp.clone(), TyAttr::default()))
                    .collect()
            } else {
                class_type_args.to_vec()
            };
            let receiver =
                crate::self_type::receiver_type_for_class_at(class_name.clone(), receiver_args);
            let mut providers: Vec<baml_type::Interface> = Vec::new();
            for resolved_impl in self.type_impls(&receiver) {
                if resolved_impl.get_method(db, method_name).is_some() {
                    let realized = resolved_impl.implemented_interface(db);
                    if !providers.contains(&realized) {
                        providers.push(realized);
                        if providers.len() > 1 {
                            return ClassMethodLookup::DeferToInterfaces;
                        }
                    }
                }
            }
        }

        for &func_loc in &class_data.methods {
            let method_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
            if method_data.name == *method_name {
                // Build bindings from class-level generic params → concrete args.
                let mut bindings =
                    crate::generics::bind_type_vars(class_generic_env.params(), class_type_args);
                // Seed class-level generics as TypeVar entries when no concrete args
                // were provided (e.g., UFCS calls like `Array.length(arr)`).
                for gp in class_generic_env.params() {
                    bindings
                        .entry(gp.clone())
                        .or_insert_with(|| Ty::TypeVar(gp.clone(), TyAttr::default()));
                }
                // Seed method-level generics as TypeVar entries so they survive
                // lowering and can be resolved by call-site inference.
                let function_generic_env = crate::generic_env::function_generic_env(db, func_loc);
                for gp in function_generic_env.own_params() {
                    bindings
                        .entry(gp.clone())
                        .or_insert_with(|| Ty::TypeVar(gp.clone(), TyAttr::default()));
                }

                // Lowered under the call site's concrete type-arg bindings, so
                // this cannot reuse the declaration-site
                // `callable::function_signature_ty` — that query's result has
                // the declaration's rigid type variables, not this call's
                // instantiation.
                let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, func_loc);
                let mut diags = Vec::new();

                // Build the self type WITH concrete type args, or TypeVars for
                // unbound generics (UFCS case). The builtin container roots are
                // their structural sugar (`baml.Array<T>` → `T[]`), matching the
                // shape their values actually have — a static-form call
                // (`baml.Array.length(arr)`) infers `T` by shape-matching the
                // `Ty::List` argument against this formal.
                let class_ty_args: Vec<Ty> = if class_type_args.is_empty() {
                    class_generic_env
                        .params()
                        .iter()
                        .map(|gp| Ty::TypeVar(gp.clone(), TyAttr::default()))
                        .collect()
                } else {
                    class_type_args.to_vec()
                };
                let class_ty =
                    crate::self_type::receiver_type_for_class_at(class_name.clone(), class_ty_args);

                // All generic params in scope for lowering (class + method).
                let all_generic_params = function_generic_env.source_params();

                // The method's in-scope type-variable bounds (class + method params), so a
                // `T.member` projection in the signature can resolve `T`'s declaring interface.
                let method_bounds =
                    crate::lower_type_expr::function_in_scope_generic_param_bounds(db, func_loc);
                // `Self` is the enclosing class's full receiver type (`Foo<T>`, carrying its
                // generics as `TypeVar` args) — resolved through the lowering context, not erased
                // to a bare `Ty::Class` by a name-substitution pre-pass. Each signature type then
                // substitutes the generics via `bindings`.
                let ctx = crate::lower_type_expr::ScopeCtx {
                    db,
                    package_items: pkg_items_for_class,
                    ns_context: &ns_context,
                    generic_params: all_generic_params,
                    bounds: method_bounds,
                    self_ty: Some(class_ty.clone()),
                };

                if let Some(target) =
                    baml_compiler2_ppir::item_data::method_interface_target(db, func_loc)
                    && let Some(iface_loc) = crate::interfaces::resolve_ref_to_interface(
                        db,
                        &target.type_refs,
                        target.target,
                        pkg_items_for_class,
                        &ns_context,
                    )
                {
                    let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
                    let iface_env = crate::generic_env::interface_generic_env(db, iface_loc);
                    let (iface_self_param, iface_params) = iface_env.interface_param_parts();
                    let iface_self_param = iface_self_param.clone();
                    {
                        if let baml_compiler2_hir::type_ref::TypeRefKind::Path {
                            generic_args,
                            ..
                        } = &target.type_refs[target.target].kind
                        {
                            for (declared, &arg) in
                                iface_data.generic_params.iter().zip(generic_args)
                            {
                                let param = iface_env
                                    .resolve_param(&declared.name)
                                    .expect("interface generic parameter is in its environment")
                                    .clone();
                                let ty = {
                                    let generic_params: Vec<_> = bindings.keys().cloned().collect();
                                    crate::generics::substitute_ty(
                                        &crate::lower_type_expr::lower_type_ref(
                                            &target.type_refs,
                                            arg,
                                            &crate::lower_type_expr::ScopeCtx {
                                                db,
                                                package_items: pkg_items_for_class,
                                                ns_context: &ns_context,
                                                generic_params: &generic_params,
                                                // The args are written in the method's scope; its
                                                // in-scope bounds resolve any `T.member` in them.
                                                bounds: method_bounds,
                                                self_ty: None,
                                            },
                                            &mut diags,
                                        ),
                                        &bindings,
                                    )
                                };
                                bindings.insert(param, ty);
                            }
                        }
                        let realized_iface_args = iface_params
                            .iter()
                            .map(|param| {
                                bindings.get(param).cloned().unwrap_or_else(|| {
                                    Ty::TypeVar(param.clone(), TyAttr::default())
                                })
                            })
                            .collect::<Vec<_>>();

                        let explicit_bindings = &target.associated_type_bindings;
                        for assoc in &iface_data.associated_types {
                            if explicit_bindings.iter().any(|b| b.name == assoc.name) {
                                continue;
                            }
                            if let Some((default, _diags)) =
                                crate::interfaces::interface_associated_type_default(
                                    db,
                                    iface_loc,
                                    assoc.name.clone(),
                                )
                            {
                                // Fill the default at this class receiver: `Self` is the class,
                                // so a Self-referencing default (`type Items = Self.Item[]`)
                                // reduces through its impl. The default is lowered once (symbolic
                                // `Self`) by the shared query; substitute the class for `Self`
                                // then the accumulated generic / associated-type bindings.
                                let realized = crate::interfaces::realize_associated_default(
                                    &default,
                                    iface_params,
                                    &realized_iface_args,
                                    &iface_self_param,
                                    &class_ty,
                                );
                                let ty = crate::generics::substitute_ty(&realized, &bindings);
                                let assoc_param = iface_env
                                    .resolve_any_param(&assoc.name)
                                    .expect(
                                        "associated type parameter is in its interface environment",
                                    )
                                    .clone();
                                bindings.insert(assoc_param, ty);
                            }
                        }
                        for binding in explicit_bindings {
                            let Some(type_ref) = binding.type_ref else {
                                continue;
                            };
                            let ty = crate::generics::substitute_ty(
                                &crate::lower_type_expr::lower_type_ref(
                                    &target.type_refs,
                                    type_ref,
                                    &ctx,
                                    &mut diags,
                                ),
                                &bindings,
                            );
                            let assoc_param = iface_env
                                .resolve_any_param(&binding.name)
                                .expect("associated type parameter is in its interface environment")
                                .clone();
                            bindings.insert(assoc_param, ty);
                        }
                    }
                }

                let callable_throws = crate::callable::callable_throws(db, func_loc).clone();

                let ty = Ty::Function {
                    params: sig
                        .params
                        .iter()
                        .map(|param| {
                            let is_unannotated_self = param.name.as_str() == "self"
                                && matches!(
                                    sig.type_refs[param.type_ref].kind,
                                    baml_compiler2_hir::type_ref::TypeRefKind::Unknown
                                );
                            let param_ty = if is_unannotated_self {
                                // self with no annotation → use the enclosing class type
                                class_ty.clone()
                            } else {
                                crate::generics::substitute_ty(
                                    &crate::lower_type_expr::lower_type_ref(
                                        &sig.type_refs,
                                        param.type_ref,
                                        &ctx,
                                        &mut diags,
                                    ),
                                    &bindings,
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
                            .map(|id| {
                                crate::generics::substitute_ty(
                                    &crate::lower_type_expr::lower_type_ref(
                                        &sig.type_refs,
                                        id,
                                        &ctx,
                                        &mut diags,
                                    ),
                                    &bindings,
                                )
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
                // BUG: that invariant only holds when this ctx and the definition-site
                // scope resolve the same names; a name resolvable there but not here
                // lowers to `Ty::Error` whose diagnostic is silently dropped (`diags`
                // above is never drained). Keep the two scopes in lockstep.
                return ClassMethodLookup::Found {
                    ty,
                    class_loc,
                    func_loc,
                };
            }
        }
        ClassMethodLookup::NotFound
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
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
        let class_generic_env = crate::generic_env::class_generic_env(db, class_loc);
        // The stub class's declared parameter bounds, so a `T.member` projection
        // in a member signature resolves `T`'s declaring interface.
        let class_bounds = crate::lower_type_expr::class_generic_param_bounds(db, class_loc);

        // Bind generic type variables: e.g. {T → int} for Array<int>.
        let mut bindings = crate::generics::bind_type_vars(class_generic_env.params(), type_args);

        // Search methods first.
        for &func_loc in &class_data.methods {
            let method_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
            if method_data.name == *member_name {
                // Add method-level generics as TypeVar entries so they survive
                // lowering and can be resolved by call-site inference.
                let function_generic_env = crate::generic_env::function_generic_env(db, func_loc);
                for gp in function_generic_env.own_params() {
                    bindings
                        .entry(gp.clone())
                        .or_insert_with(|| Ty::TypeVar(gp.clone(), TyAttr::default()));
                }
                // `bindings` is complete for this method — one snapshot serves
                // every param and the return type below.
                let generic_params: Vec<_> = bindings.keys().cloned().collect();
                // Lowered under the call site's concrete type-arg bindings, so
                // this cannot reuse the declaration-site
                // `callable::function_signature_ty` (see the note on the
                // class-member path above).
                let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, func_loc);
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
                        class_generic_env
                            .params()
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
                        let is_unannotated_self = param.name.as_str() == "self"
                            && matches!(
                                sig.type_refs[param.type_ref].kind,
                                baml_compiler2_hir::type_ref::TypeRefKind::Unknown
                            );
                        let ty = if is_unannotated_self {
                            builtin_class_ty.clone()
                        } else {
                            crate::generics::substitute_ty(
                                &crate::lower_type_expr::lower_type_ref(
                                    &sig.type_refs,
                                    param.type_ref,
                                    &crate::lower_type_expr::ScopeCtx {
                                        db,
                                        package_items: self.package_items,
                                        ns_context: stub_ns,
                                        generic_params: &generic_params,
                                        bounds: class_bounds,
                                        self_ty: None,
                                    },
                                    &mut diags,
                                ),
                                &bindings,
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
                    .map(|id| {
                        crate::generics::substitute_ty(
                            &crate::lower_type_expr::lower_type_ref(
                                &sig.type_refs,
                                id,
                                &crate::lower_type_expr::ScopeCtx {
                                    db,
                                    package_items: self.package_items,
                                    ns_context: stub_ns,
                                    generic_params: &generic_params,
                                    bounds: class_bounds,
                                    self_ty: None,
                                },
                                &mut diags,
                            ),
                            &bindings,
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
                let generic_params: Vec<_> = bindings.keys().cloned().collect();
                let field_ty = crate::generics::substitute_ty(
                    &crate::lower_type_expr::lower_type_ref(
                        &class_data.type_refs,
                        field.type_ref,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: self.package_items,
                            ns_context: stub_ns,
                            generic_params: &generic_params,
                            bounds: class_bounds,
                            self_ty: None,
                        },
                        &mut diags,
                    ),
                    &bindings,
                );
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
        // Membership-flavored callers treat an unknown enum and an empty one the
        // same; the resolved/unknown distinction lives in `inference::enum_variants`.
        crate::inference::enum_variants(self.context.db(), self.res_ctx, enum_name)
            .unwrap_or_default()
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
                self.check_index_key_type(&local_ty, &index_ty, index_id, false);
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
                self.check_index_key_type(&local_ty, &index_ty, index_id, false);
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
                } else if !self.is_subtype(&widened_val, val_ty) {
                    // The key was validated by `check_index_key_type` above.
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: *val_ty.clone(),
                            got: widened_val.clone(),
                        },
                        value_id,
                        Vec::new(),
                    );
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

    /// The diagnostic for a type argument `actual` checked against an interface `bound`,
    /// or `None` if it satisfies the bound.
    ///
    /// A generic bound is an **implements** relation, not a subset one: `T extends I`
    /// requires the argument to *implement* `I`, and only a concrete type implements an
    /// interface. So the argument must be
    ///   - a concrete type that implements `I` (through its impls), or
    ///   - a properly-bounded type variable / associated-type projection — a symbolic
    ///     stand-in filled later by a concrete type that satisfies its own bound, so it
    ///     satisfies `I` iff that bound is, or transitively requires, `I`.
    ///
    /// A union or interface-existential passes a plain subtype check (by the subset rule)
    /// but is *not* an implementor, so it is rejected with a dedicated diagnostic. Reading
    /// the bound as implements — never `is_subtype` — is what lets a bounded typevar be used
    /// as a concrete member (e.g. a virtual call to a multi-`Self` interface method): it
    /// stands for the single concrete type that fills it, not an existential over every
    /// implementor. The one place both obligations are enforced, so every bound-check site
    /// stays consistent (see
    /// [`TYPE_SYSTEM.md` § Generics on Functions](TYPE_SYSTEM.md#generics-on-functions)).
    fn bounded_type_arg_error(
        &self,
        actual: &Ty,
        bound: &baml_type::Interface,
    ) -> Option<TirTypeError> {
        // Judge the type the argument denotes: aliases expanded, `never` dropped, a reducible
        // projection collapsed. (A `_` placeholder is rejected at lowering — replaced by
        // `Ty::Error`, which is an admissible sentinel below — so `Ty::Infer` never reaches here.)
        let arg = self.normalize(actual);
        if !Self::is_bounded_arg_admissible(&arg) {
            return Some(TirTypeError::BoundedTypeArgNotConcrete {
                arg: actual.clone(),
                bound: Box::new([bound.clone()]),
            });
        }
        if !self.bounded_arg_implements(&arg, bound) {
            return Some(TirTypeError::TypeMismatch {
                expected: bound.to_ty(),
                got: actual.clone(),
            });
        }
        None
    }

    /// Whether `arg` (already normalized) is admissible as an interface-bounded type
    /// argument by KIND: a [concrete](Ty::is_concrete) type, a symbolic stand-in that is
    /// inductively one — a type variable or associated-type projection, filled later by a
    /// concrete implementor — or an already-errored sentinel (skipped to avoid a cascade).
    /// A union, interface-existential, literal, or `unknown` is not: none of them
    /// *implements* an interface (only concrete types do), so none can fill an interface bound.
    fn is_bounded_arg_admissible(arg: &Ty) -> bool {
        arg.is_concrete()
            || matches!(
                arg,
                Ty::TypeVar(..)
                    | Ty::AssociatedTypeProjection { .. }
                    | Ty::Unknown { .. }
                    | Ty::Error { .. }
            )
    }

    /// Whether the already-normalized, [admissible](Self::is_bounded_arg_admissible) argument
    /// `arg` implements `bound`. Delegates to the shared
    /// [`normalized_arg_implements_bound`](crate::interfaces::normalized_arg_implements_bound) so
    /// the builder's generic-argument gate and the impl-side associated-type-binding check read a
    /// bound identically.
    fn bounded_arg_implements(&self, arg: &Ty, bound: &baml_type::Interface) -> bool {
        crate::interfaces::normalized_arg_implements_bound(self, arg, bound)
    }

    fn validate_function_generic_bounds(
        &mut self,
        expr_id: ExprId,
        generic_params: &[crate::ty::ParamTy],
        generic_param_bounds: &[Vec<Ty>],
        bindings: &FxHashMap<crate::ty::ParamTy, Ty>,
    ) {
        for (idx, param) in generic_params.iter().enumerate() {
            let Some(actual) = bindings.get(param) else {
                continue;
            };
            // Each conjunct is a separate requirement; report every violation.
            for bound in generic_param_bounds.get(idx).into_iter().flatten() {
                let Some(bound) = crate::generics::substitute_ty(bound, bindings).as_interface()
                else {
                    continue;
                };
                if let Some(error) = self.bounded_type_arg_error(actual, &bound) {
                    self.context.report(error, expr_id, Vec::new());
                }
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
                params,
                ret,
                throws,
                ..
            } => {
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
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
        let generic_env = crate::generic_env::class_generic_env(db, class_loc);
        self.collect_named_generic_bound_errors(
            generic_env.params(),
            &class_data.type_refs,
            &class_data.generic_params,
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
        let interface_data = baml_compiler2_ppir::item_data::interface_data(db, interface_loc);
        let generic_env = crate::generic_env::interface_generic_env(db, interface_loc);
        let (_, generic_params) = generic_env.interface_param_parts();
        self.collect_named_generic_bound_errors(
            generic_params,
            &interface_data.type_refs,
            &interface_data.generic_params,
            interface_loc.file(db),
            type_args,
            errors,
        );
    }

    fn collect_named_generic_bound_errors(
        &mut self,
        generic_params: &[crate::ty::ParamTy],
        type_refs: &baml_compiler2_hir::type_ref::TypeRefStore,
        declared_params: &[baml_compiler2_ppir::item_data::GenericParamData],
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
        // The declared bounds are re-lowered here only to check `type_args`
        // against them; a bound's own lowering diagnostics (unresolved name,
        // arity) belong to — and were already reported by — the declaring
        // type's scope, so they are discarded rather than re-reported at
        // every use of the type.
        let lowered_bounds = lower_generic_param_bound_refs(
            db,
            type_refs,
            declared_params,
            pkg_items,
            &pkg_info.namespace_path,
            generic_params,
            None,
            &mut Vec::new(),
        );

        let bindings = crate::generics::bind_type_vars(generic_params, type_args);
        for idx in 0..generic_params.len() {
            let Some(actual) = type_args.get(idx) else {
                continue;
            };
            // `T extends A & B` is a conjunction — the argument must satisfy
            // every conjunct, so each is checked and reported independently.
            for bound in lowered_bounds.get(idx).into_iter().flatten() {
                let Some(bound) = crate::generics::substitute_ty(bound, &bindings).as_interface()
                else {
                    continue;
                };
                if let Some(error) = self.bounded_type_arg_error(actual, &bound) {
                    errors.push(error);
                }
            }
        }
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

    /// Widen a top-level literal / enum-variant type to its base (`1` → `int`,
    /// `Color.Red` → `Color`), leaving everything else unchanged. Comparison validity is
    /// a property of the base type, not the singleton, so the operator checks normalize
    /// here first.
    fn widen_literal_base(ty: &Ty) -> Ty {
        use baml_base::Literal;
        match ty {
            Ty::Literal(Literal::Int(_), _, attr) => Ty::Int { attr: attr.clone() },
            Ty::Literal(Literal::Bigint(_), _, attr) => Ty::Bigint { attr: attr.clone() },
            Ty::Literal(Literal::Float(_), _, attr) => Ty::Float { attr: attr.clone() },
            Ty::Literal(Literal::String(_), _, attr) => Ty::String { attr: attr.clone() },
            Ty::Literal(Literal::Bool(_), _, attr) => Ty::Bool { attr: attr.clone() },
            Ty::EnumVariant(name, _, attr) => Ty::Enum(name.clone(), attr.clone()),
            other => other.clone(),
        }
    }

    /// Whether `ty` implements `baml.ops.Compare` (so `<` `<=` `>` `>=` are defined for
    /// it). `is_subtype` against the `Compare` existential resolves both concrete impls
    /// (via the registry) and a `T extends Compare` type-variable bound.
    fn type_is_comparable(&self, ty: &Ty) -> bool {
        // Ordering needs a *single concrete type* — or a bounded type variable /
        // associated projection, which realizes to exactly one concrete type — that
        // implements `baml.ops.Compare`. A union or interface-existential is NOT
        // orderable even when every member / implementor implements `Compare`: the
        // two operands could hold different concrete types, which exact-type
        // ordering forbids and the runtime cannot order (`is_subtype(union, I)` is
        // member-wise true, so it must be excluded here). `T < T` is fine — both
        // operands share the one type `T` realizes to.
        //
        // Normalize first so a structurally-union-but-collapsible spelling like
        // `int | 99` (a `catch` result widening a literal back into its base) is
        // recognized as the single concrete type `int`, not rejected as a union.
        let ty = self.normalize(ty);
        if matches!(ty, Ty::Union(..) | Ty::Interface(..) | Ty::Unknown { .. }) {
            return false;
        }
        let compare = Ty::Interface(
            crate::ty::QualifiedTypeName::new(
                Name::new("baml"),
                vec![Name::new("ops")],
                Name::new("Compare"),
            ),
            Vec::new(),
            Vec::new(),
            TyAttr::default(),
        );
        self.is_subtype(&ty, &compare)
    }

    /// The `??` / `||` optional-chaining lints, shared by the synthesis and checking
    /// paths: `UnnecessaryNullCoalesce` (`??` on a non-nullable LHS),
    /// `NullCoalesceWithNull` (`?? null` is a no-op), and `SuggestNullCoalesce`
    /// (`||` on a nullable LHS).
    fn report_chaining_lints(
        &mut self,
        op: baml_compiler2_ast::BinaryOp,
        lhs_ty: &Ty,
        lhs: ExprId,
        rhs: ExprId,
        expr_id: ExprId,
        body: &ExprBody,
    ) {
        match op {
            baml_compiler2_ast::BinaryOp::NullCoalesce => {
                // LHS is non-nullable — ?? is unnecessary.
                let inner_lhs = crate::narrowing::remove_null(lhs_ty);
                if inner_lhs == *lhs_ty && !matches!(lhs_ty, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    let lhs_text = body.display_expr(lhs);
                    let expr_text = body.display_expr(expr_id);
                    self.context.report_simple(
                        TirTypeError::UnnecessaryNullCoalesce {
                            lhs: lhs_text,
                            expr: expr_text,
                        },
                        expr_id,
                    );
                }
                // RHS is null — ?? null is a no-op.
                if matches!(&body.exprs[rhs], Expr::Null) {
                    let lhs_text = body.display_expr(lhs);
                    self.context.report_warning_simple(
                        TirTypeError::NullCoalesceWithNull { lhs: lhs_text },
                        expr_id,
                    );
                }
            }
            baml_compiler2_ast::BinaryOp::Or => {
                // LHS is nullable — suggest ?? instead of ||.
                let inner_lhs = crate::narrowing::remove_null(lhs_ty);
                if inner_lhs != *lhs_ty && !matches!(lhs_ty, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    let lhs_text = body.display_expr(lhs);
                    let rhs_text = body.display_expr(rhs);
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
    }

    fn infer_binary_op(
        &mut self,
        op: baml_compiler2_ast::BinaryOp,
        lhs: &Ty,
        rhs: &Ty,
        at: ExprId,
    ) -> Ty {
        use baml_compiler2_ast::BinaryOp;
        // Arithmetic must resolve through its interface before folding. Otherwise
        // two literals could make an operator valid without a matching impl.
        if Self::arithmetic_interface_name(op).is_none()
            && let Some(folded) = Self::try_fold_binary(op, lhs, rhs)
        {
            return folded;
        }
        // Peel type aliases once at the entry so downstream classifiers
        // (`infer_arithmetic`, `infer_bitwise`, and the comparison helpers) only
        // need to recognise the underlying primitive shapes. Mirrors how
        // `is_subtype` and other type-aware sites expand at their entry.
        let expanded_lhs = self.expand_alias_chains(lhs.clone());
        let expanded_rhs = self.expand_alias_chains(rhs.clone());
        let lhs = &expanded_lhs;
        let rhs = &expanded_rhs;
        match op {
            // Equality (`==`, `!=`): valid for *any* pair of operands, result `bool`.
            // This allows `x == null` (the canonical null check), nullable equality, and
            // erased comparisons (`unknown`, unions, interfaces). It only *warns* when
            // the operand types are provably disjoint — no value of one can equal a value
            // of the other — so the comparison is a constant; the `equals_equals`
            // lowering makes the runtime (concrete-type equality) agree.
            BinaryOp::Eq | BinaryOp::Ne => {
                // When the operand types are provably disjoint (no shared value) or
                // provably the same single value, `==` is a constant — fold the result
                // to a `bool` literal so the static type agrees with the
                // `equals_equals` runtime. Disjoint operands (`Some(false)`) also warn
                // (the comparison is pointless); a provably-equal pair does not.
                match self.constant_equality(lhs, rhs) {
                    Some(eq) => {
                        if !eq {
                            self.context.report_warning_simple(
                                TirTypeError::ComparisonAlwaysDisjoint {
                                    op,
                                    lhs: lhs.clone(),
                                    rhs: rhs.clone(),
                                },
                                at,
                            );
                        }
                        let value = if matches!(op, BinaryOp::Eq) { eq } else { !eq };
                        Ty::Literal(
                            crate::ty::LiteralValue::Bool(value),
                            crate::ty::Freshness::Fresh,
                            TyAttr::default(),
                        )
                    }
                    None => Ty::Bool {
                        attr: TyAttr::default(),
                    },
                }
            }

            // Ordering (`<`, `<=`, `>`, `>=`): exact-type — both operands must have the
            // *same* type (subtyping is not enough; only `==` spans types/subtypes), and
            // that type must implement `baml.ops.Compare`. Operands are widened
            // (literal→base, enum-variant→enum) first so `x < 5` compares `int` to `int`;
            // "same type" is `types_equivalent` — the current-context invariant equality
            // (alias-resolving, union-order-insensitive, attr-tolerant), distinct from the
            // coherence unifier's "possible-worlds" view. So `int < float`, `Dog < Animal`,
            // and `int < int?` are all errors even when subtype-related. Error-recovery
            // operands are skipped to avoid cascading diagnostics.
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                if !matches!(lhs, Ty::Unknown { .. } | Ty::Error { .. })
                    && !matches!(rhs, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    let lhs_base = Self::widen_literal_base(lhs);
                    let rhs_base = Self::widen_literal_base(rhs);
                    // Exact-type equality via the canonical algebra, so equivalent
                    // spellings agree — e.g. a `catch` result typed `int | 99`
                    // canonicalizes to `int`, matching `int` on the other side.
                    let same_type = self.equivalent(&lhs_base, &rhs_base);
                    if !same_type {
                        self.context.report_simple(
                            TirTypeError::OrderingDifferentTypes {
                                op,
                                lhs: lhs.clone(),
                                rhs: rhs.clone(),
                            },
                            at,
                        );
                    } else if !self.type_is_comparable(&lhs_base) {
                        self.context.report_simple(
                            TirTypeError::OrderingRequiresCompare { op, ty: lhs_base },
                            at,
                        );
                    }
                }
                Ty::Bool {
                    attr: TyAttr::default(),
                }
            }

            // Logical → bool
            BinaryOp::And | BinaryOp::Or => Ty::Bool {
                attr: TyAttr::default(),
            },

            // Arithmetic: valid iff `lhs` implements `baml.ops.{Add,Subtract,...}`
            // for `rhs`; the result is that impl's `Output` (unioned over operand
            // alternatives). Constant folding runs only after the impl resolves, so
            // the interface registry is the sole source of operator validity.
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if matches!(self.normalize(lhs), Ty::Never { .. })
                    || matches!(self.normalize(rhs), Ty::Never { .. })
                {
                    // A `never` operand makes the operation unreachable (e.g.
                    // an unreachable catch arm's binding): bottom propagates,
                    // no diagnostic.
                    Ty::Never {
                        attr: TyAttr::default(),
                    }
                } else if let Some(ty) = self.infer_arithmetic(op, lhs, rhs) {
                    Self::try_fold_binary(op, lhs, rhs).unwrap_or(ty)
                } else {
                    if !matches!(lhs, Ty::Unknown { .. } | Ty::Error { .. })
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
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
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

    /// The qualified name of the `baml.ops.<name>` interface.
    fn ops_qtn(name: &str) -> crate::ty::QualifiedTypeName {
        crate::ty::QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("ops")],
            Name::new(name),
        )
    }

    /// The `baml.ops` interface an arithmetic operator dispatches through.
    fn arithmetic_interface_name(op: baml_compiler2_ast::BinaryOp) -> Option<&'static str> {
        use baml_compiler2_ast::BinaryOp;
        match op {
            BinaryOp::Add => Some("Add"),
            BinaryOp::Sub => Some("Subtract"),
            BinaryOp::Mul => Some("Multiply"),
            BinaryOp::Div => Some("Divide"),
            BinaryOp::Mod => Some("Remainder"),
            _ => None,
        }
    }

    /// Split an operand type into the concrete alternatives an operator must hold
    /// against. A union contributes each member (every pair must be valid); a
    /// single type (incl. an interface-existential) is one alternative. Normalized
    /// first so a collapsible spelling (`int | 99` → `int`) is one alternative.
    /// `unknown` / error operands yield no members so the caller suppresses
    /// cascading diagnostics.
    fn operator_operand_members(&self, ty: &Ty) -> Vec<Ty> {
        let base = |ty: &Ty| match Self::widen_literal_base(ty) {
            // Builtin class methods type `self` nominally (`baml.Float`), but
            // their runtime receiver and operator impl subject are primitive.
            Ty::Class(qtn, args, attr) if args.is_empty() => {
                if let Some(primitive) = qtn.builtin_primitive() {
                    Ty::from_primitive(primitive, attr)
                } else {
                    Ty::Class(qtn, args, attr)
                }
            }
            other => other,
        };
        // Widen each alternative to its base (literal → primitive, enum-variant →
        // enum): impls are keyed on base types, so `c + 1` must request `Add<int>`,
        // not `Add<1>`.
        let members = match self.normalize(ty) {
            Ty::Union(members, _) => members.iter().map(base).collect(),
            Ty::Unknown { .. } | Ty::Error { .. } => Vec::new(),
            other => vec![base(&other)],
        };
        // Widening can collapse distinct alternatives to one base (`1 | 2` → two
        // `int`s); dedup so each base contributes one pair to the operator product.
        let mut deduped: Vec<Ty> = Vec::with_capacity(members.len());
        for member in members {
            if !deduped.iter().any(|seen| self.equivalent(seen, &member)) {
                deduped.push(member);
            }
        }
        deduped
    }

    /// For one operand, resolve `<l as Iface<args>>::Output` — the result of the
    /// operator applied to `l` (`args` is `[rhs]` for a binary operator, empty
    /// for `Negate`) — to its concrete type. A concrete `l` (or an existential
    /// whose `Output` is specified) yields that `Output`; `None` when `l` does
    /// not implement `Iface<args>`, which the resolver signals by leaving the
    /// projection unresolved (so an `l` whose `Output` can't be pinned to a
    /// concrete type — e.g. an unbounded type variable — is rejected as an
    /// invalid operand).
    fn resolve_operator_output(
        &self,
        iface_qtn: &crate::ty::QualifiedTypeName,
        l: &Ty,
        iface_args: Vec<Ty>,
    ) -> Option<Ty> {
        // Resolve `<l as Iface<args>>::Output` directly rather than checking
        // `is_subtype(l, Iface<args>)` first: the latter fills the interface's
        // `Output` with its `= Self` default, turning the membership test into a
        // stricter `Iface<args, Output=Self>` that a concrete impl (`Output =
        // int`, say) can never satisfy. Normalization reduces the projection
        // through the impl registry (`TypeContext::project`), selecting the impl
        // by the input dimensions only — the right notion here — and leaves the
        // projection unresolved exactly when no impl applies.
        let projection = Ty::AssociatedTypeProjection {
            base: Box::new(l.clone()),
            interface: Box::new(baml_type::Interface::new(
                iface_qtn.clone(),
                iface_args,
                Vec::new(),
            )),
            member: Name::new("Output"),
            attr: TyAttr::default(),
        };
        match self.normalize(&projection) {
            Ty::AssociatedTypeProjection { .. } | Ty::Unknown { .. } | Ty::Error { .. } => None,
            // The `= Self` default realized against a symbolic operand (an
            // interface-existential, or a type var whose bound leaves `Output`
            // unpinned) resolves to the operator existential itself. That
            // claims the result implements the interface, but `Output`'s only
            // bound is `Concrete` — an impl whose `Output` doesn't implement
            // it would falsify the claim at runtime (`(x + 1) + 1` would
            // type-check yet fail to dispatch). Such an operand must pin
            // `Output` explicitly, so the pair is invalid.
            Ty::Interface(name, _, bindings, _)
                if name == *iface_qtn && !bindings.iter().any(|(n, _)| n.as_str() == "Output") =>
            {
                None
            }
            resolved => Some(resolved),
        }
    }

    /// Resolve `lhs OP rhs` through the `baml.ops` arithmetic interfaces: every
    /// `(L, R)` pair of operand alternatives must satisfy `L extends Iface<R>`,
    /// and the result is the union of each pair's `Output`. `None` when the
    /// operator is not arithmetic, an operand has no alternatives (`unknown`), or
    /// any pair is unimplemented — i.e. the operator is invalid for these types.
    fn infer_arithmetic(&self, op: baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Option<Ty> {
        let iface_qtn = Self::ops_qtn(Self::arithmetic_interface_name(op)?);
        let lhs_members = self.operator_operand_members(lhs);
        let rhs_members = self.operator_operand_members(rhs);
        if lhs_members.is_empty() || rhs_members.is_empty() {
            return None;
        }
        let mut output: Option<Ty> = None;
        for l in &lhs_members {
            for r in &rhs_members {
                let pair_output = self.resolve_operator_output(&iface_qtn, l, vec![r.clone()])?;
                output = Some(match output {
                    None => pair_output,
                    Some(acc) => Self::join_types(&acc, &pair_output),
                });
            }
        }
        output
    }

    /// Resolve unary `-operand` through `baml.ops.Negate`: every operand
    /// alternative must implement `Negate`, and the result is the union of each
    /// alternative's `Output` (defaulting to the alternative itself via the
    /// `= Self` default). `None` when an alternative does not implement `Negate`
    /// (or the operand has none) — i.e. negation is invalid.
    fn infer_negate_via_interface(&self, operand: &Ty) -> Option<Ty> {
        let members = self.operator_operand_members(operand);
        if members.is_empty() {
            return None;
        }
        let negate_qtn = Self::ops_qtn("Negate");
        let mut output: Option<Ty> = None;
        for m in &members {
            let member_output = self.resolve_operator_output(&negate_qtn, m, Vec::new())?;
            output = Some(match output {
                None => member_output,
                Some(acc) => Self::join_types(&acc, &member_output),
            });
        }
        output
    }

    /// Returns true if any runtime value of `ty` could be `null` — i.e. the
    /// type is the bare `null` primitive or a union that contains it (a
    /// nullable `T?` lowers to `T | null`).
    ///
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
        // Negation must resolve through its interface before folding.
        if matches!(op, baml_compiler2_ast::UnaryOp::Not)
            && let Some(folded) = Self::try_fold_unary(op, operand)
        {
            return folded;
        }
        let operand_attr = operand.attr().clone();
        match op {
            baml_compiler2_ast::UnaryOp::Not => Ty::Bool { attr: operand_attr },
            // Negation is valid iff the operand implements `baml.ops.Negate`.
            baml_compiler2_ast::UnaryOp::Neg => match operand {
                Ty::Unknown { attr } | Ty::Error { attr } => Ty::Unknown { attr: attr.clone() },
                _ => {
                    if let Some(ty) = self.infer_negate_via_interface(operand) {
                        Self::try_fold_unary(op, operand).unwrap_or(ty)
                    } else {
                        self.context.report_simple(
                            TirTypeError::InvalidUnaryOp {
                                op,
                                operand: operand.clone(),
                            },
                            at,
                        );
                        Ty::Unknown { attr: operand_attr }
                    }
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
    /// Wrap a constant-folded i64 result as an `int` literal type, or `None` if
    /// it falls outside BAML's i63 range. Returning `None` makes the caller skip
    /// folding and leave the operation for the VM, which throws a catchable
    /// `baml.panics.IntegerOverflow` at runtime — so a folded overflow and an
    /// unfolded one (e.g. through variables) behave identically.
    fn fold_int(v: i64, f: crate::ty::Freshness) -> Option<Ty> {
        (crate::INT_MIN..=crate::INT_MAX)
            .contains(&v)
            .then(|| Ty::Literal(crate::ty::LiteralValue::Int(v), f, TyAttr::default()))
    }

    fn try_fold_unary(op: baml_compiler2_ast::UnaryOp, operand: &Ty) -> Option<Ty> {
        use crate::ty::LiteralValue;
        let (lit, f) = match operand {
            Ty::Literal(lit, f, _) => (lit, *f),
            _ => return None,
        };
        match op {
            baml_compiler2_ast::UnaryOp::Neg => match lit {
                // `-INT_MIN` = 2^62 overflows i63, so range-check the result.
                LiteralValue::Int(n) => Self::fold_int(n.checked_neg()?, f),
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
                // Range-check every folded result against i63: `checked_*`
                // only rules out i64 overflow, so e.g. INT_MAX + 1 (fits i64,
                // not i63) must still be rejected. `fold_int` returns None on
                // overflow, deferring to the runtime op's IntegerOverflow throw.
                BinaryOp::Add => Self::fold_int(a.checked_add(b)?, f),
                BinaryOp::Sub => Self::fold_int(a.checked_sub(b)?, f),
                BinaryOp::Mul => Self::fold_int(a.checked_mul(b)?, f),
                BinaryOp::Div => Self::fold_int(a.checked_div(b)?, f),
                BinaryOp::Mod => Self::fold_int(a.checked_rem(b)?, f),
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
                    // `<<` can overflow i63 (e.g. `1 << 62`), so range-check the
                    // result; out-of-range / negative-count cases return None and
                    // defer to the runtime op's IntegerOverflow / NegativeBitShift.
                    let shift = u32::try_from(b).ok()?;
                    Self::fold_int(a.checked_shl(shift)?, f)
                }
                BinaryOp::Shr => {
                    let shift = u32::try_from(b).ok()?;
                    Self::fold_int(a.checked_shr(shift)?, f)
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
    /// Saves the current locals, `declared_return_ty`, `generic_params`,
    /// `implements_block_interface`, and `expressions` (to avoid `ExprId`
    /// collisions between the lambda's arena and the parent's arena). After
    /// inference, restores all saved state; the
    /// lambda's own inference tables are *moved* into `nested_lambda_inference`
    /// keyed by the lambda's `FileScopeId`, so the standalone
    /// `ScopeKind::Lambda` query can project them instead of re-inferring the
    /// body (which also stops its diagnostics from being reported twice).
    ///
    /// Returns `(inferred_return_ty, lambda_file_scope_id, effective_throws)`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the lambda, the body it was written in, and its inferred signature parts"
    )]
    pub fn infer_lambda_body(
        &mut self,
        func_def: &baml_compiler2_ast::LambdaDef,
        lambda_body: &ExprBody,
        param_tys: &[FunctionParamTy],
        expected_ret: Option<&Ty>,
        chosen_throws: &Ty,
        throws_report_span: TextRange,
        warn_extraneous_throws: bool,
    ) -> (Ty, Option<FileScopeId>, Ty) {
        // The body is an expression in the enclosing arena, so `lambda_body` is
        // the body this lambda was written in — there is no arena to switch to.
        let Some(root_expr) = func_def.body else {
            return (
                Ty::Unknown {
                    attr: TyAttr::default(),
                },
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
        // BEP-044: `default` is scoped to the method body and must not be
        // captured across a closure boundary.
        let saved_implements_block_interface = self.implements_block_interface.take();
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
        let saved_owner_type_arg_binding_seed =
            std::mem::take(&mut self.owner_type_arg_binding_seed);
        let saved_self_pinned_rigid_var = std::mem::take(&mut self.self_pinned_rigid_var);
        let saved_lambda_effective_throws = std::mem::take(&mut self.lambda_effective_throws);
        let saved_call_plans = std::mem::take(&mut self.call_plans);
        let saved_call_type_instantiations = std::mem::take(&mut self.call_type_instantiations);
        let saved_function_coercions = std::mem::take(&mut self.function_coercions);
        let saved_expr_metadata_scope = self.expr_metadata_scope;
        // BEP-042: a lambda body is its own control-flow region — `return`
        // targets the lambda, and the parent's defer/loop nesting must not leak
        // in. Reset the counters for the body and restore them afterwards.
        let saved_loop_depth = std::mem::take(&mut self.loop_depth);
        let saved_defer_loop_floors = std::mem::take(&mut self.defer_loop_floors);

        // A lambda declares no generics of its own, so `self.generic_params`
        // already holds the whole environment its body sees.

        // Seed lambda params (captures remain accessible via parent locals).
        //
        // Directly overwrite `locals` rather than going through `add_local`:
        // that helper preserves an existing declared contract, but lambda
        // params shadow outer lets. The lambda param's declared type must
        // replace any outer declaration, and params carry no let-pattern
        // identity.
        let duplicate_names =
            duplicate_parameter_names(param_tys.iter().filter_map(|param| param.name.as_ref()));
        for param in param_tys {
            if let Some(name) = &param.name {
                let ty = parameter_binding_ty(name, &param.ty, &duplicate_names);
                self.locals.insert(
                    name.clone(),
                    LocalBinding {
                        current_ty: ty.clone(),
                        declared_ty: Some(ty),
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
        let (lambda_file_scope_id, lambda_metadata_scope) = {
            let db = self.context.db();
            let file = self.context.scope().file(db);
            let index = baml_compiler2_ppir::file_semantic_index(db, file);
            // The lambda's own `FileScopeId`, matched by the surface lambda's
            // span. Real `(...) -> { ... }` lambdas resolve here.
            //
            // Synthetic lambdas (desugared `test` / `testset` bodies) carry a
            // default `FunctionDef::span`, so this misses. For capture *seeding*
            // (defensive suppression of false "unresolved name" diagnostics on
            // captures) we still want the scope, so we fall back to the body's
            // root-expression span to locate it — but ONLY for seeding.
            //
            // The capture *key* (`lambda_file_scope_id`, used to record this
            // body's tables in `nested_lambda_inference` for the standalone
            // `ScopeKind::Lambda` query to project) is deliberately left `None`
            // for these synthetic bodies. Their diagnostics (e.g. `unresolved
            // name: functions` inside an experimental `test T() { functions
            // [...] args { } }` block) are emitted ONLY by the standalone Lambda
            // inference, not by this owning inline pass; projecting an empty
            // table would drop them. Falling through to standalone re-inference
            // keeps them. Real lambda bodies, whose diagnostics the owner inline
            // pass does emit, are keyed and projected so they are not inferred
            // (or reported) twice.
            let key_fsi = index.lambda_scope_for(func_def.span);
            let seed_fsi = key_fsi.or_else(|| {
                self.body_source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(root_expr))
                    .and_then(|span| index.lambda_scope_for(span))
            });
            if let Some(fsi) = seed_fsi {
                let captures_to_seed: Vec<(Name, baml_compiler2_hir::semantic_index::BindingId)> =
                    index.scope_bindings[fsi.index() as usize]
                        .captures
                        .iter()
                        .filter(|(name, _)| !self.locals.contains_key(name))
                        .map(|(name, binding_id)| (name.clone(), *binding_id))
                        .collect();
                for (capture_name, binding_id) in captures_to_seed {
                    let ty = self.resolve_capture_type(binding_id);
                    self.seed_capture(capture_name, ty);
                }
            }
            (key_fsi, seed_fsi)
        };
        if let Some(scope) = lambda_metadata_scope {
            self.expr_metadata_scope = ExprMetadataScope::Body(scope);
        }

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
            Some(root_expr),
            chosen_throws,
            throws_report_span,
            warn_extraneous_throws,
        );

        let effective_facts = self.collect_effective_throws(lambda_body, Some(root_expr));
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
                                && !crate::ty::is_synthetic_effect_param(name.name())
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

        // Move the lambda's expression types out and swap the parent's back in.
        let lambda_expressions = std::mem::replace(&mut self.expressions, saved_expressions);
        // Capture this lambda's complete inference tables (keyed by its own body
        // arena's IDs) so the standalone `ScopeKind::Lambda` query can project
        // them out instead of re-inferring the body from scratch — the second
        // inference that this eliminates was both wasted work AND the source of
        // duplicated diagnostics inside lambdas. Each table is *moved* out while
        // the saved parent state is swapped back in (`mem::replace`), so there
        // are no clones: the lambda's tables have exactly one consumer.
        // `nested_lambda_inference` itself is NOT restored below, so entries
        // recorded by inner `infer_lambda_body` calls bubble up to the owning
        // Function/Let scope. `pattern_natural_cache` and the other fields
        // outside `NestedLambdaInference` are always just restored — they are
        // caches/context, not part of the lambda's projected result.
        if let Some(fsi) = lambda_file_scope_id {
            let lambda_param_types: Vec<(Name, Ty)> = func_def
                .params
                .iter()
                .zip(param_tys.iter())
                .map(|(param, param_ty)| (param.name.clone(), param_ty.ty.clone()))
                .collect();
            self.nested_lambda_inference.insert(
                fsi,
                crate::inference::NestedLambdaInference {
                    expressions: lambda_expressions,
                    pattern_types: std::mem::replace(&mut self.pattern_types, saved_bindings),
                    resolutions: std::mem::replace(&mut self.resolutions, saved_resolutions),
                    catch_residual_throws: std::mem::replace(
                        &mut self.catch_residual_throws,
                        saved_catch_residual_throws,
                    ),
                    exhaustive_matches: std::mem::replace(
                        &mut self.exhaustive_matches,
                        saved_exhaustive_matches,
                    ),
                    path_root_types: std::mem::replace(
                        &mut self.path_root_types,
                        saved_path_root_types,
                    ),
                    path_segment_types: std::mem::replace(
                        &mut self.path_segment_types,
                        saved_path_segment_types,
                    ),
                    path_member_resolutions: std::mem::replace(
                        &mut self.path_member_resolutions,
                        saved_path_member_resolutions,
                    ),
                    param_types: lambda_param_types,
                    call_plans: std::mem::replace(&mut self.call_plans, saved_call_plans),
                    call_type_instantiations: std::mem::replace(
                        &mut self.call_type_instantiations,
                        saved_call_type_instantiations,
                    ),
                    function_coercions: std::mem::replace(
                        &mut self.function_coercions,
                        saved_function_coercions,
                    ),
                },
            );
        } else {
            // Synthetic lambda with no locatable scope: just restore, dropping
            // its tables (nothing will project them).
            self.pattern_types = saved_bindings;
            self.resolutions = saved_resolutions;
            self.catch_residual_throws = saved_catch_residual_throws;
            self.exhaustive_matches = saved_exhaustive_matches;
            self.path_root_types = saved_path_root_types;
            self.path_segment_types = saved_path_segment_types;
            self.path_member_resolutions = saved_path_member_resolutions;
            self.call_plans = saved_call_plans;
            self.call_type_instantiations = saved_call_type_instantiations;
            self.function_coercions = saved_function_coercions;
        }
        self.pattern_natural_cache = saved_pattern_natural_cache;
        self.interface_method_generic_params = saved_interface_method_generic_params;
        self.owner_type_arg_binding_seed = saved_owner_type_arg_binding_seed;
        self.self_pinned_rigid_var = saved_self_pinned_rigid_var;
        self.lambda_effective_throws = saved_lambda_effective_throws;
        self.locals = saved_locals;
        self.scoped_local_declarations = saved_scoped_local_declarations;
        self.scoped_local_assignments = saved_scoped_local_assignments;
        self.declared_return_ty = saved_return_ty;
        self.generic_params = saved_generic_params;
        self.implements_block_interface = saved_implements_block_interface;
        self.loop_depth = saved_loop_depth;
        self.defer_loop_floors = saved_defer_loop_floors;
        self.expr_metadata_scope = saved_expr_metadata_scope;

        (ret_ty, lambda_file_scope_id, lambda_effective_throws)
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
            | Ty::Infer { .. }
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
            // Ctor args are canonicalized so the arg-sensitive ctor identity
            // agrees with the (equally canonicalized) pattern-side ctors.
            Ty::Class(qtn, args, _) => vec![Ctor::Class(
                qtn.clone(),
                args.iter().map(|a| self.normalize(a)).collect(),
            )],
            // Open interfaces always require a wildcard — new implementors
            // can appear in any file. See BEP-044 §"Interaction with match".
            Ty::Interface(_, _, _, _) => vec![Ctor::NonExhaustive],
            // `Future`'s instantiations cannot be enumerated, so a `Future`
            // column is covered only by a row that is wildcard-shaped at it.
            // A same-instantiation `Future<T, E>` pattern *is* such a row
            // (`dpat_for_type` lowers it to a wildcard once the column is
            // already that instantiation), so a single arm still exhausts a
            // `Future<int, never>` scrutinee; a differently-instantiated pattern
            // claims a different union member instead (`atoms_overlap`).
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
        let invalid_names = self.invalid_pattern_binding_names(pattern);

        // Per-binding pattern_types entry — keyed by source PatId so
        // LSP/codegen can look up the binding's type at any of its source
        // positions (or-pattern alternatives, chain alias binds, etc.).
        for binding in &result.bindings {
            let ty = if invalid_names.contains(&binding.name) {
                Ty::Error {
                    attr: TyAttr::default(),
                }
            } else {
                binding.ty.clone()
            };
            self.pattern_types.insert(binding.pat_id, ty);
        }

        // Scope registration. `locals` is by-name so duplicate names
        // (or-pattern alternatives) collapse to the last declared.
        for binding in &result.bindings {
            let binding_ty = if invalid_names.contains(&binding.name) {
                Ty::Error {
                    attr: TyAttr::default(),
                }
            } else {
                binding.ty.clone()
            };
            let current_ty = if matches!(binding_ty, Ty::Never { .. }) {
                declared_for_scope.cloned().unwrap_or(binding_ty)
            } else {
                binding_ty
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

    fn invalid_pattern_binding_names(&self, pattern: PatId) -> FxHashSet<Name> {
        let Some(source_map) = self.body_source_map.as_ref() else {
            return FxHashSet::default();
        };
        let db = self.context.db();
        let file = self.context.scope().file(db);
        let index = baml_compiler2_ppir::file_semantic_index(db, file);
        let Some(extra) = index.extra.as_ref() else {
            return FxHashSet::default();
        };
        let pattern_key = (source_map.pattern_span(pattern), pattern);
        let Some(&registration_scope) = extra.invalid_pattern_binding_scopes.get(&pattern_key)
        else {
            return FxHashSet::default();
        };
        extra
            .invalid_pattern_bindings
            .get(&(registration_scope, pattern))
            .cloned()
            .unwrap_or_default()
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
        if Self::ty_contains_unresolved(&pat_natural)
            || Self::ty_contains_unresolved(&scrut_for_check)
        {
            return;
        }
        let mismatch = if self.contains_in_scope_rigid_or_projection(&pat_natural)
            || self.contains_in_scope_rigid_or_projection(&scrut_for_check)
        {
            // Rigid-carrying sides: subtype/equality cannot decide reachability
            // (a rigid variable is only *potentially* unifiable with another
            // type), so ask the overlap oracle. An arm with provably no common
            // realization — `Pair<T, T>` against `Pair<int, string>`, or a
            // bounded `T` against a member outside its bound — is dead code, a
            // type error exactly like a concrete mismatch. (Previously skipped:
            // such arms compiled and silently never matched.) The verdict is
            // trusted only when every type variable on both sides is in scope:
            // an out-of-scope variable is an opaque atom to the oracle, so a
            // `No` over it would be a judgment about a variable it cannot see.
            self.all_type_vars_in_scope(&pat_natural)
                && self.all_type_vars_in_scope(&scrut_for_check)
                && self.pattern_overlap_verdict(&pat_natural, &scrut_for_check)
                    == crate::unify::Overlap::No
        } else if crate::generics::contains_typevar(&pat_natural)
            || crate::generics::contains_typevar(&scrut_for_check)
        {
            // A type variable outside this scope's rigid params is not ours to
            // judge — skipped, as before the oracle existed.
            false
        } else {
            !self.pattern_matchable(&pat_natural, &scrut_for_check)
                && !self.pattern_overlaps_scrut_member(&pat_natural, &scrut_for_check)
        };
        if mismatch {
            let err = TirTypeError::TypeMismatch {
                expected: scrut_ty.clone(),
                got: pat_natural,
            };
            self.report_at_pat_or_expr(err, pat_id, at_expr);
        }
    }

    /// Whether an arm with natural type `pat` is *plausible* against a
    /// scrutinee of type `scrut` — the arm-validity over-approximation behind
    /// [`Self::check_pattern_vs_scrut_subtype`]'s `TypeMismatch`. This is NOT a
    /// coverage or runtime-matching relation (coverage is `dpat_for_type`'s
    /// strict column-subtype rule; the runtime relation is the canonical
    /// invariant subtype): it errs toward accepting, and an accepted-but-dead
    /// arm is caught downstream as unreachable rather than mis-compiled.
    ///
    /// The container recursion is deliberately lax in element positions
    /// because a *structural* array/map pattern's natural type embeds its
    /// element patterns' types, and element sub-patterns are matched against
    /// element values with the full (bidirectional) relation: `[]` and
    /// `[first, ..]` have element type `never` and fit any `T[]`, `[1, 2]`
    /// (element type `1 | 2`) fits `int[]`. A concretely-disjoint element
    /// still refutes: `[x: string]` cannot match `int[]`. Non-container pairs
    /// use bidirectional subtyping (either direction is a possible match —
    /// `Dog` matches an `Animal` scrutinee, and vice-versa).
    fn pattern_matchable(&self, pat: &Ty, scrut: &Ty) -> bool {
        let pat = self.expand_alias_chains(pat.clone());
        let scrut = self.expand_alias_chains(scrut.clone());
        match (&pat, &scrut) {
            // An unconstrained pattern leaf (`never`) matches any scrutinee — the
            // bind takes its type from the scrutinee, so the leaf places no
            // constraint. (`never <: T` covariantly.)
            (Ty::Never { .. }, _) => true,
            (Ty::List(a, _) | Ty::EvolvingList(a, _), Ty::List(b, _) | Ty::EvolvingList(b, _)) => {
                self.pattern_matchable(a, b)
            }
            (
                Ty::Map {
                    key: ka, value: va, ..
                },
                Ty::Map {
                    key: kb, value: vb, ..
                },
            ) => self.pattern_matchable(ka, kb) && self.pattern_matchable(va, vb),
            // A bare interface pattern head (`Source { value }`) places no pin
            // constraint — it destructures any realization of that interface,
            // adopting the scrutinee's pins (mirrors `lower_class_pat`'s adoption).
            // A pinned head still constrains: it falls through to bidirectional
            // subtyping, where differing pins reject the arm.
            (
                Ty::Interface(pat_qtn, pat_args, pat_assoc, _),
                Ty::Interface(scrut_qtn, scrut_args, _, _),
            ) if pat_qtn == scrut_qtn
                && pat_assoc.is_empty()
                && (pat_args.is_empty()
                    || (pat_args.len() == scrut_args.len()
                        && pat_args
                            .iter()
                            .zip(scrut_args.iter())
                            .all(|(a, b)| self.equivalent(a, b)))) =>
            {
                true
            }
            _ => self.is_subtype(&pat, &scrut) || self.is_subtype(&scrut, &pat),
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
        // yields itself and was already covered by `pattern_matchable`. Each
        // member is checked with the same relation, so a covariant-destructure
        // pattern (bare interface head, empty array) matches a union member
        // exactly as it would a scalar scrutinee of that member's type.
        members.len() > 1 && members.iter().any(|m| self.pattern_matchable(pat, m))
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
                // bound across `S` and `NoYield` ends up typed at
                // `S | NoYield` rather than last-write-wins.
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
        // A `TypeVar` scrutinee member (e.g. the `T` in `T | string | null`)
        // is an *open* type: at runtime it stands for whatever concrete type
        // `T` is instantiated to, so a pattern targets it exactly when the
        // overlap oracle says some realization could give them a common value
        // — a concrete `let s: string` arm *does* target the `T` member (it
        // matches `T = string` values), while a pattern outside a bounded
        // `T extends I`'s bound does not. Claiming is safe against the old
        // over-claim shadowing hazard (a concrete arm claiming `T` and being
        // deemed to cover it, reporting the dedicated `let v: T` arm
        // unreachable — the `tag_or_value<T>` bug) because coverage is decided
        // separately by `dpat_for_type`: in a rigid column, a non-reflexive
        // pattern is a possible-but-not-covering `Single` row, never a cover.
        // A residual associated-type projection member (`Self.Item`) is open
        // in exactly the same way. Open-atom patterns (`TypeVar`/projection/
        // recovery atoms in the natural type) keep the oracle-free fast path.
        let natural_atoms = {
            let mut atoms = Vec::new();
            self.collect_overlap_atoms(&natural, &mut atoms);
            atoms
        };
        let pattern_claims_open = natural_atoms.iter().any(|a| {
            matches!(
                a,
                Ty::TypeVar(..)
                    | Ty::AssociatedTypeProjection { .. }
                    | Ty::Unknown { .. }
                    | Ty::Error { .. }
            )
        });
        union_members
            .iter()
            .filter(|m| {
                if matches!(m, Ty::TypeVar(..) | Ty::AssociatedTypeProjection { .. }) {
                    pattern_claims_open
                        || self.pattern_overlap_verdict(&natural, m) != crate::unify::Overlap::No
                } else {
                    self.types_overlap(&natural, m)
                }
            })
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
            // Class: same qtn AND every type-arg pair could be the same
            // realized argument (invariant positions — see
            // [`Self::dispatch_args_compatible`]).
            (Ty::Class(q1, args1, _), Ty::Class(q2, args2, _)) => {
                q1 == q2
                    && args1.len() == args2.len()
                    && args1
                        .iter()
                        .zip(args2.iter())
                        .all(|(x, y)| self.dispatch_args_compatible(x, y))
            }
            // Enum/EnumVariant: same enum qtn. Variants of the same enum
            // overlap with each other and with the bare enum.
            (Ty::Enum(q1, _), Ty::Enum(q2, _)) => q1 == q2,
            (Ty::EnumVariant(q1, v1, _), Ty::EnumVariant(q2, v2, _)) => q1 == q2 && v1 == v2,
            (Ty::Enum(q1, _), Ty::EnumVariant(q2, _, _))
            | (Ty::EnumVariant(q2, _, _), Ty::Enum(q1, _)) => q1 == q2,
            // List/Map: same head AND the element / key / value pairs could
            // be the same realized argument. These are invariant positions —
            // a runtime list carries exactly one element type, so a pattern
            // targeting `List<int>` targets neither `List<string>` nor
            // `List<int | string>` (`int[]` values are not members of
            // `(int | string)[]`). The element types are part of the
            // pattern's natural shape (e.g. `[let x: int]` has natural
            // `List<int>`).
            (
                Ty::List(a_elem, _) | Ty::EvolvingList(a_elem, _),
                Ty::List(b_elem, _) | Ty::EvolvingList(b_elem, _),
            ) => self.dispatch_args_compatible(a_elem, b_elem),
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
            ) => self.dispatch_args_compatible(a_k, b_k) && self.dispatch_args_compatible(a_v, b_v),
            // Future: same head AND both type arguments could be the same
            // realized argument — invariant positions, exactly like the
            // containers above. A spawned future carries the `<T, E>` its spawn
            // site was typed at, so a `Future<int, never>` pattern does not
            // target a `Future<string, never>` member.
            (Ty::Future(a_value, a_error, _), Ty::Future(b_value, b_error, _)) => {
                self.dispatch_args_compatible(a_value, b_value)
                    && self.dispatch_args_compatible(a_error, b_error)
            }
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
            // Two residual projections can realize to the same concrete type
            // (`Self.Item` on both sides, or `T.Item` vs `U.Item` at a common
            // instantiation), so a projection pattern targets a projection
            // member — this is what lets `let x: Self.Item` specialize onto its
            // own member of a `Done | Self.Item` scrutinee. Projection-vs-
            // concrete pairs deliberately stay non-overlapping *here*: like a
            // `TypeVar` member, an open projection member must not be claimed
            // by concrete patterns (`union_targets_for_pattern`'s directional
            // gate), and a projection pattern's possible reach into concrete
            // members is the narrowing oracle's business, not union dispatch's.
            (Ty::AssociatedTypeProjection { .. }, Ty::AssociatedTypeProjection { .. }) => true,
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

    /// Whether two generic-argument types could be the *same* realized
    /// argument — the invariant-position analogue of [`Self::types_overlap`].
    /// Runtime dispatch relates argument positions invariantly (an `int[]`
    /// value is not a member of `(int | string)[]`), so a pattern claims a
    /// container/class member only when the argument pair could be
    /// equivalent, not merely overlapping — otherwise the claim leaks into
    /// the arm's joined `matched_ty`, and the runtime test MIR emits from it
    /// would admit a foreign member's value at the pattern's wider type.
    ///
    /// Open or recovery atoms (type variables, projections, the
    /// `Unknown`/`Error` sentinels) anywhere in the pair keep the claim lax —
    /// their realization is not decidable here, and over-claiming is
    /// fail-safe (coverage stays strict per column). User-written `unknown`
    /// (`BuiltinUnknown`) is NOT open: it is the decidable top type, and in
    /// an invariant position it identifies exactly its own member (`unknown[]`
    /// values are the lists constructed at element type `unknown`; an `int[]`
    /// member shares none). Literals and enum variants widen to their bases
    /// first so a structural pattern's natural element type (`[1, 2]` ⇒
    /// `List<1 | 2>`) still claims its base-typed member (`int[]`).
    fn dispatch_args_compatible(&self, a: &Ty, b: &Ty) -> bool {
        let a = self.expand_alias_chains(a.clone());
        let b = self.expand_alias_chains(b.clone());
        let open = |t: &Ty| {
            crate::generics::contains_ty_where(t, &|x| {
                matches!(
                    x,
                    Ty::Unknown { .. }
                        | Ty::Error { .. }
                        | Ty::Infer { .. }
                        | Ty::TypeVar(..)
                        | Ty::AssociatedTypeProjection { .. }
                )
            })
        };
        if open(&a) || open(&b) {
            return true;
        }
        match (&a, &b) {
            (
                Ty::List(a_elem, _) | Ty::EvolvingList(a_elem, _),
                Ty::List(b_elem, _) | Ty::EvolvingList(b_elem, _),
            ) => self.dispatch_args_compatible(a_elem, b_elem),
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
            ) => self.dispatch_args_compatible(a_k, b_k) && self.dispatch_args_compatible(a_v, b_v),
            (Ty::Class(q1, args1, _), Ty::Class(q2, args2, _)) => {
                q1 == q2
                    && args1.len() == args2.len()
                    && args1
                        .iter()
                        .zip(args2.iter())
                        .all(|(x, y)| self.dispatch_args_compatible(x, y))
            }
            _ => self.equivalent(
                &Self::widen_literal_members(&a),
                &Self::widen_literal_members(&b),
            ),
        }
    }

    /// Widen top-level literal / enum-variant members to their bases
    /// ([`Self::widen_literal_base`]), through one level of union. Deeper
    /// occurrences sit inside containers/classes, which
    /// [`Self::dispatch_args_compatible`] recurses through before widening.
    fn widen_literal_members(ty: &Ty) -> Ty {
        match ty {
            Ty::Union(members, attr) => Ty::Union(
                members.iter().map(Self::widen_literal_base).collect(),
                attr.clone(),
            ),
            other => Self::widen_literal_base(other),
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
    /// patterns. Five regimes:
    ///
    /// - Singleton types (`Literal`, `EnumVariant`, `null`) →
    ///   `DPat::single(t, scrut_ty)`.
    /// - Finite-alphabet types (`bool`, `Enum`, finite literal unions,
    ///   `Optional<T>`, unions of finites) → `DPat::or` of the
    ///   singletons; the algorithm explodes Or rows during specialization.
    /// - Class types → `DPat::class(qtn, [Wildcard for each field])`.
    /// - Rigid-carrying leaves (`T`, `T[]`, `map<string, T>`, `Self.Item`, …):
    ///   canonically equal to the column type → `DPat::wildcard` (the arm
    ///   *definitely* covers the column — `let x: Self.Item` over a
    ///   `Self.Item` member); otherwise → `DPat::single(t, scrut_ty)`, a
    ///   possible-but-not-covering row (a rigid variable is only
    ///   *potentially* unifiable with another type, so the arm is reachable
    ///   but proves no coverage — a `_` after it stays reachable).
    /// - Opaque concrete alphabets (raw int/string/float, lists, maps,
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
            // Classes: structural ctor with all fields wildcarded. The ctor's
            // type args are canonicalized so the matrix's arg-sensitive ctor
            // identity (`Box<T>` ≠ `Box<int>`, but `Box<IntAlias>` = `Box<int>`)
            // agrees between the pattern and column sides; the field types are
            // derived from the same canonical args so nested ctor identities
            // stay consistent too.
            Ty::Class(qtn, args, _) => {
                let canonical_args: Vec<Ty> = args.iter().map(|a| self.normalize(a)).collect();
                let field_tys = self.class_field_types_ordered(qtn, &canonical_args);
                let fields = field_tys.into_iter().map(DPat::wildcard).collect();
                DPat::class_inst(qtn.clone(), canonical_args, fields, scrut_ty.clone())
            }
            Ty::Interface(_, _, _, _) => {
                let field_tys = self
                    .interface_field_infos_ordered_for_ty(&expanded)
                    .into_iter()
                    .map(|(_, ty)| ty);
                let fields = field_tys.map(DPat::wildcard).collect();
                DPat::interface(expanded.clone(), fields, scrut_ty.clone())
            }
            // Rigid-carrying leaves: reflexivity is a definite cover
            // (wildcard); anything else is a possible-but-not-covering row.
            // `Single` deliberately — the column alphabets treat it as outside
            // their ctor set, so it proves no coverage, and the split includes
            // present ctors so the row is still specialized (not falsely
            // flagged unreachable).
            _ if self.contains_in_scope_rigid_or_projection(&expanded) => {
                if self.equivalent(&expanded, scrut_ty) {
                    DPat::wildcard(scrut_ty.clone())
                } else {
                    DPat::single(self.normalize(&expanded), scrut_ty.clone())
                }
            }
            // Fallback leaves. A pattern covers its column exactly when the
            // column is provably a subtype of the pattern — every column value
            // then matches, under the same canonical relation the runtime test
            // evaluates (invariant generic arguments; contravariant/covariant
            // function components). This includes function-typed patterns: a
            // `throws unknown` pattern covers a `throws E` member because
            // `E <: unknown` holds for every realization.
            //
            // Everything else is a possible-but-not-covering `Single` row: a
            // concrete pattern over a rigid column (`let s: string` over `T`
            // matches only the realizations that make `T` its subtype), a
            // container pattern over a member with a different argument (an
            // `int[]` value is not a member of `(int | string)[]`), and a
            // disjoint concrete pair reached through a union wrap (the
            // `string` alternative of `let p: T | string` lowered against a
            // claimed `bool` member). The old lax disjunct — covariant
            // container coverage via `pattern_matchable` — let a
            // `(int | string)[]` arm cover an `int[]` column: MIR then
            // skipped the arm's (invariant) test and bound the narrower array
            // to the wider element type, and pushing through that binding
            // corrupted the original. Unresolved-carrying patterns or columns
            // stay wildcards: the resolution failure is already diagnosed,
            // and coverage must not cascade on top of it — but only genuinely
            // unresolved types (`Unknown`/`Error` recovery, in-flight
            // `Infer`) qualify. User-written `unknown` is `BuiltinUnknown`, a
            // real (top) type that is invariant in argument positions like
            // any other: an `unknown[]` arm covers the `unknown[]` member and
            // nothing else.
            _ => {
                let scrut_expanded = self.expand_alias_chains(scrut_ty.clone());
                if self.is_subtype(&scrut_expanded, &expanded)
                    || Self::ty_contains_unresolved(&expanded)
                    || Self::ty_contains_unresolved(&scrut_expanded)
                {
                    DPat::wildcard(scrut_ty.clone())
                } else {
                    DPat::single(self.normalize(&expanded), scrut_ty.clone())
                }
            }
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
    fn lower_named_pattern_fields(
        &mut self,
        owner: NamedPatternFieldOwner<'_>,
        fields: &[ast::FieldPat],
        declared_fields: &[(Name, Ty)],
        pat_id: PatId,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> LoweredNamedPatternFields {
        use crate::exhaustiveness::DPat;

        let declared_field_names: Vec<Name> = declared_fields
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let field_slots: FxHashMap<Name, usize> = declared_field_names
            .iter()
            .cloned()
            .enumerate()
            .map(|(slot, name)| (name, slot))
            .collect();
        let mut sub_dpats: Vec<DPat> = declared_fields
            .iter()
            .map(|(_, ty)| DPat::wildcard(ty.clone()))
            .collect();
        let mut bindings = Vec::new();
        let mut seen_fields = FxHashSet::default();

        for field in fields {
            let Some(&slot) = field_slots.get(&field.field) else {
                let error = match owner {
                    NamedPatternFieldOwner::Class(class_name) => {
                        TirTypeError::UnknownClassPatternField {
                            class_name: class_name.clone(),
                            field_name: field.field.clone(),
                            suggestions: Self::class_pattern_field_suggestions(
                                &field.field,
                                &declared_field_names,
                            ),
                        }
                    }
                    NamedPatternFieldOwner::Interface(interface_ty) => {
                        TirTypeError::UnresolvedMember {
                            base_type: interface_ty.clone(),
                            member: field.field.clone(),
                        }
                    }
                };
                self.report_at_pat_or_expr(error, pat_id, at_expr);
                let unknown = Ty::Unknown {
                    attr: TyAttr::default(),
                };
                let result = self.analyze_and_lower(field.pat, &unknown, body, at_expr);
                bindings.extend(result.bindings);
                continue;
            };

            let first_occurrence = seen_fields.insert(field.field.clone());
            let field_ty = if first_occurrence {
                declared_fields[slot].1.clone()
            } else {
                Ty::Error {
                    attr: TyAttr::default(),
                }
            };
            let result = self.analyze_and_lower(field.pat, &field_ty, body, at_expr);
            if first_occurrence {
                sub_dpats[slot] = result.dpat;
            }
            bindings.extend(result.bindings);
        }

        LoweredNamedPatternFields {
            sub_dpats,
            bindings,
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
        use crate::{exhaustiveness::DPat, pattern_lowering::PatternResult};

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
            // A head that omits its associated bindings adopts them from the
            // scrutinee — so the scrutinee must determine them *uniquely*. Collect
            // every scrutinee realization of this interface consistent with the
            // written generic args (a union may carry several): exactly one →
            // adopt its args/bindings; two or more distinct → ambiguous, the
            // pattern must write the bindings (`Source<Item = …> { … }`); none →
            // keep the written form (the mismatch is reported by the pattern-vs-
            // scrut gate). A head with written bindings is used as written.
            let effective_interface_ty = if pattern_assoc.is_empty() {
                // Candidate realizations, kept as the member `Ty::Interface`s themselves.
                let mut candidates: Vec<Ty> = Vec::new();
                for member in self.flatten_union_optional_members(scrut_ty) {
                    let Ty::Interface(member_qtn, member_args, ..) = &member else {
                        continue;
                    };
                    if member_qtn != pattern_iface_qtn {
                        continue;
                    }
                    let args_compatible = pattern_args.is_empty()
                        || (pattern_args.len() == member_args.len()
                            && pattern_args
                                .iter()
                                .zip(member_args.iter())
                                .all(|(a, b)| self.equivalent(a, b)));
                    if args_compatible && !candidates.contains(&member) {
                        candidates.push(member);
                    }
                }
                if candidates.len() > 1 {
                    self.report_at_pat_or_expr(
                        TirTypeError::AmbiguousInterfacePatternBindings {
                            interface: pattern_iface_qtn.clone(),
                            candidates: candidates.clone(),
                        },
                        pat_id,
                        at_expr,
                    );
                }
                match (candidates.len(), candidates.into_iter().next()) {
                    (1, Some(Ty::Interface(_, scrut_args, scrut_assoc, scrut_attr))) => {
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
                }
            } else {
                Ty::Interface(
                    pattern_iface_qtn.clone(),
                    pattern_args.clone(),
                    pattern_assoc.clone(),
                    attr.clone(),
                )
            };
            let field_infos = self.interface_field_infos_ordered_for_ty(&effective_interface_ty);
            let lowered_fields = self.lower_named_pattern_fields(
                NamedPatternFieldOwner::Interface(&effective_interface_ty),
                fields,
                &field_infos,
                pat_id,
                body,
                at_expr,
            );
            let dpat = DPat::interface(
                effective_interface_ty.clone(),
                lowered_fields.sub_dpats,
                scrut_ty.clone(),
            );
            let matched_ty = self.intersect_pattern_flow_types(scrut_ty, &effective_interface_ty);
            return PatternResult {
                dpat,
                required_ty: Some(effective_interface_ty),
                matched_ty,
                bindings: lowered_fields.bindings,
            };
        }

        if !matches!(class_ty, Ty::Class(..)) {
            // Resolution failed. Continue through every field subpattern so
            // its bindings and per-pattern types survive recovery, but poison
            // the bindings so their later uses do not produce cascades.
            let error_ty = Ty::Error {
                attr: TyAttr::default(),
            };
            let mut bindings = Vec::new();
            for field in fields {
                let result = self.analyze_and_lower(field.pat, &error_ty, body, at_expr);
                bindings.extend(result.bindings.into_iter().map(|mut binding| {
                    binding.ty = error_ty.clone();
                    binding
                }));
            }
            return PatternResult {
                dpat: DPat::wildcard(scrut_ty.clone()),
                required_ty: Some(class_ty.clone()),
                matched_ty: class_ty,
                bindings,
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

        let field_infos = self.class_field_infos_ordered(&qtn, &args);
        let lowered_fields = self.lower_named_pattern_fields(
            NamedPatternFieldOwner::Class(&qtn),
            fields,
            &field_infos,
            pat_id,
            body,
            at_expr,
        );

        // Ctor args canonicalized to agree with the column-side ctor identity
        // (see `normalize`).
        let canonical_args = args.iter().map(|a| self.normalize(a)).collect();
        PatternResult {
            dpat: DPat::class_inst(
                qtn,
                canonical_args,
                lowered_fields.sub_dpats,
                scrut_ty.clone(),
            ),
            required_ty: Some(class_ty.clone()),
            matched_ty: class_ty,
            bindings: lowered_fields.bindings,
        }
    }

    /// Best-effort typo suggestions for unknown class-pattern fields.
    ///
    /// We keep this local to pattern lowering so we can avoid adding heavier
    /// symbol-table machinery for a single-field-name hint.
    fn class_pattern_field_suggestions(
        unknown_field: &Name,
        declared_fields: &[Name],
    ) -> Vec<Name> {
        Self::similar_name_suggestions(unknown_field, declared_fields.iter())
    }

    /// Rank close names for diagnostics. Prefix and substring relationships get
    /// a small boost so singular/plural and missing-suffix mistakes remain
    /// helpful without suggesting unrelated short identifiers.
    fn similar_name_suggestions<'a>(
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

    /// Render a [`crate::exhaustiveness::WitnessPat`] for the
    /// `non-exhaustive match` diagnostic. Mirrors `WitnessPat`'s `Display`
    /// impl but looks up class field names so witnesses like
    /// `Mixed { value: int }` say which field is which instead of just
    /// `Mixed { int }`.
    fn render_witness_pat(&self, w: &crate::exhaustiveness::WitnessPat) -> String {
        use std::fmt::Write as _;

        use crate::exhaustiveness::Ctor;
        match &w.ctor {
            Ctor::Class(qtn, args) => {
                let names = self.class_field_names_ordered(qtn);
                let qtn_str = crate::exhaustiveness::class_witness_head(qtn, args);
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
        baml_compiler2_ppir::item_data::class_data(db, class_loc)
            .fields
            .iter()
            .map(|f| f.name.clone())
            .collect()
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

        // `..` may carry a binding-shaped sub-pattern: `..let r`, `.._`,
        // pure bind chains (`..let r: let s`), optionally terminated by a
        // `: T` ascription link (`..let r: int[]`, checked against the
        // slice type below). Anything else is rejected:
        //   - type patterns and class destructures are statically dead —
        //     the slice's runtime tag is always exactly `elem[]` (built by
        //     `Array.slice` from the scrutinee) and list type tests are
        //     invariant tag compares, so they can never narrow;
        //   - refutable shapes (or-patterns like `..([] | [_])`) have no
        //     column in the usefulness matrix: the slice DPat below is
        //     rustc's model, whose `..` only ever carries a binding, so
        //     allowing them makes "exhaustive" matches that fall through
        //     at runtime;
        //   - nested array rests (`..[a, ..r, b]`) are sound if flattened
        //     into the outer shape before the matrix runs, but they are a
        //     second spelling of the flat pattern (`[..[]]` = `[]`), so
        //     they stay out until someone wants them.
        // The set could be expanded in the future along exactly those
        // lines: flatten nested arrays, extend the matrix with a rest
        // constraint, or make list tests structural.
        let mut bindings: Vec<PatternBinding> = Vec::new();
        let (has_rest, rest_binding_pat) = match rest.and_then(|rp| rp.pat) {
            None => (rest.is_some(), None),
            Some(rest_pat) => {
                if Self::rest_subpattern_is_binding_shaped(body, rest_pat, false) {
                    (true, Some(rest_pat))
                } else {
                    self.report_at_pat_or_expr(
                        TirTypeError::RestSubPatternNotBinding,
                        rest_pat,
                        at_expr,
                    );
                    // Recovery: keep the rejected sub-pattern's names in
                    // scope (typed unknown) so the body doesn't cascade
                    // unresolved-name errors, and treat the rest as bare
                    // `..` for the slice shape.
                    for name in body.patterns[rest_pat].bound_names(&body.patterns) {
                        bindings.push(PatternBinding {
                            name: name.clone(),
                            pat_id: rest_pat,
                            ty: Ty::Unknown {
                                attr: TyAttr::default(),
                            },
                        });
                    }
                    (true, None)
                }
            }
        };

        let mut sub_dpats: Vec<DPat> = Vec::with_capacity(prefix.len() + suffix.len());
        let mut element_required_tys: Vec<Ty> = Vec::new();

        for &p in prefix.iter().chain(suffix) {
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
            // A `: T` ascription on the rest binding must be EQUIVALENT to
            // the slice type, not merely a subtype. The slice's runtime tag
            // is always exactly `elem[]`, so a narrowing ascription (e.g.
            // `..let r: int[]` on `(int|string)[]`) can never match — better
            // a mismatch here than a statically-dead arm. Non-list types
            // (`..let r: int`) fail the same check.
            if let Some(expected) = self.pattern_expected_ty(rest_pat, body)
                && !Self::ty_contains_unresolved(&rest_ty)
                && !crate::generics::contains_typevar(&rest_ty)
                && !(self.is_subtype(expected.ty(), &rest_ty)
                    && self.is_subtype(&rest_ty, expected.ty()))
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
                prefix: prefix.len(),
                suffix: suffix.len(),
            }
        } else {
            SliceShape::Fixed(prefix.len() + suffix.len())
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

    /// Whether a rest sub-pattern is binding-shaped: a wildcard, a bare
    /// binding, or a bind chain whose links are all binds with an optional
    /// terminal `: T` ascription link (`in_chain` distinguishes that
    /// position — a bare type pattern as the WHOLE sub-pattern, `..int`, is
    /// not binding-shaped). These shapes are irrefutable against the slice,
    /// which is what lets the usefulness matrix keep treating the rest as
    /// "matches any length" while MIR materializes the binding.
    fn rest_subpattern_is_binding_shaped(body: &ExprBody, pat: PatId, in_chain: bool) -> bool {
        match &body.patterns[pat] {
            ast::Pattern::Wildcard => true,
            ast::Pattern::Type(_) => in_chain,
            ast::Pattern::Bind { subpat, .. } => match subpat {
                None => true,
                Some(sp) => Self::rest_subpattern_is_binding_shaped(body, *sp, true),
            },
            ast::Pattern::Array { .. } | ast::Pattern::Class { .. } | ast::Pattern::Or(_) => false,
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

        // Emit `OrPatternBindingTypeMismatch` per offending branch, then
        // poison every occurrence so the arm body cannot inherit one
        // alternative's type based on source order.
        let conflicting_names =
            self.check_or_binding_type_compatibility(&bindings_by_name, at_expr);
        for binding in &mut bindings {
            if conflicting_names.contains(&binding.name) {
                binding.ty = Ty::Error {
                    attr: TyAttr::default(),
                };
            }
        }

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
