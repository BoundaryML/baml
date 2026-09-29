//! Code generation for BAML (compiler2 pipeline).
//!
//! Compiles MIR2 to bytecode for the BAML VM using stackification.

mod analysis;
mod emit;
mod items;
mod package;
mod pull_semantics;
mod refs;
mod stack_carry;
#[cfg(any(debug_assertions, test))]
mod verifier;

use std::collections::{HashMap, HashSet};

pub use analysis::OptLevel;
use baml_base::{Name, Span};
use baml_compiler2_ast::TypeExpr;
// HIR item-data firewall — enumeration + lookup queries in place of the raw
// item tree.
use baml_compiler2_hir::item_data::{
    GenericParamData, class_data, function_data, function_llm_meta, interface_data,
};
use baml_compiler2_hir::{
    contributions::Definition,
    file_package::file_package,
    loc::{FunctionLoc, LetLoc},
    package::world_files,
};
use baml_compiler2_mir::{
    BuiltinKind, MirFunctionBody, MirFunctionKind, Operand, Place, RuntimeLowering, RuntimeTy,
    Rvalue, StatementKind, definition_link_name, lower_function,
    memory::{self, CellId},
};
use baml_type::ParamTy;
use bex_vm_types::{
    Bytecode, CaptureCategory, Function, FunctionCaptureProps, FunctionKind, FunctionMeta,
    FunctionOrigin, Object, ObjectPool,
};
pub(crate) use emit::compile_mir_function;
pub use package::{emit_package, emit_session_submission};

/// Is `name` spelled under a language package? Such a function is a builtin
/// whatever file it comes from.
fn is_builtin_function_name(name: &str) -> bool {
    matches!(
        name.split('.').next(),
        Some(
            "baml"
                | "boundary"
                | "reflect"
                | "assert"
                | "testing"
                | "log"
                | "env"
                | "ai"
                | "openai"
                | "anthropic"
                | "google"
                | "aws"
                | "vercel"
                | "claude_code"
        )
    )
}

fn emitted_function_origin(
    fq_name: &str,
    is_builtin_file: bool,
    origin: baml_compiler2_ast::FunctionOrigin,
) -> FunctionOrigin {
    if is_builtin_file || is_builtin_function_name(fq_name) {
        FunctionOrigin::Builtin
    } else {
        match origin {
            baml_compiler2_ast::FunctionOrigin::UserDefined => FunctionOrigin::UserDefined,
            baml_compiler2_ast::FunctionOrigin::Companion => FunctionOrigin::Companion,
            // A test registrar is language-internal machinery on the wire.
            baml_compiler2_ast::FunctionOrigin::Internal
            | baml_compiler2_ast::FunctionOrigin::TestInitializer => FunctionOrigin::Internal,
            baml_compiler2_ast::FunctionOrigin::AutoDerive => FunctionOrigin::AutoDerive,
        }
    }
}

/// Read-only snapshot of pooled class field metadata: every name registered in
/// A class's fields (name + type, in field order), keyed by the class's own
/// [`TypeTag`](baml_type::typetag::TypeTag) — the identity its `TypeHead`
/// carries, so a lookup is an integer compare and needs no name spelling.
///
/// Built once from the `Object::Class` entries before function bodies are
/// compiled, so codegen resolves field names/types without reading the object
/// pool (a hard requirement for parallel emit, whose workers compile against
/// fragment pools that don't contain the pre-existing objects).
pub(crate) type ClassFieldSnapshot<'db> = HashMap<
    baml_compiler2_hir_ty::extern_loc::ClassRef<'db>,
    Vec<(String, baml_compiler2_mir::RuntimeTy)>,
>;

/// Context for MIR codegen.
pub(crate) struct MirCodegenContext<'db, 'ctx, 'obj, 'w> {
    /// The database link names are rendered through at the codegen boundary.
    pub db: &'ctx dyn baml_compiler2_mir::Db,
    /// The resolution surface every declaration reference goes through
    /// ([`refs::PackageRefs`]); built once per pass, it outlives the body.
    pub refs: &'obj mut refs::PackageRefs<'w, 'db>,
    pub class_fields: &'ctx ClassFieldSnapshot<'db>,
    pub objects: &'obj mut ObjectPool,
    /// Program-absolute index of `objects[0]`: 0 when `objects` is the whole
    /// program pool (serial emit), the Stage-B watermark when it is a
    /// worker-local fragment pool (parallel emit).
    pub objects_base: usize,
    /// Maps MIR lambda index → `ObjectPool` index of the compiled lambda `Function`.
    /// Parallel to `lambda_names`. Empty for non-lambda functions.
    pub lambda_object_indices: &'ctx [usize],
    /// Lambda debug names, parallel to `lambda_object_indices`.
    pub lambda_names: &'ctx [String],
    /// Compile-time types for captures in the function currently being emitted.
    pub capture_types: &'ctx [RuntimeTy],
    /// Capture slots whose cells may be touched by spawned code.
    pub spawn_capture_indices: &'ctx HashSet<usize>,
}

/// Database trait for compiler2 emit queries.
#[salsa::db]
pub trait Db: baml_compiler2_mir::Db {
    /// Mint an owned database handle that shares this database's storage, for
    /// MOVING into a worker thread.
    ///
    /// Parallel MIR lowering (Stage A of `emit_functions_parallel`) clones one
    /// handle per work chunk on the calling thread — the database type is
    /// expected to be `Send` but not `Sync`, so workers can never share `&db`.
    /// All clones share one salsa memo table (the rust-analyzer concurrency
    /// model), so a query computed by one worker is a cache hit for the rest.
    ///
    /// The default returns `None`, which keeps every salsa read on the calling
    /// thread (Stage A stays serial). `ProjectDatabase` overrides this with
    /// `Clone` (an `Arc` bump).
    fn parallel_db_handle(&self) -> Option<Box<dyn Db + Send>> {
        None
    }
}

/// Errors that can occur during bytecode generation.
#[derive(Debug)]
pub enum LoweringError {
    /// An internal invariant was violated during lowering/decomposition/linking
    /// (a compiler bug), carrying a diagnostic message.
    Internal(String),
    /// The incremental (dirty-only) reuse path cannot reuse the cached image for
    /// this project. Raised in two cases: a caller-clean file is missing from
    /// `prev_units` (a corrupt / stale cached image); or a dirty top-level `let`
    /// initializer interns a generic-function value already owned by a clean file
    /// (the design §9 R1 tail edge). Not a compiler fault: callers silently fall
    /// back to a full compile, which is byte-identical.
    ReuseUnsupported(String),
    /// The project has unresolved compile errors, so bytecode generation was
    /// not attempted. Lowering an error-bearing program would feed
    /// inference-only `Unknown`/`Error` types through the runtime-conversion
    /// boundary, which rejects them. Callers should surface the diagnostics to
    /// the user instead of treating this as a compiler fault.
    ProjectHasErrors { error_count: usize },
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal(msg) => write!(f, "internal lowering error: {msg}"),
            Self::ReuseUnsupported(msg) => {
                write!(f, "per-file bytecode reuse unsupported: {msg}")
            }
            Self::ProjectHasErrors { error_count } => write!(
                f,
                "cannot generate bytecode: project has {error_count} unresolved compile error(s)"
            ),
        }
    }
}

impl std::error::Error for LoweringError {}

pub use bex_vm_types::Program as ProgramAlias;

