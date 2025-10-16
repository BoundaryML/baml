//! Parser entry point.

use std::path::{Path, PathBuf};

use internal_baml_diagnostics::{DatamodelError, Diagnostics, SourceFile};
use pest::Parser;

use super::{
    parse_assignment::parse_assignment,
    parse_expr::{parse_comment_header_pair, parse_expr_fn, parse_top_level_assignment},
    parse_expression::parse_expression,
    parse_template_string::parse_template_string,
    parse_type_expression_block::parse_type_expression_block,
    parse_value_expression_block::parse_value_expression_block,
    BAMLParser, Rule,
};
use crate::ast::*;

#[cfg(feature = "debug_parser")]
#[allow(dead_code)]
fn pretty_print<'a>(pair: pest::iterators::Pair<'a, Rule>, indent_level: usize) {
    // Indentation for the current level
    let indent = "  ".repeat(indent_level);

    // Print the rule and its span
    println!("{}{:?} -> {:?}", indent, pair.as_rule(), pair.as_str());

    // Recursively print inner pairs with increased indentation
    for inner_pair in pair.into_inner() {
        pretty_print(inner_pair, indent_level + 1);
    }
}

/// Parse a standalone BAML expression.
pub fn parse_standalone_expression(
    input: &str,
    diagnostics: &mut Diagnostics,
) -> anyhow::Result<Expression> {
    let datamodel_result = BAMLParser::parse(Rule::expression, input);
    match datamodel_result {
        Ok(mut datamodel_wrapped) => {
            let datamodel = datamodel_wrapped.next().unwrap();
            let expression =
                parse_expression(datamodel, diagnostics).expect("parse_expression failed");
            Ok(expression)
        }
        Err(err) => {
            panic!("BAMLParser failed: Handle this error: {err:?}");
        }
    }
}

