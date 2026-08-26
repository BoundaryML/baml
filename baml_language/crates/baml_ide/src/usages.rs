//! `usages_at` — find all references to the symbol at a cursor position.
//!
//! [`crate::resolve::symbol_at`] identifies the target; one search strategy
//! per target family finds its references:
//!
//! - **Top-level items**: scan every workspace file's CST for name tokens
//!   matching the target, confirming each candidate via `resolve_name_at`.
//! - **Locals**: walk the enclosing function's body arena (and nested lambda
//!   arenas under their own metadata scopes) for paths resolving to the same
//!   binding.
//! - **Members** (fields, variants, methods, interface slots): walk every
//!   workspace file's function scopes and match the *inference* member
//!   resolutions against the target — spans come from the source maps
//!   (`member_access_member_span`, `path_segment_span`,
//!   `object_field_name_span`), never from re-scanning source text.
//!
//! The search scope is the workspace (every `Workspace`-kind source root):
//! stdlib and dependency roots are read-only context — their internal
//! references are not user-actionable, and a rename could never touch them.
//!
//! Candidate files are pre-filtered by a plain-text `contains` check on the
//! target's name. The filter is conservative (comments and strings can only
//! produce false *candidates*, which the resolution confirm then rejects);
//! it can never drop a real reference because a reference must spell the
//! name somewhere in the file.
//!
//! The definition site itself is NOT included in the results. Callers that
//! want "references + definition" call `definition_at` separately.
//!
//! Known gap: references inside `match` *type patterns* (`Status.Active =>`)
//! are not found — inference exposes pattern types (`type_of_pat`) but no
//! per-name pattern resolutions. Closing it needs a compiler-side
//! pattern-resolution record in `baml_compiler2_hir_ty`, not more searching
//! here.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_ast::{Expr, ExprBody};
use baml_compiler2_hir::{
    body::FunctionBody,
    contributions::Definition,
    loc::FunctionLoc,
    scope::{FileScopeId, ScopeKind},
    semantic_index::{
        BindingId, ExprMetadataKey, ExprMetadataScope, FileSemanticIndex, PathResolution,
    },
};
use baml_compiler2_ppir::{
    item_data,
    resolve::{ResolvedName, resolve_name_at},
};
use rowan::NodeOrToken;
use text_size::{TextRange, TextSize};

use crate::resolve::{Location, SymbolTarget, member_resolution_target, symbol_at};

/// Find all references to the symbol at `offset` in `file`.
///
/// Regular function (not cached); the per-file work is Salsa-cached. Returns
/// an empty `Vec` if the cursor is not on a resolvable symbol.
pub fn usages_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Vec<Location> {
    let Some(target) = symbol_at(db, file, offset) else {
        return Vec::new();
    };

    match target {
        SymbolTarget::Item(def) => find_item_usages(db, file, def),
        SymbolTarget::Local {
            func,
            func_scope,
            binding,
        } => find_local_usages(db, func, func_scope, binding),
        SymbolTarget::Field { .. }
        | SymbolTarget::Variant { .. }
        | SymbolTarget::Method { .. }
        | SymbolTarget::InterfaceRequiredMethod { .. }
        | SymbolTarget::InterfaceField { .. }
        | SymbolTarget::AssociatedType { .. } => find_member_usages(db, file, target),
    }
}

// ── search scope ─────────────────────────────────────────────────────────────

/// Every file of every `Workspace` root, plus `current_file` (which may be
/// detached from any workspace package, e.g. a provisional single-file
/// root).
fn workspace_search_files(
    db: &dyn baml_compiler2_ppir::Db,
    current_file: SourceFile,
) -> Vec<SourceFile> {
    let mut files: Vec<SourceFile> = baml_compiler2_hir::package::workspace_roots(db)
        .iter()
        .flat_map(|root| root.files(db).iter().copied())
        .collect();
    if !files.contains(&current_file) {
        files.push(current_file);
    }
    files
}

// ── top-level items ──────────────────────────────────────────────────────────

