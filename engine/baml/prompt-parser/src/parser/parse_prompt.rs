use std::path::PathBuf;

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

pub fn parse_prompt(
    root_path: &PathBuf,
    source: &SourceFile,
    prompt: &str,
) -> Result<(String, Diagnostics), Diagnostics> {
    let mut diagnostics = Diagnostics::new(root_path.clone());
    diagnostics.set_source(source);

    let parse_result = BAMLPromptParser::parse(Rule::entry, prompt);

    match parse_result {
        Ok(mut parsed_rules) => {
            let mut top_level_definitions = Vec::new();

            for pair in parsed_rules {
                println!("pair: {:?}", pair.as_str());
                match pair.as_rule() {
                    Rule::code_block => {
                        for current in pair.clone().into_inner() {
                            println!("current: {:?}", current.as_str());
                            match current.as_rule() {
                                Rule::input_block => {
                                    let new_code_block = CodeBlock {
                                        span: diagnostics.span(current.as_span().clone()),
                                        block: current.as_str().to_string(),
                                    };

                                    top_level_definitions.push(new_code_block);
                                }
                                Rule::print_block => {
                                    // parse print block
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                    Rule::comment_block => {
                        // handle comment block
                    }
                    Rule::empty_lines => {
                        // handle empty lines
                    }
                    Rule::prompt_text => {
                        // handle prompt text
                    }
                    _ => unreachable!(),
                }
            }

            Ok((String::new(), diagnostics))
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
    // let mut diagnostics = Diagnostics::new(source.file_path.clone());
    // diagnostics.set_source(source);

    // let prompt_result = BAMLPromptParser::parse_prompt(Rule::prompt, source.as_str());
    // match prompt_result {
    //     Ok(mut prompt_wrapped) => {
    //         let prompt = prompt_wrapped.next().unwrap();

    //         // Run the code with:
    //         // cargo build --features "debug_parser"
    //         #[cfg(feature = "debug_parser")]
    //         pretty_print(prompt.clone(), 0);

    //         let mut pairs = prompt.into_inner().peekable();

    //         let mut prompt = String::new();

    //         while let Some(current) = pairs.next() {
    //             match current.as_rule() {
    //                 Rule::prompt => {
    //                     prompt.push_str(current.as_str());
    //                 }
    //                 Rule::EOI => {}
    //                 _ => unreachable!(),
    //             }
    //         }

    //         Ok((prompt, diagnostics))
    //     }

    // }
}

fn get_expected_from_error(positives: &[Rule]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(positives.len() * 6);

    for positive in positives {
        write!(out, "{positive:?}").unwrap();
    }

    out
}