/// Parse Baml source code and return its AST.
/// It validates some basic things on the AST like name conflicts. Further
/// validation is in baml-core.
pub fn parse(root_path: &Path, source: &SourceFile) -> Result<(Ast, Diagnostics), Diagnostics> {
    let mut diagnostics = Diagnostics::new(root_path.to_path_buf());
    diagnostics.set_source(source);

    if !source.path().ends_with(".baml") {
        diagnostics.push_error(DatamodelError::new_validation_error(
            &format!(
                "A BAML file must have the file extension `.baml`, but found: {}",
                source.path()
            ),
            Span::empty(source.clone()),
        ));
        return Err(diagnostics);
    }

    let datamodel_result = BAMLParser::parse(Rule::schema, source.as_str());
    match datamodel_result {
        Ok(mut datamodel_wrapped) => {
            let datamodel = datamodel_wrapped.next().unwrap();

            // Run the code with:
            // cargo build --features "debug_parser"
            #[cfg(feature = "debug_parser")]
            pretty_print(datamodel.clone(), 0);

            let mut top_level_definitions = Vec::new();

            let mut pending_block_comment = None;
            let mut pending_headers: Vec<Header> = Vec::new();
            let mut pairs = datamodel.into_inner().peekable();

            while let Some(current) = pairs.next() {
                match current.as_rule() {
                    Rule::top_level_assignment => {
                        // Clear pending headers since assignments don't support headers
                        pending_headers.clear();
                        if let Some(top_level_assignment) =
                            parse_top_level_assignment(current, &mut diagnostics)
                        {
                            top_level_definitions
                                .push(Top::TopLevelAssignment(top_level_assignment));
                        }
                    }
                    Rule::expr_fn => {
                        if let Some(mut expr_fn) = parse_expr_fn(current, &mut diagnostics) {
                            // Bind pending headers to this function
                            expr_fn.annotations =
                                pending_headers.drain(..).map(std::sync::Arc::new).collect();
                            top_level_definitions.push(Top::ExprFn(expr_fn));
                        }
                    }
                    Rule::type_expression_block => {
                        // Clear pending headers since type expressions don't support headers
                        pending_headers.clear();
                        let type_expr = parse_type_expression_block(
                            current,
                            pending_block_comment.take(),
                            &mut diagnostics,
                        );

                        match type_expr.sub_type {
                            SubType::Class => top_level_definitions.push(Top::Class(type_expr)),
                            SubType::Enum => top_level_definitions.push(Top::Enum(type_expr)),
                            _ => (), // may need to save other somehow for error propagation
                        }
                    }
                    Rule::value_expression_block => {
                        let val_expr = parse_value_expression_block(
                            current,
                            pending_block_comment.take(),
                            &mut diagnostics,
                        );
                        match val_expr {
                            Ok(mut val) => {
                                // Bind pending headers to all value expression blocks (function, client, test, generator, retry_policy)
                                val.annotations =
                                    pending_headers.drain(..).map(std::sync::Arc::new).collect();
                                top_level_definitions.push(match val.block_type {
                                    ValueExprBlockType::Function => Top::Function(val),
                                    ValueExprBlockType::Test => Top::TestCase(val),
                                    ValueExprBlockType::Client => Top::Client(val),
                                    ValueExprBlockType::RetryPolicy => Top::RetryPolicy(val),
                                    ValueExprBlockType::Generator => Top::Generator(val),
                                });
                            }
                            Err(e) => diagnostics.push_error(e),
                        }
                    }
                    Rule::type_alias => {
                        let assignment = parse_assignment(current, &mut diagnostics);
                        top_level_definitions.push(Top::TypeAlias(assignment));
                    }

                    Rule::template_declaration => {
                        match parse_template_string(
                            current,
                            pending_block_comment.take(),
                            &mut diagnostics,
                        ) {
                            Ok(template) => {
                                top_level_definitions.push(Top::TemplateString(template))
                            }
                            Err(e) => diagnostics.push_error(e),
                        }
                    }

                    Rule::EOI => {}
                    Rule::CATCH_ALL => {
                        diagnostics.push_error(DatamodelError::new_validation_error(
                        "This line is invalid. It does not start with any known Baml schema keyword.",
                        diagnostics.span(current.as_span()),
                    ));
                        break;
                    }
                    Rule::comment_block => {
                        let headers =
                            headers_from_comment_block_top_level(current.clone(), &mut diagnostics);
                        if !headers.is_empty() {
                            pending_headers.extend(headers);
                            continue;
                        }
                        match pairs.peek().map(|b| b.as_rule()) {
                            Some(Rule::empty_lines) => {
                                // free floating
                            }
                            // Some(Rule::enum_declaration) => {
                            //     pending_block_comment = Some(current);
                            // }
                            _ => {
                                pending_block_comment = Some(current);
                            }
                        }
                    }
                    // We do nothing here.
                    Rule::raw_string_literal => (),
                    Rule::empty_lines => (),
                    _ => unreachable!("Encountered an unknown rule: {:?}", current.as_rule()),
                }
            }

            Ok((
                Ast {
                    tops: top_level_definitions,
                },
                diagnostics,
            ))
        }
        Err(err) => {
            let location: Span = match err.location {
                pest::error::InputLocation::Pos(pos) => Span {
                    file: source.clone(),
                    start: pos,
                    end: pos,
                },
                pest::error::InputLocation::Span((from, to)) => Span {
                    file: source.clone(),
                    start: from,
                    end: to,
                },
            };

            let expected = match err.variant {
                pest::error::ErrorVariant::ParsingError { positives, .. } => {
                    get_expected_from_error(&positives)
                }
                _ => panic!("Could not construct parsing error. This should never happend."),
            };

            diagnostics.push_error(DatamodelError::new_parser_error(expected, location));
            Err(diagnostics)
        }
    }
}

