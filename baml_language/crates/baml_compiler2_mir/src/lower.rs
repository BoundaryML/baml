use std::collections::{HashMap, HashSet};

use baml_base::{Name, TypePath};
use baml_type::{MediaKind, Ty, TyAttr, TyTemplate, TypeName};
use indexmap::IndexMap;

use crate::{
    builder::MirBuilder,
    ir::{
        AggregateKind, BasicBlock, BinOp, BlockId, CatchRegion, Constant, IndexKind, IntrinsicOp,
        ItemRef, Local, LocalDecl, LogLevel, MirFunction, MirFunctionBody, MirFunctionKind,
        Operand, Place, Rvalue, StatementKind, Terminator,
    },
    optimize,
};

/// Classifies what kind of switch a match/catch expression lowers to.
///
/// `Integer` and `EnumDiscriminant` are currently implemented.
/// `TypeTag` dispatches class-type and primitive-type match arms via runtime
/// type tags, using `Rvalue::TypeTag` for the switch operand.
enum SwitchKind {
    Integer,
    EnumDiscriminant(Name),
    TypeTag,
}

/// What happens in the otherwise block of a switch.
#[derive(Clone, Copy)]
enum SwitchOtherwise {
    /// Match expression: goto join (non-exhaustive) or unreachable (exhaustive).
    Match { is_exhaustive: bool },
    /// Catch expression: rethrow unmatched errors.
    /// If `needs_throw_if_panic` is true, insert a `throw_if_panic` guard before wildcard body.
    Catch {
        error_local: Local,
        needs_throw_if_panic: bool,
    },
}

struct LoopContext {
    break_target: BlockId,
    continue_target: BlockId,
    watched_locals_depth: usize,
}

struct CatchContext {
    unwind_target: BlockId,
    error_local: Local,
}

// ─── Type conversion: TIR Ty → baml_type::Ty ────────────────────────────────

use baml_compiler2_tir::ty::{FunctionParamMode, PrimitiveType, QualifiedTypeName, Ty as Tir2Ty};

pub fn qtn_to_type_name(qtn: &QualifiedTypeName) -> TypeName {
    let module_path = std::iter::once(qtn.package().clone())
        .chain(qtn.namespace().iter().cloned())
        .collect::<Vec<_>>();
    // For user-defined types (package = "user"), display with only the local
    // namespace path so snapshots show `Point` rather than `user.Point`.
    // For builtin types (package = "baml", etc.), keep the full FQ path.
    let display_name = if qtn.package().as_str() == "user" {
        let parts: Vec<_> = qtn
            .namespace()
            .iter()
            .map(std::string::ToString::to_string)
            .chain(std::iter::once(qtn.name().to_string()))
            .collect();
        smol_str::SmolStr::new(parts.join("."))
    } else {
        smol_str::SmolStr::new(qtn.to_string())
    };
    TypeName {
        name: qtn.name().clone(),
        module_path,
        display_name,
    }
}

/// Pre-computed type alias data for inline expansion in `convert_tir2_ty`.
///
/// Bundles the alias map and recursion info that are always passed together.
pub struct ResolvedAliases {
    pub aliases: HashMap<QualifiedTypeName, Tir2Ty>,
    pub recursive: HashSet<QualifiedTypeName>,
}

impl ResolvedAliases {
    /// Build resolved aliases for a package, including dependency packages.
    pub fn for_package(db: &dyn crate::Db, pkg_id: baml_compiler2_hir::package::PackageId) -> Self {
        use baml_compiler2_hir::package::{package_dependencies, package_items};

        let pkg_items = package_items(db, pkg_id);
        let mut aliases = baml_compiler2_tir::inference::collect_type_aliases(db, pkg_items);
        for &dep_id in package_dependencies(db, pkg_id) {
            let dep_items = package_items(db, dep_id);
            aliases.extend(baml_compiler2_tir::inference::collect_type_aliases(
                db, dep_items,
            ));
        }
        let recursive = baml_compiler2_tir::normalize::find_recursive_aliases(&aliases);
        Self { aliases, recursive }
    }

    /// Convert a TIR type to `baml_type::Ty` using the cached alias data.
    pub fn convert(&self, ty: &Tir2Ty) -> Ty {
        convert_tir2_ty(ty, self)
    }
}

pub fn convert_tir2_ty(ty: &Tir2Ty, resolved: &ResolvedAliases) -> Ty {
    let attr = ty.attr().clone();
    match ty {
        // Primitives
        Tir2Ty::Primitive(PrimitiveType::Int, attr) => Ty::Int { attr: attr.clone() },
        Tir2Ty::Primitive(PrimitiveType::Float, attr) => Ty::Float { attr: attr.clone() },
        Tir2Ty::Primitive(PrimitiveType::String, attr) => Ty::String { attr: attr.clone() },
        Tir2Ty::Primitive(PrimitiveType::Bool, attr) => Ty::Bool { attr: attr.clone() },
        Tir2Ty::Primitive(PrimitiveType::Null, attr) => Ty::Null { attr: attr.clone() },
        Tir2Ty::Primitive(PrimitiveType::Uint8Array, attr) => Ty::Uint8Array { attr: attr.clone() },
        Tir2Ty::Primitive(PrimitiveType::Image, attr) => Ty::Media(MediaKind::Image, attr.clone()),
        Tir2Ty::Primitive(PrimitiveType::Audio, attr) => Ty::Media(MediaKind::Audio, attr.clone()),
        Tir2Ty::Primitive(PrimitiveType::Video, attr) => Ty::Media(MediaKind::Video, attr.clone()),
        Tir2Ty::Primitive(PrimitiveType::Pdf, attr) => Ty::Media(MediaKind::Pdf, attr.clone()),

        // Named types
        Tir2Ty::Class(qtn, type_args, attr) => {
            let resolved_args: Vec<Ty> = type_args
                .iter()
                .map(|a| convert_tir2_ty(a, resolved))
                .collect();
            Ty::Class(qtn_to_type_name(qtn), resolved_args, attr.clone())
        }
        Tir2Ty::Enum(qtn, attr) => Ty::Enum(qtn_to_type_name(qtn), attr.clone()),
        Tir2Ty::TypeAlias(qtn, attr) => {
            if resolved.recursive.contains(qtn) {
                // Keep recursive aliases opaque — they need runtime resolution
                Ty::TypeAlias(qtn_to_type_name(qtn), attr.clone())
            } else if let Some(target) = resolved.aliases.get(qtn) {
                // Expand non-recursive aliases inline
                convert_tir2_ty(target, resolved)
            } else {
                // Unknown alias (e.g. from another package) — keep opaque
                Ty::TypeAlias(qtn_to_type_name(qtn), attr.clone())
            }
        }

        // EnumVariant → preserve variant-level type info
        Tir2Ty::EnumVariant(qtn, variant, attr) => {
            Ty::EnumVariant(qtn_to_type_name(qtn), variant.clone(), attr.clone())
        }

        // Containers
        Tir2Ty::List(inner, attr) => {
            Ty::List(Box::new(convert_tir2_ty(inner, resolved)), attr.clone())
        }
        Tir2Ty::Map(k, v, attr) => Ty::Map {
            key: Box::new(convert_tir2_ty(k, resolved)),
            value: Box::new(convert_tir2_ty(v, resolved)),
            attr: attr.clone(),
        },
        Tir2Ty::Union(members, attr) => Ty::Union(
            members
                .iter()
                .map(|m| convert_tir2_ty(m, resolved))
                .collect(),
            attr.clone(),
        ),
        Tir2Ty::Optional(inner, attr) => {
            Ty::Optional(Box::new(convert_tir2_ty(inner, resolved)), attr.clone())
        }
        Tir2Ty::Literal(lit, _freshness, attr) => Ty::Literal(lit.clone(), attr.clone()),

        // Evolving containers → freeze to regular containers
        Tir2Ty::EvolvingList(inner, attr) => {
            Ty::List(Box::new(convert_tir2_ty(inner, resolved)), attr.clone())
        }
        Tir2Ty::EvolvingMap(k, v, attr) => Ty::Map {
            key: Box::new(convert_tir2_ty(k, resolved)),
            value: Box::new(convert_tir2_ty(v, resolved)),
            attr: attr.clone(),
        },

        // Functions — drop param names
        Tir2Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|param| convert_tir2_ty(&param.ty, resolved))
                .collect(),
            ret: Box::new(convert_tir2_ty(ret, resolved)),
            throws: Box::new(convert_tir2_ty(throws, resolved)),
            attr: attr.clone(),
        },

        // Bottom / sentinel types
        Tir2Ty::Never { attr } => Ty::Void { attr: attr.clone() },
        Tir2Ty::Void { attr } => Ty::Void { attr: attr.clone() },
        Tir2Ty::BuiltinUnknown { attr } => Ty::BuiltinUnknown { attr: attr.clone() },
        Tir2Ty::RustType { attr } => {
            // RustType is an opaque sentinel — map to Opaque with a synthetic name
            Ty::Opaque(
                TypeName {
                    name: Name::new("RustType"),
                    module_path: vec![Name::new("baml"), Name::new("rust")],
                    display_name: Name::new("RustType"),
                },
                attr.clone(),
            )
        }
        Tir2Ty::Type { attr } => {
            // The `type` metatype maps to the same opaque representation as v1.
            // See Ty::type_type() in baml_type/src/lib.rs.
            Ty::Opaque(
                TypeName {
                    name: Name::new("Type"),
                    module_path: vec![Name::new("baml"), Name::new("reflect")],
                    display_name: Name::new("type"),
                },
                attr.clone(),
            )
        }
        Tir2Ty::Unknown { attr } => Ty::Void { attr: attr.clone() }, // error recovery
        Tir2Ty::Error { attr } => Ty::Void { attr: attr.clone() },   // error recovery
        // Demonstration-only hardcode for `baml.llm.Stream<TStream, TFinal>`'s
        // typevars (see thoughts/sam-projects/bridge-python/21d…).
        // The two stdlib names lower to `Ty::BuiltinUnknown` so the
        // host-driven streaming smokes can prove the rest of the streaming
        // wiring is intact, without yet solving general generics-through-FFI.
        // `BuiltinUnknown` is the engine's "any value matches" sentinel at
        // the FFI boundary (see `value_matches_type` in
        // `bex_engine/src/conversion.rs`), so a `string` return for
        // `Stream.next() -> TStream | StreamFinished` decodes as the
        // `TStream` arm of the union.
        // Every other TypeVar still falls through to the defensive Ty::Void.
        Tir2Ty::TypeVar(name, _) if name.as_str() == "TStream" || name.as_str() == "TFinal" => {
            Ty::BuiltinUnknown { attr }
        }
        // TypeVar should never reach MIR — it is erased to Unknown before VIR.
        // Map defensively to Void as error recovery.
        Tir2Ty::TypeVar(..) => Ty::Void { attr },
        // BEP-034: future types pass through unchanged with both
        // value and error type parameters mapped.
        Tir2Ty::Future(value, error, attr) => Ty::Future(
            Box::new(convert_tir2_ty(value, resolved)),
            Box::new(convert_tir2_ty(error, resolved)),
            attr.clone(),
        ),
    }
}

// ─── Ty → TyTemplate conversion for already-resolved Ty values ──────────────

/// Convert an already-resolved `baml_type::Ty` back to a `TyTemplate`.
///
/// This is needed for `IsType` pattern-matching where the pattern type comes
/// through `convert_tir2_ty` (so `TypeVars` are already erased), but we still
/// need a `TyTemplate` to carry class-level type args for the VM to compare
/// against `Instance::class_type_args`.
///
/// For all leaf types that aren't `Ty::Class`, the result is
/// `TyTemplate::Concrete(ty)`.  For `Ty::Class(tn, args, _)` we produce
/// `TyTemplate::Class(tn, args.map(Concrete))` so the VM can compare the
/// resolved args against the instance's `class_type_args`.
///
/// Note: by the time `emit_is_type_branch` is called, any `TypeVars` in the
/// pattern have already been resolved to concrete types — so no
/// `generic_params` are needed.  If future patterns introduce `TypeVars` that
/// survive to MIR, thread `enclosing_generic_params()` through here.
/// Convert a `Tir2Ty` to `TyTemplate`, mapping any `TypeVar(name)` whose
/// `name` appears at position `N` in `generic_params` to `TypeArgRef(N)`.
///
/// Free function counterpart to `MirLowerer::ty_to_template`, exposed so
/// that callers outside of MIR (e.g. `baml_compiler2_emit`'s class-field
/// type lowering) can build the same templates.
pub fn tir2_to_template(
    ty: &Tir2Ty,
    resolved: &ResolvedAliases,
    generic_params: &[baml_base::Name],
) -> TyTemplate {
    match ty {
        Tir2Ty::TypeVar(name, _) => {
            if let Some(n) = generic_params.iter().position(|p| p == name) {
                TyTemplate::TypeArgRef(u32::try_from(n).expect("generic param index fits in u32"))
            } else {
                TyTemplate::Concrete(Ty::Void {
                    attr: baml_type::TyAttr::default(),
                })
            }
        }
        Tir2Ty::List(inner, _) => {
            TyTemplate::Array(Box::new(tir2_to_template(inner, resolved, generic_params)))
        }
        Tir2Ty::Optional(inner, _) => {
            TyTemplate::Optional(Box::new(tir2_to_template(inner, resolved, generic_params)))
        }
        Tir2Ty::Map(k, v, _) => TyTemplate::Map(
            Box::new(tir2_to_template(k, resolved, generic_params)),
            Box::new(tir2_to_template(v, resolved, generic_params)),
        ),
        Tir2Ty::Union(parts, _) => TyTemplate::Union(
            parts
                .iter()
                .map(|p| tir2_to_template(p, resolved, generic_params))
                .collect(),
        ),
        Tir2Ty::Class(qtn, type_args, attr) => {
            if type_args
                .iter()
                .any(baml_compiler2_tir::generics::contains_typevar)
            {
                let template_args: Vec<TyTemplate> = type_args
                    .iter()
                    .map(|a| tir2_to_template(a, resolved, generic_params))
                    .collect();
                TyTemplate::Class(qtn_to_type_name(qtn), template_args)
            } else {
                let resolved_args: Vec<Ty> = type_args
                    .iter()
                    .map(|a| convert_tir2_ty(a, resolved))
                    .collect();
                TyTemplate::Concrete(Ty::Class(
                    qtn_to_type_name(qtn),
                    resolved_args,
                    attr.clone(),
                ))
            }
        }
        Tir2Ty::EvolvingList(inner, _) => {
            TyTemplate::Array(Box::new(tir2_to_template(inner, resolved, generic_params)))
        }
        Tir2Ty::EvolvingMap(k, v, _) => TyTemplate::Map(
            Box::new(tir2_to_template(k, resolved, generic_params)),
            Box::new(tir2_to_template(v, resolved, generic_params)),
        ),
        other => TyTemplate::Concrete(convert_tir2_ty(other, resolved)),
    }
}

pub(crate) fn ty_to_template_from_resolved_ty(ty: &Ty) -> TyTemplate {
    match ty {
        Ty::Class(tn, args, _) if !args.is_empty() => {
            // Parametric class: produce TyTemplate::Class with Concrete leaves.
            // This allows the VM to check `expected_args == inst.class_type_args`.
            TyTemplate::Class(
                tn.clone(),
                args.iter().map(ty_to_template_from_resolved_ty).collect(),
            )
        }
        // All other types: wrap in Concrete.  The VM uses this for the
        // existing fast paths (primitive type tags, monomorphic classes).
        other => TyTemplate::Concrete(other.clone()),
    }
}

// ─── def_to_item_ref helper ──────────────────────────────────────────────────

use baml_compiler2_hir::{
    compiler2_all_files, contributions::Definition, file_package::file_package,
};
// Use the PPIR item tree (which includes synthetic *$stream items) rather than
// the bare HIR item tree. TIR resolves methods using PPIR `LocalItemId`s, so
// MIR must use the same tree to avoid index mismatches.
use baml_compiler2_ppir::file_item_tree;

