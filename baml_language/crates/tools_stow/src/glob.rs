//! The only pattern language stow has: `*` matches any run of characters.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub(crate) struct Glob(String);

impl Glob {
    pub(crate) fn matches(&self, text: &str) -> bool {
        let mut literals = self.0.split('*');
        let first = literals.next().unwrap_or_default();
        let Some(mut rest) = text.strip_prefix(first) else {
            return false;
        };
        let Some(last) = literals.next_back() else {
            return rest.is_empty();
        };
        // Taking the leftmost match of each middle literal leaves the longest
        // possible remainder for the final literal to end.
        for literal in literals {
            match rest.find(literal) {
                Some(at) => rest = &rest[at + literal.len()..],
                None => return false,
            }
        }
        rest.ends_with(last)
    }
}
