//! Typed Intermediate Representation (TIR).
//!
//! Provides type checking and inference for BAML.
//!
//! # Architecture
//!
//! The TIR layer performs bidirectional type checking:
//! - **Inference (synthesize)**: Compute the type of an expression from its structure
//! - **Checking**: Verify an expression has an expected type
//!
//! This follows patterns from rust-analyzer and ruff for incremental type checking.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use baml_base::{FileId, Name, Span, TyAttr};
use baml_compiler_diagnostics::TypeError;
use baml_compiler_hir::{
    ErrorLocation, ExprBody, ExprId, FunctionBody, FunctionLoc, FunctionSignature, HirSourceMap,
    MatchArmId, PatId, Pattern, PromptTemplate, SignatureSourceMap, StmtId, TirContext, TypeId,
};
use baml_workspace::Project;

/// Type alias for TIR type errors.
///
/// Uses `TirContext<Ty>` which has:
/// - `Ty` as the type representation
/// - `ErrorLocation` as the location (position-independent IDs)
///
/// This enables Salsa caching to work correctly - whitespace changes don't
/// invalidate type inference results because locations use IDs instead of spans.
pub type TirTypeError = TypeError<TirContext<Ty>>;

pub mod builtins;
mod cycles;
mod divergence;
mod exhaustiveness;
pub mod jinja;
mod lower;
mod narrowing;
mod normalize;
pub mod pretty;
mod resolve;
pub mod throw_inference;
mod types;

// Re-export HIR types that are part of TIR's public API (used in Ty variants).
pub use baml_base::{Namespace, QualifiedName};
// Crate-internal imports (used within this file but not re-exported)
use builtins::Bindings;
pub use cycles::{validate_class_cycles, validate_type_alias_cycles};
use exhaustiveness::ExhaustivenessChecker;
use lower::lower_type_ref;
use narrowing::{
    extract_condition_narrowing, extract_early_return_narrowing, infer_union_member_field,
};
pub use normalize::find_recursive_aliases;
pub use pretty::render_function_tree;
use resolve::ResolutionMap;
pub use resolve::{ResolvedMethod, ResolvedValue};
use text_size::TextRange;
pub use types::*;

/// Shorthand for `TyAttr::default()` to reduce verbosity.
fn d() -> TyAttr {
    TyAttr::default()
}

/// Substitute type variable bindings into a `TypePattern`, falling back to `Ty::Unknown`
/// for unbound type variables.
///
/// This is used for builtin function type inference where some type variables may be
/// bound from arguments but others might not be.
fn substitute_with_fallback(pattern: &baml_builtins::TypePattern, bindings: &Bindings) -> Ty {
    use baml_builtins::TypePattern;
    match pattern {
        TypePattern::Var(name) => bindings
            .get(name)
            .cloned()
            .unwrap_or(Ty::Unknown { attr: d() }),
        TypePattern::Int => Ty::Int { attr: d() },
        TypePattern::Float => Ty::Float { attr: d() },
        TypePattern::String => Ty::String { attr: d() },
        TypePattern::Bool => Ty::Bool { attr: d() },
        TypePattern::Null => Ty::Null { attr: d() },
        TypePattern::Array(elem) => {
            Ty::List(Box::new(substitute_with_fallback(elem, bindings)), d())
        }
        TypePattern::Map { key, value } => Ty::Map {
            key: Box::new(substitute_with_fallback(key, bindings)),
            value: Box::new(substitute_with_fallback(value, bindings)),
            attr: d(),
        },
        TypePattern::Media => Ty::Media(baml_base::MediaKind::Generic, d()),
        TypePattern::Optional(inner) => {
            Ty::Optional(Box::new(substitute_with_fallback(inner, bindings)), d())
        }
        TypePattern::Builtin(path) => Ty::Class(builtins::parse_builtin_path(path), d()),
        TypePattern::Function { params, ret } => Ty::Function {
            params: params
                .iter()
                .map(|p| (None, substitute_with_fallback(p, bindings)))
                .collect(),
            ret: Box::new(substitute_with_fallback(ret, bindings)),
            attr: d(),
        },
        TypePattern::Resource => Ty::Resource { attr: d() },
        TypePattern::BuiltinUnknown => Ty::BuiltinUnknown { attr: d() },
        TypePattern::Enum(path) => Ty::Enum(builtins::parse_builtin_path(path), d()),
        TypePattern::Type => Ty::Type { attr: d() },
    }
}

/// Check if a `QualifiedName` refers to a builtin type with a dedicated VM heap variant
/// (i.e., `Object::PromptAst` rather than `Object::Instance`).
///
/// Used by `baml_type::convert` to decide whether `Ty::Class(fqn)` should become
/// `baml_type::Ty::PromptAst` instead of `baml_type::Ty::Class(TypeName)`.
pub fn is_prompt_ast_class(fqn: &QualifiedName) -> bool {
    baml_builtins::find_builtin_type(&fqn.display())
        .is_some_and(|td| matches!(td.runtime_kind, baml_builtins::RuntimeKind::PromptAst))
}

// ============================================================================
// Path Resolution
// ============================================================================

/// Resolved path categories after name resolution.
///
/// When we encounter a multi-segment path like `user.name.length` or `Status.Active`,
/// we need to determine what it actually refers to. This enum captures the different
/// possibilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPath {
    /// Local variable: `user` or `user.name.length`
    /// The name is the local variable. Field access types are in `path_segment_types`.
    Local { name: Name },

    /// Enum variant: `Status.Active`
    /// First segment is the enum type, second is the variant name.
    EnumVariant { enum_name: Name, variant_name: Name },

    /// Module item: `baml.HttpMethod.Get`
    /// The path navigates through modules to reach an item.
    ModuleItem {
        module_path: Vec<Name>,
        item_name: Name,
    },

    /// Function reference: `MyFunction`
    /// A single-segment path that resolves to a function.
    Function { name: Name },

    /// Method call on a type: `image.from_url`
    /// Used when the receiver is a type with associated methods.
    Method {
        receiver_type: Name,
        method_name: Name,
    },

    /// Unknown/unresolved path
    Unknown,
}

//
// ──────────────────────────────────────────────────────────── DATABASE ─────
//

/// Database trait for TIR queries.
///
/// Extends `baml_compiler_hir::Db`. Use the free functions in this crate
/// (e.g., `typing_context`, `class_field_types`) for TIR queries.
#[salsa::db]
pub trait Db: baml_compiler_hir::Db {}

// ============================================================================
// Tracked Struct for Enum Variants
// ============================================================================

/// Tracked struct holding enum variants (enum name -> variant names).
#[salsa::tracked]
pub struct EnumVariantsMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub enums: HashMap<Name, Vec<Name>>,
}

/// Tracked struct holding function types (function name -> function type).
///
/// Parameter names are stored in `Ty::Function` for Jinja template validation.
#[salsa::tracked]
pub struct TypingContextMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub functions: HashMap<Name, Ty>,
}

/// Tracked struct holding class field types (class name -> field name -> field type).
///
/// Also includes any type errors found during lowering (e.g., unknown types).
/// Following rust-analyzer's pattern of returning `(Data, Diagnostics)` from queries.
#[salsa::tracked]
pub struct ClassFieldTypesMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub classes: HashMap<Name, HashMap<Name, Ty>>,

    #[tracked]
    #[returns(ref)]
    pub errors: Vec<TirTypeError>,
}

/// Tracked struct holding type aliases (alias name -> resolved type).
///
/// Also includes any type errors found during lowering (e.g., unknown types).
#[salsa::tracked]
pub struct TypeAliasesMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub aliases: HashMap<Name, Ty>,

    #[tracked]
    #[returns(ref)]
    pub errors: Vec<TirTypeError>,
}

/// Tracked struct holding class names mapped to their qualified names.
#[salsa::tracked]
pub struct ClassNamesSet<'db> {
    #[tracked]
    #[returns(ref)]
    pub names: HashMap<Name, baml_compiler_hir::QualifiedName>,
}

/// Tracked struct holding enum names mapped to their qualified names.
#[salsa::tracked]
pub struct EnumNamesSet<'db> {
    #[tracked]
    #[returns(ref)]
    pub names: HashMap<Name, baml_compiler_hir::QualifiedName>,
}

/// Tracked struct holding type alias names.
#[salsa::tracked]
pub struct TypeAliasNamesSet<'db> {
    #[tracked]
    #[returns(ref)]
    pub names: HashSet<Name>,
}

/// Tracked set of functions that always diverge.
#[salsa::tracked]
pub struct FunctionDivergenceSet<'db> {
    #[tracked]
    #[returns(ref)]
    pub functions: HashSet<Name>,
}

/// Tracked map of transitive throw sets per function.
#[salsa::tracked]
pub struct FunctionThrowSets<'db> {
    #[tracked]
    #[returns(ref)]
    pub transitive: HashMap<Name, BTreeSet<String>>,
}

// ============================================================================
// TIR Queries
// ============================================================================

/// Query: Get enum variants for a project.
///
/// Maps enum names to their variant names, e.g.:
/// `Status` -> `[Active, Inactive, Pending]`
#[salsa::tracked]
pub fn enum_variants(db: &dyn Db, project: Project) -> EnumVariantsMap<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let mut enums = HashMap::new();

    // Add builtin enum variants (keyed by FQN)
    for builtin_enum in baml_builtins::builtin_enums() {
        let variants: Vec<Name> = builtin_enum
            .variants
            .iter()
            .map(|v| Name::new(*v))
            .collect();
        enums.insert(Name::new(builtin_enum.path), variants);
    }

    // Add user-defined enum variants
    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Enum(enum_loc) = item {
            let file = enum_loc.file(db);
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let enum_data = &item_tree[enum_loc.id(db)];

            let variants: Vec<Name> = enum_data.variants.iter().map(|v| v.name.clone()).collect();
            enums.insert(enum_data.name.clone(), variants);
        }
    }

    EnumVariantsMap::new(db, enums)
}

// ============================================================================
// Salsa Tracked Queries
// ============================================================================
//
// These queries compute type-related data and are cached by Salsa.

/// Query: Get the typing context for a project.
///
/// Maps function names and template string names to their arrow types.
/// Parameter names are stored in `Ty::Function` for Jinja template validation.
#[salsa::tracked]
pub fn typing_context(db: &dyn Db, project: Project) -> TypingContextMap<'_> {
    let resolution_ctx = TypeResolutionContext::new(db, project);
    let mut context = HashMap::new();

    for file in project.files(db) {
        let items_struct = baml_compiler_hir::file_items(db, *file);
        let items = items_struct.items(db);

        for item in items {
            match item {
                baml_compiler_hir::ItemId::Function(func_loc) => {
                    let hir_signature = baml_compiler_hir::function_signature(db, *func_loc);
                    let qualified_name = baml_compiler_hir::function_qualified_name(db, *func_loc);

                    // We don't care about the span here because any errors produced by
                    // this lowering will be immediately discarded. `infer_function` will
                    // lower the same types, and in that context, there are error locations
                    // to send, and those are the `lower_type_ref` calls whose errors we
                    // would surface to the user.
                    let span = Span::default();

                    let params: Vec<(Option<Name>, Ty)> = hir_signature
                        .params
                        .iter()
                        .map(|p| {
                            let ty = resolution_ctx.lower_type_ref(&p.type_ref, span).0;
                            (Some(p.name.clone()), ty)
                        })
                        .collect();

                    let return_type = resolution_ctx
                        .lower_type_ref(&hir_signature.return_type, span)
                        .0;

                    let func_type = Ty::Function {
                        params,
                        ret: Box::new(return_type),
                        attr: d(),
                    };

                    // Use the qualified display name so builtin BAML functions are only
                    // callable via their namespace (e.g., "baml.llm.render_prompt").
                    let func_name = qualified_name.display_name();
                    context.insert(func_name, func_type);
                }

                baml_compiler_hir::ItemId::TemplateString(ts_loc) => {
                    let hir_signature = baml_compiler_hir::template_string_signature(db, *ts_loc);
                    let span = Span::default();

                    let params: Vec<(Option<Name>, Ty)> = hir_signature
                        .params
                        .iter()
                        .map(|p| {
                            let ty = resolution_ctx.lower_type_ref(&p.type_ref, span).0;
                            (Some(p.name.clone()), ty)
                        })
                        .collect();

                    // Template strings always return String
                    let return_type = Ty::String { attr: d() };

                    let func_type = Ty::Function {
                        params,
                        ret: Box::new(return_type),
                        attr: d(),
                    };

                    let ts_name = hir_signature.name.clone();
                    context.insert(ts_name, func_type);
                }
                _ => {}
            }
        }
    }

    TypingContextMap::new(db, context /* functions */)
}

/// Query: functions that always diverge.
///
/// Uses a monotonic fixed-point over call dependencies. This is cycle-safe:
/// mutually-recursive functions only become divergent when at least one member
/// has a direct divergence seed (throw/return/break/continue or a divergent tail).
#[salsa::tracked]
pub fn function_divergence_set(db: &dyn Db, project: Project) -> FunctionDivergenceSet<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let mut function_locs: HashMap<Name, FunctionLoc<'_>> = HashMap::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Function(func_loc) = item {
            let function_name =
                baml_compiler_hir::function_qualified_name(db, *func_loc).display_name();
            function_locs.insert(function_name, *func_loc);
        }
    }

    let functions = divergence::solve_divergence_fixed_point(
        function_locs.keys().cloned(),
        |function_name, known_diverging| {
            let Some(function_loc) = function_locs.get(function_name) else {
                return false;
            };
            let body = baml_compiler_hir::function_body(db, *function_loc);
            let baml_compiler_hir::FunctionBody::Expr(expr_body, _) = body.as_ref() else {
                return false;
            };
            let Some(root_expr) = expr_body.root_expr else {
                return false;
            };
            divergence::expr_never_returns(root_expr, expr_body, &|callee| {
                known_diverging.contains(callee)
            })
        },
    );

    FunctionDivergenceSet::new(db, functions)
}

/// Query: transitive throw types per function.
///
/// Uses `AnalysisGraph` (BEP-007) to propagate direct throw facts transitively
/// over call edges with SCC-safe fixed-point semantics.
///
/// When a function has a `throws` declaration, the declared facts are used
/// as its caller-visible throw set (modular checking). Without a declaration,
/// body-derived facts propagate transitively as before.
#[salsa::tracked]
pub fn function_throw_sets(db: &dyn Db, project: Project) -> FunctionThrowSets<'_> {
    use throw_inference::{ThrowAnalysisInput, analyze_throws};

    let items = baml_compiler_hir::project_items(db, project);
    let enum_variants_map = enum_variants(db, project).enums(db).clone();
    let enum_name_set = enum_names(db, project).names(db).clone();
    let type_alias_refs = collect_type_alias_refs(db, project);

    let mut entries: Vec<(Name, Arc<FunctionBody>, Option<BTreeSet<String>>)> = Vec::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Function(func_loc) = item {
            let function_name =
                baml_compiler_hir::function_qualified_name(db, *func_loc).display_name();
            let body = baml_compiler_hir::function_body(db, *func_loc);

            let sig = baml_compiler_hir::function_signature(db, *func_loc);
            let declared_throws = sig.throws.as_ref().map(|type_ref| {
                lower_throws_to_facts(
                    type_ref,
                    &enum_variants_map,
                    &enum_name_set,
                    &type_alias_refs,
                )
            });

            entries.push((function_name, body, declared_throws));
        }
    }

    let inputs: Vec<ThrowAnalysisInput<'_>> = entries
        .iter()
        .map(|(name, body, declared)| ThrowAnalysisInput {
            name: name.clone(),
            body: match body.as_ref() {
                FunctionBody::Expr(expr_body, _) => Some(expr_body),
                _ => None,
            },
            declared_throws: declared.clone(),
        })
        .collect();

    let analysis = analyze_throws(&inputs);
    let mut transitive: HashMap<Name, BTreeSet<String>> = HashMap::new();

    for (name, _, _) in &entries {
        let throw_set = analysis.transitive(name).cloned().unwrap_or_default();
        transitive.insert(name.clone(), throw_set);
    }

    FunctionThrowSets::new(db, transitive)
}

/// Query: precise transitive throw types per function (post-TIR).
///
/// Runs `AnalysisGraph` a second time, seeded with TIR-precise facts instead
/// of HIR pre-pass facts. Functions are categorized:
///
/// 1. **`throws` declaration** → firewall (declared facts, no call edges)
/// 2. **Has catch in body** → TIR `effective_throws` is already precise
///    (catch subtraction applied locally), used as-is (no call edges)
/// 3. **No catches** → "own throws" (effective minus pre-pass callee
///    contributions) + call edges for precise callee propagation
///
/// Used for display (hover) — NOT for TIR inference itself.
#[salsa::tracked]
pub fn precise_function_throw_sets(db: &dyn Db, project: Project) -> FunctionThrowSets<'_> {
    use baml_compiler_analysis::AnalysisGraph;

    let items = baml_compiler_hir::project_items(db, project);
    let pre_pass = function_throw_sets(db, project);
    let enum_variants_map = enum_variants(db, project).enums(db).clone();
    let enum_name_set = enum_names(db, project).names(db).clone();
    let type_alias_refs = collect_type_alias_refs(db, project);

    let mut graph: AnalysisGraph<Name, String> = AnalysisGraph::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Function(func_loc) = item {
            let function_name =
                baml_compiler_hir::function_qualified_name(db, *func_loc).display_name();
            let body = baml_compiler_hir::function_body(db, *func_loc);
            let sig = baml_compiler_hir::function_signature(db, *func_loc);
            let inference = function_type_inference(db, *func_loc);

            if let Some(throws_ref) = &sig.throws {
                // Category 1: has `throws` declaration → firewall
                let declared = lower_throws_to_facts(
                    throws_ref,
                    &enum_variants_map,
                    &enum_name_set,
                    &type_alias_refs,
                );
                graph.add_node(function_name, declared);
            } else if body_has_catch(&body) {
                // Category 2: has catch → TIR effective_throws is precise
                graph.add_node(function_name, inference.effective_throws.clone());
            } else {
                // Category 3: no catches → own throws + call edges
                let callee_prepass = compute_callee_prepass_throws(&body, pre_pass, db);
                let own_throws: BTreeSet<String> = inference
                    .effective_throws
                    .difference(&callee_prepass)
                    .cloned()
                    .collect();
                graph.add_node(function_name.clone(), own_throws);

                if let FunctionBody::Expr(expr_body, _) = &*body {
                    for target in throw_inference::collect_call_targets(expr_body) {
                        graph.add_edge(function_name.clone(), target);
                    }
                }
            }
        }
    }

    let analysis = graph.analyze();
    let mut transitive: HashMap<Name, BTreeSet<String>> = HashMap::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Function(func_loc) = item {
            let name = baml_compiler_hir::function_qualified_name(db, *func_loc).display_name();
            let throw_set = analysis.transitive(&name).cloned().unwrap_or_default();
            transitive.insert(name, throw_set);
        }
    }

    FunctionThrowSets::new(db, transitive)
}

/// Check if a function body contains any catch expressions.
fn body_has_catch(body: &Arc<FunctionBody>) -> bool {
    if let FunctionBody::Expr(expr_body, _) = &**body {
        expr_body
            .exprs
            .iter()
            .any(|(_, expr)| matches!(expr, baml_compiler_hir::Expr::Catch { .. }))
    } else {
        false
    }
}

/// Compute the union of pre-pass callee throw contributions for a function.
fn compute_callee_prepass_throws(
    body: &Arc<FunctionBody>,
    pre_pass: FunctionThrowSets<'_>,
    db: &dyn Db,
) -> BTreeSet<String> {
    let mut callee_throws = BTreeSet::new();
    if let FunctionBody::Expr(expr_body, _) = &**body {
        for target in throw_inference::collect_call_targets(expr_body) {
            if let Some(ts) = pre_pass.transitive(db).get(&target) {
                callee_throws.extend(ts.iter().cloned());
            }
        }
    }
    callee_throws
}