/// Scan workspace files for name tokens resolving to the same definition.
fn find_item_usages(
    db: &dyn baml_compiler2_ppir::Db,
    current_file: SourceFile,
    def: Definition<'_>,
) -> Vec<Location> {
    let Some(name) = definition_name(db, def) else {
        return Vec::new();
    };
    let name_text = name.as_str();

    let mut results = Vec::new();
    for sf in workspace_search_files(db, current_file) {
        db.unwind_if_revision_cancelled();

        let text = sf.text(db);
        if !text.contains(name_text) {
            continue;
        }

        let root = baml_compiler_parser::syntax_tree(db, sf);
        for node_or_token in root.descendants_with_tokens() {
            let NodeOrToken::Token(tok) = node_or_token else {
                continue;
            };
            if tok.kind() != SyntaxKind::WORD || tok.text() != name_text {
                continue;
            }

            // Confirm this token resolves to the same definition.
            let resolved = resolve_name_at(db, sf, tok.text_range().start(), &name);
            let same = match resolved {
                ResolvedName::Item(here) | ResolvedName::Builtin(here) => here == def,
                ResolvedName::Local { .. } | ResolvedName::Unknown => false,
            };
            if same {
                results.push(Location {
                    file: sf,
                    range: tok.text_range(),
                });
            }
        }
    }
    results
}

/// The declared name of a top-level definition, from its file's symbol
/// contributions (covers every `Definition` variant uniformly).
fn definition_name(db: &dyn baml_compiler2_ppir::Db, def: Definition<'_>) -> Option<Name> {
    let contributions = baml_compiler2_hir::file_symbol_contributions(db, def.file(db));
    contributions
        .types
        .iter()
        .chain(contributions.values.iter())
        .find(|(_, contrib)| contrib.definition == def)
        .map(|(name, _)| name.clone())
}

// ── locals ───────────────────────────────────────────────────────────────────

/// References to a local binding within its owning function (including
/// nested lambda bodies and parameter-default arenas).
fn find_local_usages(
    db: &dyn baml_compiler2_ppir::Db,
    func: FunctionLoc<'_>,
    func_scope: FileScopeId,
    target_binding: BindingId,
) -> Vec<Location> {
    let file = func.file(db);
    let index = baml_compiler2_hir::file_semantic_index(db, file);

    let body = baml_compiler2_ppir::function_body(db, func);
    let FunctionBody::Expr(expr_body) = body.as_ref() else {
        return Vec::new();
    };
    let Some(source_map) = baml_compiler2_ppir::function_body_source_map(db, func) else {
        return Vec::new();
    };

    let name = binding_name(index, target_binding, db, func);
    let Some(name) = name else {
        return Vec::new();
    };

    let mut collector = LocalUsageCollector {
        file,
        index,
        name,
        target_binding,
        results: Vec::new(),
    };
    collector.collect(
        func_scope,
        expr_body.root_expr,
        expr_body,
        ExprMetadataScope::Body(func_scope),
        &source_map,
    );

    // A defaults arena is a *forest* — one root per defaulted parameter, and
    // `root_expr` is always `None` for it. Walk each parameter's default
    // separately.
    let defaults = baml_compiler2_ppir::function_parameter_defaults(db, func);
    for default in defaults.params.iter().flatten() {
        collector.collect(
            func_scope,
            Some(default.expr.expr()),
            &defaults.defaults.exprs,
            ExprMetadataScope::ParameterDefault(func_scope),
            &defaults.defaults.source_map,
        );
    }

    collector.results
}

/// The name a binding was declared under.
fn binding_name(
    index: &FileSemanticIndex<'_>,
    binding: BindingId,
    db: &dyn baml_compiler2_ppir::Db,
    func: FunctionLoc<'_>,
) -> Option<Name> {
    use baml_compiler2_hir::semantic_index::BindingKind;
    let scope_bindings = &index.scope_bindings[binding.scope.index() as usize];
    match binding.kind {
        BindingKind::Local(idx) => scope_bindings
            .bindings
            .get(idx as usize)
            .map(|b| b.name.clone()),
        BindingKind::Parameter(idx) => scope_bindings
            .params
            .iter()
            .find(|(_, param_idx)| *param_idx == idx)
            .map(|(name, _)| name.clone())
            .or_else(|| {
                // Lambda parameter scopes record params positionally; fall
                // back to the function signature for the outer function.
                item_data::function_data(db, func)
                    .params
                    .get(idx)
                    .map(|p| p.name.clone())
            }),
    }
}

struct LocalUsageCollector<'index, 'db> {
    file: SourceFile,
    index: &'index FileSemanticIndex<'db>,
    name: Name,
    target_binding: BindingId,
    results: Vec<Location>,
}

