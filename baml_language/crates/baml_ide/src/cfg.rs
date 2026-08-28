//! Playground control-flow graphs: build one function's (or test's) graph
//! from its PPIR body and inline user-function callees rustdoc-style.
//!
//! Salvaged from the pre-rework `ProjectDatabase` methods; every `self`
//! became `db: &dyn ppir::Db` plus the compiler-visible `files` slice the
//! old `file_map` walks iterated. Consumed by the playground host
//! (`requestControlFlowGraph`, run-overlay pinning).

use std::collections::{HashMap, HashSet};

use baml_base::SourceFile;

/// Cap on the total node count of a fully-inlined control-flow graph. Callee
/// graphs are copied into every call site, so an uncapped graph grows as
/// `fan_out^depth` on deep call chains; once the budget is reached remaining
/// calls stay plain call nodes.
const CFG_EXPANSION_NODE_BUDGET: usize = 5_000;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CfgExpansionCacheKey {
    callee_name: String,
    active_expansions: Vec<String>,
}

/// State threaded through one top-level [`ProjectDatabase::ast_control_flow_graph`] build.
#[derive(Default)]
struct CfgExpansionCtx {
    /// Functions currently being expanded (cycle guard).
    expanding: HashSet<String>,
    /// Fully-expanded callee graphs keyed by the active expansion context.
    /// `None` records callees with no buildable graph so they are not retried
    /// per equivalent site.
    cache: HashMap<
        CfgExpansionCacheKey,
        Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>>,
    >,
}

type CfgDispatchBindings = HashMap<String, baml_type::Ty>;

enum CfgCallTarget<'db> {
    Function {
        loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
        display_name: String,
        dispatch_bindings: CfgDispatchBindings,
    },
    UnresolvedName(String),
}

impl CfgExpansionCtx {
    fn cache_key(&self, callee_name: String) -> CfgExpansionCacheKey {
        // The recursion guard depends on membership in `expanding`, not call
        // order, so a sorted active set is the safe memoization context.
        let mut active_expansions = self.expanding.iter().cloned().collect::<Vec<_>>();
        active_expansions.sort();
        CfgExpansionCacheKey {
            callee_name,
            active_expansions,
        }
    }
}

pub fn ast_control_flow_graph(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    function_name: &str,
) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
    let mut ctx = CfgExpansionCtx::default();
    ast_control_flow_graph_impl(db, files, function_name, &mut ctx)
        .or_else(|| ast_test_control_flow_graph_impl(db, files, function_name, &mut ctx))
}

/// Build a graph for a statically named top-level `test "..." { ... }`
/// declaration. New-style tests are lowered into lambdas passed to the
/// per-file `$init_test_*` function, so they do not appear in
/// `file_functions`. The test registry exposes their canonical names to
/// the playground (`root[.namespace]::name`); recover the matching lambda
/// from that synthesized registration and graph its body directly.
fn ast_test_control_flow_graph_impl(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    test_name: &str,
    ctx: &mut CfgExpansionCtx,
) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
    use baml_compiler2_ast::{Expr, FunctionBodyDef, Item};
    use baml_compiler2_visualization::control_flow::{
        NodeType, build_control_flow_graph_from_expr,
    };
    use baml_type::Literal;

    if !ctx.expanding.insert(test_name.to_string()) {
        return None;
    }

    let mut result = None;
    'files: for &source_file in files {
        let ast = baml_compiler2_hir::file_ast(db, source_file);
        for item in &ast.items {
            let Item::Function(init_function) = item else {
                continue;
            };
            if !init_function.name.as_str().starts_with("$init_test") {
                continue;
            }
            let Some(FunctionBodyDef::Expr(registration_body, registration_source_map)) =
                init_function.body.as_ref()
            else {
                continue;
            };
            let Some(&init_function_loc) =
                baml_compiler2_ppir::item_data::file_functions(db, source_file)
                    .iter()
                    .find(|&&loc| {
                        baml_compiler2_ppir::item_data::function_data(db, loc).name
                            == init_function.name
                    })
            else {
                continue;
            };

            let mut duplicate_counts = HashMap::<String, usize>::new();
            for (_, expr) in registration_body.exprs.iter() {
                let Expr::Call { callee, args, .. } = expr else {
                    continue;
                };
                let Expr::Path(callee_segments) = &registration_body.exprs[*callee] else {
                    continue;
                };
                if callee_segments.last().map(AsRef::<str>::as_ref) != Some("register_test_at")
                    || args.len() != 4
                {
                    continue;
                }

                let Expr::Literal(Literal::String(owner)) = &registration_body.exprs[args[0].expr]
                else {
                    continue;
                };
                let Expr::Literal(Literal::String(name)) = &registration_body.exprs[args[1].expr]
                else {
                    // Runtime-computed test names cannot be identified
                    // statically from the canonical registry name.
                    continue;
                };
                let canonical_base = format!("{owner}::{name}");
                let duplicate_count = duplicate_counts
                    .entry(canonical_base.clone())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                let canonical_name = if *duplicate_count == 1 {
                    canonical_base
                } else {
                    format!("{canonical_base}#{duplicate_count}")
                };
                if canonical_name != test_name {
                    continue;
                }

                let Expr::Lambda(test_lambda) = &registration_body.exprs[args[2].expr] else {
                    continue;
                };
                // The test body is an expression in the registration body's
                // own arena, so it shares that body's source map.
                let test_body = registration_body;
                let mut graph =
                    build_control_flow_graph_from_expr(test_name, test_body, test_lambda.body);
                attach_source_spans_to_graph(db, &mut graph, source_file, registration_source_map);

                let test_name_span = source_map_expr_range(registration_source_map, args[1].expr)
                    .and_then(|range| source_span_for_range(db, source_file, range));
                if let Some(root) = graph
                    .nodes
                    .values_mut()
                    .find(|node| node.node_type == NodeType::FunctionRoot)
                {
                    root.source_span = test_name_span
                        .or_else(|| source_span_for_range(db, source_file, test_lambda.span));
                }

                expand_user_function_calls_in_graph(
                    db,
                    files,
                    &mut graph,
                    init_function_loc,
                    test_body,
                    &CfgDispatchBindings::new(),
                    ctx,
                );
                result = Some(graph);
                break 'files;
            }
        }
    }

    ctx.expanding.remove(test_name);
    result
}

fn ast_control_flow_graph_impl(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    function_name: &str,
    ctx: &mut CfgExpansionCtx,
) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
    let func_loc = find_function_loc(db, files, function_name)?;
    ast_control_flow_graph_for_loc(
        db,
        files,
        func_loc,
        function_name,
        &CfgDispatchBindings::new(),
        ctx,
    )
}

