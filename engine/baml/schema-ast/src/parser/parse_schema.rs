use super::{
    parse_class::parse_class, parse_client_generator_variant, parse_enum::parse_enum,
    parse_function::parse_function, BAMLParser, Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};
use pest::Parser;

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
pub fn parse_schema(datamodel_string: &str, diagnostics: &mut Diagnostics) -> SchemaAst {
    let datamodel_result = BAMLParser::parse(Rule::schema, datamodel_string);

    match datamodel_result {
        Ok(mut datamodel_wrapped) => {
            let datamodel = datamodel_wrapped.next().unwrap();

            // Run the code with:
            // cargo build --features "debug_parser"
            #[cfg(feature = "debug_parser")]
            pretty_print(datamodel.clone(), 0);

            let mut top_level_definitions: Vec<Top> = vec![];
            let mut pending_block_comment = None;
            let mut pairs = datamodel.into_inner().peekable();

            while let Some(current) = pairs.next() {
                match current.as_rule() {
                    Rule::enum_declaration => top_level_definitions.push(Top::Enum(parse_enum(current,pending_block_comment.take(),  diagnostics))),
                    Rule::interface_declaration => {
                        let keyword = current.clone().into_inner().find(|pair| matches!(pair.as_rule(), Rule::CLASS_KEYWORD | Rule::FUNCTION_KEYWORD) ).expect("Expected class keyword");
                        match keyword.as_rule() {
                            Rule::CLASS_KEYWORD => {
                                top_level_definitions.push(Top::Class(parse_class(current, pending_block_comment.take(), diagnostics)));
                            },
                            Rule::FUNCTION_KEYWORD => {
                                match parse_function(current, pending_block_comment.take(), diagnostics) {
                                    Ok(function) => top_level_definitions.push(Top::Function(function)),
                                    Err(e) => diagnostics.push_error(e),
                                }
                            },
                            _ => unreachable!(),
                        };
                    }
                    Rule::config_block => {
                        match parse_client_generator_variant::parse_config_block(
                            current,
                            pending_block_comment.take(),
                            diagnostics,
                        ) {
                            Ok(config) => top_level_definitions.push(config),
                            Err(e) => diagnostics.push_error(e),
                        }
                    }
                    Rule::EOI => {}
                    Rule::CATCH_ALL => diagnostics.push_error(DatamodelError::new_validation_error(
                        "This line is invalid. It does not start with any known Prisma schema keyword.",
                        current.as_span().into(),
                    )),
                    Rule::comment_block => {
                        match pairs.peek().map(|b| b.as_rule()) {
                            Some(Rule::empty_lines) => {
                                // free floating
                            }
                            Some(Rule::enum_declaration) => {
                                pending_block_comment = Some(current);
                            }
                            _ => (),
                        }
                    },
                    // TODO: Add view when we want it to be more visible as a feature.
                    // Rule::arbitrary_block => diagnostics.push_error(DatamodelError::new_validation_error(
                    //     "This block is invalid. It does not start with any known Prisma schema keyword. Valid keywords include \'enum\', \'type\', \'datasource\' and \'generator\'.",
                    //     current.as_span().into(),
                    // )),
                    Rule::empty_lines => (),
                    _ => unreachable!(),
                }
            }

            SchemaAst {
                tops: top_level_definitions,
            }
        }
        Err(err) => {
            let location: pest::Span<'_> = match err.location {
                pest::error::InputLocation::Pos(pos) => {
                    pest::Span::new(datamodel_string, pos, pos).unwrap()
                }
                pest::error::InputLocation::Span((from, to)) => {
                    pest::Span::new(datamodel_string, from, to).unwrap()
                }
            };

            let expected = match err.variant {
                pest::error::ErrorVariant::ParsingError { positives, .. } => {
                    get_expected_from_error(&positives)
                }
                _ => panic!("Could not construct parsing error. This should never happend."),
            };

            diagnostics.push_error(DatamodelError::new_parser_error(expected, location.into()));

            SchemaAst { tops: Vec::new() }
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
