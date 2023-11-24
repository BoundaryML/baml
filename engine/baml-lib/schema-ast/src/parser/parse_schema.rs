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

fn parse_test_from_json(
    source: &SourceFile,
    diagnostics: &mut Diagnostics,
) -> Result<SchemaAst, Diagnostics> {
    // Path relative to the root of the project.
    let source_path = source.path_buf().clone();
    let root_path = diagnostics.root_path.clone();
    let relative_path = source_path.strip_prefix(&root_path).unwrap();

    let parts = relative_path.components();
    // Ensure is of the form `tests/<function_name>/(<group_name>/)/<test_name>.json` using regex
    // or throw an error.
    let mut function_name = None;
    let mut test_name = None;
    let mut group_name = None;
    for (idx, part) in parts.enumerate() {
        let part = part.as_os_str().to_str().unwrap();
        match idx {
            0 => {
                if part != "__tests" {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "A BAML test file must be in a `__tests` directory.",
                        Span::empty(source.clone()),
                    ));
                }
            }
            1 => {
                function_name = Some(part);
            }
            _ => {
                if part.ends_with(".json") {
                    test_name = Some(
                        part.strip_suffix(".json")
                            .unwrap()
                            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_"),
                    );
                } else {
                    group_name = match group_name {
                        None => Some(part.to_string()),
                        Some(prev) => Some(format!("{}_{}", prev, part)),
                    }
                }
            }
        }
    }

    if function_name.is_none() {
        diagnostics.push_error(DatamodelError::new_validation_error(
            "Missing a function name in the path.",
            Span::empty(source.clone()),
        ));
    }

    if test_name.is_none() {
        diagnostics.push_error(DatamodelError::new_validation_error(
            "Test file must have a name",
            Span::empty(source.clone()),
        ));
    }

    diagnostics.to_result()?;

    let function_name = function_name.unwrap();
    let test_name = test_name.unwrap();

    let content = source.as_str();
    let span = Span::new(source.clone(), 0, content.len());
    let content = Expression::RawStringValue(RawString::new(
        content.to_string(),
        Span::new(source.clone(), 0, content.len()),
        Some(("json".into(), Span::empty(source.clone()))),
    ));
    let test_case = ConfigBlockProperty {
        name: Identifier::Local("input".into(), span.clone()),
        value: Some(content),
        template_args: None,
        attributes: vec![],
        documentation: None,
        span: span.clone(),
    };
    let function_name = ConfigBlockProperty {
        name: Identifier::Local("function".into(), span.clone()),
        value: Some(Expression::StringValue(function_name.into(), span.clone())),
        template_args: None,
        attributes: vec![],
        documentation: None,
        span: span.clone(),
    };
    let mut top = RetryPolicyConfig {
        name: Identifier::Local(test_name.into(), span.clone()),
        documentation: None,
        attributes: vec![],
        fields: vec![test_case, function_name],
        span: span.clone(),
    };
    if let Some(group_name) = group_name {
        top.fields.push(ConfigBlockProperty {
            name: Identifier::Local("group".into(), span.clone()),
            value: Some(Expression::StringValue(group_name.into(), span.clone())),
            template_args: None,
            attributes: vec![],
            documentation: None,
            span: span.clone(),
        });
    }
    Ok(SchemaAst {
        tops: vec![Top::Config(Configuration::TestCase(top))],
    })
}

/// Parse a PSL string and return its AST.
/// It validates some basic things on the AST like name conflicts. Further validation is in baml-core
pub fn parse_schema(
    root_path: &PathBuf,
    source: &SourceFile,
) -> Result<(SchemaAst, Diagnostics), Diagnostics> {
    let mut diagnostics = Diagnostics::new(root_path.clone());
    diagnostics.set_source(source);

    if !source.path().ends_with(".json") && !source.path().ends_with(".baml") {
        diagnostics.push_error(DatamodelError::new_validation_error(
            "A BAML file must have the file extension `.baml` or `.json`.",
            Span::empty(source.clone()),
        ));
        return Err(diagnostics);
    }

    if source.path().ends_with(".json") {
        return parse_test_from_json(source, &mut diagnostics).map(|ast| (ast, diagnostics));
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
                    // We do nothing here.
                    Rule::raw_string_literal => (),
                    Rule::arbitrary_block => diagnostics.push_error(DatamodelError::new_validation_error(
                        "This block is invalid. It does not start with any known BAML keyword. Common keywords include 'enum', 'class', 'function', and 'impl'.",
                        diagnostics.span(current.as_span()),
                    )),
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
