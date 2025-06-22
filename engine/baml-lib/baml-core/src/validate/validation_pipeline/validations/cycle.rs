use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    ops::Index,
};

use internal_baml_ast::ast::{self, Ast, FieldType, TypeAliasId, TypeExpId, WithName, WithSpan};
use internal_baml_diagnostics::DatamodelError;
use internal_baml_parser_database::{Tarjan, TypeWalker};

use crate::validate::validation_pipeline::context::Context;

/// Validates if the dependency graph contains one or more infinite cycles.
pub(super) fn validate(ctx: &mut Context<'_>) {
    // We'll check type alias cycles first. Just like Typescript, cycles are
    // allowed only for maps and lists. We'll call such cycles "structural
    // recursion". Anything else like nulls or unions won't terminate a cycle.
    let non_structural_type_aliases = HashMap::from_iter(ctx.db.walk_type_aliases().map(|alias| {
        let mut dependencies = HashSet::new();
        insert_required_alias_deps(alias.target(), ctx, &mut dependencies);

        (alias.id, dependencies)
    }));

    // Based on the graph we've built with does not include the edges created
    // by maps and lists, check the cycles and report them.
    report_infinite_cycles(
        &non_structural_type_aliases,
        ctx,
        "These aliases form a dependency cycle",
    );

    // In order to avoid infinite recursion when resolving types for class
    // dependencies below, we'll compute the cycles of aliases including maps
    // and lists so that the recursion can be stopped before entering a cycle.
    let complete_alias_cycles = ctx
        .db
        .recursive_alias_cycles()
        .iter()
        .flatten()
        .copied()
        .collect();

    // Now build a graph of all the "required" dependencies represented as an
    // adjacency list. We're only going to consider type dependencies that can
    // actually cause infinite recursion. Unions and optionals can stop the
    // recursion at any point, so they don't have to be part of the "dependency"
    // graph because technically an optional field doesn't "depend" on anything,
    // it can just be null.
    let class_dependency_graph = HashMap::from_iter(ctx.db.walk_classes().map(|class| {
        let expr_block = &ctx.db.ast()[class.id];

        // TODO: There's already a hash set that returns "dependencies" in
        // the DB, it shoudn't be necessary to traverse all the fields here
        // again and build yet another graph, we need to refactor
        // .dependencies() or add a new method that returns not only the
        // dependency name but also field arity. The arity could be computed at
        // the same time as the dependencies hash set. Code is here:
        //
        // baml-lib/parser-database/src/types/mod.rs
        // fn visit_class()
        let mut dependencies = HashSet::new();

        for field in &expr_block.fields {
            if let Some(field_type) = &field.expr {
                insert_required_class_deps(
                    class.id,
                    field_type,
                    ctx,
                    &mut dependencies,
                    &complete_alias_cycles,
                );
            }
        }

        (class.id, dependencies)
    }));

    report_infinite_cycles(
        &class_dependency_graph,
        ctx,
        "These classes form a dependency cycle",
    );

    let client_graph = HashMap::<_, _>::from_iter(ctx.db.walk_clients().map(|client| {
        let mut dependencies = HashSet::new();

        if let internal_llm_client::UnresolvedClientProperty::Fallback(options) =
            &client.properties().options
        {
            use internal_llm_client::StrategyClientProperty;

            let valid_clients = ctx.db.valid_client_names();

            for (client, span) in options.strategy() {
                if let either::Either::Right(internal_llm_client::ClientSpec::Named(s)) = client {
                    if valid_clients.contains(s) {
                        dependencies.insert(ctx.db.find_client(s).unwrap().id);
                    }
                }
            }
        }

        (client.id, dependencies)
    }));

    report_infinite_cycles(
        &client_graph,
        ctx,
        "These fallback clients form a dependency cycle",
    );
}