pub fn def_to_item_ref<'db>(db: &'db dyn crate::Db, def: Definition<'db>) -> ItemRef {
    let file = def.file(db);
    let pkg_info = file_package(db, file);
    let item_tree = file_item_tree(db, file);

    let name: Name = match def {
        Definition::Function(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Class(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Enum(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::TypeAlias(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::TemplateString(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Client(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Generator(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Test(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::RetryPolicy(loc) => item_tree[loc.id(db)].name.clone(),
        Definition::Let(loc) => item_tree[loc.id(db)].name.clone(),
    };

    // For function definitions, check if this is a class method by searching
    // the item tree's class methods lists.
    if let Definition::Function(func_loc) = def {
        let func_local_id = func_loc.id(db);
        for class_data in item_tree.classes.values() {
            if class_data.methods.contains(&func_local_id) {
                return ItemRef::Method {
                    package: pkg_info.package.clone(),
                    namespace: pkg_info.namespace_path,
                    class: class_data.name.clone(),
                    name,
                };
            }
        }
    }

    ItemRef::Free {
        package: pkg_info.package.clone(),
        namespace: pkg_info.namespace_path,
        name,
    }
}

/// Convert a `MemberResolution` (from TIR) into an `ItemRef` (for MIR).
///
/// Only `Method` and `Free` variants are callable — callers must guard against
/// `Field` and `Variant` variants before calling this function.
fn resolution_to_item_ref(
    db: &dyn crate::Db,
    res: &baml_compiler2_tir::inference::MemberResolution<'_>,
) -> Option<ItemRef> {
    use baml_compiler2_tir::inference::MemberResolution;
    match res {
        MemberResolution::Free { func_loc } => {
            let pkg_info = file_package(db, func_loc.file(db));
            let item_tree = file_item_tree(db, func_loc.file(db));
            let func_data = &item_tree[func_loc.id(db)];
            Some(ItemRef::Free {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                name: func_data.name.clone(),
            })
        }
        MemberResolution::BoundMethod {
            class_loc,
            func_loc,
        }
        | MemberResolution::UnboundMethod {
            class_loc,
            func_loc,
        } => {
            let pkg_info = file_package(db, class_loc.file(db));
            let item_tree = file_item_tree(db, class_loc.file(db));
            let class_data = &item_tree[class_loc.id(db)];
            let func_id = func_loc.id(db);
            let func_data = &item_tree[func_id];
            Some(ItemRef::Method {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                class: class_data.name.clone(),
                name: func_data.name.clone(),
            })
        }
        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
    }
}

// ─── LoweringContext ─────────────────────────────────────────────────────────

// Re-use ExprId from baml_compiler2_ast (already imported above via ExprId)
use baml_compiler2_ast::{
    AssignOp as AstAssignOp, AstSourceMap, BinaryOp as AstBinaryOp, Expr as AstExpr,
    ExprBody as AstExprBody, ExprId as AstExprId, Literal as AstLiteral, PatId as AstPatId,
    Pattern as AstPattern, Stmt as AstStmt, StmtId as AstStmtId, TypeExpr as AstTypeExpr,
    UnaryOp as AstUnaryOp,
};
use baml_compiler2_hir::{
    body::{FunctionBody, LetBody, let_body, let_body_source_map},
    loc::{FunctionLoc, LetLoc},
    package::{PackageId, package_dependencies, package_items},
    scope::FileScopeId,
    semantic_index::{BindingId, DefinitionSite},
};
use baml_compiler2_ppir::file_semantic_index;
use baml_compiler2_tir::{
    inference::infer_scope_types,
    resolve::{ResolvedName, resolve_name_at_in_scope},
};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MetadataScope {
    Body(FileScopeId),
    ParameterDefault(FileScopeId),
}

type ExprMetadataKey = (MetadataScope, AstExprId);
type PatMetadataKey = (MetadataScope, AstPatId);

struct LoweringContext<'db> {
    db: &'db dyn crate::Db,
    builder: MirBuilder,
    locals: HashMap<Name, Local>,
    binding_locals: HashMap<BindingId, Local>,
    loop_context: Option<LoopContext>,
    catch_context: Option<CatchContext>,
    exit_block: BlockId,

    // Eagerly aggregated type maps from all scopes in the function.
    // Keyed by metadata namespace plus arena ID to avoid collisions between
    // function bodies, lambda bodies, and parameter-default arenas.
    expr_types: FxHashMap<ExprMetadataKey, Tir2Ty>,
    pat_types: FxHashMap<PatMetadataKey, Tir2Ty>,
    // Member resolutions from TIR.
    resolutions: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::MemberResolution<'db>>,
    // Match expressions that TIR determined are exhaustive
    exhaustive_matches: rustc_hash::FxHashSet<ExprMetadataKey>,
    // TIR-inferred root segment type for each multi-segment Path expression.
    // Used by lower_multi_segment_path_as_field_chain to get the correct root
    // type even when the MIR local was declared with a coarser type (e.g.
    // catch variables declared as BuiltinUnknown).
    path_root_types: FxHashMap<ExprMetadataKey, Tir2Ty>,
    // TIR-inferred type of every prefix `segments[..=seg_idx]` for multi-segment
    // local-rooted Path expressions. Used by the Phase-8 method-call prepend to
    // read the receiver-prefix type (`segments[..segments.len() - 1]`) so that
    // class-level type args are threaded correctly through depth ≥ 3 paths
    // like `holder.box.describe()`.
    path_segment_types: FxHashMap<(MetadataScope, AstExprId, usize), Tir2Ty>,
    // Per-segment member resolutions for multi-segment local-rooted Path expressions.
    // Set by TIR's infer_local_rooted_path; indexed by (scope, Path ExprId).
    // path_member_resolutions[(scope, expr_id)][i] is the resolution for segments[i+1].
    path_member_resolutions:
        FxHashMap<ExprMetadataKey, Vec<baml_compiler2_tir::inference::MemberResolution<'db>>>,
    // Full-arity argument binding plans from TIR.
    call_plans: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::CallPlan>,
    // Function-value adapters from TIR checked coercions.
    function_coercions: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::FunctionCoercion>,

    // The FileScopeId of the expression body currently being lowered.
    // Updated when descending into lambda bodies (Phase 3+).
    current_scope: FileScopeId,
    // Metadata namespace for the expression arena currently being lowered.
    current_metadata_scope: MetadataScope,

    // AST expression body and source map
    body: AstExprBody,
    source_map: Option<AstSourceMap>,
    file: baml_base::SourceFile,
    func_loc: Option<FunctionLoc<'db>>,
    /// Raw function name from the item tree (e.g. `"Foo$render_prompt"`).
    /// Used to disambiguate companion scopes that share the same span.
    scope_func_name: Option<Name>,

    // Schema maps built from PackageItems.
    // class_fields and class_type_tags are keyed by TypeName (name + module_path)
    // so that e.g. baml.http.Request and a user-defined Request are distinct.
    // enum_variants is keyed by Name (short name only) because match-arm lowering
    // (PatternKind::EnumVariant) only provides the enum's short Name, not a full
    // TypeName with module_path. Upgrading to TypeName would require resolving the
    // enum's package at each match site.
    class_fields: IndexMap<TypeName, IndexMap<String, usize>>,
    class_field_types: IndexMap<TypeName, IndexMap<String, Ty>>,
    enum_variants: IndexMap<Name, IndexMap<String, usize>>,
    /// Pre-computed type tags for class types, used by `SwitchKind::TypeTag`
    /// for union-type switch optimization (ported from MIR 1).
    class_type_tags: IndexMap<TypeName, i64>,

    // Pre-computed type alias data for inline expansion in convert_tir2_ty
    resolved_aliases: ResolvedAliases,

    watched_locals_stack: Vec<Local>,

    // Counter for generating unique synthetic variable names (e.g. __for_idx, __for_idx_1)
    synthetic_name_counts: HashMap<String, usize>,

    // Lambda functions lowered during body traversal.
    // Collected here and moved into MirFunction.lambdas at the end of lowering.
    // Each entry is a fully-lowered MirFunction for one lambda expression.
    pending_lambdas: Vec<MirFunction>,

    // Capture map for the current lambda body.
    // `Some(map)` when lowering inside a lambda body; `None` for top-level functions.
    // Maps captured binding identity -> index into the closure's captures array.
    // Used by `lower_path_expr` to resolve references to captured variables as
    // `Place::Capture(idx)` instead of `Place::Local(_)`.
    capture_indices: Option<HashMap<BindingId, usize>>,

    // Bindings that were added to the current lambda's capture list transitively
    // because an inner lambda needed them but they were not in the HIR capture
    // list for this lambda. Collected by the parent `lower_lambda` call after
    // the body is lowered so it can extend the outer MakeClosure with extra captures.
    transitive_captures_needed: Vec<BindingId>,

    /// Stack of null-exit blocks for active `OptionalChain` scopes.
    /// When an `OptionalFieldAccess`/`OptionalIndex`/`OptionalCall` encounters null,
    /// it jumps to the top of this stack instead of creating its own null block.
    chain_null_exits: Vec<BlockId>,

    /// Optimization level controlling MIR-level transforms.
    /// At `OptLevel::Two`, constant folding and advanced transforms are applied.
    opt: crate::OptLevel,
}

impl<'db> LoweringContext<'db> {
    /// Populate `class_fields` and `enum_variants` from a single package's items.
    ///
    /// Note: `class_type_tags` is built separately via `build_class_type_tags` to ensure
    /// the same file-iteration order as the emitter (`generate_project_bytecode`).
    fn populate_from_package(
        db: &'db dyn crate::Db,
        pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
        pkg_name: &Name,
        class_fields: &mut IndexMap<TypeName, IndexMap<String, usize>>,
        class_field_types: &mut IndexMap<TypeName, IndexMap<String, Ty>>,
        enum_variants: &mut IndexMap<Name, IndexMap<String, usize>>,
        resolved_aliases: &ResolvedAliases,
    ) {
        for (ns_names, ns) in &pkg_items.namespaces {
            // Build module_path: [pkg_name] ++ ns_names
            let mut module_path: Vec<Name> = vec![pkg_name.clone()];
            module_path.extend(ns_names.iter().cloned());

            for def in ns.types.values() {
                match def {
                    Definition::Class(class_loc) => {
                        let cfile = class_loc.file(db);
                        let citree = file_item_tree(db, cfile);
                        let class_data = &citree[class_loc.id(db)];

                        let tn = TypeName {
                            name: class_data.name.clone(),
                            module_path: module_path.clone(),
                            display_name: class_data.name.clone(),
                        };

                        let mut fields = IndexMap::new();
                        let mut field_types = IndexMap::new();
                        let pkg_ns = baml_compiler2_hir::file_package::file_package(db, cfile)
                            .namespace_path;
                        let mut diags = Vec::new();
                        for (idx, field) in class_data.fields.iter().enumerate() {
                            fields.insert(field.name.to_string(), idx);
                            let field_ty = field
                                .type_expr
                                .as_ref()
                                .map(|te| {
                                    let tir_ty =
                                        baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                                            db,
                                            &te.expr,
                                            pkg_items,
                                            &pkg_ns,
                                            &class_data.generic_params,
                                            &mut diags,
                                        );
                                    resolved_aliases.convert(&tir_ty)
                                })
                                .unwrap_or(Ty::Null {
                                    attr: TyAttr::default(),
                                });
                            field_types.insert(field.name.to_string(), field_ty);
                        }
                        class_fields.insert(tn.clone(), fields);
                        class_field_types.insert(tn, field_types);
                    }
                    Definition::Enum(enum_loc) => {
                        let efile = enum_loc.file(db);
                        let eitree = file_item_tree(db, efile);
                        let enum_data = &eitree[enum_loc.id(db)];

                        let mut variants = IndexMap::new();
                        for (idx, variant) in enum_data.variants.iter().enumerate() {
                            variants.insert(variant.name.to_string(), idx);
                        }
                        enum_variants.insert(enum_data.name.clone(), variants);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Build `class_type_tags` by iterating `compiler2_all_files` in the same order as the
    /// emitter (`generate_project_bytecode` in `baml_compiler2_emit`). This guarantees that
    /// the integer type tags stored in Switch arms exactly match the `class.type_tag` values
    /// assigned to runtime Class objects.
    fn build_class_type_tags(db: &'db dyn crate::Db) -> IndexMap<TypeName, i64> {
        let all_files = compiler2_all_files(db);
        let mut class_type_tags: IndexMap<TypeName, i64> = IndexMap::new();
        let mut class_type_tag_counter = 0i64;

        for file in &all_files {
            let item_tree = file_item_tree(db, *file);
            let pkg_info = file_package(db, *file);

            // Build module_path: [package] ++ namespace_path
            let mut module_path: Vec<Name> = vec![pkg_info.package.clone()];
            module_path.extend(pkg_info.namespace_path.iter().cloned());

            for class_data in item_tree.classes.values() {
                let tn = TypeName {
                    name: class_data.name.clone(),
                    module_path: module_path.clone(),
                    display_name: class_data.name.clone(),
                };
                let type_tag = baml_type::typetag::CLASS_BASE + class_type_tag_counter;
                class_type_tag_counter += 1;
                // Use entry to avoid overwriting if the same class appears via multiple paths
                // (e.g., both FQ and short names). First encounter wins — consistent with emit.rs.
                class_type_tags.entry(tn).or_insert(type_tag);
            }
        }

        class_type_tags
    }

    fn new(
        db: &'db dyn crate::Db,
        func_loc: FunctionLoc<'db>,
        expr_body: AstExprBody,
        source_map: Option<AstSourceMap>,
        opt: crate::OptLevel,
    ) -> Self {
        let file = func_loc.file(db);

        // --- Resolve FunctionLoc → FileScopeId via span ---
        let item_tree = file_item_tree(db, file);
        let func_data = &item_tree[func_loc.id(db)];
        let func_span = func_data.span;

        let index = file_semantic_index(db, file);
        // For synthesized functions whose span is `0..0` (e.g. `$init_test_N`),
        // `scope_at_offset` may return a descendant Lambda scope instead of the
        // Function scope itself, because all synthesized expressions share span
        // `0..0` and the descendant search finds a matching lambda first.
        // Avoid this by searching explicitly for a `ScopeKind::Function` scope
        // with the correct name and span before falling back to `scope_at_offset`.
        let func_scope_id: FileScopeId = index
            .scopes
            .iter()
            .enumerate()
            .find_map(|(i, scope)| {
                if scope.kind == baml_compiler2_hir::scope::ScopeKind::Function
                    && scope.range == func_span
                    && scope.name.as_ref() == Some(&func_data.name)
                {
                    #[allow(clippy::cast_possible_truncation)]
                    Some(FileScopeId::new(i as u32))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| index.scope_at_offset(func_span.start(), Some(&func_data.name)));

        // --- Eagerly aggregate expr_types, pat_types, resolutions, exhaustive_matches, path_root_types, and path_member_resolutions from all scopes ---
        let mut expr_types: FxHashMap<ExprMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut pat_types: FxHashMap<PatMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut resolutions: FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::MemberResolution<'db>,
        > = FxHashMap::default();
        let mut exhaustive_matches: rustc_hash::FxHashSet<ExprMetadataKey> =
            rustc_hash::FxHashSet::default();
        let mut path_root_types: FxHashMap<ExprMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut path_segment_types: FxHashMap<(MetadataScope, AstExprId, usize), Tir2Ty> =
            FxHashMap::default();
        let mut path_member_resolutions: FxHashMap<
            ExprMetadataKey,
            Vec<baml_compiler2_tir::inference::MemberResolution<'db>>,
        > = FxHashMap::default();
        let mut call_plans: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::CallPlan> =
            FxHashMap::default();
        let mut function_coercions: FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::FunctionCoercion,
        > = FxHashMap::default();

        let merge_scope = |fsi: FileScopeId,
                           expr_types: &mut FxHashMap<ExprMetadataKey, Tir2Ty>,
                           pat_types: &mut FxHashMap<PatMetadataKey, Tir2Ty>,
                           resolutions: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::MemberResolution<'db>,
        >,
                           exhaustive_matches: &mut rustc_hash::FxHashSet<ExprMetadataKey>,
                           path_root_types: &mut FxHashMap<ExprMetadataKey, Tir2Ty>,
                           path_segment_types: &mut FxHashMap<
            (MetadataScope, AstExprId, usize),
            Tir2Ty,
        >,
                           path_member_resolutions: &mut FxHashMap<
            ExprMetadataKey,
            Vec<baml_compiler2_tir::inference::MemberResolution<'db>>,
        >,
                           call_plans: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::CallPlan,
        >,
                           function_coercions: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::FunctionCoercion,
        >| {
            let scope_id = index.scope_ids[fsi.index() as usize];
            let inference = infer_scope_types(db, scope_id);
            let body_scope = MetadataScope::Body(fsi);
            for (&expr_id, ty) in inference.iter_expressions() {
                expr_types.insert((body_scope, expr_id), ty.clone());
            }
            for (&pat_id, ty) in inference.iter_bindings() {
                pat_types.insert((body_scope, pat_id), ty.clone());
            }
            for (&expr_id, res) in inference.iter_resolutions() {
                resolutions.insert((body_scope, expr_id), res.clone());
            }
            for &expr_id in inference.iter_exhaustive_matches() {
                exhaustive_matches.insert((body_scope, expr_id));
            }
            for (&expr_id, ty) in inference.iter_path_root_types() {
                path_root_types.insert((body_scope, expr_id), ty.clone());
            }
            for (&(expr_id, seg_idx), ty) in inference.iter_path_segment_types() {
                path_segment_types.insert((body_scope, expr_id, seg_idx), ty.clone());
            }
            for (&expr_id, member_resolutions) in inference.iter_path_member_resolutions() {
                path_member_resolutions.insert((body_scope, expr_id), member_resolutions.clone());
            }
            for (&expr_id, plan) in inference.iter_call_plans() {
                call_plans.insert((body_scope, expr_id), plan.clone());
            }
            for (&expr_id, coercion) in inference.iter_function_coercions() {
                function_coercions.insert((body_scope, expr_id), coercion.clone());
            }

            let default_scope = MetadataScope::ParameterDefault(fsi);
            for (&expr_id, ty) in inference.iter_default_expressions() {
                expr_types.insert((default_scope, expr_id), ty.clone());
            }
            for (&pat_id, ty) in inference.iter_default_bindings() {
                pat_types.insert((default_scope, pat_id), ty.clone());
            }
            for (&expr_id, res) in inference.iter_default_resolutions() {
                resolutions.insert((default_scope, expr_id), res.clone());
            }
            for &expr_id in inference.iter_default_exhaustive_matches() {
                exhaustive_matches.insert((default_scope, expr_id));
            }
            for (&expr_id, ty) in inference.iter_default_path_root_types() {
                path_root_types.insert((default_scope, expr_id), ty.clone());
            }
            for (&(expr_id, seg_idx), ty) in inference.iter_default_path_segment_types() {
                path_segment_types.insert((default_scope, expr_id, seg_idx), ty.clone());
            }
            for (&expr_id, member_resolutions) in inference.iter_default_path_member_resolutions() {
                path_member_resolutions
                    .insert((default_scope, expr_id), member_resolutions.clone());
            }
            for (&expr_id, plan) in inference.iter_default_call_plans() {
                call_plans.insert((default_scope, expr_id), plan.clone());
            }
            for (&expr_id, coercion) in inference.iter_default_function_coercions() {
                function_coercions.insert((default_scope, expr_id), coercion.clone());
            }
        };

        // Include the function scope itself
        merge_scope(
            func_scope_id,
            &mut expr_types,
            &mut pat_types,
            &mut resolutions,
            &mut exhaustive_matches,
            &mut path_root_types,
            &mut path_segment_types,
            &mut path_member_resolutions,
            &mut call_plans,
            &mut function_coercions,
        );

        // Include all descendant scopes (blocks, lambdas, etc.)
        let func_scope = &index.scopes[func_scope_id.index() as usize];
        let desc_start = func_scope.descendants.start.index();
        let desc_end = func_scope.descendants.end.index();
        for raw_idx in desc_start..desc_end {
            merge_scope(
                FileScopeId::new(raw_idx),
                &mut expr_types,
                &mut pat_types,
                &mut resolutions,
                &mut exhaustive_matches,
                &mut path_root_types,
                &mut path_segment_types,
                &mut path_member_resolutions,
                &mut call_plans,
                &mut function_coercions,
            );
        }

        // --- Build class_fields / enum_variants from PackageItems ---
        let pkg_info = file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let resolved_aliases = ResolvedAliases::for_package(db, pkg_id);

        let mut class_fields: IndexMap<TypeName, IndexMap<String, usize>> = IndexMap::new();
        let mut class_field_types: IndexMap<TypeName, IndexMap<String, Ty>> = IndexMap::new();
        let mut enum_variants: IndexMap<Name, IndexMap<String, usize>> = IndexMap::new();

        // Include classes from dependency packages first (e.g., "baml" builtins).
        // Inserted first so current-package classes take priority on collision.
        for &dep_id in package_dependencies(db, pkg_id) {
            let dep_items = package_items(db, dep_id);
            let dep_name = dep_id.name(db);
            Self::populate_from_package(
                db,
                dep_items,
                &dep_name,
                &mut class_fields,
                &mut class_field_types,
                &mut enum_variants,
                &resolved_aliases,
            );
        }

        // Include classes from the current package (overwrites on collision).
        let pkg_items = package_items(db, pkg_id);
        Self::populate_from_package(
            db,
            pkg_items,
            &pkg_info.package,
            &mut class_fields,
            &mut class_field_types,
            &mut enum_variants,
            &resolved_aliases,
        );

        // Build class_type_tags using the same file-iteration order as the emitter,
        // so that switch arms get the same integer tags as runtime class.type_tag fields.
        let class_type_tags = Self::build_class_type_tags(db);

        // --- Determine arity from function signature ---
        let sig = baml_compiler2_ppir::function_signature(db, func_loc);
        let arity = sig.params.len();

        // Detect if this function is a class method by checking the parent scope.
        // If so, qualify the function name as "ClassName.MethodName".
        let func_scope = &index.scopes[func_scope_id.index() as usize];
        let func_name = if let Some(parent_idx) = func_scope.parent {
            let parent = &index.scopes[parent_idx.index() as usize];
            if matches!(parent.kind, baml_compiler2_hir::scope::ScopeKind::Class) {
                if let Some(ref class_name) = parent.name {
                    Name::new(format!(
                        "{}.{}",
                        class_name.as_str(),
                        func_data.name.as_str()
                    ))
                } else {
                    func_data.name.clone()
                }
            } else {
                func_data.name.clone()
            }
        } else {
            func_data.name.clone()
        };

        LoweringContext {
            db,
            builder: MirBuilder::new(func_name, arity),
            locals: HashMap::new(),
            binding_locals: HashMap::new(),
            loop_context: None,
            catch_context: None,
            exit_block: BlockId(0), // placeholder; overwritten in lower_function_body
            expr_types,
            pat_types,
            resolutions,
            exhaustive_matches,
            path_root_types,
            path_segment_types,
            path_member_resolutions,
            call_plans,
            function_coercions,
            current_scope: func_scope_id,
            current_metadata_scope: MetadataScope::Body(func_scope_id),
            body: expr_body,
            source_map,
            file,
            func_loc: Some(func_loc),
            scope_func_name: Some(func_data.name.clone()),
            class_fields,
            class_field_types,
            enum_variants,
            class_type_tags,
            pending_lambdas: Vec::new(),
            capture_indices: None,
            transitive_captures_needed: Vec::new(),
            resolved_aliases,
            watched_locals_stack: Vec::new(),
            synthetic_name_counts: HashMap::new(),
            chain_null_exits: Vec::new(),
            opt,
        }
    }

    /// Create a lowering context for a top-level let binding.
    ///
    /// The let binding has no parameters — arity 0, no `func_loc`.
    /// Type information is gathered from the `ScopeKind::Let` scope.
    fn new_for_let(
        db: &'db dyn crate::Db,
        let_loc: LetLoc<'db>,
        expr_body: AstExprBody,
        source_map: Option<AstSourceMap>,
        opt: crate::OptLevel,
    ) -> Self {
        let file = let_loc.file(db);

        // --- Resolve LetLoc → FileScopeId via span ---
        let item_tree = file_item_tree(db, file);
        let let_data = &item_tree[let_loc.id(db)];
        let let_span = let_data.span;
        let let_name = let_data.name.clone();

        let index = file_semantic_index(db, file);
        let let_scope_id: FileScopeId = index.scope_at_offset(let_span.start(), Some(&let_name));

        // --- Eagerly aggregate expr_types, pat_types, resolutions, path_root_types, path_member_resolutions from let scope ---
        let mut expr_types: FxHashMap<ExprMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut pat_types: FxHashMap<PatMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut resolutions: FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::MemberResolution<'db>,
        > = FxHashMap::default();
        let mut exhaustive_matches: rustc_hash::FxHashSet<ExprMetadataKey> =
            rustc_hash::FxHashSet::default();
        let mut path_root_types: FxHashMap<ExprMetadataKey, Tir2Ty> = FxHashMap::default();
        let mut path_segment_types: FxHashMap<(MetadataScope, AstExprId, usize), Tir2Ty> =
            FxHashMap::default();
        let mut path_member_resolutions: FxHashMap<
            ExprMetadataKey,
            Vec<baml_compiler2_tir::inference::MemberResolution<'db>>,
        > = FxHashMap::default();
        let mut call_plans: FxHashMap<ExprMetadataKey, baml_compiler2_tir::inference::CallPlan> =
            FxHashMap::default();
        let mut function_coercions: FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::FunctionCoercion,
        > = FxHashMap::default();

        let merge_scope = |fsi: FileScopeId,
                           expr_types: &mut FxHashMap<ExprMetadataKey, Tir2Ty>,
                           pat_types: &mut FxHashMap<PatMetadataKey, Tir2Ty>,
                           resolutions: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::MemberResolution<'db>,
        >,
                           exhaustive_matches: &mut rustc_hash::FxHashSet<ExprMetadataKey>,
                           path_root_types: &mut FxHashMap<ExprMetadataKey, Tir2Ty>,
                           path_segment_types: &mut FxHashMap<
            (MetadataScope, AstExprId, usize),
            Tir2Ty,
        >,
                           path_member_resolutions: &mut FxHashMap<
            ExprMetadataKey,
            Vec<baml_compiler2_tir::inference::MemberResolution<'db>>,
        >,
                           call_plans: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::CallPlan,
        >,
                           function_coercions: &mut FxHashMap<
            ExprMetadataKey,
            baml_compiler2_tir::inference::FunctionCoercion,
        >| {
            let scope_id = index.scope_ids[fsi.index() as usize];
            let inference = infer_scope_types(db, scope_id);
            let body_scope = MetadataScope::Body(fsi);
            for (&expr_id, ty) in inference.iter_expressions() {
                expr_types.insert((body_scope, expr_id), ty.clone());
            }
            for (&pat_id, ty) in inference.iter_bindings() {
                pat_types.insert((body_scope, pat_id), ty.clone());
            }
            for (&expr_id, res) in inference.iter_resolutions() {
                resolutions.insert((body_scope, expr_id), res.clone());
            }
            for &expr_id in inference.iter_exhaustive_matches() {
                exhaustive_matches.insert((body_scope, expr_id));
            }
            for (&expr_id, ty) in inference.iter_path_root_types() {
                path_root_types.insert((body_scope, expr_id), ty.clone());
            }
            for (&(expr_id, seg_idx), ty) in inference.iter_path_segment_types() {
                path_segment_types.insert((body_scope, expr_id, seg_idx), ty.clone());
            }
            for (&expr_id, member_resolutions) in inference.iter_path_member_resolutions() {
                path_member_resolutions.insert((body_scope, expr_id), member_resolutions.clone());
            }
            for (&expr_id, plan) in inference.iter_call_plans() {
                call_plans.insert((body_scope, expr_id), plan.clone());
            }
            for (&expr_id, coercion) in inference.iter_function_coercions() {
                function_coercions.insert((body_scope, expr_id), coercion.clone());
            }

            let default_scope = MetadataScope::ParameterDefault(fsi);
            for (&expr_id, ty) in inference.iter_default_expressions() {
                expr_types.insert((default_scope, expr_id), ty.clone());
            }
            for (&pat_id, ty) in inference.iter_default_bindings() {
                pat_types.insert((default_scope, pat_id), ty.clone());
            }
            for (&expr_id, res) in inference.iter_default_resolutions() {
                resolutions.insert((default_scope, expr_id), res.clone());
            }
            for &expr_id in inference.iter_default_exhaustive_matches() {
                exhaustive_matches.insert((default_scope, expr_id));
            }
            for (&expr_id, ty) in inference.iter_default_path_root_types() {
                path_root_types.insert((default_scope, expr_id), ty.clone());
            }
            for (&(expr_id, seg_idx), ty) in inference.iter_default_path_segment_types() {
                path_segment_types.insert((default_scope, expr_id, seg_idx), ty.clone());
            }
            for (&expr_id, member_resolutions) in inference.iter_default_path_member_resolutions() {
                path_member_resolutions
                    .insert((default_scope, expr_id), member_resolutions.clone());
            }
            for (&expr_id, plan) in inference.iter_default_call_plans() {
                call_plans.insert((default_scope, expr_id), plan.clone());
            }
            for (&expr_id, coercion) in inference.iter_default_function_coercions() {
                function_coercions.insert((default_scope, expr_id), coercion.clone());
            }
        };

        // Include the let scope itself
        merge_scope(
            let_scope_id,
            &mut expr_types,
            &mut pat_types,
            &mut resolutions,
            &mut exhaustive_matches,
            &mut path_root_types,
            &mut path_segment_types,
            &mut path_member_resolutions,
            &mut call_plans,
            &mut function_coercions,
        );

        // Include all descendant scopes (blocks, closures within the initializer)
        let let_scope = &index.scopes[let_scope_id.index() as usize];
        let desc_start = let_scope.descendants.start.index();
        let desc_end = let_scope.descendants.end.index();
        for raw_idx in desc_start..desc_end {
            merge_scope(
                FileScopeId::new(raw_idx),
                &mut expr_types,
                &mut pat_types,
                &mut resolutions,
                &mut exhaustive_matches,
                &mut path_root_types,
                &mut path_segment_types,
                &mut path_member_resolutions,
                &mut call_plans,
                &mut function_coercions,
            );
        }

        // --- Build class_fields / enum_variants from PackageItems ---
        let pkg_info = file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let resolved_aliases = ResolvedAliases::for_package(db, pkg_id);

        let mut class_fields: IndexMap<TypeName, IndexMap<String, usize>> = IndexMap::new();
        let mut class_field_types: IndexMap<TypeName, IndexMap<String, Ty>> = IndexMap::new();
        let mut enum_variants: IndexMap<Name, IndexMap<String, usize>> = IndexMap::new();

        // Include classes from dependency packages first.
        for &dep_id in package_dependencies(db, pkg_id) {
            let dep_items = package_items(db, dep_id);
            let dep_name = dep_id.name(db);
            Self::populate_from_package(
                db,
                dep_items,
                &dep_name,
                &mut class_fields,
                &mut class_field_types,
                &mut enum_variants,
                &resolved_aliases,
            );
        }

        // Include classes from the current package (overwrites on collision).
        let pkg_items = package_items(db, pkg_id);
        Self::populate_from_package(
            db,
            pkg_items,
            &pkg_info.package,
            &mut class_fields,
            &mut class_field_types,
            &mut enum_variants,
            &resolved_aliases,
        );

        // Build class_type_tags using the same file-iteration order as the emitter,
        // so that switch arms get the same integer tags as runtime class.type_tag fields.
        let class_type_tags = Self::build_class_type_tags(db);

        LoweringContext {
            db,
            builder: MirBuilder::new(let_name.clone(), 0),
            locals: HashMap::new(),
            binding_locals: HashMap::new(),
            loop_context: None,
            catch_context: None,
            exit_block: BlockId(0), // placeholder; overwritten in lower_let_body_inner
            expr_types,
            pat_types,
            resolutions,
            exhaustive_matches,
            path_root_types,
            path_segment_types,
            path_member_resolutions,
            call_plans,
            function_coercions,
            current_scope: let_scope_id,
            current_metadata_scope: MetadataScope::Body(let_scope_id),
            body: expr_body,
            source_map,
            file,
            func_loc: None,
            scope_func_name: Some(let_name),
            class_fields,
            class_field_types,
            enum_variants,
            class_type_tags,
            resolved_aliases,
            watched_locals_stack: Vec::new(),
            synthetic_name_counts: HashMap::new(),
            pending_lambdas: Vec::new(),
            capture_indices: None,
            transitive_captures_needed: Vec::new(),
            chain_null_exits: Vec::new(),
            opt,
        }
    }

    /// Generate a unique synthetic variable name, e.g. __`for_idx`, __`for_idx_1`, __`for_idx_2`.
    fn gensym(&mut self, prefix: &str) -> Name {
        let count = self
            .synthetic_name_counts
            .entry(prefix.to_string())
            .or_insert(0);
        let name = if *count == 0 {
            prefix.to_string()
        } else {
            format!("{prefix}_{count}")
        };
        *count += 1;
        Name::new(&name)
    }

    fn scope_is_descendant_or_self(
        index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
        scope_id: FileScopeId,
        ancestor_id: FileScopeId,
    ) -> bool {
        let mut current = Some(scope_id);
        while let Some(id) = current {
            if id == ancestor_id {
                return true;
            }
            current = index.scopes[id.index() as usize].parent;
        }
        false
    }

    fn binding_id_for_pattern_site_name(
        &self,
        pattern: AstPatId,
        site: DefinitionSite,
        name: &Name,
    ) -> Option<BindingId> {
        let index = file_semantic_index(self.db, self.file);
        let pattern_span = self
            .source_map
            .as_ref()
            .map(|source_map| source_map.pattern_span(pattern));

        for (scope_idx, bindings) in index.scope_bindings.iter().enumerate() {
            let scope_id = FileScopeId::new(u32::try_from(scope_idx).expect("scope id overflow"));
            if !Self::scope_is_descendant_or_self(index, scope_id, self.current_scope) {
                continue;
            }
            for (binding_idx, binding) in bindings.bindings.iter().enumerate() {
                let pattern_matches_name_range = pattern_span.is_none_or(|span| {
                    span == binding.name_range
                        || (span.start() <= binding.name_range.start()
                            && binding.name_range.end() <= span.end())
                });
                if binding.site == site
                    && binding.pattern == pattern
                    && binding.name == *name
                    && pattern_matches_name_range
                {
                    return Some(BindingId::local(scope_id, binding_idx));
                }
            }
        }
        None
    }

    fn any_pattern_binding_is_captured(&self, pattern: AstPatId, site: DefinitionSite) -> bool {
        let index = file_semantic_index(self.db, self.file);
        for (scope_idx, bindings) in index.scope_bindings.iter().enumerate() {
            let scope_id = FileScopeId::new(u32::try_from(scope_idx).expect("scope id overflow"));
            if !Self::scope_is_descendant_or_self(index, scope_id, self.current_scope) {
                continue;
            }
            for (binding_idx, binding) in bindings.bindings.iter().enumerate() {
                if binding.site == site && binding.pattern == pattern {
                    let binding_id = BindingId::local(scope_id, binding_idx);
                    if bindings.captured_bindings.contains(&binding_id) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn binding_id_for_statement_name(
        &self,
        stmt_id: AstStmtId,
        pattern: AstPatId,
        name: &Name,
    ) -> Option<BindingId> {
        self.binding_id_for_pattern_site_name(pattern, DefinitionSite::Statement(stmt_id), name)
    }

    fn record_pattern_binding_local(&mut self, pattern: AstPatId, name: &Name, local: Local) {
        if let Some(binding_id) = self.binding_id_for_pattern_site_name(
            pattern,
            DefinitionSite::PatternBinding(pattern),
            name,
        ) {
            self.binding_locals.insert(binding_id, local);
        }
    }

    fn pattern_binding_is_captured(&self, pattern: AstPatId) -> bool {
        self.any_pattern_binding_is_captured(pattern, DefinitionSite::PatternBinding(pattern))
    }

    fn binding_id_for_name_at(&self, expr_id: AstExprId, name: &Name) -> Option<BindingId> {
        let index = file_semantic_index(self.db, self.file);
        let (scope_id, offset) = if let Some(source_map) = self.source_map.as_ref() {
            let offset = source_map.expr_span(expr_id).start();
            (
                index.scope_at_offset(offset, self.scope_func_name.as_ref()),
                offset,
            )
        } else {
            // The source-map-less branch is only valid for **synthesized**
            // expressions emitted by the lowering itself (e.g. for-loop index
            // increments, capture forwarding, init function bodies). The
            // fallback uses `current_scope` and the scope's end offset, which
            // is correct for synthesized refs at the end of the current scope
            // but would silently pick the post-shadow binding for a
            // user-written name lowered without a source map.
            //
            // If you find yourself adding a user-visible expression that
            // hits this path: the right fix is to thread a `BindingId`
            // through to the call site, not to widen this fallback.
            let scope_id = self.current_scope;
            let offset = index.scopes[scope_id.index() as usize].range.end();
            (scope_id, offset)
        };
        index.visible_binding_at(scope_id, offset, name)
    }

    fn capture_index_for_name_at(&self, expr_id: AstExprId, name: &Name) -> Option<usize> {
        let binding_id = self.binding_id_for_name_at(expr_id, name)?;
        self.capture_indices
            .as_ref()
            .and_then(|captures| captures.get(&binding_id).copied())
    }

    /// Emit `unwatch` ops for every watched local at index `[watched_depth..]`
    /// of `watched_locals_stack`, in reverse declaration order.
    ///
    /// This is the single emitter for unwatch sequences. All scope-exit
    /// paths go through it:
    ///   - normal block fallthrough: `lower_scoped_block` (depth = entry stack len)
    ///   - normal `for`-body fallthrough (depth = entry stack len)
    ///   - normal match/catch arm-body fallthrough (depth = arm-entry stack len)
    ///   - `break` / `continue` (depth = `loop_context.watched_locals_depth`)
    ///   - `return` / `throw` (depth = 0 — the stack is swapped at lambda
    ///     boundaries, so 0 means "everything in the enclosing function")
    ///
    /// Does NOT truncate the stack — callers that own the scope are
    /// responsible for truncating via `restore_locals_after_scope`. Divergent
    /// callers (break/continue/return/throw) leave the stack alone because a
    /// dead block follows the divergent terminator.
    fn emit_unwatch_to_depth(&mut self, watched_depth: usize) {
        let watched = self.watched_locals_stack[watched_depth..].to_vec();
        for local in watched.into_iter().rev() {
            self.builder.unwatch(local);
        }
    }

    fn restore_locals_after_scope(
        &mut self,
        saved_locals: HashMap<Name, Local>,
        watched_depth: usize,
    ) {
        self.watched_locals_stack.truncate(watched_depth);
        self.locals = saved_locals;
    }

    fn restore_active_locals(&mut self, saved_locals: HashMap<Name, Local>) {
        self.locals = saved_locals;
    }

    fn mark_captured_locals_in_scope_tree(&mut self, root_scope: FileScopeId) {
        let index = file_semantic_index(self.db, self.file);
        let root = &index.scopes[root_scope.index() as usize];
        let start = root_scope.index();
        let end = root.descendants.end.index();

        for raw_idx in start..end {
            let scope_id = FileScopeId::new(raw_idx);
            let Some(scope_bindings) = index.scope_bindings.get(scope_id.index() as usize) else {
                continue;
            };
            for binding_id in &scope_bindings.captured_bindings {
                if let Some(&local) = self.binding_locals.get(binding_id) {
                    self.builder.local_decl_mut(local).is_captured = true;
                }
            }
        }
    }

    /// Get the `baml_type::Ty` for an expression by looking up in the aggregated map
    /// and converting from TIR Ty. Uses `current_metadata_scope` as the arena namespace.
    fn expr_metadata_key(&self, expr_id: AstExprId) -> ExprMetadataKey {
        (self.current_metadata_scope, expr_id)
    }

    fn pat_metadata_key(&self, pat_id: AstPatId) -> PatMetadataKey {
        (self.current_metadata_scope, pat_id)
    }

    fn expr_ty(&self, expr_id: AstExprId) -> Ty {
        self.expr_types
            .get(&self.expr_metadata_key(expr_id))
            .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
            .unwrap_or(Ty::Void {
                attr: TyAttr::default(),
            })
    }

    /// Compute the `TyTemplate` slice for the class-level type args of a class
    /// construction expression.
    ///
    /// Returns `vec![]` for non-generic (or unresolved) classes.
    fn class_type_arg_templates(&self, expr_id: AstExprId) -> Vec<TyTemplate> {
        let generic_params = self.enclosing_generic_params();
        match self.expr_types.get(&self.expr_metadata_key(expr_id)) {
            Some(Tir2Ty::Class(_, type_args, _)) if !type_args.is_empty() => type_args
                .iter()
                .map(|t| self.ty_to_template(t, &generic_params))
                .collect(),
            _ => vec![],
        }
    }

    /// Get the `baml_type::Ty` for a pattern binding
    fn pat_ty(&self, pat_id: AstPatId) -> Ty {
        self.pat_types
            .get(&self.pat_metadata_key(pat_id))
            .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
            .unwrap_or(Ty::Void {
                attr: TyAttr::default(),
            })
    }

    fn is_pattern_type_recovery(ty: &Ty) -> bool {
        matches!(ty, Ty::Void { .. } | Ty::BuiltinUnknown { .. })
    }

    /// Get the TIR-inferred root segment type for a multi-segment Path expression.
    /// Returns `None` if no root type was recorded (e.g. single-segment paths).
    fn path_root_ty(&self, expr_id: AstExprId) -> Option<Ty> {
        self.path_root_types
            .get(&self.expr_metadata_key(expr_id))
            .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
    }

    /// Get the TIR-inferred type of `segments[..=seg_idx]` for a multi-segment
    /// local-rooted Path expression. Returns `None` if not recorded.
    #[allow(dead_code)]
    fn path_segment_ty(&self, expr_id: AstExprId, seg_idx: usize) -> Option<Ty> {
        self.path_segment_types
            .get(&(self.current_metadata_scope, expr_id, seg_idx))
            .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
    }

    /// Resolve a `TypeExpr` annotation directly to a `baml_type::Ty`.
    /// Used for `TypedBinding` patterns where TIR may not have populated the
    /// bindings map (e.g. catch arm and match arm patterns).
    fn resolve_type_annotation(&self, ty_expr: &baml_compiler2_ast::TypeExpr) -> Ty {
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);
        let mut diags = Vec::new();
        let tir_ty = lower_type_expr_in_ns(
            self.db,
            ty_expr,
            pkg_items,
            &pkg_info.namespace_path,
            &[],
            &mut diags,
        );
        self.resolved_aliases.convert(&tir_ty)
    }

    /// Build a `Span` from an expression's source range.
    /// Returns `None` if no source map is available (e.g. synthesized bodies).
    fn span_for_expr(&self, expr_id: AstExprId) -> Option<baml_base::Span> {
        let sm = self.source_map.as_ref()?;
        let range = sm.expr_span(expr_id);
        Some(baml_base::Span::new(self.file.file_id(self.db), range))
    }

    /// Build a `Span` from a statement's source range.
    fn span_for_stmt(&self, stmt_id: AstStmtId) -> Option<baml_base::Span> {
        let sm = self.source_map.as_ref()?;
        let range = sm.stmt_span(stmt_id);
        Some(baml_base::Span::new(self.file.file_id(self.db), range))
    }
}

// ─── 3.1: lower_function_body ────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_function_body(&mut self) -> MirFunction {
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;

        let func_loc = self
            .func_loc
            .expect("lower_function_body called on non-function LoweringContext");
        let sig = baml_compiler2_ppir::function_signature(self.db, func_loc);

        // Return place _0
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);

        let ret_ty = sig
            .return_type
            .as_ref()
            .map(|te| {
                let mut diags = Vec::new();
                let tir_ty = lower_type_expr_in_ns(
                    self.db,
                    te,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &[],
                    &mut diags,
                );
                self.resolved_aliases.convert(&tir_ty)
            })
            .unwrap_or(Ty::Null {
                attr: TyAttr::default(),
            });
        let ret = self
            .builder
            .declare_local(Some(Name::new("_0")), ret_ty, None, false);

        // Detect enclosing class for `self` parameter resolution
        let index = file_semantic_index(self.db, self.file);
        let item_tree = file_item_tree(self.db, self.file);
        let func_data = &item_tree[func_loc.id(self.db)];
        // Set the function-level span on the builder so MirFunction::span is populated.
        self.builder.set_span(baml_base::Span::new(
            self.file.file_id(self.db),
            func_data.span,
        ));
        let func_scope_id: FileScopeId =
            index.scope_at_offset(func_data.span.start(), Some(&func_data.name));
        let func_scope = &index.scopes[func_scope_id.index() as usize];
        let enclosing_class_name: Option<Name> = func_scope.parent.and_then(|parent_idx| {
            let parent = &index.scopes[parent_idx.index() as usize];
            if matches!(parent.kind, baml_compiler2_hir::scope::ScopeKind::Class) {
                parent.name.clone()
            } else {
                None
            }
        });

        // Parameter locals _1..=_n
        // For `self` with no annotation, look up the TIR-inferred parameter type
        // which correctly resolves to the enclosing class type.
        for (param_idx, param) in sig.params.iter().enumerate() {
            let param_ty = if param.name.as_str() == "self"
                && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
            {
                // self parameter: TIR resolves this via enclosing class lookup.
                // Read the same resolution here.
                enclosing_class_name
                    .as_ref()
                    .and_then(|cn| {
                        pkg_items
                            .lookup_type(&pkg_info.namespace_path, cn)
                            .map(|def| {
                                let tir_ty = baml_compiler2_tir::ty::Ty::Class(
                                    baml_compiler2_tir::lower_type_expr::qualify_def(
                                        self.db, def, cn,
                                    ),
                                    vec![],
                                    baml_compiler2_tir::ty::TyAttr::default(),
                                );
                                self.resolved_aliases.convert(&tir_ty)
                            })
                    })
                    .unwrap_or(Ty::Null {
                        attr: TyAttr::default(),
                    })
            } else {
                let mut diags = Vec::new();
                let tir_ty = lower_type_expr_in_ns(
                    self.db,
                    &param.ty,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &[],
                    &mut diags,
                );
                self.resolved_aliases.convert(&tir_ty)
            };
            let local = self
                .builder
                .declare_local(Some(param.name.clone()), param_ty, None, false);
            self.locals.insert(param.name.clone(), local);
            self.binding_locals
                .insert(BindingId::parameter(self.current_scope, param_idx), local);
        }

        // Entry and exit blocks
        let entry = self.builder.create_block();
        let exit = self.builder.create_block();
        self.exit_block = exit;
        self.builder.set_current_block(entry);

        let parameter_defaults =
            baml_compiler2_ppir::function_parameter_defaults(self.db, func_loc);
        self.lower_default_parameter_prologue(func_data, &parameter_defaults);

        // Lower root expression into return place
        let root_expr = self.body.root_expr;
        if let Some(root) = root_expr {
            self.lower_expr(root, Place::local(ret));
        } else {
            self.builder.assign(
                Place::local(ret),
                Rvalue::Use(Operand::Constant(Constant::Null)),
            );
        }

        // Goto exit, emit Return terminator
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Mark locals captured by nested lambdas. HIR stores this by binding
        // identity, including block-owned bindings.
        self.mark_captured_locals_in_scope_tree(self.current_scope);

        // Take the builder out of self to call `build()` which consumes it
        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let builder = std::mem::replace(&mut self.builder, dummy);
        let mut mir = builder.build();
        optimize::optimize_function(&mut mir);

        // Drain any lambda functions lowered during this function's body into the
        // MirFunction's lambdas list.  The lambda_idx values in MakeClosure rvalues
        // index into this vec.
        mir.lambdas = std::mem::take(&mut self.pending_lambdas);

        mir
    }

    fn lower_default_parameter_prologue(
        &mut self,
        func_data: &baml_compiler2_hir::item_tree::Function,
        parameter_defaults: &baml_compiler2_hir::signature::FunctionParameterDefaults,
    ) {
        for (index, param) in func_data.params.iter().enumerate() {
            let Some(default_ref) = parameter_defaults.param_default(index) else {
                continue;
            };

            let Some(&param_local) = self.locals.get(&param.name) else {
                continue;
            };

            let test_local = self.builder.temp(Ty::Bool {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(test_local),
                Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::local(param_local)),
                    right: Operand::Constant(Constant::OmittedArg),
                },
            );

            let default_block = self.builder.create_block();
            let next_block = self.builder.create_block();
            self.builder.branch(
                Operand::Copy(Place::local(test_local)),
                default_block,
                next_block,
            );

            self.builder.set_current_block(default_block);
            self.lower_default_expr(
                default_ref.expr.expr(),
                &parameter_defaults.defaults,
                Place::local(param_local),
            );
            if !self.builder.is_current_terminated() {
                self.builder.goto(next_block);
            }

            self.builder.set_current_block(next_block);
        }
    }

    fn lower_default_expr(
        &mut self,
        expr_id: AstExprId,
        defaults: &baml_compiler2_ast::FunctionDefaults,
        dest: Place,
    ) {
        let saved_body = std::mem::replace(&mut self.body, defaults.exprs.clone());
        let saved_source_map = self.source_map.replace(defaults.source_map.clone());
        let saved_metadata_scope = self.current_metadata_scope;
        self.current_metadata_scope = MetadataScope::ParameterDefault(self.current_scope);
        self.lower_expr(expr_id, dest);
        self.current_metadata_scope = saved_metadata_scope;
        self.source_map = saved_source_map;
        self.body = saved_body;
    }

    /// Lower a top-level let binding's initializer into a zero-arg `MirFunctionBody`.
    ///
    /// The resulting body has arity 0, a single `_0` return place (type unknown/null),
    /// and evaluates the initializer expression, leaving the result in `_0`.
    /// This is used by `compile_init_function` to compile let initializers into bytecode
    /// that can then be called and have their result stored via `StoreGlobal`.
    fn lower_let_body_inner(&mut self) -> MirFunctionBody {
        // Return place _0 (type unknown — let bodies don't have type annotations)
        let ret = self.builder.declare_local(
            Some(Name::new("_0")),
            Ty::Null {
                attr: TyAttr::default(),
            },
            None,
            false,
        );

        // Entry and exit blocks
        let entry = self.builder.create_block();
        let exit = self.builder.create_block();
        self.exit_block = exit;
        self.builder.set_current_block(entry);

        // Lower root expression into return place
        if let Some(root) = self.body.root_expr {
            self.lower_expr(root, Place::local(ret));
        } else {
            self.builder.assign(
                Place::local(ret),
                Rvalue::Use(Operand::Constant(Constant::Null)),
            );
        }

        // Goto exit, emit Return terminator
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Take the builder out and build the MirFunctionBody
        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let builder = std::mem::replace(&mut self.builder, dummy);
        let mut body = builder.build_body();
        optimize::optimize_function_body(&mut body);
        body
    }

    fn lower_optional_function_adapter(
        &mut self,
        expr_id: AstExprId,
        coercion: &baml_compiler2_tir::inference::FunctionCoercion,
        dest: Place,
    ) {
        let original_ty = self.expr_ty(expr_id);
        let original_local = self.builder.temp(original_ty);
        self.lower_expr_without_function_coercion(expr_id, Place::Local(original_local));
        self.builder.local_decl_mut(original_local).is_captured = true;

        let parent_name = self.builder.name().to_string();
        let adapter_count = self
            .synthetic_name_counts
            .entry("__optional_adapter".to_string())
            .or_insert(0);
        let adapter_idx = *adapter_count;
        *adapter_count += 1;
        let adapter_name = format!("<optional-adapter({parent_name}, {adapter_idx})>");

        let mut adapter_builder =
            MirBuilder::new(Name::new(&adapter_name), coercion.target_params.len());

        let ret_ty = convert_tir2_ty(&coercion.target_return, &self.resolved_aliases);
        let ret = adapter_builder.declare_local(Some(Name::new("_0")), ret_ty, None, false);

        for param in &coercion.target_params {
            let param_ty = convert_tir2_ty(&param.ty, &self.resolved_aliases);
            adapter_builder.declare_local(param.name.clone(), param_ty, None, false);
        }

        let entry = adapter_builder.create_block();
        let after_call = adapter_builder.create_block();
        adapter_builder.set_current_block(entry);

        let mut next_required_target = 0usize;
        let mut source_args = Vec::with_capacity(coercion.source_params.len());
        for source_param in &coercion.source_params {
            match source_param.mode {
                FunctionParamMode::Required => {
                    let target_index = coercion
                        .target_params
                        .iter()
                        .enumerate()
                        .filter(|(_, param)| param.is_required())
                        .nth(next_required_target)
                        .map(|(idx, _)| idx)
                        .unwrap_or(next_required_target);
                    next_required_target += 1;
                    source_args.push(Operand::Copy(Place::Local(Local(target_index + 1))));
                }
                FunctionParamMode::Optional => {
                    let target_index = source_param.name.as_ref().and_then(|name| {
                        coercion.target_params.iter().position(|param| {
                            param.is_optional() && param.name.as_ref() == Some(name)
                        })
                    });
                    if let Some(target_index) = target_index {
                        source_args.push(Operand::Copy(Place::Local(Local(target_index + 1))));
                    } else {
                        source_args.push(Operand::Constant(Constant::OmittedArg));
                    }
                }
            }
        }

        adapter_builder.call(
            Operand::Copy(Place::Capture(0)),
            source_args,
            Place::Local(ret),
            after_call,
            None,
        );
        adapter_builder.set_current_block(after_call);
        adapter_builder.return_();

        let mut adapter_mir = adapter_builder.build();
        optimize::optimize_function(&mut adapter_mir);
        adapter_mir.item_ref = ItemRef::Free {
            package: Name::new(""),
            namespace: vec![],
            name: Name::new(&adapter_name),
        };

        let lambda_idx = self.pending_lambdas.len();
        self.pending_lambdas.push(adapter_mir);
        self.builder.assign(
            dest,
            Rvalue::MakeClosure {
                lambda_idx,
                captures: vec![Operand::Copy(Place::Local(original_local))],
                type_arg_templates: vec![],
            },
        );
    }

    /// Lower a lambda expression into a nested `MirFunction` and emit a
    /// `Rvalue::MakeClosure` assignment into `dest`.
    ///
    /// Saves all parent-body state, sets up a fresh builder for the lambda,
    /// lowers the lambda body, then restores the parent state.  The completed
    /// `MirFunction` is pushed into `self.pending_lambdas`; its index becomes
    /// the `lambda_idx` in the emitted `MakeClosure` rvalue.
    ///
    /// Captures are empty in Phase 3 (non-capturing lambdas only).
    #[allow(clippy::cast_possible_truncation)]
    fn lower_lambda(
        &mut self,
        func_def: &baml_compiler2_ast::FunctionDef,
        expr_id: AstExprId,
        dest: Place,
    ) {
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;

        // Generate a unique synthetic name for this lambda.
        let parent_name = self.builder.name().to_string();
        let lambda_count = self
            .synthetic_name_counts
            .entry("__lambda".to_string())
            .or_insert(0);
        let lambda_idx_name = *lambda_count;
        *lambda_count += 1;
        let lambda_name = format!("<lambda({parent_name}, {lambda_idx_name})>");

        // Find the lambda's FileScopeId from the HIR index.
        // The HIR builder registered a ScopeKind::Lambda at the lambda expression's span.
        let lambda_scope_id: FileScopeId = if let Some(ref sm) = self.source_map {
            let lambda_span = sm.expr_span(expr_id);
            let index = file_semantic_index(self.db, self.file);
            // Find the Lambda scope containing this span by searching for it.
            // We look for a Lambda-kind scope whose range matches the lambda span.
            let mut found = None;
            for (i, scope) in index.scopes.iter().enumerate() {
                if scope.kind == baml_compiler2_hir::scope::ScopeKind::Lambda
                    && scope.range == lambda_span
                {
                    found = Some(FileScopeId::new(i as u32));
                    break;
                }
            }
            found.unwrap_or(self.current_scope)
        } else {
            self.current_scope
        };

        // Pull out the lambda's body and source map.
        let (lambda_body, lambda_source_map) = match func_def.body.as_ref() {
            Some(baml_compiler2_ast::FunctionBodyDef::Expr(body, sm)) => {
                (body.clone(), Some(sm.clone()))
            }
            _ => {
                // No body — emit a panic stub and return.
                self.emit_panic_call("lambda without body", expr_id);
                return;
            }
        };

        // Read HIR captures for this lambda scope.
        // `captures` lists the exact binding identities that the lambda reads
        // from enclosing scopes. We build `capture_indices` so path/lvalue
        // lowering can emit `Place::Capture(idx)` without collapsing shadows by name.
        let hir_captures: Vec<(Name, BindingId)> = {
            let index = file_semantic_index(self.db, self.file);
            index
                .scope_bindings
                .get(lambda_scope_id.index() as usize)
                .map(|sb| sb.captures.clone())
                .unwrap_or_default()
        };
        let lambda_capture_indices: HashMap<BindingId, usize> = hir_captures
            .iter()
            .enumerate()
            .map(|(i, (_, binding_id))| (*binding_id, i))
            .collect();

        // Save parent state.
        let saved_builder = std::mem::replace(
            &mut self.builder,
            MirBuilder::new(Name::new(&lambda_name), 0),
        );
        let saved_body = std::mem::replace(&mut self.body, lambda_body);
        let saved_source_map = std::mem::replace(&mut self.source_map, lambda_source_map);
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_binding_locals = std::mem::take(&mut self.binding_locals);
        let saved_exit_block = self.exit_block;
        let saved_loop_context = self.loop_context.take();
        let saved_catch_context = self.catch_context.take();
        let saved_watched_locals = std::mem::take(&mut self.watched_locals_stack);
        let saved_current_scope = self.current_scope;
        let saved_metadata_scope = self.current_metadata_scope;
        // NOTE: synthetic_name_counts is intentionally NOT saved — its counter
        // keeps incrementing across the whole function for uniqueness.
        //
        // pending_lambdas IS saved so each lambda collects only its own direct
        // children. The lambda body's nested lambdas are collected separately
        // and attached to the lambda as its `.lambdas` field.
        let saved_pending_lambdas = std::mem::take(&mut self.pending_lambdas);
        let saved_capture_indices = self.capture_indices.take();
        // Save transitive_captures_needed: after lowering this lambda's body,
        // newly discovered transitive captures will be in this field.  We save
        // the parent's list and restore it after collecting.
        let saved_transitive_captures = std::mem::take(&mut self.transitive_captures_needed);

        // Switch to the lambda scope and install capture map.
        // Always use Some(map) — even for empty HIR captures — so that
        // add_transitive_capture can extend it at runtime.
        self.current_scope = lambda_scope_id;
        self.current_metadata_scope = MetadataScope::Body(lambda_scope_id);
        self.capture_indices = Some(lambda_capture_indices);

        // Set up a fresh builder with the correct arity.
        let arity = func_def.params.len();
        self.builder = MirBuilder::new(Name::new(&lambda_name), arity);

        // Declare return place _0.
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package.clone());
        let pkg_items = package_items(self.db, pkg_id);
        let ret = self.builder.declare_local(
            Some(Name::new("_0")),
            baml_type::Ty::Null {
                attr: baml_type::TyAttr::default(),
            },
            None,
            false,
        );

        // Declare parameter locals _1..=_n.
        for (param_idx, param) in func_def.params.iter().enumerate() {
            let param_ty = match &param.type_expr {
                Some(spanned_te) => {
                    let mut diags = Vec::new();
                    let tir_ty = lower_type_expr_in_ns(
                        self.db,
                        &spanned_te.expr,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &[],
                        &mut diags,
                    );
                    self.resolved_aliases.convert(&tir_ty)
                }
                None => baml_type::Ty::Null {
                    attr: baml_type::TyAttr::default(),
                },
            };
            let local = self
                .builder
                .declare_local(Some(param.name.clone()), param_ty, None, false);
            self.locals.insert(param.name.clone(), local);
            self.binding_locals
                .insert(BindingId::parameter(self.current_scope, param_idx), local);
        }

        // Create entry and exit blocks.
        let entry = self.builder.create_block();
        let exit_blk = self.builder.create_block();
        self.exit_block = exit_blk;
        self.builder.set_current_block(entry);

        // Lower the root expression into the return place.
        if let Some(root) = self.body.root_expr {
            self.lower_expr(root, Place::local(ret));
        } else {
            self.builder.assign(
                Place::local(ret),
                Rvalue::Use(Operand::Constant(Constant::Null)),
            );
        }

        // Terminate: goto exit, then return.
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Mark locals captured by nested lambdas. HIR stores this by binding
        // identity, including block-owned bindings.
        self.mark_captured_locals_in_scope_tree(lambda_scope_id);

        // Build the lambda MirFunction.
        // First, collect any nested lambdas that were encountered while lowering
        // this lambda's body (direct children only — saved_pending_lambdas holds
        // any lambdas from the parent scope that were already pending before
        // entering this lambda).
        let nested_lambdas = std::mem::take(&mut self.pending_lambdas);

        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let lambda_builder = std::mem::replace(&mut self.builder, dummy);
        let mut lambda_mir = lambda_builder.build();
        optimize::optimize_function(&mut lambda_mir);
        // Override item_ref with the synthetic name.
        lambda_mir.item_ref = ItemRef::Free {
            package: Name::new(""),
            namespace: vec![],
            name: Name::new(&lambda_name),
        };
        // Attach nested lambdas as direct children.
        lambda_mir.lambdas = nested_lambdas;

        // Collect transitive captures that inner lambda bodies discovered were
        // needed (names that weren't in hir_captures but that inner lambdas
        // required via transitive capture).
        let newly_needed_transitive = std::mem::take(&mut self.transitive_captures_needed);

        // Restore parent state.
        self.builder = saved_builder;
        self.body = saved_body;
        self.source_map = saved_source_map;
        self.locals = saved_locals;
        self.binding_locals = saved_binding_locals;
        self.exit_block = saved_exit_block;
        self.loop_context = saved_loop_context;
        self.catch_context = saved_catch_context;
        self.watched_locals_stack = saved_watched_locals;
        self.current_scope = saved_current_scope;
        self.current_metadata_scope = saved_metadata_scope;
        self.capture_indices = saved_capture_indices;
        // Restore parent's pending_lambdas (siblings of this lambda).
        self.pending_lambdas = saved_pending_lambdas;
        // Restore the parent's transitive captures (not ours).
        self.transitive_captures_needed = saved_transitive_captures;

        // Extend hir_captures with any transitively-needed names discovered
        // during body lowering (for inner lambdas that needed grandparent vars).
        // Do NOT propagate here — the capture operands building loop below will
        // handle propagation by pushing to `transitive_captures_needed` when a
        // name is not found in the current scope's locals or captures.
        let mut extended_hir_captures = hir_captures;
        for binding_id in newly_needed_transitive {
            if !extended_hir_captures
                .iter()
                .any(|(_, existing)| *existing == binding_id)
            {
                extended_hir_captures.push((Name::new("_capture"), binding_id));
            }
        }

        // Build capture operands from restored parent locals/captures.
        // Each captured name must be in the parent's locals map; we pass the cell
        // pointer (the slot itself, not the inner value) via Operand::Copy(Place::Local(local)).
        // The emit phase later replaces this with a LoadVar of the cell slot (not LoadDeref).
        //
        // If a name is not in the parent's locals AND not in the parent's
        // capture_indices, we add it as a transitive capture of the current
        // lambda — i.e. the current lambda (f) will need to capture it from ITS
        // parent, and g will receive it via f's capture slot.
        let mut capture_operands: Vec<Operand> = Vec::with_capacity(extended_hir_captures.len());
        for (_, binding_id) in &extended_hir_captures {
            if let Some(&local) = self.binding_locals.get(binding_id) {
                // Mark the local as captured at the capture site — this is the
                // definitive place where we know the exact Local being captured,
                // even in the presence of shadowing.
                self.builder.local_decl_mut(local).is_captured = true;
                capture_operands.push(Operand::Copy(Place::Local(local)));
            } else if let Some(cap_idx) = self
                .capture_indices
                .as_ref()
                .and_then(|m| m.get(binding_id))
                .copied()
            {
                // The variable is itself a capture in the current scope.
                capture_operands.push(Operand::Copy(Place::Capture(cap_idx)));
            } else {
                // Not in current scope's locals or captures.
                // Add as a new transitive capture of the current lambda so our
                // parent will pass it through to us, and we can forward it to
                // the inner lambda.
                let new_idx = {
                    let ci = self.capture_indices.get_or_insert_with(HashMap::new);
                    let idx = ci.len();
                    ci.insert(*binding_id, idx);
                    idx
                };
                // Signal to our parent lambda that it needs to capture this name.
                self.transitive_captures_needed.push(*binding_id);
                capture_operands.push(Operand::Copy(Place::Capture(new_idx)));
            }
        }

        // Push this lambda into the parent's pending_lambdas and emit MakeClosure.
        let lambda_pending_idx = self.pending_lambdas.len();
        self.pending_lambdas.push(lambda_mir);

        // Build TyTemplate entries for each enclosing generic type param so
        // the closure can materialise them at runtime.  These resolve in the
        // **outer** frame (TypeArgRef(N) → outer frame.type_args[N]).
        let enclosing_params = self.enclosing_generic_params();
        let type_arg_templates: Vec<TyTemplate> = enclosing_params
            .iter()
            .enumerate()
            .map(|(n, _)| TyTemplate::TypeArgRef(n as u32))
            .collect();

        self.builder.assign(
            dest,
            Rvalue::MakeClosure {
                lambda_idx: lambda_pending_idx,
                captures: capture_operands,
                type_arg_templates,
            },
        );
    }
}

// ─── 3.2: Core lower_expr dispatch ───────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_scoped_block(
        &mut self,
        stmts: &[AstStmtId],
        tail_expr: Option<AstExprId>,
        dest: Place,
    ) {
        let saved_locals = self.locals.clone();
        let watched_depth = self.watched_locals_stack.len();

        for &stmt_id in stmts {
            self.lower_stmt(stmt_id);
            if self.builder.is_current_terminated() {
                break;
            }
        }

        if !self.builder.is_current_terminated() {
            match tail_expr {
                Some(tail) => self.lower_expr(tail, dest),
                None => {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                }
            }
        }

        if !self.builder.is_current_terminated() {
            self.emit_unwatch_to_depth(watched_depth);
        }
        self.restore_locals_after_scope(saved_locals, watched_depth);
    }

    fn lower_expr(&mut self, expr_id: AstExprId, dest: Place) {
        if let Some(coercion) = self
            .function_coercions
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
        {
            self.lower_optional_function_adapter(expr_id, &coercion, dest);
        } else {
            self.lower_expr_without_function_coercion(expr_id, dest);
        }
    }

    fn lower_expr_without_function_coercion(&mut self, expr_id: AstExprId, dest: Place) {
        let prev_span = self.builder.current_source_span;
        if let Some(span) = self.span_for_expr(expr_id) {
            self.builder.current_source_span = Some(span);
        }

        // Clone expr to avoid borrow issues
        let expr = self.body.exprs[expr_id].clone();
        match expr {
            AstExpr::Literal(lit) => {
                let constant = Self::lower_literal(&lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
            }

            AstExpr::ByteStringLiteral(bytes) => {
                self.builder.assign(dest, Rvalue::Uint8Array(bytes));
            }

            AstExpr::Null => {
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            AstExpr::Path(segments) => {
                self.lower_path_expr(expr_id, &segments, dest);
            }

            AstExpr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.lower_if(expr_id, condition, then_branch, else_branch, dest);
            }

            AstExpr::Binary { op, lhs, rhs } => {
                self.lower_binary(expr_id, op, lhs, rhs, dest);
            }

            AstExpr::Unary { op, expr } => {
                self.lower_unary(expr_id, op, expr, dest);
            }

            AstExpr::Call { callee, args, .. } => {
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                self.lower_call(expr_id, callee, &arg_exprs, dest);
            }

            AstExpr::Array { elements } => {
                let operands: Vec<Operand> =
                    elements.iter().map(|&e| self.lower_to_operand(e)).collect();
                self.builder.assign(dest, Rvalue::Array(operands));
            }

            AstExpr::Map { entries } => {
                let pairs: Vec<(Operand, Operand)> = entries
                    .iter()
                    .map(|&(k, v)| (self.lower_to_operand(k), self.lower_to_operand(v)))
                    .collect();
                self.builder.assign(dest, Rvalue::Map(pairs));
            }

            AstExpr::Object {
                type_name,
                fields,
                spreads,
                ..
            } => {
                self.lower_object(expr_id, type_name.as_ref(), &fields, &spreads, dest);
            }

            AstExpr::MemberAccess { base, member } => {
                self.lower_member_access(expr_id, base, &member, dest);
            }

            AstExpr::OptionalMemberAccess { base, member } => {
                self.lower_optional_member_access(expr_id, base, &member, dest);
            }

            AstExpr::OptionalIndex { base, index } => {
                self.lower_optional_index(expr_id, base, index, dest);
            }

            AstExpr::OptionalCall { callee, args } => {
                let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
                self.lower_optional_call(expr_id, callee, &arg_exprs, dest);
            }

            AstExpr::Index { base, index } => {
                self.lower_index(expr_id, base, index, dest);
            }

            AstExpr::Block { stmts, tail_expr } => {
                self.lower_scoped_block(&stmts, tail_expr, dest);
            }

            AstExpr::Match {
                scrutinee, arms, ..
            } => {
                let arms_owned = arms;
                self.lower_match(expr_id, scrutinee, &arms_owned, dest);
            }

            AstExpr::Is { scrutinee, pattern } => {
                // `<scrutinee> is <pattern>` — runtime pattern test that
                // yields `true` if the pattern matches, `false` otherwise.
                // We reuse `lower_pattern_test`, the same engine match-arm
                // dispatch uses, with two terminal blocks that write the
                // boolean constant into `dest` and jump to a join.
                let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
                    let op = self.lower_to_operand(scrutinee);
                    let ty = self.expr_ty(scrutinee);
                    self.operand_to_local(op, ty)
                });

                let bb_true = self.builder.create_block();
                let bb_false = self.builder.create_block();
                let bb_join = self.builder.create_block();

                self.lower_pattern_test(scrutinee_local, pattern, bb_true, bb_false);

                self.builder.set_current_block(bb_true);
                self.builder.assign(
                    dest.clone(),
                    Rvalue::Use(Operand::Constant(Constant::Bool(true))),
                );
                self.builder.goto(bb_join);

                self.builder.set_current_block(bb_false);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Bool(false))));
                self.builder.goto(bb_join);

                self.builder.set_current_block(bb_join);
            }

            AstExpr::Catch { base, clauses } => {
                let clauses_owned = clauses;
                self.lower_catch(expr_id, base, &clauses_owned, &dest);
            }

            AstExpr::Throw { value } => {
                let val_op = self.lower_to_operand(value);
                if let Some(catch_ctx) = &self.catch_context {
                    // Inside a catch block: store the value into the error
                    // local and jump to the handler instead of unwinding.
                    let error_local = catch_ctx.error_local;
                    let unwind_target = catch_ctx.unwind_target;
                    self.builder
                        .assign(Place::Local(error_local), Rvalue::Use(val_op));
                    self.builder.goto(unwind_target);
                } else {
                    self.builder.throw(val_op);
                }
                // Start a dead block for any code after this (unreachable)
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstExpr::Lambda(func_def) => {
                self.lower_lambda(&func_def, expr_id, dest);
            }

            AstExpr::OptionalChain { expr } => {
                self.lower_optional_chain(expr_id, expr, dest);
            }

            AstExpr::Missing => {
                self.emit_panic_call("parse error", expr_id);
            }

            AstExpr::Spawn { name, body } => {
                self.lower_spawn(expr_id, name, body, dest);
            }

            AstExpr::Await { future } => {
                self.lower_await(expr_id, future, dest);
            }
        }

        self.builder.current_source_span = prev_span;
    }

    /// Lower `spawn name? { body }` into:
    ///   1. A `MakeClosure` for the body wrapped as a 0-arg lambda.
    ///   2. A name temp (string operand or null constant).
    ///   3. A `Terminator::Spawn` writing the resulting Future handle.
    fn lower_spawn(
        &mut self,
        expr_id: AstExprId,
        name: Option<AstExprId>,
        body: AstExprId,
        dest: Place,
    ) {
        // The AST-lower step has already wrapped the spawn body in a
        // synthetic 0-arg `Expr::Lambda`. Lowering it through the
        // standard expression path emits a `MakeClosure` rvalue, which
        // is exactly what we want as the closure operand to `Spawn`.
        let closure_local = self.builder.temp(Ty::Null {
            attr: TyAttr::default(),
        });
        let closure_place = Place::Local(closure_local);
        self.lower_expr(body, closure_place.clone());
        let closure_op = Operand::Copy(closure_place);

        // Lower the optional name into an operand.
        let name_op = match name {
            Some(name_id) => self.lower_to_operand(name_id),
            None => Operand::Constant(Constant::Null),
        };

        // Allocate the future temp. Phase C uses a defaulted `Null` type
        // for the future local; the TIR-tracked value/error types flow
        // through to runtime via the surrounding context. A follow-up
        // can plumb `Tir2Ty::Future` directly through `convert_tir2_ty`
        // here once we read it from `self.expr_types`.
        let future_local = self.builder.temp(Ty::Null {
            attr: TyAttr::default(),
        });
        let future_place = Place::Local(future_local);

        let resume = self.builder.create_block();
        self.builder
            .spawn(closure_op, name_op, future_place.clone(), resume);
        self.builder.set_current_block(resume);
        // The result of `spawn` is the Future handle.
        self.builder
            .assign(dest, Rvalue::Use(Operand::Copy(future_place)));
        // Phase C: `expr_id` is recorded for source-span tracking but
        // is not used for type lookup here.
        let _ = expr_id;
    }

    /// Lower `await expr` into a `Terminator::Await` whose destination is
    /// the awaited value.
    fn lower_await(&mut self, _expr_id: AstExprId, future: AstExprId, dest: Place) {
        let future_local = self.builder.temp(Ty::Null {
            attr: TyAttr::default(),
        });
        let future_place = Place::Local(future_local);
        self.lower_expr(future, future_place.clone());

        // `Terminator::Await` requires its destination to be `Place::Local`.
        // If the caller handed us a projection (field/index), await into a
        // temp local and then assign through to the projection — mirrors
        // how `lower_call` normalizes its destination.
        let (await_dest, projection_dest) = match dest {
            Place::Local(_) => (dest, None),
            projection => {
                let tmp = self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                });
                (Place::Local(tmp), Some(projection))
            }
        };

        let resume = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.builder
            .await_(future_place, await_dest.clone(), resume, unwind);
        self.builder.set_current_block(resume);

        if let Some(projection) = projection_dest {
            self.builder
                .assign(projection, Rvalue::Use(Operand::Copy(await_dest)));
        }
    }
}

// ─── Literal helper ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_literal(lit: &AstLiteral) -> Constant {
        use baml_base::Literal;
        match lit {
            Literal::Int(v) => Constant::Int(*v),
            Literal::Float(s) => {
                // Literal::Float stores a string representation — parse to f64
                let v: f64 = s.parse().unwrap_or(0.0);
                Constant::Float(v)
            }
            Literal::String(v) => Constant::String(v.clone()),
            Literal::Bool(v) => Constant::Bool(*v),
        }
    }
}

// ─── 3.3: Path expression lowering ───────────────────────────────────────────

impl<'db> LoweringContext<'db> {
    fn lower_path_expr(&mut self, expr_id: AstExprId, segments: &[Name], dest: Place) {
        // Multi-segment paths (e.g. baml.llm.render_prompt, self.field, obj.method) — check TIR resolution first
        if segments.len() > 1 {
            // Check path_member_resolutions first (set by infer_local_rooted_path for local-rooted paths).
            // This takes priority over the flat resolutions map since infer_local_rooted_path
            // moves resolutions from the flat map into path_member_resolutions.
            if let Some(member_resolutions) = self
                .path_member_resolutions
                .get(&self.expr_metadata_key(expr_id))
                .cloned()
            {
                use baml_compiler2_tir::inference::MemberResolution;
                // The last resolution corresponds to the final segment of the path.
                // - If the last resolution is a BoundMethod/UnboundMethod/Free, this path is a
                //   callee reference; emit a function constant. The receiver will be prepended
                //   by lower_call.
                // - If the last resolution is a Field, this is a pure field-chain access.
                // Note: for paths like `user.profile.items.slice`, the member_resolutions
                // are [Field{profile}, Field{items}, BoundMethod{slice}], so we check last().
                match member_resolutions.last() {
                    Some(MemberResolution::BoundMethod { .. }) => {
                        // Bound method reference: lower receiver and emit MakeBoundMethod.
                        let resolution = member_resolutions.into_iter().last().unwrap();
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            let receiver_segments = &segments[..segments.len() - 1];
                            let receiver_op = if receiver_segments.len() == 1 {
                                if let Some(&recv_local) = self.locals.get(&receiver_segments[0]) {
                                    Operand::Copy(Place::Local(recv_local))
                                } else if let Some(cap_idx) =
                                    self.capture_index_for_name_at(expr_id, &receiver_segments[0])
                                {
                                    // Receiver is a captured variable — use capture slot.
                                    Operand::Copy(Place::Capture(cap_idx))
                                } else {
                                    Operand::Constant(Constant::Null)
                                }
                            } else {
                                // Multi-segment receiver (e.g. `cfg.encoder`): lower as field chain.
                                let recv_ty = self.expr_ty(expr_id);
                                let recv_local = self.builder.temp(recv_ty);
                                self.lower_multi_segment_path_as_field_chain(
                                    expr_id,
                                    receiver_segments,
                                    Place::local(recv_local),
                                );
                                Operand::Copy(Place::local(recv_local))
                            };
                            self.builder.assign(
                                dest,
                                Rvalue::MakeBoundMethod {
                                    item_ref: item,
                                    receiver: receiver_op,
                                },
                            );
                            return;
                        }
                    }
                    Some(
                        MemberResolution::UnboundMethod { .. } | MemberResolution::Free { .. },
                    ) => {
                        // Unbound method or free function reference — emit a plain function constant.
                        let resolution = member_resolutions.into_iter().last().unwrap();
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(item))),
                            );
                            return;
                        }
                    }
                    Some(MemberResolution::Field { .. }) => {
                        // Local-rooted field access — chain field projections.
                        self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                        return;
                    }
                    Some(MemberResolution::Variant { .. }) => {
                        // Handled by expr_types check below.
                    }
                    None => {}
                }
            }

            // Check flat resolutions (set by infer_multi_segment_path for package-rooted paths
            // like baml.fs.open, baml.env.get, etc.).
            if let Some(resolution) = self
                .resolutions
                .get(&self.expr_metadata_key(expr_id))
                .cloned()
            {
                use baml_compiler2_tir::inference::MemberResolution;
                match &resolution {
                    MemberResolution::BoundMethod { .. } => {
                        // Bound method reference via flat resolutions: emit MakeBoundMethod.
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            let receiver_segments = &segments[..segments.len() - 1];
                            let receiver_op = if receiver_segments.len() == 1 {
                                if let Some(&recv_local) = self.locals.get(&receiver_segments[0]) {
                                    Operand::Copy(Place::Local(recv_local))
                                } else if let Some(cap_idx) =
                                    self.capture_index_for_name_at(expr_id, &receiver_segments[0])
                                {
                                    // Receiver is a captured variable — use capture slot.
                                    Operand::Copy(Place::Capture(cap_idx))
                                } else {
                                    Operand::Constant(Constant::Null)
                                }
                            } else {
                                let recv_ty = self.expr_ty(expr_id);
                                let recv_local = self.builder.temp(recv_ty);
                                self.lower_multi_segment_path_as_field_chain(
                                    expr_id,
                                    receiver_segments,
                                    Place::local(recv_local),
                                );
                                Operand::Copy(Place::local(recv_local))
                            };
                            self.builder.assign(
                                dest,
                                Rvalue::MakeBoundMethod {
                                    item_ref: item,
                                    receiver: receiver_op,
                                },
                            );
                            return;
                        }
                    }
                    MemberResolution::UnboundMethod { .. } | MemberResolution::Free { .. } => {
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(item))),
                            );
                            return;
                        }
                    }
                    MemberResolution::Variant { .. } => {
                        // Handled by expr_types check below.
                    }
                    MemberResolution::Field { .. } => {
                        // Local-rooted field access — chain field projections.
                        // The root segment is a local; chain through class fields.
                        self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                        return;
                    }
                }
            }
            if self.locals.contains_key(&segments[0])
                || self
                    .capture_index_for_name_at(expr_id, &segments[0])
                    .is_some()
            {
                self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                return;
            }
            // Check for enum variant (e.g. Status.Active lowered to Path(["Status","Active"]))
            if let Some(Tir2Ty::EnumVariant(qtn, variant, _)) = self
                .expr_types
                .get(&self.expr_metadata_key(expr_id))
                .cloned()
                .as_ref()
            {
                let enum_ref = ItemRef::EnumType {
                    package: qtn.package().clone(),
                    namespace: qtn.namespace().clone(),
                    name: qtn.name().clone(),
                };
                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                        enum_ref,
                        variant: variant.clone(),
                    })),
                );
                return;
            }
            // Namespace intermediate or unresolved — emit null placeholder.
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            return;
        }

        let name = &segments[0];

        let span_start = self
            .source_map
            .as_ref()
            .map(|sm| sm.expr_span(expr_id).start())
            .unwrap_or_default();

        let resolved = resolve_name_at_in_scope(
            self.db,
            self.file,
            span_start,
            name,
            self.scope_func_name.as_ref(),
        );
        match resolved {
            ResolvedName::Local {
                name: local_name, ..
            } => {
                if let Some(&local) = self.locals.get(&local_name) {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::Local(local))));
                } else if let Some(cap_idx) = self.capture_index_for_name_at(expr_id, &local_name) {
                    // This variable is captured from an enclosing scope.
                    // Emit a LoadCapture via Place::Capture.
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::Capture(cap_idx))));
                } else {
                    let msg = format!("unresolved local: {local_name}");
                    self.emit_panic_call(&msg, expr_id);
                }
            }
            ResolvedName::Item(def) => {
                self.lower_item_ref(expr_id, def, dest);
            }
            ResolvedName::Builtin(def) => {
                let item = def_to_item_ref(self.db, def);
                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Constant(Constant::Function(item))),
                );
            }
            ResolvedName::Unknown => {
                // If TIR recorded a type for this expr, it was handled as a package
                // path intermediate (e.g. `baml` in `baml.HttpMethod.Get`). Emit a
                // null placeholder — the outer FieldAccess will produce the real value.
                if self
                    .expr_types
                    .contains_key(&self.expr_metadata_key(expr_id))
                {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                } else {
                    let msg = format!("unresolved name: {name}");
                    self.emit_panic_call(&msg, expr_id);
                }
            }
        }
    }

    /// Lower a multi-segment `Path` expression (`a.b.c`) as chained field projections.
    ///
    /// The first segment is resolved as a local variable; subsequent segments are
    /// projected as struct fields (using `class_fields`) or map keys (fallback).
    fn lower_multi_segment_path_as_field_chain(
        &mut self,
        expr_id: AstExprId,
        segments: &[Name],
        dest: Place,
    ) {
        let (mut current_place, mut current_ty) =
            if let Some(&root_local) = self.locals.get(&segments[0]) {
                let place = Place::Local(root_local);
                let ty = if let Some(tir_root) = self.path_root_ty(expr_id) {
                    // If TIR inferred a more specific type for the root local,
                    // update the MIR local's declared type so the emitter can
                    // resolve field names for display (e.g. `load_field .index`).
                    if matches!(self.builder.local_ty(root_local), Ty::BuiltinUnknown { .. })
                        && !matches!(tir_root, Ty::BuiltinUnknown { .. } | Ty::Void { .. })
                    {
                        self.builder.local_decl_mut(root_local).ty = tir_root.clone();
                    }
                    tir_root
                } else {
                    self.builder.local_ty(root_local)
                };
                (place, ty)
            } else if let Some(cap_idx) = self.capture_index_for_name_at(expr_id, &segments[0]) {
                let place = Place::Capture(cap_idx);
                let ty = self
                    .path_root_ty(expr_id)
                    .unwrap_or_else(|| Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    });
                (place, ty)
            } else {
                // Root not found as a local or capture — emit null.
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                return;
            };

        for (offset, seg) in segments[1..].iter().enumerate() {
            let seg_idx = offset + 1;
            let is_last = seg_idx + 1 == segments.len();
            if let Ty::Class(ref tn, ref class_type_args, _) = current_ty.clone() {
                if let Some(fields) = self.class_fields.get(tn) {
                    if let Some(&idx) = fields.get(seg.as_str()) {
                        // Substitute the receiver's class type-args into the
                        // declared field type so chained access through generic
                        // positions (`b.value.name` where `b: Box<User>`)
                        // produces `Ty::Class(User, ...)` rather than the
                        // erased `Ty::Void`.  Without this, the next iteration
                        // falls through to the map-key fallback below and the
                        // VM hits `expected Map, got Instance`.
                        let next_ty = self.class_field_ty(tn, seg, class_type_args);
                        current_place = Place::Field {
                            base: Box::new(current_place),
                            field: idx,
                        };
                        current_ty = next_ty;
                        continue;
                    }
                }
            }

            let target_ty =
                self.path_segment_ty(expr_id, seg_idx)
                    .unwrap_or_else(|| Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    });
            let target_place = if is_last {
                dest.clone()
            } else {
                Place::local(self.builder.temp(target_ty.clone()))
            };
            let base_local = match current_place.clone() {
                Place::Local(local) => local,
                place => {
                    let local = self.builder.temp(current_ty.clone());
                    self.builder
                        .assign(Place::local(local), Rvalue::Use(Operand::Copy(place)));
                    local
                }
            };
            if self.lower_union_class_field_access(
                expr_id,
                base_local,
                &current_ty,
                seg,
                &target_place,
            ) {
                if is_last {
                    return;
                }
                current_place = target_place;
                current_ty = target_ty;
                continue;
            }

            // Dynamic map key fallback
            let key_local = self.builder.temp(Ty::String {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(key_local),
                Rvalue::Use(Operand::Constant(Constant::String(seg.to_string()))),
            );
            current_place = Place::Index {
                base: Box::new(current_place),
                index: key_local,
                kind: IndexKind::Map,
            };
            break;
        }

        self.builder
            .assign(dest, Rvalue::Use(Operand::Copy(current_place)));
    }

    /// Look up the MIR type of a named field on a class, for chained field access.
    ///
    /// `class_type_args` are the type-args carried on the receiver's
    /// `Ty::Class(tn, class_type_args, _)` (e.g. `[User]` for `Box<User>`).
    /// They are substituted into the declared field type so a generic-typed
    /// position (`item: T` in `Container<T>`) resolves to the concrete
    /// receiver-side binding rather than `Ty::Void`.
    ///
    /// Returns `Ty::Null` if the field is not found or the type cannot be
    /// resolved.  Called by `lower_multi_segment_path_as_field_chain` to
    /// track the type through a chain of field projections (`a.b.c` needs
    /// the type of `b` to find `c`).
    fn class_field_ty(&self, class_tn: &TypeName, field_name: &Name, class_type_args: &[Ty]) -> Ty {
        use baml_compiler2_hir::{contributions::Definition, package::package_items};
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;
        let db = self.db;

        let Some(pkg_name) = class_tn.module_path.first() else {
            return Ty::Null {
                attr: TyAttr::default(),
            };
        };
        let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_name.clone());
        let pkg_items_ref = package_items(db, pkg_id);

        let namespace: Vec<Name> = class_tn.module_path[1..].to_vec();

        let Some(Definition::Class(class_loc)) =
            pkg_items_ref.lookup_type(&namespace, &class_tn.name)
        else {
            return Ty::Null {
                attr: TyAttr::default(),
            };
        };

        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];

        let field = class_data.fields.iter().find(|f| &f.name == field_name);
        let Some(field) = field else {
            return Ty::Null {
                attr: TyAttr::default(),
            };
        };
        let Some(ref te) = field.type_expr else {
            return Ty::Null {
                attr: TyAttr::default(),
            };
        };

        let pkg_ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        let mut diags = Vec::new();
        let tir_ty = lower_type_expr_in_ns(
            db,
            &te.expr,
            pkg_items_ref,
            &pkg_ns,
            &class_data.generic_params,
            &mut diags,
        );
        // Build a TyTemplate with `TypeArgRef(N)` for each class-level
        // generic param, then substitute `class_type_args` so a field
        // declared as `T` resolves to the concrete receiver-side binding.
        let template =
            tir2_to_template(&tir_ty, &self.resolved_aliases, &class_data.generic_params);
        template.substitute(class_type_args)
    }

    fn lower_item_ref(&mut self, expr_id: AstExprId, def: Definition<'db>, dest: Place) {
        let item = def_to_item_ref(self.db, def);
        // Check if this expression's type is EnumVariant
        if let Some(Tir2Ty::EnumVariant(_qtn, variant, _)) = self
            .expr_types
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
            .as_ref()
        {
            let variant_name = variant.clone();
            // Convert the Free item ref to an EnumType variant
            let enum_ref = match item {
                ItemRef::Free {
                    package,
                    namespace,
                    name,
                } => ItemRef::EnumType {
                    package,
                    namespace,
                    name,
                },
                other => other,
            };
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                    enum_ref,
                    variant: variant_name,
                })),
            );
            return;
        }
        // Otherwise treat as function/constructor reference
        self.builder.assign(
            dest,
            Rvalue::Use(Operand::Constant(Constant::Function(item))),
        );
    }
}