/// The conservative source-content identity of `root`'s program file set
/// (profiling streams spec §2.3): any byte change in any project file, or a
/// compiler version change, yields a new hash. Stdlib stubs are excluded —
/// they are a compiler-build constant already covered by the version input.
pub fn project_source_content_hash(db: &dyn crate::Db, root: baml_base::SourceRoot) -> [u8; 32] {
    // The `<builtin>/` path prefix is the wire-contract spelling of "stdlib
    // stub" (and of runtime mount stubs), the same rule `builtin_count`
    // keys on — filtering by root KIND here would diverge for databases
    // that hold both source stdlib and mount-stub roots.
    let files: Vec<(String, String)> = world_files(db, root)
        .iter()
        .filter(|file| !file.path(db).to_string_lossy().starts_with("<builtin>/"))
        .map(|file| {
            (
                file.path(db).to_string_lossy().into_owned(),
                file.text(db).clone(),
            )
        })
        .collect();
    bex_vm_types::identity::program_content_hash(
        files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_bytes())),
    )
}

/// Compute the inferred throws type for a function by querying TIR throw inference.
///
/// Returns `Some(ty)` if the function (or its callees) may throw, `None` otherwise.
fn compute_throws_type(
    db: &dyn baml_compiler2_mir::Db,
    file: baml_base::SourceFile,
    func_name: &baml_base::Name,
    cache: &RuntimeLowering<'_>,
    frame_params: &[baml_type::ParamTy],
) -> baml_compiler2_mir::TyTemplate {
    // An empty throw set is `never` — the empty error set — not an absent one.
    let never = || baml_compiler2_mir::TyTemplate::Never;
    let pkg_info = file_package(db, file);
    let pkg_id = pkg_info.root;
    let throw_sets = baml_compiler2_hir_ty::package_interface::function_throw_sets(db, pkg_id);

    let key = baml_compiler2_hir_ty::package_interface::throw_set_key(
        &pkg_info.namespace_path,
        func_name,
    );

    let Some(facts) = throw_sets.transitive_for(&key) else {
        return never();
    };
    if facts.is_empty() {
        return never();
    }

    let converted: Vec<baml_compiler2_mir::TyTemplate> = facts
        .iter()
        .map(|tir_ty| baml_compiler2_mir::tir2_to_template(tir_ty, cache, frame_params))
        .collect();

    if converted.len() == 1 {
        converted.into_iter().next().unwrap()
    } else {
        baml_compiler2_mir::TyTemplate::Union(converted.into())
    }
}

/// Stamp signature metadata onto a compiled `Function` — the single writer
/// for both top-level declarations (metadata built by
/// `compute_function_metadata`) and lambdas (metadata recorded
/// by MIR's `lower_lambda` on `MirFunction::signature`).
fn apply_signature_metadata(
    f: &mut Function,
    sig: &baml_compiler2_mir::RuntimeSignature,
    anchor: &mut refs::PackageRefs<'_, '_>,
) {
    f.param_names.clone_from(&sig.param_names);
    f.param_types = sig
        .param_types
        .iter()
        .map(|ty| anchor.anchor_template(ty))
        .collect();
    f.param_has_default.clone_from(&sig.param_has_default);
    f.return_type = anchor.anchor_template(&sig.return_type);
    f.throws_type = anchor.anchor_template(&sig.throws_type);
    f.docstring.clone_from(&sig.docstring);
    f.declared_name.clone_from(&sig.name);
    f.display_type_params.clone_from(&sig.display_type_params);
    f.generic_param_bounds = sig
        .generic_param_bounds
        .iter()
        .map(|bounds| {
            bounds
                .iter()
                .map(|bound| bex_vm_types::types::InterfaceBound {
                    interface: anchor.head(&bound.interface),
                    args: bound
                        .args
                        .iter()
                        .map(|ty| anchor.anchor_template(ty))
                        .collect(),
                    assoc: bound
                        .assoc
                        .iter()
                        .map(|(name, ty)| (name.clone(), anchor.anchor_template(ty)))
                        .collect(),
                })
                .collect()
        })
        .collect();
    f.display_param_types.clone_from(&sig.display_param_types);
    f.display_return_type.clone_from(&sig.display_return_type);
}

/// A bare single-segment path type expression for `name`.
fn type_expr_for_name(name: Name) -> TypeExpr {
    baml_compiler2_ast::TypeExprKind::Path {
        segments: vec![name],
        generic_args: Vec::new(),
        associated_type_bindings: Vec::new(),
    }
    .at(baml_compiler2_ast::TextRange::default())
}

/// `Name<P0, P1, …>` — the declaring item applied to its own parameters as type
/// variables. Only the parameter *names* matter; bounds are irrelevant to the
/// synthesized path.
fn type_expr_for_name_with_generic_args(
    name: Name,
    generic_params: &[GenericParamData],
) -> TypeExpr {
    baml_compiler2_ast::TypeExprKind::Path {
        segments: vec![name],
        generic_args: generic_params
            .iter()
            .map(|param| type_expr_for_name(param.name.clone()))
            .collect(),
        associated_type_bindings: Vec::new(),
    }
    .at(baml_compiler2_ast::TextRange::default())
}

