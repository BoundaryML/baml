//! AST lowering to HIR.
//!
//! This files contains the convertions between Baml AST nodes to HIR nodes.

use baml_types::{type_meta::base::StreamingBehavior, Constraint, ConstraintLevel, TypeValue};
use internal_baml_ast::ast::{self, App, Attribute, WithName, WithSpan};
use internal_baml_diagnostics::Span;

use crate::hir::{
    Block, Class, ClassConstructor, ClassConstructorField, Enum, EnumVariant, ExprFunction,
    Expression, Field, Hir, LlmFunction, Parameter, Statement, TypeM, TypeMeta,
};

impl Hir {
    /// Lower BAML AST into HIR.
    pub fn from_ast(ast: &ast::Ast) -> Self {
        let mut hir = Self {
            expr_functions: vec![],
            llm_functions: vec![],
            classes: vec![],
            enums: vec![],
            global_assignments: baml_types::BamlMap::new(),
        };

        // Add builtin classes and enums first
        hir.classes.extend(crate::builtin::builtin_classes());
        hir.enums.extend(crate::builtin::builtin_enums());

        for top in &ast.tops {
            match top {
                ast::Top::Function(function) => {
                    hir.llm_functions.push(LlmFunction::from_ast(function))
                }
                ast::Top::ExprFn(expr_fn) => {
                    hir.expr_functions.push(ExprFunction::from_ast(expr_fn))
                }
                ast::Top::Class(class) => hir.classes.push(Class::from_ast(class)),
                ast::Top::Enum(enum_def) => hir.enums.push(Enum::from_ast(enum_def)),
                ast::Top::TopLevelAssignment(assignment) => {
                    // Add toplevel assignments to global_assignments for HIR typechecking
                    let mut statements = vec![];
                    let mut temp_counter = 0;
                    let value = Expression::from_ast(&assignment.stmt.expr, &mut statements, &mut temp_counter);
                    hir.global_assignments.insert(assignment.stmt.identifier.to_string(), value);
                }
                _ => {}
            }
        }

        hir
    }
}

impl TypeM<TypeMeta> {
    pub fn from_ast_optional(r#type: Option<&ast::FieldType>) -> Self {
        match r#type {
            Some(r#type) => Self::from_ast(r#type),
            None => Self::Null(TypeMeta {
                span: Span::fake(),
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            }),
        }
    }