fn ast_control_flow_graph_for_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    function_name: &str,
    dispatch_bindings: &CfgDispatchBindings,
    ctx: &mut CfgExpansionCtx,
) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
    use baml_compiler2_visualization::control_flow::{
        build_control_flow_graph_from_ast, build_llm_control_flow_graph,
    };

    let function_identity = cfg_function_identity(db, func_loc);
    if !ctx.expanding.insert(function_identity.clone()) {
        return None;
    }

    let source_file = func_loc.file(db);
    let func_span = baml_compiler2_ppir::item_data::function_source_map(db, func_loc).span;
    let body = baml_compiler2_ppir::function_body(db, func_loc);

    // LLM functions desugar to Expr bodies, so it is `declarative_meta`
    // (surfaced span-free by `function_llm_meta`) — not the body variant —
    // that marks them.
    let result =
        if let Some(llm_meta) = baml_compiler2_ppir::item_data::function_llm_meta(db, func_loc) {
            let client_name = llm_meta
                .client_name
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unknown".to_string());
            let mut graph = build_llm_control_flow_graph(function_name, &client_name);
            if let Some(source_span) = source_span_for_range(db, source_file, func_span) {
                if let Some(node) = graph.nodes.values_mut().next() {
                    node.source_span = Some(source_span);
                }
            }
            Some(graph)
        } else {
            match body.as_ref() {
                baml_compiler2_hir::body::FunctionBody::Expr(expr_body) => {
                    let mut graph = build_control_flow_graph_from_ast(function_name, expr_body);
                    if let Some(source_map) =
                        baml_compiler2_ppir::function_body_source_map(db, func_loc)
                    {
                        attach_source_spans_to_graph(db, &mut graph, source_file, &source_map);
                    }
                    // The FunctionRoot node has no `source_expr`, so
                    // `attach_source_spans_to_graph` skips it. Point it at the
                    // whole function declaration so clicking the root in the
                    // playground selects the function (mirrors the LLM path above).
                    if let Some(root_span) = source_span_for_range(db, source_file, func_span) {
                        if let Some(root) = graph.nodes.values_mut().find(|node| {
                            node.node_type
                            == baml_compiler2_visualization::control_flow::NodeType::FunctionRoot
                        }) {
                            root.source_span.get_or_insert(root_span);
                        }
                    }
                    expand_user_function_calls_in_graph(
                        db,
                        files,
                        &mut graph,
                        func_loc,
                        expr_body,
                        dispatch_bindings,
                        ctx,
                    );
                    Some(graph)
                }
                baml_compiler2_hir::body::FunctionBody::Builtin(_)
                | baml_compiler2_hir::body::FunctionBody::Missing => None,
            }
        };

    ctx.expanding.remove(&function_identity);
    result
}

fn expand_user_function_calls_in_graph<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    graph: &mut baml_compiler2_visualization::control_flow::ControlFlowGraph,
    caller: baml_compiler2_hir::loc::FunctionLoc<'db>,
    body: &baml_compiler2_ast::ExprBody,
    dispatch_bindings: &CfgDispatchBindings,
    ctx: &mut CfgExpansionCtx,
) {
    use baml_compiler2_visualization::control_flow::NodeType;

    for (call_expr, target) in call_sites_by_source_expr(db, caller, body, dispatch_bindings) {
        let Some((call_node_id, is_return_node)) = graph
            .nodes
            .values()
            .find(|node| node.source_expr == Some(call_expr))
            .map(|node| (node.id, matches!(node.node_type, NodeType::Return)))
        else {
            continue;
        };
        if is_return_node {
            continue;
        }

        let (callee_header, callee_graph) = match target {
            CfgCallTarget::Function {
                loc,
                display_name,
                dispatch_bindings,
            } => {
                let function_identity = cfg_function_identity(db, loc);
                if ctx.expanding.contains(&function_identity) {
                    continue;
                }
                let key = cfg_expansion_key(db, loc, &dispatch_bindings);
                let cache_key = ctx.cache_key(key.clone());
                let graph = if let Some(cached) = ctx.cache.get(&cache_key) {
                    cached.clone()
                } else {
                    let built = ast_control_flow_graph_for_loc(
                        db,
                        files,
                        loc,
                        &display_name,
                        &dispatch_bindings,
                        ctx,
                    )
                    .map(std::sync::Arc::new);
                    ctx.cache.insert(cache_key, built.clone());
                    built
                };
                (function_header_title_for_loc(db, loc), graph)
            }
            CfgCallTarget::UnresolvedName(callee_name) => {
                let cache_key = ctx.cache_key(callee_name.clone());
                let graph = if let Some(cached) = ctx.cache.get(&cache_key) {
                    cached.clone()
                } else {
                    let built = ast_control_flow_graph_impl(db, files, &callee_name, ctx)
                        .map(std::sync::Arc::new);
                    ctx.cache.insert(cache_key, built.clone());
                    built
                };
                (function_header_title(db, files, &callee_name), graph)
            }
        };

        // Recursion is cut at the call node rather than cached: a graph
        // truncated by the cycle guard must not be reused at sites where
        // the callee is not part of the active expansion chain.
        let Some(callee_graph) = callee_graph else {
            continue;
        };

        if is_single_llm_graph(&callee_graph) {
            // Calls to LLM functions always render. Mark the call node so
            // the visualization prep keeps it (and styles it as an LLM
            // call) instead of pruning it like a plain function call.
            let client_name = callee_graph
                .nodes
                .values()
                .next()
                .and_then(|node| node.llm_client.clone());
            if let Some(node) = graph.nodes.get_mut(&call_node_id) {
                node.llm_client = Some(client_name.unwrap_or_else(|| "unknown".to_string()));
                if matches!(node.node_type, NodeType::OtherScope) {
                    node.node_type = NodeType::LlmFunction;
                }
            }
            continue;
        }

        // A `//#` header directly above the callee's declaration names the
        // call node: `//# process stuff` above `function somefunc()` makes
        // every `somefunc()` call render as a "process stuff" node.
        if let Some(title) = callee_header {
            if let Some(node) = graph.nodes.get_mut(&call_node_id) {
                node.label = title;
                if matches!(node.node_type, NodeType::OtherScope) {
                    node.node_type = NodeType::HeaderContextEnter;
                }
            }
        }

        // Even with per-callee memoization the merged output copies the
        // callee graph at every call site, so deep chains still multiply
        // node counts. Stop inlining once the graph reaches the budget;
        // remaining calls render as plain call nodes.
        if graph.nodes.len() + callee_graph.nodes.len() > CFG_EXPANSION_NODE_BUDGET {
            continue;
        }
        merge_callee_graph_under_call_node(graph, call_node_id, &callee_graph);
    }
}

/// Find the `//#` header comment immediately above a function declaration,
/// if any. Blank lines and regular `//` comments between the header and
/// the declaration are skipped; any other code stops the search. If multiple
/// same-named declarations have different headers, do not guess which one a
/// name-only call resolved to.
fn function_header_title(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    function_name: &str,
) -> Option<String> {
    let mut unique_title = None;
    for &source_file in files {
        for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, source_file) {
            let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
            if !crate::symbols::function_name_matches_source_name(
                db,
                source_file,
                &func_data.name,
                function_name,
            ) {
                continue;
            }
            let func_span = baml_compiler2_ppir::item_data::function_source_map(db, func_loc).span;
            let text = source_file.text(db);
            let start = usize::from(func_span.start()).min(text.len());
            if let Some(title) = header_title_above(&text[..start]) {
                match &unique_title {
                    Some(existing) if existing != &title => return None,
                    Some(_) => {}
                    None => unique_title = Some(title),
                }
            }
        }
    }
    unique_title
}