/// Extract runtime and display signature metadata for a function, off the
/// span-free firewall (`function_data` + the enclosing owner's `*_data`).
///
/// Type resolution delegates to TIR's `lower_type_ref` (single source of truth)
/// then converts via MIR's `convert_tir_ty_for_runtime` to produce runtime `baml_type::RuntimeTy`.
/// The display fields keep generic type variables and unresolved projections
/// intact for self-documenting surfaces like `baml run --list`.
fn compute_function_metadata<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    parameter_defaults: &baml_compiler2_hir::signature::FunctionParameterDefaults,
    cache: &RuntimeLowering<'_>,
) -> baml_compiler2_mir::RuntimeSignature {
    use baml_compiler2_hir::{
        item_data::{MethodOwner, method_owner},
        type_ref::{TypeRefId, TypeRefStore},
    };
    use baml_compiler2_hir_ty::diagnostics::TirTypeError;
    use baml_type::{Ty, unify::substitute_ty};

    /// One in-scope type variable's declared bound conjunction, as `(store, id)`
    /// refs into whichever arena declared it — enclosing (class/interface/impl)
    /// and function bounds live in different arenas. Empty when unbounded.
    type BoundRef<'a> = Vec<(&'a TypeRefStore, TypeRefId)>;

    /// A declaration's parameters split into parallel name and bound-ref lists,
    /// keeping every `&`-separated conjunct.
    fn split_declared<'a>(
        params: &'a [GenericParamData],
        store: &'a TypeRefStore,
    ) -> (Vec<Name>, Vec<BoundRef<'a>>) {
        params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    param.bounds.iter().map(|&id| (store, id)).collect(),
                )
            })
            .unzip()
    }

    let vp = baml_compiler2_hir_ty::render::Viewpoint::user_facing(
        db,
        file_package(db, func_loc.file(db)).root,
    );

    let file = func_loc.file(db);
    let func = function_data(db, func_loc);
    // The arena holding this function's own signature type refs (params, return).
    let func_store = &func.type_refs;

    let param_names: Vec<String> = func.params.iter().map(|p| p.name.to_string()).collect();
    let param_has_default: Vec<bool> = parameter_defaults
        .params
        .iter()
        .map(Option::is_some)
        .collect();

    let pkg_info = file_package(db, file);
    let pkg_id = pkg_info.root;
    let _pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);

    // The item this method belongs to, via the firewall (mirrors MIR's enclosing
    // lookups; replaces the removed `method_owners`/`implements_for` flat fields).
    // Each `*_data` result carries its own `TypeRefStore` for the refs read below.
    let owner = method_owner(db, func_loc);
    let enclosing_interface_loc = match owner {
        Some(MethodOwner::Interface(iface_loc)) => Some(iface_loc),
        _ => None,
    };
    let enclosing_interface = enclosing_interface_loc.map(|loc| interface_data(db, loc));

    // For methods on generic classes/interfaces/impls, the enclosing generic
    // params are in scope inside the method signature. Mirror
    // `MirLowerer::enclosing_generic_params`: enclosing params come first, then
    // function-level params. Impl-owned methods go through the uniform
    // `impl_declared_generics` surface (the in-body-vs-free split is HIR's
    // business, not emit's).
    let (scoped_generic_param_names, scoped_generic_bound_refs): (Vec<Name>, Vec<BoundRef>) = {
        let (mut names, mut bounds) = match owner {
            Some(MethodOwner::Impl(impl_loc)) => {
                let (params, store) =
                    baml_compiler2_hir::item_data::impl_declared_generics(db, impl_loc);
                split_declared(params, store)
            }
            Some(MethodOwner::Interface(_)) => {
                let iface = enclosing_interface.expect("interface owner resolved above");
                split_declared(&iface.generic_params, &iface.type_refs)
            }
            Some(MethodOwner::Class(class_loc)) => {
                let class = class_data(db, class_loc);
                split_declared(&class.generic_params, &class.type_refs)
            }
            None => (Vec::new(), Vec::new()),
        };
        let (own_names, own_bounds) = split_declared(&func.generic_params, func_store);
        names.extend(own_names);
        bounds.extend(own_bounds);
        (names, bounds)
    };
    let enclosing_generics = baml_compiler2_hir_ty::lower::function_generic_frame(db, func_loc);

    // Every type variable in scope for this signature, with its interface
    // bounds - the one shared param env (`function_generic_bounds` covers
    // the enclosing class/interface/impl plus the function's own params;
    // the interface arm carries `Self`'s own bound — associated types are
    // not slots). Threaded into every lowering below so a
    // `T.member` projection resolves through `T`'s declared bound DURING
    // lowering.
    let scope_bounds = baml_compiler2_hir_ty::lower::function_generic_bounds(db, func_loc);

    // A method declared inside an interface resolves its associated types
    // (`Item`/`Error`) and `Self` against the rigid `Self` type variable, the same
    // way the method body does. Binding each associated-type name to the projection
    // `Self.<name>` (and `Self` to its rigid type variable) keeps a bare `Item` in a
    // signature as a faithful `Self.Item` projection rather than an unresolved type.
    // Empty for non-interface methods (the `receiver_ty` path below handles
    // class/impl receivers). Only the interface's own associated types are bound;
    // names inherited through `requires` are not (that ambiguity-aware resolution
    // lived in the removed `interface_self_projection_bindings`).
    let self_param = enclosing_generics
        .iter()
        .find(|param| param.as_str() == "Self")
        .cloned();
    let self_var = || {
        Ty::TypeVar(
            self_param
                .clone()
                .expect("interface method environment contains Self"),
        )
    };
    let interface_signature_bindings: rustc_hash::FxHashMap<ParamTy, Ty> = match enclosing_interface
    {
        Some(_) => {
            let mut bindings: rustc_hash::FxHashMap<ParamTy, Ty> = enclosing_generics
                .iter()
                .map(|p| (p.clone(), Ty::TypeVar(p.clone())))
                .collect();
            // Associated types are not frame params: signature references to
            // them already lower as `Self.X` projections, so only `Self` and
            // the identity bindings remain to install.
            bindings.insert(
                self_param
                    .clone()
                    .expect("interface method environment contains Self"),
                self_var(),
            );
            bindings
        }
        None => rustc_hash::FxHashMap::default(),
    };
    // The interface branch's generic-param scope: every name it binds (`Self`, the
    // interface's params, its associated types), so a bare `Item` lowers to
    // `TypeVar(Item)` before substitution.
    let interface_binding_params = enclosing_generics.clone();

    // The concrete receiver (`ClassName<T,…>` for a class method, or a free-impl
    // `for` target), lowered once. A non-interface method's `Self` / `Self.Assoc`
    // then root at it through the `self_ty` channel — the same projection path the
    // interface branch drives with its rigid `Self` type variable. `None` for a
    // free function (no receiver in scope) and for interface methods (which bind
    // `Self` via `interface_signature_bindings`). The class/interface receiver is a
    // synthetic `Name<params>` path (no item-tree read); the free-impl receiver is
    // its `for_target` ref, lowered from the impl block's arena.
    let receiver_ty: Option<Ty> = if enclosing_interface.is_some() {
        None
    } else {
        // `owner_self_ty` resolves both the class receiver (with the
        // builtin-container sugar) and a free impl's `for` target.
        baml_compiler2_hir_ty::lower::owner_self_ty(db, func_loc)
    };

    // Lower a signature type ref (in `store`) against this method's scope. For an
    // interface method the associated-type / `Self` bindings are applied by lowering
    // with their names in scope (so `Item` lowers to `TypeVar(Item)`) and then
    // substituting; for every other method `Self` is bound to the concrete
    // `receiver_ty` via the `self_ty` channel. In both cases `scope_bounds` drives
    // projection resolution. Namespace-relative resolution (e.g. `MyLorem` in a
    // signature under `ns_lorem/`) uses the file's namespace so a non-root-ns class
    // does not erase to `unknown`.
    let scoped_ctx = || {
        let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
            .with_bounds(scope_bounds.clone());
        if enclosing_interface.is_some() {
            ctx.with_frame(interface_binding_params.clone())
        } else {
            ctx.with_frame(enclosing_generics.clone())
                .with_self_ty(receiver_ty.clone())
                .with_impl_target(baml_compiler2_hir_ty::lower::owner_impl_target(
                    db,
                    func_loc,
                    &enclosing_generics,
                ))
        }
    };
    let lower_scoped =
        |store: &TypeRefStore, id: TypeRefId, _diags: &mut Vec<TirTypeError>| -> Ty {
            let lowered =
                baml_compiler2_hir_ty::lower::reject_holes(&scoped_ctx().lower_type_ref(store, id));
            let realized = if enclosing_interface.is_some() {
                substitute_ty(&lowered, &interface_signature_bindings)
            } else {
                lowered
            };
            // Post-substitution normalization: a projection over a ground
            // base (`(UserRepository as Repository<Record = UserRecord>)
            // .Record`) reduces to what it IS; a rigid-var base stays
            // symbolic (`(T as BoxLike).Item`).
            baml_compiler2_hir_ty::package_interface::reduce_ground_projections(db, &realized, 8)
        };

    // Each scoped generic parameter's bound as a displayable `Ty`, kept only when it
    // lowers cleanly (a bound that fails to resolve is dropped rather than shown as
    // `unknown`). A bound is a constraint head — it pins only the associated
    // members it writes, so lowering it existentially would mint completeness
    // diagnostics for members the bound legitimately leaves free (and drop the
    // bound from display). Rendered into `display_type_params` below.
    let generic_param_bounds: HashMap<Name, Vec<Ty>> = scoped_generic_param_names
        .iter()
        .enumerate()
        .filter_map(|(idx, name)| {
            let refs = scoped_generic_bound_refs.get(idx)?;
            let frame = if enclosing_interface.is_some() {
                &interface_binding_params
            } else {
                &enclosing_generics
            };
            // Each conjunct lowers independently; one that fails to lower is
            // dropped (its declaration reported the error) without hiding the
            // rest of the conjunction.
            let bound_tys: Vec<Ty> = refs
                .iter()
                .filter_map(|&(store, id)| {
                    let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
                        .with_bounds(scope_bounds.clone())
                        .with_frame(frame.clone());
                    let (lowered, diagnostics) = ctx.lower_type_ref_at_with_diagnostics(
                        store,
                        id,
                        baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
                    );
                    let lowered = baml_compiler2_hir_ty::lower::reject_holes(&lowered);
                    let clean = diagnostics.is_empty();
                    let bound_ty = if enclosing_interface.is_some() {
                        substitute_ty(&lowered, &interface_signature_bindings)
                    } else {
                        lowered
                    };
                    clean.then_some(bound_ty)
                })
                .collect();
            (!bound_tys.is_empty()).then(|| (name.clone(), bound_tys))
        })
        .collect();

    // Projection resolution is folded into `lower_scoped` (via `scope_bounds`), so
    // no separate resolve pass is needed. Diagnostics are discarded — the display
    // fields keep whatever resolved.
    let resolve_display_tir = |store: &TypeRefStore, id: TypeRefId| -> Ty {
        let mut diags = Vec::new();
        lower_scoped(store, id, &mut diags)
    };

    let display_type_params: Vec<String> = scoped_generic_param_names
        .iter()
        .map(|name| match generic_param_bounds.get(name) {
            Some(bounds) => {
                let rendered = bounds
                    .iter()
                    .map(|ty| ty.render_with(&vp))
                    .collect::<Vec<_>>()
                    .join(" & ");
                format!("{} extends {rendered}", name.as_str())
            }
            None => name.to_string(),
        })
        .collect();

    // The receiver type used for a `self` parameter with no written annotation. For
    // an interface method it is the interface at its own params, resolved through
    // the same binding path (a synthetic `Name<params>` path — no item-tree read);
    // for every other method it is the already-lowered `receiver_ty`. The interface
    // view is `Self`'s *bound*, so it lowers as a constraint head: associated
    // members stay unpinned (they realize per-receiver — a default is not a pin),
    // rather than demanding the existential's completeness.
    let self_param_ty = || -> Option<Ty> {
        match enclosing_interface {
            Some(iface) => {
                let te =
                    type_expr_for_name_with_generic_args(iface.name.clone(), &iface.generic_params);
                let mut builder = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
                let id = builder.lower(&te);
                let (store, _spans) = builder.finish();
                let lowered = baml_compiler2_hir_ty::lower::reject_holes(
                    &baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
                        .with_bounds(scope_bounds.clone())
                        .with_frame(interface_binding_params.clone())
                        .lower_type_ref_at(
                            &store,
                            id,
                            baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
                        ),
                );
                Some(substitute_ty(&lowered, &interface_signature_bindings))
            }
            None => receiver_ty.clone(),
        }
    };

    // The signature is templated over this function's own callee frame, so a
    // *value* of it reconstructs precisely by substituting the realized args it
    // carries. `frame_generic_params` is the same layout the body's `TypeArgRef`s
    // and the frames callers seed use — templating against any other list would
    // silently name the wrong types.
    let frame_params = baml_compiler2_hir_ty::lower::function_generic_frame(db, func_loc);
    let to_template = |tir_ty: &Ty| {
        // Metadata shows what the type IS: a projection over a ground base
        // (`(UserRepository as Repository<Record = UserRecord>).Record`)
        // reduces through the oracle; a rigid-var base stays symbolic
        // (`(T as BoxLike).Item`), rendered as written.
        let tir_ty =
            &baml_compiler2_hir_ty::package_interface::reduce_ground_projections(db, tir_ty, 8);
        baml_compiler2_mir::tir2_to_template(tir_ty, cache, &frame_params)
    };
    let runtime_generic_param_bounds = frame_params
        .iter()
        .map(|param| {
            scope_bounds
                .get(param)
                .into_iter()
                .flatten()
                .map(|bound| baml_compiler2_mir::RuntimeInterfaceBound {
                    interface: bound.name.clone(),
                    args: bound.generics.iter().map(to_template).collect(),
                    assoc: bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), to_template(ty)))
                        .collect(),
                })
                .collect()
        })
        .collect();
    let null_template = || baml_compiler2_mir::TyTemplate::Null;

    // Runtime parameter templates come from the ELABORATED signature — the one
    // the checker types calls against. Only the effect differs from the raw
    // spelling: a callback parameter with an omitted `throws` is opened to a
    // synthetic effect parameter that each call site instantiates, which
    // `tir2_to_template` erases to `unknown` (an open contract the host
    // boundary accepts opaquely), whereas the raw lowering falls back to
    // `never` — the spelling of an EXPLICIT `throws never`, a closed contract
    // the boundary enforces. Display keeps the raw spelling the user wrote.
    let elaborated = baml_compiler2_hir::item_data::elaborated_function_data(db, func_loc);
    debug_assert_eq!(elaborated.params.len(), func.params.len());
    let mut param_types = Vec::with_capacity(func.params.len());
    let mut display_param_types = Vec::with_capacity(func.params.len());
    for (param, elaborated_param) in func.params.iter().zip(&elaborated.params) {
        if let Some(id) = param.type_ref {
            display_param_types.push(resolve_display_tir(func_store, id).render_with(&vp));
            param_types.push(to_template(&resolve_display_tir(
                &elaborated.type_refs,
                elaborated_param.type_ref,
            )));
        } else if let Some(tir_ty) = (param.name.as_str() == "self")
            .then(self_param_ty)
            .flatten()
        {
            display_param_types.push(tir_ty.render_with(&vp));
            param_types.push(to_template(&tir_ty));
        } else {
            display_param_types.push("null".to_string());
            param_types.push(null_template());
        }
    }

    let (return_type, display_return_type) = if let Some(id) = func.return_type {
        let tir_ty = resolve_display_tir(func_store, id);
        (to_template(&tir_ty), tir_ty.render_with(&vp))
    } else {
        (null_template(), "null".to_string())
    };

    baml_compiler2_mir::RuntimeSignature {
        param_names,
        param_types,
        param_has_default,
        return_type,
        // TIR's inferred transitive throw set — richer than the declared
        // clause (a declared clause is a firewall the inference respects).
        throws_type: compute_throws_type(db, file, &func.name, cache, &frame_params),
        docstring: func.docstring.clone(),
        name: Some(func.name.to_string()),
        display_type_params,
        generic_param_bounds: runtime_generic_param_bounds,
        display_param_types,
        display_return_type,
    }
}

