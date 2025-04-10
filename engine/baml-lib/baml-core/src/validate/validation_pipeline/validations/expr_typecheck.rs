use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

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
        }
    }
    Ok(())
}

pub fn typecheck_in_context(
    ir: &IntermediateRepr,
    diagnostics: &mut Diagnostics,
    typing_context: &HashMap<String, FieldType>,
    expr: &Expr<ExprMetadata>,
) -> Result<()> {
    match expr {
        Expr::Atom(atom) => {
            // Atoms always typecheck.
            Ok(())
        }
        Expr::LLMFunction(llm_function, args, _) => {
            // Bare functions always typecheck.
            Ok(())
        }
        Expr::Var(var, (var_span, maybe_type)) => {
            if let Some(var_type) = maybe_type {
                if let Some(ctx_type) = typing_context.get(var) {
                    if ir.is_subtype(&ctx_type, var_type) {
                        Ok(())
                    } else {
                        diagnostics.push_error(DatamodelError::new_validation_error(
                            "Type mismatch",
                            var_span.clone(),
                        ));
                        Ok(())
                    }
                } else {
                    Ok(())
                }
            } else {
                Ok(())
            }
        }
        Expr::Lambda(param_names, body, (span, maybe_type)) => {
            // (\(x,y) -> x + y) : (Int,Int) -> Int
            if let Some(FieldType::Arrow(arrow)) = maybe_type {
                let mut inner_context = typing_context.clone();
                for (param_type, param_name) in arrow.param_types.iter().zip(param_names.iter()) {
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
                typecheck_in_context(ir, diagnostics, &inner_context, body)?;
            }
            Ok(())
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
                    eprintln!("TYPECHECKING APP: UNEXPECTED ARGS: {:?}", x);
                }
            }
            Ok(())

            // TODO: What was this? Bring it back?
            // match (f.as_ref(), xs.as_ref(), maybe_app_type) {
            //     (
            //         _, // Expr::Lambda(params, body, (lambda_span, _)),
            //         Expr::ArgsTuple(args, (args_span, args_type)),
            //         _,
            //     ) => {
            //         // First, check that the arguments are the right type
            //         // for the lambda.
            //         let maybe_lambda_type= &f.meta().1;
            //         eprintln!("LAMBDA_TYPE: {:?}", maybe_lambda_type);
            //         if let Some(lambda_type) = maybe_lambda_type {
            //             eprintln!("checking lambda_type: {:?}", lambda_type);
            //             match lambda_type {
            //                 ExprType::Arrow(arrow) => {
            //                     if let Some(app_type) = maybe_app_type {

            //                         if !compatible_as_subtype(
            //                             ir,
            //                             &maybe_app_type,
            //                             &Some(arrow.body_type.clone()),
            //                         ) {
            //                             eprintln!(
            //                                 "C Type mismatch in app: {} vs {}",
            //                                 app_type.dump_str(),
            //                                 arrow.body_type.dump_str()
            //                             );
            //                             diagnostics.push_error(DatamodelError::new_validation_error(
            //                                 &format!(
            //                                     "D Type mismatch in app: {} vs {}",
            //                                     app_type.dump_str(),
            //                                     arrow.body_type.dump_str()
            //                                 ),
            //                                 span.clone(),
            //                             ));
            //                         }
            //                     }
            //                     for (param_type, arg) in arrow.param_types.iter().zip(args.iter()) {
            //                         eprintln!("TYPECHECKING APP COMPARING PARAMTYPE: {:?} vs ARG: {:?}", param_type, arg);
            //                         if !compatible_as_subtype(
            //                             ir,
            //                             &arg.meta().1,
            //                             &Some(param_type.clone()),
            //                         ) {
            //                             eprintln!(
            //                                 "E Type mismatch in app: {} vs {}",
            //                                 arg.meta()
            //                                     .1
            //                                     .as_ref()
            //                                     .map_or("?".to_string(), |t| t.dump_str()),
            //                                 param_type.dump_str()
            //                             );
            //                             diagnostics.push_error(
            //                                 DatamodelError::new_validation_error(
            //                                     &format!(
            //                                         "F Type mismatch in app: {} vs {}",
            //                                         arg.meta()
            //                                             .1
            //                                             .as_ref()
            //                                             .map_or("?".to_string(), |t| t.dump_str()),
            //                                         param_type.dump_str()
            //                                     ),
            //                                     span.clone(),
            //                                 ),
            //                             );
            //                         }
            //                     }
            //                 }
            //                 ExprType::Atom(_) => {
            //                     diagnostics.push_error(DatamodelError::new_validation_error(
            //                         "Expected a function type",
            //                         span.clone(),
            //                     ));
            //                 }
            //             }
            //         }

            //         typecheck_in_context(ir, diagnostics, &inner_context, body)?;

            //         Ok(())
            //     }
            //     _ => Ok(()),
            // }
            // Applications typecheck if the function arguments
        }
        Expr::Let(let_expr, _, _, _) => Ok(()),
        Expr::ArgsTuple(args, _) => Ok(()),
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
            Ok(())
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
            Ok(())
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
            Ok(())
        }
    }
}

// fn is_subtype(ir: &IntermediateRepr, a: &ExprType, b: &ExprType) -> bool {
//     match (a, b) {
//         (ExprType::Atom(a), ExprType::Atom(b)) => ir.is_subtype(a, b),
//         (ExprType::Arrow(a), ExprType::Arrow(b)) => {
//             let a_arrow = a.as_ref();
//             let b_arrow = b.as_ref();
//             let return_type_ok = is_subtype(ir, &a_arrow.body_type, &b_arrow.body_type);
//             let arg_types_ok = a_arrow
//                 .param_types
//                 .iter()
//                 .zip(b_arrow.param_types.iter())
//                 .all(|(a, b)| is_subtype(ir, b, a));
//             return_type_ok && arg_types_ok
//         }
//         _ => false,
//     }
// }

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

pub fn infer_types_in_context(
    typing_context: &mut HashMap<String, FieldType>,
    expr: Arc<Expr<ExprMetadata>>,
) -> Arc<Expr<ExprMetadata>> {
    match expr.as_ref() {
        Expr::Var(ref var_name, (span, maybe_type)) => {
            // Assign variables from the context.
            if let Some(ctx_ty) = typing_context.get(var_name) {
                Arc::new(Expr::Var(
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
        Expr::Lambda(param_names, body, (span, maybe_type)) => {
            let mut local_typing_context = typing_context.clone();
            if let Some(FieldType::Arrow(arrow)) = maybe_type {
                for (param_type, param_name) in arrow.param_types.iter().zip(param_names.iter()) {
                    local_typing_context.insert(param_name.to_string(), param_type.clone());
                }
            }
            let new_body = infer_types_in_context(&mut local_typing_context, body.clone());
            Arc::new(Expr::Lambda(
                param_names.clone(),
                new_body,
                (span.clone(), maybe_type.clone()),
            ))
        }
        _ => expr.clone(),
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
