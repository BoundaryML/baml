use super::Rule;

pub type Pair<'a> = pest::iterators::Pair<'a, Rule>;

#[track_caller]
pub fn parsing_catch_all(token: &Pair<'_>, kind: &str) {
    match token.as_rule() {
        Rule::empty_lines
        | Rule::trailing_comment
        | Rule::comment_block
        | Rule::block_comment
        | Rule::SPACER_TEXT => {}
        x => unreachable!(
            "Encountered impossible {} during parsing: {:?} {:?}",
            kind,
            &x,
            token.clone().tokens()
        ),
    }
}

#[macro_export]
macro_rules! assert_correct_parser {
    ($pair:expr, $rule:expr) => {
        assert_eq!(
            $pair.as_rule(),
            $rule,
            "Expected {:?}. Got: {:?}.",
            $rule,
            $pair.as_rule()
        );
    };
    ($pair:expr, $($rule:expr),+ ) => {
        let rules = vec![$($rule),+];
        assert!(
            rules.contains(&$pair.as_rule()),
            "Expected one of {:?}. Got: {:?}.",
            rules,
            $pair.as_rule()
        );
    };
}

#[macro_export]
macro_rules! unreachable_rule {
    ($pair:expr, $rule:expr) => {
        unreachable!(
            "Encountered impossible field during parsing {:?}: {:?}",
            $rule,
            $pair.as_rule()
        )
    };
}