/// Root-relative display path for `file` (our `-trimpath`).
///
/// `Function::source_file` is display/metadata-only (backtraces, event
/// metadata, reflection locations) — never opened from disk — so stripping
/// the root keeps serialized `Program`s location-independent (a cached or
/// packed blob is byte-identical wherever the project lives) and backtraces
/// machine-independent. Only `Workspace` roots are stripped: `Stdlib` and
/// `Dependency` roots keep their paths verbatim, because their
/// `<builtin>/…` unit paths are a wire contract
/// (`bex_vm_types/src/link.rs` string-matches the prefix).
fn relative_source_path(db: &dyn baml_compiler2_mir::Db, file: baml_base::SourceFile) -> String {
    let path = file.path(db);
    let root = file.source_root(db);
    match root.kind(db) {
        baml_base::SourceRootKind::Workspace => path
            .strip_prefix(root.path(db))
            .unwrap_or(&path)
            .display()
            .to_string(),
        baml_base::SourceRootKind::Stdlib
        | baml_base::SourceRootKind::Dependency
        | baml_base::SourceRootKind::Dynamic => path.display().to_string(),
    }
}

/// Build a table of byte offsets where each line starts in the source text.
///
/// Returns `[0, offset_of_line_2, offset_of_line_3, ...]`.
#[allow(clippy::cast_possible_truncation)]
fn build_line_starts(text: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (i, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    starts
}

// ─── Let-binding helpers ─────────────────────────────────────────────────────

/// Topologically sort let bindings by their dependencies.
///
/// Walks each binding's `ExprBody` to find `Expr::Path` references to other
/// let bindings in the same package, then runs Kahn's algorithm. Returns an
/// error if circular dependencies are detected.
fn topological_sort_lets<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    bindings: &[(String, LetLoc<'db>, baml_base::SourceFile)],
) -> Result<Vec<(String, LetLoc<'db>, baml_base::SourceFile)>, LoweringError> {
    use std::collections::{HashSet, VecDeque};

    // Build adjacency list: binding[i] depends on (needs) binding[j]
    let mut deps: Vec<HashSet<usize>> = vec![HashSet::new(); bindings.len()];
    for (i, (_name, let_loc, _file)) in bindings.iter().enumerate() {
        let body = baml_compiler2_hir::body::let_body(db, *let_loc);
        if let baml_compiler2_hir::body::LetBody::Expr(expr_body) = body.as_ref() {
            // Walk all expressions to find path references to other let bindings.
            for (_expr_id, expr) in expr_body.exprs.iter() {
                if let baml_compiler2_ast::Expr::Path(segments) = expr {
                    // Single-segment paths might reference another let binding.
                    if segments.len() == 1 {
                        let ref_name_short = segments[0].as_str();
                        for (j, (fq, _, _)) in bindings.iter().enumerate() {
                            if j != i && fq.ends_with(&format!(".{ref_name_short}")) {
                                deps[i].insert(j);
                            }
                        }
                    }
                }
            }
        }
    }

    // Kahn's algorithm: if A depends on B, B must come first.
    // Build reverse edges (used_by) and in-degree (dep count).
    let mut in_degree: Vec<usize> = deps.iter().map(HashSet::len).collect();
    let mut reverse_deps: Vec<Vec<usize>> = vec![Vec::new(); bindings.len()];
    for (i, dep_set) in deps.iter().enumerate() {
        for &j in dep_set {
            reverse_deps[j].push(i);
        }
    }

    let mut queue: VecDeque<usize> = VecDeque::new();
    for (i, &deg) in in_degree.iter().enumerate() {
        if deg == 0 {
            queue.push_back(i);
        }
    }

    let mut sorted = Vec::with_capacity(bindings.len());
    while let Some(node) = queue.pop_front() {
        sorted.push(node);
        for &dependent in &reverse_deps[node] {
            in_degree[dependent] -= 1;
            if in_degree[dependent] == 0 {
                queue.push_back(dependent);
            }
        }
    }

    if sorted.len() != bindings.len() {
        return Err(LoweringError::Internal(
            "Circular dependency detected among top-level let bindings".to_string(),
        ));
    }

    Ok(sorted.into_iter().map(|i| bindings[i].clone()).collect())
}

#[derive(Clone, Default)]
struct LambdaCaptureInfo {
    capture_types: Vec<RuntimeTy>,
    spawn_capture_indices: HashSet<usize>,
}

fn unknown_capture_ty() -> RuntimeTy {
    RuntimeTy::Unknown
}

fn resolve_capture_operand_type<'db>(
    body: &MirFunctionBody<'db>,
    parent_capture_types: &[RuntimeTy],
    operand: &Operand<'db>,
) -> Option<RuntimeTy> {
    match operand {
        Operand::Constant(c) => match c {
            baml_compiler2_mir::Constant::Int(_) => Some(RuntimeTy::int()),
            baml_compiler2_mir::Constant::Bigint(_) => Some(RuntimeTy::bigint()),
            baml_compiler2_mir::Constant::Float(_) => Some(RuntimeTy::float()),
            baml_compiler2_mir::Constant::String(_) => Some(RuntimeTy::string()),
            baml_compiler2_mir::Constant::Bool(_) => Some(RuntimeTy::bool()),
            baml_compiler2_mir::Constant::Null => Some(RuntimeTy::null()),
            _ => None,
        },
        Operand::Copy(place) | Operand::Move(place) => {
            resolve_capture_place_type(body, parent_capture_types, place)
        }
    }
}

fn resolve_capture_place_type(
    body: &MirFunctionBody<'_>,
    parent_capture_types: &[RuntimeTy],
    place: &Place,
) -> Option<RuntimeTy> {
    match place {
        Place::Local(local) => body.locals.get(local.0).map(|decl| decl.ty.clone()),
        Place::Capture(idx) => parent_capture_types.get(*idx).cloned(),
        Place::Deref(CellId::Local(local)) => body.locals.get(local.0).map(|decl| decl.ty.clone()),
        Place::Deref(CellId::Capture(idx)) => parent_capture_types.get(*idx).cloned(),
        Place::Field { .. } | Place::Index { .. } => None,
    }
}

fn collect_lambda_capture_infos(
    body: &MirFunctionBody<'_>,
    lambda_count: usize,
    parent_capture_types: &[RuntimeTy],
    parent_spawn_capture_indices: &HashSet<usize>,
) -> Vec<LambdaCaptureInfo> {
    let mut infos = vec![LambdaCaptureInfo::default(); lambda_count];
    let shared_cells = memory::spawn_shared_cells(body);

    for block in &body.blocks {
        for statement in &block.statements {
            let StatementKind::Assign { value, .. } = &statement.kind else {
                continue;
            };
            let Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            } = value
            else {
                continue;
            };
            let Some(info) = infos.get_mut(*lambda_idx) else {
                continue;
            };

            info.capture_types = captures
                .iter()
                .map(|capture| {
                    resolve_capture_operand_type(body, parent_capture_types, capture)
                        .unwrap_or_else(unknown_capture_ty)
                })
                .collect();

            // A capture is shared with a task when this body spawns something
            // reaching its cell, or when it forwards one of this body's own
            // captures that an enclosing body already shares.
            for (capture_idx, capture) in captures.iter().enumerate() {
                let (Operand::Copy(place) | Operand::Move(place)) = capture else {
                    continue;
                };
                let cell = CellId::try_from(place.clone())
                    .unwrap_or_else(|_| unreachable!("a capture operand is a cell pointer"));
                let shared = shared_cells.contains(&cell)
                    || matches!(cell, CellId::Capture(idx) if parent_spawn_capture_indices.contains(&idx));
                if shared {
                    info.spawn_capture_indices.insert(capture_idx);
                }
            }
        }
    }

    infos
}

