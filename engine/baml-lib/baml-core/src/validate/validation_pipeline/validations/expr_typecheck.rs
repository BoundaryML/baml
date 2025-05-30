use anyhow::Result;
use baml_types::expr::VarIndex;
use baml_types::TypeValue;
use std::collections::HashMap;
use std::sync::Arc;

use crate::ir::ir_helpers::item_type;
use crate::ir::IntermediateRepr;
use crate::ir::{repr::initial_context, IRHelper};
use crate::validate::validation_pipeline::context::Context;
use crate::Configuration;
use baml_types::{
    expr::{Expr, ExprMetadata},
    Arrow, BamlValueWithMeta, FieldType,
};
use internal_baml_diagnostics::{DatamodelError, Diagnostics, Span};

use crate::ir::IRHelperExtended;

/// Check the types of all expressions in the IR.
/// It relies on the types previously inferred and added to the expression metadata.
/// TODO: move this to a compiler pass, so that it transforms IR to IR.
/// TODO: Implement it directly in terms of the bidirectional typing algorithm.
pub fn typecheck_exprs(ctx: &mut Context<'_>) -> Result<()> {
    let null_configuration = Configuration::new();
    if let Ok(ir) = IntermediateRepr::from_parser_database(ctx.db, null_configuration) {
        let mut typing_context: HashMap<String, FieldType> = ir
            .expr_fns
            .iter()
            .map(|expr_fn| {
                (
                    expr_fn.elem.name.clone(),
                    FieldType::Arrow(Box::new(Arrow {
                        param_types: expr_fn.elem.inputs.iter().map(|(_, t)| t.clone()).collect(),
                        return_type: expr_fn.elem.output.clone(),
                    })),
                )
            })
            .chain(ir.functions.iter().map(|llm_function| {
                (
                    llm_function.elem.name.clone(),
                    FieldType::Arrow(Box::new(Arrow {
                        param_types: llm_function
                            .elem
                            .inputs
                            .iter()
                            .map(|(_, t)| t.clone())
                            .collect(),
                        return_type: llm_function.elem.output.clone(),
                    })),
                )
            }))
            .collect();

        for expr_fn in ir.expr_fns.iter() {
            let expr_fn_with_types = infer_types_in_context(
                &mut typing_context,
                Arc::new(
                    expr_fn
                        .elem
                        .clone()
                        .assign_param_types_to_body_variables()
                        .expr
                        .clone(),
                ),
            );
            typecheck_in_context(
                &ir,
                &mut ctx.diagnostics,
                &typing_context,
                &expr_fn_with_types,
            )?;
            // deeply_check_inference(&expr_fn_with_types)?;
        }

        for toplevel_assignment in ir.toplevel_assignments.iter() {
            typecheck_in_context(
                &ir,
                &mut ctx.diagnostics,
                &typing_context,
                &toplevel_assignment.elem.expr.elem,
            )?;
        }
    }
    Ok(())
}

