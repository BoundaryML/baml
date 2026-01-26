use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::{
    helpers::{assert_correct_parser, parsing_catch_all, Pair},
    Rule,
};
use crate::{
    ast::*,
    parser::{
        parse_assignment::parse_assignment,
        parse_type_expression_block::parse_type_expression_block,
    },
};

pub fn parse_type_builder_block(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<TypeBuilderBlock, DatamodelError> {
    assert_correct_parser(&pair, &[Rule::type_builder_block], diagnostics);

    let span = diagnostics.span(pair.as_span());
    let mut entries = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            // First token is the `type_builder` keyword.
            Rule::TYPE_BUILDER_KEYWORD => {}

            // Second token is opening bracket.
            Rule::BLOCK_OPEN => {}

            // Block content.
            Rule::type_builder_contents => {
                parse_type_builder_contents(current, &mut entries, diagnostics);
            }

            // Last token, closing bracket.
            Rule::BLOCK_CLOSE => {}

            _ => parsing_catch_all(current, "type_builder_block", diagnostics),
        }
    }

    Ok(TypeBuilderBlock { entries, span })
}

pub fn parse_type_builder_contents(
    pair: Pair<'_>,
    entries: &mut Vec<TypeBuilderEntry>,
    diagnostics: &mut Diagnostics,
) {
    assert_correct_parser(&pair, &[Rule::type_builder_contents], diagnostics);

    let mut pending_block_comment = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::comment_block => pending_block_comment = Some(current),

            Rule::dynamic_type_expression_block => {
                let dyn_type_expr_span = diagnostics.span(current.as_span());

                for nested in current.into_inner() {
                    match nested.as_rule() {
                        Rule::identifier => {
                            if nested.as_str() != "dynamic" {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    &format!("Unexpected keyword '{nested}' in dynamic type definition. Use 'dynamic class' or 'dynamic enum'."),
                                    diagnostics.span(nested.as_span()),
                                ));
                            }
                        }

                        Rule::type_expression_block => {
                            let mut type_expr = parse_type_expression_block(
                                nested,
                                pending_block_comment.take(),
                                diagnostics,
                            );

                            // Include the dynamic keyword in the span.
                            type_expr.span = dyn_type_expr_span.to_owned();

                            // TODO: #1343 Temporary solution until we implement scoping in the AST.
                            // We know it's dynamic. The Dynamic subtype will be
                            // removed later because it's not supported in the
                            // AST but we store this information here.
                            type_expr.is_dynamic_type_def = true;

                            match type_expr.sub_type {
                                SubType::Class | SubType::Enum => {
                                    entries.push(TypeBuilderEntry::Dynamic(type_expr))
                                }
                                SubType::Dynamic(_) | SubType::Other(_) => {} // may need to save other somehow for error propagation
                            }
                        }

                        _ => {
                            parsing_catch_all(nested, "dynamic_type_expression_block", diagnostics)
                        }
                    }
                }
            }

            Rule::type_expression_block => {
                let type_expr =
                    parse_type_expression_block(current, pending_block_comment.take(), diagnostics);

                match type_expr.sub_type {
                    SubType::Class => entries.push(TypeBuilderEntry::Class(type_expr)),
                    SubType::Enum => entries.push(TypeBuilderEntry::Enum(type_expr)),
                    SubType::Dynamic(_) | SubType::Other(_) => {} // may need to save other somehow for error propagation
                }
            }

            Rule::type_alias => {
                let assignment = parse_assignment(current, diagnostics);
                entries.push(TypeBuilderEntry::TypeAlias(assignment));
            }

            Rule::BLOCK_LEVEL_CATCH_ALL => {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "Syntax error in type builder block",
                    diagnostics.span(current.as_span()),
                ))
            }

            _ => parsing_catch_all(current, "type_builder_contents", diagnostics),
        }
    }
}

pub fn parse_type_builder_contents_from_str(
    input: &str,
    diagnostics: &mut Diagnostics,
) -> anyhow::Result<Vec<TypeBuilderEntry>> {
    use pest::Parser;

    let pair = crate::parser::BAMLParser::parse(Rule::type_builder_contents, input)?
        .next()
        .unwrap();

    let mut entries = Vec::new();

    parse_type_builder_contents(pair, &mut entries, diagnostics);

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use internal_baml_diagnostics::{Diagnostics, SourceFile};
    use pest::Parser;

    use super::*;
    use crate::parser::{BAMLParser, Rule};

    #[test]
    fn parse_block() {
        let root_path = "test_file.baml";

        let input = r#"type_builder {
            class Example {
                a string
                b int
            }

            enum Bar {
                A
                B
            }

            /// Some doc
            /// comment
            dynamic class Cls {
                e Example
                s string
            }

            dynamic enum Enm {
                C
                D
            }

            type Alias = Example
        }"#;

        let source = SourceFile::new_static(root_path.into(), input);
        let mut diagnostics = Diagnostics::new(root_path.into());

        diagnostics.set_source(&source);

        let parsed = BAMLParser::parse(Rule::type_builder_block, input)
            .unwrap()
            .next()
            .unwrap();

        let type_buider_block = parse_type_builder_block(parsed, &mut diagnostics).unwrap();

        assert_eq!(type_buider_block.entries.len(), 5);

        let TypeBuilderEntry::Class(example) = &type_buider_block.entries[0] else {
            panic!(
                "Expected class Example, got {:?}",
                type_buider_block.entries[0]
            );
        };

        let TypeBuilderEntry::Enum(bar) = &type_buider_block.entries[1] else {
            panic!("Expected enum Bar, got {:?}", type_buider_block.entries[1]);
        };

        let TypeBuilderEntry::Dynamic(cls) = &type_buider_block.entries[2] else {
            panic!(
                "Expected dynamic Cls, got {:?}",
                type_buider_block.entries[2]
            );
        };

        let TypeBuilderEntry::Dynamic(enm) = &type_buider_block.entries[3] else {
            panic!(
                "Expected dynamic Enm, got {:?}",
                type_buider_block.entries[3]
            );
        };

        let TypeBuilderEntry::TypeAlias(alias) = &type_buider_block.entries[4] else {
            panic!(
                "Expected type Alias, got {:?}",
                type_buider_block.entries[4]
            );
        };

        assert_eq!(example.name(), "Example");
        assert_eq!(bar.name(), "Bar");
        assert_eq!(cls.name(), "Cls");
        assert_eq!(cls.documentation(), Some("Some doc\ncomment"));
        assert_eq!(enm.name(), "Enm");
        assert_eq!(alias.name(), "Alias");
    }
}
