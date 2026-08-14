/// A single layer of `baml test` selectors.
///
/// Selectors are matched against the complete canonical test id (for example
/// `root.payments::integration::declined_card`). A selector without `*` is a
/// substring filter; a selector containing `*` is an anchored glob. Repeated
/// includes are ORed, repeated excludes are ORed, and exclusions win.
#[derive(Debug, Clone, Default)]
pub struct TestFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// Selector match against a canonical test id.
///
/// Plain selectors are case-sensitive substring filters. A selector containing
/// `*` is instead an anchored glob where `*` matches any run of Unicode
/// characters, including `::`, and every other character is literal.
pub(crate) fn glob_match(subject: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return subject.contains(pattern);
    }
    let first = parts[0];
    let last = parts[parts.len() - 1];
    if !subject.starts_with(first) || !subject.ends_with(last) {
        return false;
    }
    let chars: Vec<char> = subject.chars().collect();
    let mut pos = first.chars().count();
    let end_limit = chars.len() - last.chars().count();
    if pos > end_limit {
        return false;
    }
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        let window: String = chars[pos..end_limit].iter().collect();
        match window.find(part) {
            Some(byte_idx) => {
                pos += window[..byte_idx].chars().count() + part.chars().count();
            }
            None => return false,
        }
    }
    true
}

impl TestFilter {
    pub fn new<'a>(
        include: impl Iterator<Item = &'a str>,
        exclude: impl Iterator<Item = &'a str>,
    ) -> Self {
        Self {
            include: include.map(str::to_owned).collect(),
            exclude: exclude.map(str::to_owned).collect(),
        }
    }

    pub fn includes_id(&self, canonical_id: &str) -> bool {
        Self::includes_patterns(canonical_id, &self.include, &self.exclude)
    }

    pub(crate) fn includes_patterns(
        canonical_id: &str,
        include: &[String],
        exclude: &[String],
    ) -> bool {
        if exclude.iter().any(|p| glob_match(canonical_id, p)) {
            return false;
        }
        include.is_empty() || include.iter().any(|p| glob_match(canonical_id, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(include: &[&str], exclude: &[&str]) -> TestFilter {
        TestFilter::new(include.iter().copied(), exclude.iter().copied())
    }

    #[test]
    fn plain_selectors_are_substrings_and_explicit_globs_are_anchored() {
        let id = "root.payments::integration::declined_card";
        assert!(filter(&["payments"], &[]).includes_id(id));
        assert!(filter(&["declined_card"], &[]).includes_id(id));
        assert!(filter(&["root.payments::*"], &[]).includes_id(id));
        assert!(filter(&["*::integration::*"], &[]).includes_id(id));
        assert!(filter(&["*declined*"], &[]).includes_id(id));
        assert!(!filter(&["payments::*"], &[]).includes_id(id));
        assert!(!filter(&["root.orders::*"], &[]).includes_id(id));
    }

    #[test]
    fn excludes_win_and_repeated_includes_are_or() {
        let f = filter(
            &["root.payments::*", "root.orders::*"],
            &["*::integration::*"],
        );
        assert!(!f.includes_id("root.payments::integration::charge"));
        assert!(f.includes_id("root.orders::unit::parse"));
    }

    #[test]
    fn no_includes_selects_everything_not_excluded() {
        let f = filter(&[], &["*::slow::*"]);
        assert!(f.includes_id("root::unit::fast"));
        assert!(!f.includes_id("root::slow::case"));
    }

    #[test]
    fn slash_unicode_punctuation_and_consecutive_stars_are_literal_or_wildcard_as_documented() {
        let id = "root.orders::path/to::café?[x]";
        assert!(filter(&["path/to::café?[x]"], &[]).includes_id(id));
        assert!(filter(&["*::path/to::café?[x]"], &[]).includes_id(id));
        assert!(filter(&["root**::path/to::*"], &[]).includes_id(id));
        assert!(!filter(&["*::pathXto::*"], &[]).includes_id(id));
        assert!(!filter(&["*::path/to::*"], &["*café?[x]"]).includes_id(id));
    }
}