// ─── 3.4: Operator mapping and binary/unary lowering ─────────────────────────

impl LoweringContext<'_> {
    fn convert_binop(op: AstBinaryOp) -> Option<BinOp> {
        match op {
            AstBinaryOp::Add => Some(BinOp::Add),
            AstBinaryOp::Sub => Some(BinOp::Sub),
            AstBinaryOp::Mul => Some(BinOp::Mul),
            AstBinaryOp::Div => Some(BinOp::Div),
            AstBinaryOp::Mod => Some(BinOp::Mod),
            AstBinaryOp::Eq => Some(BinOp::Eq),
            AstBinaryOp::Ne => Some(BinOp::Ne),
            AstBinaryOp::Lt => Some(BinOp::Lt),
            AstBinaryOp::Le => Some(BinOp::Le),
            AstBinaryOp::Gt => Some(BinOp::Gt),
            AstBinaryOp::Ge => Some(BinOp::Ge),
            AstBinaryOp::BitAnd => Some(BinOp::BitAnd),
            AstBinaryOp::BitOr => Some(BinOp::BitOr),
            AstBinaryOp::BitXor => Some(BinOp::BitXor),
            AstBinaryOp::Shl => Some(BinOp::Shl),
            AstBinaryOp::Shr => Some(BinOp::Shr),
            // Short-circuit operators handled separately
            AstBinaryOp::And | AstBinaryOp::Or => None,
            // Null coalescing desugars to control flow, not a binary op
            AstBinaryOp::NullCoalesce => None,
        }
    }

    fn lower_binary(
        &mut self,
        expr_id: AstExprId,
        op: AstBinaryOp,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        match op {
            AstBinaryOp::And => {
                return self.lower_short_circuit(expr_id, lhs, rhs, dest, true);
            }
            AstBinaryOp::Or => {
                return self.lower_short_circuit(expr_id, lhs, rhs, dest, false);
            }
            AstBinaryOp::NullCoalesce => {
                return self.lower_null_coalesce(expr_id, lhs, rhs, dest);
            }
            _ => {}
        }

        // Check if TIR already folded this expression to a literal constant
        if self.opt >= crate::OptLevel::Two {
            if let Ty::Literal(ref lit, _) = self.expr_ty(expr_id) {
                let constant = Self::lower_literal(lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
                return;
            }
        }

        let left = self.lower_to_operand(lhs);
        let right = self.lower_to_operand(rhs);
        if let Some(mir_op) = Self::convert_binop(op) {
            self.builder.assign(
                dest,
                Rvalue::BinaryOp {
                    op: mir_op,
                    left,
                    right,
                },
            );
        } else {
            // Fallback — shouldn't happen for well-typed code
            self.emit_panic_call("unsupported binary op", expr_id);
        }
    }

    fn lower_short_circuit(
        &mut self,
        _expr_id: AstExprId,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
        is_and: bool,
    ) {
        let lhs_op = self.lower_to_operand(lhs);

        let bb_rhs = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // ShortCircuit terminator: JumpIfFalse (peek) keeps lhs on TOS
        // when short-circuiting. The rhs block evaluates and leaves its
        // result on TOS. At join, dest is on TOS (PhiLike).
        self.builder
            .short_circuit(lhs_op, is_and, dest.clone(), bb_rhs, bb_join);

        self.builder.set_current_block(bb_rhs);
        self.lower_expr(rhs, dest);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    /// Lower `a ?? b` — evaluate `a`, if null then evaluate `b`, otherwise use `a`.
    fn lower_null_coalesce(
        &mut self,
        _expr_id: AstExprId,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        // Evaluate LHS and store in dest
        let lhs_op = self.lower_to_operand(lhs);
        self.builder
            .assign(dest.clone(), Rvalue::Use(lhs_op.clone()));

        // Test: lhs == null
        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: lhs_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_rhs = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // If null → evaluate RHS, otherwise keep LHS
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), bb_rhs, bb_join);

        self.builder.set_current_block(bb_rhs);
        self.lower_expr(rhs, dest);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    /// Lower `OptionalChain { expr }` — set up shared null exit for the entire chain.
    fn lower_optional_chain(&mut self, _expr_id: AstExprId, inner: AstExprId, dest: Place) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // Push shared null exit
        self.chain_null_exits.push(bb_null);

        // Lower inner expression — Optional* nodes will jump to bb_null on null
        self.lower_expr(inner, dest.clone());

        self.chain_null_exits.pop();

        // Non-null path: goto join
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Null path: assign null, goto join
        self.builder.set_current_block(bb_null);
        self.builder
            .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower an assignment whose target is wrapped in `OptionalChain`.
    /// Sets up null guards, then emits the assignment only on the non-null path.
    fn lower_assign_optional_chain(&mut self, inner_target: AstExprId, value: AstExprId) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // Push shared null exit — Optional* nodes inside will jump here on null
        self.chain_null_exits.push(bb_null);

        // Lower target as lvalue (this will trigger null checks at each ?. node)
        let place = self.lower_lvalue(inner_target);

        // Lower value and assign
        self.lower_expr(value, place);

        self.chain_null_exits.pop();

        // Non-null path: goto join
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Null path: skip assignment, goto join
        self.builder.set_current_block(bb_null);
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower a compound assignment (+=, etc.) whose target is wrapped in `OptionalChain`.
    fn lower_assign_op_optional_chain(
        &mut self,
        inner_target: AstExprId,
        op: AstAssignOp,
        value: AstExprId,
    ) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.chain_null_exits.push(bb_null);

        let place = self.lower_lvalue(inner_target);
        let current = Operand::Copy(place.clone());
        let rhs = self.lower_to_operand(value);
        let mir_op = Self::convert_assign_op(op);
        self.builder.assign(
            place,
            Rvalue::BinaryOp {
                op: mir_op,
                left: current,
                right: rhs,
            },
        );

        self.chain_null_exits.pop();

        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_null);
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower `obj?.member` — null-check obj, then access member or produce null.
    fn lower_optional_member_access(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        field: &Name,
        dest: Place,
    ) {
        let base_op = self.lower_to_operand(base);

        // Test: base == null
        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: base_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_access = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            // Inside an OptionalChain — jump to shared null exit
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_member_access(expr_id, base, field, dest);
            // Don't create our own join — the OptionalChain handler does that
        } else {
            // Standalone (no wrapping OptionalChain) — fall back to own null/join blocks
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_member_access(expr_id, base, field, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    /// Lower `obj?.[index]` — null-check obj, then index or produce null.
    fn lower_optional_index(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        index: AstExprId,
        dest: Place,
    ) {
        let base_op = self.lower_to_operand(base);

        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: base_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_access = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_index(expr_id, base, index, dest);
        } else {
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_index(expr_id, base, index, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    /// Lower `func?.(args)` — null-check callee, then call or produce null.
    fn lower_optional_call(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        dest: Place,
    ) {
        let callee_op = self.lower_to_operand(callee);

        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: callee_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_call = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_call);

            self.builder.set_current_block(bb_call);
            self.lower_call(expr_id, callee, args, dest);
        } else {
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_call);

            self.builder.set_current_block(bb_call);
            self.lower_call(expr_id, callee, args, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    fn lower_unary(&mut self, expr_id: AstExprId, op: AstUnaryOp, expr: AstExprId, dest: Place) {
        // Check if TIR already folded this expression to a literal constant
        if self.opt >= crate::OptLevel::Two {
            if let Ty::Literal(ref lit, _) = self.expr_ty(expr_id) {
                let constant = Self::lower_literal(lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
                return;
            }
        }
        let operand = self.lower_to_operand(expr);
        let mir_op = match op {
            AstUnaryOp::Not => crate::UnaryOp::Not,
            AstUnaryOp::Neg => crate::UnaryOp::Neg,
        };
        self.builder.assign(
            dest,
            Rvalue::UnaryOp {
                op: mir_op,
                operand,
            },
        );
    }
}

// ─── 3.5: Call lowering with builtin detection ────────────────────────────────

impl LoweringContext<'_> {
    fn lower_call_arg_operands(&mut self, expr_id: AstExprId, args: &[AstExprId]) -> Vec<Operand> {
        let Some(plan) = self
            .call_plans
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
        else {
            return args.iter().map(|&a| self.lower_to_operand(a)).collect();
        };

        let provided_args: Vec<_> = plan.provided_args().collect();
        let mut lowered_args = FxHashMap::default();
        for &arg in args {
            if provided_args.contains(&arg) {
                lowered_args.insert(arg, self.lower_to_operand(arg));
            }
        }

        plan.bindings
            .into_iter()
            .map(|binding| match binding {
                baml_compiler2_tir::inference::ParamBinding::Provided { arg, .. } => lowered_args
                    .remove(&arg)
                    .expect("call plan referenced an argument outside the call expression"),
                baml_compiler2_tir::inference::ParamBinding::OmittedDefault { .. } => {
                    Operand::Constant(Constant::OmittedArg)
                }
            })
            .collect()
    }

    fn lower_call(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        dest: Place,
    ) {
        // Check if callee is a member access (potential watch method call)
        let callee_expr = self.body.exprs[callee].clone();
        if let AstExpr::MemberAccess { base, member } = &callee_expr {
            let member_name = member.clone();
            let base_id = *base;
            if member_name.as_str() == "options" || member_name.as_str() == "notify" {
                let args_owned = args.to_vec();
                self.lower_watch_method(expr_id, base_id, &member_name, &args_owned, dest);
                return;
            }
        }

        // Check if callee is a method call (MemberAccess or multi-segment Path with a
        // MemberResolution::BoundMethod/UnboundMethod/Free). Field and Variant resolutions are not callable.
        // If the base is a real value (not a package namespace), prepend it as self.
        let mut receiver_base_for_class_type_args: Option<AstExprId> = None;
        let mut receiver_path_tir_ty: Option<Tir2Ty> = None;
        let (callee_operand, arg_operands) =
            if let AstExpr::MemberAccess { base, .. } = &callee_expr {
                if self
                    .resolutions
                    .get(&self.expr_metadata_key(callee))
                    .is_some_and(|r| {
                        use baml_compiler2_tir::inference::MemberResolution;
                        matches!(
                            r,
                            MemberResolution::BoundMethod { .. }
                                | MemberResolution::UnboundMethod { .. }
                                | MemberResolution::Free { .. }
                        )
                    })
                {
                    // Check if base is a value receiver or a package path.
                    // Package paths have Unknown type in TIR (baml, baml.Array, etc.)
                    let base_is_value = self
                        .expr_types
                        .get(&self.expr_metadata_key(*base))
                        .map(|ty| !matches!(ty, Tir2Ty::Unknown { .. }))
                        .unwrap_or(false);
                    // Check if the resolved method expects a `self` receiver.
                    // Static methods (e.g. StreamCache.new) have no `self` param
                    // and must not get the class reference prepended as an argument.
                    let method_takes_self = {
                        use baml_compiler2_tir::inference::MemberResolution;
                        self.resolutions
                            .get(&self.expr_metadata_key(callee))
                            .is_some_and(|r| match r {
                                MemberResolution::BoundMethod { func_loc, .. }
                                | MemberResolution::UnboundMethod { func_loc, .. }
                                | MemberResolution::Free { func_loc } => {
                                    let sig =
                                        baml_compiler2_ppir::function_signature(self.db, *func_loc);
                                    sig.params
                                        .first()
                                        .is_some_and(|param| param.name.as_str() == "self")
                                }
                                _ => false,
                            })
                    };
                    if base_is_value && method_takes_self {
                        // Instance method call: arr.length() — prepend receiver as self.
                        // For immediate calls, emit the callee as a plain function constant
                        // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                        let receiver_op = self.lower_to_operand(*base);
                        receiver_base_for_class_type_args = Some(*base);
                        let callee_op = {
                            let resolution = self
                                .resolutions
                                .get(&self.expr_metadata_key(callee))
                                .cloned();
                            match resolution
                                .as_ref()
                                .and_then(|r| resolution_to_item_ref(self.db, r))
                            {
                                Some(item) => Operand::Constant(Constant::Function(item)),
                                None => self.lower_to_operand(callee),
                            }
                        };
                        let mut all_args = vec![receiver_op];
                        all_args.extend(self.lower_call_arg_operands(expr_id, args));
                        (callee_op, all_args)
                    } else {
                        // Non-self method or package function reference:
                        // e.g. Factory<int>.create(42), baml.Array.length(array).
                        // Resolve the callee as a plain function constant using
                        // resolution_to_item_ref to avoid lower_member_access emitting
                        // MakeBoundMethod (which would try to load the base type as a
                        // runtime value).
                        let callee_op = {
                            let resolution = self
                                .resolutions
                                .get(&self.expr_metadata_key(callee))
                                .cloned();
                            match resolution
                                .as_ref()
                                .and_then(|r| resolution_to_item_ref(self.db, r))
                            {
                                Some(item) => Operand::Constant(Constant::Function(item)),
                                None => self.lower_to_operand(callee),
                            }
                        };
                        (callee_op, self.lower_call_arg_operands(expr_id, args))
                    }
                } else {
                    let callee_op = self.lower_to_operand(callee);
                    (callee_op, self.lower_call_arg_operands(expr_id, args))
                }
            } else if let AstExpr::Path(segments) = &callee_expr {
                // Check path_member_resolutions first (local-rooted paths like `self.method()`
                // or `obj.field.method()`). The last resolution determines if the final segment
                // is a method call (e.g. for `user.profile.items.slice`, resolutions are
                // [Field{profile}, Field{items}, Method{slice}] — last() is Method).
                let is_local_method = segments.len() >= 2
                    && self
                        .path_member_resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|resolutions| resolutions.last())
                        .is_some_and(|r| {
                            use baml_compiler2_tir::inference::MemberResolution;
                            matches!(
                                r,
                                MemberResolution::BoundMethod { .. }
                                    | MemberResolution::UnboundMethod { .. }
                            )
                        });
                // Also check flat resolutions (package-path method call, kept for compatibility).
                let is_pkg_method = !is_local_method
                    && segments.len() >= 2
                    && self
                        .resolutions
                        .get(&self.expr_metadata_key(callee))
                        .is_some_and(|r| {
                            use baml_compiler2_tir::inference::MemberResolution;
                            matches!(
                                r,
                                MemberResolution::BoundMethod { .. }
                                    | MemberResolution::UnboundMethod { .. }
                            )
                        });

                if is_local_method {
                    // Multi-segment path callee with a local-rooted Method resolution.
                    // The last segment is the method; segments[0..n-1] form the receiver.
                    // e.g. `self.method()` → receiver=self, `user.profile.items.slice()` → receiver=user.profile.items.
                    //
                    // For immediate calls we emit the callee as a plain function constant
                    // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                    let receiver_segments = &segments[..segments.len() - 1];
                    let method_resolution = self
                        .path_member_resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|resolutions| resolutions.last())
                        .cloned();
                    let callee_op = match method_resolution
                        .as_ref()
                        .and_then(|r| resolution_to_item_ref(self.db, r))
                    {
                        Some(item) => Operand::Constant(Constant::Function(item)),
                        None => self.lower_to_operand(callee),
                    };
                    let receiver_op = if receiver_segments.len() == 1 {
                        // Simple local variable receiver (e.g. `self`).
                        if let Some(&recv_local) = self.locals.get(&receiver_segments[0]) {
                            Operand::Copy(Place::Local(recv_local))
                        } else if let Some(cap_idx) =
                            self.capture_index_for_name_at(callee, &receiver_segments[0])
                        {
                            Operand::Copy(Place::Capture(cap_idx))
                        } else {
                            Operand::Constant(Constant::Null)
                        }
                    } else {
                        // Multi-segment receiver (e.g. `user.profile.items`): lower as field chain.
                        let recv_ty = self.expr_ty(callee); // approximation; actual type not critical here
                        let recv_local = self.builder.temp(recv_ty);
                        self.lower_multi_segment_path_as_field_chain(
                            callee,
                            receiver_segments,
                            Place::local(recv_local),
                        );
                        Operand::Copy(Place::local(recv_local))
                    };
                    let prefix_idx = segments.len() - 2;
                    receiver_path_tir_ty = self
                        .path_segment_types
                        .get(&(self.current_metadata_scope, callee, prefix_idx))
                        .cloned();
                    let mut all_args = vec![receiver_op];
                    all_args.extend(self.lower_call_arg_operands(expr_id, args));
                    (callee_op, all_args)
                } else if is_pkg_method {
                    // Package-path method call (via flat resolutions): same treatment.
                    // For immediate calls, emit the callee as a plain function constant
                    // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                    let flat_resolution = self
                        .resolutions
                        .get(&self.expr_metadata_key(callee))
                        .cloned();
                    let callee_op = match flat_resolution
                        .as_ref()
                        .and_then(|r| resolution_to_item_ref(self.db, r))
                    {
                        Some(item) => Operand::Constant(Constant::Function(item)),
                        None => self.lower_to_operand(callee),
                    };
                    let first_seg = &segments[0];
                    let receiver_op = if let Some(&receiver_local) = self.locals.get(first_seg) {
                        Some(Operand::Copy(Place::Local(receiver_local)))
                    } else {
                        self.capture_index_for_name_at(callee, first_seg)
                            .map(|cap_idx| Operand::Copy(Place::Capture(cap_idx)))
                    };
                    if let Some(receiver_op) = receiver_op {
                        let prefix_idx = segments.len() - 2;
                        receiver_path_tir_ty = self
                            .path_segment_types
                            .get(&(self.current_metadata_scope, callee, prefix_idx))
                            .cloned();
                        let mut all_args = vec![receiver_op];
                        all_args.extend(self.lower_call_arg_operands(expr_id, args));
                        (callee_op, all_args)
                    } else {
                        (callee_op, self.lower_call_arg_operands(expr_id, args))
                    }
                } else {
                    let callee_op = self.lower_to_operand(callee);
                    (callee_op, self.lower_call_arg_operands(expr_id, args))
                }
            } else {
                let callee_op = self.lower_to_operand(callee);
                (callee_op, self.lower_call_arg_operands(expr_id, args))
            };

        let target = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);

        // Check if callee is `reflect.type_of<T>()` — a value-producing intrinsic.
        // Unlike void intrinsics (log.*, baml.events.send), this emits an assignment
        // of `Rvalue::LoadType(template)` to `dest` rather than a StatementKind::Intrinsic.
        if let Some(template) = self.check_type_of_intrinsic(callee, expr_id) {
            self.builder.assign(dest, Rvalue::LoadType(template));
            self.builder.goto(target);
            self.builder.set_current_block(target);
            return;
        }

        // Check if callee is a compiler intrinsic (log.*, baml.events.send).
        // Intrinsics are void side effects — emit as a statement, not a call.
        if let Some(op) = self.check_intrinsic(callee) {
            self.builder.push_statement(
                StatementKind::Intrinsic {
                    op,
                    args: arg_operands,
                },
                None,
            );
            self.builder.goto(target);
            self.builder.set_current_block(target);
            return;
        }

        // ── Emit LoadType temps for explicit type arguments ──────────────────
        // When the call carries explicit type args (e.g. `describe<User>()` or
        // `fwd<T>()` where T forwards to `described_type<T>()`), materialise
        // each as a `type` value on the stack before the regular value args.
        // The VM pops these `ntypeargs` Object::Type values into the new frame's
        // `type_args` vec so that inner `reflect.type_of<T>()` calls can
        // substitute them at runtime.
        let ast_type_args: Vec<AstTypeExpr> =
            if let AstExpr::Call { type_args, .. } = &self.body.exprs[expr_id] {
                type_args.clone()
            } else {
                vec![]
            };
        let explicit_type_arg_operands = self.lower_explicit_type_args(&ast_type_args);

        // ── Prepend receiver's class-level type args ─────────────────────────
        // For `b.describe()` where `b: Box<int>`, the method `describe` is compiled
        // as a direct call `describe(b)` (not via MakeBoundMethod). The VM's
        // BoundMethod path for seeding frame.type_args is bypassed, so we instead
        // emit LoadType for each class-level type arg and prepend them as leading
        // call-site type args.  This preserves De Bruijn ordering:
        //   frame.type_args = [class_T, class_U, ..., fn_A, fn_B, ...]
        // matching `enclosing_generic_params()` = class_params ++ fn_params.
        //
        // There are two receiver paths:
        //   1. MemberAccess callee (`base.method()`): receiver type from `expr_types[recv_base_id]`.
        //   2. Path callee (`b.describe()` compiled as Path(["b","describe"])): receiver type
        //      from `path_root_types[callee_expr_id]` (TIR records root segment type there).
        let receiver_tir_ty: Option<Tir2Ty> =
            if let Some(recv_base_id) = receiver_base_for_class_type_args {
                self.expr_types
                    .get(&self.expr_metadata_key(recv_base_id))
                    .cloned()
            } else {
                receiver_path_tir_ty
            };
        let receiver_class_type_arg_operands: Vec<Operand> = match receiver_tir_ty {
            Some(Tir2Ty::Class(_, class_type_args, _)) if !class_type_args.is_empty() => {
                let generic_params = self.enclosing_generic_params();
                class_type_args
                    .iter()
                    .map(|ty_arg| {
                        let template = self.ty_to_template(ty_arg, &generic_params);
                        let temp = self.builder.temp(Ty::type_type());
                        self.builder
                            .assign(Place::local(temp), Rvalue::LoadType(template));
                        Operand::Copy(Place::local(temp))
                    })
                    .collect()
            }
            _ => vec![],
        };

        let type_arg_operands: Vec<Operand> = if !receiver_class_type_arg_operands.is_empty() {
            let mut combined = receiver_class_type_arg_operands;
            combined.extend(explicit_type_arg_operands);
            combined
        } else {
            explicit_type_arg_operands
        };
        let ntypeargs = type_arg_operands.len();

        // Prepend type-arg operands before the value-arg operands.
        // (For regular BAML calls, type args are leading so the callee's frame
        // can pop them into `frame.type_args` before reading value args.)
        let all_arg_operands_for_call = if ntypeargs > 0 {
            let mut combined = type_arg_operands.clone();
            combined.extend(arg_operands.iter().cloned());
            combined
        } else {
            arg_operands.clone()
        };

        // Check if callee resolves to a builtin IO function (sys-op)
        let is_sys_op = self.check_sys_op(callee);

        if is_sys_op {
            // BEP-034 phase D′: sys-ops now lower to a single
            // `Terminator::SysOp` that runs the op inline in the
            // engine and binds the return value directly into `dest`
            // — no intermediate `Future` heap object, no separate
            // `Await` terminator, no `FutureManager` entry.
            //
            // The bytecode emit just becomes:
            //     <args ...>
            //     SYS_OP g
            //     <store dest>
            let dest_local = match dest {
                Place::Local(l) => l,
                _ => self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                }),
            };
            // For generic IO builtins (`$rust_io_function` with type
            // params), the compiler injects synthetic trailing
            // value-arg slots — one `baml_type::Ty` per type
            // parameter.  The Rust glue reads them positionally after
            // the regular value args.  We therefore append type-arg
            // operands AFTER the value args here (unlike regular BAML
            // calls where they are prepended as leading args).
            let sys_op_arg_operands = if ntypeargs > 0 {
                let mut combined = arg_operands;
                combined.extend(type_arg_operands);
                combined
            } else {
                arg_operands
            };
            self.builder.sys_op(
                callee_operand,
                sys_op_arg_operands,
                Place::Local(dest_local),
                target,
                unwind,
            );
        } else {
            // Call destinations must be Place::Local in MIR. If `dest` is a
            // projection (Field/Index) or a capture, call into a temp local
            // first, then assign from the temp to the real destination.
            match &dest {
                Place::Local(_) => {
                    self.builder.call_with_type_args(
                        callee_operand,
                        all_arg_operands_for_call,
                        ntypeargs,
                        dest,
                        target,
                        unwind,
                    );
                }
                _ => {
                    let call_ty = self.expr_ty(expr_id);
                    let tmp = self.builder.temp(call_ty);
                    self.builder.call_with_type_args(
                        callee_operand,
                        all_arg_operands_for_call,
                        ntypeargs,
                        Place::local(tmp),
                        target,
                        unwind,
                    );
                    self.builder.set_current_block(target);
                    let after = self.builder.create_block();
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::local(tmp))));
                    self.builder.goto(after);
                    self.builder.set_current_block(after);
                    return;
                }
            }
        }

        self.builder.set_current_block(target);
    }

    fn check_sys_op(&self, callee: AstExprId) -> bool {
        use baml_compiler2_ast::BuiltinKind;

        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                // Multi-segment: check path_member_resolutions first (local-rooted paths
                // like `file.read_string`), then fall back to flat resolutions (package paths).
                // The last resolution in path_member_resolutions is the final-segment resolution.
                use baml_compiler2_tir::inference::MemberResolution;
                let from_pmr = self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| match res {
                        MemberResolution::Free { func_loc } => Some(*func_loc),
                        MemberResolution::BoundMethod { func_loc, .. }
                        | MemberResolution::UnboundMethod { func_loc, .. } => Some(*func_loc),
                        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                    });
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|res| match res {
                            MemberResolution::Free { func_loc } => Some(*func_loc),
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. } => Some(*func_loc),
                            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                                None
                            }
                        })
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                    return true;
                }
            }
        }

        // ── NEW: MemberAccess callee (e.g. f.read, sock.recv) ──────────────────
        if let AstExpr::MemberAccess { .. } = &self.body.exprs[callee] {
            use baml_compiler2_tir::inference::MemberResolution;
            if let Some(resolution) = self.resolutions.get(&self.expr_metadata_key(callee)) {
                let func_loc = match resolution {
                    MemberResolution::BoundMethod { func_loc, .. }
                    | MemberResolution::UnboundMethod { func_loc, .. } => Some(*func_loc),
                    MemberResolution::Free { func_loc } => Some(*func_loc),
                    MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                };
                if let Some(fl) = func_loc {
                    let body = baml_compiler2_ppir::function_body(self.db, fl);
                    if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if the callee resolves to a `$compiler_intrinsic` function and return the
    /// corresponding `IntrinsicOp`. Follows the same resolution pattern as `check_sys_op`.
    fn check_intrinsic(&self, callee: AstExprId) -> Option<IntrinsicOp> {
        use baml_compiler2_ast::BuiltinKind;

        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                use baml_compiler2_tir::inference::MemberResolution;
                let from_pmr = self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| match res {
                        MemberResolution::Free { func_loc } => Some(*func_loc),
                        MemberResolution::BoundMethod { func_loc, .. }
                        | MemberResolution::UnboundMethod { func_loc, .. } => Some(*func_loc),
                        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                    });
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|res| match res {
                            MemberResolution::Free { func_loc } => Some(*func_loc),
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. } => Some(*func_loc),
                            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                                None
                            }
                        })
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(BuiltinKind::Intrinsic) = body.as_ref() {
                    let item_ref = def_to_item_ref(self.db, Definition::Function(fl));
                    return match item_ref.to_string().as_str() {
                        "log.info" => Some(IntrinsicOp::Log(LogLevel::Info)),
                        "log.debug" => Some(IntrinsicOp::Log(LogLevel::Debug)),
                        "log.warn" => Some(IntrinsicOp::Log(LogLevel::Warn)),
                        "log.error" => Some(IntrinsicOp::Log(LogLevel::Error)),
                        "baml.events.send" => Some(IntrinsicOp::SendEvent),
                        _ => None,
                    };
                }
            }
        }

        None
    }
}