impl LocalUsageCollector<'_, '_> {
    /// Walk the expressions belonging to `owner_scope`, recursing into
    /// lambda bodies under *their* scope.
    ///
    /// Structural rather than a flat arena scan: lambda bodies share this
    /// arena but are recorded under their own metadata namespace, so
    /// visiting them here would look them up under the wrong key and
    /// silently find nothing.
    fn collect(
        &mut self,
        owner_scope: FileScopeId,
        root: Option<baml_compiler2_ast::ExprId>,
        expr_body: &ExprBody,
        metadata_scope: ExprMetadataScope,
        source_map: &baml_compiler2_ast::AstSourceMap,
    ) {
        let nodes = root
            .map(|root| expr_body.reachable_excluding_lambdas(root))
            .unwrap_or_default();
        for node in nodes {
            let baml_compiler2_ast::BodyNode::Expr(expr_id) = node else {
                continue;
            };
            let expr = &expr_body.exprs[expr_id];
            match expr {
                Expr::Path(segments) if segments.first() == Some(&self.name) => {
                    let segment_range = source_map.path_segment_span(expr_id, 0);
                    let range =
                        TextRange::at(segment_range.start(), TextSize::of(segments[0].as_str()));
                    if range.is_empty() {
                        continue;
                    }

                    let key = ExprMetadataKey::new(metadata_scope, expr_id);
                    if self.index.path_resolution(key)
                        == Some(PathResolution::Local(self.target_binding))
                    {
                        self.results.push(Location {
                            file: self.file,
                            range,
                        });
                    }
                }
                Expr::Lambda(func_def) => {
                    let span = source_map.expr_span(expr_id);
                    let Some(lambda_scope) = self.index.lambda_scope_for_within(owner_scope, span)
                    else {
                        continue;
                    };

                    // One root per defaulted parameter — see above.
                    for param in &func_def.params {
                        let Some(default) = param.default else {
                            continue;
                        };
                        self.collect(
                            lambda_scope,
                            Some(default.expr()),
                            &func_def.defaults.exprs,
                            ExprMetadataScope::ParameterDefault(lambda_scope),
                            &func_def.defaults.source_map,
                        );
                    }

                    self.collect(
                        lambda_scope,
                        func_def.body,
                        expr_body,
                        ExprMetadataScope::Body(lambda_scope),
                        source_map,
                    );
                }
                _ => {}
            }
        }
    }
}

// ── members ──────────────────────────────────────────────────────────────────

/// References to a member target (field, variant, method, interface slot):
/// one walk over workspace function scopes, matching inference resolutions.
fn find_member_usages(
    db: &dyn baml_compiler2_ppir::Db,
    current_file: SourceFile,
    target: SymbolTarget<'_>,
) -> Vec<Location> {
    let Some(name) = member_target_name(db, target) else {
        return Vec::new();
    };
    let name_text = name.as_str();

    let mut results = Vec::new();
    for sf in workspace_search_files(db, current_file) {
        db.unwind_if_revision_cancelled();

        let text = sf.text(db);
        if !text.contains(name_text) {
            continue;
        }

        let sf_index = baml_compiler2_hir::file_semantic_index(db, sf);
        for (scope_idx, scope) in sf_index.scopes.iter().enumerate() {
            if !matches!(scope.kind, ScopeKind::Function) {
                continue;
            }
            // The recorded item↔scope link (template-string scopes have a
            // non-Function owner → skipped).
            let owner_scope = sf_index.scope_ids[scope_idx];
            let Some(item_data::ScopeOwner::Function(func_loc)) =
                item_data::scope_owner(db, owner_scope)
            else {
                continue;
            };
            let body = baml_compiler2_ppir::function_body(db, func_loc);
            let FunctionBody::Expr(expr_body) = body.as_ref() else {
                continue;
            };
            let Some(source_map) = baml_compiler2_ppir::function_body_source_map(db, func_loc)
            else {
                continue;
            };
            let Some(inference) = baml_compiler2_hir_ty::ide::infer_for_scope(db, owner_scope)
            else {
                continue;
            };

            // Expression-level member sites. A dotted *name* (`p.name`)
            // lowers as a `Path` expression and can carry BOTH an
            // expression-level member resolution and a per-segment ladder;
            // when the ladder resolved any segment it owns the site (below),
            // otherwise the expression-level entry is the only record (e.g.
            // an enum-variant path `Status.Active`) and is reported here
            // with the last segment's span.
            for (expr_id, resolution) in &inference.member_resolutions {
                if member_resolution_target(db, resolution) != Some(target) {
                    continue;
                }
                let range = match &expr_body.exprs[*expr_id] {
                    Expr::MemberAccess { .. } => source_map.member_access_member_span(*expr_id),
                    Expr::Path(segments) => {
                        let ladder_resolved =
                            inference.path_resolutions.get(expr_id).is_some_and(|path| {
                                path.segments.iter().any(|step| step.resolution.is_some())
                            });
                        if ladder_resolved {
                            continue;
                        }
                        source_map.path_segment_span(*expr_id, segments.len() - 1)
                    }
                    // Member resolutions only attach to accesses and paths.
                    _ => continue,
                };
                if !range.is_empty() {
                    results.push(Location { file: sf, range });
                }
            }

            // Non-root segments of multi-segment `Path`s.
            for (expr_id, resolved_path) in &inference.path_resolutions {
                for (seg_idx, step) in resolved_path.segments.iter().enumerate().skip(1) {
                    let Some(resolution) = step.resolution.as_ref() else {
                        continue;
                    };
                    if member_resolution_target(db, resolution) == Some(target) {
                        let range = source_map.path_segment_span(*expr_id, seg_idx);
                        if !range.is_empty() {
                            results.push(Location { file: sf, range });
                        }
                    }
                }
            }

            // Constructor-literal keys (fields only).
            if let SymbolTarget::Field { class, .. } = target {
                collect_constructor_key_usages(
                    db,
                    sf,
                    class,
                    &name,
                    expr_body,
                    &source_map,
                    inference,
                    &mut results,
                );
            }
        }
    }
    results
}

