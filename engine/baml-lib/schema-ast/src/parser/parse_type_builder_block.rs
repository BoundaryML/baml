use super::{
    helpers::{parsing_catch_all, Pair},
    Rule,
};

use crate::{
    assert_correct_parser,
    ast::*,
    parser::{
        parse_assignment::parse_assignment,
        parse_type_expression_block::parse_type_expression_block,
    },
};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_type_builder_block(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<TypeBuilderBlock, DatamodelError> {
    assert_correct_parser!(pair, Rule::type_builder_block);

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
                let mut pending_block_comment = None;

                for nested in current.into_inner() {
                    match nested.as_rule() {
                        Rule::comment_block => pending_block_comment = Some(nested),

                        Rule::type_expression_block => {
                            let type_expr = parse_type_expression_block(
                                nested,
                                pending_block_comment.take(),
                                diagnostics,
                            );

                            match type_expr.sub_type {
                                SubType::Class => entries.push(TypeBuilderEntry::Class(type_expr)),
                                SubType::Enum => entries.push(TypeBuilderEntry::Enum(type_expr)),
                                SubType::Dynamic => {
                                    entries.push(TypeBuilderEntry::Dynamic(type_expr))
                                }
                                _ => {} // may need to save other somehow for error propagation
                            }
                        }

                        Rule::type_alias => {
                            let assignment = parse_assignment(nested, diagnostics);
                            entries.push(TypeBuilderEntry::TypeAlias(assignment));
                        }

                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "Syntax error in type builder block",
                                diagnostics.span(nested.as_span()),
                            ))
                        }

                        _ => parsing_catch_all(nested, "type_builder_contents"),
                    }
                }
            }

            // Last token, closing bracket.
            Rule::BLOCK_CLOSE => {}

            _ => parsing_catch_all(current, "type_builder_block"),
        }
    }

    Ok(TypeBuilderBlock { entries, span })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{BAMLParser, Rule};
    use internal_baml_diagnostics::{Diagnostics, SourceFile};
    use pest::Parser;

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
            dynamic Cls {
                e Example
                s string
            }

            dynamic Enm {
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