/// Query: Get class field types for a project.
///
/// Maps class names to their field types. Also collects type errors
/// (e.g., unknown types) with position-independent locations for caching.
///
/// Following rust-analyzer's pattern: queries return both data and diagnostics,
/// making errors cacheable alongside the data they're derived from.
/// Error locations use position-independent IDs (class name + field index)
/// which are resolved to spans at diagnostic rendering time.
#[salsa::tracked]
pub fn class_field_types(db: &dyn Db, project: Project) -> ClassFieldTypesMap<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let resolution_ctx = TypeResolutionContext::new(db, project);
    let mut classes: HashMap<Name, HashMap<Name, Ty>> = HashMap::new();
    let mut errors: Vec<TirTypeError> = Vec::new();

    // Process user-defined classes
    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Class(class_loc) = item {
            let item_tree = baml_compiler_hir::file_item_tree(db, class_loc.file(db));
            let class_data = &item_tree[class_loc.id(db)];
            // Use qualified name so the key matches Ty::Class(fqn) references
            // (e.g., builtin file classes like "baml.llm.OrchestrationStep").
            let fqn = baml_compiler_hir::class_qualified_name(db, *class_loc);
            let class_name = fqn.display_name();

            let mut lowered_fields: HashMap<Name, Ty> = HashMap::new();

            // Lower each field's type with position-independent error location
            for (field_index, field_data) in class_data.fields.iter().enumerate() {
                // Use position-independent error location for cacheability
                let error_location = ErrorLocation::ClassFieldType {
                    class_name: class_name.clone(),
                    field_index,
                };

                let (ty, field_errors) =
                    resolution_ctx.lower_type_ref(&field_data.type_ref, error_location);
                errors.extend(field_errors);
                lowered_fields.insert(field_data.name.clone(), ty);
            }

            classes.insert(class_name, lowered_fields);
        }
    }

    // Add builtin class public fields (no errors possible here)
    for builtin in baml_builtins::builtin_types() {
        let public_fields: HashMap<Name, Ty> = builtin
            .fields
            .iter()
            .filter(|f| !f.is_private)
            .map(|f| (Name::new(f.name), builtins::substitute_unknown(&f.ty)))
            .collect();
        classes.insert(Name::new(builtin.path), public_fields);
    }

    ClassFieldTypesMap::new(db, classes, errors)
}

/// Query: Get type alias definitions for a project.
///
/// Maps type alias names to their resolved types. Also collects type errors
/// (e.g., unknown types) with position-independent locations for caching.
#[salsa::tracked]
pub fn type_aliases(db: &dyn Db, project: Project) -> TypeAliasesMap<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let resolution_ctx = TypeResolutionContext::new(db, project);
    let mut aliases = HashMap::new();
    let mut errors: Vec<TirTypeError> = Vec::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::TypeAlias(alias_loc) = item {
            let item_tree = baml_compiler_hir::file_item_tree(db, alias_loc.file(db));
            let alias_data = &item_tree[alias_loc.id(db)];

            // Use position-independent error location for cacheability
            // Start with empty path; the path will be updated as we recurse into nested types
            let error_location = ErrorLocation::TypeAliasType {
                alias_name: alias_data.name.clone(),
                path: vec![],
            };

            let (lowered_ty, alias_errors) =
                resolution_ctx.lower_type_ref(&alias_data.type_ref, error_location);
            errors.extend(alias_errors);
            aliases.insert(alias_data.name.clone(), lowered_ty);
        }
    }

    TypeAliasesMap::new(db, aliases, errors)
}

/// Query: Get class names for a project.
#[salsa::tracked]
pub fn class_names(db: &dyn Db, project: Project) -> ClassNamesSet<'_> {
    use baml_compiler_hir::QualifiedName;
    let items = baml_compiler_hir::project_items(db, project);
    let mut names = HashMap::new();

    // Add builtin class names (Rust-defined builtins like PrimitiveClient, PromptAst)
    for builtin in baml_builtins::builtin_types() {
        let qn = QualifiedName::from_builtin_path(builtin.path);
        names.insert(Name::new(builtin.path), qn);
    }

    // Add user-defined class names (using FQN for builtin-file classes)
    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Class(class_loc) = item {
            let qn = baml_compiler_hir::class_qualified_name(db, *class_loc);
            names.insert(qn.display_name(), qn);
        }
    }

    ClassNamesSet::new(db, names)
}

/// Query: Get enum names for a project.
#[salsa::tracked]
pub fn enum_names(db: &dyn Db, project: Project) -> EnumNamesSet<'_> {
    use baml_compiler_hir::QualifiedName;
    let items = baml_compiler_hir::project_items(db, project);
    let mut names = HashMap::new();

    // Add builtin enum names (FQN, e.g. "baml.llm.ClientType")
    for builtin_enum in baml_builtins::builtin_enums() {
        let qn = QualifiedName::from_builtin_path(builtin_enum.path);
        names.insert(Name::new(builtin_enum.path), qn);
    }

    // Add user-defined enum names
    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Enum(enum_loc) = item {
            let file = enum_loc.file(db);
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let enum_data = &item_tree[enum_loc.id(db)];
            names.insert(
                enum_data.name.clone(),
                QualifiedName::local(enum_data.name.clone()),
            );
        }
    }

    EnumNamesSet::new(db, names)
}

/// Query: Get type alias names for a project.
#[salsa::tracked]
pub fn type_alias_names(db: &dyn Db, project: Project) -> TypeAliasNamesSet<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let mut names = HashSet::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::TypeAlias(alias_loc) = item {
            let file = alias_loc.file(db);
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let alias_data = &item_tree[alias_loc.id(db)];
            names.insert(alias_data.name.clone());
        }
    }

    TypeAliasNamesSet::new(db, names)
}

/// Context for type resolution across a project.
///
/// This bundles together all the sets needed for resolved type lowering.
/// Create this once per project and reuse it for all type lowering operations.
pub struct TypeResolutionContext {
    pub class_names: HashMap<Name, baml_compiler_hir::QualifiedName>,
    pub enum_names: HashMap<Name, baml_compiler_hir::QualifiedName>,
    pub type_alias_names: HashSet<Name>,
}

impl TypeResolutionContext {
    /// Create a new type resolution context for a project.
    pub fn new(db: &dyn Db, project: Project) -> Self {
        Self {
            class_names: class_names(db, project).names(db).clone(),
            enum_names: enum_names(db, project).names(db).clone(),
            type_alias_names: type_alias_names(db, project).names(db).clone(),
        }
    }

    /// Lower a type reference with full resolution.
    ///
    /// The `location` parameter can be either:
    /// - A `Span` for direct span-based error reporting
    /// - An `ErrorLocation` for position-independent error locations (used by cached queries)
    pub fn lower_type_ref(
        &self,
        type_ref: &baml_compiler_hir::TypeRef,
        location: impl Into<ErrorLocation>,
    ) -> (Ty, Vec<TirTypeError>) {
        lower_type_ref(
            type_ref,
            &self.type_alias_names,
            &self.class_names,
            &self.enum_names,
            location,
        )
    }
}

// ============================================================================
// Type Inference Results
// ============================================================================

/// Result of type inference for a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceResult {
    /// Inferred return type of the function.
    pub return_type: Ty,
    /// Types of parameters.
    pub param_types: HashMap<Name, Ty>,
    /// Types inferred for each expression.
    pub expr_types: HashMap<ExprId, Ty>,
    /// For multi-segment path expressions, the type of each segment.
    /// For `o.inner.value` where `o: Outer`, stores `[Outer, Inner, int]`.
    /// Used by codegen to look up field indices at each step.
    pub path_segment_types: HashMap<ExprId, Vec<Ty>>,
    /// For multi-segment path expressions, the resolution for each segment.
    /// Parallel structure to `path_segment_types` but stores resolutions.
    /// This allows VIR to properly resolve each synthesized expression when
    /// converting multi-segment paths to nested expressions.
    pub path_segment_resolutions: HashMap<ExprId, Vec<ResolvedValue>>,
    /// Expressions that are enum variant values (e.g., `Status.Active`).
    /// Maps expression ID to (`enum_name`, `variant_name`).
    /// Used by codegen to emit enum variant construction.
    pub enum_variant_exprs: HashMap<ExprId, (Name, Name)>,
    /// Match expressions that are exhaustive (all cases covered).
    /// Used by codegen to emit `unreachable` for fallthrough paths,
    /// enabling phi-like optimization for match results.
    pub exhaustive_matches: HashSet<ExprId>,
    /// Whether the function body always diverges (every code path throws/returns-never).
    /// Used at call sites: if `body_diverges` is true, the call expression's type is
    /// refined to `Never` instead of the declared return type, since the function can
    /// never produce a value.
    pub body_diverges: bool,
    /// Throw types that actually escape the function after catch handling.
    /// Computed post-inference; used for `throws` contract checking.
    pub effective_throws: BTreeSet<String>,
    /// Type checking errors.
    pub errors: Vec<TirTypeError>,
    /// Resolution information for IDE features (go-to-definition, find-references).
    /// Maps expression IDs to what they resolve to.
    pub expr_resolutions: ResolutionMap,
    /// Types for pattern bindings (catch, match arm).
    /// Used by hover to show the binding's declaration type at the pattern site.
    pub pattern_types: HashMap<PatId, Ty>,
}

// ============================================================================
// Type Context
// ============================================================================

/// Where a local variable was defined (for go-to-definition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionSite {
    /// Defined in a let statement.
    Statement(StmtId),
    /// Defined as a function parameter (with its index).
    Parameter(usize),
    /// Defined by a pattern binding (catch, match arm).
    Pattern(PatId),
}

/// Context for type inference, tracking scopes and accumulated results.
pub struct TypeContext<'db> {
    db: &'db dyn Db,
    /// Stack of variable scopes (innermost last).
    scopes: Vec<HashMap<Name, Ty>>,
    /// Class field types: `class_name` -> (`field_name` -> `field_type`)
    class_fields: HashMap<Name, HashMap<Name, Ty>>,
    /// Type alias definitions: `alias_name` -> `resolved_type`
    type_aliases: HashMap<Name, Ty>,
    /// Enum variant definitions: `enum_name` -> `Vec<variant_name>`
    enum_variants: HashMap<Name, Vec<Name>>,
    /// Class names mapped to their qualified names for type resolution
    class_names: HashMap<Name, baml_compiler_hir::QualifiedName>,
    /// Enum names mapped to their qualified names for type resolution
    enum_names: HashMap<Name, baml_compiler_hir::QualifiedName>,
    /// Type alias names for validation
    type_alias_names: HashSet<Name>,
    /// Inferred types for expressions.
    expr_types: HashMap<ExprId, Ty>,
    /// For multi-segment paths, the type of each segment.
    path_segment_types: HashMap<ExprId, Vec<Ty>>,
    /// For multi-segment paths, the resolution for each segment.
    path_segment_resolutions: HashMap<ExprId, Vec<ResolvedValue>>,
    /// Expressions that are enum variant values.
    enum_variant_exprs: HashMap<ExprId, (Name, Name)>,
    /// Match expressions that are exhaustive (all cases covered).
    exhaustive_matches: HashSet<ExprId>,
    /// Types of all return statements encountered during inference.
    /// Used to validate that all return paths match the declared return type.
    return_types: Vec<(Ty, Span)>,
    /// Accumulated type errors.
    errors: Vec<TirTypeError>,
    /// The current file being typechecked
    file_id: FileId,
    /// Variables declared with `watch let` (tracked for $watch validation).
    watched_vars: HashSet<Name>,
    /// Residual throw types for each catch expression (types that escape the catch).
    /// Populated during inference, used for `throws` contract checking.
    catch_residual_throws: HashMap<ExprId, BTreeSet<String>>,
    /// Resolution map for expressions (for IDE features).
    expr_resolutions: ResolutionMap,
    /// Track where local variables were defined (for go-to-definition).
    local_definitions: HashMap<Name, DefinitionSite>,
    /// Types for pattern bindings (catch, match arm).
    pattern_types: HashMap<PatId, Ty>,
    /// Optional source map for looking up spans (for type annotation errors).
    hir_source_map: Option<HirSourceMap>,
    /// The function's declared return type.
    /// Used as a fallback so that `return` statements in deeply nested contexts
    /// (e.g. catch arms, match arms) are always validated.
    fn_return_type: Option<Ty>,
}

impl<'db> TypeContext<'db> {
    /// Create a new type context with full type resolution info.
    #[allow(clippy::too_many_arguments)]
    pub fn with_type_info(
        db: &'db dyn Db,
        globals: HashMap<Name, Ty>,
        class_fields: HashMap<Name, HashMap<Name, Ty>>,
        type_aliases: HashMap<Name, Ty>,
        enum_variants: HashMap<Name, Vec<Name>>,
        class_names: HashMap<Name, baml_compiler_hir::QualifiedName>,
        enum_names: HashMap<Name, baml_compiler_hir::QualifiedName>,
        type_alias_names: HashSet<Name>,
        file_id: FileId,
        hir_source_map: Option<HirSourceMap>,
    ) -> Self {
        TypeContext {
            db,
            scopes: vec![globals],
            class_fields,
            type_aliases,
            enum_variants,
            class_names,
            enum_names,
            type_alias_names,
            expr_types: HashMap::new(),
            path_segment_types: HashMap::new(),
            path_segment_resolutions: HashMap::new(),
            enum_variant_exprs: HashMap::new(),
            exhaustive_matches: HashSet::new(),
            return_types: Vec::new(),
            errors: Vec::new(),
            file_id,
            watched_vars: HashSet::new(),
            catch_residual_throws: HashMap::new(),
            expr_resolutions: HashMap::new(),
            local_definitions: HashMap::new(),
            pattern_types: HashMap::new(),
            hir_source_map,
            fn_return_type: None,
        }
    }

    /// Record a return type encountered during inference.
    pub fn record_return(&mut self, ty: Ty, span: Span) {
        self.return_types.push((ty, span));
    }

    /// Look up a type alias definition.
    pub fn lookup_type_alias(&self, name: &Name) -> Option<&Ty> {
        self.type_aliases.get(name)
    }

    /// Look up enum variants.
    pub fn lookup_enum_variants(&self, name: &Name) -> Option<&Vec<Name>> {
        self.enum_variants.get(name)
    }

    /// Look up a field in a class.
    pub fn lookup_class_field(&self, class_name: &Name, field_name: &Name) -> Option<&Ty> {
        self.class_fields
            .get(class_name)
            .and_then(|fields| fields.get(field_name))
    }

    /// Push a new scope.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current scope.
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a variable in the current scope.
    pub fn define(&mut self, name: Name, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Look up a variable in the scope chain.
    pub fn lookup(&self, name: &Name) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    /// Check if a variable is defined in a local scope (not the global scope).
    ///
    /// The global scope is `scopes[0]`. Local scopes are any scopes pushed after that
    /// (function parameters, let bindings, match arm patterns, etc.).
    ///
    /// Returns true if the variable is found in a local scope, false if it's only
    /// found in the global scope or not found at all.
    pub fn is_in_local_scope(&self, name: &Name) -> bool {
        // Search local scopes only (skip scopes[0] which is global)
        // scopes[1..] are local scopes
        for scope in self.scopes.iter().skip(1) {
            if scope.contains_key(name) {
                return true;
            }
        }
        false
    }

    /// Record the type of an expression.
    pub fn set_expr_type(&mut self, expr: ExprId, ty: Ty) {
        self.expr_types.insert(expr, ty);
    }

    /// Add a type error.
    pub fn push_error(&mut self, error: TirTypeError) {
        self.errors.push(error);
    }

    /// Mark a variable as watched (declared with `watch let`).
    pub fn mark_watched(&mut self, name: Name) {
        self.watched_vars.insert(name);
    }

    /// Check if a variable is watched (declared with `watch let`).
    pub fn is_watched(&self, name: &Name) -> bool {
        self.watched_vars.contains(name)
    }

    /// Set the resolution for an expression.
    pub fn set_expr_resolution(&mut self, expr_id: ExprId, resolution: ResolvedValue) {
        self.expr_resolutions.insert(expr_id, resolution);
    }

    /// Define a local variable and track its definition site.
    pub fn define_with_site(&mut self, name: Name, ty: Ty, definition_site: DefinitionSite) {
        // Get the current scope (last in the stack)
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.clone(), ty);
        }
        self.local_definitions.insert(name, definition_site);
    }

    /// Get the definition site for a local variable.
    pub fn get_definition_site(&self, name: &Name) -> Option<DefinitionSite> {
        self.local_definitions.get(name).copied()
    }

    /// Get the database reference.
    pub fn db(&self) -> &'db dyn Db {
        self.db
    }

    pub fn build_span(&self, range: TextRange) -> Span {
        Span::new(self.file_id, range)
    }

    pub fn build_span_default(&self, range: &Option<TextRange>) -> Span {
        // todo: probably this should be an error? it should be an invariant that
        // all exprs have valid spans
        range.map(|s| self.build_span(s)).unwrap_or_default()
    }

    /// Look up the span for a type from the source map.
    pub fn type_span(&self, id: TypeId) -> Span {
        self.hir_source_map
            .as_ref()
            .and_then(|sm| sm.type_span(id))
            .unwrap_or_default()
    }

    /// Look up the span for a pattern from the source map.
    pub fn pattern_span(&self, id: PatId) -> Span {
        self.hir_source_map
            .as_ref()
            .and_then(|sm| sm.pattern_span(id))
            .unwrap_or_default()
    }

    /// Check if `sub` is a subtype of `sup`, resolving type aliases.
    pub fn is_subtype_of(&self, sub: &Ty, sup: &Ty) -> bool {
        normalize::is_subtype_of(sub, sup, &self.type_aliases)
    }

    /// Lower a `TypeRef` to a `Ty` with full resolution and validation.
    ///
    /// This is the single entry point for type lowering during inference.
    /// It resolves classes/enums to their concrete types, validates map key
    /// types, and accumulates any errors.
    pub fn lower_type(&mut self, type_ref: &baml_compiler_hir::TypeRef, span: Span) -> Ty {
        let (ty, errors) = lower_type_ref(
            type_ref,
            &self.type_alias_names,
            &self.class_names,
            &self.enum_names,
            span,
        );

        // Accumulate lowering errors (e.g., unknown types)
        self.errors.extend(errors);

        // Validate map key types
        let invalid_keys = normalize::find_invalid_map_keys(&ty, &self.type_aliases);
        for invalid_key in invalid_keys {
            self.errors.push(TypeError::InvalidMapKeyType {
                ty: invalid_key,
                location: ErrorLocation::Span(span),
            });
        }

        ty
    }

    /// Resolve a named type to its proper Ty representation.
    ///
    /// Looks up the name in the class/enum maps to get the pre-computed `QualifiedName`.
    /// Type aliases and unknown types stay as `Ty::TypeAlias`.
    pub fn resolve_named_type(&self, name: &Name) -> Ty {
        use baml_compiler_hir::QualifiedName;
        if let Some(qn) = self.class_names.get(name) {
            Ty::Class(qn.clone(), d())
        } else if let Some(qn) = self.enum_names.get(name) {
            Ty::Enum(qn.clone(), d())
        } else {
            Ty::TypeAlias(QualifiedName::local(name.clone()), d())
        }
    }

    /// Resolve a path to determine what it refers to.
    ///
    /// This is the core path resolution logic that determines whether a path like
    /// `user.name` is a local variable with field access, an enum variant, a module
    /// item, etc.
    ///
    /// # Resolution Order
    /// 1. Check if the first segment is a local variable -> Local with field accesses
    /// 2. Check if it's a function name -> Function
    /// 3. Check if first segment is a class name (for enum variants)
    /// 4. Check if it's a module path (TODO: not yet implemented)
    /// 5. Unknown
    pub fn resolve_path(&self, segments: &[Name]) -> ResolvedPath {
        if segments.is_empty() {
            return ResolvedPath::Unknown;
        }

        let first = &segments[0];

        // Check if first segment is a local variable
        if self.lookup(first).is_some() {
            return ResolvedPath::Local {
                name: first.clone(),
            };
        }

        // For single-segment paths, check if it's a function
        if segments.len() == 1 {
            // Check globals (which include functions)
            if self.scopes.first().and_then(|s| s.get(first)).is_some() {
                return ResolvedPath::Function {
                    name: first.clone(),
                };
            }
        }

        // For two-segment paths, check if it could be an enum variant
        // TODO: This needs access to the type registry to check if `first` is an enum
        if segments.len() == 2 {
            // For now, we can't distinguish enum variants without more context
            // This will be populated when we have the Module infrastructure
        }

        // TODO: Check module paths when Module type is fully integrated

        // Unknown path
        ResolvedPath::Unknown
    }
}

// ============================================================================
// Type Inference
// ============================================================================