fn function_header_title_for_loc(
    db: &dyn baml_compiler2_ppir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> Option<String> {
    let source_file = func_loc.file(db);
    let func_span = baml_compiler2_ppir::item_data::function_source_map(db, func_loc).span;
    let text = source_file.text(db);
    let start = usize::from(func_span.start()).min(text.len());
    header_title_above(&text[..start])
}

fn find_function_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    function_name: &str,
) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
    for &source_file in files {
        for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, source_file) {
            let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
            if crate::symbols::function_name_matches_source_name(
                db,
                source_file,
                &func_data.name,
                function_name,
            ) {
                return Some(func_loc);
            }
        }
    }
    None
}

fn function_display_name(
    db: &dyn baml_compiler2_ppir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> String {
    use baml_compiler2_ppir::item_data::MethodOwner;

    let data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
    match baml_compiler2_ppir::item_data::method_owner(db, func_loc) {
        Some(MethodOwner::Class(class_loc)) => {
            let class = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            format!("{}.{}", class.name, data.name)
        }
        Some(MethodOwner::Interface(iface_loc)) => {
            let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            format!("{}.{}", iface.name, data.name)
        }
        Some(MethodOwner::FreeImpl(_)) | None => {
            crate::symbols::playground_function_name_for_file(db, func_loc.file(db), &data.name)
        }
    }
}

fn cfg_expansion_key(
    db: &dyn baml_compiler2_ppir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    dispatch_bindings: &CfgDispatchBindings,
) -> String {
    let mut bindings = dispatch_bindings
        .iter()
        .map(|(name, ty)| format!("{name}={ty:?}"))
        .collect::<Vec<_>>();
    bindings.sort();
    format!(
        "{}<{}>",
        cfg_function_identity(db, func_loc),
        bindings.join(",")
    )
}

fn cfg_function_identity(
    db: &dyn baml_compiler2_ppir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> String {
    format!(
        "{}#{}",
        func_loc.file(db).path(db).display(),
        func_loc.id(db).as_u32()
    )
}

fn call_sites_by_source_expr<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    caller: baml_compiler2_hir::loc::FunctionLoc<'db>,
    body: &baml_compiler2_ast::ExprBody,
    dispatch_bindings: &CfgDispatchBindings,
) -> Vec<(u32, CfgCallTarget<'db>)> {
    use baml_compiler2_ast::Expr;

    let inference = Some(baml_compiler2_hir_ty::infer::infer_body(
        db,
        baml_compiler2_hir::body::BodyOwnerId::Function(caller),
    ));
    let mut calls = Vec::new();
    for (expr_id, expr) in body.exprs.iter() {
        let (callee, args) = match expr {
            Expr::Call { callee, args, .. } | Expr::OptionalCall { callee, args } => {
                (*callee, args)
            }
            _ => continue,
        };

        if let Some(inference) = inference {
            if let Some(loc) =
                resolved_call_function(db, inference, body, callee, dispatch_bindings)
            {
                calls.push((
                    expr_id.into_raw().into_u32(),
                    CfgCallTarget::Function {
                        loc,
                        display_name: function_display_name(db, loc),
                        dispatch_bindings: dispatch_bindings_for_call(
                            db, inference, body, expr_id, args, loc,
                        ),
                    },
                ));
                continue;
            }
        }

        let Expr::Path(segments) = &body.exprs[callee] else {
            continue;
        };

        if let Some(loc) = resolve_path_function(db, caller.file(db), segments) {
            calls.push((
                expr_id.into_raw().into_u32(),
                CfgCallTarget::Function {
                    loc,
                    display_name: function_display_name(db, loc),
                    dispatch_bindings: inference
                        .map(|inference| {
                            dispatch_bindings_for_call(db, inference, body, expr_id, args, loc)
                        })
                        .unwrap_or_default(),
                },
            ));
            continue;
        }

        let callee_name = segments
            .iter()
            .map(AsRef::<str>::as_ref)
            .collect::<Vec<_>>()
            .join(".");
        calls.push((
            expr_id.into_raw().into_u32(),
            CfgCallTarget::UnresolvedName(callee_name),
        ));
    }
    calls
}

fn resolve_path_function<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    caller_file: SourceFile,
    callee_path: &[baml_base::Name],
) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
    use baml_compiler2_hir::{contributions::Definition, file_package, package::PackageId};
    use baml_compiler2_hir_ty::package_interface::ResolvedValue;

    let caller_package = file_package::file_package(db, caller_file);
    let package_id = PackageId::new(db, caller_package.package.clone());
    let resolution =
        baml_compiler2_hir_ty::package_interface::package_resolution_context(db, package_id);
    match resolution.resolve_value(db, callee_path, &caller_package.namespace_path) {
        Some(ResolvedValue::Source(Definition::Function(function))) => Some(function),
        _ => None,
    }
}

fn resolved_call_function<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    inference: &baml_compiler2_hir_ty::infer::InferenceResult<'db>,
    body: &baml_compiler2_ast::ExprBody,
    callee: baml_compiler2_ast::ExprId,
    dispatch_bindings: &CfgDispatchBindings,
) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
    use baml_compiler2_ast::Expr;
    use baml_compiler2_hir_ty::infer::MemberResolution;

    let resolution = inference.member_resolutions.get(&callee).or_else(|| {
        inference
            .path_resolutions
            .get(&callee)
            .and_then(|path| path.segments.last())
            .and_then(|segment| segment.resolution.as_ref())
    });

    match resolution {
        Some(
            MemberResolution::Free { func }
            | MemberResolution::BoundMethod { func, .. }
            | MemberResolution::UnboundMethod { func, .. }
            | MemberResolution::InterfaceConcreteMethod { func, .. },
        ) => Some(*func),
        Some(MemberResolution::InterfaceVirtualMethod { interface, method }) => {
            let receiver = match &body.exprs[callee] {
                Expr::MemberAccess { base, .. } | Expr::OptionalMemberAccess { base, .. } => {
                    match &body.exprs[*base] {
                        Expr::Path(segments) if segments.len() == 1 => Some(segments[0].as_str()),
                        _ => None,
                    }
                }
                Expr::Path(segments) if segments.len() >= 2 => {
                    segments.first().map(baml_base::Name::as_str)
                }
                _ => None,
            }?;
            let concrete = dispatch_bindings.get(receiver)?;
            interface_method_impl_loc(db, concrete, *interface, method)
        }
        Some(
            MemberResolution::Field { .. }
            | MemberResolution::Variant { .. }
            | MemberResolution::InterfaceVirtualField { .. }
            | MemberResolution::External(_)
            | MemberResolution::ExternalField { .. }
            | MemberResolution::ExternalVariant { .. }
            | MemberResolution::ExternalInterfaceVirtualField { .. },
        )
        | None => None,
    }
}