// ─── 3.6: reflect.type_of intrinsic ─────────────────────────────────────────

impl LoweringContext<'_> {
    /// Detect a `reflect.type_of<T>()` call and, if found, resolve the type
    /// argument and return the corresponding `TyTemplate`.
    ///
    /// Returns `Some(template)` when:
    /// - The callee is the `baml.reflect.type_of` `$compiler_intrinsic`.
    /// - The call carries exactly one type argument.
    /// - The type argument resolves to a concrete `Ty` (no `TypeVar` leaves).
    ///
    /// Returns `None` when the callee is not `type_of` **or** when the type
    /// argument contains a `TypeVar` (generic-parameter reference).  The latter
    /// case is deferred to template lowering, which produces
    /// `TyTemplate::TypeArgRef` leaves; attempting it here would emit a broken
    /// `LoadType` instruction.
    fn check_type_of_intrinsic(
        &self,
        callee: AstExprId,
        call_expr_id: AstExprId,
    ) -> Option<TyTemplate> {
        use baml_compiler2_ast::BuiltinKind;
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;

        // ── 1. Check the callee resolves to `baml.reflect.type_of` ──────────
        let func_loc = if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = baml_compiler2_tir::resolve::resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    baml_compiler2_tir::resolve::ResolvedName::Builtin(
                        baml_compiler2_hir::contributions::Definition::Function(fl),
                    ) => Some(fl),
                    baml_compiler2_tir::resolve::ResolvedName::Item(
                        baml_compiler2_hir::contributions::Definition::Function(fl),
                    ) => Some(fl),
                    _ => None,
                }
            } else {
                use baml_compiler2_tir::inference::MemberResolution;
                let from_pmr = self
                    .path_member_resolutions
                    .get(&self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| match res {
                        MemberResolution::Free { func_loc } => Some(*func_loc),
                        MemberResolution::BoundMethod { func_loc, .. }
                        | MemberResolution::UnboundMethod { func_loc, .. } => Some(*func_loc),
                        MemberResolution::Field { .. } | MemberResolution::Variant { .. } => None,
                    });
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.resolutions
                        .get(&self.expr_metadata_key(callee))
                        .and_then(|res| match res {
                            MemberResolution::Free { func_loc } => Some(*func_loc),
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. } => Some(*func_loc),
                            MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                                None
                            }
                        })
                }
            }
        } else {
            None
        }?;

        let body = baml_compiler2_ppir::function_body(self.db, func_loc);
        if !matches!(
            body.as_ref(),
            baml_compiler2_hir::body::FunctionBody::Builtin(BuiltinKind::Intrinsic)
        ) {
            return None;
        }
        let item_ref = def_to_item_ref(
            self.db,
            baml_compiler2_hir::contributions::Definition::Function(func_loc),
        );
        if item_ref.to_string().as_str() != "reflect.type_of" {
            return None;
        }

        // ── 2. Extract the single type argument ─────────────────────────────
        let type_args = if let AstExpr::Call { type_args, .. } = &self.body.exprs[call_expr_id] {
            type_args.clone()
        } else {
            return None;
        };
        let type_arg = type_args.into_iter().next()?;

        // ── 3. Lower the type expression to a Tir2Ty ────────────────────────
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);

        // Include the enclosing class + function generic params so that `T`
        // in `reflect.type_of<T>()` resolves to `Tir2Ty::TypeVar("T")` rather
        // than an unresolved-type error — both for free generic functions and
        // for methods on generic classes.  The order (class params first,
        // then function params) mirrors TIR's `enclosing_class_generic_params
        // ++ user_generic_params` convention used in `callable.rs`.
        let generic_params = self.enclosing_generic_params();

        let mut diags = Vec::new();
        let tir_ty = lower_type_expr_in_ns(
            self.db,
            &type_arg,
            pkg_items,
            &pkg_info.namespace_path,
            &generic_params,
            &mut diags,
        );

        // ── 4. Build TyTemplate — TypeVar → TypeArgRef(N) ─────────────────────
        let template = self.ty_to_template(&tir_ty, &generic_params);
        Some(template)
    }

    /// Recursively convert a `Tir2Ty` to a `TyTemplate`.
    ///
    /// `Tir2Ty::TypeVar("T")` whose name appears at position `N` in
    /// `generic_params` maps to `TyTemplate::TypeArgRef(N)`.  All other types
    /// recurse structurally and bottom out at `TyTemplate::Concrete(...)`.
    fn ty_to_template(&self, ty: &Tir2Ty, generic_params: &[baml_base::Name]) -> TyTemplate {
        match ty {
            Tir2Ty::TypeVar(name, _) => {
                // Find the de Bruijn index in the enclosing function's param list.
                if let Some(n) = generic_params.iter().position(|p| p == name) {
                    TyTemplate::TypeArgRef(
                        u32::try_from(n).expect("generic param index fits in u32"),
                    )
                } else {
                    // TypeVar not found in enclosing params — defensive fallback.
                    // This should not happen for well-typed programs.
                    TyTemplate::Concrete(Ty::Void {
                        attr: baml_type::TyAttr::default(),
                    })
                }
            }
            Tir2Ty::List(inner, _) => {
                TyTemplate::Array(Box::new(self.ty_to_template(inner, generic_params)))
            }
            Tir2Ty::Optional(inner, _) => {
                TyTemplate::Optional(Box::new(self.ty_to_template(inner, generic_params)))
            }
            Tir2Ty::Map(k, v, _) => TyTemplate::Map(
                Box::new(self.ty_to_template(k, generic_params)),
                Box::new(self.ty_to_template(v, generic_params)),
            ),
            Tir2Ty::Union(parts, _) => TyTemplate::Union(
                parts
                    .iter()
                    .map(|p| self.ty_to_template(p, generic_params))
                    .collect(),
            ),
            Tir2Ty::Class(qtn, type_args, attr) => {
                if type_args
                    .iter()
                    .any(baml_compiler2_tir::generics::contains_typevar)
                {
                    // Generic class instantiation with type-variable args.
                    let template_args: Vec<TyTemplate> = type_args
                        .iter()
                        .map(|a| self.ty_to_template(a, generic_params))
                        .collect();
                    TyTemplate::Class(qtn_to_type_name(qtn), template_args)
                } else {
                    // Monomorphic class — no TypeVars in args.
                    let resolved_args: Vec<Ty> = type_args
                        .iter()
                        .map(|a| convert_tir2_ty(a, &self.resolved_aliases))
                        .collect();
                    TyTemplate::Concrete(Ty::Class(
                        qtn_to_type_name(qtn),
                        resolved_args,
                        attr.clone(),
                    ))
                }
            }
            // EvolvingList and EvolvingMap: treat like their non-evolving counterparts.
            Tir2Ty::EvolvingList(inner, _) => {
                TyTemplate::Array(Box::new(self.ty_to_template(inner, generic_params)))
            }
            Tir2Ty::EvolvingMap(k, v, _) => TyTemplate::Map(
                Box::new(self.ty_to_template(k, generic_params)),
                Box::new(self.ty_to_template(v, generic_params)),
            ),
            // All remaining concrete leaf types.
            other => TyTemplate::Concrete(convert_tir2_ty(other, &self.resolved_aliases)),
        }
    }

    /// Return the list of generic parameter names in scope for the
    /// **enclosing** function being lowered.  Empty for top-level expressions
    /// that have no enclosing generic function.
    ///
    /// When the enclosing function is a method on a generic class, the
    /// class-level params come first, followed by the function-level params
    /// — matching TIR's `enclosing_class_generic_params ++ generic_params`
    /// convention (see `baml_compiler2_tir::callable`).  This keeps MIR's
    /// view of in-scope generics consistent with how TIR types the body.
    ///
    /// **Runtime caveat**: the runtime ABI does not yet thread class-level
    /// type args through method calls (a method's `frame.type_args` is
    /// populated only from explicit call-site `<...>` args, not from the
    /// receiver's class type args).  Class-level generics resolve correctly
    /// at the MIR layer via this helper, but `reflect.type_of<T>()` where
    /// `T` is a class-level param will substitute to `Ty::unknown` at
    /// runtime until that gap is closed.
    fn enclosing_generic_params(&self) -> Vec<baml_base::Name> {
        let Some(fl) = self.func_loc else {
            return Vec::new();
        };
        let item_tree = file_item_tree(self.db, fl.file(self.db));
        let func_id = fl.id(self.db);
        let mut params: Vec<baml_base::Name> = item_tree
            .classes
            .values()
            .find(|class_data| class_data.methods.contains(&func_id))
            .map(|class_data| class_data.generic_params.clone())
            .unwrap_or_default();
        params.extend(item_tree[func_id].generic_params.iter().cloned());
        params
    }

    /// Emit `LoadType` rvalue assignments for the explicit type arguments of a
    /// generic call and return the resulting operands plus the count.
    ///
    /// For each `TypeExpr` in `ast_type_args`:
    /// 1. Lowers it to `Tir2Ty` (respecting the enclosing generic params so
    ///    that `T` resolves to `Tir2Ty::TypeVar("T")` rather than an error).
    /// 2. Converts it to a `TyTemplate` via `ty_to_template` (`TypeVar` → `TypeArgRef(N)`).
    /// 3. Assigns `Rvalue::LoadType(template)` to a fresh `type`-typed temp.
    /// 4. Appends that temp as an `Operand::Copy` to the returned vec.
    ///
    /// Returns `(type_arg_operands, ntypeargs)` — the number equals
    /// `ast_type_args.len()`.  Returns an empty vec when there are no type args.
    fn lower_explicit_type_args(&mut self, ast_type_args: &[AstTypeExpr]) -> Vec<Operand> {
        use baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns;

        if ast_type_args.is_empty() {
            return vec![];
        }

        let generic_params = self.enclosing_generic_params();
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);

        let type_ty = baml_type::Ty::Opaque(
            baml_type::TypeName {
                name: baml_base::Name::new("Type"),
                module_path: vec![
                    baml_base::Name::new("baml"),
                    baml_base::Name::new("reflect"),
                ],
                display_name: baml_base::Name::new("type"),
            },
            baml_type::TyAttr::default(),
        );

        let mut operands = Vec::with_capacity(ast_type_args.len());
        for type_arg in ast_type_args {
            let mut diags = Vec::new();
            let tir_ty = lower_type_expr_in_ns(
                self.db,
                type_arg,
                pkg_items,
                &pkg_info.namespace_path,
                &generic_params,
                &mut diags,
            );
            // Ignore diagnostics here — TIR already validated the type args.
            let template = self.ty_to_template(&tir_ty, &generic_params);
            let temp = self.builder.temp(type_ty.clone());
            self.builder
                .assign(Place::local(temp), Rvalue::LoadType(template));
            operands.push(Operand::Copy(Place::local(temp)));
        }
        operands
    }
}

