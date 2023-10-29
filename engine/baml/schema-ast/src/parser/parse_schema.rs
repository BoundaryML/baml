use std::path::PathBuf;

use super::{
    parse_class::parse_class, parse_config, parse_enum::parse_enum, parse_function::parse_function,
    BAMLParser, Rule,
};
use crate::{ast::*, parser::parse_variant};
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
                    Rule::enum_declaration => top_level_definitions.push(Top::Enum(parse_enum(current,pending_block_comment.take(),  &mut diagnostics))),
                    Rule::interface_declaration => {
                        let keyword = current.clone().into_inner().find(|pair| matches!(pair.as_rule(), Rule::CLASS_KEYWORD | Rule::FUNCTION_KEYWORD) ).expect("Expected class keyword");
                        match keyword.as_rule() {
                            Rule::CLASS_KEYWORD => {
                                top_level_definitions.push(Top::Class(parse_class(current, pending_block_comment.take(), &mut diagnostics)));
                            },
                            _ => unreachable!(),
                        };
                    }
                    Rule::function_declaration => {
                        match parse_function(current, pending_block_comment.take(), &mut diagnostics) {
                            Ok(function) => top_level_definitions.push(Top::Function(function)),
                            Err(e) => diagnostics.push_error(e),
                        }
                    },
                    Rule::config_block => {
                        match parse_config::parse_config_block(
                            current,
                            pending_block_comment.take(),
                            &mut diagnostics,
                        ) {
                            Ok(config) => top_level_definitions.push(config),
                            Err(e) => diagnostics.push_error(e),
                        }
                    }
                    Rule::variant_block => {
                        match parse_variant::parse_variant_block(
                            current,
                            pending_block_comment.take(),
                            &mut diagnostics,
                        ) {
                            Ok(config) => top_level_definitions.push(Top::Variant(config)),
                            Err(e) => diagnostics.push_error(e),
                        }
                    }
                    Rule::EOI => {}
                    Rule::CATCH_ALL => diagnostics.push_error(DatamodelError::new_validation_error(
                        "This line is invalid. It does not start with any known Baml schema keyword.",
                        diagnostics.span(current.as_span()),
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