    pub fn from_ast(type_: &ast::FieldType) -> Self {
        let mut constraints = Vec::new();
        let mut streaming_behavior = StreamingBehavior::default();

        // Convert attributes to constraints and streaming behavior
        type_.attributes().iter().for_each(|attr: &Attribute| {
        match attr.name.name() {
            // Handle constraint attributes
            "assert" | "check" => {
                let level = match attr.name.name() {
                    "assert" => ConstraintLevel::Assert,
                    "check" => ConstraintLevel::Check,
                    _ => unreachable!(),
                };

                // Extract label and expression from arguments
                let arguments: Vec<&ast::Expression> = attr.arguments.arguments
                    .iter()
                    .map(|arg| &arg.value)
                    .collect();

                let (label, expression) = match arguments.as_slice() {
                    // Single argument: just the expression
                    [ast::Expression::JinjaExpressionValue(jinja_expr, _)] => {
                        (None, Some(jinja_expr.clone()))
                    }
                    // Two arguments: label and expression
                    [ast::Expression::Identifier(label_id), ast::Expression::JinjaExpressionValue(jinja_expr, _)] => {
                        (Some(label_id.to_string()), Some(jinja_expr.clone()))
                    }
                    _ => {
                        // Skip invalid constraint formats
                        (None, None)
                    }
                };

                if let Some(expr) = expression {
                    constraints.push(Constraint {
                        level,
                        expression: expr,
                        label,
                    });
                }
            }
            // Handle streaming behavior attributes
            "stream.not_null" => {
                streaming_behavior.needed = true;
            }
            "stream.done" => {
                streaming_behavior.done = true;
            }
            "stream.with_state" => {
                streaming_behavior.state = true;
            }
            _ => {
                // Ignore other attributes for now
            }
        }
    });

        let meta = TypeMeta {
            span: type_.span().clone(),
            constraints,
            streaming_behavior,
        };

        match type_ {
            ast::FieldType::Symbol(_, name, _) => {
                if name.name().starts_with("Enum") {
                    TypeM::EnumName(name.name().to_string(), meta)
                } else {
                    TypeM::ClassName(name.name().to_string(), meta)
                }
            }
            ast::FieldType::Primitive(_, prim, _, _) => match prim {
                TypeValue::Int => TypeM::Int(meta),
                TypeValue::String => TypeM::String(meta),
                TypeValue::Bool => TypeM::Bool(meta),
                TypeValue::Float => TypeM::String(meta), // TODO: Add Float type to TypeM
                TypeValue::Null => TypeM::String(meta),  // TODO: Add Null type to TypeM
                TypeValue::Media(_) => TypeM::String(meta), // TODO: Add Media type to TypeM
            },
            ast::FieldType::List(_, inner, _, _, _) => {
                TypeM::Array(Box::new(Self::from_ast(inner)), meta)
            }
            ast::FieldType::Map(_, box_pair, _, _) => TypeM::Map(
                Box::new(Self::from_ast(&box_pair.0)),
                Box::new(Self::from_ast(&box_pair.1)),
                meta,
            ),
            ast::FieldType::Union(_, types, _, _) => {
                TypeM::Union(types.iter().map(Self::from_ast).collect(), meta)
            }
            _ => TypeM::String(meta), // Default case for other variants
        }
    }
    pub fn get_meta(&self) -> &TypeMeta {
        match self {
            TypeM::Int(meta) => meta,
            TypeM::String(meta) => meta,
            TypeM::Bool(meta) => meta,
            TypeM::Null(meta) => meta,
            TypeM::Array(_, meta) => meta,
            TypeM::Map(_, _, meta) => meta,
            TypeM::ClassName(_, meta) => meta,
            TypeM::EnumName(_, meta) => meta,
            TypeM::Union(_, meta) => meta,
            TypeM::Arrow(_, meta) => meta,
        }
    }

    /// Is the type complex enough that it should be parenthesized if it's not
    /// top-level?
    pub fn complex(&self) -> bool {
        let meta = self.get_meta();
        if meta.streaming_behavior != StreamingBehavior::default() {
            return true;
        }
        if !meta.constraints.is_empty() {
            return true;
        }
        match self {
            TypeM::Union(_, _) => true,
            TypeM::Int(_) => false,
            TypeM::String(_) => false,
            TypeM::Bool(_) => false,
            TypeM::Array(_, _) => false,
            TypeM::Map(_, _, _) => false,
            TypeM::ClassName(_, _) => false,
            TypeM::EnumName(_, _) => false,
            TypeM::Null(_) => false,
            TypeM::Arrow(_, _) => true,
        }
    }
}