// ─── 3.7: Helper methods ─────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_to_operand(&mut self, expr_id: AstExprId) -> Operand {
        let ty = self.expr_ty(expr_id);
        let temp = self.builder.temp(ty);
        self.lower_expr(expr_id, Place::local(temp));
        Operand::Copy(Place::Local(temp))
    }

    fn emit_panic_call(&mut self, message: &str, _expr_id: AstExprId) {
        // Emit a call to baml.sys.panic with the error message
        let callee = Operand::Constant(Constant::Function(ItemRef::Free {
            package: Name::new("baml"),
            namespace: vec![Name::new("sys")],
            name: Name::new("panic"),
        }));
        let msg = Operand::Constant(Constant::String(message.to_string()));
        let temp = self.builder.temp(Ty::Null {
            attr: TyAttr::default(),
        });
        let unreachable_block = self.builder.create_block();
        self.builder.call(
            callee,
            vec![msg],
            Place::local(temp),
            unreachable_block,
            None,
        );
        self.builder.set_current_block(unreachable_block);
        self.builder.unreachable();
        // Start a new block for any code after this (dead code)
        let dead = self.builder.create_block();
        self.builder.set_current_block(dead);
    }

    fn lower_if(
        &mut self,
        _expr_id: AstExprId,
        condition: AstExprId,
        then_branch: AstExprId,
        else_branch: Option<AstExprId>,
        dest: Place,
    ) {
        let cond_op = self.lower_to_operand(condition);
        let bb_then = self.builder.create_block();
        let bb_else = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.builder.branch(cond_op, bb_then, bb_else);

        self.builder.set_current_block(bb_then);
        self.lower_expr(then_branch, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_else);
        if let Some(else_expr) = else_branch {
            self.lower_expr(else_expr, dest);
        } else {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    fn lower_object(
        &mut self,
        expr_id: AstExprId,
        type_name: Option<&TypePath>,
        fields: &[(Name, AstExprId)],
        spreads: &[baml_compiler2_ast::SpreadField],
        dest: Place,
    ) {
        // Prefer the explicitly written type name. If absent (e.g., when the
        // type is a qualified path like `baml.errors.DevOther`), fall back to
        // the TIR-inferred type to get the short class name.
        //
        // We also extract a `TypeName` for looking up fields in `class_fields`,
        // which is keyed by `TypeName`.
        let ty = self.expr_ty(expr_id);
        let type_name_key: Option<TypeName> = match &ty {
            Ty::Class(tn, _, _) => Some(tn.clone()),
            _ => None,
        };
        // Prefer the TIR-resolved fully-qualified name (`<package>.<ns>.<name>`)
        // because that matches the bytecode emitter's FQN registry. The parser
        // stores qualified paths verbatim from source (e.g. `root.http.Response`
        // for user types), but the emitter registers user types under the `user.`
        // prefix — so the source-verbatim form would miss the lookup. Falling
        // back to the parser name only when TIR has no type info handles
        // synthetic Object exprs from `lower_cst.rs` that already use registry-
        // matching dotted forms like "baml.llm.Client".
        let class_name = if let Some(tn) = &type_name_key {
            let mut parts: Vec<String> = tn.module_path.iter().map(ToString::to_string).collect();
            parts.push(tn.name.to_string());
            parts.join(".")
        } else {
            type_name.map(ToString::to_string).unwrap_or_default()
        };

        if spreads.is_empty() {
            // Lower fields in class-definition order, filling unspecified slots
            // with Null. Source order in the literal does not match definition
            // order, so a partial literal like `ScanOptions { absolute: true }`
            // would otherwise put `absolute` into whichever slot happens to be
            // first. The TIR Object handler resolves the type via its qualified
            // path, so `class_fields.get(tn)` always finds the definition for
            // any user-written class literal.
            let field_operands: Vec<Operand> = if let Some(field_name_to_idx) = type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
                .cloned()
            {
                let mut result: Vec<Operand> = (0..field_name_to_idx.len())
                    .map(|_| Operand::Constant(Constant::Null))
                    .collect();
                for (name, expr) in fields {
                    if let Some(&idx) = field_name_to_idx.get(&name.to_string()) {
                        result[idx] = self.lower_to_operand(*expr);
                    }
                }
                result
            } else {
                // Synthetic Object exprs without TIR type info (e.g. compiler
                // sugar for retry policies) fall back to source order. These
                // construction sites build full, ordered literals so the order
                // matches the class definition.
                fields
                    .iter()
                    .map(|(_, e)| self.lower_to_operand(*e))
                    .collect()
            };
            let type_arg_templates = self.class_type_arg_templates(expr_id);
            self.builder.assign(
                dest,
                Rvalue::Aggregate {
                    kind: AggregateKind::Class {
                        name: class_name,
                        type_arg_templates,
                    },
                    fields: field_operands,
                },
            );
        } else {
            // Lower spread base(s) and explicit fields eagerly (in source
            // order), then assemble the aggregate respecting override semantics:
            // later source entries override earlier ones for the same class field.

            enum Entry {
                Spread(Local),
                Named(String, Operand),
            }

            let field_count = type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
                .map(indexmap::IndexMap::len)
                .unwrap_or(0);

            // Lower all spread expressions into locals.
            let spread_locals: Vec<(usize, Local)> = spreads
                .iter()
                .map(|s| {
                    let op = self.lower_to_operand(s.expr);
                    let ty = self.expr_ty(s.expr);
                    (s.position, self.operand_to_local(op, ty))
                })
                .collect();

            // Lower all explicit field expressions into operands.
            // Named fields occupy source positions 0.. excluding spread positions.
            // Assign each named field its source position by counting up and
            // skipping positions occupied by spreads.
            let spread_positions: HashSet<usize> = spreads.iter().map(|s| s.position).collect();
            let explicit_with_pos: Vec<(usize, String, Operand)> = {
                let mut pos = 0usize;
                fields
                    .iter()
                    .map(|(name, e)| {
                        while spread_positions.contains(&pos) {
                            pos += 1;
                        }
                        let cur = pos;
                        pos += 1;
                        (cur, name.to_string(), self.lower_to_operand(*e))
                    })
                    .collect()
            };

            // Build per-class-field operand array. Process all entries in source
            // position order; later entries overwrite earlier ones.
            let field_name_to_idx: &IndexMap<String, usize> = match type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
            {
                Some(m) => m,
                None => {
                    // Unknown class — just emit named fields in order.
                    let field_operands: Vec<Operand> = fields
                        .iter()
                        .map(|(_, e)| self.lower_to_operand(*e))
                        .collect();
                    let type_arg_templates = self.class_type_arg_templates(expr_id);
                    self.builder.assign(
                        dest,
                        Rvalue::Aggregate {
                            kind: AggregateKind::Class {
                                name: class_name,
                                type_arg_templates,
                            },
                            fields: field_operands,
                        },
                    );
                    return;
                }
            };

            // Merge all entries into a single sorted list by source position.
            let mut entries: Vec<(usize, Entry)> = Vec::new();
            for (pos, local) in &spread_locals {
                entries.push((*pos, Entry::Spread(*local)));
            }
            for (pos, name, op) in explicit_with_pos {
                entries.push((pos, Entry::Named(name, op)));
            }
            entries.sort_by_key(|(pos, _)| *pos);

            // Initialize all fields to null, then apply entries in order.
            let mut result: Vec<Operand> = (0..field_count)
                .map(|_| Operand::Constant(Constant::Null))
                .collect();

            for (_, entry) in &entries {
                match entry {
                    Entry::Spread(local) => {
                        // A spread fills every field from the base object.
                        for (idx, slot) in result.iter_mut().enumerate().take(field_count) {
                            *slot = Operand::Copy(Place::Field {
                                base: Box::new(Place::Local(*local)),
                                field: idx,
                            });
                        }
                    }
                    Entry::Named(name, op) => {
                        if let Some(&idx) = field_name_to_idx.get(name) {
                            result[idx] = op.clone();
                        }
                    }
                }
            }

            let type_arg_templates = self.class_type_arg_templates(expr_id);
            self.builder.assign(
                dest,
                Rvalue::Aggregate {
                    kind: AggregateKind::Class {
                        name: class_name,
                        type_arg_templates,
                    },
                    fields: result,
                },
            );
        }
    }

    fn lower_member_access(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        field: &Name,
        dest: Place,
    ) {
        // Check if TIR resolved this to a method or free function — if so, emit a function constant
        // (unbound) or MakeBoundMethod (bound). Field and Variant resolutions fall through to the
        // existing lowering paths below.
        if let Some(resolution) = self
            .resolutions
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
        {
            use baml_compiler2_tir::inference::MemberResolution;
            match &resolution {
                MemberResolution::BoundMethod { .. } => {
                    // Bound method reference: lower receiver and emit MakeBoundMethod.
                    let item = resolution_to_item_ref(self.db, &resolution);
                    if let Some(item) = item {
                        let receiver_op = self.lower_to_operand(base);
                        self.builder.assign(
                            dest,
                            Rvalue::MakeBoundMethod {
                                item_ref: item,
                                receiver: receiver_op,
                            },
                        );
                        return;
                    }
                }
                MemberResolution::UnboundMethod { .. } | MemberResolution::Free { .. } => {
                    // Unbound method or free function reference: emit a plain function constant.
                    let item = resolution_to_item_ref(self.db, &resolution);
                    if let Some(item) = item {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(item))),
                        );
                        return;
                    }
                }
                MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                    // Fall through — handled by the existing field/enum-variant lowering below
                }
            }
        }

        // Check if TIR resolved this to an enum variant (e.g. baml.HttpMethod.Get via package path)
        if let Some(Tir2Ty::EnumVariant(qtn, variant, _)) = self
            .expr_types
            .get(&self.expr_metadata_key(expr_id))
            .cloned()
            .as_ref()
        {
            let enum_ref = ItemRef::EnumType {
                package: qtn.package().clone(),
                namespace: qtn.namespace().clone(),
                name: qtn.name().clone(),
            };
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                    enum_ref,
                    variant: variant.clone(),
                })),
            );
            return;
        }

        // Check if this is a package path intermediate (e.g. `baml.HttpMethod` in
        // `baml.HttpMethod.Get`). TIR marks these as Ty::Unknown. Emit null placeholder.
        // CRITICAL: only treat the expression as a namespace intermediate if the BASE
        // is also Unknown (i.e. `baml` in `baml.HttpMethod`). If the base has a
        // concrete type, this is a real field access whose field type happens to be
        // Unknown (unresolved type annotation). In that case, fall through to emit
        // the field projection.
        if let Some(Tir2Ty::Unknown { .. }) = self.expr_types.get(&self.expr_metadata_key(expr_id))
        {
            let base_is_also_unknown = self
                .expr_types
                .get(&self.expr_metadata_key(base))
                .map(|ty| matches!(ty, Tir2Ty::Unknown { .. }))
                .unwrap_or(true);
            if base_is_also_unknown {
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                return;
            }
            // Base is a real value (non-Unknown type) — fall through to field projection
        }

        // Regular field access
        let base_ty = self.expr_ty(base);
        let base_op = self.lower_to_operand(base);
        let field_str = field.to_string();

        // Unwrap Optional — when called from lower_optional_member_access,
        // the base type is T? but we've already null-checked, so use the inner type.
        let unwrapped_ty = match &base_ty {
            Ty::Optional(inner, _) => inner.as_ref(),
            _ => &base_ty,
        };

        // Look up field index from class_fields
        let field_idx = if let Ty::Class(tn, _, _) = unwrapped_ty {
            self.class_fields
                .get(tn)
                .and_then(|fields| fields.get(&field_str))
                .copied()
        } else {
            None
        };

        let base_local = self.operand_to_local(base_op, base_ty.clone());

        if let Some(idx) = field_idx {
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(base_local)),
                    field: idx,
                })),
            );
        } else if self.lower_union_class_field_access(
            expr_id,
            base_local,
            unwrapped_ty,
            field,
            &dest,
        ) {
        } else {
            if let Ty::Class(tn, _, _) = unwrapped_ty {
                self.emit_panic_call(
                    &format!(
                        "internal compiler error: MIR failed to resolve field access \
                         .{} against class definition '{}' (module_path: {:?}). \
                         This class should be in class_fields but isn't.",
                        field_str, tn.name, tn.module_path,
                    ),
                    expr_id,
                );
                return;
            }
            // Dynamic map access — only valid for map types, unknown, etc.
            let key_local = self.builder.temp(Ty::String {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(key_local),
                Rvalue::Use(Operand::Constant(Constant::String(field_str))),
            );
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Copy(Place::Index {
                    base: Box::new(Place::Local(base_local)),
                    index: key_local,
                    kind: IndexKind::Map,
                })),
            );
        }
    }

    fn lower_union_class_field_access(
        &mut self,
        _expr_id: AstExprId,
        base_local: Local,
        base_ty: &Ty,
        field: &Name,
        dest: &Place,
    ) -> bool {
        let Some(candidates) = self.class_union_field_candidates(base_ty, field) else {
            return false;
        };

        let bb_entry = self.builder.current_block();
        let bb_join = self.builder.create_block();
        let bb_otherwise = self.builder.create_block();

        let tag_local = self.builder.temp(Ty::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(tag_local),
            Rvalue::TypeTag(Place::local(base_local)),
        );

        let mut arms = Vec::with_capacity(candidates.len());
        let mut arm_names = Vec::with_capacity(candidates.len());
        for (tag, class_name, field_idx) in candidates {
            let bb_body = self.builder.create_block();
            arms.push((tag, bb_body));
            arm_names.push((tag, class_name.name.to_string()));

            self.builder.set_current_block(bb_body);
            self.builder.assign(
                dest.clone(),
                Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(base_local)),
                    field: field_idx,
                })),
            );
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_otherwise);
        self.builder.unreachable();

        self.builder.set_current_block(bb_entry);
        self.builder.switch(
            Operand::Copy(Place::Local(tag_local)),
            arms,
            bb_otherwise,
            true,
            arm_names,
        );
        self.builder.set_current_block(bb_join);
        true
    }

    fn class_union_field_candidates(
        &self,
        ty: &Ty,
        field: &Name,
    ) -> Option<Vec<(i64, TypeName, usize)>> {
        let Ty::Union(members, _) = ty else {
            return None;
        };

        let mut candidates = Vec::new();
        for member in members {
            let Ty::Class(class_name, _, _) = member else {
                return None;
            };
            let field_idx = self
                .class_fields
                .get(class_name)
                .and_then(|fields| fields.get(field.as_str()))
                .copied()?;
            let tag = self.class_type_tags.get(class_name).copied()?;
            if !candidates
                .iter()
                .any(|(existing_tag, _, _)| *existing_tag == tag)
            {
                candidates.push((tag, class_name.clone(), field_idx));
            }
        }

        (!candidates.is_empty()).then_some(candidates)
    }

    fn lower_index(&mut self, _expr_id: AstExprId, base: AstExprId, index: AstExprId, dest: Place) {
        let base_ty = self.expr_ty(base);
        let base_op = self.lower_to_operand(base);
        let index_op = self.lower_to_operand(index);
        let index_ty = self.expr_ty(index);

        let base_local = self.operand_to_local(base_op, base_ty.clone());
        let index_local = self.operand_to_local(index_op, index_ty);

        // Unwrap Optional — when called from lower_optional_index,
        // the base type is T? but we've already null-checked.
        let unwrapped_ty = match &base_ty {
            Ty::Optional(inner, _) => inner.as_ref(),
            _ => &base_ty,
        };

        let kind = if matches!(unwrapped_ty, Ty::List(..) | Ty::Uint8Array { .. }) {
            IndexKind::Array
        } else {
            IndexKind::Map
        };

        self.builder.assign(
            dest,
            Rvalue::Use(Operand::Copy(Place::Index {
                base: Box::new(Place::Local(base_local)),
                index: index_local,
                kind,
            })),
        );
    }

    /// If the expression is a simple local variable reference (single-segment path
    /// resolving to a known local), return its Local directly without allocating
    /// a temp or emitting a copy.
    fn try_resolve_to_local(&self, expr_id: AstExprId) -> Option<Local> {
        let expr = &self.body.exprs[expr_id];
        if let AstExpr::Path(segments) = expr {
            if segments.len() == 1 {
                if let Some(&local) = self.locals.get(&segments[0]) {
                    return Some(local);
                }
            }
        }
        None
    }

    /// Convert an operand to a local, materializing a temp if necessary.
    fn operand_to_local(&mut self, op: Operand, ty: Ty) -> Local {
        match op {
            Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) => l,
            _ => {
                let temp = self.builder.temp(ty);
                self.builder.assign(Place::local(temp), Rvalue::Use(op));
                temp
            }
        }
    }

    fn lower_watch_method(
        &mut self,
        _expr_id: AstExprId,
        base: AstExprId,
        method: &Name,
        args: &[AstExprId],
        dest: Place,
    ) {
        // Find the watched local from the base expression
        let base_op = self.lower_to_operand(base);
        let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = base_op
        else {
            // Not a direct local — fall back to regular call lowering
            // (shouldn't happen in well-formed code)
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            return;
        };

        if method.as_str() == "options" {
            // $watch.options(filter) — emit WatchOptions statement
            if let Some(&filter_expr) = args.first() {
                let filter_op = self.lower_to_operand(filter_expr);
                self.builder.watch_options(local, filter_op);
            }
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        } else if method.as_str() == "notify" {
            // $watch.notify() — emit WatchNotify statement
            self.builder.watch_notify(local);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        } else {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
    }
}