/// Infer types for a function body.
///
/// This is the main entry point for type inference. It takes a pre-lowered
/// function body and infers types for all expressions.
///
/// The `globals` parameter provides types for top-level functions, allowing
/// function calls to be properly typed. Pass `None` if no global context is needed.
///
/// Note: In a full implementation, this would be a Salsa tracked function.
/// For now, it's a regular function that takes the body directly.
#[allow(clippy::too_many_arguments)]
pub fn infer_function_body<'db>(
    db: &'db dyn Db,
    body: &FunctionBody,
    param_types: HashMap<Name, Ty>,
    expected_return: &Ty,
    return_type_span: Option<Span>,
    globals: Option<HashMap<Name, Ty>>,
    class_fields: Option<HashMap<Name, HashMap<Name, Ty>>>,
    type_aliases: Option<HashMap<Name, Ty>>,
    enum_variants: Option<HashMap<Name, Vec<Name>>>,
    class_names_opt: Option<HashMap<Name, baml_compiler_hir::QualifiedName>>,
    enum_names_opt: Option<HashMap<Name, baml_compiler_hir::QualifiedName>>,
    type_alias_names: Option<HashSet<Name>>,
    function_loc: FunctionLoc<'db>,
    declared_throws: Option<&(BTreeSet<String>, Span)>,
) -> InferenceResult {
    let file_id = function_loc.file(db).file_id(db);

    // Extract source map from body if available
    let hir_source_map = match body {
        FunctionBody::Expr(_, source_map) => Some(source_map.clone()),
        _ => None,
    };

    let mut ctx = TypeContext::with_type_info(
        db,
        globals.unwrap_or_default(),
        class_fields.unwrap_or_default(),
        type_aliases.unwrap_or_default(),
        enum_variants.unwrap_or_default(),
        class_names_opt.unwrap_or_default(),
        enum_names_opt.unwrap_or_default(),
        type_alias_names.unwrap_or_default(),
        file_id,
        hir_source_map,
    );

    ctx.fn_return_type = Some(expected_return.clone());

    // Add parameters to the current scope (on top of globals)
    // Track their index in the parameter list for go-to-definition
    for (index, (name, ty)) in param_types.iter().enumerate() {
        ctx.define_with_site(name.clone(), ty.clone(), DefinitionSite::Parameter(index));
    }

    // Type check the body against the expected return type (checking mode for bidirectional typing)
    let (trailing_expr_type, body_location) = match body {
        FunctionBody::Expr(expr_body, _source_map) => {
            if let Some(root_expr) = expr_body.root_expr {
                // Use check_expr for bidirectional typing - check body against expected return type
                let ty = check_expr(&mut ctx, root_expr, expr_body, expected_return);
                (ty, ErrorLocation::Expr(root_expr))
            } else {
                (
                    Ty::Void { attr: d() },
                    ErrorLocation::Span(return_type_span.unwrap_or_default()),
                )
            }
        }
        FunctionBody::Llm(llm_body) => {
            // Validate Jinja templates in the prompt
            validate_llm_prompt(&mut ctx, &llm_body.prompt, &param_types);

            // LLM functions return their declared return type
            (
                expected_return.clone(),
                ErrorLocation::Span(return_type_span.unwrap_or_default()),
            )
        }
        FunctionBody::Missing => (
            expected_return.clone(),
            ErrorLocation::Span(return_type_span.unwrap_or_default()),
        ),
    };

    // With bidirectional type checking, return statements are already checked
    // via check_stmt_with_return, so we don't need to check them again here.

    // With bidirectional type checking, check_expr already reported any mismatches
    // between the body and expected return type. We only need to check one case:
    // If there are no returns and no tail expression, and we expected a non-void type
    if ctx.return_types.is_empty()
        && trailing_expr_type.is_void()
        && !expected_return.is_void()
        && !expected_return.is_unknown()
        && !expected_return.is_error()
    {
        // If the trailing type is void (no tail expression) but we need a non-void return,
        // emit a clearer "missing return expression" error
        let error = if trailing_expr_type.is_void() && !expected_return.is_void() {
            TypeError::MissingReturnExpression {
                expected: expected_return.clone(),
                location: body_location,
            }
        } else {
            TypeError::TypeMismatch {
                expected: expected_return.clone(),
                found: trailing_expr_type.clone(),
                location: body_location,
                info_location: return_type_span.map(ErrorLocation::Span),
            }
        };
        ctx.push_error(error);
    }

    // Determine the inferred return type:
    // - If there are explicit return statements, use the expected type (we already validated them)
    // - If there's a trailing expression (not Void), use its type
    // - Otherwise, use Void
    let return_type = if !ctx.return_types.is_empty() {
        // If there are return statements, the function returns what they return
        // (we've already checked they match expected_return)
        expected_return.clone()
    } else if !trailing_expr_type.is_void() {
        trailing_expr_type
    } else {
        Ty::Void { attr: d() }
    };

    // Track whether control flow can ever complete normally.
    let body_diverges = match body {
        FunctionBody::Expr(expr_body, _) => expr_body
            .root_expr
            .is_some_and(|root_expr| expr_always_diverges(ctx.db(), root_expr, expr_body)),
        _ => false,
    };

    // Compute effective throws (catch-aware) for contract checking
    let effective_throws = match body {
        FunctionBody::Expr(expr_body, _) => {
            let mut throws = BTreeSet::new();
            if let Some(root_expr) = expr_body.root_expr {
                collect_effective_throws_from_expr(&ctx, root_expr, expr_body, &mut throws);
            }
            throws
        }
        _ => BTreeSet::new(),
    };

    // Check throws contract if declared (fact-set comparison, no Ty conversion)
    if let Some((declared_facts, throws_span)) = declared_throws {
        let location = ErrorLocation::Span(*throws_span);

        // Violation: effective throws include facts not in the declared set
        let uncovered: Vec<String> = effective_throws
            .iter()
            .filter(|fact| !declared_facts.contains(*fact))
            .cloned()
            .collect();
        if !uncovered.is_empty() {
            ctx.push_error(TypeError::ThrowsContractViolation {
                extra_types: uncovered,
                location: location.clone(),
            });
        }

        // Extraneous: declared facts not matched by any effective throw.
        // Skip when effective throws is empty (stub function) or declared
        // is empty (`throws never` — an assertion, not a type list).
        if !effective_throws.is_empty() && !declared_facts.is_empty() {
            let unused: Vec<String> = declared_facts
                .iter()
                .filter(|df| !effective_throws.contains(*df))
                .cloned()
                .collect();
            if !unused.is_empty() {
                ctx.push_error(TypeError::ThrowsContractExtraneous {
                    unused_types: unused,
                    location,
                });
            }
        }
    }

    InferenceResult {
        return_type,
        param_types,
        expr_types: ctx.expr_types,
        path_segment_types: ctx.path_segment_types,
        path_segment_resolutions: ctx.path_segment_resolutions,
        enum_variant_exprs: ctx.enum_variant_exprs,
        exhaustive_matches: ctx.exhaustive_matches,
        body_diverges,
        effective_throws,
        errors: ctx.errors,
        expr_resolutions: ctx.expr_resolutions,
        pattern_types: ctx.pattern_types,
    }
}

/// Add built-in BAML types to a Jinja type environment.
///
/// This adds the special variables `_` and `ctx` along with their class definitions:
/// - `_` (`baml::BuiltIn)`: has `chat` and `role` function properties
/// - `ctx` (`baml::Context)`: has `output_format`, `client`, and `tags` properties
/// - `baml::Client`: has `name` and `provider` string properties
/// - `jinja::loop`: has standard Jinja loop variables (index, first, last, etc.)
fn add_builtin_jinja_types(jinja_env: &mut jinja::JinjaTypeEnv) {
    use jinja::JinjaType;

    // Define baml::Client class
    jinja_env.add_class(
        "baml::Client",
        indexmap::IndexMap::from([
            ("name".to_string(), JinjaType::String),
            ("provider".to_string(), JinjaType::String),
        ]),
    );

    // Define baml::Context class
    // output_format can be used as a string or called as a function
    jinja_env.add_class(
        "baml::Context",
        indexmap::IndexMap::from([
            ("output_format".to_string(), JinjaType::String), // Simplified: just String for now
            (
                "client".to_string(),
                JinjaType::ClassRef("baml::Client".to_string()),
            ),
            (
                "tags".to_string(),
                JinjaType::Map(Box::new(JinjaType::String), Box::new(JinjaType::String)),
            ),
        ]),
    );

    // Define baml::BuiltIn class (for `_`)
    // chat and role are functions that set the chat role
    jinja_env.add_class(
        "baml::BuiltIn",
        indexmap::IndexMap::from([
            (
                "chat".to_string(),
                JinjaType::FunctionRef("baml::Chat".to_string()),
            ),
            (
                "role".to_string(),
                JinjaType::FunctionRef("baml::Chat".to_string()),
            ),
        ]),
    );

    // Define jinja::loop class (available inside for loops)
    jinja_env.add_class(
        "jinja::loop",
        indexmap::IndexMap::from([
            ("index".to_string(), JinjaType::Int),
            ("index0".to_string(), JinjaType::Int),
            ("revindex".to_string(), JinjaType::Int),
            ("revindex0".to_string(), JinjaType::Int),
            ("first".to_string(), JinjaType::Bool),
            ("last".to_string(), JinjaType::Bool),
            ("length".to_string(), JinjaType::Int),
            ("depth".to_string(), JinjaType::Int),
            ("depth0".to_string(), JinjaType::Int),
        ]),
    );

    // Define baml::Chat function as String -> ()
    jinja_env.add_function(
        "baml::Chat".to_string(),
        JinjaType::None, // Returns null/void
        vec![("role".to_string(), JinjaType::String)],
    );

    // Add the special variables
    jinja_env.add_variable("_", JinjaType::ClassRef("baml::BuiltIn".to_string()));
    jinja_env.add_variable("ctx", JinjaType::ClassRef("baml::Context".to_string()));
}

/// Convert a Jinja type error to a TIR type error.
///
/// This maps the structured `jinja::TypeError` enum to the compiler's `TypeError` enum,
/// preserving all error data while converting the span to an `ErrorLocation`.
fn jinja_error_to_tir(error: jinja::TypeError) -> TirTypeError {
    let span = error.span();
    // Minijinja spans are 0-based byte offsets into the template text.
    let location = ErrorLocation::JinjaTemplate {
        start_offset: span.start_offset,
        end_offset: span.end_offset,
    };

    match error {
        jinja::TypeError::UnresolvedVariable {
            name, suggestions, ..
        } => TypeError::JinjaUnresolvedVariable {
            name,
            suggestions,
            location,
        },
        jinja::TypeError::FunctionReferenceWithoutCall { function_name, .. } => {
            TypeError::JinjaFunctionReferenceWithoutCall {
                function_name,
                location,
            }
        }
        jinja::TypeError::InvalidFilter {
            filter_name,
            suggestions,
            ..
        } => TypeError::JinjaInvalidFilter {
            filter_name,
            suggestions,
            location,
        },
        jinja::TypeError::InvalidType {
            expression,
            expected,
            found,
            ..
        } => TypeError::JinjaInvalidType {
            expression,
            expected,
            found,
            location,
        },
        jinja::TypeError::PropertyNotDefined {
            variable,
            class_name,
            property,
            ..
        } => TypeError::JinjaPropertyNotDefined {
            variable,
            class_name,
            property,
            location,
        },
        jinja::TypeError::EnumValuePropertyAccess {
            variable,
            enum_value,
            property,
            ..
        } => TypeError::JinjaEnumValuePropertyAccess {
            variable,
            enum_value,
            property,
            location,
        },
        jinja::TypeError::EnumStringComparison { enum_name, .. } => {
            TypeError::JinjaEnumStringComparison {
                enum_name,
                location,
            }
        }
        jinja::TypeError::PropertyNotFoundInUnion {
            property,
            missing_on,
            ..
        } => TypeError::JinjaPropertyNotFoundInUnion {
            property,
            missing_on,
            location,
        },
        jinja::TypeError::PropertyTypeMismatchInUnion { property, .. } => {
            TypeError::JinjaPropertyTypeMismatchInUnion { property, location }
        }
        jinja::TypeError::NonClassInUnion {
            variable,
            property,
            non_class_type,
            ..
        } => TypeError::JinjaNonClassInUnion {
            variable,
            property,
            non_class_type,
            location,
        },
        jinja::TypeError::WrongArgCount {
            function_name,
            expected,
            found,
            ..
        } => TypeError::JinjaWrongArgCount {
            function_name,
            expected,
            found,
            location,
        },
        jinja::TypeError::MissingArg {
            function_name,
            arg_name,
            ..
        } => TypeError::JinjaMissingArg {
            function_name,
            arg_name,
            location,
        },
        jinja::TypeError::UnknownArg {
            function_name,
            arg_name,
            suggestions,
            ..
        } => TypeError::JinjaUnknownArg {
            function_name,
            arg_name,
            suggestions,
            location,
        },
        jinja::TypeError::WrongArgType {
            function_name,
            arg_name,
            expected,
            found,
            ..
        } => TypeError::JinjaWrongArgType {
            function_name,
            arg_name,
            expected,
            found,
            location,
        },
        jinja::TypeError::UnsupportedFeature { feature, .. } => {
            TypeError::JinjaUnsupportedFeature { feature, location }
        }
        jinja::TypeError::InvalidSyntax { message, .. } => {
            TypeError::JinjaInvalidSyntax { message, location }
        }
        jinja::TypeError::InvalidTest {
            test_name,
            suggestions,
            ..
        } => TypeError::JinjaInvalidTest {
            test_name,
            suggestions,
            location,
        },
    }
}

/// Validate Jinja templates in an LLM function's prompt.
///
/// This builds a Jinja type environment from the TIR context and validates
/// the prompt template, converting any Jinja type errors to TIR type errors.
#[allow(clippy::cast_possible_truncation)]
fn validate_llm_prompt(
    ctx: &mut TypeContext<'_>,
    prompt: &PromptTemplate,
    param_types: &HashMap<Name, Ty>,
) {
    use jinja::{JinjaType, JinjaTypeEnv};

    // Build a Jinja type environment from the TIR context
    let mut jinja_env = JinjaTypeEnv::new();

    let aliases = &ctx.type_aliases;

    // Add function parameters
    for (param_name, param_ty) in param_types {
        let jinja_ty = JinjaType::from_ty(param_ty, aliases);
        jinja_env.add_variable(param_name.to_string(), jinja_ty);
    }

    // Add built-in BAML types for Jinja templates
    add_builtin_jinja_types(&mut jinja_env);

    // Add class definitions from the context
    for (class_name, fields) in &ctx.class_fields {
        let field_types: indexmap::IndexMap<String, JinjaType> = fields
            .iter()
            .map(|(fname, fty)| (fname.to_string(), JinjaType::from_ty(fty, aliases)))
            .collect();
        jinja_env.add_class(class_name.to_string(), field_types);
    }

    // Add enum definitions from the context
    for (enum_name, variants) in &ctx.enum_variants {
        jinja_env.add_enum(
            enum_name.to_string(),
            variants
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        );
    }

    // Add template string functions from globals
    // Functions in scope are available to call in templates
    // We add them both as functions (for signature checking) and as variables
    // (so {{ Foo() }} resolves "Foo" as a callable)
    for (func_name, func_ty) in ctx.scopes.first().unwrap_or(&HashMap::new()) {
        if let Ty::Function { params, ret, .. } = func_ty {
            // Extract parameter names and types from Ty::Function
            // Names are stored directly in params as (Option<Name>, Ty)
            let jinja_params: Vec<(String, JinjaType)> = params
                .iter()
                .enumerate()
                .map(|(i, (name, ty))| {
                    let param_name = name
                        .as_ref()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| format!("arg{i}"));
                    (param_name, JinjaType::from_ty(ty, aliases))
                })
                .collect();
            let jinja_ret = JinjaType::from_ty(ret.as_ref(), aliases);

            // Add function to the function map (for signature validation)
            jinja_env.add_function(func_name.to_string(), jinja_ret, jinja_params);

            // Also add as a variable with FunctionRef type (so Var lookup succeeds)
            jinja_env.add_variable(
                func_name.to_string(),
                JinjaType::FunctionRef(func_name.to_string()),
            );
        }
    }

    // Validate the entire prompt template
    match jinja::validate_template(&prompt.text, &mut jinja_env) {
        Ok(errors) => {
            // Convert Jinja errors to TIR errors with position-independent locations.
            // The jinja span is relative to the prompt text start.
            // We store relative offsets here; they'll be converted to absolute spans
            // at diagnostic rendering time by looking up the prompt's file offset from CST.
            for error in errors {
                ctx.push_error(jinja_error_to_tir(error));
            }
        }
        Err(parse_error) => {
            // Jinja parse error - report the error location if available.
            let (start_offset, end_offset) = parse_error
                .range()
                .map(|r| (r.start as u32, r.end as u32))
                .unwrap_or((0, 1));
            ctx.push_error(TypeError::JinjaParseError {
                message: parse_error.to_string(),
                location: ErrorLocation::JinjaTemplate {
                    start_offset,
                    end_offset,
                },
            });
        }
    }
}

/// Salsa tracked query for function type inference.
///
/// This caches the type inference results for a function, enabling
/// incremental recomputation when dependencies change.
#[salsa::tracked]
pub fn function_type_inference<'db>(
    db: &'db dyn Db,
    function: FunctionLoc<'db>,
) -> Arc<InferenceResult> {
    // Get the function signature and body
    // NOTE: We intentionally don't call function_signature_source_map here.
    // This allows Salsa early cutoff: when only whitespace/comments change,
    // function_signature returns an equal value, so this query is cached.
    // The trade-off is that type mismatch errors won't point to the return
    // type annotation, but they'll still point to the offending expression.
    let signature = baml_compiler_hir::function_signature(db, function);

    // For the main LLM-call function (Foo), use the original LlmBody for type inference.
    // For Foo.render_prompt and Foo.build_request, use the synthetic Expr body.
    let body = if let Some(llm_meta) = baml_compiler_hir::llm_function_meta(db, function) {
        Arc::new(baml_compiler_hir::FunctionBody::Llm((*llm_meta).clone()))
    } else {
        baml_compiler_hir::function_body(db, function)
    };

    // Get the project context
    let project = db.project();

    // Build global context from the project
    // Get function signatures as global types (for function calls)
    let typing_ctx = typing_context(db, project);
    let globals = Some(typing_ctx.functions(db).clone());

    // Get class field types
    let class_field_types = class_field_types(db, project);
    let class_fields = Some(class_field_types.classes(db).clone());

    // Get type aliases
    let type_aliases_map = type_aliases(db, project);
    let type_aliases = Some(type_aliases_map.aliases(db).clone());

    // Get enum variants
    let enum_variants_map = enum_variants(db, project);
    let enum_variants = Some(enum_variants_map.enums(db).clone());

    let result = infer_function(
        db,
        &signature,
        None, // No source map - enables Salsa early cutoff on whitespace changes
        &body,
        globals,
        class_fields,
        type_aliases,
        enum_variants,
        function,
    );

    Arc::new(result)
}