fn dispatch_bindings_for_call(
    db: &dyn baml_compiler2_ppir::Db,
    inference: &baml_compiler2_hir_ty::infer::InferenceResult<'_>,
    body: &baml_compiler2_ast::ExprBody,
    call_expr: baml_compiler2_ast::ExprId,
    args: &[baml_compiler2_ast::CallArg],
    callee: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> CfgDispatchBindings {
    use baml_compiler2_ast::Expr;
    use baml_compiler2_hir_ty::infer::MemberResolution;

    let params = &baml_compiler2_ppir::item_data::function_data(db, callee).params;
    let callee_expr = match &body.exprs[call_expr] {
        Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } => Some(*callee),
        _ => None,
    };
    let resolution = callee_expr.and_then(|callee_expr| {
        inference.member_resolutions.get(&callee_expr).or_else(|| {
            inference
                .path_resolutions
                .get(&callee_expr)
                .and_then(|path| path.segments.last())
                .and_then(|segment| segment.resolution.as_ref())
        })
    });
    // Call plans index only the arguments provided by the caller. A bound
    // method's declared `self` parameter is implicit, so shift those
    // indices back into the declaration's full parameter list.
    let implicit_self = usize::from(matches!(
        resolution,
        Some(
            MemberResolution::BoundMethod { .. }
                | MemberResolution::InterfaceConcreteMethod { .. }
                | MemberResolution::InterfaceVirtualMethod { .. }
        )
    ));
    let mut bindings = CfgDispatchBindings::new();
    let mut record = |param_index: usize, arg_expr: baml_compiler2_ast::ExprId| {
        let Some(param) = params.get(param_index) else {
            return;
        };
        let Some(concrete) = inference.type_of_expr.get(&arg_expr) else {
            return;
        };
        bindings.insert(param.name.to_string(), concrete.clone());
    };

    if let Some(plan) = inference.call_plans.get(&call_expr) {
        for binding in &plan.bindings {
            let baml_compiler2_hir_ty::infer::ParamBinding::Provided { param_index, arg } = binding
            else {
                continue;
            };
            record(param_index + implicit_self, *arg);
        }
    } else {
        for (position, arg) in args.iter().enumerate() {
            let param_index = arg
                .label
                .as_ref()
                .and_then(|label| params.iter().position(|param| &param.name == label))
                .unwrap_or(position + implicit_self);
            record(param_index, arg.expr);
        }
    }
    bindings
}

fn interface_method_impl_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    concrete: &baml_type::Ty,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    method_name: &baml_base::Name,
) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
    let method_of = |func_loc: &baml_compiler2_hir::loc::FunctionLoc<'db>| {
        baml_compiler2_ppir::item_data::function_data(db, *func_loc).name == *method_name
    };
    let mut methods = baml_compiler2_hir_ty::impls::impls_for_type(db, concrete)
        .into_iter()
        .filter_map(|resolved| resolved.source_block())
        .filter(|block| {
            baml_compiler2_hir_ty::interfaces::impl_data(db, *block)
                .as_ref()
                .is_ok_and(|data| data.interface == iface_loc)
        })
        .filter_map(|block| {
            // The impl's own override wins; an inherited interface
            // default method fills the slot otherwise.
            baml_compiler2_hir_ty::interfaces::impl_data(db, block)
                .as_ref()
                .ok()
                .and_then(|data| data.methods.iter().find(|loc| method_of(loc)).copied())
                .or_else(|| {
                    baml_compiler2_ppir::item_data::interface_data(db, iface_loc)
                        .default_methods
                        .iter()
                        .find(|loc| method_of(loc))
                        .copied()
                })
        });
    let method = methods.next()?;
    if methods.next().is_some() {
        return None;
    }
    Some(method)
}

fn is_single_llm_graph(
    graph: &baml_compiler2_visualization::control_flow::ControlFlowGraph,
) -> bool {
    graph.nodes.len() == 1
        && graph.nodes.values().any(|node| {
            matches!(
                node.node_type,
                baml_compiler2_visualization::control_flow::NodeType::LlmFunction
            )
        })
}

fn merge_callee_graph_under_call_node(
    graph: &mut baml_compiler2_visualization::control_flow::ControlFlowGraph,
    call_node_id: baml_compiler2_visualization::control_flow::NodeId,
    callee_graph: &baml_compiler2_visualization::control_flow::ControlFlowGraph,
) {
    use baml_compiler2_visualization::control_flow::{Edge, NodeId};

    let Some(root_id) = callee_graph
        .nodes
        .values()
        .find(|node| node.parent_node_id.is_none())
        .map(|node| node.id)
    else {
        return;
    };

    let mut next_raw = graph
        .nodes
        .keys()
        .map(NodeId::raw)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut remap: HashMap<NodeId, NodeId> = HashMap::new();
    for node in callee_graph.nodes.values() {
        if node.id == root_id {
            continue;
        }
        let new_id = NodeId::new(next_raw);
        next_raw = next_raw.saturating_add(1);
        remap.insert(node.id, new_id);
    }

    if remap.is_empty() {
        return;
    }

    for node in callee_graph.nodes.values() {
        if node.id == root_id {
            continue;
        }

        let mut node = node.clone();
        node.id = remap[&node.id];
        node.parent_node_id = match node.parent_node_id {
            Some(parent) if parent == root_id => Some(call_node_id),
            Some(parent) => remap.get(&parent).copied().or(Some(call_node_id)),
            None => Some(call_node_id),
        };
        graph.nodes.insert(node.id, node);
    }

    for edges in callee_graph.edges_by_src.values() {
        for edge in edges {
            let src = if edge.src == root_id {
                call_node_id
            } else if let Some(src) = remap.get(&edge.src).copied() {
                src
            } else {
                continue;
            };
            let Some(dst) = (if edge.dst == root_id {
                None
            } else {
                remap.get(&edge.dst).copied()
            }) else {
                continue;
            };

            graph.edges_by_src.entry(src).or_default().push(Edge {
                src,
                dst,
                label: edge.label.clone(),
            });
        }
    }
}

fn attach_source_spans_to_graph(
    db: &dyn baml_compiler2_ppir::Db,
    graph: &mut baml_compiler2_visualization::control_flow::ControlFlowGraph,
    source_file: SourceFile,
    source_map: &baml_compiler2_ast::AstSourceMap,
) {
    for node in graph.nodes.values_mut() {
        let Some(source_expr) = node.source_expr else {
            continue;
        };
        if let Some(source_span) =
            source_span_for_source_expr(db, source_file, source_map, source_expr)
        {
            node.source_span = Some(source_span);
        }
    }
}

fn source_span_for_source_expr(
    db: &dyn baml_compiler2_ppir::Db,
    source_file: SourceFile,
    source_map: &baml_compiler2_ast::AstSourceMap,
    source_expr: u32,
) -> Option<baml_compiler2_visualization::control_flow::SourceSpan> {
    let tag = baml_compiler2_visualization::control_flow::STMT_SOURCE_EXPR_TAG;
    let (raw, spans) = if source_expr & tag != 0 {
        (source_expr & !tag, &source_map.stmt_spans)
    } else {
        (source_expr, &source_map.expr_spans)
    };

    let idx = raw as usize;
    if idx >= spans.len() {
        return None;
    }

    let span_idx = la_arena::Idx::<text_size::TextRange>::from_raw(la_arena::RawIdx::from_u32(raw));
    source_span_for_range(db, source_file, spans[span_idx])
}

fn source_map_expr_range(
    source_map: &baml_compiler2_ast::AstSourceMap,
    expr_id: baml_compiler2_ast::ExprId,
) -> Option<text_size::TextRange> {
    let raw = expr_id.into_raw();
    if raw.into_u32() as usize >= source_map.expr_spans.len() {
        return None;
    }
    let span_idx = la_arena::Idx::<text_size::TextRange>::from_raw(raw);
    Some(source_map.expr_spans[span_idx])
}