fn headers_from_comment_block_top_level(
    token: pest::iterators::Pair<'_, Rule>,
    diagnostics: &mut Diagnostics,
) -> Vec<Header> {
    if token.as_rule() != Rule::comment_block {
        return Vec::new();
    }

    let mut headers = Vec::new();
    for current in token.into_inner() {
        if current.as_rule() == Rule::comment {
            if let Some(header) = parse_comment_header_pair(&current, diagnostics) {
                headers.push(header);
            }
        }
    }
    headers
}

fn get_expected_from_error(positives: &[Rule]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(positives.len() * 6);

    for positive in positives {
        write!(out, "{positive:?}").unwrap();
    }

    out
}

#[cfg(test)]
mod tests {

    use std::path::Path;

    use baml_types::{expr::Expr, TypeValue};
    // Add this line to import the ast module
    use internal_baml_diagnostics::SourceFile;

    use super::parse;
    use crate::ast::*;

    #[test]
    // #[test_log::test]
    fn test_parse_schema() {
        let input = r#"
            class MyClass {
                myProperty string[] @description("This is a description") @alias("MP")
                prop2 string @description({{ "a " + "b" }})
            }
        "#;

        let root_path = "test_file.baml";
        let source = SourceFile::new_static(root_path.into(), input);

        let result = parse(Path::new(root_path), &source);

        assert!(result.is_ok());
        let (ast, _) = result.unwrap();

        assert_eq!(ast.tops.len(), 1);

        match &ast.tops[0] {
            Top::Class(TypeExpressionBlock { name, fields, .. }) => {
                assert_eq!(name.name(), "MyClass");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name.name(), "myProperty");
                assert_eq!(fields[1].name.name(), "prop2");
                assert_eq!(fields[0].attributes.len(), 2);
                assert_eq!(fields[1].attributes.len(), 1);
            }
            _ => panic!("Expected a model declaration"),
        }
    }

    #[test]
    fn test_example() {
        let input = r##"
          function EvaluateCaption(imgs: image[], captions: string[]) -> string {
            client GPTo4
            prompt #"
              Evaluate the quality of the captions for the images.

              {{ imgs }}
              {{ captions }}
            "#
          }

          test EvaluateCaptionTest {
            functions [EvaluateCaption]
            args {
              image [{
                file ../../files/images/restaurant.png
              },{
                file ../../files/images/bear.png
              }]
              captions [
                #"
                  A bear walking next to a rabbit in the woods.
                "#,
                #"
                  A restaurant full of diners.
                "#,
              ]
            }
          }
        "##;

        let root_path = "example_file.baml";
        let source = SourceFile::new_static(root_path.into(), input);

        let result = parse(Path::new(root_path), &source).unwrap();
        assert_eq!(result.1.errors().len(), 0);
    }

    #[test]
    fn test_comments() {
        let input = r##"
          /// Doc comment for Foo
          /// has multiple lines
          class Foo {
            /// A nice bar.
            bar int

            /// Followed by a
            /// multiline baz.
            baz string
          }

          /// Documented enum.
          enum E {
            /// Documented variant.
            EFoo

            /// Another documented variant.
            EBar
            EBaz
          }
        "##;
        let root_path = "a.baml";
        let source = SourceFile::new_static(root_path.into(), input);
        let schema = parse(Path::new(root_path), &source).unwrap().0;
        let mut tops = schema.iter_tops();
        let foo_top = tops.next().unwrap().1;
        match foo_top {
            Top::Class(TypeExpressionBlock {
                name,
                fields,
                documentation,
                ..
            }) => {
                assert_eq!(name.to_string().as_str(), "Foo");
                assert_eq!(
                    documentation.as_ref().unwrap().text.as_str(),
                    "Doc comment for Foo\nhas multiple lines"
                );
                match fields.as_slice() {
                    [field1, field2] => {
                        assert_eq!(
                            field1.documentation.as_ref().unwrap().text.as_str(),
                            "A nice bar."
                        );
                        assert_eq!(
                            field2.documentation.as_ref().unwrap().text.as_str(),
                            "Followed by a\nmultiline baz."
                        );
                    }
                    _ => {
                        panic!("Expected exactly 2 fields");
                    }
                }
            }
            _ => {
                panic!("Expected class.")
            }
        }
        let e_top = tops.next().unwrap().1;
        match e_top {
            Top::Enum(TypeExpressionBlock {
                name,
                fields,
                documentation,
                ..
            }) => {
                assert_eq!(name.to_string().as_str(), "E");
                assert_eq!(
                    documentation.as_ref().unwrap().text.as_str(),
                    "Documented enum."
                );
                match fields.as_slice() {
                    [field1, field2, field3] => {
                        assert_eq!(
                            field1.documentation.as_ref().unwrap().text.as_str(),
                            "Documented variant."
                        );
                        assert_eq!(
                            field2.documentation.as_ref().unwrap().text.as_str(),
                            "Another documented variant."
                        );
                        assert!(field3.documentation.is_none());
                    }
                    _ => {
                        panic!("Expected exactly 3 enum variants");
                    }
                }
            }
            _ => {
                panic!("Expected enum. got {e_top:?}")
            }
        }
    }

    #[test]
    fn test_push_type_aliases() {
        let input = "type One = int\ntype Two = string | One";

        let path = "example_file.baml";
        let source = SourceFile::new_static(path.into(), input);

        let (ast, _) = parse(Path::new(path), &source).unwrap();

        let [Top::TypeAlias(one), Top::TypeAlias(two)] = ast.tops.as_slice() else {
            panic!(
                "Expected two type aliases (type One, type Two), got: {:?}",
                ast.tops
            );
        };

        assert_eq!(one.identifier.to_string(), "One");
        assert!(matches!(
            one.value,
            FieldType::Primitive(_, TypeValue::Int, _, _)
        ));

        assert_eq!(two.identifier.to_string(), "Two");
        let FieldType::Union(_, elements, _, _) = &two.value else {
            panic!("Expected union type (string | One), got: {:?}", two.value);
        };

        let [FieldType::Primitive(_, TypeValue::String, _, _), FieldType::Symbol(_, alias, _)] =
            elements.as_slice()
        else {
            panic!("Expected union type (string | One), got: {:?}", two.value);
        };

        assert_eq!(alias.to_string(), "One");
    }

    #[test]
    fn test_top_level_assignment() {
        let input = "let x = 1;";
        let path = "example_file.baml";
        let source = SourceFile::new_static(path.into(), input);
        let (ast, _) = parse(Path::new(path), &source).unwrap();
        match ast.tops.as_slice() {
            [Top::TopLevelAssignment(x)] => {
                assert_eq!(x.stmt.identifier.name(), "x");
            }
            _ => panic!("Expected a single top level assignment."),
        }
    }

    #[test]
    fn test_top_level_block_assignment() {
        let input = r#"
          let x = {
            let y = 10;
            go(y, 20)
          };
        "#;
        let path = "example_file.baml";
        let source = SourceFile::new_static(path.into(), input);
        let (ast, _) = parse(Path::new(path), &source).unwrap();
        match ast.tops.as_slice() {
            [Top::TopLevelAssignment(x)] => {
                dbg!(&x);
                dbg!(&x.stmt);
                assert_eq!(x.stmt.identifier.name(), "x");
                match &x.stmt.expr {
                    Expression::ExprBlock(ExpressionBlock { stmts, expr, .. }, _) => {
                        assert_eq!(stmts.len(), 1);
                        assert_eq!(stmts[0].identifier().name(), "y");
                        assert!(expr.as_ref().is_some());
                        assert!(matches!(
                            expr.as_ref().unwrap().as_ref(),
                            Expression::App(_)
                        ));
                    }
                    _ => panic!("Expected ExpressionBlock"),
                }
            }
            _ => panic!("Expected a single top level assignment."),
        }
    }
}