/// Validate a template string's Jinja template body.
///
/// Template strings don't need full type inference like functions - they just need
/// their Jinja templates validated against available variables (parameters, globals, etc.)
///
/// This also validates that parameter types exist (e.g., no unknown types).
pub fn validate_template_string_body(
    db: &dyn Db,
    ts_loc: baml_compiler_hir::TemplateStringLoc<'_>,
) -> Vec<TirTypeError> {
    use baml_compiler_hir::{template_string_body, template_string_signature};
    use baml_compiler_parser::syntax_tree;
    use baml_compiler_syntax::ast::{Item, Parameter, SourceFile};
    use jinja::{JinjaType, JinjaTypeEnv};
    use rowan::ast::AstNode;

    let signature = template_string_signature(db, ts_loc);
    let body = template_string_body(db, ts_loc);
    let project = db.project();

    // Get file_id for span conversion
    let file = ts_loc.file(db);
    let file_id = file.file_id(db);

    // Get the typing context (functions/template strings available)
    let typing_ctx = typing_context(db, project);
    let globals = typing_ctx.functions(db);

    // Get class field types
    let class_field_types = class_field_types(db, project);
    let class_fields = class_field_types.classes(db);

    // Get enum variants
    let enum_variants_map = enum_variants(db, project);
    let enum_variants = enum_variants_map.enums(db);

    // Get type aliases for resolving alias types in Jinja
    let type_aliases_map = type_aliases(db, project);
    let aliases = type_aliases_map.aliases(db);

    // Build a Jinja type environment
    let mut jinja_env = JinjaTypeEnv::new();

    // Collect parameter type errors with proper spans
    let mut type_errors: Vec<TirTypeError> = Vec::new();
    let resolution_ctx = TypeResolutionContext::new(db, project);

    // Get CST to find parameter type spans
    let item_tree = baml_compiler_hir::file_item_tree(db, file);
    let ts_data = &item_tree[ts_loc.id(db)];
    let ts_name = ts_data.name.as_str();
    let occurrence = ts_loc.id(db).index();

    let tree = syntax_tree(db, file);
    let source_file = SourceFile::cast(tree).unwrap();

    let ts_def = source_file
        .items()
        .filter_map(|item| match item {
            Item::TemplateString(t) => Some(t),
            _ => None,
        })
        .filter(|t| t.name().map(|n| n.text() == ts_name).unwrap_or(false))
        .nth(occurrence as usize);

    let cst_params: Vec<Parameter> = ts_def
        .and_then(|ts| ts.param_list())
        .map(|pl| pl.params().collect())
        .unwrap_or_default();

    // Add template string parameters with proper span-based error collection
    for (idx, param) in signature.params.iter().enumerate() {
        let span = cst_params
            .get(idx)
            .and_then(baml_compiler_syntax::Parameter::ty)
            .map(|te| Span::new(file_id, te.syntax().text_range()))
            .unwrap_or_default();

        let (ty, param_errors) = resolution_ctx.lower_type_ref(&param.type_ref, span);
        type_errors.extend(param_errors);

        let jinja_ty = JinjaType::from_ty(&ty, aliases);
        jinja_env.add_variable(param.name.to_string(), jinja_ty);
    }

    // Add built-in BAML types for Jinja templates
    add_builtin_jinja_types(&mut jinja_env);

    // Add class definitions
    for (class_name, fields) in class_fields {
        let field_types: indexmap::IndexMap<String, JinjaType> = fields
            .iter()
            .map(|(fname, fty)| (fname.to_string(), JinjaType::from_ty(fty, aliases)))
            .collect();
        jinja_env.add_class(class_name.to_string(), field_types);
    }

    // Add enum definitions
    for (enum_name, variants) in enum_variants {
        jinja_env.add_enum(
            enum_name.to_string(),
            variants
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        );
    }

    // Add functions (including other template strings) from globals
    for (func_name, func_ty) in globals {
        if let Ty::Function { params, ret, .. } = func_ty {
            // Extract parameter names and types from Ty::Function
            // Names are stored directly in params as (Option<Name>, Ty)
            let jinja_params: Vec<(String, JinjaType)> = params
                .iter()
                .enumerate()
                .map(|(i, (name, ty))| {
                    let param_name = name
                        .as_ref()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| format!("arg{i}"));
                    (param_name, JinjaType::from_ty(ty, aliases))
                })
                .collect();
            let jinja_ret = JinjaType::from_ty(ret.as_ref(), aliases);

            jinja_env.add_function(func_name.to_string(), jinja_ret, jinja_params);
            jinja_env.add_variable(
                func_name.to_string(),
                JinjaType::FunctionRef(func_name.to_string()),
            );
        }
    }

    // Validate the template with position-independent error locations
    match jinja::validate_template(&body.text, &mut jinja_env) {
        Ok(jinja_errors) => {
            for error in jinja_errors {
                type_errors.push(jinja_error_to_tir(error));
            }
        }
        Err(parse_error) => {
            type_errors.push(TypeError::JinjaParseError {
                message: parse_error.to_string(),
                location: ErrorLocation::JinjaTemplate {
                    start_offset: 0,
                    end_offset: 1,
                },
            });
        }
    }

    type_errors
}

/// Infer types for a function given its signature and body.
///
/// This queries the database for known type names and validates that all type
/// references in the signature refer to types that exist. Unknown types will
/// produce errors and be replaced with `Ty::Error` to suppress downstream
/// type mismatches.
///
/// The `globals` parameter provides types for top-level functions, allowing
/// function calls to be properly typed. Pass `None` if no global context is needed.
///
/// The `sig_source_map` parameter is optional. When provided, type mismatch errors
/// will include a secondary location pointing to the return type annotation.
/// When `None`, errors still point to the offending expression but without the
/// return type annotation location. Pass `None` for cached queries to enable
/// Salsa early cutoff on whitespace/comment changes.
#[allow(clippy::too_many_arguments)]
pub fn infer_function<'db>(
    db: &'db dyn Db,
    signature: &FunctionSignature,
    sig_source_map: Option<&SignatureSourceMap>,
    body: &FunctionBody,
    globals: Option<HashMap<Name, Ty>>,
    class_fields: Option<HashMap<Name, HashMap<Name, Ty>>>,
    type_aliases: Option<HashMap<Name, Ty>>,
    enum_variants: Option<HashMap<Name, Vec<Name>>>,
    function_loc: FunctionLoc<'db>,
) -> InferenceResult {
    // Compiler-generated functions (e.g., client resolve) now go through type inference.
    // With Ty::BuiltinUnknown, builtins can accept heterogeneous values.

    let project = db.project();
    let type_aliases = type_aliases.unwrap_or_default();
    let type_alias_name_set: HashSet<Name> = type_aliases.keys().cloned().collect();

    // Get class and enum name sets for type resolution (Salsa-cached)
    let class_name_set = class_names(db, project).names(db).clone();
    let enum_name_set = enum_names(db, project).names(db).clone();

    let file_id = function_loc.file(db).file_id(db);

    let mut type_errors: Vec<TirTypeError> = Vec::new();

    // Convert parameter TypeRefs to Tys with validation and resolution
    // Use type spans from the source map when available for accurate error locations
    let param_types: HashMap<Name, Ty> = signature
        .params
        .iter()
        .enumerate()
        .map(|(idx, param)| {
            // Get the type span from SignatureSourceMap if available (just the type, not the whole param)
            let span = sig_source_map
                .and_then(|sm| sm.param_type_span(idx))
                .map(|range| Span::new(file_id, range))
                .unwrap_or_default();
            let (ty, errors) = lower_type_ref(
                &param.type_ref,
                &type_alias_name_set,
                &class_name_set,
                &enum_name_set,
                span,
            );
            type_errors.extend(errors);
            (param.name.clone(), ty)
        })
        .collect();

    // Convert return type with validation and resolution
    // Use span from the source map when available
    let return_type_span = sig_source_map
        .and_then(SignatureSourceMap::return_type_span)
        .map(|range| Span::new(file_id, range))
        .unwrap_or_default();
    let (expected_return, errors) = lower_type_ref(
        &signature.return_type,
        &type_alias_name_set,
        &class_name_set,
        &enum_name_set,
        return_type_span,
    );
    type_errors.extend(errors);

    // Lower declared throws to fact set (bypasses Ty lattice for variant precision)
    let declared_throws = signature.throws.as_ref().map(|throws_type_ref| {
        let enum_variants_map = crate::enum_variants(db, project).enums(db).clone();
        let alias_refs = collect_type_alias_refs(db, project);
        let facts = lower_throws_to_facts(
            throws_type_ref,
            &enum_variants_map,
            &enum_name_set,
            &alias_refs,
        );
        (facts, return_type_span)
    });

    // Validate map key types in function signature
    // Check return type for invalid map keys (only if we have a valid span)
    if return_type_span != Span::default() {
        let invalid_return_keys = normalize::find_invalid_map_keys(&expected_return, &type_aliases);
        for invalid_key in invalid_return_keys {
            type_errors.push(TypeError::InvalidMapKeyType {
                ty: invalid_key,
                location: ErrorLocation::Span(return_type_span),
            });
        }
    }

    // Check param types for invalid map keys
    if let Some(source_map) = sig_source_map {
        for (idx, param) in signature.params.iter().enumerate() {
            if let Some(param_ty) = param_types.get(&param.name) {
                if let Some(range) = source_map.param_type_span(idx) {
                    let span = Span::new(file_id, range);
                    let invalid_param_keys =
                        normalize::find_invalid_map_keys(param_ty, &type_aliases);
                    for invalid_key in invalid_param_keys {
                        type_errors.push(TypeError::InvalidMapKeyType {
                            ty: invalid_key,
                            location: ErrorLocation::Span(span),
                        });
                    }
                }
            }
        }
    }

    // Delegate to the body inference function
    // Convert return_type_span to Option (None if default/empty)
    let return_type_span_opt = if return_type_span == Span::default() {
        None
    } else {
        Some(return_type_span)
    };
    let mut result = infer_function_body(
        db,
        body,
        param_types,
        &expected_return,
        return_type_span_opt,
        globals,
        class_fields,
        Some(type_aliases),
        enum_variants,
        Some(class_name_set),
        Some(enum_name_set),
        Some(type_alias_name_set),
        function_loc,
        declared_throws.as_ref(),
    );

    // Prepend type lowering errors to the result
    // (they should appear before type checking errors)
    type_errors.extend(result.errors);
    result.errors = type_errors;

    result
}