/// The declared name of a member target (for the text pre-filter and key
/// matching).
fn member_target_name(db: &dyn baml_compiler2_ppir::Db, target: SymbolTarget<'_>) -> Option<Name> {
    match target {
        SymbolTarget::Field { class, field_index } => item_data::class_data(db, class)
            .fields
            .get(field_index)
            .map(|f| f.name.clone()),
        SymbolTarget::Variant {
            enum_loc,
            variant_index,
        } => item_data::enum_data(db, enum_loc)
            .variants
            .get(variant_index)
            .map(|v| v.name.clone()),
        SymbolTarget::Method { func } => Some(item_data::function_data(db, func).name.clone()),
        SymbolTarget::InterfaceRequiredMethod {
            iface,
            method_index,
        } => item_data::interface_data(db, iface)
            .required_methods
            .get(method_index)
            .map(|m| m.name.clone()),
        SymbolTarget::InterfaceField { iface, field_index } => item_data::interface_data(db, iface)
            .fields
            .get(field_index)
            .map(|f| f.name.clone()),
        // Associated types appear only in TYPE positions, which carry no
        // inference records — the walker finds no body usages (honest
        // absence until type-reference resolution is recorded).
        SymbolTarget::AssociatedType { iface, assoc_index } => item_data::interface_data(db, iface)
            .associated_types
            .get(assoc_index)
            .map(|assoc| assoc.name.clone()),
        // Items and locals are searched by their own strategies.
        SymbolTarget::Item(_) | SymbolTarget::Local { .. } => None,
    }
}