impl LlmFunction {
    pub fn from_ast(function: &ast::ValueExprBlock) -> Self {
        LlmFunction {
            name: function.name().to_string(),
            parameters: function
                .input()
                .map(|input| {
                    input
                        .args
                        .iter()
                        .map(|(name, param)| Parameter {
                            name: name.to_string(),
                            r#type: TypeM::from_ast(&param.field_type),
                            span: name.span().clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or(vec![]),

            return_type: TypeM::from_ast_optional(
                function.output().map(|output| &output.field_type),
            ),
            // return_type: TypeM::from_ast(function.output().unwrap_or(&FieldType::Primitive(
            //     FieldArity::Required,
            //     TypeValue::Null,
            //     Span::fake(),
            //     None,
            // ))),
            client: function
                .fields()
                .iter()
                .find(|attr| attr.name() == "client")
                .map(|attr| {
                    attr.expr
                        .as_ref()
                        .expect("client must be specified")
                        .to_string()
                })
                .unwrap_or("llm".to_string()),
            prompt: function
                .fields()
                .iter()
                .find(|attr| attr.name() == "prompt")
                .map(|attr| {
                    attr.expr
                        .as_ref()
                        .expect("prompt must be specified")
                        .to_string()
                })
                .unwrap_or("".to_string()),
            span: function.span().clone(),
        }
    }
}

impl ExprFunction {
    /// Lower an expression function into HIR.
    pub fn from_ast(function: &ast::ExprFn) -> Self {
        ExprFunction {
            name: function.name.to_string(),
            parameters: function
                .args
                .args
                .iter()
                .map(|(name, param)| Parameter {
                    name: name.to_string(),
                    r#type: TypeM::from_ast(&param.field_type),
                    span: name.span().clone(),
                })
                .collect::<Vec<_>>(),
            return_type: TypeM::from_ast_optional(function.return_type.as_ref()),
            body: Block::from_function_body(&function.body),
            span: function.span.clone(),
        }
    }
}

impl Block {
    /// Lower an expression block into HIR for function bodies (ends with Statement::Return).
    pub fn from_function_body(block: &ast::ExpressionBlock) -> Self {
        Self::from_ast_with_context(block, true)
    }

    /// Lower an expression block into HIR for expression blocks (ends with Statement::Expression).
    pub fn from_expression_block(block: &ast::ExpressionBlock) -> Self {
        Self::from_ast_with_context(block, false)
    }

    /// Lower an expression block into HIR with specified context.
    /// If is_function_body is true, the final expression becomes Statement::Return.
    /// If is_function_body is false, the final expression becomes Statement::Expression.
    fn from_ast_with_context(block: &ast::ExpressionBlock, is_function_body: bool) -> Self {
        let mut statements = vec![];

        // Process statements, checking for if expressions in let bindings
        for stmt in &block.stmts {
            match stmt {
                ast::Stmt::Let(ast::LetStmt {
                    identifier,
                    expr,
                    span,
                }) => {
                    // Regular let statement - but check for if expressions in nested contexts
                    // NOTE: Since we're not desugaring assignments, there will be no
                    // lifted statements.
                    let mut temp_counter = 0;
                    let mut lifted_statements = vec![];
                    let lifted_expr =
                        Expression::from_ast(expr, &mut lifted_statements, &mut temp_counter);

                    // Add any lifted statements first
                    statements.extend(lifted_statements);

                    // Then add the actual let statement
                    statements.push(Statement::Let {
                        name: identifier.to_string(),
                        value: lifted_expr,
                        span: span.clone(),
                    });
                }
                ast::Stmt::ForLoop(ast::ForLoopStmt {
                    identifier,
                    iterator,
                    body,
                    span,
                }) => {
                    // Lower for loop to HIR
                    let mut temp_counter = 0;
                    let mut lifted_statements = vec![];
                    let lifted_iterator =
                        Expression::from_ast(iterator, &mut lifted_statements, &mut temp_counter);

                    // Add any lifted statements first
                    statements.extend(lifted_statements);

                    // Add the for loop statement
                    statements.push(Statement::ForLoop {
                        identifier: identifier.name().to_string(),
                        iterator: Box::new(lifted_iterator),
                        block: Block::from_expression_block(body),
                        span: span.clone(),
                    });
                }
            }
        }

        // Handle if expressions specially in return position
        // Normal expression - but check for if expressions in nested contexts
        let mut temp_counter = 0;
        let mut lifted_statements = vec![];
        let lifted_expr = Expression::from_ast(
            block.expr.as_ref(),
            &mut lifted_statements,
            &mut temp_counter,
        );

        // Add any lifted statements first
        statements.extend(lifted_statements);

        // Then add the final statement
        statements.push(if is_function_body {
            Statement::Return {
                expr: lifted_expr,
                span: block.expr.span().clone(),
            }
        } else {
            Statement::Expression {
                expr: lifted_expr,
                span: block.expr.span().clone(),
            }
        });

        Block { statements }
    }
}

impl Expression {
    /// Lower an expression into HIR.
    ///
    /// If `with_lifting` is true, if expressions will be lifted to temporary variables
    /// and the statements will be added to the provided vector.
    /// If `with_lifting` is false, if expressions will fall back to placeholders.
    pub fn from_ast(
        expr: &ast::Expression,
        statements: &mut Vec<Statement>,
        temp_counter: &mut usize,
    ) -> Self {
        match expr {
            ast::Expression::ArrayAccess(base, index, span) => Expression::ArrayAccess {
                base: Box::new(Self::from_ast(base, statements, temp_counter)),
                index: Box::new(Self::from_ast(index, statements, temp_counter)),
                span: span.clone(),
            },
            ast::Expression::FieldAccess(base, field, span) => Expression::FieldAccess {
                base: Box::new(Self::from_ast(base, statements, temp_counter)),
                field: field.to_string(),
                span: span.clone(),
            },
            ast::Expression::BoolValue(value, span) => Expression::BoolValue(*value, span.clone()),
            ast::Expression::NumericValue(value, span) => {
                Expression::NumericValue(value.to_string(), span.clone())
            }
            ast::Expression::Identifier(identifier) => {
                Expression::Identifier(identifier.to_string(), identifier.span().clone())
            }
            ast::Expression::StringValue(value, span) => {
                Expression::StringValue(value.to_string(), span.clone())
            }
            ast::Expression::RawStringValue(raw_string) => Expression::RawStringValue(
                raw_string.inner_value.to_string(),
                raw_string.span().clone(),
            ),
            ast::Expression::Array(values, span) => Expression::Array(
                values
                    .iter()
                    .map(|value| Self::from_ast(value, statements, temp_counter))
                    .collect(),
                span.clone(),
            ),
            ast::Expression::App(App {
                name, args, span, ..
            }) => Expression::Call(
                name.to_string(),
                args.iter()
                    .map(|arg| Self::from_ast(arg, statements, temp_counter))
                    .collect(),
                span.clone(),
            ),
            ast::Expression::Map(pairs, span) => Expression::Map(
                pairs
                    .iter()
                    .map(|(key, value)| {
                        (
                            Self::from_ast(key, statements, temp_counter),
                            Self::from_ast(value, statements, temp_counter),
                        )
                    })
                    .collect(),
                span.clone(),
            ),
            ast::Expression::If(condition, if_branch, else_branch, span) => Expression::If {
                condition: Box::new(Self::from_ast(condition, statements, temp_counter)),
                if_branch: Box::new(Self::from_ast(if_branch, statements, temp_counter)),
                else_branch: else_branch
                    .as_ref()
                    .map(|block| Box::new(Self::from_ast(block, statements, temp_counter))),
                span: span.clone(),
            },
            ast::Expression::ExprBlock(block, span) => {
                // Expression blocks are lowered to HIR preserving their structure
                // This maintains proper scoping - variables defined inside the block
                // are only visible within that block
                Expression::ExpressionBlock(
                    Box::new(Block::from_expression_block(block)),
                    span.clone(),
                )
            }
            ast::Expression::Lambda(_args, _body, span) => {
                todo!("lambdas are not yet implemented")
            }
            ast::Expression::ClassConstructor(cc, span) => {
                // TODO: To handle spreads, if there is a spread, compute a sequence
                // of (field_name, spread_value.field_name) pairs. Use these pairs
                // in the lowering of ClassConstructors, for each field of the class
                // not present in the class constructor.
                //
                // We can't do this yet because we have no syntax for field accessors.

                Expression::ClassConstructor(
                    ClassConstructor {
                        class_name: cc.class_name.to_string(),
                        fields: cc
                            .fields
                            .iter()
                            .map(|field| match field {
                                ast::ClassConstructorField::Named(name, expr) => {
                                    ClassConstructorField::Named {
                                        name: name.to_string(),
                                        value: Self::from_ast(expr, statements, temp_counter),
                                    }
                                }
                                ast::ClassConstructorField::Spread(expr) => {
                                    ClassConstructorField::Spread {
                                        value: Self::from_ast(expr, statements, temp_counter),
                                    }
                                }
                            })
                            .collect(),
                    },
                    span.clone(),
                )
            }
            ast::Expression::JinjaExpressionValue(jinja, span) => {
                Expression::JinjaExpressionValue(jinja.to_string(), span.clone())
            }
        }
    }
}

impl Class {
    /// Lower a class from AST to HIR.
    pub fn from_ast(class: &ast::TypeExpressionBlock) -> Self {
        Class {
            name: class.name().to_string(),
            fields: class
                .fields
                .iter()
                .map(|field| Field {
                    name: field.name().to_string(),
                    r#type: field
                        .expr
                        .as_ref()
                        .map(|field_type| TypeM::from_ast(field_type))
                        .unwrap_or_else(|| {
                            TypeM::String(TypeMeta {
                                span: field.span().clone(),
                                constraints: Vec::new(),
                                streaming_behavior: StreamingBehavior::default(),
                            })
                        }),
                    span: field.span().clone(),
                })
                .collect(),
            span: class.span().clone(),
        }
    }
}

impl Enum {
    /// Lower an enum from AST to HIR.
    pub fn from_ast(enum_def: &ast::TypeExpressionBlock) -> Self {
        Enum {
            name: enum_def.name().to_string(),
            variants: enum_def
                .fields
                .iter()
                .map(|field| EnumVariant {
                    name: field.name().to_string(),
                    span: field.span().clone(),
                })
                .collect(),
            span: enum_def.span().clone(),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    /// Test helper to generate HIR from BAML source
    fn hir_from_source(source: &'static str) -> String {
        let parser_db = crate::test::ast(source).unwrap_or_else(|e| panic!("{}", e));
        let hir = Hir::from_ast(&parser_db.ast);
        hir.pretty_print()
    }

    // Test cases start here

    #[test]
    fn test_simple_expression_function() {
        let source = r#"
          function MyFunc(x: int, y: string) -> int {
              42
          }
      "#;
        let expected = r#"function MyFunc(x, y) {
  return 42;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_expression_with_let_binding() {
        let source = r#"
          function AddOne(x: int) -> int {
              let y = x;
              y
          }
      "#;
        let expected = r#"function AddOne(x) {
  let y = x;
  return y;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_basic_expressions() {
        let source = r#"
          function TestExpressions() -> string {
              let bool_val = true;
              let num_val = 123.45;
              let str_val = "hello";
              str_val
          }
      "#;
        let expected = r#"function TestExpressions() {
  let bool_val = true;
  let num_val = 123.45;
  let str_val = "hello";
  return str_val;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_array_expression() {
        let source = r#"
          function TestArray() -> int[] {
              [1, 2, 3]
          }
      "#;
        let expected = r#"function TestArray() {
  return [1, 2, 3];
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_function_call() {
        let source = r#"
          function myFunc(x: int, y: string) -> int {
              x
          }

          function CallTest() -> int {
              let result = myFunc(42, "hello");
              result
          }
      "#;
        let expected = r#"function myFunc(x, y) {
  return x;
}

function CallTest() {
  let result = myFunc(42, "hello");
  return result;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    // Note: LLM function test disabled due to string literal parsing issues
    // TODO: Re-enable and fix string literal issues
    #[test]
    fn test_pretty_print_demo() {
        let source = r#"
          function fibonacci(n: int) -> int {
              let a = 0;
              let b = 1;
              let result = add(a, b);
              result
          }

          fn add(x: int, y: int) -> int {
              x
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        println!("\n=== HIR Pretty Print Demo ===");
        println!("Original HIR structure:");
        println!("{}", hir.pretty_print());
        println!("\n=== With different line widths ===");
        println!("Line width 40:");
        println!("{}", hir.pretty_print_with_options(40, 2));
        println!("\nLine width 120:");
        println!("{}", hir.pretty_print_with_options(120, 2));
    }

    #[test]
    fn test_pretty_print_expression_function() {
        let source = r#"
          function AddOne(x: int) -> int {
              let y = x;
              y
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        let pretty_printed = hir.pretty_print();
        // Check that the pretty printed output contains the expected structure
        assert!(pretty_printed.contains("function AddOne(x)"));
        assert!(pretty_printed.contains("let y = x;"));
        assert!(pretty_printed.contains("return y;"));
        // Print it for visual inspection
        println!("Pretty printed HIR:");
        println!("{}", pretty_printed);
    }
    #[test]
    fn test_pretty_print_array_and_call() {
        let source = r#"
          function helper(x: int) -> int {
              x
          }

          function TestArray() -> int[] {
              let arr = [1, 2, 3];
              let result = helper(42);
              [arr, result]
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        let pretty_printed = hir.pretty_print();
        // Check that the pretty printed output contains the expected structure
        assert!(pretty_printed.contains("function helper(x)"));
        assert!(pretty_printed.contains("function TestArray()"));
        assert!(pretty_printed.contains("let arr = [1, 2, 3];"));
        assert!(pretty_printed.contains("let result = helper(42);"));
        assert!(pretty_printed.contains("[arr, result]"));
        // Print it for visual inspection
        println!("Pretty printed HIR with arrays and calls:");
        println!("{}", pretty_printed);
    }
    #[test]
    fn test_indentation_consistency() {
        let source = r#"
          function simple() -> string {
              "hello"
          }
      "#;
        let expected = r#"function simple() {
  return "hello";
}"#;
        assert_eq!(hir_from_source(source), expected);
    }
    #[test]
    fn test_if_expression_desugaring() {
        // Test if expression desugaring in let bindings
        let source = r#"
          function simpleIf() -> string {
              let x = if true { "yes" } else { "no" };
              x
          }
      "#;
        let expected = r#"function simpleIf() {
  let x = if true { "yes" } else { "no" };
  return x;
}"#;
        assert_eq!(hir_from_source(source), expected);
    }

    #[test]
    fn test_attribute_conversion() {
        // Test constraint attributes
        let source = r#"
          function TestConstraints() -> string @assert("this.length > 0") @check("this != 'bad'") {
              "hello"
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);

        // The HIR should have the constraints stored in the type metadata
        // This test verifies that the attribute parsing doesn't crash
        let pretty = hir.pretty_print();
        assert!(pretty.contains("function TestConstraints"));
    }

    #[test]
    fn test_streaming_behavior_attributes() {
        // Test streaming behavior attributes
        let source = r#"
          class MyClass {
              field1 string @stream.done
              field2 int @stream.not_null
              field3 bool @stream.with_state
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);

        // Verify the HIR was created successfully with streaming attributes
        let pretty = hir.pretty_print();
        assert!(pretty.contains("class MyClass"));
        assert!(pretty.contains("field1: string @stream.done"));
        assert!(pretty.contains("field2: int @stream.needed"));
        assert!(pretty.contains("field3: bool @stream.with_state"));
    }

    #[test]
    fn test_class_with_constraints() {
        // Test class fields with constraint attributes - simplified test for now
        let source = r#"
          class User {
              name string
              age int
              email string
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        let pretty = hir.pretty_print();

        // Verify the class pretty prints successfully
        assert!(pretty.contains("class User"));
        assert!(pretty.contains("name: string"));
        assert!(pretty.contains("age: int"));
        assert!(pretty.contains("email: string"));

        // Print for visual inspection
        println!("Simple class:");
        println!("{}", pretty);
    }

    #[test]
    fn test_constraint_parsing() {
        // Test that constraints are properly parsed from AST to HIR
        let source = r#"
          class User {
              name string @assert({{ this.length > 0 }})
              age int @check(valid_age, {{ this >= 0 }})
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);

        // Find the User class
        let user_class = hir
            .classes
            .iter()
            .find(|c| c.name == "User")
            .expect("User class not found");

        // Check name field constraints
        let name_field = user_class
            .fields
            .iter()
            .find(|f| f.name == "name")
            .expect("name field not found");
        let name_meta = name_field.r#type.get_meta();
        assert_eq!(name_meta.constraints.len(), 1);
        let name_constraint = &name_meta.constraints[0];
        assert_eq!(name_constraint.level, baml_types::ConstraintLevel::Assert);
        assert_eq!(name_constraint.expression.0, "this.length > 0");
        assert_eq!(name_constraint.label, None);

        // Check age field constraints
        let age_field = user_class
            .fields
            .iter()
            .find(|f| f.name == "age")
            .expect("age field not found");
        let age_meta = age_field.r#type.get_meta();
        assert_eq!(age_meta.constraints.len(), 1);
        let age_constraint = &age_meta.constraints[0];
        assert_eq!(age_constraint.level, baml_types::ConstraintLevel::Check);
        assert_eq!(age_constraint.expression.0, "this >= 0");
        assert_eq!(age_constraint.label, Some("valid_age".to_string()));

        println!("Constraint parsing test passed!");
    }

    #[test]
    fn test_complex_class_with_mixed_attributes() {
        // Test class with streaming behaviors (no constraints for now)
        let source = r#"
          class StreamingUser {
              id string @stream.done
              username string @stream.not_null
              messages string[] @stream.with_state
              score int @stream.done
              metadata map<string, string> @stream.not_null
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        let pretty = hir.pretty_print();

        // Verify the class pretty prints with streaming attributes
        assert!(pretty.contains("class StreamingUser"));
        assert!(pretty.contains("id: string @stream.done"));
        assert!(pretty.contains("username: string @stream.needed"));
        assert!(pretty.contains("messages: arraystring @stream.with_state"));
        assert!(pretty.contains("score: int @stream.done"));
        assert!(pretty.contains("metadata: mapstring:string @stream.needed"));

        // Print for visual inspection
        println!("\nClass with streaming attributes:");
        println!("{}", pretty);
    }

    #[test]
    fn test_nested_types_with_attributes() {
        // Test nested types (arrays, unions) with streaming attributes only for now
        let source = r#"
          class DataModel {
              tags string[]
              status (string | int) @stream.done
              matrix int[][]
              config map<string, bool> @stream.not_null
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        let pretty = hir.pretty_print();

        // Print for visual inspection
        println!("\nNested types with streaming attributes:");
        println!("{}", pretty);

        // Verify nested types with attributes
        assert!(pretty.contains("class DataModel"));
        assert!(pretty.contains("tags:"));
        assert!(pretty.contains("status:"));
        assert!(pretty.contains("matrix:"));
        assert!(pretty.contains("config:"));
    }

    #[test]
    fn test_enum_indentation() {
        // Test enum pretty printing with proper indentation
        let source = r#"
          enum Status {
              PENDING
              APPROVED
              REJECTED
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        let pretty = hir.pretty_print();

        // Print for visual inspection
        println!("\nEnum with proper indentation:");
        println!("{}", pretty);

        // Verify enum structure
        assert!(pretty.contains("enum Status"));
        assert!(pretty.contains("PENDING"));
        assert!(pretty.contains("APPROVED"));
        assert!(pretty.contains("REJECTED"));
    }

    #[test]
    fn test_pretty_print_complex_structures() {
        let source = r#"
          function complexFunction(a: int, b: string, c: bool) -> string {
              let nested_array = [[1, 2], [3, 4]];
              let result = helper(a, b);
              result
          }

          function helper(x: int, y: string) -> string {
              "result"
          }
      "#;
        let ast = crate::test::ast(source).unwrap().ast;
        let hir = Hir::from_ast(&ast);
        let pretty_printed = hir.pretty_print();
        // Check that it contains the expected structure
        assert!(pretty_printed.contains("function complexFunction(a, b, c)"));
        assert!(pretty_printed.contains("function helper(x, y)"));
        assert!(pretty_printed.contains("[[1, 2], [3, 4]]"));
        assert!(pretty_printed.contains("helper(a, b)"));
        // Test custom formatting options
        let narrow_format = hir.pretty_print_with_options(40, 4);
        assert!(narrow_format.len() > 0);
        // Print for visual inspection
        println!("Pretty printed complex HIR:");
        println!("{}", pretty_printed);
        println!("\nNarrow format (40 chars wide):");
        println!("{}", narrow_format);
    }

    // TODO: This is broken.
    #[test]
    #[ignore] // This is about to change.
    fn test_if_expression_in_return_position() {
        // Test if expression desugaring in return position
        let source = r#"
          function conditionalReturn(flag: bool) -> string {
              if flag { "success" } else { "failure" }
          }
      "#;
        let expected = r#"function conditionalReturn(flag) {
if flag {
return "success";
} else {
return "failure";
}
}"#;
        assert_eq!(hir_from_source(source), expected);
    }

    #[test]
    fn test_nested_expression_blocks() {
        // Test nested expression blocks with proper scoping
        let source = r#"
          function Foo() -> int {
              let x = {
                  let y = 1;
                  y
              };
              x
          }
      "#;
        // Expression blocks now properly preserve scope - the inner block
        // maintains its own variables which are not visible outside
        let result = hir_from_source(source);
        let expected = r#"function Foo() {
  let x = {
  let y = 1;
    y
  };
  return x;
}"#;
        assert_eq!(result, expected);
    }

    #[test]
    #[ignore] // This is about to change.
    fn test_class_constructor_with_complex_expressions() {
        // Test class constructor with both if expressions and expression blocks
        let source = r#"
          class Foo {
              a int
              b int
          }

          function TestConstructor() -> Foo {
              Foo { a: if true { 1 } else { 0 }, b: { let y = 1; y } }
          }
      "#;
        let result = hir_from_source(source);
        // The if expression in field 'a' should get lifted to temporary variables
        // The expression block in field 'b' should work correctly.
        let expected = r#"function TestConstructor() {
return Foo { a: if true { 1 } else { 0 }, b: {
let y = 1;
  y
} };
}

class Foo {
a: int
b: int
}"#;
        assert_eq!(result, expected);
        // Print for visual inspection
        println!("HIR for class constructor with complex expressions:");
        println!("{}", result);
    }

    #[test]
    #[ignore] // TODO: This doesn't pass syntax validation.
    fn test_for_loop_lowering() {
        // Test for loop lowering to while loop with iterator
        let source = r#"
           function TestForLoop() -> int[] {
               for (item in [1, 2, 3]) { mul(item, 2) }
           }
       "#;
        let result = hir_from_source(source);

        // The for loop should be lowered to:
        // - iterator variable declaration
        // - index variable initialization
        // - result array initialization
        // - while loop with condition and body
        let expected = r#"function TestForLoop() {
 let iter_0 = [1, 2, 3];
 let index_0 = 0;
 let result_0 = [];
 while lt(index_0, length(iter_0)) {
 let item = index(iter_0, index_0);
   var temp_push_1 = push(result_0, mul(item, 2));
   index_0 = add(index_0, 1);
 }
 return result_0;
}"#;
        assert_eq!(result, expected);

        // Print for visual inspection
        println!("HIR for for loop lowering:");
        println!("{}", result);
    }
}