/// Infer the type of an expression (synthesize mode).
fn infer_expr(ctx: &mut TypeContext<'_>, expr_id: ExprId, body: &ExprBody) -> Ty {
    use baml_compiler_hir::Expr;

    let expr = &body.exprs[expr_id];

    // Use position-independent location for errors - resolved to spans at render time
    let location = ErrorLocation::Expr(expr_id);

    let ty = match expr {
        Expr::Literal(lit) => infer_literal(lit),

        Expr::Path(segments) => {
            if segments.is_empty() {
                Ty::Unknown { attr: d() }
            } else if segments.len() == 1 {
                // Single segment: variable, function, class, or enum lookup
                let name = &segments[0];
                if let Some(ty) = ctx.lookup(name) {
                    let ty = ty.clone();

                    // Determine the resolution based on what kind of entity this is
                    // Check if it's a local variable first (has definition site or found in a local scope)
                    let resolution = if let Some(definition_site) = ctx.get_definition_site(name) {
                        // Has a definition site -> it's a local variable or parameter
                        ResolvedValue::Local {
                            name: name.clone(),
                            definition_site: Some(definition_site),
                        }
                    } else if ctx.is_in_local_scope(name) {
                        // In a local scope but no definition site — this is a scope-narrowed
                        // variable (e.g. scrutinee narrowed inside a match/catch arm).
                        ResolvedValue::Local {
                            name: name.clone(),
                            definition_site: None,
                        }
                    } else if let Some(qn) = ctx.class_names.get(name) {
                        // Class name (in global scope)
                        ResolvedValue::Class(qn.clone())
                    } else if let Some(qn) = ctx.enum_names.get(name) {
                        // Enum name (in global scope)
                        ResolvedValue::Enum(qn.clone())
                    } else if ctx.type_aliases.contains_key(name) {
                        // Type alias (in global scope)
                        use baml_compiler_hir::QualifiedName;
                        ResolvedValue::TypeAlias(QualifiedName::local(name.clone()))
                    } else {
                        // Must be a function in globals
                        use baml_compiler_hir::QualifiedName;
                        ResolvedValue::Function(QualifiedName::local(name.clone()))
                    };

                    // Store resolution for IDE features
                    ctx.set_expr_resolution(expr_id, resolution);
                    ty
                } else {
                    ctx.push_error(TypeError::UnknownVariable {
                        name: name.to_string(),
                        location,
                    });
                    Ty::Unknown { attr: d() }
                }
            } else {
                // Multi-segment path: use HIR name resolution first, then
                // fall back to variable + field access chain for unresolved paths.
                let project = ctx.db.project();
                if let Some(resolution) =
                    baml_compiler_hir::path_resolve::resolve_path(ctx.db, project, segments)
                {
                    use baml_compiler_hir::path_resolve::PathResolution;
                    match resolution {
                        PathResolution::BuiltinFunction(qn) => {
                            ctx.set_expr_resolution(
                                expr_id,
                                ResolvedValue::BuiltinFunction(qn.clone()),
                            );
                            // Look up builtin signature and compute function type
                            let full_path = qn.display();
                            let ty = if let Some(def) =
                                builtins::lookup_builtin_by_path(full_path.as_str())
                            {
                                let mut param_types: Vec<(Option<Name>, Ty)> = Vec::new();
                                if let Some(ref receiver_pattern) = def.receiver {
                                    param_types.push((
                                        None,
                                        builtins::substitute_unknown(receiver_pattern),
                                    ));
                                }
                                for (param_name, pattern) in &def.params {
                                    param_types.push((
                                        Some(Name::new(*param_name)),
                                        builtins::substitute_unknown(pattern),
                                    ));
                                }
                                let return_type = builtins::substitute_unknown(&def.returns);
                                Ty::Function {
                                    params: param_types,
                                    ret: Box::new(return_type),
                                    attr: d(),
                                }
                            } else {
                                Ty::Unknown { attr: d() }
                            };
                            ctx.set_expr_type(expr_id, ty.clone());
                            return ty;
                        }
                        PathResolution::Function(qn) => {
                            ctx.set_expr_resolution(expr_id, ResolvedValue::Function(qn.clone()));
                            let path_name = qn.display_name();
                            let Some(ty) = ctx.lookup(&path_name).cloned() else {
                                return Ty::Unknown { attr: d() };
                            };
                            ctx.set_expr_type(expr_id, ty.clone());
                            return ty;
                        }
                        PathResolution::EnumVariant { enum_fqn, variant } => {
                            ctx.set_expr_resolution(
                                expr_id,
                                ResolvedValue::EnumVariant {
                                    enum_fqn: enum_fqn.clone(),
                                    variant: variant.clone(),
                                },
                            );
                            let enum_name = enum_fqn.display_name();
                            ctx.enum_variant_exprs.insert(expr_id, (enum_name, variant));
                            let ty = Ty::Enum(enum_fqn, d());
                            ctx.set_expr_type(expr_id, ty.clone());
                            return ty;
                        }
                    }
                }

                // Unresolved: treat as variable + field accesses
                let first = &segments[0];
                let mut ty = if let Some(t) = ctx.lookup(first) {
                    t.clone()
                } else {
                    ctx.push_error(TypeError::UnknownVariable {
                        name: first.to_string(),
                        location,
                    });
                    return Ty::Unknown { attr: d() };
                };

                // Record segment types and resolutions for codegen
                let mut segment_types = vec![ty.clone()];
                let mut segment_resolutions = Vec::new();

                // Resolution for the first segment (local variable)
                let first_resolution = if let Some(definition_site) = ctx.get_definition_site(first)
                {
                    ResolvedValue::Local {
                        name: first.clone(),
                        definition_site: Some(definition_site),
                    }
                } else if ctx.is_in_local_scope(first) {
                    ResolvedValue::Local {
                        name: first.clone(),
                        definition_site: None,
                    }
                } else {
                    // In globals (function name) - but this case shouldn't happen
                    // for multi-segment paths that become field accesses
                    ResolvedValue::Local {
                        name: first.clone(),
                        definition_site: None,
                    }
                };
                segment_resolutions.push(first_resolution);

                // Apply field accesses for remaining segments
                for field in &segments[1..] {
                    // Infer the field type first to distinguish methods from fields
                    let field_ty = infer_field_access(ctx, &ty, field, location.clone(), None);

                    // Build resolution for this segment based on base type and field type
                    let segment_resolution = if let Some((def, _bindings)) =
                        builtins::lookup_method(&ty, field.as_str())
                    {
                        // Method reference on a builtin type
                        ResolvedValue::BuiltinFunction(baml_base::QualifiedName::from_builtin_path(
                            def.path,
                        ))
                    } else if let Ty::Class(class_fqn, _) = &ty {
                        // Check if this is a method (function type) or a data field
                        if matches!(field_ty, Ty::Function { .. }) {
                            // Method reference - use qualified name
                            let method_qn = baml_base::QualifiedName {
                                namespace: class_fqn.namespace.clone(),
                                name: baml_base::QualifiedName::local_method_from_str(
                                    class_fqn.name.as_str(),
                                    field.as_str(),
                                ),
                            };
                            ctx.set_expr_resolution(
                                expr_id,
                                ResolvedValue::Function(method_qn.clone()),
                            );
                            ResolvedValue::Function(method_qn)
                        } else {
                            // Data field access
                            ctx.set_expr_resolution(
                                expr_id,
                                ResolvedValue::Field {
                                    class_fqn: class_fqn.clone(),
                                    field: field.clone(),
                                },
                            );
                            ResolvedValue::Field {
                                class_fqn: class_fqn.clone(),
                                field: field.clone(),
                            }
                        }
                    } else {
                        // Generic field access (shouldn't normally happen)
                        ResolvedValue::Local {
                            name: field.clone(),
                            definition_site: None,
                        }
                    };
                    segment_resolutions.push(segment_resolution);

                    ty = field_ty;
                    segment_types.push(ty.clone());
                }

                // Store segment types and resolutions for this path expression
                ctx.path_segment_types.insert(expr_id, segment_types);
                ctx.path_segment_resolutions
                    .insert(expr_id, segment_resolutions);

                ty
            }
        }

        Expr::Binary { lhs, op, rhs } => {
            // Special case: instanceof operator - RHS is a type reference, not an expression
            if *op == baml_compiler_hir::BinaryOp::Instanceof {
                let _lhs_ty = infer_expr(ctx, *lhs, body);
                // For instanceof, don't try to resolve RHS as a variable.
                // The RHS is a type name and will be resolved at runtime.
                // Just return bool since instanceof always returns a boolean.
                Ty::Bool { attr: d() }
            } else {
                let lhs_ty = infer_expr(ctx, *lhs, body);
                let rhs_ty = infer_expr(ctx, *rhs, body);
                infer_binary_op(ctx, *op, &lhs_ty, &rhs_ty, location)
            }
        }

        Expr::Unary { op, expr: inner } => {
            let inner_ty = infer_expr(ctx, *inner, body);
            infer_unary_op(ctx, *op, &inner_ty, location)
        }

        Expr::Call { callee, args } => {
            // Check if this is a method call (callee is a FieldAccess or multi-segment Path)
            // If so, we need to pass the receiver as the first argument.
            // We track (type, Option<span>) for each argument so we can report errors
            // at the correct location. Implicit receiver args have None for span.
            let (callee_ty, effective_args): (Ty, Vec<(Ty, Option<ErrorLocation>)>) = match &body
                .exprs[*callee]
            {
                Expr::FieldAccess { base, field } => {
                    // Method call: receiver.method(args) -> Type.method(receiver, args)
                    // This handles complex expressions like `f().method()` or `arr[0].method()`
                    let receiver_ty = infer_expr(ctx, *base, body);

                    // Try builtin method lookup first to handle cases where a field name
                    // collides with a method name (e.g., Response.headers field vs headers() method)
                    let callee_ty = if let Some((def, bindings)) =
                        builtins::lookup_method(&receiver_ty, field.as_str())
                    {
                        // Build the function type from the builtin definition
                        let mut param_types: Vec<(Option<Name>, Ty)> = Vec::new();
                        if def.receiver.is_some() {
                            param_types.push((None, receiver_ty.clone()));
                        }
                        for (param_name, pattern) in &def.params {
                            param_types.push((
                                Some(Name::new(*param_name)),
                                builtins::substitute(pattern, &bindings),
                            ));
                        }
                        let return_type = builtins::substitute(&def.returns, &bindings);
                        let callee_ty = Ty::Function {
                            params: param_types,
                            ret: Box::new(return_type),
                            attr: d(),
                        };
                        // Store the callee type so downstream passes (VIR, MIR) can find it
                        ctx.set_expr_type(*callee, callee_ty.clone());
                        // Store the resolution for the callee (builtin method reference)
                        ctx.set_expr_resolution(
                            *callee,
                            ResolvedValue::BuiltinFunction(
                                baml_base::QualifiedName::from_builtin_path(def.path),
                            ),
                        );
                        callee_ty
                    } else {
                        // Fall back to normal field access inference (which may find a class field)
                        infer_expr(ctx, *callee, body)
                    };

                    // Build effective args: [(receiver_type, None), ...explicit_args with spans]
                    let mut effective_args = vec![(receiver_ty, None)];
                    for arg in args {
                        let arg_ty = infer_expr(ctx, *arg, body);
                        let arg_location = Some(ErrorLocation::Expr(*arg));
                        effective_args.push((arg_ty, arg_location));
                    }
                    (callee_ty, effective_args)
                }
                Expr::Path(segments) if segments.len() >= 2 => {
                    // First, check if this is a direct builtin function call
                    // (e.g., baml.Array.length(arr), baml.deep_copy(x))
                    let full_path = segments
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join(".");
                    if let Some(def) = builtins::lookup_builtin_by_path(&full_path) {
                        // It's a builtin function called via Path (e.g., baml.Array.length(arr)).
                        // For Path-based calls, the receiver (if any) is passed as an explicit
                        // argument, unlike FieldAccess where it's implicit.
                        //
                        // Use bidirectional type checking:
                        // 1. First infer argument types to extract type variable bindings
                        // 2. Compute expected parameter types using bindings
                        // 3. Re-check arguments with expected types (bidirectional checking)

                        // Phase 1: Infer argument types for type variable binding
                        let inferred_arg_types: Vec<Ty> =
                            args.iter().map(|arg| infer_expr(ctx, *arg, body)).collect();

                        // Build all parameter patterns including receiver (for Path-based calls,
                        // the receiver is passed as an explicit argument)
                        let mut all_param_patterns: Vec<&baml_builtins::TypePattern> = Vec::new();
                        if let Some(ref receiver_pattern) = def.receiver {
                            all_param_patterns.push(receiver_pattern);
                        }
                        for (_, pattern) in &def.params {
                            all_param_patterns.push(pattern);
                        }

                        // Try to match each argument against its parameter pattern to extract bindings
                        let mut bindings = builtins::Bindings::new();
                        for (arg_ty, param_pattern) in
                            inferred_arg_types.iter().zip(all_param_patterns.iter())
                        {
                            if let Some(new_bindings) =
                                builtins::match_pattern(param_pattern, arg_ty)
                            {
                                // Merge bindings (first binding wins for consistency)
                                for (name, ty) in new_bindings {
                                    bindings.entry(name).or_insert(ty);
                                }
                            }
                        }

                        // Phase 2: Compute expected parameter types using bindings
                        let param_types_only: Vec<Ty> = all_param_patterns
                            .iter()
                            .map(|p| {
                                if bindings.is_empty() {
                                    builtins::substitute_unknown(p)
                                } else {
                                    substitute_with_fallback(p, &bindings)
                                }
                            })
                            .collect();

                        // Phase 3: Re-check arguments with expected types (bidirectional checking)
                        // This allows empty maps/arrays to pick up their expected types
                        let arg_types_with_spans: Vec<(Ty, Option<ErrorLocation>)> = args
                            .iter()
                            .zip(param_types_only.iter())
                            .map(|(arg, expected_ty)| {
                                let ty = check_expr(ctx, *arg, body, expected_ty);
                                let arg_location = Some(ErrorLocation::Expr(*arg));
                                (ty, arg_location)
                            })
                            .collect();

                        let return_type = if bindings.is_empty() {
                            builtins::substitute_unknown(&def.returns)
                        } else {
                            substitute_with_fallback(&def.returns, &bindings)
                        };

                        // Build params with names for Ty::Function
                        let mut params: Vec<(Option<Name>, Ty)> = Vec::new();
                        let mut ty_iter = param_types_only.into_iter();
                        if def.receiver.is_some() {
                            if let Some(ty) = ty_iter.next() {
                                params.push((None, ty));
                            }
                        }
                        for ((param_name, _), ty) in def.params.iter().zip(ty_iter) {
                            params.push((Some(Name::new(*param_name)), ty));
                        }

                        let callee_ty = Ty::Function {
                            params,
                            ret: Box::new(return_type),
                            attr: d(),
                        };
                        // Store the callee type so downstream passes (VIR, MIR) can find it
                        ctx.set_expr_type(*callee, callee_ty.clone());
                        // Store the resolution for the callee expression (needed by MIR)
                        ctx.set_expr_resolution(
                            *callee,
                            ResolvedValue::BuiltinFunction(
                                baml_base::QualifiedName::from_builtin_path(def.path),
                            ),
                        );
                        (callee_ty, arg_types_with_spans)
                    } else if ctx.lookup(&Name::new(&full_path)).is_some() {
                        // BAML-defined function in a namespace (e.g., baml.llm.call_llm_function).
                        // These are stored in globals with their qualified name.
                        // Treat as a regular function call (no implicit receiver).
                        let callee_ty = infer_expr(ctx, *callee, body);

                        // Use bidirectional typing: check args against expected param types
                        // so empty maps/arrays pick up their expected types.
                        let param_types: Vec<Ty> = match &callee_ty {
                            Ty::Function { params, .. } => {
                                params.iter().map(|(_, ty)| ty.clone()).collect()
                            }
                            _ => Vec::new(),
                        };
                        let arg_types_with_spans: Vec<(Ty, Option<ErrorLocation>)> = args
                            .iter()
                            .enumerate()
                            .map(|(i, arg)| {
                                let ty = if let Some(expected) = param_types.get(i) {
                                    check_expr(ctx, *arg, body, expected)
                                } else {
                                    infer_expr(ctx, *arg, body)
                                };
                                let arg_location = Some(ErrorLocation::Expr(*arg));
                                (ty, arg_location)
                            })
                            .collect();
                        (callee_ty, arg_types_with_spans)
                    } else {
                        // Method call via Path: `receiver.method(args)`
                        // For multi-segment paths like `baz.Greeting()`, the first segment(s)
                        // form the receiver and the last segment is the method name.
                        //
                        // We infer the receiver type from all segments except the last,
                        // then look up the method on that type.
                        let receiver_segments = &segments[..segments.len() - 1];

                        // Infer receiver type (could be single var or nested field access)
                        let receiver_ty = if receiver_segments.len() == 1 {
                            // Simple receiver: `baz.method()`
                            ctx.lookup(&receiver_segments[0])
                                .cloned()
                                .unwrap_or(Ty::Unknown { attr: d() })
                        } else {
                            // Nested receiver: `obj.field.method()`
                            let first = &receiver_segments[0];
                            let mut ty = ctx
                                .lookup(first)
                                .cloned()
                                .unwrap_or(Ty::Unknown { attr: d() });
                            for field in &receiver_segments[1..] {
                                ty = infer_field_access(ctx, &ty, field, location.clone(), None);
                            }
                            ty
                        };

                        let callee_ty = infer_expr(ctx, *callee, body);

                        // Build effective args: [(receiver_type, None), ...explicit_args with spans]
                        let mut effective_args = vec![(receiver_ty, None)];
                        for arg in args {
                            let arg_ty = infer_expr(ctx, *arg, body);
                            let arg_location = Some(ErrorLocation::Expr(*arg));
                            effective_args.push((arg_ty, arg_location));
                        }
                        (callee_ty, effective_args)
                    }
                }
                _ => {
                    // Regular function call (single-segment Path or other expression)
                    let callee_ty = infer_expr(ctx, *callee, body);
                    let arg_types_with_spans: Vec<(Ty, Option<ErrorLocation>)> = args
                        .iter()
                        .map(|arg| {
                            let ty = infer_expr(ctx, *arg, body);
                            let arg_location = Some(ErrorLocation::Expr(*arg));
                            (ty, arg_location)
                        })
                        .collect();
                    (callee_ty, arg_types_with_spans)
                }
            };

            // If the callee is a function type, check arguments and return the return type
            match &callee_ty {
                Ty::Function { params, ret, .. } => {
                    // Check argument count
                    if effective_args.len() != params.len() {
                        ctx.push_error(TypeError::ArgumentCountMismatch {
                            expected: params.len(),
                            found: effective_args.len(),
                            location: location.clone(),
                        });
                    }

                    // Check argument types - use each argument's location for precise error location
                    for ((arg_ty, arg_location), (_, param_ty)) in
                        effective_args.iter().zip(params.iter())
                    {
                        if !ctx.is_subtype_of(arg_ty, param_ty) {
                            // Use the argument's location if available, otherwise fall back to call location
                            let error_location =
                                arg_location.clone().unwrap_or_else(|| location.clone());
                            ctx.push_error(TypeError::TypeMismatch {
                                expected: param_ty.clone(),
                                found: generalize_for_error(param_ty, arg_ty),
                                location: error_location,
                                info_location: None,
                            });
                        }
                    }

                    // Return the function's return type, refined to `never` when the
                    // callee is known to always diverge.
                    if let Some(target_name) =
                        divergence::call_target_from_callee_expr(*callee, body)
                    {
                        if is_function_always_diverging(ctx, &target_name) {
                            Ty::Never { attr: d() }
                        } else {
                            (**ret).clone()
                        }
                    } else {
                        (**ret).clone()
                    }
                }
                Ty::Unknown { .. } => Ty::Unknown { attr: d() },
                _ => {
                    ctx.push_error(TypeError::NotCallable {
                        ty: callee_ty,
                        location,
                    });
                    Ty::Unknown { attr: d() }
                }
            }
        }

        Expr::FieldAccess { base, field } => {
            // Special validation for $watch accessor
            if field.as_str() == "$watch" {
                // $watch can only be used on simple variable expressions
                let base_expr = &body.exprs[*base];
                match base_expr {
                    Expr::Path(segments) if segments.len() == 1 => {
                        // Simple variable - check if it's declared as watched
                        let var_name = &segments[0];
                        if !ctx.is_watched(var_name) {
                            ctx.push_error(TypeError::WatchOnUnwatchedVariable {
                                name: var_name.to_string(),
                                location: location.clone(),
                            });
                        }
                    }
                    _ => {
                        // Not a simple variable (e.g., arr[0].$watch, obj.field.$watch)
                        ctx.push_error(TypeError::WatchOnNonVariable {
                            location: location.clone(),
                        });
                    }
                }
            }

            let base_ty = infer_expr(ctx, *base, body);
            infer_field_access(ctx, &base_ty, field, location, Some(expr_id))
        }

        Expr::Index { base, index } => {
            let base_ty = infer_expr(ctx, *base, body);
            let index_ty = infer_expr(ctx, *index, body);
            infer_index_access(ctx, &base_ty, &index_ty, location)
        }

        Expr::Array { elements } => {
            if elements.is_empty() {
                Ty::List(Box::new(Ty::Unknown { attr: d() }), d())
            } else {
                // Infer element type from first element, but generalize literals to base types
                // This ensures [1, 2, 3] is int[] not "1"[]
                let first_ty = infer_expr(ctx, elements[0], body);
                let elem_ty = generalize(&first_ty);

                // Infer all element types (for expression type info) but don't emit errors.
                // Errors are reported in check_expr when we have the expected type context.
                // This avoids duplicate errors during bidirectional type checking.
                for &elem in &elements[1..] {
                    infer_expr(ctx, elem, body);
                }
                Ty::List(Box::new(elem_ty), d())
            }
        }

        Expr::Object {
            type_name,
            fields,
            spreads,
        } => {
            // Infer field types
            for (_, value_expr) in fields {
                infer_expr(ctx, *value_expr, body);
            }

            // Determine the expected object type
            let obj_ty = if let Some(name) = type_name {
                ctx.resolve_named_type(name)
            } else {
                Ty::Unknown { attr: d() }
            };

            // Store resolution for IDE features if this is a class instantiation
            if let Some(name) = type_name {
                if let Some(qn) = ctx.class_names.get(name) {
                    ctx.set_expr_resolution(expr_id, ResolvedValue::Class(qn.clone()));
                }
            }

            // Type check spread expressions - they must be the same type as the object
            for spread in spreads {
                let spread_ty = infer_expr(ctx, spread.expr, body);
                // If we have a named type, verify the spread is compatible
                if !matches!(obj_ty, Ty::Unknown { .. }) && !ctx.is_subtype_of(&spread_ty, &obj_ty)
                {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: obj_ty.clone(),
                        found: spread_ty,
                        location: location.clone(),
                        info_location: None,
                    });
                }
            }

            obj_ty
        }

        Expr::Map { entries } => {
            if entries.is_empty() {
                Ty::Map {
                    key: Box::new(Ty::Unknown { attr: d() }),
                    value: Box::new(Ty::Unknown { attr: d() }),
                    attr: d(),
                }
            } else {
                // Infer key and value types from first entry, but generalize literals to base types
                // This ensures {"x": 1} is map<string, int> not map<"x", 1>
                let first_key_ty = infer_expr(ctx, entries[0].0, body);
                let first_value_ty = infer_expr(ctx, entries[0].1, body);
                let key_ty = generalize(&first_key_ty);
                let value_ty = generalize(&first_value_ty);

                // Infer all entry types (for expression type info) but don't emit errors.
                // Errors are reported in check_expr when we have the expected type context.
                // This allows heterogeneous maps to be inferred without errors, then
                // properly type-checked when passed to functions expecting map<K, unknown>.
                for &(key, value) in &entries[1..] {
                    infer_expr(ctx, key, body);
                    infer_expr(ctx, value, body);
                }
                Ty::Map {
                    key: Box::new(key_ty),
                    value: Box::new(value_ty),
                    attr: d(),
                }
            }
        }

        Expr::Block { stmts, tail_expr } => {
            ctx.push_scope();

            // Type check statements, applying narrowing after early-return ifs
            for &stmt_id in stmts {
                check_stmt(ctx, stmt_id, body);

                for (var_name, narrowed_ty) in extract_early_return_narrowing(ctx, stmt_id, body) {
                    ctx.define(var_name, narrowed_ty);
                }
            }

            // Preserve diagnostics in unreachable tails by still inferring/checking
            // the tail expression, but force block type to `never` if any prior
            // statement definitely diverges.
            let block_diverges = block_always_diverges(ctx, stmts, body);
            let result = if let Some(tail) = tail_expr {
                let tail_ty = infer_expr(ctx, *tail, body);
                if block_diverges {
                    Ty::Never { attr: d() }
                } else {
                    tail_ty
                }
            } else {
                block_without_tail_type(ctx, stmts, body)
            };

            ctx.pop_scope();
            result
        }

        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            // Condition: accept any type (truthiness check), not just bool
            infer_expr(ctx, *condition, body);

            // Apply true-branch narrowing (instanceof + null checks + truthiness)
            let true_narrowings = extract_condition_narrowing(ctx, *condition, body, true);
            let then_ty = if !true_narrowings.is_empty() {
                ctx.push_scope();
                for (var_name, narrowed_ty) in true_narrowings {
                    ctx.define(var_name, narrowed_ty);
                }
                let ty = infer_expr(ctx, *then_branch, body);
                ctx.pop_scope();
                ty
            } else {
                infer_expr(ctx, *then_branch, body)
            };

            // Apply false-branch narrowing to else
            let false_narrowings = extract_condition_narrowing(ctx, *condition, body, false);
            let else_ty = if let Some(else_expr) = else_branch {
                if !false_narrowings.is_empty() {
                    ctx.push_scope();
                    for (var_name, narrowed_ty) in false_narrowings {
                        ctx.define(var_name, narrowed_ty);
                    }
                    let ty = infer_expr(ctx, *else_expr, body);
                    ctx.pop_scope();
                    ty
                } else {
                    infer_expr(ctx, *else_expr, body)
                }
            } else {
                Ty::Void { attr: d() }
            };

            // Generalize literal types for the result, similar to arrays.
            // This ensures `if (c) { 1 } else { 2 }` is `int` not `1 | 2`.
            let then_ty = generalize(&then_ty);
            let else_ty = if else_branch.is_none() {
                // An if-without-else implicitly produces null on the false path.
                Ty::Null { attr: d() }
            } else {
                generalize(&else_ty)
            };

            // Never is the bottom type: never | T = T, so a diverging
            // branch doesn't contribute to the result type.
            if matches!(then_ty, Ty::Never { .. }) {
                else_ty
            } else if matches!(else_ty, Ty::Never { .. }) || then_ty == else_ty {
                then_ty
            } else {
                Ty::Union(vec![then_ty, else_ty], d())
            }
        }

        // Match expressions synthesize a type.
        Expr::Match {
            scrutinee,
            scrutinee_type,
            arms,
        } => {
            // Infer the scrutinee expression (needed for variable resolution / side effects)
            let inferred_ty = infer_expr(ctx, *scrutinee, body);
            // If there's an explicit type annotation, use it; otherwise use inferred type
            let scrutinee_ty = if let Some(type_id) = scrutinee_type {
                let type_ref = &body.types[*type_id];
                let span = ctx.type_span(*type_id);
                ctx.lower_type(type_ref, span)
            } else {
                inferred_ty
            };

            if arms.is_empty() {
                // Empty match is non-exhaustive (unless scrutinee is uninhabited).
                // An uninhabited type has no possible values, so an empty match is
                // actually exhaustive—there are no cases to handle.
                // See `Ty::is_uninhabited()` for the full definition and rationale.
                if !scrutinee_ty.is_uninhabited() {
                    ctx.push_error(TypeError::NonExhaustiveMatch {
                        scrutinee_type: scrutinee_ty.clone(),
                        missing_cases: vec!["all cases".to_string()],
                        location: ErrorLocation::Expr(expr_id),
                    });
                }
                Ty::Unknown { attr: d() }
            } else {
                let arms_and_patterns: Vec<(MatchArmId, PatId)> = arms
                    .iter()
                    .map(|arm_id| (*arm_id, body.match_arms[*arm_id].pattern))
                    .collect();
                // Perform exhaustiveness checking and unreachable arm detection
                check_match_exhaustiveness(ctx, &scrutinee_ty, &arms_and_patterns, body, expr_id);

                // Collect result types from all arms
                let arm_types: Vec<Ty> = arms
                    .iter()
                    .map(|arm_id| {
                        let arm = &body.match_arms[*arm_id];

                        // Push a scope for the arm's pattern bindings
                        ctx.push_scope();

                        // Extract pattern and determine the narrowed type
                        let pattern = &body.patterns[arm.pattern];
                        let (binding_name, narrowed_ty) =
                            extract_pattern_binding(ctx, pattern, arm.pattern, &scrutinee_ty, body);

                        // Narrow the scrutinee variable itself within this arm scope,
                        // so that e.g. `Ok => r.value` sees `r` as `Ok` not `Ok | Err`.
                        if let Expr::Path(segments) = &body.exprs[*scrutinee] {
                            if segments.len() == 1 && narrowed_ty != scrutinee_ty {
                                ctx.define(segments[0].clone(), narrowed_ty.clone());
                            }
                        }

                        if let Some(name) = binding_name {
                            if name.as_str() != "_" {
                                ctx.define_with_site(
                                    name,
                                    narrowed_ty,
                                    DefinitionSite::Pattern(arm.pattern),
                                );
                            }
                        }

                        // Type-check the guard (if present)
                        if let Some(guard) = arm.guard {
                            let guard_ty = infer_expr(ctx, guard, body);
                            if !ctx.is_subtype_of(&guard_ty, &Ty::Bool { attr: d() })
                                && !guard_ty.is_unknown()
                            {
                                ctx.push_error(TypeError::TypeMismatch {
                                    expected: Ty::Bool { attr: d() },
                                    found: guard_ty,
                                    location: location.clone(),
                                    info_location: None,
                                });
                            }
                        }

                        // Type-check the arm body
                        let body_ty = infer_expr(ctx, arm.body, body);

                        ctx.pop_scope();
                        body_ty
                    })
                    .collect();

                normalize_union_members(arm_types)
            }
        }

        Expr::Catch { base, clauses } => {
            use baml_compiler_hir::CatchClauseKind;

            let base_ty = infer_expr(ctx, *base, body);
            let mut residual_throw_types = catch_base_throw_types(ctx, *base, &base_ty, body);

            let mut arm_types: Vec<Ty> = Vec::new();

            for clause in clauses {
                ctx.push_scope();

                let clause_scrutinee_ty = throw_types_to_ty(ctx, &residual_throw_types);
                let binding_pat = &body.patterns[clause.binding];
                validate_catch_binding_type(ctx, binding_pat, clause.binding, body);
                let (binding_name, binding_ty) = extract_pattern_binding(
                    ctx,
                    binding_pat,
                    clause.binding,
                    &clause_scrutinee_ty,
                    body,
                );

                // Remember the clause binding name and type for per-arm narrowing
                let clause_binding_name = match binding_pat {
                    Pattern::Binding(name) => Some(name.clone()),
                    Pattern::TypedBinding { name, .. } => Some(name.clone()),
                    _ => None,
                };
                let clause_binding_ty = binding_ty.clone();

                if let Some(ref name) = binding_name {
                    if name.as_str() != "_" {
                        ctx.define_with_site(
                            name.clone(),
                            binding_ty,
                            DefinitionSite::Pattern(clause.binding),
                        );
                    }
                }

                let mut clause_residual = residual_throw_types.clone();

                for arm_id in &clause.arms {
                    let arm = &body.catch_arms[*arm_id];
                    ctx.push_scope();

                    let arm_pat = &body.patterns[arm.pattern];
                    validate_catch_binding_type(ctx, arm_pat, arm.pattern, body);

                    let match_sets =
                        match_throw_types_for_pattern(ctx, arm.pattern, &clause_residual, body);
                    if match_sets.may_match.is_empty() {
                        ctx.push_error(TypeError::UnreachableCatchArm {
                            location: ErrorLocation::Pattern(arm.pattern),
                        });
                    }

                    let arm_scrutinee_ty = throw_types_to_ty(ctx, &match_sets.may_match);
                    let (arm_name, arm_binding_ty) =
                        extract_pattern_binding(ctx, arm_pat, arm.pattern, &arm_scrutinee_ty, body);

                    // Narrow the clause binding variable within this arm
                    if let Some(ref clause_name) = clause_binding_name {
                        if clause_name.as_str() != "_" && arm_scrutinee_ty != clause_binding_ty {
                            ctx.define(clause_name.clone(), arm_scrutinee_ty.clone());
                        }
                    }

                    if let Some(name) = arm_name {
                        if name.as_str() != "_" {
                            ctx.define_with_site(
                                name,
                                arm_binding_ty,
                                DefinitionSite::Pattern(arm.pattern),
                            );
                        }
                    }

                    let arm_ty = infer_expr(ctx, arm.body, body);
                    arm_types.push(arm_ty);
                    ctx.pop_scope();

                    for handled in match_sets.definitely_handled {
                        clause_residual.remove(&handled);
                    }
                }

                residual_throw_types = clause_residual;

                if !residual_throw_types.is_empty()
                    && matches!(clause.kind, CatchClauseKind::CatchAll)
                {
                    ctx.push_error(TypeError::NonExhaustiveCatch {
                        unhandled_types: residual_throw_types.iter().cloned().collect(),
                        location: ErrorLocation::Expr(expr_id),
                    });
                }

                ctx.pop_scope();
            }

            ctx.catch_residual_throws
                .insert(expr_id, residual_throw_types);

            let mut all_types = vec![base_ty];
            all_types.extend(arm_types);
            normalize_union_members(all_types)
        }

        Expr::Throw { value } => {
            infer_expr(ctx, *value, body);
            Ty::Never { attr: d() }
        }

        Expr::Missing => Ty::Unknown { attr: d() },
    };

    ctx.set_expr_type(expr_id, ty.clone());
    ty
}