// ─── Statement lowering ───────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_stmt(&mut self, stmt_id: AstStmtId) {
        let prev_span = self.builder.current_source_span;
        if let Some(span) = self.span_for_stmt(stmt_id) {
            self.builder.current_source_span = Some(span);
        }

        let stmt = self.body.stmts[stmt_id].clone();
        match stmt {
            AstStmt::Expr(expr_id) => {
                let ty = self.expr_ty(expr_id);
                let temp = self.builder.temp(ty);
                self.lower_expr(expr_id, Place::local(temp));
            }

            AstStmt::Let {
                pattern,
                initializer,
                is_watched,
                ..
            } if self.pattern_contains_structural(pattern) => {
                let local_ty = self.pat_ty(pattern);
                let scrutinee = self.builder.temp(local_ty);

                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(scrutinee));
                } else {
                    self.builder.assign(
                        Place::local(scrutinee),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
                }

                self.bind_pattern_inner(scrutinee, pattern, pattern, pattern, false, is_watched);

                let names: Vec<Name> = self.body.patterns[pattern]
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                for name in names {
                    if let Some(&local) = self.locals.get(&name)
                        && let Some(binding_id) =
                            self.binding_id_for_statement_name(stmt_id, pattern, &name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                }

                if is_watched {
                    for name in self.body.patterns[pattern].bound_names(&self.body.patterns) {
                        if let Some(&local) = self.locals.get(name) {
                            self.watched_locals_stack.push(local);
                        }
                    }
                }
            }

            AstStmt::Let {
                pattern,
                initializer,
                is_watched,
                ..
            } => {
                // Extract binding names from pattern. A simple `let x` has
                // one name; a chain `let x: let y: let z` has three. The
                // first name owns the declared slot (the init writes into
                // it directly); remaining names alias via copy-assignment.
                let pat = self.body.patterns[pattern].clone();
                let names: Vec<Name> = pat
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                let first_name = names.first().cloned();

                let local_ty = self.pat_ty(pattern);
                let local = self.builder.declare_local(
                    first_name.clone(),
                    local_ty.clone(),
                    None,
                    is_watched,
                );

                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(local));
                } else {
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
                }

                if let Some(first_name) = first_name {
                    if let Some(binding_id) =
                        self.binding_id_for_statement_name(stmt_id, pattern, &first_name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                    self.locals.insert(first_name, local);
                }

                // Additional chain-link bindings get their own locals that
                // copy from the first. `let x: let y` ⇒ y = x at runtime.
                for extra in names.iter().skip(1) {
                    let alias = self.builder.declare_local(
                        Some(extra.clone()),
                        local_ty.clone(),
                        None,
                        false,
                    );
                    self.builder.assign(
                        Place::local(alias),
                        Rvalue::Use(Operand::Copy(Place::Local(local))),
                    );
                    if let Some(binding_id) =
                        self.binding_id_for_statement_name(stmt_id, pattern, extra)
                    {
                        self.binding_locals.insert(binding_id, alias);
                    }
                    self.locals.insert(extra.clone(), alias);
                }

                if is_watched {
                    self.watched_locals_stack.push(local);
                }
            }

            AstStmt::While {
                condition,
                body,
                after,
                ..
            } => {
                let bb_cond = self.builder.create_block();
                let bb_body = self.builder.create_block();
                let bb_after = if after.is_some() {
                    self.builder.create_block()
                } else {
                    bb_cond
                };
                let bb_exit = self.builder.create_block();

                let prev_loop = self.loop_context.take();
                let watched_depth = self.watched_locals_stack.len();
                self.loop_context = Some(LoopContext {
                    break_target: bb_exit,
                    continue_target: bb_after,
                    watched_locals_depth: watched_depth,
                });

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_cond);
                }

                self.builder.set_current_block(bb_cond);
                let cond_op = self.lower_to_operand(condition);
                self.builder.branch(cond_op, bb_body, bb_exit);

                self.builder.set_current_block(bb_body);
                let body_ty = self.expr_ty(body);
                let body_temp = self.builder.temp(body_ty);
                self.lower_expr(body, Place::local(body_temp));

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_after);
                }

                if after.is_some() {
                    self.builder.set_current_block(bb_after);
                }
                if let Some(after_stmt) = after {
                    self.lower_stmt(after_stmt);
                }

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_cond);
                }

                self.loop_context = prev_loop;
                self.builder.set_current_block(bb_exit);
            }

            // For-loop desugaring to index-based iteration.
            //
            // We desugar here (at MIR level) rather than at AST level because:
            // - TIR needs the first-class For node for proper diagnostics
            // - Future codegen can pattern-match on the loop structure to emit
            //   native for-loops in target languages
            //
            // The desugaring is:
            //   let __coll = <collection>;
            //   let __idx = 0;
            //   while __idx < len(__coll) {
            //       let <binding> = __coll[__idx];
            //       <body>;
            //       __idx += 1;
            //   }
            AstStmt::For {
                binding,
                collection,
                body,
            } => {
                let saved_locals = self.locals.clone();
                let watched_depth = self.watched_locals_stack.len();

                // 1. Evaluate collection into a temp local
                let coll_ty = self.expr_ty(collection);
                let coll_local = self.builder.temp(coll_ty.clone());
                self.lower_expr(collection, Place::local(coll_local));

                // 2. Determine element type from collection type
                let elem_ty = match &coll_ty {
                    Ty::List(inner, _) => *inner.clone(),
                    _ => Ty::Void {
                        attr: TyAttr::default(),
                    },
                };

                // 3. Create index local, initialized to 0
                let int_ty = Ty::Int {
                    attr: TyAttr::default(),
                };
                let idx_name = self.gensym("__for_idx");
                let idx_local =
                    self.builder
                        .declare_local(Some(idx_name), int_ty.clone(), None, false);
                self.builder.assign(
                    Place::local(idx_local),
                    Rvalue::Use(Operand::Constant(Constant::Int(0))),
                );

                // 4. Create loop blocks: header (condition check), body, after (increment), exit
                let bb_header = self.builder.create_block();
                let bb_body = self.builder.create_block();
                let bb_after = self.builder.create_block();
                let bb_exit = self.builder.create_block();

                // Register loop context so break/continue work inside for-loops
                let prev_loop = self.loop_context.take();
                self.loop_context = Some(LoopContext {
                    break_target: bb_exit,
                    continue_target: bb_after,
                    watched_locals_depth: watched_depth,
                });

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_header);
                }

                // 5. Header: check __idx < len(__coll)
                self.builder.set_current_block(bb_header);
                let len_local = self.builder.temp(int_ty);
                self.builder.assign(
                    Place::local(len_local),
                    Rvalue::Len(Place::local(coll_local)),
                );
                let cond_local = self.builder.temp(Ty::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(cond_local),
                    Rvalue::BinaryOp {
                        op: BinOp::Lt,
                        left: Operand::Copy(Place::Local(idx_local)),
                        right: Operand::Copy(Place::Local(len_local)),
                    },
                );
                self.builder
                    .branch(Operand::Copy(Place::Local(cond_local)), bb_body, bb_exit);

                // 6. Body: let <binding> = __coll[__idx]; <body_expr>
                self.builder.set_current_block(bb_body);

                // Bind the loop variable: declare a local and assign __coll[__idx] to it
                let elem_local = self.builder.declare_local(None, elem_ty, None, false);
                self.builder.assign(
                    Place::local(elem_local),
                    Rvalue::Use(Operand::Copy(Place::Index {
                        base: Box::new(Place::local(coll_local)),
                        index: idx_local,
                        kind: IndexKind::Array,
                    })),
                );
                // Bind the pattern to the element local. FreshCell before each
                // assignment gives closures a distinct cell per iteration.
                self.bind_pattern_with_fresh_cells(elem_local, binding);
                let names: Vec<Name> = self.body.patterns[binding]
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                for name in names {
                    if let Some(&local) = self.locals.get(&name)
                        && let Some(binding_id) =
                            self.binding_id_for_statement_name(stmt_id, binding, &name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                }

                // Lower the body expression (result discarded)
                let body_temp = self.builder.temp(Ty::Void {
                    attr: TyAttr::default(),
                });
                self.lower_expr(body, Place::local(body_temp));

                if !self.builder.is_current_terminated() {
                    self.emit_unwatch_to_depth(watched_depth);
                    self.builder.goto(bb_after);
                }
                self.restore_locals_after_scope(saved_locals, watched_depth);

                // 7. After: __idx += 1
                self.builder.set_current_block(bb_after);
                self.builder.assign(
                    Place::local(idx_local),
                    Rvalue::BinaryOp {
                        op: BinOp::Add,
                        left: Operand::Copy(Place::Local(idx_local)),
                        right: Operand::Constant(Constant::Int(1)),
                    },
                );
                self.builder.goto(bb_header);

                // 8. Exit block
                self.loop_context = prev_loop;
                self.builder.set_current_block(bb_exit);
            }

            AstStmt::Return(expr) => {
                let ret = Local(0); // _0 is always the return place
                if let Some(e) = expr {
                    self.lower_expr(e, Place::local(ret));
                }
                // Unwatch all watched locals in this function (the stack is
                // swapped at lambda boundaries, so depth=0 covers exactly the
                // current function's watches).
                self.emit_unwatch_to_depth(0);
                self.builder.goto(self.exit_block);
                // Create a dead successor block for the builder cursor
                // (subsequent statements in the same block-list are dead code)
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
                // Dead block is unterminated — subsequent stmts are lowered as
                // dead code (matching AstStmt::Throw behavior at lower.rs:1653-1658).
                // Phase 1 eliminates unreachable blocks.
            }

            AstStmt::Throw { value } => {
                let val_op = self.lower_to_operand(value);
                // Unwatch all watched locals in this function before throwing,
                // matching the Return path. Without this, a
                // `watch let conn = …` followed by a `throw` leaks the watcher.
                self.emit_unwatch_to_depth(0);
                self.builder.throw(val_op);
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Break => {
                if let Some(ref loop_ctx) = self.loop_context {
                    let target = loop_ctx.break_target;
                    let depth = loop_ctx.watched_locals_depth;
                    self.emit_unwatch_to_depth(depth);
                    self.builder.goto(target);
                }
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Continue => {
                if let Some(ref loop_ctx) = self.loop_context {
                    let target = loop_ctx.continue_target;
                    let depth = loop_ctx.watched_locals_depth;
                    self.emit_unwatch_to_depth(depth);
                    self.builder.goto(target);
                }
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Assign { target, value } => {
                let target_expr = &self.body.exprs[target];
                if let AstExpr::OptionalChain { expr: inner } = target_expr {
                    let inner = *inner;
                    self.lower_assign_optional_chain(inner, value);
                } else {
                    let place = self.lower_lvalue(target);
                    self.lower_expr(value, place);
                }
            }

            AstStmt::AssignOp { target, op, value } => {
                let target_expr = &self.body.exprs[target];
                if let AstExpr::OptionalChain { expr: inner } = target_expr {
                    let inner = *inner;
                    self.lower_assign_op_optional_chain(inner, op, value);
                } else {
                    let place = self.lower_lvalue(target);
                    let current = Operand::Copy(place.clone());
                    let rhs = self.lower_to_operand(value);
                    let mir_op = Self::convert_assign_op(op);
                    self.builder.assign(
                        place,
                        Rvalue::BinaryOp {
                            op: mir_op,
                            left: current,
                            right: rhs,
                        },
                    );
                }
            }

            AstStmt::Missing => {
                let callee = Operand::Constant(Constant::Function(ItemRef::Free {
                    package: Name::new("baml"),
                    namespace: vec![Name::new("sys")],
                    name: Name::new("panic"),
                }));
                let msg = Operand::Constant(Constant::String("missing statement".to_string()));
                let temp = self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                });
                let unreachable_block = self.builder.create_block();
                self.builder.call(
                    callee,
                    vec![msg],
                    Place::local(temp),
                    unreachable_block,
                    None,
                );
                self.builder.set_current_block(unreachable_block);
                self.builder.unreachable();
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::HeaderComment { name, level } => {
                self.builder
                    .push_statement(StatementKind::NotifyBlock { name, level }, None);
            }
        }

        self.builder.current_source_span = prev_span;
    }

    fn convert_assign_op(op: AstAssignOp) -> BinOp {
        match op {
            AstAssignOp::Add => BinOp::Add,
            AstAssignOp::Sub => BinOp::Sub,
            AstAssignOp::Mul => BinOp::Mul,
            AstAssignOp::Div => BinOp::Div,
            AstAssignOp::Mod => BinOp::Mod,
            AstAssignOp::BitAnd => BinOp::BitAnd,
            AstAssignOp::BitOr => BinOp::BitOr,
            AstAssignOp::BitXor => BinOp::BitXor,
            AstAssignOp::Shl => BinOp::Shl,
            AstAssignOp::Shr => BinOp::Shr,
        }
    }

    fn lower_lvalue(&mut self, expr_id: AstExprId) -> Place {
        let expr = self.body.exprs[expr_id].clone();
        match &expr {
            AstExpr::Path(segments) if segments.len() == 1 => {
                if let Some(&local) = self.locals.get(&segments[0]) {
                    Place::Local(local)
                } else if let Some(cap_idx) = self.capture_index_for_name_at(expr_id, &segments[0])
                {
                    // Assignment to a captured variable in a closure body.
                    Place::Capture(cap_idx)
                } else {
                    let temp = self.builder.temp(Ty::Null {
                        attr: TyAttr::default(),
                    });
                    Place::Local(temp)
                }
            }
            AstExpr::Path(segments) if segments.len() >= 2 => {
                // Multi-segment path lvalue: `a.b` or `a.b.c`.
                // Chain field projections from the root local or capture.
                let (mut current_place, mut current_ty) = if let Some(&l) =
                    self.locals.get(&segments[0])
                {
                    let ty = self
                        .path_root_ty(expr_id)
                        .unwrap_or_else(|| self.builder.local_ty(l));
                    (Place::Local(l), ty)
                } else if let Some(cap_idx) = self.capture_index_for_name_at(expr_id, &segments[0])
                {
                    let ty = self
                        .path_root_ty(expr_id)
                        .unwrap_or_else(|| Ty::BuiltinUnknown {
                            attr: TyAttr::default(),
                        });
                    (Place::Capture(cap_idx), ty)
                } else {
                    let tmp = self.builder.temp(Ty::Null {
                        attr: TyAttr::default(),
                    });
                    (
                        Place::Local(tmp),
                        Ty::Null {
                            attr: TyAttr::default(),
                        },
                    )
                };

                for seg in &segments[1..] {
                    if let Ty::Class(ref tn, ref class_type_args, _) = current_ty.clone() {
                        if let Some(fields) = self.class_fields.get(tn) {
                            if let Some(&idx) = fields.get(seg.as_str()) {
                                let next_ty = self.class_field_ty(tn, seg, class_type_args);
                                current_place = Place::Field {
                                    base: Box::new(current_place),
                                    field: idx,
                                };
                                current_ty = next_ty;
                                continue;
                            }
                        }
                    }
                    // Dynamic map fallback for non-class base or unknown field
                    let key_local = self.builder.temp(Ty::String {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(
                        Place::local(key_local),
                        Rvalue::Use(Operand::Constant(Constant::String(seg.to_string()))),
                    );
                    current_place = Place::Index {
                        base: Box::new(current_place),
                        index: key_local,
                        kind: IndexKind::Map,
                    };
                    break;
                }
                current_place
            }
            AstExpr::MemberAccess { base, member } => {
                let base_id = *base;
                let member_name = member.clone();
                let base_place = self.lower_lvalue(base_id);
                let base_ty = self.expr_ty(base_id);
                if let Ty::Class(ref tn, _, _) = base_ty {
                    if let Some(fields) = self.class_fields.get(tn) {
                        if let Some(&idx) = fields.get(member_name.as_str()) {
                            return Place::Field {
                                base: Box::new(base_place),
                                field: idx,
                            };
                        }
                    }
                    self.emit_panic_call(
                        &format!(
                            "internal compiler error: MIR failed to resolve member access \
                             .{} against class definition '{}' (module_path: {:?}). \
                             This class should be in class_fields but isn't.",
                            member_name, tn.name, tn.module_path,
                        ),
                        base_id,
                    );
                    // Dead code after panic — return a dummy place
                    let dead = self.builder.temp(Ty::Null {
                        attr: TyAttr::default(),
                    });
                    return Place::Local(dead);
                }
                // Dynamic map access — only valid for map types, unknown, etc.
                let key_local = self.builder.temp(Ty::String {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(key_local),
                    Rvalue::Use(Operand::Constant(Constant::String(member_name.to_string()))),
                );
                Place::Index {
                    base: Box::new(base_place),
                    index: key_local,
                    kind: IndexKind::Map,
                }
            }
            AstExpr::Index { base, index } => {
                let base_id = *base;
                let index_id = *index;
                let base_place = self.lower_lvalue(base_id);
                let index_op = self.lower_to_operand(index_id);
                let base_ty = self.expr_ty(base_id);
                let index_ty = self.expr_ty(index_id);
                let index_local = self.operand_to_local(index_op, index_ty);
                let unwrapped_ty = match &base_ty {
                    Ty::Optional(inner, _) => inner.as_ref(),
                    _ => &base_ty,
                };
                let kind = if matches!(unwrapped_ty, Ty::List(..) | Ty::Uint8Array { .. }) {
                    IndexKind::Array
                } else {
                    IndexKind::Map
                };
                Place::Index {
                    base: Box::new(base_place),
                    index: index_local,
                    kind,
                }
            }
            AstExpr::OptionalMemberAccess { base, member } => {
                let base_id = *base;
                let member_name = member.clone();

                // Evaluate base once into a temp local
                let base_op = self.lower_to_operand(base_id);
                let base_ty = self.expr_ty(base_id);
                let base_local = self.operand_to_local(base_op, base_ty.clone());

                // Null check using the operand
                let is_null = Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::Local(base_local)),
                    right: Operand::Constant(Constant::Null),
                };
                let test_local = self.builder.temp(Ty::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(Place::local(test_local), is_null);

                let bb_continue = self.builder.create_block();
                let bb_null = *self
                    .chain_null_exits
                    .last()
                    .expect("OptionalMemberAccess in lvalue must be inside OptionalChain");
                self.builder.branch(
                    Operand::Copy(Place::Local(test_local)),
                    bb_null,
                    bb_continue,
                );

                self.builder.set_current_block(bb_continue);

                // Project member from the same temp local — no second evaluation
                let base_place = Place::Local(base_local);
                // Unwrap Optional — we've already null-checked, so use the inner type.
                let unwrapped_ty = match &base_ty {
                    Ty::Optional(inner, _) => inner.as_ref(),
                    _ => &base_ty,
                };
                if let Ty::Class(tn, _, _) = unwrapped_ty {
                    if let Some(fields) = self.class_fields.get(tn) {
                        if let Some(&idx) = fields.get(member_name.as_str()) {
                            return Place::Field {
                                base: Box::new(base_place),
                                field: idx,
                            };
                        }
                    }
                }
                // Dynamic map access
                let key_local = self.builder.temp(Ty::String {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(key_local),
                    Rvalue::Use(Operand::Constant(Constant::String(member_name.to_string()))),
                );
                Place::Index {
                    base: Box::new(base_place),
                    index: key_local,
                    kind: IndexKind::Map,
                }
            }
            AstExpr::OptionalIndex { base, index } => {
                let base_id = *base;
                let index_id = *index;

                // Evaluate base once into a temp local
                let base_op = self.lower_to_operand(base_id);
                let base_ty = self.expr_ty(base_id);
                let base_local = self.operand_to_local(base_op, base_ty.clone());

                // Null check
                let is_null = Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::Local(base_local)),
                    right: Operand::Constant(Constant::Null),
                };
                let test_local = self.builder.temp(Ty::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(Place::local(test_local), is_null);

                let bb_continue = self.builder.create_block();
                let bb_null = *self
                    .chain_null_exits
                    .last()
                    .expect("OptionalIndex in lvalue must be inside OptionalChain");
                self.builder.branch(
                    Operand::Copy(Place::Local(test_local)),
                    bb_null,
                    bb_continue,
                );

                self.builder.set_current_block(bb_continue);

                // Project index from the same temp local
                let index_op = self.lower_to_operand(index_id);
                let index_ty = self.expr_ty(index_id);
                let index_local = self.operand_to_local(index_op, index_ty);
                let unwrapped_ty = match &base_ty {
                    Ty::Optional(inner, _) => inner.as_ref(),
                    _ => &base_ty,
                };
                let kind = if matches!(unwrapped_ty, Ty::List(..)) {
                    IndexKind::Array
                } else {
                    IndexKind::Map
                };
                Place::Index {
                    base: Box::new(Place::Local(base_local)),
                    index: index_local,
                    kind,
                }
            }
            _ => {
                let ty = self.expr_ty(expr_id);
                let temp = self.builder.temp(ty);
                Place::Local(temp)
            }
        }
    }
}

// ─── Match lowering ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_match(
        &mut self,
        expr_id: AstExprId,
        scrutinee: AstExprId,
        arm_ids: &[baml_compiler2_ast::MatchArmId],
        dest: Place,
    ) {
        let is_exhaustive = self
            .exhaustive_matches
            .contains(&self.expr_metadata_key(expr_id));

        // If scrutinee is a simple variable reference, reuse the local directly
        // instead of copying into a temp (matches MIR1 behavior).
        let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
            let op = self.lower_to_operand(scrutinee);
            let ty = self.expr_ty(scrutinee);
            self.operand_to_local(op, ty)
        });

        let bb_join = self.builder.create_block();

        // Collect arms from arena
        let arms: Vec<baml_compiler2_ast::MatchArm> = arm_ids
            .iter()
            .map(|&id| self.body.match_arms[id].clone())
            .collect();

        // Try switch optimization: if all non-wildcard arms have compatible patterns
        // (int literal, enum variant, or type tag) with no guards, emit a Switch.
        let switch_arms: Vec<(AstPatId, AstExprId, Option<AstExprId>)> = arms
            .iter()
            .map(|arm| (arm.pattern, arm.body, arm.guard))
            .collect();
        if self.try_lower_as_switch(
            scrutinee_local,
            &switch_arms,
            dest.clone(),
            bb_join,
            SwitchOtherwise::Match { is_exhaustive },
            None,
        ) {
            self.builder.set_current_block(bb_join);
            return;
        }

        self.lower_match_chain(scrutinee_local, &arms, dest, bb_join, is_exhaustive);

        self.builder.set_current_block(bb_join);
    }

    /// Attempt to lower a match or catch as a Switch terminator.
    /// Returns true if successful, false if the arms aren't switch-eligible.
    ///
    /// Unified entry point for both match and catch switch dispatch.
    /// - `arms`: `(pattern, body_expr, optional_guard)` tuples
    /// - `otherwise`: controls what happens for unmatched values
    /// - `pre_created_blocks`: if `Some`, use these pre-created body blocks instead
    ///   of creating new ones (used by catch, which pre-creates blocks)
    fn try_lower_as_switch(
        &mut self,
        scrutinee: Local,
        arms: &[(AstPatId, AstExprId, Option<AstExprId>)],
        dest: Place,
        join: BlockId,
        otherwise: SwitchOtherwise,
        pre_created_blocks: Option<&[Option<BlockId>]>,
    ) -> bool {
        use std::collections::HashSet;

        if arms.is_empty() {
            return false;
        }

        let is_exhaustive = matches!(
            &otherwise,
            SwitchOtherwise::Match {
                is_exhaustive: true
            }
        );

        // Classify arms: collect (i64_value, arm_index) for int literal or enum variant
        // patterns, and check for a trailing wildcard/binding.
        let mut switch_kind: Option<SwitchKind> = None;
        let mut int_arms: Vec<(i64, usize)> = Vec::new();
        let mut otherwise_idx: Option<usize> = None;
        // Deduplicate discriminant values so union patterns don't produce duplicate switch arms.
        let mut seen_values: HashSet<i64> = HashSet::new();

        for (i, &(pattern, _body, guard)) in arms.iter().enumerate() {
            // Guards disqualify switch optimization
            if guard.is_some() {
                return false;
            }
            // OLD `pat.narrow.is_some()` branch: Chain encodes the narrow
            // as a `Type` link, so recover it and treat as a TypeTag arm.
            if self.pattern_narrow_type(pattern).is_some() {
                match &switch_kind {
                    None => switch_kind = Some(SwitchKind::TypeTag),
                    Some(SwitchKind::TypeTag) => {}
                    Some(_) => return false,
                }
                match self.classify_pattern_type_tag(pattern) {
                    Some(tags) => {
                        for tag in tags {
                            if seen_values.insert(tag) {
                                int_arms.push((tag, i));
                            }
                        }
                    }
                    None => return false,
                }
                continue;
            }

            // Helpers that classify a pattern (the arm pattern itself, or a
            // sub-pattern of an `Or`) into a switch kind. Mutate `switch_kind`
            // and `int_arms`. Return `false` if the pattern disqualifies.
            let pat = &self.body.patterns[pattern];
            let classify_atom = |this: &Self,
                                 atom_id: AstPatId,
                                 atom: &AstPattern,
                                 switch_kind: &mut Option<SwitchKind>,
                                 int_arms: &mut Vec<(i64, usize)>,
                                 seen_values: &mut HashSet<i64>|
             -> bool {
                match atom {
                    // OLD `Literal(Int(val))`: integer switch
                    AstPattern::Type(AstTypeExpr::Literal {
                        value: AstLiteral::Int(val),
                        ..
                    }) => {
                        match switch_kind.as_ref() {
                            None => *switch_kind = Some(SwitchKind::Integer),
                            Some(SwitchKind::Integer) => {}
                            Some(_) => return false,
                        }
                        if seen_values.insert(*val) {
                            int_arms.push((*val, i));
                        }
                        true
                    }
                    // OLD `EnumVariant { ... }`: integer switch with discriminant.
                    // The new repr puts enum variants inside `Pattern::Type`;
                    // detect via TIR.
                    AstPattern::Type(AstTypeExpr::Path { .. })
                        if matches!(
                            this.pat_types.get(&this.pat_metadata_key(atom_id)),
                            Some(Tir2Ty::EnumVariant(_, _, _))
                        ) =>
                    {
                        let Some(Tir2Ty::EnumVariant(qtn, variant, _)) =
                            this.pat_types.get(&this.pat_metadata_key(atom_id))
                        else {
                            unreachable!("guarded by matches! above");
                        };
                        let short_name = qtn.name().clone();
                        let variant = variant.clone();
                        match switch_kind.as_ref() {
                            None => {
                                *switch_kind =
                                    Some(SwitchKind::EnumDiscriminant(short_name.clone()));
                            }
                            Some(SwitchKind::EnumDiscriminant(n)) if *n == short_name => {}
                            _ => return false,
                        }
                        let idx = this
                            .enum_variants
                            .get(&short_name)
                            .and_then(|m| m.get(variant.as_str()))
                            .copied();
                        let Some(idx) = idx else { return false };
                        let disc = i64::try_from(idx).expect("discriminant overflow");
                        if seen_values.insert(disc) {
                            int_arms.push((disc, i));
                        }
                        true
                    }
                    // OLD `Type(_)` / `Bind { .. }` (with TIR type): TypeTag.
                    AstPattern::Type(_) | AstPattern::Bind { .. } => {
                        match switch_kind.as_ref() {
                            None => *switch_kind = Some(SwitchKind::TypeTag),
                            Some(SwitchKind::TypeTag) => {}
                            Some(_) => return false,
                        }
                        match this.classify_pattern_type_tag(atom_id) {
                            Some(tags) => {
                                for tag in tags {
                                    if seen_values.insert(tag) {
                                        int_arms.push((tag, i));
                                    }
                                }
                            }
                            None => return false,
                        }
                        true
                    }
                    _ => false,
                }
            };

            match pat {
                AstPattern::Or(sub_pats) => {
                    for sub_pat_id in sub_pats {
                        let sub_pat = &self.body.patterns[*sub_pat_id];
                        if !classify_atom(
                            self,
                            *sub_pat_id,
                            sub_pat,
                            &mut switch_kind,
                            &mut int_arms,
                            &mut seen_values,
                        ) {
                            return false;
                        }
                    }
                }
                AstPattern::Wildcard => {
                    if i != arms.len() - 1 {
                        return false;
                    }
                    otherwise_idx = Some(i);
                }
                // Plain `let x` without a narrow always acts as the
                // catch-all arm, enabling jump-table dispatch. (Narrowed
                // bindings — e.g. `let n: int` — are encoded as `Chain` and
                // were handled by the `pattern_narrow_type` branch above.)
                AstPattern::Bind { .. } => {
                    if i != arms.len() - 1 {
                        return false;
                    }
                    otherwise_idx = Some(i);
                }
                _ => {
                    if !classify_atom(
                        self,
                        pattern,
                        pat,
                        &mut switch_kind,
                        &mut int_arms,
                        &mut seen_values,
                    ) {
                        return false;
                    }
                }
            }
        }

        // Need at least one int arm to justify a switch.
        if int_arms.is_empty() {
            return false;
        }

        // TypeTag switches only pay off at 4+ arms (JumpTable). For fewer arms
        // the sequential `is_type` chain is more compact because the if-else
        // chain adds copy/pop stack management overhead per arm.
        if matches!(switch_kind, Some(SwitchKind::TypeTag)) && int_arms.len() < 4 {
            return false;
        }

        // Exhaustiveness: for **match** TypeTag switches without a wildcard arm,
        // all typed arms together cover the union — the otherwise block is dead.
        // TIR's `required_match_cases` returns None for class types, so class
        // unions are never marked exhaustive by TIR even when all arms are
        // covered. For match + TypeTag, if there's no wildcard, treat as
        // exhaustive so the last arm skips its comparison and the otherwise
        // block becomes Unreachable.
        //
        // For **catch** expressions, we never mark the switch as exhaustive
        // even when all declared thrown types are covered, because panics can
        // always occur at runtime and must be rethrown via the otherwise block.
        let is_match = matches!(&otherwise, SwitchOtherwise::Match { .. });
        let is_switch_exhaustive = otherwise_idx.is_none()
            && (is_exhaustive || (is_match && matches!(switch_kind, Some(SwitchKind::TypeTag))));

        // Save the entry block — this is where the switch terminator goes
        let bb_entry = self.builder.current_block();

        // Emit discriminant/type-tag extraction before building arm blocks.
        // We must do this before create_block() calls so the assignment goes into bb_entry.
        let switch_operand = match &switch_kind {
            Some(SwitchKind::EnumDiscriminant(_)) => {
                let disc = self.builder.temp(Ty::Int {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(disc),
                    Rvalue::Discriminant(Place::local(scrutinee)),
                );
                Operand::Copy(Place::Local(disc))
            }
            Some(SwitchKind::TypeTag) => {
                let tag_local = self.builder.temp(Ty::Int {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(tag_local),
                    Rvalue::TypeTag(Place::local(scrutinee)),
                );
                Operand::Copy(Place::Local(tag_local))
            }
            _ => Operand::Copy(Place::Local(scrutinee)),
        };

        // Build body blocks for each arm. Union sub-patterns sharing the same
        // arm_idx reuse a single block (e.g. Active | Pending → same bb).
        let bb_otherwise = self.builder.create_block();
        let mut switch_arms: Vec<(i64, BlockId)> = Vec::new();
        let mut arm_blocks: std::collections::HashMap<usize, BlockId> =
            std::collections::HashMap::new();

        for &(val, arm_idx) in &int_arms {
            if let Some(&existing_bb) = arm_blocks.get(&arm_idx) {
                // Union sub-pattern: reuse the same body block
                switch_arms.push((val, existing_bb));
            } else {
                // Use pre-created block if available, otherwise create a new one
                let bb_body = if let Some(blocks) = pre_created_blocks {
                    blocks[arm_idx].expect("pre-created block missing for arm")
                } else {
                    self.builder.create_block()
                };
                switch_arms.push((val, bb_body));
                arm_blocks.insert(arm_idx, bb_body);

                self.builder.set_current_block(bb_body);
                let (pattern, body, _) = arms[arm_idx];
                let saved_locals = self.locals.clone();
                let watched_depth = self.watched_locals_stack.len();
                self.bind_pattern(scrutinee, pattern);
                self.lower_expr(body, dest.clone());
                if !self.builder.is_current_terminated() {
                    // A `watch let` declared inside an arm body must be torn
                    // down on fallthrough. Without this the watcher leaks past
                    // the arm. Mirrors `lower_match_chain`.
                    self.emit_unwatch_to_depth(watched_depth);
                    self.builder.goto(join);
                }
                // Restore both the name→local map AND truncate the watched
                // stack back to the arm-entry depth (mirrors `lower_scoped_block`).
                self.restore_locals_after_scope(saved_locals, watched_depth);
            }
        }

        // Build arm_names: symbolic labels for the switch arms (debug metadata).
        let arm_names: Vec<(i64, String)> = match &switch_kind {
            Some(SwitchKind::EnumDiscriminant(enum_name)) => {
                if let Some(variants) = self.enum_variants.get(enum_name) {
                    // Build reverse map: variant_idx -> variant_name
                    let reverse: std::collections::HashMap<i64, &str> = variants
                        .iter()
                        .map(|(name, idx)| {
                            (
                                i64::try_from(*idx).expect("discriminant overflow"),
                                name.as_str(),
                            )
                        })
                        .collect();
                    int_arms
                        .iter()
                        .filter_map(|(val, _)| {
                            reverse
                                .get(val)
                                .map(|vname| (*val, format!("{enum_name}.{vname}")))
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
            Some(SwitchKind::TypeTag) => {
                // Reverse map: tag value → human-readable type name.
                let reverse_class: std::collections::HashMap<i64, &str> = self
                    .class_type_tags
                    .iter()
                    .map(|(tn, tag)| (*tag, tn.name.as_str()))
                    .collect();
                int_arms
                    .iter()
                    .map(|(v, _)| {
                        let name = reverse_class
                            .get(v)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| format_type_tag_name(*v));
                        (*v, name)
                    })
                    .collect()
            }
            _ => int_arms.iter().map(|(v, _)| (*v, v.to_string())).collect(),
        };

        // Lower the otherwise block
        self.builder.set_current_block(bb_otherwise);
        if let Some(idx) = otherwise_idx {
            // Wildcard arm present
            if let SwitchOtherwise::Catch {
                error_local,
                needs_throw_if_panic: true,
            } = &otherwise
            {
                let bb_wildcard_body = self.builder.create_block();
                self.builder
                    .throw_if_panic(Operand::Copy(Place::Local(*error_local)), bb_wildcard_body);
                self.builder.set_current_block(bb_wildcard_body);
            }
            let (pattern, body, _) = arms[idx];
            let saved_locals = self.locals.clone();
            let watched_depth = self.watched_locals_stack.len();
            self.bind_pattern(scrutinee, pattern);
            self.lower_expr(body, dest);
            if !self.builder.is_current_terminated() {
                // A `watch let` declared inside the wildcard body must be
                // torn down on fallthrough; mirrors the int-arm path above.
                self.emit_unwatch_to_depth(watched_depth);
                self.builder.goto(join);
            }
            // Restore name→local map AND truncate the watched stack back to
            // the arm-entry depth (mirrors `lower_scoped_block`).
            self.restore_locals_after_scope(saved_locals, watched_depth);
        } else {
            // No wildcard — decide what the otherwise block does.
            // Use `is_switch_exhaustive` (which may be inferred for TypeTag)
            // rather than the caller's original `is_exhaustive`, so the
            // otherwise block stays consistent with the switch terminator flag.
            if is_switch_exhaustive {
                match &otherwise {
                    SwitchOtherwise::Match { .. } => {
                        self.builder.unreachable();
                    }
                    SwitchOtherwise::Catch { error_local, .. } => {
                        // Even if exhaustive, catch otherwise should rethrow
                        // (the error might not match any arm at runtime).
                        self.builder
                            .throw(Operand::Copy(Place::Local(*error_local)));
                    }
                }
            } else {
                match &otherwise {
                    SwitchOtherwise::Catch { error_local, .. } => {
                        self.builder
                            .throw(Operand::Copy(Place::Local(*error_local)));
                    }
                    SwitchOtherwise::Match { .. } => {
                        self.builder.goto(join);
                    }
                }
            }
        }

        // For catch with pre-created blocks: redirect wildcard arm's pre-created block
        // to bb_otherwise, since the wildcard body was lowered there.
        if let Some(blocks) = pre_created_blocks {
            for (i, block_opt) in blocks.iter().enumerate() {
                if let Some(block) = block_opt {
                    if otherwise_idx == Some(i) {
                        // Wildcard arm's pre-created block → redirect to otherwise
                        self.builder.set_current_block(*block);
                        self.builder.goto(bb_otherwise);
                    } else if !arm_blocks.contains_key(&i) {
                        // Unreachable pre-created block (e.g. duplicate tag) → terminate it
                        self.builder.set_current_block(*block);
                        self.builder.goto(bb_otherwise);
                    }
                }
            }
        }

        // Emit the switch terminator in the entry block
        self.builder.set_current_block(bb_entry);
        self.builder.switch(
            switch_operand,
            switch_arms,
            bb_otherwise,
            is_switch_exhaustive,
            arm_names,
        );

        true
    }

    fn lower_match_chain(
        &mut self,
        scrutinee: Local,
        arms: &[baml_compiler2_ast::MatchArm],
        dest: Place,
        join: BlockId,
        exhaustive: bool,
    ) {
        if arms.is_empty() {
            // No more arms to test. Either a preceding wildcard/binding arm
            // consumed all inputs (making this dead code), or the match is
            // non-exhaustive and a runtime value could reach here. In both
            // cases, jump to the join block so execution continues.
            self.builder.goto(join);
            return;
        }

        let arm = &arms[0];
        let rest = &arms[1..];

        // Exhaustive last arm: skip the pattern test — it must match. Do not
        // take this shortcut for Or-patterns because bindings must come from
        // the specific alternative that matched.
        if exhaustive
            && rest.is_empty()
            && arm.guard.is_none()
            && !matches!(self.body.patterns[arm.pattern], AstPattern::Or(_))
        {
            let saved_locals = self.locals.clone();
            let watched_depth = self.watched_locals_stack.len();
            self.bind_pattern(scrutinee, arm.pattern);
            self.lower_expr(arm.body, dest);
            if !self.builder.is_current_terminated() {
                // A `watch let` declared inside an arm body must be torn
                // down on fallthrough. Without this the watcher leaks past
                // the arm.
                self.emit_unwatch_to_depth(watched_depth);
                self.builder.goto(join);
            }
            // Restore both the name→local map AND truncate the watched stack
            // back to the arm-entry depth (mirrors `lower_scoped_block`).
            self.restore_locals_after_scope(saved_locals, watched_depth);
            return;
        }

        if let AstPattern::Or(parts) = self.body.patterns[arm.pattern].clone() {
            let bb_next = self.builder.create_block();
            for (idx, part) in parts.iter().copied().enumerate() {
                let bb_body = self.builder.create_block();
                let bb_alt_next = if idx + 1 == parts.len() {
                    bb_next
                } else {
                    self.builder.create_block()
                };

                self.lower_pattern_test(scrutinee, part, bb_body, bb_alt_next);

                self.builder.set_current_block(bb_body);
                let saved_locals = self.locals.clone();
                let watched_depth = self.watched_locals_stack.len();
                self.bind_pattern(scrutinee, part);
                if let Some(guard) = arm.guard {
                    let guard_op = self.lower_to_operand(guard);
                    let bb_guarded = self.builder.create_block();
                    self.builder.branch(guard_op, bb_guarded, bb_next);
                    self.builder.set_current_block(bb_guarded);
                }
                self.lower_expr(arm.body, dest.clone());
                if !self.builder.is_current_terminated() {
                    self.emit_unwatch_to_depth(watched_depth);
                    self.builder.goto(join);
                }
                self.restore_locals_after_scope(saved_locals, watched_depth);

                if idx + 1 < parts.len() {
                    self.builder.set_current_block(bb_alt_next);
                }
            }

            self.builder.set_current_block(bb_next);
            self.lower_match_chain(scrutinee, rest, dest, join, exhaustive);
            return;
        }

        let bb_body = self.builder.create_block();
        let bb_next = self.builder.create_block();

        self.lower_pattern_test(scrutinee, arm.pattern, bb_body, bb_next);

        self.builder.set_current_block(bb_body);
        let saved_locals = self.locals.clone();
        let watched_depth = self.watched_locals_stack.len();
        self.bind_pattern(scrutinee, arm.pattern);
        if let Some(guard) = arm.guard {
            let guard_op = self.lower_to_operand(guard);
            let bb_guarded = self.builder.create_block();
            self.builder.branch(guard_op, bb_guarded, bb_next);
            self.builder.set_current_block(bb_guarded);
        }
        self.lower_expr(arm.body, dest.clone());
        if !self.builder.is_current_terminated() {
            // See exhaustive arm comment above.
            self.emit_unwatch_to_depth(watched_depth);
            self.builder.goto(join);
        }
        self.restore_locals_after_scope(saved_locals, watched_depth);

        self.builder.set_current_block(bb_next);
        self.lower_match_chain(scrutinee, rest, dest, join, exhaustive);
    }

    /// Emit an `IsType` check that handles union types by expanding them
    /// into a chain: try each member, branch to `success` if any matches.
    fn emit_is_type_branch(
        &mut self,
        scrutinee: Local,
        ty: Ty,
        success: BlockId,
        failure: BlockId,
    ) {
        if let Ty::Union(members, _) = ty {
            // For union A | B | C: check A → success, else check B → success,
            // else check C → success, else failure.
            let mut remaining = members.into_iter().peekable();
            while let Some(member) = remaining.next() {
                if remaining.peek().is_none() {
                    // Last member: branch directly to success/failure.
                    self.emit_is_type_branch(scrutinee, member, success, failure);
                } else {
                    // Not last: if this member matches, jump to success;
                    // otherwise try the next member.
                    let next_check = self.builder.create_block();
                    self.emit_is_type_branch(scrutinee, member, success, next_check);
                    self.builder.set_current_block(next_check);
                }
            }
        } else {
            // Convert Ty → TyTemplate so the emitter can handle generic class
            // checks (Ty::Class with args containing TypeVars map to
            // TyTemplate::Class with TypeArgRef leaves).  For non-generic types
            // the template is TyTemplate::Concrete(ty) — the emitter falls back
            // to the same fast path as before.
            let ty_template = ty_to_template_from_resolved_ty(&ty);
            let test = Rvalue::IsType {
                operand: Operand::Copy(Place::Local(scrutinee)),
                ty_template,
            };
            let test_local = self.builder.temp(Ty::Bool {
                attr: TyAttr::default(),
            });
            self.builder.assign(Place::local(test_local), test);
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), success, failure);
        }
    }

    fn emit_is_tir_type_branch(
        &mut self,
        scrutinee: Local,
        ty: &Tir2Ty,
        success: BlockId,
        failure: BlockId,
    ) {
        let mut visited = HashSet::new();
        self.emit_is_tir_type_branch_inner(scrutinee, ty, success, failure, &mut visited);
    }

    fn emit_is_tir_type_branch_inner(
        &mut self,
        scrutinee: Local,
        ty: &Tir2Ty,
        success: BlockId,
        failure: BlockId,
        visited: &mut HashSet<String>,
    ) {
        match ty {
            Tir2Ty::Union(members, _) => {
                let mut remaining = members.iter().peekable();
                while let Some(member) = remaining.next() {
                    if remaining.peek().is_none() {
                        self.emit_is_tir_type_branch_inner(
                            scrutinee, member, success, failure, visited,
                        );
                    } else {
                        let next_check = self.builder.create_block();
                        self.emit_is_tir_type_branch_inner(
                            scrutinee, member, success, next_check, visited,
                        );
                        self.builder.set_current_block(next_check);
                    }
                }
            }
            Tir2Ty::Optional(inner, _) => {
                let test = Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::Local(scrutinee)),
                    right: Operand::Constant(Constant::Null),
                };
                let test_local = self.builder.temp(Ty::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(Place::local(test_local), test);

                let bb_inner = self.builder.create_block();
                self.builder
                    .branch(Operand::Copy(Place::Local(test_local)), success, bb_inner);
                self.builder.set_current_block(bb_inner);
                self.emit_is_tir_type_branch_inner(scrutinee, inner, success, failure, visited);
            }
            Tir2Ty::Class(qtn, type_args, _) if !type_args.is_empty() => {
                let erased = self.resolved_aliases.convert(ty);
                let class_fields = self.lookup_tir_class_fields(qtn, type_args);
                if class_fields.is_empty() {
                    self.emit_is_type_branch(scrutinee, erased, success, failure);
                    return;
                }

                let class_success = self.builder.create_block();
                self.emit_is_type_branch(scrutinee, erased, class_success, failure);
                self.builder.set_current_block(class_success);

                let key = format!("{qtn:?}<{type_args:?}>");
                if !visited.insert(key.clone()) {
                    self.builder.goto(success);
                    return;
                }

                let class_tn = qtn_to_type_name(qtn);
                let fields: Vec<_> = class_fields.into_iter().collect();
                for (idx, (field_name, field_ty)) in fields.iter().enumerate() {
                    let next = if idx + 1 == fields.len() {
                        success
                    } else {
                        self.builder.create_block()
                    };

                    let Some(field_idx) = self
                        .class_fields
                        .get(&class_tn)
                        .and_then(|fields| fields.get(field_name.as_str()))
                        .copied()
                    else {
                        self.builder.goto(failure);
                        visited.remove(&key);
                        return;
                    };

                    let field_local = self.builder.temp(self.resolved_aliases.convert(field_ty));
                    self.builder.assign(
                        Place::local(field_local),
                        Rvalue::Use(Operand::Copy(Place::Field {
                            base: Box::new(Place::Local(scrutinee)),
                            field: field_idx,
                        })),
                    );
                    self.emit_is_tir_type_branch_inner(
                        field_local,
                        field_ty,
                        next,
                        failure,
                        visited,
                    );
                    if idx + 1 < fields.len() {
                        self.builder.set_current_block(next);
                    }
                }

                visited.remove(&key);
            }
            // Singleton-valued types pin a specific runtime value, so emit
            // equality checks rather than type-tag tests. `is_type` on a
            // literal type like `Ty::Literal("specific")` checks the value's
            // *type* (string) rather than its content — which is too permissive
            // and would let `let x: "specific" => …` fire on any string.
            Tir2Ty::Literal(lit, _, _) => {
                let constant = Self::lower_literal(lit);
                self.emit_value_eq_branch(scrutinee, Operand::Constant(constant), success, failure);
            }
            Tir2Ty::Primitive(baml_compiler2_tir::ty::PrimitiveType::Null, _) => {
                self.emit_value_eq_branch(
                    scrutinee,
                    Operand::Constant(Constant::Null),
                    success,
                    failure,
                );
            }
            _ => {
                let resolved = self.resolved_aliases.convert(ty);
                self.emit_is_type_branch(scrutinee, resolved, success, failure);
            }
        }
    }

    /// Branch on `scrutinee == rhs` (value equality). Used for singleton-typed
    /// patterns where the type pins a specific value.
    fn emit_value_eq_branch(
        &mut self,
        scrutinee: Local,
        rhs: Operand,
        success: BlockId,
        failure: BlockId,
    ) {
        let test = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: Operand::Copy(Place::Local(scrutinee)),
            right: rhs,
        };
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), test);
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), success, failure);
    }

    fn lookup_tir_class_fields(
        &self,
        class_name: &QualifiedTypeName,
        class_type_args: &[Tir2Ty],
    ) -> IndexMap<Name, Tir2Ty> {
        let pkg_id = PackageId::new(self.db, class_name.package().clone());
        let pkg_items_for_class = package_items(self.db, pkg_id);
        let Some(Definition::Class(class_loc)) =
            pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())
        else {
            return IndexMap::new();
        };

        let file = class_loc.file(self.db);
        let ns_context = file_package(self.db, file).namespace_path;
        let item_tree = file_item_tree(self.db, file);
        let class_data = &item_tree[class_loc.id(self.db)];
        let bindings = baml_compiler2_tir::generics::bind_type_vars(
            &class_data.generic_params,
            class_type_args,
        );

        let mut result = IndexMap::new();
        for field in &class_data.fields {
            let mut diags = Vec::new();
            let field_ty = field
                .type_expr
                .as_ref()
                .map(|te| {
                    if bindings.is_empty() {
                        baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                            self.db,
                            &te.expr,
                            pkg_items_for_class,
                            &ns_context,
                            &class_data.generic_params,
                            &mut diags,
                        )
                    } else {
                        baml_compiler2_tir::generics::lower_type_expr_with_generics(
                            self.db,
                            &te.expr,
                            pkg_items_for_class,
                            &ns_context,
                            &bindings,
                            &mut diags,
                        )
                    }
                })
                .unwrap_or(Tir2Ty::Unknown {
                    attr: baml_compiler2_tir::ty::TyAttr::default(),
                });
            result.insert(field.name.clone(), field_ty);
        }
        result
    }

    /// Look up the integer type tag for a type. Returns `Some(tag)` for
    /// primitives (INT=0, STRING=1, etc.) and classes (`CLASS_BASE` + index),
    /// or `None` for types that don't have a tag (unions, generics, etc.).
    fn type_tag_for_ty(&self, ty: &Ty) -> Option<i64> {
        match ty {
            Ty::Int { .. } => Some(baml_type::typetag::INT),
            Ty::String { .. } => Some(baml_type::typetag::STRING),
            Ty::Bool { .. } => Some(baml_type::typetag::BOOL),
            Ty::Null { .. } => Some(baml_type::typetag::NULL),
            Ty::Float { .. } => Some(baml_type::typetag::FLOAT),
            Ty::Class(tn, _, _) => self.class_type_tags.get(tn).copied(),
            _ => None,
        }
    }

    fn pattern_contains_structural(&self, pat_id: AstPatId) -> bool {
        match &self.body.patterns[pat_id] {
            AstPattern::Class { .. } | AstPattern::Array { .. } => true,
            AstPattern::Or(parts) => parts.iter().any(|p| self.pattern_contains_structural(*p)),
            AstPattern::Wildcard | AstPattern::Bind { .. } | AstPattern::Type(_) => false,
        }
    }

    fn class_pattern_fields(&self, pat_id: AstPatId) -> Vec<baml_compiler2_ast::FieldPat> {
        match &self.body.patterns[pat_id] {
            AstPattern::Class { fields, .. } => fields.clone(),
            _ => Vec::new(),
        }
    }

    fn class_pattern_type_name(&self, pat_id: AstPatId) -> Option<TypeName> {
        let tir_ty = self.pat_types.get(&self.pat_metadata_key(pat_id))?;
        match convert_tir2_ty(tir_ty, &self.resolved_aliases) {
            Ty::Class(tn, _, _) => Some(tn),
            _ => None,
        }
    }

    fn class_pattern_field_ty(&self, pat_id: AstPatId, field: &Name) -> Option<Ty> {
        let tir_ty = self.pat_types.get(&self.pat_metadata_key(pat_id))?;
        let Tir2Ty::Class(qtn, type_args, _) = tir_ty else {
            return None;
        };
        let fields = self.lookup_tir_class_fields(qtn, type_args);
        fields
            .get(field)
            .map(|field_ty| self.resolved_aliases.convert(field_ty))
    }

    fn project_class_pattern_field(
        &mut self,
        scrutinee: Local,
        class_pat_id: AstPatId,
        field_pat_id: AstPatId,
        field: &Name,
    ) -> Option<Local> {
        let class_tn = self.class_pattern_type_name(class_pat_id)?;
        let field_idx = self
            .class_fields
            .get(&class_tn)?
            .get(field.as_str())
            .copied()?;
        let inferred_pat_ty = self.pat_ty(field_pat_id);
        let source_field_ty = self.class_pattern_field_ty(class_pat_id, field);
        let cached_field_ty = self
            .class_field_types
            .get(&class_tn)
            .and_then(|fields| fields.get(field.as_str()))
            .cloned();
        let field_ty = source_field_ty
            .or_else(|| cached_field_ty.filter(|ty| !Self::is_pattern_type_recovery(ty)))
            .unwrap_or(inferred_pat_ty);
        let field_local = self.builder.temp(field_ty);
        self.builder.assign(
            Place::local(field_local),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: Box::new(Place::Local(scrutinee)),
                field: field_idx,
            })),
        );
        Some(field_local)
    }

    fn const_int_local(&mut self, value: i64) -> Local {
        let local = self.builder.temp(Ty::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(local),
            Rvalue::Use(Operand::Constant(Constant::Int(value))),
        );
        local
    }

    fn const_usize_int_local(&mut self, value: usize) -> Local {
        self.const_int_local(i64::try_from(value).expect("array pattern length/index overflow"))
    }

    fn array_len_local(&mut self, scrutinee: Local) -> Local {
        let len_local = self.builder.temp(Ty::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(len_local),
            Rvalue::Len(Place::local(scrutinee)),
        );
        len_local
    }

    fn lower_array_pattern_length_test(
        &mut self,
        scrutinee: Local,
        has_rest: bool,
        fixed_len: usize,
        success: BlockId,
        failure: BlockId,
    ) {
        let len_local = self.array_len_local(scrutinee);
        let expected = self.const_usize_int_local(fixed_len);
        let test_local = self.builder.temp(Ty::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(test_local),
            Rvalue::BinaryOp {
                op: if has_rest { BinOp::Ge } else { BinOp::Eq },
                left: Operand::Copy(Place::local(len_local)),
                right: Operand::Copy(Place::local(expected)),
            },
        );
        self.builder
            .branch(Operand::Copy(Place::local(test_local)), success, failure);
    }

    fn project_array_pattern_element_from_start(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index: usize,
    ) -> Local {
        let index_local = self.const_usize_int_local(index);
        self.project_array_pattern_element(scrutinee, elem_pat, index_local)
    }

    fn project_array_pattern_element_from_end(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index_from_end: usize,
    ) -> Local {
        let len_local = self.array_len_local(scrutinee);
        let offset = self.const_usize_int_local(index_from_end);
        let index_local = self.builder.temp(Ty::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(index_local),
            Rvalue::BinaryOp {
                op: BinOp::Sub,
                left: Operand::Copy(Place::local(len_local)),
                right: Operand::Copy(Place::local(offset)),
            },
        );
        self.project_array_pattern_element(scrutinee, elem_pat, index_local)
    }

    fn project_array_pattern_element(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index_local: Local,
    ) -> Local {
        let elem_ty = self.pat_ty(elem_pat);
        let elem_local = self.builder.temp(elem_ty);
        self.builder.assign(
            Place::local(elem_local),
            Rvalue::Use(Operand::Copy(Place::Index {
                base: Box::new(Place::Local(scrutinee)),
                index: index_local,
                kind: IndexKind::Array,
            })),
        );
        elem_local
    }

    fn project_array_pattern_rest(
        &mut self,
        scrutinee: Local,
        rest_pat: AstPatId,
        prefix_len: usize,
        suffix_len: usize,
    ) -> Local {
        let rest_ty = self.pat_ty(rest_pat);
        let rest_local = self.builder.temp(rest_ty);
        let start = self.const_usize_int_local(prefix_len);
        let end = if suffix_len == 0 {
            self.array_len_local(scrutinee)
        } else {
            let len_local = self.array_len_local(scrutinee);
            let suffix = self.const_usize_int_local(suffix_len);
            let end = self.builder.temp(Ty::Int {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(end),
                Rvalue::BinaryOp {
                    op: BinOp::Sub,
                    left: Operand::Copy(Place::local(len_local)),
                    right: Operand::Copy(Place::local(suffix)),
                },
            );
            end
        };
        let target = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.builder.call(
            Operand::Constant(Constant::Function(ItemRef::Method {
                package: Name::new("baml"),
                namespace: Vec::new(),
                class: Name::new("Array"),
                name: Name::new("slice"),
            })),
            vec![
                Operand::Copy(Place::local(scrutinee)),
                Operand::Copy(Place::local(start)),
                Operand::Copy(Place::local(end)),
            ],
            Place::local(rest_local),
            target,
            unwind,
        );
        self.builder.set_current_block(target);
        rest_local
    }

    fn lower_pattern_test(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        success: BlockId,
        failure: BlockId,
    ) {
        let pat = self.body.patterns[pat_id].clone();

        // Bind sub-pattern: `let x: <pattern>` defers to the sub-
        // pattern's runtime test (recursively). The bind itself doesn't
        // emit a runtime check; the sub-pattern does.
        if let AstPattern::Bind {
            subpat: Some(sp), ..
        } = &pat
        {
            return self.lower_pattern_test(scrutinee, *sp, success, failure);
        }
        // Array `: T` ascription emits an `is_type` test before the
        // structural shape test below.
        if let AstPattern::Array {
            ascription: Some(ty_expr),
            ..
        } = &pat
        {
            let after_ascription = self.builder.create_block();
            if let Some(tir_ty) = self
                .pat_types
                .get(&self.pat_metadata_key(pat_id))
                .filter(|ty| !matches!(ty, Tir2Ty::Never { .. }))
                .cloned()
            {
                self.emit_is_tir_type_branch(scrutinee, &tir_ty, after_ascription, failure);
            } else {
                let annotation_ty = self.resolve_type_annotation(ty_expr);
                self.emit_is_type_branch(scrutinee, annotation_ty, after_ascription, failure);
            }
            self.builder.set_current_block(after_ascription);
            // Fall through to the array shape test below.
        }

        match &pat {
            AstPattern::Wildcard => {
                self.builder.goto(success);
            }
            AstPattern::Bind { .. } => {
                if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)).cloned() {
                    let resolved = self.resolved_aliases.convert(&tir_ty);
                    self.emit_is_type_branch(scrutinee, resolved, success, failure);
                } else {
                    self.builder.goto(success);
                }
            }
            // OLD's Pattern::Type covered structural shape tests; OLD's
            // Pattern::Literal / Pattern::Null / Pattern::EnumVariant were
            // separate variants. The new flat enum collapses all of those
            // into `Pattern::Type(TypeExpr)`, so we dispatch on the inner
            // TypeExpr to recover OLD's per-kind codegen.
            AstPattern::Type(ty_expr) => match ty_expr {
                AstTypeExpr::Literal { value: lit, .. } => {
                    let constant = Self::lower_literal(lit);
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(constant),
                    };
                    let test_local = self.builder.temp(Ty::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                AstTypeExpr::Null { .. } => {
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(Constant::Null),
                    };
                    let test_local = self.builder.temp(Ty::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                AstTypeExpr::Path { .. }
                    if matches!(
                        self.pat_types.get(&self.pat_metadata_key(pat_id)),
                        Some(Tir2Ty::EnumVariant(_, _, _))
                    ) =>
                {
                    let Some(Tir2Ty::EnumVariant(qtn, variant, _)) =
                        self.pat_types.get(&self.pat_metadata_key(pat_id))
                    else {
                        unreachable!("guarded by matches! above");
                    };
                    let enum_ref = ItemRef::EnumType {
                        package: qtn.package().clone(),
                        namespace: qtn.namespace().clone(),
                        name: qtn.name().clone(),
                    };
                    let variant = variant.clone();
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(Constant::EnumVariant { enum_ref, variant }),
                    };
                    let test_local = self.builder.temp(Ty::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                _ => {
                    let annotation_ty = self
                        .pat_types
                        .get(&self.pat_metadata_key(pat_id))
                        .map(|tir_ty| convert_tir2_ty(tir_ty, &self.resolved_aliases))
                        .unwrap_or_else(|| self.resolve_type_annotation(ty_expr));
                    self.emit_is_type_branch(scrutinee, annotation_ty, success, failure);
                }
            },
            AstPattern::Or(sub_pats) => {
                if sub_pats.is_empty() {
                    self.builder.goto(failure);
                    return;
                }
                let n = sub_pats.len();
                for (i, &sub_pat) in sub_pats.iter().enumerate() {
                    let next = if i + 1 < n {
                        self.builder.create_block()
                    } else {
                        failure
                    };
                    self.lower_pattern_test(scrutinee, sub_pat, success, next);
                    if i + 1 < n {
                        self.builder.set_current_block(next);
                    }
                }
            }
            AstPattern::Class { .. } => {
                let class_success = if self.class_pattern_fields(pat_id).is_empty() {
                    success
                } else {
                    self.builder.create_block()
                };

                if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)).cloned() {
                    self.emit_is_tir_type_branch(scrutinee, &tir_ty, class_success, failure);
                } else if class_success == success {
                    self.builder.goto(success);
                } else {
                    self.builder.goto(class_success);
                }

                if class_success != success {
                    self.builder.set_current_block(class_success);
                    let fields = self.class_pattern_fields(pat_id);
                    for (idx, field) in fields.iter().enumerate() {
                        let next = if idx + 1 == fields.len() {
                            success
                        } else {
                            self.builder.create_block()
                        };
                        if let Some(field_local) = self.project_class_pattern_field(
                            scrutinee,
                            pat_id,
                            field.pat,
                            &field.field,
                        ) {
                            self.lower_pattern_test(field_local, field.pat, next, failure);
                        } else {
                            self.builder.goto(failure);
                        }
                        if idx + 1 < fields.len() {
                            self.builder.set_current_block(next);
                        }
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                let array_success = self.builder.create_block();

                if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)).cloned() {
                    self.emit_is_tir_type_branch(scrutinee, &tir_ty, array_success, failure);
                } else {
                    self.builder.goto(array_success);
                }

                self.builder.set_current_block(array_success);
                let has_rest_test = rest.as_ref().and_then(|r| r.pat).is_some();
                let element_count = prefix.len() + suffix.len();
                let has_nested_tests = element_count > 0 || has_rest_test;
                let after_len = if has_nested_tests {
                    self.builder.create_block()
                } else {
                    success
                };
                self.lower_array_pattern_length_test(
                    scrutinee,
                    rest.is_some(),
                    prefix.len() + suffix.len(),
                    after_len,
                    failure,
                );
                if !has_nested_tests {
                    return;
                }

                self.builder.set_current_block(after_len);
                let rest_entry = has_rest_test.then(|| self.builder.create_block());
                let element_success = rest_entry.unwrap_or(success);
                if element_count == 0 {
                    self.builder.goto(element_success);
                }

                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let next = if idx + 1 == element_count {
                        element_success
                    } else {
                        self.builder.create_block()
                    };
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.lower_pattern_test(elem_local, elem_pat, next, failure);
                    if idx + 1 < element_count {
                        self.builder.set_current_block(next);
                    }
                }

                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_idx = prefix.len() + suffix_idx;
                    let next = if elem_idx + 1 == element_count {
                        element_success
                    } else {
                        self.builder.create_block()
                    };
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.lower_pattern_test(elem_local, elem_pat, next, failure);
                    if elem_idx + 1 < element_count {
                        self.builder.set_current_block(next);
                    }
                }

                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    if let Some(rest_entry) = rest_entry {
                        self.builder.set_current_block(rest_entry);
                    }
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.lower_pattern_test(rest_local, rest_pat, success, failure);
                }
            }
        }
    }

    fn is_irrefutable_catch_all(&self, pat_id: AstPatId) -> bool {
        match &self.body.patterns[pat_id] {
            AstPattern::Wildcard => true,
            // `let x` is irrefutable; `let x: <pat>` is refutable iff
            // the inner sub-pattern is.
            AstPattern::Bind { subpat, .. } => match subpat {
                None => true,
                Some(sp) => self.is_irrefutable_catch_all(*sp),
            },
            AstPattern::Or(parts) => parts
                .iter()
                .any(|part| self.is_irrefutable_catch_all(*part)),
            AstPattern::Type(_) | AstPattern::Class { .. } | AstPattern::Array { .. } => false,
        }
    }

    /// Type ascription on the pattern, if any. For `let x: T` (where the
    /// sub-pattern is a `Type`), returns `T`. For `[…]: T` (Array with
    /// ascription), returns `T`. Returns `None` for everything else
    /// (including `let x: <non-type-pattern>`).
    fn pattern_narrow_type(&self, pat_id: AstPatId) -> Option<AstTypeExpr> {
        match &self.body.patterns[pat_id] {
            AstPattern::Bind {
                subpat: Some(sp), ..
            } => match &self.body.patterns[*sp] {
                AstPattern::Type(t) => Some(t.clone()),
                _ => None,
            },
            AstPattern::Array {
                ascription: Some(t),
                ..
            } => Some(t.clone()),
            _ => None,
        }
    }

    fn bind_pattern(&mut self, scrutinee: Local, pat_id: AstPatId) {
        // Pass the root pat_id through recursion: HIR registers bindings
        // keyed by the OUTER pattern PatId (the let-stmt's pattern, the
        // match-arm's pattern, etc.), never by the inner Bind. To wire up
        // closure capture lookups correctly, we register the local against
        // that root.
        self.bind_pattern_inner(scrutinee, pat_id, pat_id, pat_id, false, false);
    }

    fn bind_pattern_with_fresh_cells(&mut self, scrutinee: Local, pat_id: AstPatId) {
        self.bind_pattern_inner(scrutinee, pat_id, pat_id, pat_id, true, false);
    }

    fn bind_pattern_inner(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        root: AstPatId,
        narrow_root: AstPatId,
        fresh_cell: bool,
        is_watched: bool,
    ) {
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, subpat } => {
                // For Or-patterns we look up `pat_types` against the inner
                // bind's `pat_id`, not the outer `root`. That's safe because
                // TIR rejects Or-branches whose shared bindings disagree on
                // type (`OrPatternBindingTypeMismatch`), so by the time we
                // reach MIR every alternative's bind for `name` carries the
                // same type. If you ever loosen that TIR invariant, switch
                // this lookup to `root` so we don't over-narrow.
                let narrow = self.pattern_narrow_type(narrow_root);
                let ty = if let Some(narrow) = &narrow {
                    self.resolve_type_annotation(narrow)
                } else {
                    self.pat_types
                        .get(&self.pat_metadata_key(pat_id))
                        .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
                        .unwrap_or_else(|| self.builder.local_ty(scrutinee))
                };
                let local = self
                    .builder
                    .declare_local(Some(name.clone()), ty, None, is_watched);
                if fresh_cell {
                    self.builder.fresh_cell(local);
                }
                self.builder.assign(
                    Place::local(local),
                    Rvalue::Use(Operand::Copy(Place::Local(scrutinee))),
                );
                self.record_pattern_binding_local(root, &name, local);
                self.locals.insert(name, local);
                // Recurse into the sub-pattern so inner bindings (e.g.
                // `let x: let y` or `let x: Class { f }`) get emitted too.
                if let Some(sp) = subpat {
                    self.bind_pattern_inner(scrutinee, sp, root, sp, fresh_cell, is_watched);
                }
            }
            AstPattern::Or(parts) => {
                let mut bindings = Vec::new();
                self.collect_pattern_bindings(pat_id, &mut bindings);
                if bindings.is_empty() {
                    return;
                }
                self.declare_or_pattern_bindings(pat_id, root, fresh_cell, is_watched);
                self.lower_or_pattern_assign_existing(scrutinee, &parts, root, narrow_root);
            }
            AstPattern::Class { fields, .. } => {
                for f in fields {
                    if let Some(field_local) =
                        self.project_class_pattern_field(scrutinee, pat_id, f.pat, &f.field)
                    {
                        self.bind_pattern_inner(
                            field_local,
                            f.pat,
                            root,
                            f.pat,
                            fresh_cell,
                            is_watched,
                        );
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.bind_pattern_inner(
                        elem_local, elem_pat, root, elem_pat, fresh_cell, is_watched,
                    );
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.bind_pattern_inner(
                        rest_local, rest_pat, root, rest_pat, fresh_cell, is_watched,
                    );
                }
                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.bind_pattern_inner(
                        elem_local, elem_pat, root, elem_pat, fresh_cell, is_watched,
                    );
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) => {}
        }
    }

    fn collect_pattern_bindings(&self, pat_id: AstPatId, out: &mut Vec<(Name, AstPatId)>) {
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, subpat } => {
                out.push((name, pat_id));
                if let Some(sp) = subpat {
                    self.collect_pattern_bindings(sp, out);
                }
            }
            AstPattern::Or(parts) => {
                if let Some(first) = parts.first() {
                    self.collect_pattern_bindings(*first, out);
                }
            }
            AstPattern::Class { fields, .. } => {
                for field in fields {
                    self.collect_pattern_bindings(field.pat, out);
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for part in prefix {
                    self.collect_pattern_bindings(part, out);
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    self.collect_pattern_bindings(rest_pat, out);
                }
                for part in suffix {
                    self.collect_pattern_bindings(part, out);
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) => {}
        }
    }

    fn declare_or_pattern_bindings(
        &mut self,
        pat_id: AstPatId,
        root: AstPatId,
        fresh_cell: bool,
        is_watched: bool,
    ) {
        let mut bindings = Vec::new();
        self.collect_pattern_bindings(pat_id, &mut bindings);
        for (name, bind_pat) in bindings {
            let local = self.builder.declare_local(
                Some(name.clone()),
                self.pat_ty(bind_pat),
                None,
                is_watched,
            );
            if fresh_cell {
                self.builder.fresh_cell(local);
            }
            self.record_pattern_binding_local(root, &name, local);
            self.locals.insert(name, local);
        }
    }

    fn lower_or_pattern_assign_existing(
        &mut self,
        scrutinee: Local,
        parts: &[AstPatId],
        root: AstPatId,
        narrow_root: AstPatId,
    ) {
        if parts.is_empty() {
            self.builder.unreachable();
            return;
        }

        let join = self.builder.create_block();
        let failure = self.builder.create_block();

        for (idx, part) in parts.iter().copied().enumerate() {
            let body = self.builder.create_block();
            let next = if idx + 1 == parts.len() {
                failure
            } else {
                self.builder.create_block()
            };
            self.lower_pattern_test(scrutinee, part, body, next);

            self.builder.set_current_block(body);
            self.assign_pattern_to_existing(scrutinee, part, root, narrow_root);
            if !self.builder.is_current_terminated() {
                self.builder.goto(join);
            }

            if idx + 1 < parts.len() {
                self.builder.set_current_block(next);
            }
        }

        self.builder.set_current_block(failure);
        self.builder.unreachable();
        self.builder.set_current_block(join);
    }

    fn assign_pattern_to_existing(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        root: AstPatId,
        narrow_root: AstPatId,
    ) {
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, .. } => {
                if let Some(&local) = self.locals.get(&name) {
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::Copy(Place::Local(scrutinee))),
                    );
                    self.record_pattern_binding_local(root, &name, local);
                }
            }
            AstPattern::Or(parts) => {
                self.lower_or_pattern_assign_existing(scrutinee, &parts, root, narrow_root);
            }
            AstPattern::Class { fields, .. } => {
                for field in fields {
                    if let Some(field_local) =
                        self.project_class_pattern_field(scrutinee, pat_id, field.pat, &field.field)
                    {
                        self.assign_pattern_to_existing(field_local, field.pat, root, field.pat);
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.assign_pattern_to_existing(elem_local, elem_pat, root, elem_pat);
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.assign_pattern_to_existing(rest_local, rest_pat, root, rest_pat);
                }
                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.assign_pattern_to_existing(elem_local, elem_pat, root, elem_pat);
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) => {}
        }
    }
}

