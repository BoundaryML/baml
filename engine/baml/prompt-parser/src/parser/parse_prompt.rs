use std::{collections::HashMap, path::PathBuf};

use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics, SourceFile, Span};
use log::info;
use pest::Parser;

use super::{BAMLPromptParser, Rule};

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

fn max_leading_whitespace_to_remove(input: &str) -> usize {
    input
        .lines()
        .filter(|line| !line.trim().is_empty()) // Filter out empty lines
        .map(|line| line.chars().take_while(|c| c.is_whitespace()).count()) // Count leading whitespaces for each line
        .min()
        .unwrap_or(0) // Return the minimum count or 0 if there are no lines
}

pub fn parse_prompt(
    root_path: &PathBuf,
    source: &SourceFile,
    prompt_tuple: (String, Span),
) -> Result<(PromptAst, Diagnostics), Diagnostics> {
    let mut diagnostics = Diagnostics::new(root_path.clone());
    diagnostics.set_source(source);

    // remove the first \n that is the first character if it exists
    // also remove the last \n if it exists
    let mut span_offset = 0;
    let prompt = prompt_tuple.0;
    if prompt.starts_with('\n') {
        span_offset = 1;
    }

    // let prompt = prompt.trim_matches('\n');
    span_offset += max_leading_whitespace_to_remove(&prompt);

    span_offset += prompt_tuple.1.start;
    // add 2 more for now to account for the 2 characters in the prompt raw string "\"#". TODO: fix this.
    span_offset += 2;
    diagnostics.set_span_offset(span_offset);
    // now dedent the prompt
    let prompt = prompt.clone(); //textwrap::dedent(prompt);

    let parse_result = BAMLPromptParser::parse(Rule::entry, &prompt);

    let mut top_level_definitions = Vec::new();

    match parse_result {
        Ok(mut parsed_rules) => {
            for pair in parsed_rules.next().unwrap().into_inner() {
                pretty_print(pair.clone(), 0);

                match pair.as_rule() {
                    Rule::WHITESPACE => {
                        handle_prompt_text(pair, &mut top_level_definitions, &diagnostics)
                    }
                    Rule::segment => {
                        for inner in pair.into_inner() {
                            match inner.as_rule() {
                                Rule::code_block => handle_code_block(
                                    inner,
                                    &mut top_level_definitions,
                                    &mut diagnostics,
                                ),
                                Rule::comment_block => handle_comment_block(
                                    inner,
                                    &mut top_level_definitions,
                                    &diagnostics,
                                ),
                                Rule::empty_lines => handle_empty_lines(
                                    inner,
                                    &mut top_level_definitions,
                                    &diagnostics,
                                ),
                                Rule::prompt_text => handle_prompt_text(
                                    inner,
                                    &mut top_level_definitions,
                                    &diagnostics,
                                ),
                                _ => unreachable!(
                                    "Unexpected rule: {:?} {:?}",
                                    inner.as_rule(),
                                    inner.as_str()
                                ),
                            }
                        }
                    }
                    Rule::EOI => {}
                    _ => unreachable!("Unexpected rule: {:?}", pair.as_rule()),
                }
            }

            Ok((
                PromptAst {
                    tops: top_level_definitions,
                },
                diagnostics,
            ))
        }
        Err(err) => {
            let location: Span = match err.location {
                pest::error::InputLocation::Pos(pos) => Span {
                    file: source.clone(),
                    start: pos + span_offset,
                    end: pos + span_offset,
                },
                pest::error::InputLocation::Span((from, to)) => Span {
                    file: source.clone(),
                    start: from + span_offset,
                    end: to + span_offset,
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
fn handle_code_block(
    pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &mut Diagnostics,
) {
    let pair_clone = pair.clone();
    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::variable => handle_variable(current, top_level_definitions, diagnostics),
            Rule::print_block => handle_print_block(current, top_level_definitions, diagnostics),
            _ => unreachable!(),
        }
    }
}

fn handle_variable(
    current: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &mut Diagnostics,
) {
    let type_path = current
        .clone()
        .into_inner()
        .filter_map(|inner| {
            if let Rule::identifier = inner.as_rule() {
                Some(inner.as_str().to_string())
            } else {
                diagnostics.push_error(DatamodelError::new_parser_error(
                    format!("Unexpected rule: {:?}", inner.as_rule()),
                    diagnostics.span(inner.as_span().clone()),
                ));
                None
            }
        })
        .collect::<Vec<_>>();

    let variable = Variable {
        path: type_path.clone(),
        text: current.as_str().to_string(),
        span: diagnostics.span(current.as_span().clone()),
    };
    let new_code_block = CodeBlock {
        code_type: CodeType::Variable,
        block: current.as_str().to_string(),
        arguments: vec![variable],
        span: diagnostics.span(current.as_span().clone()),
    };
    top_level_definitions.push(Top::CodeBlock(new_code_block));
}

fn handle_print_block(
    current: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &mut Diagnostics,
) {
    let code = &current.as_str().to_string();
    let block_span = &diagnostics.span(current.as_span().clone());
    let mut printer_type: Option<CodeType> = None;
    let mut argument: Option<String> = None;

    for current in current.clone().into_inner() {
        match current.as_rule() {
            Rule::identifier => match current.as_str() {
                "_enum" => {
                    printer_type = Some(CodeType::PrintEnum);
                }
                "_type" => {
                    printer_type = Some(CodeType::PrintType);
                }
                _ => {}
            },
            Rule::variable => {
                for current in current.into_inner() {
                    match current.as_rule() {
                        Rule::identifier => {
                            argument = Some(current.as_str().to_string());
                        }
                        _ => diagnostics.push_error(DatamodelError::new_parser_error(
                            "missing argument".to_string(),
                            diagnostics.span(current.as_span().clone()),
                        )),
                    }
                }
            }
            _ => diagnostics.push_error(DatamodelError::new_parser_error(
                "Missing argument or incorrect function. Use print_type(YourClassName)".to_string(),
                diagnostics.span(current.as_span().clone()),
            )),
        }
    }

    if printer_type.is_some() && argument.is_some() {
        let variable = Variable {
            path: vec![argument.clone().unwrap()],
            text: argument.clone().unwrap(),
            span: block_span.clone(),
        };
        let new_code_block = CodeBlock {
            code_type: printer_type.unwrap(),
            block: code.to_string(),
            arguments: vec![variable],
            span: block_span.clone(),
        };
        top_level_definitions.push(Top::CodeBlock(new_code_block));
    } else {
        diagnostics.push_error(DatamodelError::new_parser_error(
            "unknown printer function name. Did you mean print_type or print_enum?".to_string(),
            diagnostics.span(current.clone().as_span()),
        ));
    }
}

fn handle_comment_block(
    _pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &Diagnostics,
) {
    // handle comment block
    top_level_definitions.push(Top::CommentBlock(CommentBlock {
        span: diagnostics.span(_pair.as_span().clone()),
        block: _pair.as_str().to_string(),
    }));
}

fn handle_empty_lines(
    _pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &Diagnostics,
) {
    // handle empty lines
    top_level_definitions.push(Top::PromptText(PromptText {
        text: _pair.as_str().to_string(),
        span: diagnostics.span(_pair.as_span().clone()),
    }))
}

fn handle_prompt_text(
    _pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &Diagnostics,
) {
    // handle prompt text
    top_level_definitions.push(Top::PromptText(PromptText {
        span: diagnostics.span(_pair.as_span().clone()),
        text: _pair.as_str().to_string(),
    }));
}

fn get_expected_from_error(positives: &[Rule]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(positives.len() * 6);

    for positive in positives {
        write!(out, "{:?} ", positive).unwrap();
    }

    out
}
