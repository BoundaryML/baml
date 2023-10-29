use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{FieldType, Identifier, WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

fn errors_with_names<'a>(ctx: &'a mut Context<'_>, idn: &Identifier) {
    let name = idn.name();
    let names = ctx.db.valid_type_names();

    // Calculate OSA distances and sort names by distance
    let mut distances: Vec<(usize, &str)> = names
        .iter()
        .map(|n| (strsim::osa_distance(n, name), *n))
        .collect();
    distances.sort_by_key(|k| k.0);

    // Set a threshold for "closeness"
    let threshold = 2; // for example, you can adjust this based on your needs

    // Filter names that are within the threshold
    let close_names: Vec<&str> = distances
        .iter()
        .filter(|&&(dist, _)| dist <= threshold)
        .map(|&(_, name)| name)
        .collect();

    // Push the error with the appropriate message
    ctx.push_error(DatamodelError::new_type_not_found_error(
        name,
        close_names,
        idn.span().clone(),
    ));
}

pub(crate) fn validate_type_exists(ctx: &mut Context<'_>, field_type: &FieldType) {
    field_type
        .flat_idns()
        .iter()
        .for_each(|f| match ctx.db.find_type(f) {
            Some(_) => {}
            None => match f {
                Identifier::Primitive(..) => {}
                _ => errors_with_names(ctx, f),
            },
        });
}
