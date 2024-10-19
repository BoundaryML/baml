use std::collections::HashSet;

use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{FieldType, TypeExpId, WithIdentifier, WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

/// Validates if there's a cycle in any dependency graph.
pub(super) fn validate(ctx: &mut Context<'_>) {
    // We're only going to consider type dependencies that can actually cause
    // infinite recursion. Unions and optionals can stop the recursion at any
    // point, so they don't have to be part of the "dependency" graph because
    // technically an optional field doesn't "depend" on anything, it can just
    // be null.
    let mut required_deps = ctx
        .db
        .walk_classes()
        .map(|cls| {
            let expr_block = &ctx.db.ast()[cls.class_id()];

            // TODO: There's already a hash set that returns "dependencies" in
            // the DB, it shoudn't be necessary to traverse all the fields here
            // again, we need to refactor .dependencies() or add a new method
            // that returns not only the dependency name but also field arity.
            // The arity could be computed at the same time as the dependencies
            // hash set. Code is here:
            //
            // baml-lib/parser-database/src/types/mod.rs
            // fn visit_class()
            let mut deps = HashSet::new();

            for field in &expr_block.fields {
                if let Some(field_type) = &field.expr {
                    insert_deps(field_type, ctx, &mut deps);
                }
            }

            (cls.id, deps)
        })
        .collect::<Vec<_>>();

    // println!("{:?}", required_deps);

    // Now we can check for cycles using topological sort.
    let mut stack: Vec<(TypeExpId, Vec<TypeExpId>)> = Vec::new(); // This stack now also keeps track of the path
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    // Find all items with 0 dependencies
    for (id, deps) in &required_deps {
        if deps.is_empty() {
            stack.push((*id, vec![*id]));
        }
    }

    while let Some((current, path)) = stack.pop() {
        let name = ctx.db.ast()[current].name().to_string();
        let span = ctx.db.ast()[current].span();

        if in_stack.contains(&current) {
            let cycle_start_index = match path.iter().position(|&x| x == current) {
                Some(index) => index,
                None => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Cycle start index not found in the path.",
                        span.clone(),
                    ));
                    return;
                }
            };
            let cycle = path[cycle_start_index..]
                .iter()
                .map(|&x| ctx.db.ast()[x].name())
                .collect::<Vec<_>>()
                .join(" -> ");
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("These classes form a dependency cycle: {}", cycle),
                span.clone(),
            ));
            return;
        }

        in_stack.insert(current);
        visited.insert(current);

        required_deps.iter_mut().for_each(|(id, deps)| {
            if deps.remove(&name) {
                // If this item has now 0 dependencies, add it to the stack
                if deps.is_empty() {
                    let mut new_path = path.clone();
                    new_path.push(*id);
                    stack.push((*id, new_path));
                }
            }
        });

        in_stack.remove(&current);
    }

    // If there are still items left in deps_list after the above steps, there's a cycle
    if visited.len() != required_deps.len() {
        for (id, _) in &required_deps {
            if !visited.contains(id) {
                let cls = &ctx.db.ast()[*id];
                ctx.push_error(DatamodelError::new_validation_error(
                    &format!("These classes form a dependency cycle: {}", cls.name()),
                    cls.identifier().span().clone(),
                ));
            }
        }
    }
}

/// Inserts all the required dependencies of a field into the given set.
///
/// Recursively deals with unions of unions. Can be implemented iteratively with
/// a while loop and a stack/queue if this ends up being slow / inefficient.
fn insert_deps(field: &FieldType, ctx: &Context<'_>, deps: &mut HashSet<String>) {
    match field {
        FieldType::Symbol(arity, ident, _) if arity.is_required() => {
            let name = ident.name();
            if let Some(Either::Left(_cls_dep)) = ctx.db.find_type_by_str(&name) {
                deps.insert(name.to_string());
            }
        }

        FieldType::Union(arity, field_types, _, _) if arity.is_required() => {
            for f in field_types {
                insert_deps(f, ctx, deps);
            }
        }

        _ => {}
    }
}