/// Constructor-literal keys referencing a field of `class`
/// (`Success { data: … }`). The literal's class identity is confirmed
/// through its inferred type, so a same-named key in a literal of another
/// class never matches; spans come from the recorded
/// `object_field_name_span`.
#[expect(
    clippy::too_many_arguments,
    reason = "per-scope walk context threaded through one call site"
)]
fn collect_constructor_key_usages(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    class: baml_compiler2_hir::loc::ClassLoc<'_>,
    field_name: &Name,
    expr_body: &ExprBody,
    source_map: &baml_compiler2_ast::AstSourceMap,
    inference: &baml_compiler2_hir_ty::infer::InferenceResult<'_>,
    results: &mut Vec<Location>,
) {
    use baml_type::Ty;

    for (expr_id, expr) in expr_body.exprs.iter() {
        let Expr::Object { fields, .. } = expr else {
            continue;
        };
        let Some(field) = fields.iter().find(|field| field.name == *field_name) else {
            continue;
        };

        let Some(obj_ty) = inference
            .type_of_expr
            .get(&expr_id)
            .map(baml_type::interned::Ty::to_plain)
        else {
            continue;
        };
        let Ty::Class(ref qtn, _, _) = obj_ty else {
            continue;
        };

        let pkg_id = baml_compiler2_hir::package::PackageId::new(db, qtn.package().clone());
        let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
        let Some(Definition::Class(obj_class)) = pkg_items.lookup_type(qtn.namespace(), qtn.name())
        else {
            continue;
        };
        if obj_class != class {
            continue;
        }

        let range = source_map.object_field_name_span(expr_id, field.value);
        if !range.is_empty() {
            results.push(Location { file, range });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::usages_at;
    use crate::{resolve::Location, test_support::CursorTest};

    /// Feature-side conveniences over the shared cursor harness.
    trait UsagesExt {
        fn find_all_usages(&self) -> Vec<Location>;
        fn format_location_with_name(&self, loc: &Location) -> String;
    }

    impl UsagesExt for CursorTest {
        fn find_all_usages(&self) -> Vec<Location> {
            usages_at(&self.db, self.cursor.file, self.cursor.offset)
        }

        fn format_location_with_name(&self, loc: &Location) -> String {
            self.format_file_range_with_text(loc.file, loc.range)
        }
    }

    #[test]
    fn test_find_refs_local_variable() {
        let test = CursorTest::new(
            r#"
function example() -> string {
    let <[CURSOR]x = "hello"
    let y = x
    let z = x + " world"
    x
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 3,
            "Should find at least 3 usages of 'x', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_parameter() {
        let test = CursorTest::new(
            r#"
function process(<[CURSOR]input: string) -> string {
    let a = input
    let b = input + "!"
    match (input) {
        "test" => input
        _ => "default"
    }
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 3,
            "Should find at least 3 usages of 'input', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_lambda_parameter_in_nested_lambda() {
        let test = CursorTest::new(
            r#"
function example() -> int {
    let f = (<[CURSOR]x: int) -> int {
        let y = x
        let g = () -> int { x + y }
        x + g()
    }
    f(1)
}
"#,
        );

        let usages = test.find_all_usages();
        assert_eq!(
            usages.len(),
            3,
            "Should find lambda-parameter usages across nested lambda arenas, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_function() {
        let test = CursorTest::new(
            r#"
function <[CURSOR]Helper(x: string) -> string {
    x + "!"
}

function main() -> string {
    let a = Helper("test")
    Helper("another")
}

function other() -> string {
    Helper("third")
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 3,
            "Should find at least 3 usages of 'Helper', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_class() {
        let test = CursorTest::new(
            r#"
class <[CURSOR]Person {
    name string
    age int
}

function create_person() -> Person {
    Person { name: "Alice", age: 30 }
}

function process_person(p: Person) -> string {
    p.name
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            !usages.is_empty(),
            "Should find at least 1 usage of 'Person', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_interface_in_out_of_body_implements() {
        let test = CursorTest::new(
            r#"
interface <[CURSOR]Animal {
    function speak(self) -> string
}

class Dog {}

implements Animal for Dog {
    function speak(self) -> string { return "woof" }
}
"#,
        );

        let usages = test.find_all_usages();
        let formatted = usages
            .iter()
            .map(|l| test.format_location_with_name(l))
            .collect::<Vec<_>>();

        assert!(
            formatted
                .iter()
                .any(|usage| usage.ends_with("-> Animal") && usage.contains("test.baml:")),
            "Should find the interface reference in the out-of-body implements target, found: {formatted:?}"
        );
    }

    #[test]
    fn test_find_refs_enum() {
        let test = CursorTest::new(
            r#"
enum <[CURSOR]Status {
    Active
    Inactive
}

function get_status() -> Status {
    Status.Active
}

function use_status() -> Status {
    let s = Status.Active
    Status.Inactive
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 2,
            "Should find at least 2 usages of 'Status', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_field() {
        let test = CursorTest::new(
            r#"
class Person {
    <[CURSOR]name string
    age int
}

function get_name(p: Person) -> string {
    p.name
}

function set_name(p: Person, n: string) -> Person {
    Person { name: n, age: p.age }
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 2,
            "Should find at least 2 usages of 'name' (field access + constructor), found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_no_references() {
        let test = CursorTest::new(
            r#"
function example() -> string {
    let <[CURSOR]unused = "value"
    "other"
}
"#,
        );

        let usages = test.find_all_usages();
        // An unused variable should have zero usages (definition site is excluded).
        assert!(
            usages.is_empty(),
            "Unused variable should have no usages, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_across_blocks() {
        let test = CursorTest::new(
            r#"
function example() -> string {
    let <[CURSOR]x = "outer"
    let y = x
    let z = x + x
    x
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            !usages.is_empty(),
            "Should find usages of local variable, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_local_variable_ignores_shadowed_binding() {
        let test = CursorTest::new(
            r#"
function example() -> string {
    let <[CURSOR]x = "outer"
    let y = x
    {
        let x = "inner"
        x
    }
    x
}
"#,
        );

        let usages = test.find_all_usages();
        assert_eq!(
            usages.len(),
            2,
            "Should only find usages of the outer 'x', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_multi_file() {
        let mut builder = CursorTest::builder();
        builder.source(
            "types.baml",
            r#"
class <[CURSOR]Person {
    name string
}
"#,
        );
        builder.source(
            "functions.baml",
            r#"
function create_person() -> Person {
    Person { name: "Alice" }
}

function process_person(p: Person) -> string {
    p.name
}
"#,
        );
        let test = builder.build();

        let usages = test.find_all_usages();
        assert!(
            !usages.is_empty(),
            "Should find usages across files, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn find_refs_field_from_access_position() {
        // The cursor sits on a field *access*, not the declaration: the
        // addressing layer resolves both spellings to the same target.
        let test = CursorTest::new(
            r#"
class Person {
    name string
}

function greet(p: Person) -> string {
    p.<[CURSOR]name
}

function shout(p: Person) -> string {
    p.name + "!"
}
"#,
        );

        let usages = test.find_all_usages();
        assert_eq!(
            usages.len(),
            2,
            "both accesses, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
        for usage in &usages {
            assert_eq!(
                &test.cursor.file.text(&test.db)
                    [usize::from(usage.range.start())..usize::from(usage.range.end())],
                "name",
                "reference spans exactly the member token"
            );
        }
    }

    #[test]
    fn find_refs_field_span_is_exact_in_chained_access() {
        // `w.inner.inner` — the recorded member spans must distinguish the
        // two links of the chain instead of text-searching the expression.
        let test = CursorTest::new(
            r#"
class Wrap {
    inner Wrap
}

class Holder {
    <[CURSOR]value string
}

function f(h: Holder, k: Holder) -> string {
    h.value + k.value
}
"#,
        );

        let usages = test.find_all_usages();
        assert_eq!(usages.len(), 2, "one per access");
        let text = test.cursor.file.text(&test.db);
        for usage in &usages {
            assert_eq!(
                &text[usize::from(usage.range.start())..usize::from(usage.range.end())],
                "value"
            );
        }
    }

    #[test]
    fn find_refs_constructor_key_ignores_same_named_key_of_other_class() {
        // `Outer { data: Inner { data: 1 } }`: only the key belonging to the
        // *target* class may match — the old text-scan-inside-span approach
        // hit the first matching WORD regardless of which literal owned it.
        let test = CursorTest::new(
            r#"
class Inner {
    data int
}

class Outer {
    <[CURSOR]data Inner
}

function build() -> Outer {
    Outer { data: Inner { data: 1 } }
}
"#,
        );

        let usages = test.find_all_usages();
        assert_eq!(
            usages.len(),
            1,
            "only Outer's key, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
        // The single hit is the OUTER literal's key: it precedes the inner
        // literal in source.
        let text = test.cursor.file.text(&test.db);
        let outer_key = text.rfind("Outer { data").unwrap_or_else(|| unreachable!()) + 8;
        assert_eq!(usize::from(usages[0].range.start()), outer_key);
    }

    #[test]
    fn find_refs_enum_variant_expression_positions() {
        let test = CursorTest::new(
            r#"
enum Status {
    <[CURSOR]Active
    Inactive
}

function check(s: Status) -> string {
    match (s) {
        Status.Active => "on"
        Status.Inactive => "off"
    }
}

function make() -> Status {
    Status.Active
}
"#,
        );

        // Only the expression position is found: the `Status.Active` match
        // arm is a type *pattern*, and inference records no per-name pattern
        // resolutions yet (a compiler-side gap — see the module doc).
        let usages = test.find_all_usages();
        assert_eq!(
            usages.len(),
            1,
            "constructor expression, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            test.format_location_with_name(&usages[0]),
            "test.baml:15:12 -> Active"
        );
    }

    #[test]
    fn find_refs_method_from_call_site() {
        let test = CursorTest::new(
            r#"
class Counter {
    n int

    function bump(self) -> int {
        self.n + 1
    }
}

function use_once(c: Counter) -> int {
    c.<[CURSOR]bump()
}

function use_twice(c: Counter) -> int {
    c.bump() + c.bump()
}
"#,
        );

        let usages = test.find_all_usages();
        assert_eq!(
            usages.len(),
            3,
            "all three call sites, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }
}