/// Check that an expression has the expected type (checking mode).
///
/// In bidirectional type checking, checking mode is used when we know what type
/// we expect an expression to have. This allows for better type inference in many
/// cases compared to synthesis followed by subtype checking.
///
/// Returns the actual type of the expression (which should be a subtype of expected).
fn check_expr(ctx: &mut TypeContext<'_>, expr_id: ExprId, body: &ExprBody, expected: &Ty) -> Ty {
    check_expr_with_info_location(ctx, expr_id, body, expected, None)
}

/// Check an expression with an optional location for the type constraint source.
///
/// When `info_location` is provided, type mismatches include a secondary location
/// that points to where the expected type requirement came from (for example,
/// a `let` type annotation).
fn check_expr_with_info_location(
    ctx: &mut TypeContext<'_>,
    expr_id: ExprId,
    body: &ExprBody,
    expected: &Ty,
    info_location: Option<&ErrorLocation>,
) -> Ty {
    use baml_compiler_hir::Expr;

    let expr = &body.exprs[expr_id];
    let location = ErrorLocation::Expr(expr_id);

    let ty = match expr {
        // For most cases, we synthesize then check subtyping
        // But some cases can use the expected type for better inference
        Expr::Block { stmts, tail_expr } => {
            ctx.push_scope();

            // Type check statements, applying narrowing after early-return ifs
            for &stmt_id in stmts {
                check_stmt_with_return(ctx, stmt_id, body, Some(expected));

                for (var_name, narrowed_ty) in extract_early_return_narrowing(ctx, stmt_id, body) {
                    ctx.define(var_name, narrowed_ty);
                }
            }

            // Check tail expression against expected type
            let block_diverges = block_always_diverges(ctx, stmts, body);
            let result = if let Some(tail) = tail_expr {
                let tail_ty = check_expr(ctx, *tail, body, expected);
                if block_diverges {
                    Ty::Never { attr: d() }
                } else {
                    tail_ty
                }
            } else {
                // A tail-less block can still diverge (e.g. `{ throw e }`).
                block_without_tail_type(ctx, stmts, body)
            };

            ctx.pop_scope();
            result
        }

        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            // Condition: accept any type (truthiness check), not just bool
            infer_expr(ctx, *condition, body);

            // Apply true-branch narrowing (instanceof + null checks + truthiness)
            let true_narrowings = extract_condition_narrowing(ctx, *condition, body, true);
            let then_ty = if !true_narrowings.is_empty() {
                ctx.push_scope();
                for (var_name, narrowed_ty) in true_narrowings {
                    ctx.define(var_name, narrowed_ty);
                }
                let ty = check_expr(ctx, *then_branch, body, expected);
                ctx.pop_scope();
                ty
            } else {
                check_expr(ctx, *then_branch, body, expected)
            };

            // Apply false-branch narrowing to else
            let false_narrowings = extract_condition_narrowing(ctx, *condition, body, false);
            let else_ty = if let Some(else_expr) = else_branch {
                if !false_narrowings.is_empty() {
                    ctx.push_scope();
                    for (var_name, narrowed_ty) in false_narrowings {
                        ctx.define(var_name, narrowed_ty);
                    }
                    let ty = check_expr(ctx, *else_expr, body, expected);
                    ctx.pop_scope();
                    ty
                } else {
                    check_expr(ctx, *else_expr, body, expected)
                }
            } else {
                Ty::Null { attr: d() }
            };

            // Never is the bottom type: never | T = T.
            if matches!(then_ty, Ty::Never { .. }) {
                else_ty
            } else if matches!(else_ty, Ty::Never { .. }) || then_ty == else_ty {
                then_ty
            } else {
                Ty::Union(vec![then_ty, else_ty], d())
            }
        }

        Expr::Array { elements } => {
            // If we expect a specific list type, use it to check elements
            if let Ty::List(expected_elem, _) = expected {
                if elements.is_empty() {
                    expected.clone()
                } else {
                    // Check all elements against the expected element type
                    // check_expr already emits type mismatch errors, no need for redundant check
                    for &elem in elements {
                        check_expr(ctx, elem, body, expected_elem);
                    }
                    expected.clone()
                }
            } else {
                // Fall back to synthesis
                let ty = infer_expr(ctx, expr_id, body);
                if !ctx.is_subtype_of(&ty, expected)
                    && !expected.is_unknown()
                    && !expected.is_error()
                {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: expected.clone(),
                        found: generalize_for_error(expected, &ty),
                        location,
                        info_location: info_location.cloned(),
                    });
                }
                ty
            }
        }

        Expr::Object {
            type_name,
            fields,
            spreads: _,
        } => {
            // Store resolution for IDE features if this is a class instantiation
            if let Some(name) = type_name {
                if let Some(qn) = ctx.class_names.get(name) {
                    ctx.set_expr_resolution(expr_id, ResolvedValue::Class(qn.clone()));
                }
            }

            // If we expect a specific class type, we can use its field types
            if let Ty::Class(expected_fqn, _) = expected {
                // Check field types against the expected class fields
                for (field_name, value_expr) in fields {
                    // Clone the field type to avoid borrow issues
                    let class_key = expected_fqn.display_name();
                    let expected_field_ty = ctx.lookup_class_field(&class_key, field_name).cloned();
                    if let Some(field_ty) = expected_field_ty {
                        check_expr(ctx, *value_expr, body, &field_ty);
                    } else {
                        // Field doesn't exist in expected type - still infer it for error reporting
                        infer_expr(ctx, *value_expr, body);
                    }
                }

                // Return the expected type if type_name matches
                if type_name.as_ref() == Some(&expected_fqn.name) {
                    expected.clone()
                } else if let Some(name) = type_name {
                    ctx.resolve_named_type(name)
                } else {
                    Ty::Unknown { attr: d() }
                }
            } else if let Ty::TypeAlias(expected_fqn, _) = expected {
                use baml_compiler_hir::QualifiedName;
                // Similar handling for TypeAlias types
                let alias_key = expected_fqn.display_name();
                for (field_name, value_expr) in fields {
                    let expected_field_ty = ctx.lookup_class_field(&alias_key, field_name).cloned();
                    if let Some(field_ty) = expected_field_ty {
                        check_expr(ctx, *value_expr, body, &field_ty);
                    } else {
                        infer_expr(ctx, *value_expr, body);
                    }
                }

                if type_name.as_ref() == Some(&expected_fqn.name) {
                    expected.clone()
                } else if let Some(name) = type_name {
                    Ty::TypeAlias(QualifiedName::local(name.clone()), d())
                } else {
                    Ty::Unknown { attr: d() }
                }
            } else {
                // Fall back to synthesis
                let ty = infer_expr(ctx, expr_id, body);
                if !ctx.is_subtype_of(&ty, expected)
                    && !expected.is_unknown()
                    && !expected.is_error()
                {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: expected.clone(),
                        found: generalize_for_error(expected, &ty),
                        location,
                        info_location: info_location.cloned(),
                    });
                }
                ty
            }
        }

        Expr::Map { entries } => {
            // If we expect a specific map type, use it to check entries
            if let Ty::Map {
                key: expected_key,
                value: expected_value,
                ..
            } = expected
            {
                if entries.is_empty() {
                    expected.clone()
                } else {
                    // Check all entries against the expected key/value types
                    // check_expr already emits type mismatch errors, no need for redundant check
                    for &(key_expr, value_expr) in entries {
                        check_expr(ctx, key_expr, body, expected_key);
                        check_expr(ctx, value_expr, body, expected_value);
                    }
                    expected.clone()
                }
            } else {
                // Fall back to synthesis
                let ty = infer_expr(ctx, expr_id, body);
                if !ctx.is_subtype_of(&ty, expected)
                    && !expected.is_unknown()
                    && !expected.is_error()
                {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: expected.clone(),
                        found: generalize_for_error(expected, &ty),
                        location,
                        info_location: info_location.cloned(),
                    });
                }
                ty
            }
        }

        // For all other cases, synthesize then check
        _ => {
            let ty = infer_expr(ctx, expr_id, body);
            if !ctx.is_subtype_of(&ty, expected)
                && !expected.is_unknown()
                && !expected.is_error()
                && !ty.is_unknown()
            {
                // Generalize found type for clearer error messages
                // e.g., "Expected int[], found int" instead of "Expected int[], found 42"
                // But preserve literals when expected is also a literal (e.g., "Expected 4, found 3")
                ctx.push_error(TypeError::TypeMismatch {
                    expected: expected.clone(),
                    found: generalize_for_error(expected, &ty),
                    location,
                    info_location: info_location.cloned(),
                });
            }
            ty
        }
    };

    ctx.set_expr_type(expr_id, ty.clone());
    ty
}

/// Infer the type of a block with no tail expression.
///
/// `{ throw e }` and similar blocks do not complete normally and should not
/// be treated as `void`.
fn block_without_tail_type(ctx: &TypeContext<'_>, stmts: &[StmtId], body: &ExprBody) -> Ty {
    if block_always_diverges(ctx, stmts, body) {
        Ty::Never { attr: d() }
    } else {
        Ty::Void { attr: d() }
    }
}

fn is_function_always_diverging(ctx: &TypeContext<'_>, function_name: &Name) -> bool {
    let divergence = function_divergence_set(ctx.db(), ctx.db().project());
    divergence.functions(ctx.db()).contains(function_name)
}

fn expr_always_diverges(db: &dyn Db, expr_id: ExprId, body: &ExprBody) -> bool {
    divergence::expr_definitely_diverges(expr_id, body, &|callee| {
        let divergence = function_divergence_set(db, db.project());
        divergence.functions(db).contains(callee)
    })
}

fn block_always_diverges(ctx: &TypeContext<'_>, stmts: &[StmtId], body: &ExprBody) -> bool {
    divergence::any_stmt_diverges(stmts, body, &|callee| {
        is_function_always_diverging(ctx, callee)
    })
}

fn throw_fact_to_ty(ctx: &TypeContext<'_>, throw_fact: &str) -> Ty {
    match throw_fact {
        "int" => Ty::Int { attr: d() },
        "float" => Ty::Float { attr: d() },
        "string" => Ty::String { attr: d() },
        "bool" => Ty::Bool { attr: d() },
        "null" => Ty::Null { attr: d() },
        "unknown" => Ty::Unknown { attr: d() },
        named => {
            let direct = ctx.resolve_named_type(&Name::new(named));
            if !direct.is_unknown() {
                return direct;
            }

            // Variant facts are stored as `EnumName.Variant` (or namespaced
            // `pkg.EnumName.Variant`), so recover the enum type from the prefix.
            if let Some((type_name, _variant_name)) = named.rsplit_once('.') {
                let from_prefix = ctx.resolve_named_type(&Name::new(type_name));
                if !from_prefix.is_unknown() {
                    return from_prefix;
                }
            }

            Ty::Unknown { attr: d() }
        }
    }
}

fn throw_types_to_ty(ctx: &TypeContext<'_>, throw_types: &BTreeSet<String>) -> Ty {
    let members: Vec<Ty> = throw_types
        .iter()
        .map(|throw_fact| throw_fact_to_ty(ctx, throw_fact))
        .collect();
    normalize_union_members(members)
}

/// Collect type alias name -> `TypeRef` map for resolving aliases in throws clauses.
fn collect_type_alias_refs(
    db: &dyn Db,
    project: Project,
) -> HashMap<Name, baml_compiler_hir::TypeRef> {
    let items = baml_compiler_hir::project_items(db, project);
    let mut alias_refs = HashMap::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::TypeAlias(alias_loc) = item {
            let item_tree = baml_compiler_hir::file_item_tree(db, alias_loc.file(db));
            let alias_data = &item_tree[alias_loc.id(db)];
            alias_refs.insert(alias_data.name.clone(), alias_data.type_ref.clone());
        }
    }

    alias_refs
}

/// Lower a `throws` clause `TypeRef` directly to throw facts.
///
/// Unlike `lower_type_ref` (which produces a Ty), this produces the set of
/// throw-fact strings that the declaration covers. This keeps `throws` in
/// the fact lattice — the same lattice used by catch exhaustiveness — so
/// variant-level precision is preserved.
///
/// Expansion rules:
/// - Primitive names: `string` → {"string"}, `int` → {"int"}, etc.
/// - Enum names: `Errors` → {"Errors.V1", "Errors.V2", ...} (all variants)
/// - Enum variant paths: `Errors.AuthError` → {"Errors.AuthError"}
/// - Other names (classes, etc.): `MyError` → {`"MyError"`}
/// - Type alias names: resolved through to their underlying `TypeRef`
/// - Unions: recurse and merge
/// - `never` → {} (empty set)
fn lower_throws_to_facts(
    type_ref: &baml_compiler_hir::TypeRef,
    enum_variants: &HashMap<Name, Vec<Name>>,
    enum_names: &HashMap<Name, baml_compiler_hir::QualifiedName>,
    type_alias_refs: &HashMap<Name, baml_compiler_hir::TypeRef>,
) -> BTreeSet<String> {
    use baml_compiler_hir::TypeRef;

    let mut facts = BTreeSet::new();
    match type_ref {
        TypeRef::Int => {
            facts.insert("int".into());
        }
        TypeRef::Float => {
            facts.insert("float".into());
        }
        TypeRef::String => {
            facts.insert("string".into());
        }
        TypeRef::Bool => {
            facts.insert("bool".into());
        }
        TypeRef::Null => {
            facts.insert("null".into());
        }
        TypeRef::Never => {}

        TypeRef::Path(path) => match path.segments.len() {
            1 => {
                let name = &path.segments[0];
                if enum_names.contains_key(name) {
                    if let Some(variants) = enum_variants.get(name) {
                        for v in variants {
                            facts.insert(format!("{}.{}", name.as_str(), v.as_str()));
                        }
                    } else {
                        facts.insert(name.as_str().into());
                    }
                } else if let Some(alias_ref) = type_alias_refs.get(name) {
                    facts.extend(lower_throws_to_facts(
                        alias_ref,
                        enum_variants,
                        enum_names,
                        type_alias_refs,
                    ));
                } else {
                    facts.insert(name.as_str().into());
                }
            }
            2 => {
                let enum_name = &path.segments[0];
                let variant_name = &path.segments[1];
                facts.insert(format!("{}.{}", enum_name.as_str(), variant_name.as_str()));
            }
            _ => {
                let full = path
                    .segments
                    .iter()
                    .map(smol_str::SmolStr::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                facts.insert(full);
            }
        },

        TypeRef::Union(members) => {
            for m in members {
                facts.extend(lower_throws_to_facts(
                    m,
                    enum_variants,
                    enum_names,
                    type_alias_refs,
                ));
            }
        }

        TypeRef::Optional(inner) => {
            facts.insert("null".into());
            facts.extend(lower_throws_to_facts(
                inner,
                enum_variants,
                enum_names,
                type_alias_refs,
            ));
        }

        TypeRef::StringLiteral(_) => {
            facts.insert("string".into());
        }
        TypeRef::IntLiteral(_) => {
            facts.insert("int".into());
        }
        TypeRef::FloatLiteral(_) => {
            facts.insert("float".into());
        }
        TypeRef::BoolLiteral(_) => {
            facts.insert("bool".into());
        }

        _ => {
            facts.insert("unknown".into());
        }
    }
    facts
}

/// Build a canonical union type:
/// - removes `never` members (bottom type)
/// - deduplicates remaining members preserving order
/// - returns bare member when cardinality is 1
fn normalize_union_members(mut members: Vec<Ty>) -> Ty {
    members.retain(|ty| !matches!(ty, Ty::Never { .. }));

    let mut deduped: Vec<Ty> = Vec::new();
    for ty in members {
        if !deduped.contains(&ty) {
            deduped.push(ty);
        }
    }

    match deduped.len() {
        0 => Ty::Never { attr: d() },
        1 => deduped
            .into_iter()
            .next()
            .unwrap_or(Ty::Never { attr: d() }),
        _ => Ty::Union(deduped, d()),
    }
}

fn literal_throw_fact(lit: &baml_compiler_hir::Literal) -> &'static str {
    match lit {
        baml_compiler_hir::Literal::Int(_) => "int",
        baml_compiler_hir::Literal::Float(_) => "float",
        baml_compiler_hir::Literal::String(_) => "string",
        baml_compiler_hir::Literal::Bool(_) => "bool",
        baml_compiler_hir::Literal::Null => "null",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThrowPatternMatch {
    No,
    Maybe,
    Definite,
}

impl ThrowPatternMatch {
    fn is_match(self) -> bool {
        !matches!(self, ThrowPatternMatch::No)
    }

    fn is_definite(self) -> bool {
        matches!(self, ThrowPatternMatch::Definite)
    }
}

#[derive(Debug, Default)]
struct ThrowPatternMatchSets {
    may_match: BTreeSet<String>,
    definitely_handled: BTreeSet<String>,
}

fn throw_fact_matches_named_type(throw_fact: &str, type_name: &str) -> bool {
    throw_fact == type_name
        || throw_fact
            .strip_prefix(type_name)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn ty_matches_throw_fact(ty: &Ty, throw_fact: &str) -> bool {
    if throw_fact == "unknown" {
        return true;
    }

    match ty {
        Ty::Int { .. } => throw_fact == "int",
        Ty::Float { .. } => throw_fact == "float",
        Ty::String { .. } => throw_fact == "string",
        Ty::Bool { .. } => throw_fact == "bool",
        Ty::Null { .. } => throw_fact == "null",
        Ty::Literal(lit, _) => match lit {
            LiteralValue::Int(_) => throw_fact == "int",
            LiteralValue::Float(_) => throw_fact == "float",
            LiteralValue::String(_) => throw_fact == "string",
            LiteralValue::Bool(_) => throw_fact == "bool",
        },
        Ty::Optional(inner, _) => throw_fact == "null" || ty_matches_throw_fact(inner, throw_fact),
        Ty::Union(members, _) => members
            .iter()
            .any(|member| ty_matches_throw_fact(member, throw_fact)),
        Ty::Class(qn, _) | Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) => {
            throw_fact_matches_named_type(throw_fact, qn.display_name().as_str())
        }
        Ty::Unknown { .. } | Ty::Error { .. } | Ty::BuiltinUnknown { .. } => true,
        Ty::Never { .. } => false,
        _ => false,
    }
}

fn pattern_match_strength(
    ctx: &mut TypeContext<'_>,
    pattern_id: PatId,
    throw_fact: &str,
    body: &ExprBody,
) -> ThrowPatternMatch {
    let pattern = &body.patterns[pattern_id];
    match pattern {
        Pattern::Binding(_) => ThrowPatternMatch::Definite,
        Pattern::TypedBinding { ty, .. } => {
            let span = ctx.pattern_span(pattern_id);
            let lowered = ctx.lower_type(ty, span);
            if !ty_matches_throw_fact(&lowered, throw_fact) {
                ThrowPatternMatch::No
            } else if throw_fact == "unknown" {
                ThrowPatternMatch::Maybe
            } else {
                let throw_ty = throw_fact_to_ty(ctx, throw_fact);
                if !throw_ty.is_unknown() && ctx.is_subtype_of(&throw_ty, &lowered) {
                    ThrowPatternMatch::Definite
                } else {
                    ThrowPatternMatch::Maybe
                }
            }
        }
        Pattern::Literal(lit) => {
            if throw_fact == "unknown" || literal_throw_fact(lit) == throw_fact {
                ThrowPatternMatch::Maybe
            } else {
                ThrowPatternMatch::No
            }
        }
        Pattern::EnumVariant { enum_name, variant } => {
            let enum_name = enum_name.as_str();
            let variant_fact = format!("{enum_name}.{}", variant.as_str());
            if throw_fact == variant_fact {
                ThrowPatternMatch::Definite
            } else if throw_fact == enum_name || throw_fact == "unknown" {
                ThrowPatternMatch::Maybe
            } else {
                ThrowPatternMatch::No
            }
        }
        Pattern::Union(parts) => {
            let mut saw_maybe = false;
            for part in parts {
                match pattern_match_strength(ctx, *part, throw_fact, body) {
                    ThrowPatternMatch::Definite => return ThrowPatternMatch::Definite,
                    ThrowPatternMatch::Maybe => saw_maybe = true,
                    ThrowPatternMatch::No => {}
                }
            }
            if saw_maybe {
                ThrowPatternMatch::Maybe
            } else {
                ThrowPatternMatch::No
            }
        }
    }
}

fn match_throw_types_for_pattern(
    ctx: &mut TypeContext<'_>,
    pattern_id: PatId,
    residual_throw_types: &BTreeSet<String>,
    body: &ExprBody,
) -> ThrowPatternMatchSets {
    let mut out = ThrowPatternMatchSets::default();
    for throw_fact in residual_throw_types {
        let strength = pattern_match_strength(ctx, pattern_id, throw_fact, body);
        if strength.is_match() {
            out.may_match.insert(throw_fact.clone());
        }
        if strength.is_definite() {
            out.definitely_handled.insert(throw_fact.clone());
        }
    }
    out
}

/// TIR-level counterpart to `throw_inference::throw_fact_from_expr`.
///
/// Operates on fully inferred types (`ctx.expr_types`) rather than raw syntax,
/// so it can resolve variables, function results, and other expressions that the
/// HIR-level pre-pass records as `"unknown"`. Used for local catch-base analysis
/// during type inference.
fn collect_throw_facts_from_value(
    ctx: &TypeContext<'_>,
    value_expr_id: ExprId,
    _body: &ExprBody,
    out: &mut BTreeSet<String>,
) {
    if let Some((enum_name, variant_name)) = ctx.enum_variant_exprs.get(&value_expr_id) {
        out.insert(format!("{}.{}", enum_name.as_str(), variant_name.as_str()));
        return;
    }

    let unknown_ty = Ty::Unknown { attr: d() };
    let thrown_ty = ctx.expr_types.get(&value_expr_id).unwrap_or(&unknown_ty);
    match thrown_ty {
        Ty::Literal(lit, _) => {
            out.insert(match lit {
                LiteralValue::Int(_) => "int".to_string(),
                LiteralValue::Float(_) => "float".to_string(),
                LiteralValue::String(_) => "string".to_string(),
                LiteralValue::Bool(_) => "bool".to_string(),
            });
        }
        Ty::Int { .. } => {
            out.insert("int".to_string());
        }
        Ty::Float { .. } => {
            out.insert("float".to_string());
        }
        Ty::String { .. } => {
            out.insert("string".to_string());
        }
        Ty::Bool { .. } => {
            out.insert("bool".to_string());
        }
        Ty::Null { .. } => {
            out.insert("null".to_string());
        }
        Ty::Class(qn, _) | Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) => {
            out.insert(qn.display_name().as_str().to_string());
        }
        Ty::Union(members, _) => {
            for member in members {
                if let Ty::Class(qn, _) | Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) = member {
                    out.insert(qn.display_name().as_str().to_string());
                }
            }
        }
        Ty::Never { .. } => {
            // Uninhabited — no value of this type exists, so no fact to record.
            // This arises when `throw e` appears inside an unreachable catch arm
            // (the catch binding has type Never because the base never throws).
        }
        _ => {
            out.insert("unknown".to_string());
        }
    }
}

