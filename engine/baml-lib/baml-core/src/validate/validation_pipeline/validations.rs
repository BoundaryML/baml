mod classes;
mod clients;
mod configurations;
mod cycle;
mod enums;
mod expr_fns;
mod functions;
pub mod identifiers;
mod reserved_names;
mod template_strings;
mod tests;
mod types;

use std::collections::HashSet;

use anyhow::Result;
use baml_compiler::{hir::Hir, thir::typecheck::typecheck, watch::WatchChannels};
use baml_types::GeneratorOutputType;

use super::context::Context;
use crate::{configuration::Generator, validate::generator_loader::load_generators_from_ast};

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
    enums::assert_no_enum_value_collisions(ctx, &codegen_targets);

    expr_fns::validate_expr_fns(ctx);

    // Use HIR-based typechecking from baml-compiler
    let _ = hir_typecheck_exprs(ctx);

    if !ctx.diagnostics.has_errors() {
        cycle::validate(ctx);
    }
}

/// HIR-based typechecking using functions from baml-compiler
fn hir_typecheck_exprs(ctx: &mut Context<'_>) -> Result<()> {
    // Create HIR from AST using baml-compiler
    let hir = Hir::from_ast(ctx.db.ast());

    // Run HIR-based typechecking using baml-compiler
    let thir = typecheck(&hir, ctx.diagnostics);

    let _ = WatchChannels::analyze_program(&thir, ctx.diagnostics);

    Ok(())
}
