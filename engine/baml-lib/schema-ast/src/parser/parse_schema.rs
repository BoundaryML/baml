use std::path::PathBuf;

use super::{
    parse_template_string::parse_template_string,
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
    root_path: &PathBuf,
    source: &SourceFile,
) -> Result<(SchemaAst, Diagnostics), Diagnostics> {
    let mut diagnostics = Diagnostics::new(root_path.clone());
    diagnostics.set_source(source);

    if !source.path().ends_with(".baml") {
        diagnostics.push_error(DatamodelError::new_validation_error(
            "A BAML file must have the file extension `.baml`",
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
                            Ok(val) => {
                                if let Some(top) = match val.block_type {
                                    ValueExprBlockType::Function => Some(Top::Function(val)),
                                    ValueExprBlockType::Test => Some(Top::TestCase(val)),
                                    ValueExprBlockType::Client => Some(Top::Client(val)),
                                    ValueExprBlockType::RetryPolicy => Some(Top::RetryPolicy(val)),
                                    ValueExprBlockType::Generator => Some(Top::Generator(val)),
                                } {
                                    top_level_definitions.push(top);
                                }
                            }
                            Err(e) => diagnostics.push_error(e),
                        }
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
                            _ => (),
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

#[cfg(test)]
mod tests {

    use super::parse_schema;
    use crate::ast::*; // Add this line to import the ast module
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

        let result = parse_schema(&root_path.into(), &source);

        assert!(result.is_ok());
        let (schema_ast, _) = result.unwrap();

        assert_eq!(schema_ast.tops.len(), 1);

        match &schema_ast.tops[0] {
            Top::Class(model) => {
                assert_eq!(model.name.name(), "MyClass");
                assert_eq!(model.fields.len(), 2);
                assert_eq!(model.fields[0].name.name(), "myProperty");
                assert_eq!(model.fields[0].attributes.len(), 2)
            }
            _ => panic!("Expected a model declaration"),
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