fn collect_throw_facts_from_stmt(
    ctx: &TypeContext<'_>,
    stmt_id: StmtId,
    body: &ExprBody,
    out: &mut BTreeSet<String>,
) {
    use baml_compiler_hir::Stmt;

    match &body.stmts[stmt_id] {
        Stmt::Expr(expr_id) => collect_throw_facts_from_expr(ctx, *expr_id, body, out),
        Stmt::Let { initializer, .. } => {
            if let Some(initializer) = initializer {
                collect_throw_facts_from_expr(ctx, *initializer, body, out);
            }
        }
        Stmt::While {
            condition,
            body: loop_body,
            after,
            ..
        } => {
            collect_throw_facts_from_expr(ctx, *condition, body, out);
            collect_throw_facts_from_expr(ctx, *loop_body, body, out);
            if let Some(after) = after {
                collect_throw_facts_from_stmt(ctx, *after, body, out);
            }
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                collect_throw_facts_from_expr(ctx, *expr, body, out);
            }
        }
        Stmt::Assign { target, value } => {
            collect_throw_facts_from_expr(ctx, *target, body, out);
            collect_throw_facts_from_expr(ctx, *value, body, out);
        }
        Stmt::AssignOp { target, value, .. } => {
            collect_throw_facts_from_expr(ctx, *target, body, out);
            collect_throw_facts_from_expr(ctx, *value, body, out);
        }
        Stmt::Assert { condition } => collect_throw_facts_from_expr(ctx, *condition, body, out),
        Stmt::Throw { value } => {
            collect_throw_facts_from_expr(ctx, *value, body, out);
            collect_throw_facts_from_value(ctx, *value, body, out);
        }
        Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
    }
}

