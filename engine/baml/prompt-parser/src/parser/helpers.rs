use super::Rule;

pub type Pair<'a> = pest::iterators::Pair<'a, Rule>;

#[track_caller]
pub fn parsing_catch_all(token: &Pair<'_>, kind: &str) {
    match token.as_rule() {
        Rule::segment => {}
        x => unreachable!(
            "Encountered impossible {} during parsing: {:?} {:?}",
            kind,
            &x,
            token.clone().tokens()
        ),
    }
}