fn source_span_for_range(
    db: &dyn baml_compiler2_ppir::Db,
    source_file: SourceFile,
    range: text_size::TextRange,
) -> Option<baml_compiler2_visualization::control_flow::SourceSpan> {
    let text = source_file.text(db);
    let len = u32::try_from(text.len()).ok()?;
    let start_offset: u32 = range.start().into();
    if start_offset > len {
        return None;
    }
    let end_offset: u32 = range.end().into();
    let end_offset = end_offset.min(len);
    let line_index = crate::line_index::LineIndex::new(text);
    let (line, column) = line_index.offset_to_position(start_offset)?;
    let (end_line, end_column) = line_index
        .offset_to_position(end_offset)
        .unwrap_or((line, column));

    Some(baml_compiler2_visualization::control_flow::SourceSpan {
        file_id: source_file.file_id(db).as_u32(),
        file_path: source_file.path(db).to_string_lossy().into_owned(),
        start_offset,
        end_offset,
        line,
        column,
        end_line,
        end_column,
    })
}

/// Scan backwards through the source text that precedes a declaration and
/// return the title of the nearest `//#` header comment, if it is separated
/// from the declaration only by blank lines and regular `//` comments.
fn header_title_above(before: &str) -> Option<String> {
    for line in before.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("//") else {
            // Reached code — no header directly above the declaration.
            return None;
        };
        if let Some(title) = rest.strip_prefix('#') {
            let title = title.trim_start_matches('#').trim();
            if title.is_empty() {
                return None;
            }
            return Some(title.to_string());
        }
        // Regular `//` or `///` comment between the header and the
        // declaration — keep scanning upwards.
    }
    None
}

#[cfg(test)]
mod tests {
    use baml_compiler2_visualization::control_flow::ControlFlowGraph;
    use baml_db::ProjectDatabase;

    use super::*;

    /// A workspace at `/tmp` on the source-root API, mirroring the old
    /// single-root fixture the salvaged tests were written against.
    fn test_db() -> (ProjectDatabase, baml_db::SourceRoot) {
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let root = db
            .add_source_root(baml_db::SourceRootSpec {
                path: std::path::PathBuf::from("/cfg-test"),
                package: baml_base::Name::new(baml_type::RESERVED_USER_PACKAGE),
                kind: baml_base::SourceRootKind::Workspace,
            })
            .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
        (db, root)
    }

    fn build_graph(db: &ProjectDatabase, name: &str) -> Option<ControlFlowGraph> {
        let files = baml_compiler2_hir::compiler2_all_files(db);
        ast_control_flow_graph(db, &files, name)
    }

    fn build_header_title(db: &ProjectDatabase, name: &str) -> Option<String> {
        let files = baml_compiler2_hir::compiler2_all_files(db);
        function_header_title(db, &files, name)
    }

    fn build_cursor_context(
        db: &ProjectDatabase,
        file_path: &str,
        byte_offset: u32,
    ) -> crate::cursor_context::CursorContext {
        let files = baml_compiler2_hir::compiler2_all_files(db);
        crate::cursor_context::playground_cursor_context(db, &files, file_path, byte_offset)
    }

    #[test]
    fn callee_graphs_are_still_inlined_per_call_site() {
        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/diamond.baml"),
            r#"
function Leaf(input: string) -> string {
  //# leaf work
  let a = input;
  a
}

function Mid(input: string) -> string {
  let a = Leaf(input);
  let b = Leaf(a);
  b
}

function Top(input: string) -> string {
  let a = Mid(input);
  let b = Mid(a);
  b
}
"#,
        );
        let leaf = build_graph(&db, "Leaf").unwrap();
        let mid = build_graph(&db, "Mid").unwrap();
        let top = build_graph(&db, "Top").unwrap();
        // Memoization must not change the inlined-output shape: every call
        // site still receives its own copy of the callee graph.
        assert!(
            mid.nodes.len() > leaf.nodes.len(),
            "Mid should contain inlined copies of Leaf ({} vs {})",
            mid.nodes.len(),
            leaf.nodes.len()
        );
        assert!(
            top.nodes.len() > mid.nodes.len(),
            "Top should contain inlined copies of Mid ({} vs {})",
            top.nodes.len(),
            mid.nodes.len()
        );
    }

    #[test]
    fn method_calls_inline_concrete_runner_graphs_through_generic_dispatch() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/runner.baml"),
            r#"
interface Runner<Input> {
  function run(self, input: Input) -> string throws never
}

class Task {
  function run<R extends Runner<Task>>(
self,
runner: R,
  ) -> string throws never {
//# Dispatch the task to its runner
runner.run(self)
  }
}

class Agent {
  implements Runner<Task> {
function run(self, input: Task) -> string throws never {
  //# Initialize the agent
  let steps = 0;
  //# Run agent steps until completion
  while (steps < 1) {
    //## Advance one agent step
    steps = steps + 1;
  }
  "done"
}
  }
}

function observe_an_agent() -> string throws never {
  let task = Task {};
  task.run(runner = Agent {})
}
"#,
        );

        let graph =
            build_graph(&db, "observe_an_agent").expect("expected graph for observe_an_agent");
        let labels = graph
            .nodes
            .values()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>();

        assert!(
            labels.contains(&"Dispatch the task to its runner"),
            "Task.run should be inlined into the entry graph; got {labels:?}"
        );
        assert!(
            labels.contains(&"Run agent steps until completion"),
            "the concrete Agent.run body should be inlined through Runner.run; got {labels:?}"
        );
        assert!(
            graph
                .nodes
                .values()
                .any(|node| node.node_type == NodeType::Loop),
            "the concrete Agent.run loop should be visible; got {labels:?}"
        );
        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            prepared.nodes.values().any(|node| {
                node.label == "Run agent steps until completion" || node.node_type == NodeType::Loop
            }),
            "the rendered graph should retain the concrete agent loop"
        );
    }

    #[test]
    fn recursive_callee_cache_is_scoped_by_active_expansions() {
        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/recursive-cache.baml"),
            r#"
//# a step
function A(input: string) -> string {
  let next = B(input);
  next
}

//# b step
function B(input: string) -> string {
  let looped = A(input);
  let done = Leaf(looped);
  done
}

//# leaf step
function Leaf(input: string) -> string {
  input
}

function Top(input: string) -> string {
  let first = A(input);
  let second = B(first);
  second
}
"#,
        );

        let graph = build_graph(&db, "Top").unwrap();
        let a_step_count = graph
            .nodes
            .values()
            .filter(|node| node.label == "a step")
            .count();

        assert!(
            a_step_count >= 2,
            "direct B expansion must not reuse a B graph truncated under A recursion; got {a_step_count} A call node(s)"
        );
    }

    #[test]
    fn deep_call_chains_are_capped_not_exponential() {
        let (mut db, root) = test_db();
        // Depth 12, fan-out 3: fully inlined and uncapped this is 3^12 ≈ 531k
        // nodes per top-level function (and exponential build time without
        // per-callee memoization).
        let mut src = String::from(
            "function F12(input: string) -> string {\n  //# leaf\n  let a = input;\n  a\n}\n",
        );
        for i in (0..12).rev() {
            use std::fmt::Write as _;
            let callee = i + 1;
            let _ = write!(
                src,
                "function F{i}(input: string) -> string {{\n  let v0 = F{callee}(input);\n  let v1 = F{callee}(v0);\n  let v2 = F{callee}(v1);\n  v2\n}}\n"
            );
        }
        db.add_or_update_file_in(root, std::path::Path::new("/cfg-test/chain.baml"), &src);

        let graph = build_graph(&db, "F0").unwrap();
        assert!(
            graph.nodes.len() <= CFG_EXPANSION_NODE_BUDGET,
            "inlined graph must respect the node budget, got {}",
            graph.nodes.len()
        );
    }

    #[test]
    fn header_above_if_keeps_all_branch_arms() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };
        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/wf.baml"),
            r#"