/// Identity of one Pass-4 function before MIR lowering: the [`FnWorkItem`]
/// fields known from enumeration alone (Stage A's input).
struct FnSeed {
    file: baml_base::SourceFile,
    local_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
    /// Project-relative source path (`relative_source_path`).
    source_file: String,
    /// Line index of the item's file, shared by every function in the file.
    line_starts: std::sync::Arc<[u32]>,
    is_builtin_file: bool,
}

/// Lower one seed's function to MIR on the given database handle.
///
/// `FunctionLoc` is minted on the SAME handle the lowering reads from — it is
/// a `'db`-interned key, and interning is shared storage, so every handle
/// mints the identical id (and hence the identical memo key).
fn lower_seed<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    seed: &FnSeed,
    opt: OptLevel,
) -> Result<&'db baml_compiler2_mir::MirFunction<'db>, LoweringError> {
    let function = FunctionLoc::new(db, seed.file, seed.local_id);
    lowered(db, function, lower_function(db, function, opt))
}

/// A function's MIR, or the compile failure its lowering reported: an
/// internal inconsistency leaves a function with no MIR to emit.
fn lowered<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    function: FunctionLoc<'db>,
    mir: &'db Result<baml_compiler2_mir::MirFunction<'db>, baml_compiler2_mir::MirInternalError>,
) -> Result<&'db baml_compiler2_mir::MirFunction<'db>, LoweringError> {
    mir.as_ref().map_err(|error| {
        internal_lowering_error(
            db,
            function.file(db),
            &definition_link_name(db, Definition::Function(function)),
            error,
        )
    })
}

