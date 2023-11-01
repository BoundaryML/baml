use std::collections::HashSet;

use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{ClassId, WithName};

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    // Validates if there's a cycle in any dependency graph.
    let mut deps_list = ctx
        .db
        .walk_classes()
        .map(|f| {
            (
                f.id,
                f.dependencies()
                    .into_iter()
                    .filter(|f| match ctx.db.find_type_by_str(f) {
                        Some(either::Either::Left(_cls)) => true,
                        // Don't worry about enum dependencies, they can't form cycles.
                        Some(either::Either::Right(_enm)) => false,
                        None => panic!("Unknown class `{}`", f),
                    })
                    .collect::<HashSet<_>>(),
            )
        })
        .collect::<Vec<_>>();

    // Now we can check for cycles using topological sort.
    let mut stack: Vec<(ClassId, Vec<ClassId>)> = Vec::new(); // This stack now also keeps track of the path
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    // Find all items with 0 dependencies
    for (id, deps) in &deps_list {
        if deps.is_empty() {
            stack.push((*id, vec![*id]));
        }
    }

    while let Some((current, path)) = stack.pop() {
        let name = ctx.db.ast()[current].name().to_string();
        let span = ctx.db.ast()[current].span();

        if in_stack.contains(&current) {
            // If current is in in_stack, then we're revisiting a node, which means there's a cycle
            let cycle_start_index = path.iter().position(|&x| x == current).unwrap();
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

        deps_list.iter_mut().for_each(|(id, deps)| {
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
    if visited.len() != deps_list.len() {
        for (id, _) in &deps_list {
            if !visited.contains(id) {
                let cls = &ctx.db.ast()[*id];
                ctx.push_error(DatamodelError::new_validation_error(
                    &format!("These classes form a dependency cycle: {}", cls.name()),
                    cls.span().clone(),
                ));
            }
        }
    }
}