// ─── Type tag classification (shared by match/catch switch dispatch) ──────────

impl LoweringContext<'_> {
    /// Classify a pattern into type tag value(s) for switch dispatch.
    /// Classify a pattern as type-tag-eligible and return its tag(s).
    ///
    /// Shared by match and catch lowering.
    ///
    /// Returns `Some(tags)` for `TypedBinding` and Binding-with-TIR-type patterns
    /// that resolve to primitive or class types. Returns `None` for literals,
    /// wildcards, enum variants, and types without tag mappings.
    fn classify_pattern_type_tag(&self, pat_id: AstPatId) -> Option<Vec<i64>> {
        let pat = &self.body.patterns[pat_id];
        if self.pattern_contains_structural(pat_id) {
            return None;
        }
        // Bind/Array patterns may carry a `:T` type ascription; resolve
        // via the ascription's TypeExpr if present. For Bind, the
        // ascription is the sub-pattern when it's a `Type(...)` shape.
        let ascription_ty = match pat {
            AstPattern::Bind {
                subpat: Some(sp), ..
            } => match &self.body.patterns[*sp] {
                AstPattern::Type(t) => Some(t.clone()),
                _ => None,
            },
            AstPattern::Array {
                ascription: Some(t),
                ..
            } => Some(t.clone()),
            _ => None,
        };
        if let Some(ty_expr) = ascription_ty {
            if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)) {
                let resolved = self.resolved_aliases.convert(tir_ty);
                if let Some(tags) = self.ty_to_type_tags(&resolved) {
                    return Some(tags);
                }
            }
            let resolved = self.resolve_type_annotation(&ty_expr);
            return self.ty_to_type_tags(&resolved);
        }
        match pat {
            AstPattern::Wildcard => None,
            AstPattern::Bind { .. } => {
                let tir_ty = self.pat_types.get(&self.pat_metadata_key(pat_id))?;
                let resolved = self.resolved_aliases.convert(tir_ty);
                self.ty_to_type_tags(&resolved)
            }
            AstPattern::Type(_) => {
                if let Some(tir_ty) = self.pat_types.get(&self.pat_metadata_key(pat_id)) {
                    let resolved = self.resolved_aliases.convert(tir_ty);
                    if let Some(tags) = self.ty_to_type_tags(&resolved) {
                        return Some(tags);
                    }
                }
                if let AstPattern::Type(ty_expr) = pat {
                    let resolved = self.resolve_type_annotation(ty_expr);
                    return self.ty_to_type_tags(&resolved);
                }
                None
            }
            _ => None,
        }
    }

    /// Convert a `Ty` to the list of type tag integers it corresponds to.
    /// Returns `None` if the type has no simple tag representation.
    ///
    /// Supports primitives (globally-stable tags) and class types (looked up
    /// from `class_type_tags`). Union types are flattened — all members must
    /// be tag-eligible.
    fn ty_to_type_tags(&self, ty: &Ty) -> Option<Vec<i64>> {
        match ty {
            Ty::Union(members, _) => {
                let mut tags = Vec::new();
                for m in members {
                    let member_tags = self.ty_to_type_tags(m)?;
                    tags.extend(member_tags);
                }
                Some(tags)
            }
            _ => self.type_tag_for_ty(ty).map(|tag| vec![tag]),
        }
    }
}