/// The compile failure for an item of `file` whose lowering reported an
/// internal inconsistency, located by line where the inconsistency was found.
fn internal_lowering_error(
    db: &dyn baml_compiler2_mir::Db,
    file: baml_base::SourceFile,
    item: &str,
    error: &baml_compiler2_mir::MirInternalError,
) -> LoweringError {
    let path = relative_source_path(db, file);
    let at = error.span.map_or_else(
        || path.clone(),
        |span| {
            let line_starts = build_line_starts(file.text(db));
            let line = line_starts.partition_point(|&start| start <= u32::from(span.range.start()));
            format!("{path}:{line}")
        },
    );
    LoweringError::Internal(format!("lowering `{item}` ({at}): {error}"))
}

/// Stage A driver: lower every seed's function to MIR, returned in seed order.
///
/// [`lower_function`] is a tracked salsa query and all database handles share
/// one memo table — so the parallel path WARMS the memo across rayon workers
/// and then re-reads every seed on THIS thread, where each re-read is a cache
/// hit returning a reference branded with the caller's own `'db`.
///
/// The database type is `Send` but deliberately not `Sync` (each salsa handle
/// carries thread-confined query-stack state), so — exactly like
/// `baml_project`'s parallel check — one handle per chunk is cloned on THIS
/// thread and MOVED into its task; all clones share one memo table, so a
/// body lowered (or a scope inferred) by one worker is a cache hit for every
/// other. The first seed is lowered serially before fanning out to warm the
/// file/package-level memos every body read reaches. Tiny batches,
/// single-threaded pools, and databases without handles take the serial loop
/// directly.
fn lower_seed_mirs<'db>(
    db: &'db dyn crate::Db,
    seeds: &[FnSeed],
    opt: OptLevel,
) -> Result<Vec<&'db baml_compiler2_mir::MirFunction<'db>>, LoweringError> {
    // Small chunks keep rayon's work-stealing effective — bodies vary a lot
    // in inference cost — while amortizing the per-task handle clone.
    const CHUNK: usize = 4;
    // Fan-out pays for itself only past a handful of bodies (mirrors the
    // parallel-check threshold in `baml_project`).
    const MIN_PARALLEL: usize = 9;

    let read_all = || seeds.iter().map(|seed| lower_seed(db, seed, opt)).collect();

    if seeds.len() < MIN_PARALLEL || rayon::current_num_threads() <= 1 {
        return read_all();
    }
    let (first, rest) = seeds.split_first().expect("seeds checked non-empty above");

    // Handles are cloned OUTSIDE the rayon scope — a `!Sync` database cannot
    // be borrowed by the (Send) scope closure — and each chunk's handle is
    // MOVED into its task. A database that mints no handles keeps Stage A
    // serial.
    let chunks: Vec<&[FnSeed]> = rest.chunks(CHUNK).collect();
    let mut handles: Vec<Box<dyn crate::Db + Send>> = Vec::with_capacity(chunks.len());
    for _ in &chunks {
        match db.parallel_db_handle() {
            Some(handle) => handles.push(handle),
            None => return read_all(),
        }
    }

    // Warm the shared file/package-level memos before fanning out, so cold
    // workers don't all block on the same shared memo slots.
    lower_seed(db, first, opt)?;

    rayon::scope(move |s| {
        for (chunk, handle) in chunks.into_iter().zip(handles) {
            s.spawn(move |_| {
                let db: &dyn baml_compiler2_mir::Db = &*handle;
                for seed in chunk {
                    // Warming only: the re-read below reports a failure, at
                    // this thread's own `'db`.
                    let _ = lower_seed(db, seed, opt);
                }
            });
        }
    });

    // Every memo is warm: these are cache hits handed back at our own `'db`.
    read_all()
}

/// Interning table for pooled `Object::GenericFunction`s, bucketed by target
/// global slot. Mirrors the serial `emit_constant` scan's equality exactly:
/// same `function` slot and `==` on the `type_args` slice, first pooled match
/// wins.
#[derive(Default)]
struct GenericFunctionInterner {
    by_function: HashMap<usize, Vec<InternedGenericFunction>>,
}

/// One interned instantiation: its type arguments and its final pool index.
type InternedGenericFunction = (Box<[bex_vm_types::RealizedTy]>, usize);

impl GenericFunctionInterner {
    fn get(&self, gf: &bex_vm_types::GenericFunction) -> Option<usize> {
        self.by_function
            .get(&gf.function.raw())?
            .iter()
            .find(|(args, _)| args.as_ref() == gf.type_args.as_ref())
            .map(|(_, idx)| *idx)
    }

    fn insert_if_absent(&mut self, gf: &bex_vm_types::GenericFunction, idx: usize) {
        let bucket = self.by_function.entry(gf.function.raw()).or_default();
        if !bucket
            .iter()
            .any(|(args, _)| args.as_ref() == gf.type_args.as_ref())
        {
            bucket.push((gf.type_args.clone(), idx));
        }
    }
}

/// Build the callable `Function` object for a builtin, or `None` for the
/// kinds that never become callable objects: intrinsics (call sites lower to
/// `StatementKind::Intrinsic`) and BEP-034 `__await_any` (call sites lower to
/// a `Terminator::AwaitAny` suspend point).
fn builtin_emit_function(
    kind: BuiltinKind,
    fq_name: &str,
    native_path: &str,
    arity: usize,
) -> Option<Function> {
    let kind = match kind {
        BuiltinKind::Intrinsic | BuiltinKind::AwaitAny => return None,
        BuiltinKind::Io => {
            let sys_op = bex_vm_types::sys_op_for_path(native_path)
                .unwrap_or_else(|| panic!("unknown sys_op path: {native_path}"));
            FunctionKind::SysOp(sys_op)
        }
        BuiltinKind::Vm => FunctionKind::NativeUnresolved,
    };
    // `$rust_function` bodies dispatch through the codegen-produced native
    // tables, KEYED on `native_path` (`native_key_for`'s codegen-lockstep
    // spelling); `fq_name` is the display name and need not coincide.
    let native_key = matches!(kind, FunctionKind::NativeUnresolved).then(|| native_path.into());
    Some(Function {
        name: fq_name.to_string(),
        source_file: String::new(), // builtins have no source file
        docstring: None,
        declared_name: None,
        arity,
        real_local_count: 0,
        bytecode: Bytecode::default(),
        kind,
        local_names: Vec::new(),
        debug_locals: Vec::new(),
        span: Span::fake(),
        return_type: bex_vm_types::TyTemplate::Null,
        param_names: Vec::new(),
        param_types: Vec::new(),
        param_has_default: Vec::new(),
        display_type_params: Vec::new(),
        generic_param_bounds: Vec::new(),
        display_param_types: Vec::new(),
        display_return_type: "null".to_string(),
        throws_type: bex_vm_types::TyTemplate::Never,
        origin: FunctionOrigin::Builtin,
        is_interface_body: false, // set from the item tree by attach_function_metadata
        native_key,
        body_meta: None,
        capture: FunctionCaptureProps::disabled(),
        function_id: 0, // assigned at engine init (interim provider)
        runtime_package: bex_vm_types::HeapPtr::null(),
    })
}

