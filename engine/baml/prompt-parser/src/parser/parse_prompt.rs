use std::{fmt::format, path::PathBuf};

use crate::{assert_correct_parser, ast::*, unreachable_rule};
use internal_baml_diagnostics::{DatamodelError, Diagnostics, SourceFile, Span};

use pest::Parser;

use super::{BAMLPromptParser, Rule};

#[cfg(feature = "debug_parser")]
fn pretty_print<'a>(pair: pest::iterators::Pair<'a, Rule>, indent_level: usize) {
    // Indentation for the current level
    let indent = "  ".repeat(indent_level);

    // Print the rule and its span
    println!("{}{:?} -> {:?}", indent, pair.as_rule(), pair.as_str());

    // Recursively print inner pairs with increased indentation
    for innerpair in pair.into_inner() {
        pretty_print(innerpair, indent_level + 1);
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
    (prompt, prompt_span): (&str, Span),
) -> Result<(PromptAst, Diagnostics), Diagnostics> {
    let mut diagnostics = Diagnostics::new(root_path.clone());
    diagnostics.set_source(source);

    let parse_result = BAMLPromptParser::parse(Rule::entry, &prompt);
    let mut top_level_definitions = Vec::new();

    match parse_result {
        Ok(mut parsed_rules) => {
            // The offset for diagnostics is based on where the prompt starts.
            // The prompt itself also includes the leading characters for a raw_string "\"#"
            // TODO (aaronv): pass in the right number here based on the number of prompt.
            diagnostics.set_span_offset(prompt_span.start + 2);

            for pair in parsed_rules.next().unwrap().into_inner() {
                match pair.as_rule() {
                    Rule::whitespaces | Rule::WHITESPACE => {
                        handle_whitespace(pair, &mut top_level_definitions, &diagnostics)
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
                                Rule::prompt_text => handle_prompt_text(
                                    inner,
                                    &mut top_level_definitions,
                                    &diagnostics,
                                ),
                                Rule::dangling_code_block => {
                                    diagnostics.push_error(DatamodelError::new_parser_error(
                                        "{#input..} or {#print_enum(..)} or {#print_type(..)} or {// some comment //}".to_string(),
                                        diagnostics.span(inner.as_span().clone()),
                                    ));
                                }
                                Rule::dangling_comment_block => {
                                    diagnostics.push_error(DatamodelError::new_parser_error(
                                        "Unterminated comment".to_string(),
                                        diagnostics.span(inner.as_span().clone()),
                                    ));
                                }
                                _ => unreachable_rule!(inner, Rule::segment),
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
            diagnostics.push_error(DatamodelError::new_parser_error(
                format!(
                    "Unabled to parse this raw string. Please file a bug.\n{}",
                    err
                ),
                prompt_span.clone(),
            ));
            Err(diagnostics)
        }
    }
}
fn handle_code_block(
    pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &mut Diagnostics,
) {
    assert_correct_parser!(pair, Rule::code_block);

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::variable => handle_variable(current, top_level_definitions, diagnostics),
            Rule::print_block => handle_print_block(current, top_level_definitions, diagnostics),
            _ => unreachable_rule!(current, Rule::code_block),
        }
    }
}

fn handle_variable(
    current: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &mut Diagnostics,
) {
    assert_correct_parser!(current, Rule::variable);

    let span = diagnostics.span(current.as_span());
    let raw_text = current.as_str().to_string();
    let type_path = current
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
        path: type_path,
        text: raw_text.clone(),
        span: span.clone(),
    };
    top_level_definitions.push(Top::CodeBlock(CodeBlock {
        code_type: CodeType::Variable,
        block: raw_text,
        arguments: vec![variable],
        span,
    }));
}

fn handle_print_block(
    current: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &mut Diagnostics,
) {
    assert_correct_parser!(current, Rule::print_block);

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
            Rule::template_args => {
                // TODO: actually read this.
            }
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
            _ => unreachable_rule!(current, Rule::print_block),
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
    pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &Diagnostics,
) {
    assert_correct_parser!(pair, Rule::comment_block);

    // handle comment block
    top_level_definitions.push(Top::CommentBlock(CommentBlock {
        span: diagnostics.span(pair.as_span().clone()),
        block: pair.as_str().to_string(),
    }));
}

fn handle_empty_lines(
    pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &Diagnostics,
) {
    // handle empty lines
    top_level_definitions.push(Top::WhiteSpace(
        pair.as_str().to_string(),
        diagnostics.span(pair.as_span().clone()),
    ));
}

fn handle_prompt_text(
    pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &Diagnostics,
) {
    let content = pair.as_str();
    let trailing_whitespace = content
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .count();

    if trailing_whitespace > 0 && content.len() > trailing_whitespace {
        let span = diagnostics.span(pair.as_span().clone());
        let start = span.start;
        let end = span.end - trailing_whitespace;
        top_level_definitions.push(Top::PromptText(PromptText {
            span: Span::new(span.file.clone(), start, end),
            text: content[..content.len() - trailing_whitespace].to_string(),
        }));
        top_level_definitions.push(Top::WhiteSpace(
            content[content.len() - trailing_whitespace..].to_string(),
            Span::new(span.file, end, span.end),
        ));
    } else if trailing_whitespace > 0 {
        // handle empty lines
        top_level_definitions.push(Top::WhiteSpace(
            content.to_string(),
            diagnostics.span(pair.as_span().clone()),
        ));
    } else {
        // handle prompt text
        top_level_definitions.push(Top::PromptText(PromptText {
            span: diagnostics.span(pair.as_span()),
            text: content.to_string(),
        }));
    }
}

fn handle_whitespace(
    pair: pest::iterators::Pair<'_, Rule>,
    top_level_definitions: &mut Vec<Top>,
    diagnostics: &Diagnostics,
) {
    assert_correct_parser!(pair, Rule::WHITESPACE);

    // handle whitespace
    top_level_definitions.push(Top::WhiteSpace(
        pair.as_str().to_string(),
        diagnostics.span(pair.as_span()),
    ));
}

fn get_expected_from_error(positives: &[Rule]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(positives.len() * 6);

    for positive in positives {
        write!(out, "{:?} ", positive).unwrap();
    }

    out
}
