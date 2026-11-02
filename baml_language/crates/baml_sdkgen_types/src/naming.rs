//! Host-identifier casing and per-scope name allocation, shared by the
//! generators that honor `naming_convention = "language"`.
//!
//! BAML names stay the wire identity: a generator that respells a name keeps
//! the BAML spelling for the runtime (a field alias, a keyword argument
//! mapping, a string literal in the codec) and only the host-facing binding
//! changes.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use crate::NamingConvention;

/// The casing a host identifier takes under
/// [`NamingConvention::Language`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Case {
    /// `next_cursor`: functions, methods, modules, fields, parameters.
    Snake,
    /// `PageResult`: types, and enum variants where the host requires it.
    UpperCamel,
}

impl Case {
    /// Whether `name` already follows this case, by the rules rustc's
    /// `non_snake_case` and `non_camel_case_types` lints apply. Acronyms are
    /// accepted as written (`LLMClient`, `POSITIVE`), so idiomatic BAML never
    /// changes spelling. Names with characters outside identifiers (`@`)
    /// never qualify.
    pub fn admits(self, name: &str) -> bool {
        if !name.chars().all(|ch| ch == '_' || ch.is_alphanumeric()) {
            return false;
        }
        let core = name.trim_matches('_');
        if core.contains("__") {
            return false;
        }
        match self {
            Self::Snake => !core.chars().any(char::is_uppercase),
            Self::UpperCamel => {
                let chars: Vec<char> = core.chars().collect();
                chars.first().is_none_or(|first| !first.is_lowercase())
                    && !chars.windows(2).any(|pair| {
                        let [left, right] = [pair[0], pair[1]];
                        has_case(left) && right == '_' || left == '_' && has_case(right)
                    })
            }
        }
    }

    /// `name` respelled in this case. A name this case
    /// [admits](Self::admits) is returned unchanged. Otherwise the name is
    /// split into words (at non-alphanumeric characters, lower-to-upper and
    /// digit-to-upper transitions, and the end of an acronym: `HTTPServer` is
    /// `HTTP` + `Server`) and the words are joined in this case. Leading
    /// underscores are kept; a name with no words is returned unchanged.
    pub fn apply(self, name: &str) -> Cow<'_, str> {
        if self.admits(name) {
            return Cow::Borrowed(name);
        }
        let body = name.trim_start_matches('_');
        let prefix = &name[..name.len() - body.len()];
        let words = words(body);
        if words.is_empty() {
            return Cow::Borrowed(name);
        }
        let mut out = String::with_capacity(name.len() + words.len());
        out.push_str(prefix);
        for (index, word) in words.iter().enumerate() {
            match self {
                Self::Snake => {
                    if index > 0 {
                        out.push('_');
                    }
                    out.extend(word.chars().flat_map(char::to_lowercase));
                }
                Self::UpperCamel => {
                    let mut chars = word.chars();
                    if let Some(first) = chars.next() {
                        out.extend(first.to_uppercase());
                        out.extend(chars.flat_map(char::to_lowercase));
                    }
                }
            }
        }
        Cow::Owned(out)
    }
}

impl NamingConvention {
    /// The host spelling of `name` for a binding that takes `case` under
    /// [`NamingConvention::Language`]; `name` itself under
    /// [`NamingConvention::PreserveCase`].
    pub fn spell(self, name: &str, case: Case) -> Cow<'_, str> {
        match self {
            Self::PreserveCase => Cow::Borrowed(name),
            Self::Language => case.apply(name),
        }
    }
}

/// One name requesting a spelling in a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate<K> {
    /// The caller's handle for the allocated spelling.
    pub key: K,
    /// The spelling the name would take with no other name in the scope.
    pub spelling: String,
    /// Whether `spelling` is exactly what the user wrote. Authored spellings
    /// are placed first, so a `next_cursor` keeps its name and a `nextCursor`
    /// that only projects onto it is the one renamed.
    pub authored: bool,
}