/// Finds and reports all the infinite cycles in the given graph.
///
/// It prints errors like this:
///
/// "Error validating: These classes form a dependency cycle: A -> B -> C"
fn report_infinite_cycles<V: Ord + Eq + Hash + Copy>(
    graph: &HashMap<V, HashSet<V>>,
    ctx: &mut Context<'_>,
    message: &str,
) -> Vec<Vec<V>>
where
    Ast: Index<V>,
    <Ast as Index<V>>::Output: WithName,
    <Ast as Index<V>>::Output: WithSpan,
{
    let components = Tarjan::components(graph);

    for component in &components {
        let cycle = component
            .iter()
            .map(|id| {
                // TODO: #1343 Temporary solution until we implement scoping in the AST.
                let name = ctx.db.ast()[*id].name().to_string();
                if name.starts_with(ast::DYNAMIC_TYPE_NAME_PREFIX) {
                    name.strip_prefix(ast::DYNAMIC_TYPE_NAME_PREFIX)
                        .map(ToOwned::to_owned)
                        .unwrap()
                } else {
                    name
                }
            })
            .collect::<Vec<_>>()
            .join(" -> ");

        // TODO: We can push an error for every sinlge class here (that's what
        // Rust does), for now it's an error for every cycle found.
        ctx.push_error(DatamodelError::new_validation_error(
            &format!("{message}: {cycle}"),
            ctx.db.ast()[component[0]].span().clone(),
        ));
    }

    components
}

/// Inserts all the required dependencies of a field into the given set.
///
/// Recursively deals with unions of unions. Can be implemented iteratively with
/// a while loop and a stack/queue if this ends up being slow / inefficient or
/// it reaches stack overflows with large inputs.
///
/// TODO: Use a struct to keep all this state. Too many parameters already.
fn insert_required_class_deps(
    id: TypeExpId,
    field: &FieldType,
    ctx: &Context<'_>,
    deps: &mut HashSet<TypeExpId>,
    alias_cycles: &HashSet<TypeAliasId>,
) {
    match field {
        FieldType::Symbol(arity, ident, _) if arity.is_required() => {
            match ctx.db.find_type_by_str(ident.name()) {
                Some(TypeWalker::Class(class)) => {
                    deps.insert(class.id);

                    // TODO: #1343 Temporary solution until we implement scoping in the AST.
                    if !class.name().starts_with(ast::DYNAMIC_TYPE_NAME_PREFIX) {
                        let dyn_def_name =
                            format!("{}{}", ast::DYNAMIC_TYPE_NAME_PREFIX, class.name());
                        if let Some(TypeWalker::Class(dyn_def)) =
                            ctx.db.find_type_by_str(&dyn_def_name)
                        {
                            deps.insert(dyn_def.id);
                        }
                    }
                }
                Some(TypeWalker::TypeAlias(alias)) => {
                    // This code runs after aliases are already resolved.
                    if !alias_cycles.contains(&alias.id) {
                        insert_required_class_deps(id, alias.resolved(), ctx, deps, alias_cycles)
                    }
                }
                _ => {}
            }
        }

        FieldType::Union(arity, field_types, _, _) if arity.is_required() => {
            // All the dependencies of the union.
            let mut union_deps = HashSet::new();

            // All the dependencies of a single field in the union. This is
            // reused on every iteration of the loop below to avoid allocating
            // a new hash set every time.
            let mut nested_deps = HashSet::new();

            for f in field_types {
                insert_required_class_deps(id, f, ctx, &mut nested_deps, alias_cycles);

                // No nested deps found on this component, this makes the
                // union finite, so no need to go deeper.
                if nested_deps.is_empty() {
                    return;
                }

                // Add the nested deps to the overall union deps and clear the
                // iteration hash set.
                union_deps.extend(nested_deps.drain());
            }

            // A union does not depend on itself if the field can take other
            // values. However, if it only depends on itself, it means we have
            // something like this:
            //
            // class Example {
            //    field: Example | Example | Example
            // }
            if union_deps.len() > 1 {
                union_deps.remove(&id);
            }

            deps.extend(union_deps);
        }

        _ => {}
    }
}

/// Implemented a la TS, maps and lists are not included as edges.
fn insert_required_alias_deps(
    field_type: &FieldType,
    ctx: &Context<'_>,
    required: &mut HashSet<TypeAliasId>,
) {
    match field_type {
        FieldType::Symbol(_, ident, _) => {
            if let Some(TypeWalker::TypeAlias(alias)) = ctx.db.find_type_by_str(ident.name()) {
                required.insert(alias.id);
            }
        }

        FieldType::Union(_, field_types, ..) | FieldType::Tuple(_, field_types, ..) => {
            for f in field_types {
                insert_required_alias_deps(f, ctx, required);
            }
        }

        _ => {}
    }
}
