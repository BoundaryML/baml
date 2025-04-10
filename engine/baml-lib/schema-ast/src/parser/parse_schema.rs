use std::path::{Path, PathBuf};

use super::{
    parse_assignment::parse_assignment, parse_template_string::parse_template_string,
    parse_type_expression_block::parse_type_expression_block,
    parse_value_expression_block::parse_value_expression_block, BAMLParser, Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics, SourceFile};
use pest::Parser;

#[cfg(feature = "debug_parser")]
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

/// Parse a PSL string and return its AST.
/// It validates some basic things on the AST like name conflicts. Further validation is in baml-core
pub fn parse_schema(
    root_path: &Path,
    source: &SourceFile,
) -> Result<(SchemaAst, Diagnostics), Diagnostics> {
    let mut diagnostics = Diagnostics::new(root_path.to_path_buf());
    diagnostics.set_source(source);

    if !source.path().ends_with(".baml") {
        diagnostics.push_error(DatamodelError::new_validation_error(
            &format!(
                "A BAML file must have the file extension `.baml`, but found: {}",
                source.path().to_string()
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
            let mut pairs = datamodel.into_inner().peekable();

            while let Some(current) = pairs.next() {
                match current.as_rule() {
                    Rule::type_expression_block => {
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
                            Ok(val) => top_level_definitions.push(match val.block_type {
                                ValueExprBlockType::Function => Top::Function(val),
                                ValueExprBlockType::Test => Top::TestCase(val),
                                ValueExprBlockType::Client => Top::Client(val),
                                ValueExprBlockType::RetryPolicy => Top::RetryPolicy(val),
                                ValueExprBlockType::Generator => Top::Generator(val),
                            }),
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
                SchemaAst {
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

    use super::parse_schema;
    use crate::ast::*;
    use baml_types::TypeValue;
    // Add this line to import the ast module
    use internal_baml_diagnostics::SourceFile;

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

        let result = parse_schema(Path::new(root_path), &source);

        assert!(result.is_ok());
        let (schema_ast, _) = result.unwrap();

        assert_eq!(schema_ast.tops.len(), 1);

        match &schema_ast.tops[0] {
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

        let result = parse_schema(Path::new(root_path), &source).unwrap();
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
        let schema = parse_schema(Path::new(root_path), &source).unwrap().0;
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

        let (ast, _) = parse_schema(&Path::new(path), &source).unwrap();

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
}