/// Give every candidate in one scope a distinct spelling, none of them in
/// `reserved`. A spelling already taken gets trailing underscores until it is
/// free. Authored spellings are placed before projected ones; otherwise the
/// input order decides, so callers pass candidates in a deterministic order.
pub fn allocate_distinct<'r, K: Eq + std::hash::Hash>(
    candidates: Vec<Candidate<K>>,
    reserved: impl IntoIterator<Item = &'r str>,
) -> HashMap<K, String> {
    let mut used: HashSet<String> = reserved.into_iter().map(str::to_string).collect();
    let (authored, projected): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .partition(|candidate| candidate.authored);
    authored
        .into_iter()
        .chain(projected)
        .map(|candidate| {
            let spelling = allocate_one(&candidate.spelling, &mut used);
            (candidate.key, spelling)
        })
        .collect()
}

/// `candidate`, or `candidate` followed by as many underscores as it takes to
/// leave `used`; the result is added to `used`.
pub fn allocate_one(candidate: &str, used: &mut HashSet<String>) -> String {
    let mut spelling = candidate.to_string();
    while used.contains(&spelling) {
        spelling.push('_');
    }
    used.insert(spelling.clone());
    spelling
}

fn has_case(ch: char) -> bool {
    ch.is_lowercase() || ch.is_uppercase()
}

/// The words of `name`: maximal alphanumeric runs, each split before an
/// uppercase letter that follows a lowercase letter or digit (`nextCursor`,
/// `v2Api`) and before the last letter of an acronym followed by lowercase
/// (`HTTPServer`).
fn words(name: &str) -> Vec<&str> {
    let mut words = Vec::new();
    for run in name
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|run| !run.is_empty())
    {
        let chars: Vec<(usize, char)> = run.char_indices().collect();
        let mut start = 0;
        for (index, &(offset, ch)) in chars.iter().enumerate().skip(1) {
            let previous = chars[index - 1].1;
            let next = chars.get(index + 1).map(|&(_, next)| next);
            let boundary = ch.is_uppercase()
                && (previous.is_lowercase()
                    || previous.is_numeric()
                    || previous.is_uppercase() && next.is_some_and(char::is_lowercase));
            if boundary {
                words.push(&run[start..offset]);
                start = offset;
            }
        }
        words.push(&run[start..]);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_respells_only_names_that_need_it() {
        for (raw, expected) in [
            ("next_cursor", "next_cursor"),
            ("nextCursor", "next_cursor"),
            ("NextCursor", "next_cursor"),
            ("deploymentGroups", "deployment_groups"),
            ("HTTPServer", "http_server"),
            ("getURL", "get_url"),
            ("v2Api", "v2_api"),
            ("utf8", "utf8"),
            ("_privateField", "_private_field"),
            ("__dunder", "__dunder"),
            ("a__b", "a_b"),
            ("extract@stream", "extract_stream"),
            ("Extract@stream", "extract_stream"),
            ("ClassifySentiment", "classify_sentiment"),
            ("_", "_"),
        ] {
            assert_eq!(Case::Snake.apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn upper_camel_case_keeps_acronyms_it_admits() {
        for (raw, expected) in [
            ("PageResult", "PageResult"),
            ("page_result", "PageResult"),
            ("pageResult", "PageResult"),
            ("LLMClient", "LLMClient"),
            ("POSITIVE", "POSITIVE"),
            ("NOT_FOUND", "NotFound"),
            ("HTTP_request", "HttpRequest"),
            ("_Hidden", "_Hidden"),
            ("foo_2", "Foo2"),
        ] {
            assert_eq!(Case::UpperCamel.apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn preserve_case_never_respells() {
        assert_eq!(
            NamingConvention::PreserveCase.spell("nextCursor", Case::Snake),
            "nextCursor"
        );
        assert_eq!(
            NamingConvention::Language.spell("nextCursor", Case::Snake),
            "next_cursor"
        );
    }

    #[test]
    fn authored_spelling_wins_a_projected_collision() {
        let allocated = allocate_distinct(
            vec![
                Candidate {
                    key: "nextCursor",
                    spelling: "next_cursor".to_string(),
                    authored: false,
                },
                Candidate {
                    key: "next_cursor",
                    spelling: "next_cursor".to_string(),
                    authored: true,
                },
                Candidate {
                    key: "baml",
                    spelling: "baml".to_string(),
                    authored: true,
                },
            ],
            ["baml"],
        );
        assert_eq!(allocated["next_cursor"], "next_cursor");
        assert_eq!(allocated["nextCursor"], "next_cursor_");
        assert_eq!(allocated["baml"], "baml_");
    }
}
