use std::collections::{HashMap, HashSet};

use baml_base::Name;
use baml_type::{MediaKind, Ty, TyAttr, TypeName};
use indexmap::IndexMap;

use crate::{
    builder::MirBuilder,
    ir::{
        AggregateKind, BasicBlock, BinOp, BlockId, CatchRegion, Constant, IndexKind, ItemRef,
        Local, LocalDecl, MirFunction, MirFunctionBody, MirFunctionKind, Operand, Place, Rvalue,
        StatementKind, Terminator,
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

use baml_compiler2_tir::ty::{PrimitiveType, QualifiedTypeName, Ty as Tir2Ty};

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
        Tir2Ty::Class(qtn, _, attr) => Ty::Class(qtn_to_type_name(qtn), attr.clone()),
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
        Tir2Ty::Function { params, ret, attr } => Ty::Function {
            params: params
                .iter()
                .map(|(_, t)| convert_tir2_ty(t, resolved))
                .collect(),
            ret: Box::new(convert_tir2_ty(ret, resolved)),
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
        // TypeVar should never reach MIR — it is erased to Unknown before VIR.
        // Map defensively to Void as error recovery.
        Tir2Ty::TypeVar(..) => Ty::Void { attr },
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
        MemberResolution::FreeFunction { func_loc } => {
            let pkg_info = file_package(db, func_loc.file(db));
            let item_tree = file_item_tree(db, func_loc.file(db));
            let func_data = &item_tree[func_loc.id(db)];
            Some(ItemRef::Free {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                name: func_data.name.clone(),
            })
        }
        MemberResolution::Method {
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
        MemberResolution::FreeLet { let_loc } => {
            let pkg_info = file_package(db, let_loc.file(db));
            let item_tree = file_item_tree(db, let_loc.file(db));
            let let_data = &item_tree[let_loc.id(db)];
            Some(ItemRef::Free {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                name: let_data.name.clone(),
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
    Pattern as AstPattern, Stmt as AstStmt, StmtId as AstStmtId, UnaryOp as AstUnaryOp,
};
use baml_compiler2_hir::{
    body::{FunctionBody, LetBody, let_body, let_body_source_map},
    file_semantic_index,
    loc::{FunctionLoc, LetLoc},
    package::{PackageId, package_dependencies, package_items},
    scope::FileScopeId,
};
use baml_compiler2_tir::{
    inference::infer_scope_types,
    resolve::{ResolvedName, resolve_name_at_in_scope},
};
use rustc_hash::FxHashMap;

struct LoweringContext<'db> {
    db: &'db dyn crate::Db,
    builder: MirBuilder,
    locals: HashMap<Name, Local>,
    loop_context: Option<LoopContext>,
    catch_context: Option<CatchContext>,
    exit_block: BlockId,

    // Eagerly aggregated type maps from all scopes in the function.
    // Key is (FileScopeId, AstExprId) to avoid collisions between lambda body
    // arenas and their parent function body arenas (both start ExprIds at 0).
    expr_types: FxHashMap<(FileScopeId, AstExprId), Tir2Ty>,
    pat_types: FxHashMap<(FileScopeId, AstPatId), Tir2Ty>,
    // Member resolutions from TIR: (scope, ExprId) → MemberResolution
    // Keyed by (FileScopeId, AstExprId) to avoid collisions between lambda body
    // arenas and their parent function body arenas (both start ExprIds at 0).
    resolutions:
        FxHashMap<(FileScopeId, AstExprId), baml_compiler2_tir::inference::MemberResolution<'db>>,
    // Match expressions that TIR determined are exhaustive
    exhaustive_matches: rustc_hash::FxHashSet<(FileScopeId, AstExprId)>,

    // The FileScopeId of the expression body currently being lowered.
    // Updated when descending into lambda bodies (Phase 3+).
    current_scope: FileScopeId,

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
    // (AstPattern::EnumVariant) only provides the enum's short Name, not a full
    // TypeName with module_path. Upgrading to TypeName would require resolving the
    // enum's package at each match site.
    class_fields: IndexMap<TypeName, IndexMap<String, usize>>,
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
    // Maps captured variable name -> index into the closure's captures array.
    // Used by `lower_path_expr` to resolve references to captured variables as
    // `Place::Capture(idx)` instead of `Place::Local(_)`.
    capture_indices: Option<HashMap<Name, usize>>,

    // Names that were added to the current lambda's capture list transitively
    // (i.e. because an inner lambda needed them but they weren't in the HIR
    // capture list for this lambda).  Populated by `lower_lambda` when building
    // an inner closure's capture operands.  Collected by the *parent*
    // `lower_lambda` call after the body is lowered so it can extend the outer
    // MakeClosure with extra captures.
    transitive_captures_needed: Vec<Name>,

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
        enum_variants: &mut IndexMap<Name, IndexMap<String, usize>>,
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
                        for (idx, field) in class_data.fields.iter().enumerate() {
                            fields.insert(field.name.to_string(), idx);
                        }
                        class_fields.insert(tn, fields);
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

        // --- Eagerly aggregate expr_types, pat_types, resolutions, and exhaustive_matches from all scopes ---
        let mut expr_types: FxHashMap<(FileScopeId, AstExprId), Tir2Ty> = FxHashMap::default();
        let mut pat_types: FxHashMap<(FileScopeId, AstPatId), Tir2Ty> = FxHashMap::default();
        let mut resolutions: FxHashMap<
            (FileScopeId, AstExprId),
            baml_compiler2_tir::inference::MemberResolution<'db>,
        > = FxHashMap::default();
        let mut exhaustive_matches: rustc_hash::FxHashSet<(FileScopeId, AstExprId)> =
            rustc_hash::FxHashSet::default();

        let merge_scope =
            |fsi: FileScopeId,
             expr_types: &mut FxHashMap<(FileScopeId, AstExprId), Tir2Ty>,
             pat_types: &mut FxHashMap<(FileScopeId, AstPatId), Tir2Ty>,
             resolutions: &mut FxHashMap<
                (FileScopeId, AstExprId),
                baml_compiler2_tir::inference::MemberResolution<'db>,
            >,
             exhaustive_matches: &mut rustc_hash::FxHashSet<(FileScopeId, AstExprId)>| {
                let scope_id = index.scope_ids[fsi.index() as usize];
                let inference = infer_scope_types(db, scope_id);
                for (&expr_id, ty) in inference.iter_expressions() {
                    expr_types.insert((fsi, expr_id), ty.clone());
                }
                for (&pat_id, ty) in inference.iter_bindings() {
                    pat_types.insert((fsi, pat_id), ty.clone());
                }
                for (&expr_id, res) in inference.iter_resolutions() {
                    resolutions.insert((fsi, expr_id), res.clone());
                }
                for &expr_id in inference.iter_exhaustive_matches() {
                    exhaustive_matches.insert((fsi, expr_id));
                }
            };

        // Include the function scope itself
        merge_scope(
            func_scope_id,
            &mut expr_types,
            &mut pat_types,
            &mut resolutions,
            &mut exhaustive_matches,
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
            );
        }

        // --- Build class_fields / enum_variants from PackageItems ---
        let pkg_info = file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());

        let mut class_fields: IndexMap<TypeName, IndexMap<String, usize>> = IndexMap::new();
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
                &mut enum_variants,
            );
        }

        // Include classes from the current package (overwrites on collision).
        let pkg_items = package_items(db, pkg_id);
        Self::populate_from_package(
            db,
            pkg_items,
            &pkg_info.package,
            &mut class_fields,
            &mut enum_variants,
        );

        // Build class_type_tags using the same file-iteration order as the emitter,
        // so that switch arms get the same integer tags as runtime class.type_tag fields.
        let class_type_tags = Self::build_class_type_tags(db);

        let resolved_aliases = ResolvedAliases::for_package(db, pkg_id);

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
            loop_context: None,
            catch_context: None,
            exit_block: BlockId(0), // placeholder; overwritten in lower_function_body
            expr_types,
            pat_types,
            resolutions,
            exhaustive_matches,
            current_scope: func_scope_id,
            body: expr_body,
            source_map,
            file,
            func_loc: Some(func_loc),
            scope_func_name: Some(func_data.name.clone()),
            class_fields,
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

        // --- Eagerly aggregate expr_types, pat_types, resolutions from let scope ---
        let mut expr_types: FxHashMap<(FileScopeId, AstExprId), Tir2Ty> = FxHashMap::default();
        let mut pat_types: FxHashMap<(FileScopeId, AstPatId), Tir2Ty> = FxHashMap::default();
        let mut resolutions: FxHashMap<
            (FileScopeId, AstExprId),
            baml_compiler2_tir::inference::MemberResolution<'db>,
        > = FxHashMap::default();
        let mut exhaustive_matches: rustc_hash::FxHashSet<(FileScopeId, AstExprId)> =
            rustc_hash::FxHashSet::default();

        let merge_scope =
            |fsi: FileScopeId,
             expr_types: &mut FxHashMap<(FileScopeId, AstExprId), Tir2Ty>,
             pat_types: &mut FxHashMap<(FileScopeId, AstPatId), Tir2Ty>,
             resolutions: &mut FxHashMap<
                (FileScopeId, AstExprId),
                baml_compiler2_tir::inference::MemberResolution<'db>,
            >,
             exhaustive_matches: &mut rustc_hash::FxHashSet<(FileScopeId, AstExprId)>| {
                let scope_id = index.scope_ids[fsi.index() as usize];
                let inference = infer_scope_types(db, scope_id);
                for (&expr_id, ty) in inference.iter_expressions() {
                    expr_types.insert((fsi, expr_id), ty.clone());
                }
                for (&pat_id, ty) in inference.iter_bindings() {
                    pat_types.insert((fsi, pat_id), ty.clone());
                }
                for (&expr_id, res) in inference.iter_resolutions() {
                    resolutions.insert((fsi, expr_id), res.clone());
                }
                for &expr_id in inference.iter_exhaustive_matches() {
                    exhaustive_matches.insert((fsi, expr_id));
                }
            };

        // Include the let scope itself
        merge_scope(
            let_scope_id,
            &mut expr_types,
            &mut pat_types,
            &mut resolutions,
            &mut exhaustive_matches,
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
            );
        }

        // --- Build class_fields / enum_variants from PackageItems ---
        let pkg_info = file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());

        let mut class_fields: IndexMap<TypeName, IndexMap<String, usize>> = IndexMap::new();
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
                &mut enum_variants,
            );
        }

        // Include classes from the current package (overwrites on collision).
        let pkg_items = package_items(db, pkg_id);
        Self::populate_from_package(
            db,
            pkg_items,
            &pkg_info.package,
            &mut class_fields,
            &mut enum_variants,
        );

        // Build class_type_tags using the same file-iteration order as the emitter,
        // so that switch arms get the same integer tags as runtime class.type_tag fields.
        let class_type_tags = Self::build_class_type_tags(db);

        let resolved_aliases = ResolvedAliases::for_package(db, pkg_id);

        LoweringContext {
            db,
            builder: MirBuilder::new(let_name.clone(), 0),
            locals: HashMap::new(),
            loop_context: None,
            catch_context: None,
            exit_block: BlockId(0), // placeholder; overwritten in lower_let_body_inner
            expr_types,
            pat_types,
            resolutions,
            exhaustive_matches,
            current_scope: let_scope_id,
            body: expr_body,
            source_map,
            file,
            func_loc: None,
            scope_func_name: Some(let_name),
            class_fields,
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

    /// Get the `baml_type::Ty` for an expression by looking up in the aggregated map
    /// and converting from TIR Ty. Uses `current_scope` as the `FileScopeId` key.
    fn expr_ty(&self, expr_id: AstExprId) -> Ty {
        self.expr_types
            .get(&(self.current_scope, expr_id))
            .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
            .unwrap_or(Ty::Void {
                attr: TyAttr::default(),
            })
    }

    /// Get the `baml_type::Ty` for a pattern binding
    fn pat_ty(&self, pat_id: AstPatId) -> Ty {
        self.pat_types
            .get(&(self.current_scope, pat_id))
            .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
            .unwrap_or(Ty::Void {
                attr: TyAttr::default(),
            })
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
        for (param_name, param_te) in &sig.params {
            let param_ty = if param_name.as_str() == "self"
                && matches!(param_te, baml_compiler2_ast::TypeExpr::Unknown { .. })
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
                    param_te,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &[],
                    &mut diags,
                );
                self.resolved_aliases.convert(&tir_ty)
            };
            let local = self
                .builder
                .declare_local(Some(param_name.clone()), param_ty, None, false);
            self.locals.insert(param_name.clone(), local);
        }

        // Entry and exit blocks
        let entry = self.builder.create_block();
        let exit = self.builder.create_block();
        self.exit_block = exit;
        self.builder.set_current_block(entry);

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

        // Mark locals that are captured by nested lambdas with `is_captured = true`.
        // The HIR `ScopeBindings.captured_names` for the function scope records which
        // names are captured by any descendant lambda. These locals need cell wrapping.
        {
            let func_scope_id = self.current_scope;
            let index = file_semantic_index(self.db, self.file);
            if let Some(sb) = index.scope_bindings.get(func_scope_id.index() as usize) {
                for captured_name in &sb.captured_names {
                    if let Some(&local) = self.locals.get(captured_name) {
                        self.builder.local_decl_mut(local).is_captured = true;
                    }
                }
            }
        }

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
        // `captures` lists (name, DefinitionSite) pairs that the lambda reads from
        // enclosing scopes. The DefinitionSite uniquely identifies the declaration
        // even with shadowing.
        // We build `capture_indices` (name → index in closure.captures[]) so that
        // `lower_path_expr` and `lower_lvalue` can emit Place::Capture(idx).
        let hir_captures: Vec<Name> = {
            let index = file_semantic_index(self.db, self.file);
            index
                .scope_bindings
                .get(lambda_scope_id.index() as usize)
                .map(|sb| sb.captures.iter().map(|(name, _)| name.clone()).collect())
                .unwrap_or_default()
        };
        let lambda_capture_indices: HashMap<Name, usize> = hir_captures
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i))
            .collect();

        // Save parent state.
        let saved_builder = std::mem::replace(
            &mut self.builder,
            MirBuilder::new(Name::new(&lambda_name), 0),
        );
        let saved_body = std::mem::replace(&mut self.body, lambda_body);
        let saved_source_map = std::mem::replace(&mut self.source_map, lambda_source_map);
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_exit_block = self.exit_block;
        let saved_loop_context = self.loop_context.take();
        let saved_catch_context = self.catch_context.take();
        let saved_watched_locals = std::mem::take(&mut self.watched_locals_stack);
        let saved_current_scope = self.current_scope;
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
        for param in &func_def.params {
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

        // Mark locals that are captured by nested lambdas with `is_captured = true`.
        // This mirrors the same step in `lower_function_body` but for lambdas.
        // Uses the lambda's own scope id (lambda_scope_id) to look up HIR captured_names.
        {
            let index = file_semantic_index(self.db, self.file);
            if let Some(sb) = index.scope_bindings.get(lambda_scope_id.index() as usize) {
                for captured_name in &sb.captured_names {
                    if let Some(&local) = self.locals.get(captured_name) {
                        self.builder.local_decl_mut(local).is_captured = true;
                    }
                }
            }
        }

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
        self.exit_block = saved_exit_block;
        self.loop_context = saved_loop_context;
        self.catch_context = saved_catch_context;
        self.watched_locals_stack = saved_watched_locals;
        self.current_scope = saved_current_scope;
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
        for name in &newly_needed_transitive {
            if !extended_hir_captures.contains(name) {
                extended_hir_captures.push(name.clone());
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
        for name in &extended_hir_captures {
            if let Some(&local) = self.locals.get(name) {
                // Mark the local as captured at the capture site — this is the
                // definitive place where we know the exact Local being captured,
                // even in the presence of shadowing (future-proofing).
                self.builder.local_decl_mut(local).is_captured = true;
                capture_operands.push(Operand::Copy(Place::Local(local)));
            } else if let Some(cap_idx) = self
                .capture_indices
                .as_ref()
                .and_then(|m| m.get(name))
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
                    ci.insert(name.clone(), idx);
                    idx
                };
                // Signal to our parent lambda that it needs to capture this name.
                self.transitive_captures_needed.push(name.clone());
                capture_operands.push(Operand::Copy(Place::Capture(new_idx)));
            }
        }

        // Push this lambda into the parent's pending_lambdas and emit MakeClosure.
        let lambda_pending_idx = self.pending_lambdas.len();
        self.pending_lambdas.push(lambda_mir);

        self.builder.assign(
            dest,
            Rvalue::MakeClosure {
                lambda_idx: lambda_pending_idx,
                captures: capture_operands,
            },
        );
    }
}