function classify(text: string) -> string {
  let t = text.to_lower_case();
  //# check sentiment
  if (t.includes("love")) { "positive" } else { "negative" }
}
"#,
        );
        let graph = build_graph(&db, "classify").unwrap();
        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        // The `//# check sentiment` header sits directly above the if, so the
        // whole branch group and both arms must survive pruning, even though
        // neither arm holds its own anchor (header / LLM call).
        let arms = prepared
            .nodes
            .values()
            .filter(|n| matches!(n.node_type, NodeType::BranchArm))
            .count();
        assert!(
            arms >= 2,
            "a header directly above an if should keep all its branch arms; got {arms}"
        );
        assert!(
            prepared
                .nodes
                .values()
                .any(|n| matches!(n.node_type, NodeType::BranchGroup)),
            "the annotated branch group should survive pruning"
        );
    }

    #[test]
    fn test_ast_control_flow_graph_with_headers() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let (mut db, root) = test_db();
        // Use header comments (//#) inside the function body — these produce
        // HeaderContextEnter nodes which survive the flattening pipeline.
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/workflow.baml"),
            r#"
function Workflow(input: string) -> string {
//# Prepare
let x = input;
//# Process
if (true) { x } else { "fallback" }
}
"#,
        );

        let graph = build_graph(&db, "Workflow");
        assert!(
            graph.is_some(),
            "ast_control_flow_graph should return a graph for a known function"
        );
        let graph = graph.unwrap();

        // Should have at least: FunctionRoot + two HeaderContextEnter nodes.
        assert!(
            graph.nodes.len() >= 3,
            "expected at least 3 nodes (root + 2 headers), got {}",
            graph.nodes.len()
        );

        // The root node should have FunctionRoot type.
        let root = graph.nodes.values().next().unwrap();
        assert!(
            matches!(root.node_type, NodeType::FunctionRoot),
            "first node should be FunctionRoot, got {:?}",
            root.node_type
        );
        // The root carries the function's declaration span so clicking it in
        // the playground selects the function (it has no `source_expr`, so this
        // is attached explicitly rather than via the source map).
        let root_span = root
            .source_span
            .as_ref()
            .expect("FunctionRoot should have a source span");
        assert!(
            root_span.end_offset > root_span.start_offset,
            "FunctionRoot span should be non-empty"
        );

        // There should be at least two HeaderContextEnter nodes.
        let header_count = graph
            .nodes
            .values()
            .filter(|n| matches!(n.node_type, NodeType::HeaderContextEnter))
            .count();
        assert!(
            header_count >= 2,
            "expected at least 2 HeaderContextEnter nodes, got {header_count}"
        );
        assert!(
            graph
                .nodes
                .values()
                .filter(|n| matches!(n.node_type, NodeType::HeaderContextEnter))
                .all(|n| n.source_span.is_some()),
            "header graph nodes should include source spans"
        );

        // Edges should be non-empty.
        assert!(!graph.edges_by_src.is_empty(), "graph should have edges");
    }

    #[test]
    fn graph_source_spans_use_vscode_utf16_columns() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let (mut db, root) = test_db();
        let src = r#"function Workflow() -> string { let rocket = "🚀"; Summarize(rocket) }"#;
        db.add_or_update_file_in(root, std::path::Path::new("/cfg-test/workflow.baml"), src);

        let graph = build_graph(&db, "Workflow").unwrap();
        let call_span = graph
            .nodes
            .values()
            .find(|node| {
                matches!(node.node_type, NodeType::OtherScope) && node.label == "Summarize(rocket)"
            })
            .and_then(|node| node.source_span.as_ref())
            .expect("call graph node should have a source span");

        let byte_start = src.find("Summarize(rocket)").unwrap();
        let byte_end = byte_start + "Summarize(rocket)".len();
        assert_eq!(call_span.start_offset, u32::try_from(byte_start).unwrap());
        assert_eq!(call_span.end_offset, u32::try_from(byte_end).unwrap());
        assert_eq!(
            call_span.column,
            u32::try_from(src[..byte_start].encode_utf16().count()).unwrap()
        );
        assert_eq!(
            call_span.end_column,
            u32::try_from(src[..byte_end].encode_utf16().count()).unwrap()
        );
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)] // tiny test fixtures fit in u32
    fn cursor_in_header_region_selects_governing_header() {
        use baml_compiler2_visualization::control_flow::{NodeType, STMT_SOURCE_EXPR_TAG};

        let (mut db, root) = test_db();
        let src = r#"
function Workflow(input: string) -> string {
//# Prepare
let x = input;
//# Process
if (input == "go") {
    "yes"
} else {
    "no"
}
}
"#;
        db.add_or_update_file_in(root, std::path::Path::new("/cfg-test/wf.baml"), src);

        // The "Process" header node's tagged source_expr.
        let graph = build_graph(&db, "Workflow").unwrap();
        let process_expr = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::HeaderContextEnter) && n.label == "Process")
            .and_then(|n| n.source_expr)
            .expect("Process header should exist with a source_expr");
        assert!(process_expr & STMT_SOURCE_EXPR_TAG != 0);

        // Cursor inside the if-arm (`"yes"`) — that arm is not itself a rendered
        // node, but it lives in the region governed by "//# Process".
        let offset = (src.find("\"yes\"").unwrap() as u32) + 1;
        let ctx = build_cursor_context(&db, "/cfg-test/wf.baml", offset);
        assert!(
            ctx.source_expr_candidates.contains(&process_expr),
            "cursor inside the Process region should offer the Process header; got {:?}",
            ctx.source_expr_candidates
        );

        // Cursor on the `let x = input;` line is governed by "//# Prepare", not
        // "//# Process" (the later header only governs from its own line down).
        let prepare_expr = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::HeaderContextEnter) && n.label == "Prepare")
            .and_then(|n| n.source_expr)
            .expect("Prepare header should exist with a source_expr");
        let let_offset = (src.find("let x = input;").unwrap() as u32) + 4;
        let let_ctx = build_cursor_context(&db, "/cfg-test/wf.baml", let_offset);
        assert!(
            let_ctx.source_expr_candidates.contains(&prepare_expr),
            "cursor on the let line should offer the Prepare header; got {:?}",
            let_ctx.source_expr_candidates
        );
        assert!(
            !let_ctx.source_expr_candidates.contains(&process_expr),
            "the later Process header must not govern lines above it"
        );
    }

    #[test]
    fn test_ast_control_flow_graph_not_found() {
        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/test.baml"),
            r#"function Simple(x: int) -> int { x + 1 }"#,
        );

        // Non-existent function should return None.
        let graph = build_graph(&db, "DoesNotExist");
        assert!(graph.is_none(), "should return None for unknown function");
    }

    #[test]
    fn test_ast_control_flow_graph_accepts_playground_qualified_namespace_name() {
        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/ns_demo/workflow.baml"),
            r#"
function Workflow(input: string) -> string {
input
}
"#,
        );

        assert!(
            build_graph(&db, "Workflow").is_some(),
            "legacy bare lookup should keep working"
        );
        assert!(
            build_graph(&db, "demo.Workflow").is_some(),
            "playground-qualified lookup should resolve the namespaced function"
        );
    }

    #[test]
    fn test_ast_control_flow_graph_llm_is_single_semantic_node() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/llm.baml"),
            r##"