/// A helper function for `typecheck_exprs`. It typechecks a given expression,
/// within a typing_context (Γ) of types for variables.
pub fn typecheck_in_context(
    ir: &IntermediateRepr,
    diagnostics: &mut Diagnostics,
    typing_context: &HashMap<String, FieldType>,
    expr: &Expr<ExprMetadata>,
) -> Result<()> {
    match expr {
        Expr::Atom(atom) => {
            // Atoms always typecheck.
            //  Ok(())
        }
        Expr::LLMFunction(llm_function, args, _) => {
            // Bare functions always typecheck.
            // Ok(())
        }
        Expr::FreeVar(var, (var_span, maybe_type)) => {
            if let Some(var_type) = maybe_type {
                if let Some(ctx_type) = typing_context.get(var) {
                    if !ir.is_subtype(&ctx_type, var_type) {
                        diagnostics.push_error(DatamodelError::new_validation_error(
                            "Type mismatch",
                            var_span.clone(),
                        ));
                    }
                }
            }
        }
        Expr::BoundVar(_, _) => {}
        Expr::Lambda(arity, body, (span, maybe_type)) => {
            // (\(x,y) -> x + y) : (Int,Int) -> Int
            if let Some(FieldType::Arrow(arrow)) = maybe_type {
                let mut inner_context = typing_context.clone();
                let fresh_names = body.fresh_names(*arity);
                let opened_body = fresh_names.iter().enumerate().fold(
                    body.clone(),
                    |body, (index, fresh_name)| {
                        let target = VarIndex {
                            de_bruijn: 0,
                            tuple: index as u32,
                        };
                        Arc::new(body.open(&target, fresh_name))
                    },
                );
                for ((ind, param_name), param_type) in
                    fresh_names.iter().enumerate().zip(arrow.param_types.iter())
                {
                    inner_context.insert(param_name.to_string(), param_type.clone());
                }
                if !compatible_as_subtype(ir, &body.meta().1, &Some(arrow.return_type.clone())) {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        &format!(
                            "Type mismatch in lambda: {} vs {}",
                            body.meta()
                                .1
                                .as_ref()
                                .map_or("?".to_string(), |t| t.to_string()),
                            arrow.return_type.to_string()
                        ),
                        body.meta().0.clone(),
                    ));
                }
                typecheck_in_context(ir, diagnostics, &inner_context, &opened_body)?;
            }
        }
        // (\[x,y] -> x + y) (1,2)
        // ([Int,Int] -> Int) ([Int,Int]
        Expr::App(f, xs, (span, maybe_app_type)) => {
            // First check that the param types are compatible with the arguments.
            match (&f.meta().1, xs.as_ref()) {
                (Some(FieldType::Arrow(arrow)), Expr::ArgsTuple(args, _)) => {
                    for (param_type, arg) in arrow.param_types.iter().zip(args.iter()) {
                        if !compatible_as_subtype(ir, &arg.meta().1, &Some(param_type.clone())) {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "Type mismatch in app",
                                span.clone(),
                            ));
                        }
                    }
                }
                x => {
                    eprintln!(
                        "TYPECHECKING APP: UNEXPECTED ARGS: ({}: {:?} ) {:?}",
                        f.dump_str(),
                        f.meta()
                            .1
                            .as_ref()
                            .map_or("?".to_string(), |t| t.to_string()),
                        x
                    );
                }
            }
        }
        Expr::Let(binder, value, body, meta) => {
            typecheck_in_context(ir, diagnostics, typing_context, value)?;
            let mut body_context = typing_context.clone();
            if let Some(value_type) = value.meta().1.clone() {
                body_context.insert(binder.to_string(), value_type);
            }
            typecheck_in_context(ir, diagnostics, &body_context, body)?;
        }
        Expr::ArgsTuple(args, _) => {}
        Expr::List(items, meta) => {
            for item in items.iter() {
                if let Some(item_type) = item.meta().1.as_ref() {
                    let item_list_type = FieldType::List(Box::new(item_type.clone()));
                    if !compatible_as_subtype(ir, &Some(item_list_type), &meta.1.clone()) {
                        diagnostics.push_error(DatamodelError::new_validation_error(
                            "Type mismatch in list",
                            meta.0.clone(),
                        ));
                    }
                }
                typecheck_in_context(ir, diagnostics, typing_context, item)?;
            }
        }
        Expr::Map(items, meta) => {
            if let Some(map_type) = meta.1.as_ref() {
                if let Some((key_type, item_type)) = match map_type {
                    FieldType::Map(key_type, item_type) => Some((key_type, item_type)),
                    _ => None,
                } {
                    for (_key, item) in items.iter() {
                        if let Some(item_type) = item.meta().1.as_ref() {
                            let item_map_type =
                                FieldType::Map(key_type.clone(), Box::new(item_type.clone()));
                            if !compatible_as_subtype(ir, &Some(item_map_type), &meta.1.clone()) {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    "Type mismatch in map",
                                    meta.0.clone(),
                                ));
                            }
                        }
                        typecheck_in_context(ir, diagnostics, typing_context, item)?;
                    }
                } else {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "Type mismatch in map",
                        meta.0.clone(),
                    ));
                }
            }
        }
        Expr::ClassConstructor {
            name,
            fields,
            spread,
            meta,
        } => {
            if let Ok(class_walker) = ir.find_class(name) {
                for (field_name, field_value) in fields.iter() {
                    let maybe_field_type = field_value.meta().1.clone();
                    if let Some(field_type) = maybe_field_type {
                        if let Some(field_walker) = class_walker.find_field(field_name) {
                            if !compatible_as_subtype(
                                ir,
                                &Some(field_walker.r#type().clone()),
                                &Some(field_type),
                            ) {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    "Type mismatch in class constructor",
                                    meta.0.clone(),
                                ));
                            }
                            typecheck_in_context(ir, diagnostics, typing_context, field_value)?;
                        }
                    }
                }
            }

            let spread_type = spread.as_ref().and_then(|s| s.meta().1.clone());
            if !compatible_as_subtype(ir, &meta.1, &spread_type) {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "Type mismatch in class constructor",
                    meta.0.clone(),
                ));
            }
        }
        Expr::If(cond, then, else_, meta) => {
            if !compatible_as_subtype(
                ir,
                &cond.meta().1,
                &Some(FieldType::Primitive(TypeValue::Bool)),
            ) {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "Type mismatch in if",
                    meta.0.clone(),
                ));
            }
            // TODO: Check that then and else have the same type? Or, if they're compatible,
            // who should be a subtype of who?
        }
        Expr::ForLoop {
            item,
            iterable,
            body,
            meta,
        } => {
            let iterable_type_ok: bool = match &iterable.meta().1 {
                Some(FieldType::List(_)) => true,
                _ => false, // TODO: Aliases.
            };
            if !iterable_type_ok {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "For loop must iterate over a list",
                    iterable.meta().0.clone(),
                ));
            }
        }
    };

    // Finally, assert that we know the type of the whole expression.
    if expr.meta().1.is_none() {
        return Err(anyhow::anyhow!(
            "type inference failed for expression: {}",
            expr.dump_str()
        ));
    }
    Ok(())
}