/// Fill a compiled function's signature, throws, origin, and LLM metadata
/// from the item tree — the Pass-4 tail shared by the serial and parallel
/// passes. Every lookup here is a salsa query, so this always runs on the
/// serial control thread.
#[allow(clippy::too_many_arguments)]
fn attach_function_metadata<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    cache: &RuntimeLowering<'_>,
    is_builtin_file: bool,
    fq_name: &str,
    anchor: &mut refs::PackageRefs<'_, '_>,
    compiled_fn: &mut Function,
) {
    let func = function_data(db, func_loc);
    // Set function metadata from signature
    let parameter_defaults =
        baml_compiler2_hir::signature::function_parameter_defaults(db, func_loc);
    let signature_metadata = compute_function_metadata(db, func_loc, &parameter_defaults, cache);
    apply_signature_metadata(compiled_fn, &signature_metadata, anchor);
    compiled_fn.origin = emitted_function_origin(fq_name, is_builtin_file, func.metadata.origin);
    compiled_fn.is_interface_body = baml_compiler2_mir::function_is_interface_body(db, func_loc);

    // Set LLM-specific body_meta if this is an LLM function with a client.
    //
    // NOTE (canary merge): canary removed the runtime `Function.stream_return_type`
    // field and its plumbing (the pre-existing streaming infra from PRs #3362/#3755).
    // The stream return type is now carried by the `@stream` companion's own
    // `return_type` (see `baml_compiler2_ast`'s `companions::llm_stream`), so the
    // old emit-side pre-computation block was dropped. BEP-049 M5e stream-path rendering
    // of `ctx.output_format()` should be re-verified against canary's streaming.
    if let Some(llm_meta) = function_llm_meta(db, func_loc)
        && let Some(client) = &llm_meta.client_name
    {
        compiled_fn.body_meta = Some(FunctionMeta::Llm {
            client: client.to_string(),
        });
        compiled_fn.capture = FunctionCaptureProps::disabled()
            .with_auto(CaptureCategory::Input)
            .with_auto(CaptureCategory::Output)
            .with_auto(CaptureCategory::Error);
    }
}