function Summarize(input: string) -> string {
client: GPT4
prompt: `Summarize ${input}`
}
"##,
        );

        let graph = build_graph(&db, "Summarize").expect("expected graph for LLM function");

        assert_eq!(graph.nodes.len(), 1);
        let node = graph.nodes.values().next().unwrap();
        assert!(matches!(node.node_type, NodeType::LlmFunction));
        assert_eq!(node.label, "Summarize");
        assert_eq!(node.llm_client.as_deref(), Some("GPT4"));
        let source_span = node.source_span.as_ref().expect("LLM node has source span");
        assert_eq!(source_span.file_path, "/cfg-test/llm.baml");
        assert!(source_span.end_offset > source_span.start_offset);
        assert!(graph.edges_by_src.is_empty());
    }

    #[test]
    fn test_ast_control_flow_graph_expands_user_function_match_at_call_site() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let (mut db, root) = test_db();
        let path = std::path::Path::new("/cfg-test/game.baml");
        let source = r#"
function ScoreGuess(outcome: string) -> string {
match (outcome) {
    "hit" => {
        //# matched correct
        "correct"
    }
    _ => {
        //# matched default
        "wrong"
    }
}
}

function GuessingGame() -> string {
let outcome = "hit";
ScoreGuess(outcome)
}
"#;
        db.add_or_update_file_in(root, path, source);

        let call_offset =
            u32::try_from(source.rfind("ScoreGuess(outcome)").expect("call exists")).unwrap();
        let call_ctx = build_cursor_context(&db, path.to_str().unwrap(), call_offset);
        assert_eq!(call_ctx.function_name.as_deref(), Some("ScoreGuess"));
        assert_eq!(
            call_ctx.source_expr_function_name.as_deref(),
            Some("GuessingGame"),
            "call-site expression ids should be owned by the caller"
        );

        let match_offset = u32::try_from(source.find("match (outcome)").expect("match exists"))
            .expect("offset fits");
        let match_ctx = build_cursor_context(&db, path.to_str().unwrap(), match_offset);
        assert_eq!(match_ctx.function_name.as_deref(), Some("ScoreGuess"));
        assert_eq!(
            match_ctx.source_expr_function_name.as_deref(),
            Some("ScoreGuess"),
            "callee body expression ids should be owned by the callee"
        );

        let graph = build_graph(&db, "GuessingGame").expect("expected graph for GuessingGame");

        let call_node = graph
            .nodes
            .values()
            .find(|node| node.label == "ScoreGuess(outcome)")
            .expect("caller graph should contain the ScoreGuess call node");
        let match_node = graph
            .nodes
            .values()
            .find(|node| {
                matches!(node.node_type, NodeType::BranchGroup)
                    && node.label == "match (outcome)"
                    && node.log_filter_key.starts_with("ScoreGuess|")
            })
            .expect("callee match should be expanded under the call node");
        assert_eq!(match_node.parent_node_id, Some(call_node.id));
        assert!(
            graph.nodes.values().any(|node| {
                matches!(node.node_type, NodeType::HeaderContextEnter)
                    && node.label == "matched correct"
                    && node.log_filter_key.starts_with("ScoreGuess|")
            }),
            "expanded match arms should keep their branch header nodes"
        );

        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        let call_node = prepared
            .nodes
            .get(&call_node.id)
            .expect("call node should remain visible");
        assert!(
            call_node.is_container,
            "call node should become a visualization container for the expanded callee graph"
        );
        let match_node = prepared
            .nodes
            .values()
            .find(|node| {
                matches!(node.node_type, NodeType::BranchGroup)
                    && node.label == "match (outcome)"
                    && node.log_filter_key.starts_with("ScoreGuess|")
            })
            .expect("prepared graph should keep the expanded match group");
        let edge_labels: Vec<_> = prepared
            .edges_by_src
            .get(&match_node.id)
            .map(|edges| {
                edges
                    .iter()
                    .filter_map(|edge| edge.label.as_deref())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            edge_labels.contains(&r#""hit""#) && edge_labels.contains(&"default"),
            "prepared match fan-out edges should preserve match arm labels, got {edge_labels:?}"
        );
    }

    #[test]
    fn test_llm_call_node_is_marked_and_always_rendered() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/wf.baml"),
            r##"
function Summarize(input: string) -> string {
client: GPT4
prompt: `Summarize ${input}`
}

function Workflow(input: string) -> string {
let x = Summarize(input);
x
}
"##,
        );

        let graph = build_graph(&db, "Workflow").expect("expected graph for Workflow");
        let call_node = graph
            .nodes
            .values()
            .find(|n| n.label == "Summarize(input)")
            .expect("caller graph should contain the Summarize call node");
        assert!(
            matches!(call_node.node_type, NodeType::LlmFunction),
            "LLM call node should be marked as LlmFunction, got {:?}",
            call_node.node_type
        );
        assert_eq!(call_node.llm_client.as_deref(), Some("GPT4"));

        // Even with no //# headers anywhere, the LLM call must survive
        // visualization prep.
        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            prepared.nodes.contains_key(&call_node.id),
            "LLM call must always render"
        );
    }

    #[test]
    fn test_cross_namespace_llm_call_node_is_marked_and_rendered() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/ns_workflows/ns_prompts/summarize.baml"),
            r##"
function Summarize(input: string) -> string {
client: GPT4
prompt: `Summarize ${input}`
}
"##,
        );
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/ns_workflows/workflow.baml"),
            r#"
function Workflow(input: string) -> string {
prompts.Summarize(input)
}
"#,
        );

        let graph =
            build_graph(&db, "workflows.Workflow").expect("expected graph for workflows.Workflow");
        let call_node = graph
            .nodes
            .values()
            .find(|node| node.label == "prompts.Summarize(input)")
            .expect("caller graph should contain the cross-namespace LLM call node");
        assert!(
            matches!(call_node.node_type, NodeType::LlmFunction),
            "cross-namespace LLM call node should be marked as LlmFunction, got {:?}",
            call_node.node_type
        );
        assert_eq!(call_node.llm_client.as_deref(), Some("GPT4"));

        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            prepared.nodes.contains_key(&call_node.id),
            "cross-namespace LLM call must survive visualization preparation"
        );
    }

    #[test]
    fn test_dependency_call_does_not_expand_same_named_user_function() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/ns_http/fetch.baml"),
            r##"
function fetch(input: string) -> string {
client: UserClient
prompt: `User fetch ${input}`
}
"##,
        );
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/workflow.baml"),
            r#"
function Workflow() -> int {
let response = baml.http.fetch("https://example.com");
response.status
}
"#,
        );

        let graph = build_graph(&db, "Workflow").expect("expected graph for Workflow");
        let call_node = graph
            .nodes
            .values()
            .find(|node| node.label.contains("baml.http.fetch"))
            .expect("caller graph should contain the dependency call node");
        assert!(
            matches!(call_node.node_type, NodeType::OtherScope),
            "dependency call must not be marked from the same-named user function, got {:?}",
            call_node.node_type
        );
        assert_eq!(call_node.llm_client, None);
    }

    #[test]
    fn test_function_level_header_labels_call_nodes() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/wf.baml"),
            r#"
