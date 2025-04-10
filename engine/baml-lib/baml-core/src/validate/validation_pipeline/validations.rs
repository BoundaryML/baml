mod classes;
mod clients;
mod configurations;
mod cycle;
mod enums;
mod functions;
mod template_strings;
mod tests;
mod types;

use baml_types::GeneratorOutputType;

use crate::{configuration::Generator, validate::generator_loader::load_generators_from_ast};

use super::context::Context;

use std::collections::HashSet;

pub(super) fn validate(ctx: &mut Context<'_>) {
    enums::validate(ctx);
    classes::validate(ctx);
    functions::validate(ctx);
    clients::validate(ctx);
    template_strings::validate(ctx);
    configurations::validate(ctx);
    tests::validate(ctx);

    let generators = load_generators_from_ast(ctx.db.ast(), ctx.diagnostics);
    let codegen_targets: HashSet<GeneratorOutputType> = generators
        .into_iter()
        .filter_map(|generator| match generator {
            Generator::Codegen(gen) => Some(gen.output_type),
            Generator::BoundaryCloud(_) => None,
        })
        .collect::<HashSet<_>>();
    classes::assert_no_field_name_collisions(ctx, &codegen_targets);

    if !ctx.diagnostics.has_errors() {
        cycle::validate(ctx);
    }
}