fn compatible_as_subtype(
    ir: &IntermediateRepr,
    a: &Option<FieldType>,
    b: &Option<FieldType>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => ir.is_subtype(a, b),
        _ => true,
    }
}

/// Extends a typing context while examining an expression, also returns
/// the expression with modified metadata.
pub fn infer_types_in_context(
    typing_context: &mut HashMap<String, FieldType>,
    expr: Arc<Expr<ExprMetadata>>,
) -> Arc<Expr<ExprMetadata>> {
    match expr.as_ref() {
        Expr::FreeVar(ref var_name, (span, maybe_type)) => {
            // Assign variables from the context.
            if let Some(ctx_ty) = typing_context.get(var_name) {
                Arc::new(Expr::FreeVar(
                    var_name.clone(),
                    (span.clone(), Some(ctx_ty.clone())),
                ))
            } else {
                // Otherwise, and if we know the type, add it to the context.
                if let Some(var_ty) = &expr.meta().1 {
                    typing_context.insert(var_name.to_string(), var_ty.clone());
                }
                expr.clone()
            }
        }
        Expr::Atom(_) => {
            // All atoms are typed during parsing, so we ignore them.
            expr.clone()
        }
        Expr::Let(ref var_name, expr, ref body, _) => {
            let new_expr = infer_types_in_context(typing_context, expr.clone());
            let new_body = infer_types_in_context(typing_context, body.clone());
            if let Some(ref expr_ty) = new_expr.meta().1 {
                typing_context.insert(var_name.to_string(), expr_ty.clone());
            }
            let new_meta = (expr.meta().0.clone(), new_body.meta().1.clone());
            Arc::new(Expr::Let(var_name.clone(), new_expr, new_body, new_meta))
        }
        Expr::App(f, args, (span, maybe_app_type)) => {
            // Infer the type of an App from the return type of the function, if
            // it is a function with a known return type.
            let new_f = infer_types_in_context(typing_context, f.clone());
            let new_args = infer_types_in_context(typing_context, args.clone());
            let new_app_type = match &new_f.meta().1 {
                Some(FieldType::Arrow(arrow)) => Some(arrow.return_type.clone()),
                ty => None,
            }
            .or(maybe_app_type.clone());
            let new_meta = (span.clone(), new_app_type);
            Arc::new(Expr::App(new_f, new_args, new_meta))
        }
        Expr::ArgsTuple(ref args, _) => {
            let new_args = args
                .iter()
                .map(|arg| {
                    Arc::unwrap_or_clone(infer_types_in_context(
                        typing_context,
                        Arc::new(arg.clone()),
                    ))
                })
                .collect();
            Arc::new(Expr::ArgsTuple(
                new_args,
                (expr.meta().0.clone(), expr.meta().1.clone()),
            ))
        }
        Expr::Lambda(arity, body, (span, maybe_type)) => {
            let fresh_names = body.fresh_names(*arity);
            let mut local_typing_context = typing_context.clone();
            let opened_body =
                fresh_names
                    .iter()
                    .enumerate()
                    .fold(body.clone(), |body, (index, fresh_name)| {
                        let target = VarIndex {
                            de_bruijn: 0,
                            tuple: index as u32,
                        };
                        Arc::new(body.open(&target, fresh_name))
                    });
            if let Some(FieldType::Arrow(arrow)) = maybe_type {
                for (param_type, param_name) in arrow.param_types.iter().zip(fresh_names.iter()) {
                    local_typing_context.insert(param_name.to_string(), param_type.clone());
                }
            }
            let body_with_inferred_types =
                infer_types_in_context(&mut local_typing_context, opened_body.clone());
            let new_body = fresh_names.iter().enumerate().fold(
                body_with_inferred_types.clone(),
                |body, (index, fresh_name)| {
                    let target = VarIndex {
                        de_bruijn: 0,
                        tuple: index as u32,
                    };
                    Arc::new(body.close(&target, fresh_name))
                },
            );
            Arc::new(Expr::Lambda(
                *arity,
                new_body,
                (span.clone(), maybe_type.clone()),
            ))
        }
        Expr::List(items, (span, maybe_type)) => {
            let new_items = items
                .iter()
                .map(|item| {
                    Arc::unwrap_or_clone(infer_types_in_context(
                        typing_context,
                        Arc::new(item.clone()),
                    ))
                })
                .collect();
            Arc::new(Expr::List(new_items, (span.clone(), maybe_type.clone())))
        }
        Expr::Map(items, (span, maybe_type)) => {
            let new_items = items
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        Arc::unwrap_or_clone(infer_types_in_context(
                            typing_context,
                            Arc::new(value.clone()),
                        )),
                    )
                })
                .collect();
            Arc::new(Expr::Map(new_items, (span.clone(), maybe_type.clone())))
        }
        Expr::ClassConstructor {
            name,
            fields,
            spread,
            meta,
        } => {
            let new_fields = fields
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        Arc::unwrap_or_clone(infer_types_in_context(
                            typing_context,
                            Arc::new(value.clone()),
                        )),
                    )
                })
                .collect();
            let new_spread = spread.as_ref().map(|s| {
                Box::new(Arc::unwrap_or_clone(infer_types_in_context(
                    typing_context,
                    Arc::new(s.as_ref().clone()),
                )))
            });
            Arc::new(Expr::ClassConstructor {
                name: name.clone(),
                fields: new_fields,
                spread: new_spread,
                meta: meta.clone(),
            })
        }
        Expr::LLMFunction(llm_function, args, (span, maybe_type)) => expr.clone(),
        Expr::BoundVar(_, _) => expr.clone(),
        Expr::If(cond, then, else_, meta) => {
            // TODO: Infer the type of the whole expression from new_then?
            let new_cond = infer_types_in_context(typing_context, cond.clone());
            let new_then = infer_types_in_context(typing_context, then.clone());
            let new_else = else_
                .as_ref()
                .map(|e| infer_types_in_context(typing_context, e.clone()));
            let mut new_meta = meta.clone();
            if new_meta.1.is_none() {
                new_meta.1 = new_then.meta().1.clone();
            }
            Arc::new(Expr::If(new_cond, new_then, new_else, new_meta))
        }
        Expr::ForLoop {
            item,
            iterable,
            body,
            meta,
        } => {
            let mut body_context = typing_context.clone();
            // TODO: Handle aliases. To do this, we will need access to the IR.
            // We can't have access to the IR until we introduce compiler passes,
            // otherwise there is a borrowing issue. (To see why, try taking an immutable
            // reference to `repr` in `from_parser_database`).
            let item_ty = iterable.meta().1.as_ref().and_then(|t| match t {
                FieldType::List(inner) => Some(inner),
                _ => None,
            });
            if let Some(item_ty) = item_ty {
                body_context.insert(item.to_string(), *item_ty.clone());
            }
            let new_iterable = infer_types_in_context(typing_context, iterable.clone());
            let new_body = infer_types_in_context(typing_context, body.clone());
            let mut new_meta = meta.clone();
            new_meta.1 = new_body
                .meta()
                .1
                .as_ref()
                .map(|body_type| FieldType::List(Box::new(body_type.clone())));
            Arc::new(Expr::ForLoop {
                item: item.clone(),
                iterable: iterable.clone(),
                body: new_body,
                meta: meta.clone(),
            })
        }
    }
}