//# process stuff
function somefunc(x: int) -> int {
//# inner step
x + 1
}

//# do the thing

// a regular note between the header and the function is fine
function plain(x: int) -> int { x + 1 }

function Caller(x: int) -> int {
let a = somefunc(x);
let b = plain(a);
b
}
"#,
        );

        let graph = build_graph(&db, "Caller").expect("expected graph for Caller");

        let somefunc_node = graph
            .nodes
            .values()
            .find(|n| n.label == "process stuff")
            .expect("somefunc call should be relabeled from its function-level header");
        assert!(matches!(
            somefunc_node.node_type,
            NodeType::HeaderContextEnter
        ));
        // The callee's body headers nest under the relabeled call node.
        assert!(
            graph
                .nodes
                .values()
                .any(|n| n.label == "inner step" && n.log_filter_key.starts_with("somefunc|")),
            "callee body headers should be expanded under the call node"
        );

        let plain_node = graph
            .nodes
            .values()
            .find(|n| n.label == "do the thing")
            .expect("plain call should be relabeled from its function-level header");

        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            prepared.nodes.contains_key(&somefunc_node.id),
            "annotated function call must render"
        );
        assert!(
            prepared.nodes.contains_key(&plain_node.id),
            "function-level header alone is enough to render the call node"
        );
    }

    #[test]
    fn test_function_header_title_keeps_searching_after_missing_header() {
        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/dupe.baml"),
            r#"
function helper(x: int) -> int { x }

//# titled helper
function helper(x: int) -> int { x + 1 }
"#,
        );

        assert_eq!(
            build_header_title(&db, "helper"),
            Some("titled helper".to_string())
        );
    }

    #[test]
    fn test_function_header_title_ignores_ambiguous_same_name_headers() {
        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/dupe.baml"),
            r#"
//# first helper
function helper(x: int) -> int { x }

//# second helper
function helper(x: int) -> int { x + 1 }
"#,
        );

        assert_eq!(build_header_title(&db, "helper"), None);
    }

    #[test]
    fn test_early_return_renders_as_terminal_node() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/early.baml"),
            r#"
function Early(x: int) -> string {
//# Validate
if (x < 0) {
    //# bail out
    return "neg";
}
//# Continue
"ok"
}
"#,
        );

        let graph = build_graph(&db, "Early").expect("expected graph for Early");
        let ret_node = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::Return))
            .expect("early return should create a Return node");
        assert!(ret_node.label.starts_with("return"));

        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        let prepared_ret = prepared
            .nodes
            .get(&ret_node.id)
            .expect("return inside annotated branch should render");
        let outgoing = prepared
            .edges_by_src
            .get(&prepared_ret.id)
            .map(Vec::len)
            .unwrap_or(0);
        assert_eq!(
            outgoing, 0,
            "return node must not be connected to later nodes"
        );
    }

    #[test]
    fn test_return_call_is_not_expanded_under_return_node() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let (mut db, root) = test_db();
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/cfg-test/return-call.baml"),
            r#"
function Helper() -> string {
//# helper body
"ok"
}

function Early(x: int) -> string {
if (x < 0) {
    return Helper();
}

"later"
}
"#,
        );

        let graph = build_graph(&db, "Early").expect("expected graph for Early");
        let ret_node = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::Return))
            .expect("return call should create a Return node");

        assert_eq!(ret_node.label, "return Helper()");
        assert!(
            graph
                .edges_by_src
                .get(&ret_node.id)
                .is_none_or(Vec::is_empty),
            "return-call node must stay terminal instead of owning callee edges"
        );
        assert!(
            graph.nodes.values().all(|n| n.label != "helper body"),
            "callee body headers should not be expanded below a terminal return"
        );
    }

    #[test]
    fn ast_control_flow_graph_builds_new_style_test_bodies() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let (mut db, root) = test_db();
        let src = r#"
function Workflow(input: int) -> int {
  //# Choose result
  input + 1
}

test "renders workflow" {
  let result = Workflow(41)
  assert.equal(result, 42)
}
"#;
        db.add_or_update_file_in(root, std::path::Path::new("/cfg-test/tests.baml"), src);

        let workflow_graph = build_graph(&db, "Workflow").expect("workflow should have a graph");
        assert!(
            workflow_graph
                .nodes
                .values()
                .any(|node| node.node_type == NodeType::HeaderContextEnter),
            "fixture workflow must have control flow: {:#?}",
            workflow_graph.nodes
        );
        let graph =
            build_graph(&db, "root::renders workflow").expect("new-style test should have a graph");
        let root = graph
            .nodes
            .values()
            .find(|node| node.node_type == NodeType::FunctionRoot)
            .expect("test graph should have a root");
        let root_span = root
            .source_span
            .as_ref()
            .expect("test graph root should navigate to its declaration");
        let name_start = src.find("\"renders workflow\"").unwrap();
        assert_eq!(root_span.start_offset as usize, name_start);
        assert_eq!(
            root_span.end_offset as usize,
            name_start + "\"renders workflow\"".len()
        );
        assert!(
            graph
                .nodes
                .values()
                .any(|node| node.label.contains("Workflow")),
            "the test body's workflow call should be represented"
        );
        let prepared =
        baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(
            &graph,
        );
        assert!(
            prepared.nodes.values().any(|node| {
                node.node_type == NodeType::HeaderContextEnter && node.label == "Choose result"
            }),
            "the selected test should render the called workflow's control flow; raw={:#?}; prepared={:#?}",
            graph.nodes,
            prepared.nodes
        );
    }

    #[test]
    fn test_header_title_above() {
        assert_eq!(
            header_title_above("//# process stuff\n"),
            Some("process stuff".to_string())
        );
        assert_eq!(
            header_title_above("//# process stuff\n\n// note\n/// docs\n"),
            Some("process stuff".to_string()),
            "blank lines and comments between header and declaration are skipped"
        );
        assert_eq!(
            header_title_above("//## nested level\n"),
            Some("nested level".to_string())
        );
        assert_eq!(
            header_title_above("//# old header\n}\n"),
            None,
            "code between the header and the declaration stops the search"
        );
        assert_eq!(header_title_above("// just a comment\n"), None);
        assert_eq!(header_title_above(""), None);
    }

    #[test]
    fn test_playground_cursor_context_inside_llm_prefers_parent_function() {
        let (mut db, root) = test_db();
        let path = std::path::Path::new("/cfg-test/llm.baml");
        let source = r##"
function Summarize(input: string) -> string {
client: GPT4
prompt: `Summarize ${input}`
}
"##;
        db.add_or_update_file_in(root, path, source);

        for needle in ["client", "GPT4", "prompt", "Summarize ${input}"] {
            let offset = u32::try_from(source.find(needle).expect("needle exists")).unwrap();
            let ctx = build_cursor_context(&db, path.to_str().unwrap(), offset);

            assert_eq!(
                ctx.function_name.as_deref(),
                Some("Summarize"),
                "cursor on {needle:?} should select the top-level LLM function"
            );
        }
    }
}
