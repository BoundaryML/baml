use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::Rule;

pub type Pair<'a> = pest::iterators::Pair<'a, Rule>;

#[track_caller]
pub fn parsing_catch_all(token: Pair<'_>, kind: &str, diagnostics: &mut Diagnostics) {
    match token.as_rule() {
        Rule::empty_lines
        | Rule::trailing_comment
        | Rule::comment_block
        | Rule::block_comment
        | Rule::SPACER_TEXT
        | Rule::NEWLINE => {}
        x => {
            let message = format!(
                "Encountered impossible {} during parsing: {:?} {:?}",
                kind,
                &x,
                token.clone().tokens()
            );
            diagnostics.push_error(DatamodelError::new_parser_error(
                message,
                diagnostics.span(token.as_span()),
            ))
        }
    }
}

#[track_caller]
pub fn assert_correct_parser(pair: &Pair<'_>, expected: &[Rule], diagnostics: &mut Diagnostics) {
    if !expected.contains(&pair.as_rule()) {
        let message = if expected.len() == 1 {
            format!("Expected {:?}. Got: {:?}.", expected[0], pair.as_rule())
        } else {
            format!("Expected one of {:?}. Got: {:?}.", expected, pair.as_rule())
        };
        diagnostics.push_error(DatamodelError::new_parser_error(
            message,
            diagnostics.span(pair.as_span()),
        ));
    }
}

#[track_caller]
pub fn unreachable_rule(pair: &Pair<'_>, context: &str, diagnostics: &mut Diagnostics) {
    let message = format!(
        "Encountered impossible field during parsing {:?}: {:?}",
        context,
        pair.as_rule()
    );
    diagnostics.push_error(DatamodelError::new_parser_error(
        message,
        diagnostics.span(pair.as_span()),
    ));
}

#[macro_export]
macro_rules! test_parse_baml_type {
    ( source: $source:expr, target: $target:expr, $(,)* ) => {
        use internal_baml_diagnostics::{Diagnostics, SourceFile};
        use pest::Parser;
        use $crate::parser::{BAMLParser, Rule};

        let root_path = "test_file.baml";
        let source = SourceFile::new_static(root_path.into(), $source);
        let mut diagnostics = Diagnostics::new(root_path.into());
        diagnostics.set_source(&source);

        let parsed = BAMLParser::parse(Rule::field_type_chain, $source)
            .expect("Pest parsing should succeed")
            .next()
            .unwrap();
        let type_ =
            parse_field_type_chain(parsed, &mut diagnostics).expect("Type parsing should succeed");

        type_.assert_eq_up_to_span(&$target);
    };
}