/// Compile a flat list of lambda `MirFunction`s into bytecode `Function` objects
/// and register them in `objects`.  Returns a parallel `Vec<(obj_idx, name)>`
/// that can be used to build `lambda_object_indices` and `lambda_names` for the
/// parent function's `MirCodegenContext`.
///
/// "Flat" means we do NOT recurse into nested lambda children here — Phase 3
/// only supports lambdas at one level of nesting inside a top-level function.
/// Nested lambda support (lambdas inside lambdas) comes in a later phase.
#[allow(clippy::too_many_arguments)]
fn compile_lambdas<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    lambdas: &[baml_compiler2_mir::MirFunction<'db>],
    parent_body: Option<&MirFunctionBody<'db>>,
    parent_capture_types: &[RuntimeTy],
    parent_spawn_capture_indices: &HashSet<usize>,
    line_starts: &[u32],
    source_file: &str,
    refs: &mut refs::PackageRefs<'_, 'db>,
    class_fields: &ClassFieldSnapshot<'db>,
    objects: &mut ObjectPool,
    objects_base: usize,
    opt: OptLevel,
) -> Vec<(usize, String)> {
    let capture_infos = parent_body.map_or_else(
        || vec![LambdaCaptureInfo::default(); lambdas.len()],
        |body| {
            collect_lambda_capture_infos(
                body,
                lambdas.len(),
                parent_capture_types,
                parent_spawn_capture_indices,
            )
        },
    );
    let mut result = Vec::with_capacity(lambdas.len());
    for (lambda_idx, lambda) in lambdas.iter().enumerate() {
        let capture_info = capture_infos.get(lambda_idx).cloned().unwrap_or_default();
        let lambda_name = lambda.identity.link_name(db);
        let obj_idx = match &lambda.kind {
            MirFunctionKind::Bytecode(body) => {
                // Recursively compile any nested lambdas within this lambda.
                let nested_info = compile_lambdas(
                    db,
                    &lambda.lambdas,
                    Some(body),
                    &capture_info.capture_types,
                    &capture_info.spawn_capture_indices,
                    line_starts,
                    source_file,
                    &mut *refs,
                    class_fields,
                    objects,
                    objects_base,
                    opt,
                );
                let nested_obj_indices: Vec<usize> =
                    nested_info.iter().map(|(idx, _)| *idx).collect();
                let nested_names: Vec<String> =
                    nested_info.iter().map(|(_, name)| name.clone()).collect();
                let ctx = MirCodegenContext {
                    db,
                    refs: &mut *refs,
                    class_fields,
                    objects,
                    objects_base,
                    lambda_object_indices: &nested_obj_indices,
                    lambda_names: &nested_names,
                    capture_types: &capture_info.capture_types,
                    spawn_capture_indices: &capture_info.spawn_capture_indices,
                };
                let mut f =
                    compile_mir_function(body, lambda.arity, lambda.span, line_starts, ctx, opt);
                f.name.clone_from(&lambda_name);
                f.source_file = source_file.to_string();
                // Stamp the runtime signature `lower_lambda` recorded — the
                // same struct and writer as a top-level declaration (lambdas
                // have no TIR `func_data` to read from). Closure values
                // otherwise carry no signature, which BEP-062's
                // `reflect.signature` / `reflect.call_any` consume.
                if let Some(sig) = &lambda.signature {
                    apply_signature_metadata(&mut f, sig, &mut *refs);
                }
                let idx = objects_base + objects.len();
                objects.push(Object::Function(Box::new(f)));
                idx
            }
            MirFunctionKind::Builtin(_) => {
                // Builtins can't be lambdas — skip.
                continue;
            }
        };
        result.push((obj_idx, lambda_name));
    }
    result
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
        sync::atomic::{AtomicU32, Ordering},
    };

    use baml_base::{FileId, SourceFile, SourceRoot, SourceRootKind, SourceRootTable};
    use baml_compiler2_ast::parse_string_attr_value;
    use salsa::Setter;

    use super::*;

    #[salsa::db]
    pub(crate) struct TestDb {
        storage: salsa::Storage<TestDb>,
        next_file_id: AtomicU32,
        /// Present from construction (`Default` fills them in immediately).
        roots: Option<SourceRootTable>,
        workspace: Option<SourceRoot>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            let mut db = Self {
                storage: salsa::Storage::default(),
                next_file_id: AtomicU32::new(0),
                roots: None,
                workspace: None,
            };
            let workspace = SourceRoot::new(
                &db,
                PathBuf::from("."),
                SourceRootKind::Workspace,
                None,
                Vec::new(),
                None,
                Vec::new(),
            );
            db.roots = Some(SourceRootTable::new(&db, vec![workspace]));
            db.workspace = Some(workspace);
            db
        }
    }

    impl Clone for TestDb {
        fn clone(&self) -> Self {
            Self {
                storage: self.storage.clone(),
                next_file_id: AtomicU32::new(self.next_file_id.load(Ordering::SeqCst)),
                roots: self.roots,
                workspace: self.workspace,
            }
        }
    }

    impl TestDb {
        /// The workspace root every test file lands on.
        pub(crate) fn workspace(&self) -> SourceRoot {
            self.workspace
                .expect("workspace root present from construction")
        }

        /// Add a workspace file (registered on the workspace root).
        pub(crate) fn add_file(&mut self, path: impl Into<PathBuf>, content: &str) -> SourceFile {
            let file_id = FileId::new(self.next_file_id.fetch_add(1, Ordering::SeqCst));
            let root = self
                .workspace
                .expect("workspace root present from construction");
            let file =
                SourceFile::new(self, content.to_string(), path.into(), file_id, false, root);
            let mut files = root.files(self).clone();
            files.push(file);
            root.set_files(self).to(files);
            file
        }
    }

    /// Classes the codegen unit tests build aggregates of.
    pub(crate) const CLASSES_FOR_TESTS: &str =
        "class Box<T> {\n  item T\n}\nclass GuideHooks {\n  a int\n  b int\n}\n";

    /// The declaration of the class named `name` in `file`, for a test that
    /// needs a real [`ClassRef`] behind a MIR aggregate.
    pub(crate) fn class_ref<'db>(
        db: &'db TestDb,
        file: SourceFile,
        name: &str,
    ) -> baml_compiler2_hir_ty::extern_loc::ClassRef<'db> {
        let baml_compiler2_hir::contributions::Definition::Class(class) =
            baml_compiler2_hir::package::package_items(
                db,
                baml_compiler2_hir::file_package::file_package(db, file).root,
            )
            .lookup_type(&[], &Name::new(name))
            .unwrap_or_else(|| panic!("test source declares `{name}`"))
        else {
            panic!("`{name}` is not a class")
        };
        baml_compiler2_hir::loc::DeclRef::Source(class)
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl baml_compiler2_hir::Db for TestDb {
        fn source_roots(&self) -> SourceRootTable {
            self.roots.expect("root table present from construction")
        }
    }

    #[salsa::db]
    impl baml_compiler2_mir::Db for TestDb {}

    #[salsa::db]
    impl Db for TestDb {}

    // ── parse_string_attr_value ─────────────────────────────────────────

    #[test]
    fn parse_regular_string() {
        assert_eq!(
            parse_string_attr_value(r#""hello world""#),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn parse_regular_string_with_escapes() {
        assert_eq!(
            parse_string_attr_value(r#""line\nbreak""#),
            Some("line\nbreak".to_string())
        );
        assert_eq!(
            parse_string_attr_value(r#""tab\tstop""#),
            Some("tab\tstop".to_string())
        );
        assert_eq!(
            parse_string_attr_value(r#""a\\b""#),
            Some(r"a\b".to_string())
        );
        assert_eq!(
            parse_string_attr_value(r#""a\"b""#),
            Some(r#"a"b"#.to_string())
        );
    }

    #[test]
    fn parse_single_quoted_string() {
        assert_eq!(
            parse_string_attr_value("'hello world'"),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn parse_empty_regular_string() {
        assert_eq!(parse_string_attr_value(r#""""#), Some(String::new()));
    }

    #[test]
    fn removed_hash_string_returns_none() {
        assert_eq!(parse_string_attr_value("#\"raw text\"#"), None);
        assert_eq!(parse_string_attr_value("##\"raw text\"##"), None);
        assert_eq!(parse_string_attr_value("#\"\"#"), None);
    }

    #[test]
    fn parse_non_string_returns_none() {
        assert_eq!(parse_string_attr_value("vm"), None);
        assert_eq!(parse_string_attr_value("42"), None);
        assert_eq!(parse_string_attr_value("true"), None);
    }

    #[test]
    fn parse_malformed_returns_none() {
        // Unclosed quote: just a bare "
        assert_eq!(parse_string_attr_value("\"unclosed"), None);
    }

    #[test]
    fn function_metadata_reports_defaulted_params() {
        // A real parsed function, not a fabricated item tree: the firewall
        // queries this flows through (`function_in_scope_generic_param_bounds`
        // → `function_data`) are total over Locs minted from a real tree, and
        // panic on ids that were never allocated.
        let mut db = TestDb::default();
        let file = db.add_file(
            "test.baml",
            "function f(required: int, with_default: int = 1, also_required: int) -> int { 1 }",
        );

        let func_loc = baml_compiler2_hir::item_data::file_functions(&db, file)
            .iter()
            .copied()
            .find(|&loc| {
                baml_compiler2_hir::item_data::function_data(&db, loc)
                    .name
                    .as_str()
                    == "f"
            })
            .expect("test file declares `f`");
        let parameter_defaults =
            baml_compiler2_hir::signature::function_parameter_defaults(&db, func_loc);
        let aliases = baml_compiler2_mir::ResolvedAliases {
            aliases: HashMap::new(),
            recursive: HashSet::new(),
        };
        let cache = RuntimeLowering {
            aliases: &aliases,
            spelling: baml_compiler2_hir::package::spelling(&db),
            db: &db,
        };

        let metadata = compute_function_metadata(&db, func_loc, &parameter_defaults, &cache);

        assert_eq!(metadata.param_has_default, vec![false, true, false]);
    }

    // ── build_interface_def ─────────────────────────────────────────────

    /// Build the runtime signature for the single interface declared in `source`.
    fn interface_def_for(source: &str, name: &str) -> bex_vm_types::types::InterfaceDef {
        let mut db = TestDb::default();
        let file = db.add_file("test.baml", source);

        let iface_loc = baml_compiler2_hir::item_data::file_interfaces(&db, file)
            .iter()
            .copied()
            .find(|&loc| {
                baml_compiler2_hir::item_data::interface_data(&db, loc)
                    .name
                    .as_str()
                    == name
            })
            .expect("test file declares the interface");
        let aliases = baml_compiler2_mir::ResolvedAliases {
            aliases: HashMap::new(),
            recursive: HashSet::new(),
        };
        let cache = RuntimeLowering {
            aliases: &aliases,
            spelling: baml_compiler2_hir::package::spelling(&db),
            db: &db,
        };
        let root = file_package(&db, file).root;
        let mut refs = refs::PackageRefs::new(&db, root, refs::Own::Tail);
        items::build_interface_def(
            &db,
            iface_loc,
            &baml_type::DeclName::in_root(root, Vec::new(), baml_base::Name::new(name)),
            refs::unit_tag(0),
            &cache,
            &mut refs,
        )
    }

    /// A declaration is described symbolically — `Self.Item` stays an associated
    /// projection rather than being dropped for want of a receiver to substitute.
    #[test]
    fn interface_def_keeps_self_projections() {
        let def = interface_def_for(
            concat!(
                "interface Src {\n",
                "  type Item\n",
                "  function next(self) -> Self.Item throws never\n",
                "}\n",
            ),
            "Src",
        );
        let next = def
            .methods
            .iter()
            .find(|m| m.name.as_str() == "next")
            .expect("`next` is declared");
        assert!(
            matches!(
                next.returns,
                bex_vm_types::RuntimeTy::AssociatedTypeProjection { .. }
            ),
            "expected a projection return, got {:?}",
            next.returns
        );
    }

    /// Every declared parameter occupies its position: a signature's positional
    /// layout is only meaningful if no parameter can silently vanish from it.
    #[test]
    fn interface_def_keeps_every_parameter_position() {
        let def = interface_def_for(
            concat!(
                "interface Sink<T> {\n",
                "  type Item\n",
                "  function put(self, first: Self.Item, second: T, third: int) -> int throws never\n",
                "}\n",
            ),
            "Sink",
        );
        let put = def
            .methods
            .iter()
            .find(|m| m.name.as_str() == "put")
            .expect("`put` is declared");
        assert_eq!(
            put.args.len(),
            3,
            "the `self` receiver drops, the other three stay: {:?}",
            put.args
        );
        assert!(matches!(put.args[2], bex_vm_types::RuntimeTy::Int));
    }

    /// A `requires` clause is recorded even when it projects through `Self`.
    #[test]
    fn interface_def_keeps_self_projecting_requires() {
        let def = interface_def_for(
            concat!(
                "interface Base {\n",
                "  type Item\n",
                "  function b(self) -> int throws never\n",
                "}\n",
                "interface Derived requires Base<Item = Self.Item> {\n",
                "  type Item\n",
                "  function d(self) -> int throws never\n",
                "}\n",
            ),
            "Derived",
        );
        assert_eq!(
            def.requires.len(),
            1,
            "the `requires` clause must survive lowering: {:?}",
            def.requires
        );
    }
}