fn deeply_check_inference(expr: &Expr<ExprMetadata>) -> Result<()> {
    let mut untyped_subexprs = Vec::new();
    for subexpr in expr.into_iter() {
        if subexpr.meta().1.is_none() {
            untyped_subexprs.push(subexpr);
        }
    }
    if untyped_subexprs.is_empty() {
        Ok(())
    } else {
        let error_message = untyped_subexprs
            .iter()
            .map(|e| e.dump_str())
            .collect::<Vec<_>>()
            .join(",\n");
        Err(anyhow::anyhow!(
            "type inference failed for expressions:\n{}",
            error_message
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::repr::make_test_ir_and_diagnostics;

    #[test]
    fn null_case() {
        let (ir, diagnostics) = make_test_ir_and_diagnostics(
            r##"
        fn First(x: int, y: int) -> int {
          x
        }
        "##,
        )
        .expect("Valid source");
        assert!(!diagnostics.has_errors());
    }

    #[test]
    fn param_body_mismatch() {
        let (ir, diagnostics) = make_test_ir_and_diagnostics(
            r##"
          fn First(x: int, y: int) -> string {
            x
          }
        "##,
        )
        .expect("Valid source");
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn application_mismatch() {
        let (ir, diagnostics) = make_test_ir_and_diagnostics(
            r##"
        fn First(x: int, y: int) -> int {
          Inner(x)
        }

        fn Inner(x: string) -> int {
          5
        }
        "##,
        )
        .expect("Valid source");
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn multiple_calls() {
        let (ir, diagnostics) = make_test_ir_and_diagnostics(
            r##"
        fn Compare(x: string, y: string) -> int {
          1
        }

        fn MkPoem1(x: string) -> string {
          "Pretty"
        }

        fn MkPoem2(x: string) -> string {
          "Poem"
        }

        fn Go(x: string) -> int {
          let poem1 = MkPoem1(x);
          let poem2 = MkPoem2(x);
          Compare(poem1, poem2)
        }
        "##,
        )
        .expect("Valid source");
        dbg!(&diagnostics);
        assert!(!diagnostics.has_errors());
    }
}