fn collect_throw_facts_from_expr(
    ctx: &TypeContext<'_>,
    expr_id: ExprId,
    body: &ExprBody,
    out: &mut BTreeSet<String>,
) {
    use baml_compiler_hir::Expr;

    match &body.exprs[expr_id] {
        Expr::Throw { value } => {
            collect_throw_facts_from_expr(ctx, *value, body, out);
            collect_throw_facts_from_value(ctx, *value, body, out);
        }
        Expr::Call { callee, args } => {
            collect_throw_facts_from_expr(ctx, *callee, body, out);
            for arg in args {
                collect_throw_facts_from_expr(ctx, *arg, body, out);
            }

            if let Some(function_name) = divergence::call_target_from_callee_expr(*callee, body) {
                let throws = function_throw_sets(ctx.db(), ctx.db().project());
                if let Some(transitive) = throws.transitive(ctx.db()).get(&function_name) {
                    out.extend(transitive.iter().cloned());
                } else if is_function_always_diverging(ctx, &function_name) {
                    out.insert("unknown".to_string());
                }
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_throw_facts_from_expr(ctx, *condition, body, out);
            collect_throw_facts_from_expr(ctx, *then_branch, body, out);
            if let Some(else_branch) = else_branch {
                collect_throw_facts_from_expr(ctx, *else_branch, body, out);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_throw_facts_from_expr(ctx, *scrutinee, body, out);
            for arm_id in arms {
                let arm = &body.match_arms[*arm_id];
                if let Some(guard) = arm.guard {
                    collect_throw_facts_from_expr(ctx, guard, body, out);
                }
                collect_throw_facts_from_expr(ctx, arm.body, body, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_throw_facts_from_expr(ctx, *lhs, body, out);
            collect_throw_facts_from_expr(ctx, *rhs, body, out);
        }
        Expr::Unary { expr, .. } => collect_throw_facts_from_expr(ctx, *expr, body, out),
        Expr::Object {
            fields, spreads, ..
        } => {
            for (_, value) in fields {
                collect_throw_facts_from_expr(ctx, *value, body, out);
            }
            for spread in spreads {
                collect_throw_facts_from_expr(ctx, spread.expr, body, out);
            }
        }
        Expr::Array { elements } => {
            for elem in elements {
                collect_throw_facts_from_expr(ctx, *elem, body, out);
            }
        }
        Expr::Map { entries } => {
            for (key, value) in entries {
                collect_throw_facts_from_expr(ctx, *key, body, out);
                collect_throw_facts_from_expr(ctx, *value, body, out);
            }
        }
        Expr::Block { stmts, tail_expr } => {
            for stmt_id in stmts {
                collect_throw_facts_from_stmt(ctx, *stmt_id, body, out);
            }
            if let Some(tail_expr) = tail_expr {
                collect_throw_facts_from_expr(ctx, *tail_expr, body, out);
            }
        }
        Expr::FieldAccess { base, .. } => collect_throw_facts_from_expr(ctx, *base, body, out),
        Expr::Index { base, index } => {
            collect_throw_facts_from_expr(ctx, *base, body, out);
            collect_throw_facts_from_expr(ctx, *index, body, out);
        }
        Expr::Catch { base, .. } => {
            collect_throw_facts_from_expr(ctx, *base, body, out);
        }
        Expr::Literal(_) | Expr::Path(_) | Expr::Missing => {}
    }
}

fn catch_base_throw_types(
    ctx: &TypeContext<'_>,
    base_expr_id: ExprId,
    base_ty: &Ty,
    body: &ExprBody,
) -> BTreeSet<String> {
    let mut throw_types = BTreeSet::new();
    collect_throw_facts_from_expr(ctx, base_expr_id, body, &mut throw_types);
    if !throw_types.is_empty() {
        return throw_types;
    }

    if matches!(base_ty, Ty::Never { .. }) {
        BTreeSet::from(["unknown".to_string()])
    } else {
        BTreeSet::new()
    }
}

/// Collect the effective throw types that escape a function body, accounting for
/// catch clauses. Unlike `collect_throw_facts_from_expr`, this uses the residual
/// throw types computed during type inference to correctly subtract what catches handle.
fn collect_effective_throws_from_expr(
    ctx: &TypeContext<'_>,
    expr_id: ExprId,
    body: &ExprBody,
    out: &mut BTreeSet<String>,
) {
    use baml_compiler_hir::Expr;

    match &body.exprs[expr_id] {
        Expr::Throw { value } => {
            collect_effective_throws_from_expr(ctx, *value, body, out);
            collect_throw_facts_from_value(ctx, *value, body, out);
        }
        Expr::Call { callee, args } => {
            collect_effective_throws_from_expr(ctx, *callee, body, out);
            for arg in args {
                collect_effective_throws_from_expr(ctx, *arg, body, out);
            }
            if let Some(function_name) = divergence::call_target_from_callee_expr(*callee, body) {
                let throws = function_throw_sets(ctx.db(), ctx.db().project());
                if let Some(transitive) = throws.transitive(ctx.db()).get(&function_name) {
                    out.extend(transitive.iter().cloned());
                } else if is_function_always_diverging(ctx, &function_name) {
                    out.insert("unknown".to_string());
                }
            }
        }
        Expr::Catch { base, .. } => {
            // Use the residual from type inference (catch-aware)
            if let Some(residual) = ctx.catch_residual_throws.get(&expr_id) {
                out.extend(residual.iter().cloned());
            }
            // Still recurse into catch arm bodies for throws within handlers
            if let Expr::Catch { clauses, .. } = &body.exprs[expr_id] {
                for clause in clauses {
                    for arm_id in &clause.arms {
                        let arm = &body.catch_arms[*arm_id];
                        collect_effective_throws_from_expr(ctx, arm.body, body, out);
                    }
                }
            }
            // Don't recurse into base — its throws are accounted for by the residual
            let _ = base;
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_effective_throws_from_expr(ctx, *condition, body, out);
            collect_effective_throws_from_expr(ctx, *then_branch, body, out);
            if let Some(else_branch) = else_branch {
                collect_effective_throws_from_expr(ctx, *else_branch, body, out);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_effective_throws_from_expr(ctx, *scrutinee, body, out);
            for arm_id in arms {
                let arm = &body.match_arms[*arm_id];
                if let Some(guard) = arm.guard {
                    collect_effective_throws_from_expr(ctx, guard, body, out);
                }
                collect_effective_throws_from_expr(ctx, arm.body, body, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_effective_throws_from_expr(ctx, *lhs, body, out);
            collect_effective_throws_from_expr(ctx, *rhs, body, out);
        }
        Expr::Unary { expr, .. } => {
            collect_effective_throws_from_expr(ctx, *expr, body, out);
        }
        Expr::Object {
            fields, spreads, ..
        } => {
            for (_, value) in fields {
                collect_effective_throws_from_expr(ctx, *value, body, out);
            }
            for spread in spreads {
                collect_effective_throws_from_expr(ctx, spread.expr, body, out);
            }
        }
        Expr::Array { elements } => {
            for elem in elements {
                collect_effective_throws_from_expr(ctx, *elem, body, out);
            }
        }
        Expr::Map { entries } => {
            for (key, value) in entries {
                collect_effective_throws_from_expr(ctx, *key, body, out);
                collect_effective_throws_from_expr(ctx, *value, body, out);
            }
        }
        Expr::Block { stmts, tail_expr } => {
            for stmt_id in stmts {
                collect_effective_throws_from_stmt(ctx, *stmt_id, body, out);
            }
            if let Some(tail_expr) = tail_expr {
                collect_effective_throws_from_expr(ctx, *tail_expr, body, out);
            }
        }
        Expr::FieldAccess { base, .. } => {
            collect_effective_throws_from_expr(ctx, *base, body, out);
        }
        Expr::Index { base, index } => {
            collect_effective_throws_from_expr(ctx, *base, body, out);
            collect_effective_throws_from_expr(ctx, *index, body, out);
        }
        Expr::Literal(_) | Expr::Path(_) | Expr::Missing => {}
    }
}

fn collect_effective_throws_from_stmt(
    ctx: &TypeContext<'_>,
    stmt_id: StmtId,
    body: &ExprBody,
    out: &mut BTreeSet<String>,
) {
    use baml_compiler_hir::Stmt;

    match &body.stmts[stmt_id] {
        Stmt::Expr(expr_id) => collect_effective_throws_from_expr(ctx, *expr_id, body, out),
        Stmt::Let { initializer, .. } => {
            if let Some(initializer) = initializer {
                collect_effective_throws_from_expr(ctx, *initializer, body, out);
            }
        }
        Stmt::While {
            condition,
            body: loop_body,
            after,
            ..
        } => {
            collect_effective_throws_from_expr(ctx, *condition, body, out);
            collect_effective_throws_from_expr(ctx, *loop_body, body, out);
            if let Some(after) = after {
                collect_effective_throws_from_stmt(ctx, *after, body, out);
            }
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                collect_effective_throws_from_expr(ctx, *expr, body, out);
            }
        }
        Stmt::Assign { target, value } => {
            collect_effective_throws_from_expr(ctx, *target, body, out);
            collect_effective_throws_from_expr(ctx, *value, body, out);
        }
        Stmt::AssignOp { target, value, .. } => {
            collect_effective_throws_from_expr(ctx, *target, body, out);
            collect_effective_throws_from_expr(ctx, *value, body, out);
        }
        Stmt::Assert { condition } => {
            collect_effective_throws_from_expr(ctx, *condition, body, out);
        }
        Stmt::Throw { value } => {
            collect_effective_throws_from_expr(ctx, *value, body, out);
            collect_throw_facts_from_value(ctx, *value, body, out);
        }
        Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
    }
}

/// Extract binding name and narrowed type from a match pattern.
///
/// Returns `(Some(name), narrowed_type)` for binding patterns, or `(None, scrutinee_type)` for
/// patterns that don't introduce bindings (literals, enum variants, unions).
///
/// # Type Narrowing Rules
/// - `name: Type` binds `name` with type `Type` (from the type annotation)
/// - `name` (without type) binds `name` with the scrutinee type (catch-all)
/// - `_` is a special case of binding that's semantically discarded later
/// - Literals, enum variants, and union patterns don't introduce bindings
fn extract_pattern_binding(
    ctx: &mut TypeContext<'_>,
    pattern: &Pattern,
    pattern_id: PatId,
    scrutinee_ty: &Ty,
    _body: &ExprBody,
) -> (Option<Name>, Ty) {
    let result = match pattern {
        // Typed binding: `s: Success` -> s has type Success
        Pattern::TypedBinding { name, ty } => {
            // Use the pattern's span for type errors (points to where the type is used)
            let pattern_span = ctx.pattern_span(pattern_id);
            let narrowed_ty = ctx.lower_type(ty, pattern_span);
            (Some(name.clone()), narrowed_ty)
        }

        // Simple binding: `x` or `_` -> binds with scrutinee type (catch-all)
        Pattern::Binding(name) => (Some(name.clone()), scrutinee_ty.clone()),

        // Literal patterns don't introduce bindings
        Pattern::Literal(_) => (None, scrutinee_ty.clone()),

        // Enum variant patterns don't introduce bindings
        // (they match by value equality, not type)
        Pattern::EnumVariant { .. } => (None, scrutinee_ty.clone()),

        // Union patterns don't introduce bindings
        // (they're unions of literals or enum variants)
        Pattern::Union(_) => (None, scrutinee_ty.clone()),
    };

    if result.0.is_some() {
        ctx.pattern_types.insert(pattern_id, result.1.clone());
    }

    result
}

/// Reject `any` and `unknown` type annotations in catch binding positions.
fn validate_catch_binding_type(
    ctx: &mut TypeContext<'_>,
    pattern: &Pattern,
    pattern_id: PatId,
    _body: &ExprBody,
) {
    if let Pattern::TypedBinding { ty, .. } = pattern {
        use baml_compiler_hir::TypeRef;
        let banned_name = match ty {
            TypeRef::BuiltinUnknown => Some("unknown"),
            TypeRef::Path(path)
                if path.is_simple()
                    && path.first_segment().map(baml_base::Name::as_str) == Some("any") =>
            {
                Some("any")
            }
            _ => None,
        };
        if let Some(name) = banned_name {
            ctx.push_error(TypeError::InvalidCatchBindingType {
                type_name: name.to_string(),
                location: ErrorLocation::Pattern(pattern_id),
            });
        }
    }
}

// ============================================================================
// Match Exhaustiveness and Unreachability Checking
// ============================================================================
//
// Exhaustiveness checking is implemented in the `exhaustiveness` module using
// a value-based model (`ValueSet`). See `exhaustiveness.rs` for details.
//
// Key design principle: Pattern matching operates on VALUES, not types.
// - `Status.Active` matches one specific value
// - `s: Status` matches all values of type Status
// - `_` or `other` matches everything (catch-all)
//
// The `ExhaustivenessChecker` tracks which value sets have been covered
// by match arms and reports:
// - Non-exhaustive matches (uncovered cases)
// - Unreachable arms (arms that can never match)
// ============================================================================

/// Check match exhaustiveness and detect unreachable arms.
///
/// This function implements the exhaustiveness checking rules from BEP-002:
/// 1. All cases must be covered explicitly or via catch-all
/// 2. Guards do NOT contribute to exhaustiveness
/// 3. Catch-all (`_` or untyped binding) covers remaining cases
/// 4. Arms after a catch-all are unreachable
///
/// # Errors
/// - `TypeError::NonExhaustiveMatch` if not all cases are covered
/// - `TypeError::UnreachableArm` if an arm can never match
fn check_match_exhaustiveness(
    ctx: &mut TypeContext<'_>,
    scrutinee_ty: &Ty,
    arms_and_patterns: &[(MatchArmId, PatId)],
    body: &ExprBody,
    match_expr_id: ExprId,
) {
    // Skip exhaustiveness checking for unknown/error types
    if scrutinee_ty.is_unknown() || scrutinee_ty.is_error() {
        return;
    }

    // Use the new value-based exhaustiveness checker
    let checker = ExhaustivenessChecker::new(
        &ctx.enum_variants,
        &ctx.type_aliases,
        &ctx.class_names,
        &ctx.enum_names,
        &ctx.type_alias_names,
    );

    let arms = arms_and_patterns
        .iter()
        .map(|(arm, _)| *arm)
        .collect::<Vec<_>>();
    let result = checker.check(scrutinee_ty, &arms, body);

    // Report unreachable arms using position-independent MatchArmId
    for arm_idx in result.unreachable_arms {
        let pat_id = arms_and_patterns[arm_idx].1;
        ctx.push_error(TypeError::UnreachableArm {
            location: ErrorLocation::Pattern(pat_id),
        });
    }

    // Report non-exhaustive match (points to the match expression itself)
    if !result.is_exhaustive {
        let missing_cases: Vec<String> = result
            .uncovered
            .iter()
            .map(std::string::ToString::to_string)
            .collect();

        ctx.push_error(TypeError::NonExhaustiveMatch {
            scrutinee_type: scrutinee_ty.clone(),
            missing_cases,
            location: ErrorLocation::Expr(match_expr_id),
        });
    } else {
        // Record that this match is exhaustive for codegen optimization
        ctx.exhaustive_matches.insert(match_expr_id);
    }
}

/// Infer the type of a literal.
///
/// Returns literal types (singleton types) for better bidirectional type checking.
/// For example, the literal `42` has type `Ty::Literal(LiteralValue::Int(42))`,
/// which is a subtype of `Ty::Int`.
fn infer_literal(lit: &baml_compiler_hir::Literal) -> Ty {
    use crate::types::LiteralValue;
    match lit {
        baml_compiler_hir::Literal::Int(n) => Ty::Literal(LiteralValue::Int(*n), d()),
        baml_compiler_hir::Literal::Float(f) => Ty::Literal(LiteralValue::Float(f.clone()), d()),
        baml_compiler_hir::Literal::String(s) => Ty::Literal(LiteralValue::String(s.clone()), d()),
        baml_compiler_hir::Literal::Bool(b) => Ty::Literal(LiteralValue::Bool(*b), d()),
        baml_compiler_hir::Literal::Null => Ty::Null { attr: d() },
    }
}

/// Generalize a literal type to its base type (reference version for error messages).
///
/// Used in error messages where we want to show "int" instead of "42".
/// For operator errors, the issue is type compatibility, not the specific value.
fn generalize(ty: &Ty) -> Ty {
    use crate::types::LiteralValue;
    match ty {
        Ty::Literal(LiteralValue::Int(_), attr) => Ty::Int { attr: attr.clone() },
        Ty::Literal(LiteralValue::Float(_), attr) => Ty::Float { attr: attr.clone() },
        Ty::Literal(LiteralValue::String(_), attr) => Ty::String { attr: attr.clone() },
        Ty::Literal(LiteralValue::Bool(_), attr) => Ty::Bool { attr: attr.clone() },
        other => other.clone(),
    }
}

/// Generalize the found type for error messages, but preserve literals when expected is also a literal.
///
/// When expected is a literal type (like `4`), we want to show "Expected `4`, found `3`"
/// rather than "Expected `4`, found `int`". But when expected is a base type like `int[]`,
/// we want to show "Expected `int[]`, found `int`" rather than "Expected `int[]`, found `42`".
fn generalize_for_error(expected: &Ty, found: &Ty) -> Ty {
    if matches!(expected, Ty::Literal(..)) {
        // Keep literal types when expected is also a literal
        found.clone()
    } else {
        generalize(found)
    }
}

/// Infer the result type of a binary operation.
fn infer_binary_op(
    ctx: &mut TypeContext<'_>,
    op: baml_compiler_hir::BinaryOp,
    lhs: &Ty,
    rhs: &Ty,
    location: ErrorLocation,
) -> Ty {
    use baml_compiler_hir::BinaryOp::{
        Add, And, BitAnd, BitOr, BitXor, Div, Eq, Ge, Gt, Instanceof, Le, Lt, Mod, Mul, Ne, Or,
        Shl, Shr, Sub,
    };

    use crate::types::LiteralValue;

    // Helpers check base type and literal types, including unions of the same kind.
    // e.g., `20 | 0` is int-like because all members are int literals.
    fn is_int_like(ty: &Ty) -> bool {
        match ty {
            Ty::Int { .. } | Ty::Literal(LiteralValue::Int(_), _) => true,
            Ty::Union(members, _) => members.iter().all(is_int_like),
            _ => false,
        }
    }
    fn is_float_like(ty: &Ty) -> bool {
        match ty {
            Ty::Float { .. } | Ty::Literal(LiteralValue::Float(_), _) => true,
            Ty::Union(members, _) => members.iter().all(is_float_like),
            _ => false,
        }
    }
    fn is_string_like(ty: &Ty) -> bool {
        match ty {
            Ty::String { .. } | Ty::Literal(LiteralValue::String(_), _) => true,
            Ty::Union(members, _) => members.iter().all(is_string_like),
            _ => false,
        }
    }
    fn is_bool_like(ty: &Ty) -> bool {
        match ty {
            Ty::Bool { .. } | Ty::Literal(LiteralValue::Bool(_), _) => true,
            Ty::Union(members, _) => members.iter().all(is_bool_like),
            _ => false,
        }
    }

    // Don't emit errors for operations involving unknown, error, or never types.
    // Unknown/Error: the root cause (e.g., unknown variable) has already been reported.
    // Never: the expression is unreachable (e.g., after throw), so no error needed.
    if lhs.is_unknown() || lhs.is_error() || rhs.is_unknown() || rhs.is_error() {
        return Ty::Unknown { attr: d() };
    }
    if matches!(lhs, Ty::Never { .. }) || matches!(rhs, Ty::Never { .. }) {
        return Ty::Never { attr: d() };
    }

    match op {
        // Arithmetic operations (and string concatenation for Add)
        Add => {
            if is_int_like(lhs) && is_int_like(rhs) {
                Ty::Int { attr: d() }
            } else if (is_int_like(lhs) || is_float_like(lhs))
                && (is_int_like(rhs) || is_float_like(rhs))
            {
                Ty::Float { attr: d() }
            } else if is_string_like(lhs) && is_string_like(rhs) {
                // String concatenation
                Ty::String { attr: d() }
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error { attr: d() }
            }
        }
        Sub | Mul | Div | Mod => {
            if is_int_like(lhs) && is_int_like(rhs) {
                Ty::Int { attr: d() }
            } else if (is_int_like(lhs) || is_float_like(lhs))
                && (is_int_like(rhs) || is_float_like(rhs))
            {
                Ty::Float { attr: d() }
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error { attr: d() }
            }
        }

        // Comparison operations
        Eq | Ne => Ty::Bool { attr: d() },

        Lt | Le | Gt | Ge => {
            let numeric_lhs = is_int_like(lhs) || is_float_like(lhs);
            let numeric_rhs = is_int_like(rhs) || is_float_like(rhs);
            if (numeric_lhs && numeric_rhs) || (is_string_like(lhs) && is_string_like(rhs)) {
                Ty::Bool { attr: d() }
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error { attr: d() }
            }
        }

        // Logical operations
        And | Or => {
            if is_bool_like(lhs) && is_bool_like(rhs) {
                Ty::Bool { attr: d() }
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error { attr: d() }
            }
        }

        // Bitwise operations
        BitAnd | BitOr | BitXor | Shl | Shr => {
            if is_int_like(lhs) && is_int_like(rhs) {
                Ty::Int { attr: d() }
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error { attr: d() }
            }
        }

        // Type checking operations
        Instanceof => Ty::Bool { attr: d() },
    }
}

/// Infer the result type of a unary operation.
fn infer_unary_op(
    ctx: &mut TypeContext<'_>,
    op: baml_compiler_hir::UnaryOp,
    operand: &Ty,
    location: ErrorLocation,
) -> Ty {
    use baml_compiler_hir::UnaryOp::{Neg, Not};

    use crate::types::LiteralValue;

    // Don't emit errors for operations involving unknown or error types - the root cause
    // has already been reported
    if operand.is_unknown() || operand.is_error() {
        return Ty::Unknown { attr: d() };
    }

    match op {
        Not => {
            // `!` is a truthiness check: accepts any type, returns bool.
            // For bool operands this is logical negation; for other types
            // it converts to bool (falsy: null, false; truthy: everything else).
            Ty::Bool { attr: d() }
        }
        Neg => match operand {
            Ty::Int { .. } | Ty::Literal(LiteralValue::Int(_), _) => Ty::Int { attr: d() },
            Ty::Float { .. } | Ty::Literal(LiteralValue::Float(_), _) => Ty::Float { attr: d() },
            _ => {
                ctx.push_error(TypeError::InvalidUnaryOp {
                    op: "-".to_string(),
                    operand: generalize(operand),
                    location,
                });
                Ty::Error { attr: d() }
            }
        },
    }
}

/// Infer the type of a field access.
///
/// For class types, this handles both field access and method access.
/// For primitive types (arrays, strings, maps), this handles builtin methods.
///
/// The `expr_id` parameter is optional - when provided (for standalone `FieldAccess` expressions),
/// the resolution is stored for MIR to use. For field accesses within multi-segment paths,
/// pass None since the resolution is handled at the path level.
fn infer_field_access(
    ctx: &mut TypeContext<'_>,
    base: &Ty,
    field: &Name,
    location: ErrorLocation,
    expr_id: Option<ExprId>,
) -> Ty {
    // Special case: $watch accessor on any type
    // The actual watched check happens at MIR lowering time
    if field.as_str() == "$watch" {
        return Ty::WatchAccessor(Box::new(base.clone()), d());
    }

    // Special case: methods on WatchAccessor type
    if let Ty::WatchAccessor(_inner_ty, _) = base {
        match field.as_str() {
            "options" => {
                // $watch.options(filter) - filter can be a function, "manual", or "never"
                // Returns null (void operation)
                return Ty::Function {
                    // First param is receiver (the WatchAccessor), second is filter
                    params: vec![
                        (None, base.clone()),
                        (Some(Name::new("filter")), Ty::Unknown { attr: d() }),
                    ], // Filter type is flexible
                    ret: Box::new(Ty::Null { attr: d() }),
                    attr: d(),
                };
            }
            "notify" => {
                // $watch.notify() - manually trigger notification
                // Returns null (void operation)
                return Ty::Function {
                    params: vec![(None, base.clone())], // Just the receiver
                    ret: Box::new(Ty::Null { attr: d() }),
                    attr: d(),
                };
            }
            _ => {
                ctx.push_error(TypeError::NoSuchField {
                    ty: base.clone(),
                    field: field.to_string(),
                    location,
                });
                return Ty::Unknown { attr: d() };
            }
        }
    }

    // First, try class field lookup for named types
    let found_field = match base {
        Ty::TypeAlias(fqn, _) => {
            let key = fqn.display_name();
            ctx.lookup(field)
                .or(ctx.lookup_class_field(&key, field))
                .cloned()
        }
        Ty::Class(fqn, _) => {
            // First try to find a method using a class-qualified name in the same
            // namespace as the class (e.g., `baml.llm.Foo.bar`).
            let method_qn = QualifiedName {
                namespace: fqn.namespace.clone(),
                name: QualifiedName::local_method_from_str(fqn.name.as_str(), field.as_str()),
            };
            let method_lookup_name = method_qn.display_name();
            if let Some(method_ty) = ctx.lookup(&method_lookup_name).cloned() {
                // Store resolution for method reference so MIR can look it up
                if let Some(expr_id) = expr_id {
                    ctx.set_expr_resolution(expr_id, ResolvedValue::Function(method_qn));
                }
                return method_ty;
            }
            // Check the context's class_fields for this class name.
            // Use display_name() to get the full qualified path for builtins
            // (e.g., "baml.http.Response") while keeping simple names for locals.
            let key = fqn.display_name();
            ctx.lookup_class_field(&key, field).cloned()
        }
        // Union field access: x.field where x: A | B is allowed if all members
        // have the field. Result type is the union of field types across members.
        Ty::Union(members, _) => {
            let field_types: Vec<Ty> = members
                .iter()
                .filter_map(|member| infer_union_member_field(ctx, member, field))
                .collect();
            if field_types.len() == members.len() {
                // All members have the field — return union of field types
                // (deduplicated: if all the same, just return that type)
                if field_types.iter().all(|t| t == &field_types[0]) {
                    Some(field_types.into_iter().next().unwrap())
                } else {
                    Some(Ty::Union(field_types, d()))
                }
            } else {
                None
            }
        }
        Ty::Unknown { .. } => return Ty::Unknown { attr: d() },
        _ => None,
    };

    if let Some(ty) = found_field {
        return ty;
    }

    // Try builtin method lookup
    if let Some((def, bindings)) = builtins::lookup_method(base, field.as_str()) {
        // Build the function type from the builtin definition.
        // If this is a method (has a receiver), include the receiver type as the first param
        // since the Call handler will pass the receiver as the first argument.
        if let Some(expr_id) = expr_id {
            ctx.set_expr_resolution(
                expr_id,
                ResolvedValue::BuiltinFunction(QualifiedName::from_builtin_path(def.path)),
            );
        }
        let mut params: Vec<(Option<Name>, Ty)> = Vec::new();
        if def.receiver.is_some() {
            params.push((None, base.clone()));
        }
        for (param_name, pattern) in &def.params {
            params.push((
                Some(Name::new(*param_name)),
                builtins::substitute(pattern, &bindings),
            ));
        }
        let return_type = builtins::substitute(&def.returns, &bindings);

        return Ty::Function {
            params,
            ret: Box::new(return_type),
            attr: d(),
        };
    }

    // Field/method not found
    ctx.push_error(TypeError::NoSuchField {
        ty: base.clone(),
        field: field.to_string(),
        location,
    });
    Ty::Unknown { attr: d() }
}

/// Infer the type of an index access.
fn infer_index_access(
    ctx: &mut TypeContext<'_>,
    base: &Ty,
    index: &Ty,
    location: ErrorLocation,
) -> Ty {
    match base {
        Ty::List(elem, _) => {
            // Index must be int
            if !ctx.is_subtype_of(index, &Ty::Int { attr: d() }) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Int { attr: d() },
                    found: index.clone(),
                    location,
                    info_location: None,
                });
            }
            (**elem).clone()
        }
        Ty::Map { key, value, .. } => {
            // Index must match key type
            if !ctx.is_subtype_of(index, key) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: (**key).clone(),
                    found: index.clone(),
                    location,
                    info_location: None,
                });
            }
            (**value).clone()
        }
        Ty::String { .. } => {
            // String indexing returns a character (string of length 1)
            if !ctx.is_subtype_of(index, &Ty::Int { attr: d() }) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Int { attr: d() },
                    found: index.clone(),
                    location,
                    info_location: None,
                });
            }
            Ty::String { attr: d() }
        }
        Ty::Unknown { .. } => Ty::Unknown { attr: d() },
        _ => {
            ctx.push_error(TypeError::NotIndexable {
                ty: base.clone(),
                location,
            });
            Ty::Unknown { attr: d() }
        }
    }
}

/// Type check a statement.
fn check_stmt(ctx: &mut TypeContext<'_>, stmt_id: StmtId, body: &ExprBody) {
    check_stmt_with_return(ctx, stmt_id, body, None);
}

/// Type check a statement with an optional expected return type for better checking.
///
/// When `expected_return` is provided, return statements and let initializers
/// can use bidirectional type checking for better error messages and inference.
fn check_stmt_with_return(
    ctx: &mut TypeContext<'_>,
    stmt_id: StmtId,
    body: &ExprBody,
    expected_return: Option<&Ty>,
) {
    use baml_compiler_hir::Stmt;

    let stmt = &body.stmts[stmt_id];

    match stmt {
        Stmt::Let {
            pattern,
            type_annotation,
            initializer,
            is_watched,
            ..
        } => {
            let ty = if let Some(init) = initializer {
                // If there's a type annotation, use check_expr for bidirectional typing
                if let Some(type_id) = type_annotation {
                    let type_ref = &body.types[*type_id];
                    let span = ctx.type_span(*type_id);
                    let annot_ty = ctx.lower_type(type_ref, span);
                    let annotation_location = ErrorLocation::Span(span);
                    // Use check_expr when we have an expected type
                    // check_expr already reports any type mismatch errors
                    check_expr_with_info_location(
                        ctx,
                        *init,
                        body,
                        &annot_ty,
                        Some(&annotation_location),
                    );
                    annot_ty
                } else {
                    // No type annotation - infer and generalize for mutable variables
                    // This ensures `let x = 5` gives `x : int`, not `x : 5`
                    let inferred = infer_expr(ctx, *init, body);
                    generalize(&inferred)
                }
            } else if let Some(type_id) = type_annotation {
                let type_ref = &body.types[*type_id];
                let span = ctx.type_span(*type_id);
                ctx.lower_type(type_ref, span)
            } else {
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            };

            // Extract variable name from pattern and track watched status
            let pat = &body.patterns[*pattern];
            match pat {
                Pattern::Binding(name) => {
                    ctx.define_with_site(name.clone(), ty, DefinitionSite::Statement(stmt_id));
                    if *is_watched {
                        ctx.mark_watched(name.clone());
                    }
                }
                Pattern::TypedBinding { name, ty: _ } => {
                    // TODO: Check declared type matches inferred type
                    ctx.define_with_site(name.clone(), ty, DefinitionSite::Statement(stmt_id));
                    if *is_watched {
                        ctx.mark_watched(name.clone());
                    }
                }
                Pattern::Literal(_) | Pattern::EnumVariant { .. } | Pattern::Union(_) => {
                    // Literals/enum variants/unions don't introduce bindings in let statements
                    // This would be a semantic error, but we'll handle it elsewhere
                }
            }
        }

        Stmt::Expr(expr) => {
            infer_expr(ctx, *expr, body);
        }

        Stmt::Return(expr) => {
            let fn_ret = ctx.fn_return_type.clone();
            let effective_expected = expected_return.or(fn_ret.as_ref());
            let return_ty = if let Some(e) = expr {
                if let Some(expected) = effective_expected {
                    check_expr(ctx, *e, body, expected)
                } else {
                    infer_expr(ctx, *e, body)
                }
            } else {
                Ty::Void {
                    attr: TyAttr::default(),
                }
            };
            ctx.record_return(return_ty, Span::default());
        }

        Stmt::While {
            condition,
            body: while_body,
            after,
            origin: _, // origin is used for diagnostics, not type checking
        } => {
            // Condition: accept any type (truthiness check)
            infer_expr(ctx, *condition, body);
            infer_expr(ctx, *while_body, body);
            // Type-check the after statement (for desugared C-style for loops)
            if let Some(after_stmt) = after {
                check_stmt_with_return(ctx, *after_stmt, body, expected_return);
            }
        }

        Stmt::Break | Stmt::Continue => {
            // These are control flow statements with no expressions to type-check.
            // Loop context validation could be added here in the future.
        }

        Stmt::Assign { target, value } => {
            // Type-check both the target and value expressions
            let target_ty = infer_expr(ctx, *target, body);
            let value_ty = infer_expr(ctx, *value, body);
            // Check that value type is compatible with target type
            if !ctx.is_subtype_of(&value_ty, &target_ty) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: target_ty.clone(),
                    found: generalize_for_error(&target_ty, &value_ty),
                    location: ErrorLocation::Expr(*value),
                    info_location: None,
                });
            }
        }

        Stmt::AssignOp {
            target,
            op: _,
            value,
        } => {
            // Type-check both the target and value expressions
            let target_ty = infer_expr(ctx, *target, body);
            let value_ty = infer_expr(ctx, *value, body);
            // Check that value type is compatible with target type
            if !ctx.is_subtype_of(&value_ty, &target_ty) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: target_ty.clone(),
                    found: generalize_for_error(&target_ty, &value_ty),
                    location: ErrorLocation::Expr(*value),
                    info_location: None,
                });
            }
        }

        Stmt::Assert { condition } => {
            // Type-check the condition expression (bidirectional)
            check_expr(
                ctx,
                *condition,
                body,
                &Ty::Bool {
                    attr: TyAttr::default(),
                },
            );
        }

        Stmt::Missing => {}

        Stmt::HeaderComment { .. } => {
            // Header comments don't need type checking - they're just annotations
        }

        Stmt::Throw { value } => {
            infer_expr(ctx, *value, body);
        }
    }
}