/// Format a type tag integer as a human-readable name for switch arm debug metadata.
fn format_type_tag_name(tag: i64) -> String {
    match tag {
        baml_type::typetag::INT => "int".to_string(),
        baml_type::typetag::STRING => "string".to_string(),
        baml_type::typetag::BOOL => "bool".to_string(),
        baml_type::typetag::NULL => "null".to_string(),
        baml_type::typetag::FLOAT => "float".to_string(),
        baml_type::typetag::LIST => "list".to_string(),
        baml_type::typetag::MAP => "map".to_string(),
        baml_type::typetag::ENUM => "enum".to_string(),
        baml_type::typetag::FUNCTION => "function".to_string(),
        baml_type::typetag::FUTURE => "future".to_string(),
        baml_type::typetag::TYPE => "type".to_string(),
        baml_type::typetag::COLLECTOR => "collector".to_string(),
        baml_type::typetag::UINT8ARRAY => "uint8array".to_string(),
        tag if tag >= baml_type::typetag::CLASS_BASE => {
            format!("class#{}", tag - baml_type::typetag::CLASS_BASE)
        }
        _ => format!("tag#{tag}"),
    }
}

// ─── Catch lowering ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_catch(
        &mut self,
        _expr_id: AstExprId,
        base: AstExprId,
        clauses: &[baml_compiler2_ast::CatchClause],
        dest: &Place,
    ) {
        use baml_compiler2_ast::CatchClauseKind;

        #[derive(Clone)]
        struct ClauseLocals {
            binding_name: Option<Name>,
            binding_local: Option<Local>,
            binding_copy_local: Option<Local>,
            stack_trace_name: Option<Name>,
            stack_trace_payload: Option<Local>,
            stack_trace_copy_local: Option<Local>,
        }

        fn install_clause_locals(
            ctx: &mut LoweringContext<'_>,
            error_local: Local,
            clause: &ClauseLocals,
        ) {
            if let (Some(name), Some(local)) = (&clause.binding_name, clause.binding_local) {
                ctx.locals.insert(name.clone(), local);
            }
            if let Some(binding_copy_local) = clause.binding_copy_local {
                ctx.builder.assign(
                    Place::local(binding_copy_local),
                    Rvalue::Use(Operand::Copy(Place::Local(error_local))),
                );
            }
            if let (Some(name), Some(local)) =
                (&clause.stack_trace_name, clause.stack_trace_copy_local)
            {
                ctx.locals.insert(name.clone(), local);
            }
            if let (Some(payload), Some(copy_local)) =
                (clause.stack_trace_payload, clause.stack_trace_copy_local)
                && payload != copy_local
            {
                ctx.builder.assign(
                    Place::local(copy_local),
                    Rvalue::Use(Operand::Copy(Place::Local(payload))),
                );
            }
        }

        let saved_catch_outer_locals = self.locals.clone();
        let bb_join = self.builder.create_block();
        let bb_handler = self.builder.create_block();

        // Use the user-provided binding name (e.g. `e` from `catch (e)`) so it
        // shows up in bytecode instead of an anonymous `_N` temp. Only do this
        // for single-clause catches with a non-captured binding.
        let single_clause_binding_name = clauses.first().and_then(|c| {
            if clauses.len() == 1 && !self.pattern_binding_is_captured(c.binding) {
                self.body.patterns[c.binding]
                    .binding_name(&self.body.patterns)
                    .cloned()
            } else {
                None
            }
        });
        let error_local = self.builder.declare_local(
            single_clause_binding_name,
            Ty::BuiltinUnknown {
                attr: TyAttr::default(),
            },
            None,
            false,
        );

        let stack_trace_local = clauses
            .iter()
            .any(|c| c.stack_trace_binding.is_some())
            .then(|| {
                self.builder.declare_local(
                    None,
                    Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    },
                    None,
                    false,
                )
            });

        let mut clause_locals = Vec::with_capacity(clauses.len());
        for clause in clauses {
            let binding_name = self.body.patterns[clause.binding]
                .binding_name(&self.body.patterns)
                .cloned();
            let binding_is_captured = self.pattern_binding_is_captured(clause.binding);
            let (binding_local, binding_copy_local) = match binding_name.clone() {
                Some(name) if binding_is_captured => {
                    let local = self.builder.declare_local(
                        Some(name.clone()),
                        Ty::BuiltinUnknown {
                            attr: TyAttr::default(),
                        },
                        None,
                        false,
                    );
                    self.record_pattern_binding_local(clause.binding, &name, local);
                    (Some(local), Some(local))
                }
                Some(name) => {
                    self.record_pattern_binding_local(clause.binding, &name, error_local);
                    (Some(error_local), None)
                }
                None => (None, None),
            };

            let (stack_trace_name, stack_trace_copy_local) = if let (Some(st_pat), Some(payload)) =
                (clause.stack_trace_binding, stack_trace_local)
            {
                let name = self.body.patterns[st_pat]
                    .binding_name(&self.body.patterns)
                    .cloned();
                let is_captured = self.pattern_binding_is_captured(st_pat);
                match name.clone() {
                    Some(name) if is_captured => {
                        let local = self.builder.declare_local(
                            Some(name.clone()),
                            Ty::BuiltinUnknown {
                                attr: TyAttr::default(),
                            },
                            None,
                            false,
                        );
                        self.record_pattern_binding_local(st_pat, &name, local);
                        (Some(name), Some(local))
                    }
                    Some(name) => {
                        self.record_pattern_binding_local(st_pat, &name, payload);
                        (Some(name), Some(payload))
                    }
                    None => (None, None),
                }
            } else {
                (None, None)
            };

            clause_locals.push(ClauseLocals {
                binding_name,
                binding_local,
                binding_copy_local,
                stack_trace_name,
                stack_trace_payload: stack_trace_local,
                stack_trace_copy_local,
            });
        }

        // Flatten all arms from all clauses (blocks created lazily below).
        let mut arms: Vec<(baml_compiler2_ast::CatchArm, bool, usize)> = Vec::new();
        for (clause_idx, clause) in clauses.iter().enumerate() {
            for &arm_id in &clause.arms {
                let arm = self.body.catch_arms[arm_id].clone();
                let is_wildcard = self.is_irrefutable_catch_all(arm.pattern);
                arms.push((arm, is_wildcard, clause_idx));
            }
        }

        let has_wildcard = arms.iter().any(|(_, is_wc, _)| *is_wc);
        let is_catch_all_panics = clauses
            .iter()
            .any(|clause| matches!(clause.kind, CatchClauseKind::CatchAllPanics));

        // Record the catch region (always one handler, one exception table entry).
        let body_entry = self.builder.current_block();
        self.builder.catch_regions.push(CatchRegion {
            body_entry,
            handler: bb_handler,
            error_local,
            stack_trace_local,
        });

        let prev_catch = self.catch_context.take();
        self.catch_context = Some(CatchContext {
            unwind_target: bb_handler,
            error_local,
        });

        // Lower the try body.
        self.lower_expr(base, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.catch_context = prev_catch;

        // Before the wildcard arm (if any), insert a throw_if_panic guard to
        // prevent the wildcard from swallowing panics the programmer didn't
        // explicitly name. Skipped for catch_all_panics (user wants everything).
        let needs_throw_if_panic = has_wildcard && !is_catch_all_panics;

        // Attempt switch-style dispatch on type tags.
        // If all non-wildcard arms have pure type-test patterns, emit a single
        // Switch on Rvalue::TypeTag instead of a sequential is_type chain.
        let switch_arms: Vec<(AstPatId, AstExprId, Option<AstExprId>)> = arms
            .iter()
            .map(|(arm, _, _)| (arm.pattern, arm.body, None))
            .collect();
        self.builder.set_current_block(bb_handler);
        if clauses.len() == 1 {
            install_clause_locals(self, error_local, &clause_locals[0]);
        }
        if clauses.len() == 1
            && self.try_lower_as_switch(
                error_local,
                &switch_arms,
                dest.clone(),
                bb_join,
                SwitchOtherwise::Catch {
                    error_local,
                    needs_throw_if_panic,
                },
                None,
            )
        {
            self.builder.set_current_block(bb_join);
            self.restore_active_locals(saved_catch_outer_locals);
            return;
        }

        // Fallback: sequential pattern-test chain.
        // Create body blocks now (not created earlier so the switch path
        // doesn't leave orphaned unterminated blocks).
        let arms_with_blocks: Vec<_> = arms
            .iter()
            .map(|(arm, is_wc, clause_idx)| {
                (
                    arm.clone(),
                    self.builder.create_block(),
                    *is_wc,
                    *clause_idx,
                )
            })
            .collect();

        for &(ref arm, body_block, is_wildcard, _) in &arms_with_blocks {
            if is_wildcard && needs_throw_if_panic {
                let bb_wildcard = self.builder.create_block();
                self.builder
                    .throw_if_panic(Operand::Copy(Place::Local(error_local)), bb_wildcard);
                self.builder.set_current_block(bb_wildcard);
            }

            let bb_arm_next = self.builder.create_block();
            self.lower_pattern_test(error_local, arm.pattern, body_block, bb_arm_next);
            self.builder.set_current_block(bb_arm_next);
        }

        // Rethrow if nothing matched.
        if !self.builder.is_current_terminated() {
            self.builder.throw(Operand::Copy(Place::Local(error_local)));
        }

        // Lower each arm body.
        for &(ref arm, body_block, _, clause_idx) in &arms_with_blocks {
            self.builder.set_current_block(body_block);
            let saved_locals = self.locals.clone();
            let watched_depth = self.watched_locals_stack.len();
            let clause = clause_locals[clause_idx].clone();
            install_clause_locals(self, error_local, &clause);
            self.bind_pattern(error_local, arm.pattern);
            self.lower_expr(arm.body, dest.clone());
            if !self.builder.is_current_terminated() {
                // A `watch let` declared inside a catch-arm body must be
                // torn down on fallthrough.
                self.emit_unwatch_to_depth(watched_depth);
                self.builder.goto(bb_join);
            }
            // Restore name→local map AND truncate the watched stack back to
            // the arm-entry depth (mirrors `lower_scoped_block`).
            self.restore_locals_after_scope(saved_locals, watched_depth);
        }

        self.builder.set_current_block(bb_join);
        self.restore_active_locals(saved_catch_outer_locals);
    }
}

// ─── 3.7: Entry points ────────────────────────────────────────────────────────

/// Lower a top-level let binding's initializer into a `MirFunctionBody`.
///
/// The body has arity 0 and contains only the initializer expression.
/// Used by `compile_init_function` in the emit crate to compile let initializers
/// into bytecode for the `$init` function.
pub fn lower_let_body<'db>(
    db: &'db dyn crate::Db,
    let_loc: LetLoc<'db>,
    opt: crate::OptLevel,
) -> Option<(MirFunctionBody, Vec<MirFunction>)> {
    let body = let_body(db, let_loc);
    let source_map = let_body_source_map(db, let_loc);

    match body.as_ref() {
        LetBody::Expr(expr_body) => {
            let mut ctx =
                LoweringContext::new_for_let(db, let_loc, expr_body.clone(), source_map, opt);
            let mir_body = ctx.lower_let_body_inner();
            let lambdas = std::mem::take(&mut ctx.pending_lambdas);
            Some((mir_body, lambdas))
        }
        LetBody::Missing => None,
    }
}

pub fn lower_function<'db>(
    db: &'db dyn crate::Db,
    func_loc: FunctionLoc<'db>,
    opt: crate::OptLevel,
) -> MirFunction {
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let source_map = baml_compiler2_ppir::function_body_source_map(db, func_loc);
    let item_ref = def_to_item_ref(
        db,
        baml_compiler2_hir::contributions::Definition::Function(func_loc),
    );
    let sig = baml_compiler2_ppir::function_signature(db, func_loc);
    let arity = sig.params.len();

    match body.as_ref() {
        FunctionBody::Expr(expr_body) => {
            let mut ctx = LoweringContext::new(db, func_loc, expr_body.clone(), source_map, opt);
            let mut mir = ctx.lower_function_body();
            mir.item_ref = item_ref;
            mir
        }
        FunctionBody::Builtin(kind) => {
            use baml_compiler2_ast::BuiltinKind;
            // For IO builtins (`$rust_io_function`), the compiler injects one
            // synthetic trailing value-arg slot for each generic type parameter
            // (e.g. `parse<T>` gets one extra `baml_type::Ty` slot after the
            // regular params).  We must include those synthetic slots in the
            // arity so that `ScheduleFuture` pops the correct number of args
            // from the stack.
            let extra_arity = if matches!(kind, BuiltinKind::Io) {
                // For IO builtins (`$rust_io_function`), the compiler injects
                // one synthetic trailing value-arg slot for each *function-level*
                // generic type parameter.  Class-level generics (from the
                // enclosing class definition) do NOT generate extra slots —
                // `baml_builtins2_codegen` only adds type-arg params for
                // function-level generics.  We therefore only count the
                // function's own generic_params here.
                let item_tree = file_item_tree(db, func_loc.file(db));
                item_tree[func_loc.id(db)].generic_params.len()
            } else {
                0
            };
            MirFunction {
                arity: arity + extra_arity,
                span: None,
                item_ref,
                kind: MirFunctionKind::Builtin(*kind),
                lambdas: vec![],
            }
        }
        FunctionBody::Missing => MirFunction {
            arity,
            span: None,
            item_ref,
            kind: MirFunctionKind::Bytecode(MirFunctionBody {
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Unreachable),
                    span: None,
                    terminator_span: None,
                }],
                entry: BlockId(0),
                locals: (0..=arity)
                    .map(|_| LocalDecl {
                        name: None,
                        ty: baml_type::Ty::Void {
                            attr: baml_type::TyAttr::default(),
                        },
                        is_captured: false,
                        span: None,
                        scope_span: None,
                        is_watched: false,
                    })
                    .collect(),
                catch_regions: vec![],
                viz_nodes: vec![],
            }),
            lambdas: vec![],
        },
    }
}
