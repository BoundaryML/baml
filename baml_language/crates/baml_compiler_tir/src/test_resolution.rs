//! Tests for the resolution system.

#[cfg(test)]
mod tests {
    use super::super::*;
    use baml_base::{FileId, Name};
    use baml_compiler_hir::{Expr, ExprBody, FunctionBody, FunctionLoc, Literal, Pattern, Stmt};
    use std::collections::HashMap;

    /// Test that resolution information is captured during type inference.
    #[test]
    fn test_resolution_capture() {
        // Create a simple test database
        let db = baml_workspace::TestDatabase::default();

        // Create a simple function body with:
        // let x = 42
        // x
        let mut body = ExprBody::default();

        // Create the literal 42
        let lit_42 = body.exprs.insert(Expr::Literal(Literal::Int(42)));

        // Create the pattern for x
        let pat_x = body.patterns.insert(Pattern::Binding(Name::new("x")));

        // Create the let statement
        let let_stmt = body.stmts.insert(Stmt::Let {
            pattern: pat_x,
            type_annotation: None,
            type_span: None,
            initializer: Some(lit_42),
            is_watched: false,
        });

        // Create the path expression for x
        let path_x = body.exprs.insert(Expr::Path(vec![Name::new("x")]));

        // Create a block with the let statement and x as the tail expression
        let block = body.exprs.insert(Expr::Block {
            stmts: vec![let_stmt],
            tail_expr: Some(path_x),
        });

        body.root_expr = Some(block);

        // Create a simple function
        let function_body = FunctionBody::Expr(body);

        // Prepare for type inference
        let param_types = HashMap::new();
        let expected_return = Ty::Int;
        let file_id = FileId::new(&db, 0);
        let function_loc = FunctionLoc::new(&db, file_id, 0.into());

        // Run type inference
        let result = infer_function_body(
            &db,
            &function_body,
            param_types,
            &expected_return,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            function_loc,
        );

        // Check that we have resolutions
        assert!(!result.expr_resolutions.is_empty(), "Should have captured resolutions");

        // Check that the path expression for 'x' resolved to a local
        if let Some(resolution) = result.expr_resolutions.get(&path_x) {
            match resolution {
                ResolvedValue::Local { name, definition_site } => {
                    assert_eq!(name.as_str(), "x");
                    assert_eq!(*definition_site, Some(crate::DefinitionSite::Statement(let_stmt)));
                }
                _ => panic!("Expected local resolution for 'x'"),
            }
        } else {
            panic!("No resolution found for path expression 'x'");
        }

        // Check that we inferred the correct type
        assert_eq!(result.return_type, Ty::Int);
    }

    /// Test that builtin function resolutions are captured.
    #[test]
    fn test_builtin_resolution() {
        let db = baml_workspace::TestDatabase::default();
        let mut body = ExprBody::default();

        // Create a path expression for baml.deep_copy
        let builtin_path = body.exprs.insert(Expr::Path(vec![
            Name::new("baml"),
            Name::new("deep_copy"),
        ]));

        body.root_expr = Some(builtin_path);

        let function_body = FunctionBody::Expr(body);
        let param_types = HashMap::new();
        let expected_return = Ty::Unknown;
        let file_id = FileId::new(&db, 0);
        let function_loc = FunctionLoc::new(&db, file_id, 0.into());

        // Run type inference
        let result = infer_function_body(
            &db,
            &function_body,
            param_types,
            &expected_return,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            function_loc,
        );

        // Check that the builtin was resolved
        if let Some(resolution) = result.expr_resolutions.get(&builtin_path) {
            match resolution {
                ResolvedValue::BuiltinFunction { path } => {
                    assert_eq!(path, "baml.deep_copy");
                }
                _ => panic!("Expected builtin function resolution"),
            }
        } else {
            panic!("No resolution found for builtin path");
        }
    }
}