// ─── 3.2: Core lower_expr dispatch ───────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_expr(&mut self, expr_id: AstExprId, dest: Place) {
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

            AstExpr::Call { callee, args } => {
                self.lower_call(expr_id, callee, &args, dest);
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
            } => {
                self.lower_object(expr_id, type_name.as_ref(), &fields, &spreads, dest);
            }

            AstExpr::FieldAccess { base, field } => {
                self.lower_field_access(expr_id, base, &field, dest);
            }

            AstExpr::OptionalFieldAccess { base, field } => {
                self.lower_optional_field_access(expr_id, base, &field, dest);
            }

            AstExpr::OptionalIndex { base, index } => {
                self.lower_optional_index(expr_id, base, index, dest);
            }

            AstExpr::OptionalCall { callee, args } => {
                self.lower_optional_call(expr_id, callee, &args, dest);
            }

            AstExpr::Index { base, index } => {
                self.lower_index(expr_id, base, index, dest);
            }

            AstExpr::Block { stmts, tail_expr } => {
                for &stmt_id in &stmts {
                    self.lower_stmt(stmt_id);
                    if self.builder.is_current_terminated() {
                        break; // Remaining stmts are dead code (after return/throw/break/continue)
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
            }

            AstExpr::Match {
                scrutinee, arms, ..
            } => {
                let arms_owned = arms;
                self.lower_match(expr_id, scrutinee, &arms_owned, dest);
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
        }

        self.builder.current_source_span = prev_span;
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
        // Multi-segment paths (e.g. baml.llm.render_prompt) — check TIR resolution first
        if segments.len() > 1 {
            if let Some(resolution) = self
                .resolutions
                .get(&(self.current_scope, expr_id))
                .cloned()
            {
                use baml_compiler2_tir::inference::MemberResolution;
                match &resolution {
                    MemberResolution::Method { .. }
                    | MemberResolution::FreeFunction { .. }
                    | MemberResolution::FreeLet { .. } => {
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(item))),
                            );
                            return;
                        }
                    }
                    MemberResolution::Field { .. } | MemberResolution::Variant { .. } => {
                        // Not a callable — fall through to null placeholder
                    }
                }
            }
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
                } else if let Some(cap_idx) = self
                    .capture_indices
                    .as_ref()
                    .and_then(|m| m.get(&local_name))
                    .copied()
                {
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
                if self.expr_types.contains_key(&(self.current_scope, expr_id)) {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                } else {
                    let msg = format!("unresolved name: {name}");
                    self.emit_panic_call(&msg, expr_id);
                }
            }
        }
    }

    fn lower_item_ref(&mut self, expr_id: AstExprId, def: Definition<'db>, dest: Place) {
        let item = def_to_item_ref(self.db, def);
        // Check if this expression's type is EnumVariant
        if let Some(Tir2Ty::EnumVariant(_qtn, variant, _)) = self
            .expr_types
            .get(&(self.current_scope, expr_id))
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

    /// Lower `obj?.field` — null-check obj, then access field or produce null.
    fn lower_optional_field_access(
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
            self.lower_field_access(expr_id, base, field, dest);
            // Don't create our own join — the OptionalChain handler does that
        } else {
            // Standalone (no wrapping OptionalChain) — fall back to own null/join blocks
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_field_access(expr_id, base, field, dest.clone());
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
    fn lower_call(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        dest: Place,
    ) {
        // Check if callee is a field access (potential watch method call)
        let callee_expr = self.body.exprs[callee].clone();
        if let AstExpr::FieldAccess { base, field } = &callee_expr {
            let field_name = field.clone();
            let base_id = *base;
            if field_name.as_str() == "options" || field_name.as_str() == "notify" {
                let args_owned = args.to_vec();
                self.lower_watch_method(expr_id, base_id, &field_name, &args_owned, dest);
                return;
            }
        }

        // Check if callee is a method call (FieldAccess with a MemberResolution::Method/Free).
        // Field and Variant resolutions are not callable — treat them like unresolved accesses.
        // If the base is a real value (not a package namespace), prepend it as self.
        let (callee_operand, arg_operands) = if let AstExpr::FieldAccess { base, .. } = &callee_expr
        {
            if self
                .resolutions
                .get(&(self.current_scope, callee))
                .is_some_and(|r| {
                    use baml_compiler2_tir::inference::MemberResolution;
                    matches!(
                        r,
                        MemberResolution::Method { .. }
                            | MemberResolution::FreeFunction { .. }
                            | MemberResolution::FreeLet { .. }
                    )
                })
            {
                // Check if base is a value receiver or a package path.
                // Package paths have Unknown type in TIR (baml, baml.Array, etc.)
                let base_is_value = self
                    .expr_types
                    .get(&(self.current_scope, *base))
                    .map(|ty| !matches!(ty, Tir2Ty::Unknown { .. }))
                    .unwrap_or(false);
                // Check if the resolved method expects a `self` receiver.
                // Static methods (e.g. StreamCache.new) have no `self` param
                // and must not get the class reference prepended as an argument.
                let method_takes_self = {
                    use baml_compiler2_tir::inference::MemberResolution;
                    self.resolutions
                        .get(&(self.current_scope, callee))
                        .is_some_and(|r| match r {
                            MemberResolution::Method { func_loc, .. }
                            | MemberResolution::FreeFunction { func_loc } => {
                                let sig =
                                    baml_compiler2_ppir::function_signature(self.db, *func_loc);
                                sig.params
                                    .first()
                                    .is_some_and(|(name, _)| name.as_str() == "self")
                            }
                            _ => false,
                        })
                };
                if base_is_value && method_takes_self {
                    // Instance method call: arr.length() — prepend receiver as self
                    let receiver_op = self.lower_to_operand(*base);
                    let callee_op = self.lower_to_operand(callee);
                    let mut all_args = vec![receiver_op];
                    all_args.extend(args.iter().map(|&a| self.lower_to_operand(a)));
                    (callee_op, all_args)
                } else {
                    // Package function reference: baml.Array.length(array) — no self prepend
                    let callee_op = self.lower_to_operand(callee);
                    let arg_ops: Vec<Operand> =
                        args.iter().map(|&a| self.lower_to_operand(a)).collect();
                    (callee_op, arg_ops)
                }
            } else {
                let callee_op = self.lower_to_operand(callee);
                let arg_ops: Vec<Operand> =
                    args.iter().map(|&a| self.lower_to_operand(a)).collect();
                (callee_op, arg_ops)
            }
        } else {
            let callee_op = self.lower_to_operand(callee);
            let arg_ops: Vec<Operand> = args.iter().map(|&a| self.lower_to_operand(a)).collect();
            (callee_op, arg_ops)
        };

        let target = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);

        // Check if callee resolves to a builtin IO function (sys-op)
        let is_sys_op = self.check_sys_op(callee);

        if is_sys_op {
            // dest must be a local place for Await
            let dest_local = match dest {
                Place::Local(l) => l,
                _ => self.builder.temp(Ty::Null {
                    attr: TyAttr::default(),
                }),
            };
            let result_ty = self.builder.local_ty(dest_local);
            let future_ty = Ty::Future(Box::new(result_ty), TyAttr::default());
            let future_local = self.builder.temp(future_ty);
            let future_place = Place::Local(future_local);
            let resume = self.builder.create_block();
            self.builder.dispatch_future(
                callee_operand,
                arg_operands,
                future_place.clone(),
                resume,
            );
            self.builder.set_current_block(resume);
            let dest_place = Place::Local(dest_local);
            self.builder
                .await_(future_place, dest_place, target, unwind);
        } else {
            // Call destinations must be Place::Local in MIR. If `dest` is a
            // projection (Field/Index) or a capture, call into a temp local
            // first, then assign from the temp to the real destination.
            match &dest {
                Place::Local(_) => {
                    self.builder
                        .call(callee_operand, arg_operands, dest, target, unwind);
                }
                _ => {
                    let call_ty = self.expr_ty(expr_id);
                    let tmp = self.builder.temp(call_ty);
                    self.builder.call(
                        callee_operand,
                        arg_operands,
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
                // Multi-segment: check TIR resolution
                use baml_compiler2_tir::inference::MemberResolution;
                self.resolutions
                    .get(&(self.current_scope, callee))
                    .and_then(|res| match res {
                        MemberResolution::FreeFunction { func_loc } => Some(*func_loc),
                        MemberResolution::Method { func_loc, .. } => Some(*func_loc),
                        MemberResolution::FreeLet { .. }
                        | MemberResolution::Field { .. }
                        | MemberResolution::Variant { .. } => None,
                    })
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                    return true;
                }
            }
        }

        // ── NEW: FieldAccess callee (e.g. f.read, sock.recv) ──────────────────
        if let AstExpr::FieldAccess { .. } = &self.body.exprs[callee] {
            use baml_compiler2_tir::inference::MemberResolution;
            if let Some(resolution) = self.resolutions.get(&(self.current_scope, callee)) {
                let func_loc = match resolution {
                    MemberResolution::Method { func_loc, .. } => Some(*func_loc),
                    MemberResolution::FreeFunction { func_loc } => Some(*func_loc),
                    MemberResolution::FreeLet { .. }
                    | MemberResolution::Field { .. }
                    | MemberResolution::Variant { .. } => None,
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
}

// ─── 3.6: Helper methods ─────────────────────────────────────────────────────

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
        type_name: Option<&Name>,
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
            Ty::Class(tn, _) => Some(tn.clone()),
            _ => None,
        };
        let class_name = if let Some(n) = type_name {
            n.to_string()
        } else {
            type_name_key
                .as_ref()
                .map_or_else(String::new, |tn| tn.name.to_string())
        };

        if spreads.is_empty() {
            let field_operands: Vec<Operand> = fields
                .iter()
                .map(|(_, e)| self.lower_to_operand(*e))
                .collect();
            self.builder.assign(
                dest,
                Rvalue::Aggregate {
                    kind: AggregateKind::Class(class_name),
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
                    self.builder.assign(
                        dest,
                        Rvalue::Aggregate {
                            kind: AggregateKind::Class(class_name),
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

            self.builder.assign(
                dest,
                Rvalue::Aggregate {
                    kind: AggregateKind::Class(class_name),
                    fields: result,
                },
            );
        }
    }

    fn lower_field_access(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        field: &Name,
        dest: Place,
    ) {
        // Check if TIR resolved this to a method or free function — if so, emit a function constant.
        // Field and Variant resolutions fall through to the existing lowering paths below.
        if let Some(resolution) = self
            .resolutions
            .get(&(self.current_scope, expr_id))
            .cloned()
        {
            use baml_compiler2_tir::inference::MemberResolution;
            match &resolution {
                MemberResolution::Method { .. }
                | MemberResolution::FreeFunction { .. }
                | MemberResolution::FreeLet { .. } => {
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
            .get(&(self.current_scope, expr_id))
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
        if let Some(Tir2Ty::Unknown { .. }) = self.expr_types.get(&(self.current_scope, expr_id)) {
            let base_is_also_unknown = self
                .expr_types
                .get(&(self.current_scope, base))
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

        // Unwrap Optional — when called from lower_optional_field_access,
        // the base type is T? but we've already null-checked, so use the inner type.
        let unwrapped_ty = match &base_ty {
            Ty::Optional(inner, _) => inner.as_ref(),
            _ => &base_ty,
        };

        // Look up field index from class_fields
        let field_idx = if let Ty::Class(tn, _) = unwrapped_ty {
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
        } else {
            if let Ty::Class(tn, _) = unwrapped_ty {
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
            } => {
                // Extract binding name from pattern
                let pat = self.body.patterns[pattern].clone();
                let name = match &pat {
                    AstPattern::Binding(n) => n.clone(),
                    AstPattern::TypedBinding { name, .. } => name.clone(),
                    _ => Name::new("_"),
                };

                let local_ty = self.pat_ty(pattern);
                let local =
                    self.builder
                        .declare_local(Some(name.clone()), local_ty, None, is_watched);
                self.locals.insert(name, local);

                if is_watched {
                    self.watched_locals_stack.push(local);
                }

                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(local));
                } else {
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
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
                let watched_depth = self.watched_locals_stack.len();
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
                // Bind the pattern to the element local, inserting FreshCell
                // before the assignment so each iteration's closures capture a
                // distinct cell.
                {
                    let pat = self.body.patterns[binding].clone();
                    match pat {
                        AstPattern::Binding(name) if name.as_str() != "_" => {
                            let ty = self
                                .pat_types
                                .get(&(self.current_scope, binding))
                                .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
                                .unwrap_or_else(|| self.builder.local_ty(elem_local));
                            let local =
                                self.builder
                                    .declare_local(Some(name.clone()), ty, None, false);
                            self.builder.fresh_cell(local);
                            self.builder.assign(
                                Place::local(local),
                                Rvalue::Use(Operand::Copy(Place::Local(elem_local))),
                            );
                            self.locals.insert(name, local);
                        }
                        AstPattern::TypedBinding { name, ty, .. } if name.as_str() != "_" => {
                            let resolved_ty = self.resolve_type_annotation(&ty);
                            let local = self.builder.declare_local(
                                Some(name.clone()),
                                resolved_ty,
                                None,
                                false,
                            );
                            self.builder.fresh_cell(local);
                            self.builder.assign(
                                Place::local(local),
                                Rvalue::Use(Operand::Copy(Place::Local(elem_local))),
                            );
                            self.locals.insert(name, local);
                        }
                        _ => {}
                    }
                }

                // Lower the body expression (result discarded)
                let body_temp = self.builder.temp(Ty::Void {
                    attr: TyAttr::default(),
                });
                self.lower_expr(body, Place::local(body_temp));

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_after);
                }

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
                // Unwatch all watched locals before returning
                let watched = self.watched_locals_stack.clone();
                for &local in watched.iter().rev() {
                    self.builder.unwatch(local);
                }
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
                self.builder.throw(val_op);
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Break => {
                if let Some(ref loop_ctx) = self.loop_context {
                    let target = loop_ctx.break_target;
                    let depth = loop_ctx.watched_locals_depth;
                    let watched: Vec<Local> = self.watched_locals_stack[depth..].to_vec();
                    for &local in watched.iter().rev() {
                        self.builder.unwatch(local);
                    }
                    self.builder.goto(target);
                }
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Continue => {
                if let Some(ref loop_ctx) = self.loop_context {
                    let target = loop_ctx.continue_target;
                    let depth = loop_ctx.watched_locals_depth;
                    let watched: Vec<Local> = self.watched_locals_stack[depth..].to_vec();
                    for &local in watched.iter().rev() {
                        self.builder.unwatch(local);
                    }
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
                } else if let Some(cap_idx) = self
                    .capture_indices
                    .as_ref()
                    .and_then(|m| m.get(&segments[0]))
                    .copied()
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
            AstExpr::FieldAccess { base, field } => {
                let base_id = *base;
                let field_name = field.clone();
                let base_place = self.lower_lvalue(base_id);
                let base_ty = self.expr_ty(base_id);
                if let Ty::Class(ref tn, _) = base_ty {
                    if let Some(fields) = self.class_fields.get(tn) {
                        if let Some(&idx) = fields.get(field_name.as_str()) {
                            return Place::Field {
                                base: Box::new(base_place),
                                field: idx,
                            };
                        }
                    }
                    self.emit_panic_call(
                        &format!(
                            "internal compiler error: MIR failed to resolve field access \
                             .{} against class definition '{}' (module_path: {:?}). \
                             This class should be in class_fields but isn't.",
                            field_name, tn.name, tn.module_path,
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
                    Rvalue::Use(Operand::Constant(Constant::String(field_name.to_string()))),
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
            AstExpr::OptionalFieldAccess { base, field } => {
                let base_id = *base;
                let field_name = field.clone();

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
                    .expect("OptionalFieldAccess in lvalue must be inside OptionalChain");
                self.builder.branch(
                    Operand::Copy(Place::Local(test_local)),
                    bb_null,
                    bb_continue,
                );

                self.builder.set_current_block(bb_continue);

                // Project field from the same temp local — no second evaluation
                let base_place = Place::Local(base_local);
                // Unwrap Optional — we've already null-checked, so use the inner type.
                let unwrapped_ty = match &base_ty {
                    Ty::Optional(inner, _) => inner.as_ref(),
                    _ => &base_ty,
                };
                if let Ty::Class(tn, _) = unwrapped_ty {
                    if let Some(fields) = self.class_fields.get(tn) {
                        if let Some(&idx) = fields.get(field_name.as_str()) {
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
                    Rvalue::Use(Operand::Constant(Constant::String(field_name.to_string()))),
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
            .contains(&(self.current_scope, expr_id));

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

        use baml_base::Literal;

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
            let pat = &self.body.patterns[pattern];
            match pat {
                AstPattern::Literal(Literal::Int(val)) => {
                    // Integer arm: verify kind consistency
                    match &switch_kind {
                        None => switch_kind = Some(SwitchKind::Integer),
                        Some(SwitchKind::Integer) => {}
                        Some(_) => return false, // Mixed int + enum
                    }
                    let v = *val;
                    if seen_values.insert(v) {
                        int_arms.push((v, i));
                    }
                }
                AstPattern::EnumVariant { enum_name, variant } => {
                    // Enum variant arm: verify enum consistency
                    let enum_name = enum_name.clone();
                    let variant = variant.clone();
                    match &switch_kind {
                        None => switch_kind = Some(SwitchKind::EnumDiscriminant(enum_name.clone())),
                        Some(SwitchKind::EnumDiscriminant(n)) if *n == enum_name => {}
                        _ => return false, // Different enum or mixed with int
                    }
                    // Look up variant index
                    let idx = self
                        .enum_variants
                        .get(&enum_name)
                        .and_then(|m| m.get(variant.as_str()))
                        .copied();
                    let Some(idx) = idx else { return false };
                    let disc = i64::try_from(idx).expect("discriminant overflow");
                    if seen_values.insert(disc) {
                        int_arms.push((disc, i));
                    }
                }
                AstPattern::Union(sub_pats) => {
                    // Union pattern: each sub-pattern maps to the same arm body.
                    // All sub-patterns must be the same kind (int, enum variant, or type tag).
                    for sub_pat_id in sub_pats {
                        let sub_pat = &self.body.patterns[*sub_pat_id];
                        match sub_pat {
                            AstPattern::Literal(Literal::Int(val)) => {
                                match &switch_kind {
                                    None => switch_kind = Some(SwitchKind::Integer),
                                    Some(SwitchKind::Integer) => {}
                                    Some(_) => return false,
                                }
                                let v = *val;
                                if seen_values.insert(v) {
                                    int_arms.push((v, i));
                                }
                            }
                            AstPattern::EnumVariant { enum_name, variant } => {
                                let enum_name = enum_name.clone();
                                let variant = variant.clone();
                                match &switch_kind {
                                    None => {
                                        switch_kind =
                                            Some(SwitchKind::EnumDiscriminant(enum_name.clone()));
                                    }
                                    Some(SwitchKind::EnumDiscriminant(n)) if *n == enum_name => {}
                                    _ => return false,
                                }
                                let idx = self
                                    .enum_variants
                                    .get(&enum_name)
                                    .and_then(|m| m.get(variant.as_str()))
                                    .copied();
                                let Some(idx) = idx else { return false };
                                let disc = i64::try_from(idx).expect("discriminant overflow");
                                if seen_values.insert(disc) {
                                    int_arms.push((disc, i));
                                }
                            }
                            AstPattern::Binding(_) | AstPattern::TypedBinding { .. } => {
                                // Type-pattern sub-arm: look up type tag via TIR.
                                match &switch_kind {
                                    None => switch_kind = Some(SwitchKind::TypeTag),
                                    Some(SwitchKind::TypeTag) => {}
                                    Some(_) => return false,
                                }
                                match self.classify_pattern_type_tag(*sub_pat_id) {
                                    Some(tags) => {
                                        for tag in tags {
                                            if seen_values.insert(tag) {
                                                int_arms.push((tag, i));
                                            }
                                        }
                                    }
                                    None => return false,
                                }
                            }
                            _ => return false,
                        }
                    }
                }
                AstPattern::TypedBinding { .. } => {
                    // Type-annotated binding → classify as TypeTag
                    match &switch_kind {
                        None => switch_kind = Some(SwitchKind::TypeTag),
                        Some(SwitchKind::TypeTag) => {}
                        Some(_) => return false, // Mixed TypeTag + Integer/Enum
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
                }
                AstPattern::Binding(name) if name.as_str() != "_" => {
                    // Bare name resolved to a type by TIR (e.g. `DivisionByZero =>`)
                    if self.pat_types.contains_key(&(self.current_scope, pattern)) {
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
                    } else {
                        // Regular binding without type info — wildcard-like
                        if i != arms.len() - 1 {
                            return false;
                        }
                        otherwise_idx = Some(i);
                    }
                }
                AstPattern::Binding(_) => {
                    // Wildcard `_` — must be last arm
                    if i != arms.len() - 1 {
                        return false;
                    }
                    otherwise_idx = Some(i);
                }
                AstPattern::Null | AstPattern::Literal(_) => return false,
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
                self.bind_pattern(scrutinee, pattern);
                self.lower_expr(body, dest.clone());
                if !self.builder.is_current_terminated() {
                    self.builder.goto(join);
                }
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
            self.bind_pattern(scrutinee, pattern);
            self.lower_expr(body, dest);
            if !self.builder.is_current_terminated() {
                self.builder.goto(join);
            }
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

        // Exhaustive last arm: skip the pattern test — it must match.
        if exhaustive && rest.is_empty() && arm.guard.is_none() {
            self.bind_pattern(scrutinee, arm.pattern);
            self.lower_expr(arm.body, dest);
            if !self.builder.is_current_terminated() {
                self.builder.goto(join);
            }
            return;
        }

        let bb_body = self.builder.create_block();
        let bb_next = self.builder.create_block();

        self.lower_pattern_test(scrutinee, arm.pattern, bb_body, bb_next);

        self.builder.set_current_block(bb_body);
        self.bind_pattern(scrutinee, arm.pattern);
        if let Some(guard) = arm.guard {
            let guard_op = self.lower_to_operand(guard);
            let bb_guarded = self.builder.create_block();
            self.builder.branch(guard_op, bb_guarded, bb_next);
            self.builder.set_current_block(bb_guarded);
        }
        self.lower_expr(arm.body, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(join);
        }

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
            let test = Rvalue::IsType {
                operand: Operand::Copy(Place::Local(scrutinee)),
                ty,
            };
            let test_local = self.builder.temp(Ty::Bool {
                attr: TyAttr::default(),
            });
            self.builder.assign(Place::local(test_local), test);
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), success, failure);
        }
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
            Ty::Class(tn, _) => self.class_type_tags.get(tn).copied(),
            _ => None,
        }
    }

    fn lower_pattern_test(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        success: BlockId,
        failure: BlockId,
    ) {
        let pat = self.body.patterns[pat_id].clone();
        match pat {
            AstPattern::Binding(ref name) => {
                // Bare type sugar: the TIR resolved this binding name to a
                // type (e.g. `DivisionByZero =>` resolves to a class).
                // Generate an IsType test instead of an unconditional match.
                if name.as_str() != "_" {
                    if let Some(tir_ty) = self.pat_types.get(&(self.current_scope, pat_id)).cloned()
                    {
                        let resolved = self.resolved_aliases.convert(&tir_ty);
                        self.emit_is_type_branch(scrutinee, resolved, success, failure);
                        return;
                    }
                }
                // Regular binding — always matches
                self.builder.goto(success);
            }
            AstPattern::TypedBinding { .. } => {
                // TIR always resolves TypedBinding patterns via record_pattern_test_types.
                let annotation_ty = self
                    .pat_types
                    .get(&(self.current_scope, pat_id))
                    .map(|tir_ty| convert_tir2_ty(tir_ty, &self.resolved_aliases))
                    .expect("TIR must resolve TypedBinding patterns");
                self.emit_is_type_branch(scrutinee, annotation_ty, success, failure);
            }
            AstPattern::Literal(lit) => {
                let constant = Self::lower_literal(&lit);
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
            AstPattern::Null => {
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
            AstPattern::EnumVariant { enum_name, variant } => {
                // Resolve the enum's package from TIR type info when available,
                // otherwise fall back to the current file's package.
                let enum_ref = if let Some(Tir2Ty::EnumVariant(qtn, _, _)) =
                    self.pat_types.get(&(self.current_scope, pat_id))
                {
                    ItemRef::EnumType {
                        package: qtn.package().clone(),
                        namespace: qtn.namespace().clone(),
                        name: qtn.name().clone(),
                    }
                } else {
                    let pkg_info = file_package(self.db, self.file);
                    ItemRef::EnumType {
                        package: pkg_info.package.clone(),
                        namespace: pkg_info.namespace_path,
                        name: enum_name,
                    }
                };
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
            AstPattern::Union(sub_pats) => {
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
        }
    }

    fn bind_pattern(&mut self, scrutinee: Local, pat_id: AstPatId) {
        let pat = self.body.patterns[pat_id].clone();
        match pat {
            AstPattern::Binding(name) if name.as_str() != "_" => {
                // Prefer TIR-inferred type; fall back to the scrutinee's declared type
                // rather than Null, so catch bindings get the error's type (unknown) not null.
                let ty = self
                    .pat_types
                    .get(&(self.current_scope, pat_id))
                    .map(|ty| convert_tir2_ty(ty, &self.resolved_aliases))
                    .unwrap_or_else(|| self.builder.local_ty(scrutinee));
                let local = self
                    .builder
                    .declare_local(Some(name.clone()), ty, None, false);
                self.builder.assign(
                    Place::local(local),
                    Rvalue::Use(Operand::Copy(Place::Local(scrutinee))),
                );
                self.locals.insert(name, local);
            }
            AstPattern::TypedBinding { name, ty, .. } if name.as_str() != "_" => {
                let resolved_ty = self.resolve_type_annotation(&ty);
                let local =
                    self.builder
                        .declare_local(Some(name.clone()), resolved_ty, None, false);
                self.builder.assign(
                    Place::local(local),
                    Rvalue::Use(Operand::Copy(Place::Local(scrutinee))),
                );
                self.locals.insert(name, local);
            }
            _ => {
                // Wildcard `_`, Literal, Null, EnumVariant, Union — no binding needed
            }
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
        match pat {
            AstPattern::Binding(name) if name.as_str() == "_" => {
                // Pure wildcard — handled separately, not a type test
                None
            }
            AstPattern::Binding(_) => {
                // Named binding resolved by TIR to a type
                let tir_ty = self.pat_types.get(&(self.current_scope, pat_id))?;
                let resolved = self.resolved_aliases.convert(tir_ty);
                self.ty_to_type_tags(&resolved)
            }
            AstPattern::TypedBinding { .. } => {
                // Type-annotated binding — always resolved by TIR
                let tir_ty = self.pat_types.get(&(self.current_scope, pat_id))?;
                let resolved = self.resolved_aliases.convert(tir_ty);
                self.ty_to_type_tags(&resolved)
            }
            // Literal, Null, EnumVariant, Union — not type-tag eligible
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

        let bb_join = self.builder.create_block();
        let bb_handler = self.builder.create_block();

        // Use the user-provided binding name (e.g. `e` from `catch (e)`) so it
        // shows up in bytecode instead of an anonymous `_N` temp.
        let binding_name = clauses
            .first()
            .and_then(|c| match &self.body.patterns[c.binding] {
                AstPattern::Binding(name) if name.as_str() != "_" => Some(name.clone()),
                _ => None,
            });
        let error_local = self.builder.declare_local(
            binding_name.clone(),
            Ty::BuiltinUnknown {
                attr: TyAttr::default(),
            },
            None,
            false,
        );
        if let Some(name) = binding_name {
            self.locals.insert(name, error_local);
        }

        // Declare stack trace local if the catch clause has a second binding.
        let stack_trace_local = clauses.first().and_then(|c| {
            c.stack_trace_binding.map(|st_pat| {
                let st_name = match &self.body.patterns[st_pat] {
                    AstPattern::Binding(name) if name.as_str() != "_" => Some(name.clone()),
                    _ => None,
                };
                let local = self.builder.declare_local(
                    st_name.clone(),
                    Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    },
                    None,
                    false,
                );
                if let Some(name) = st_name {
                    self.locals.insert(name, local);
                }
                local
            })
        });

        // Flatten all arms from all clauses (blocks created lazily below).
        let mut arms: Vec<(baml_compiler2_ast::CatchArm, bool)> = Vec::new();
        for clause in clauses {
            for &arm_id in &clause.arms {
                let arm = self.body.catch_arms[arm_id].clone();
                let is_wildcard = matches!(
                    self.body.patterns[arm.pattern],
                    AstPattern::Binding(ref name) if name.as_str() == "_"
                );
                arms.push((arm, is_wildcard));
            }
        }

        let has_wildcard = arms.iter().any(|(_, is_wc)| *is_wc);
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
            .map(|(arm, _)| (arm.pattern, arm.body, None))
            .collect();
        self.builder.set_current_block(bb_handler);
        if self.try_lower_as_switch(
            error_local,
            &switch_arms,
            dest.clone(),
            bb_join,
            SwitchOtherwise::Catch {
                error_local,
                needs_throw_if_panic,
            },
            None,
        ) {
            self.builder.set_current_block(bb_join);
            return;
        }

        // Fallback: sequential pattern-test chain.
        // Create body blocks now (not created earlier so the switch path
        // doesn't leave orphaned unterminated blocks).
        let arms_with_blocks: Vec<_> = arms
            .iter()
            .map(|(arm, is_wc)| (arm.clone(), self.builder.create_block(), *is_wc))
            .collect();

        for &(ref arm, body_block, is_wildcard) in &arms_with_blocks {
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
        for &(ref arm, body_block, _) in &arms_with_blocks {
            self.builder.set_current_block(body_block);
            self.bind_pattern(error_local, arm.pattern);
            self.lower_expr(arm.body, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }
        }

        self.builder.set_current_block(bb_join);
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
        FunctionBody::Builtin(kind) => MirFunction {
            arity,
            span: None,
            item_ref,
            kind: MirFunctionKind::Builtin(*kind),
            lambdas: vec![],
        },
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
