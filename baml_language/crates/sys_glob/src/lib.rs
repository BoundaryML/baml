//! Bun-compatible glob pattern matcher shared by `sys_native` and `bridge_wasm`.
//!
//! Patterns and paths use a portable, slash-separated grammar on every platform.
//! Windows paths must therefore be written as `C:/...` or `//server/share/...`;
//! backslash remains the glob escape character rather than a path separator. `Glob.scan`
//! converts native walker entries at its boundary, while `Glob.matches` expects callers
//! to supply this portable form directly.
//!
//! Supports: `*` (any non-separator chars), `**` (any chars including separators),
//! `?` (single non-separator char), `[...]` (character classes), `{a,b}` (alternations),
//! `!` prefix (negation), `\` (escape).

use regex::Regex;

pub struct GlobPattern {
    re: Regex,
    negated: bool,
    target: MatchTarget,
}

/// Which path representation a glob pattern is meant to be tested against.
///
/// Decided once at compile time from the pattern's leading characters. Picking
/// a canonical form avoids the false positives an OR-against-three-forms
/// strategy produces: e.g. `.*` would match every file via the `./<name>`
/// form, even files that don't actually start with a dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchTarget {
    /// Plain pattern (`foo.txt`, `**/*.ts`) — match against the entry's
    /// path relative to the scan root (e.g. `src/foo.ts`).
    Relative,
    /// `./`-prefixed pattern (`./*.txt`) — match against `./<rel>`.
    DotRelative,
    /// Absolute pattern (`/abs/*.ts`) — match against the entry's absolute
    /// path.
    Absolute,
}

impl GlobPattern {
    pub fn new(pattern: &str) -> Result<Self, String> {
        // Negation is `!` followed by an inner pattern; classify by the inner
        // pattern's shape, not the leading `!`.
        let inner = pattern.strip_prefix('!').unwrap_or(pattern);
        let target = if is_absolute(inner) {
            MatchTarget::Absolute
        } else if inner.starts_with("./") {
            MatchTarget::DotRelative
        } else {
            MatchTarget::Relative
        };
        let (re, negated) = glob_to_regex(pattern)?;
        Ok(Self {
            re,
            negated,
            target,
        })
    }

    pub fn target(&self) -> MatchTarget {
        self.target
    }

    /// Test one portable, slash-separated path against the compiled pattern.
    /// Used by `Glob.matches(path)`; callers must convert native separators at
    /// their filesystem boundary before calling this method.
    pub fn is_match(&self, path: &str) -> bool {
        let matched = self.re.is_match(path);
        if self.negated { !matched } else { matched }
    }

    /// Test a directory-walk entry, picking the canonical path form based on
    /// the pattern's shape. Used by `Glob.scan` so callers don't have to
    /// reimplement the form-selection rule.
    pub fn is_match_entry(&self, rel: &str, abs: &str) -> bool {
        let dot_rel;
        let path = match self.target {
            MatchTarget::Relative => rel,
            MatchTarget::DotRelative => {
                dot_rel = format!("./{rel}");
                &dot_rel
            }
            MatchTarget::Absolute => abs,
        };
        let matched = self.re.is_match(path);
        if self.negated { !matched } else { matched }
    }
}

/// Whether a glob pattern is rooted at the filesystem root.
///
/// POSIX absolute paths start with `/`. Windows absolute paths use a
/// drive-letter prefix like `C:/...` — `sys_native::Glob.scan` normalizes
/// backslashes to forward slashes before matching, so we only need to handle
/// the forward-slash form here. Without this, a Windows pattern like
/// `C:/foo/**/*.txt` would be classified as `Relative` and tested against the
/// entry's relative path, never matching anything.
fn is_absolute(pattern: &str) -> bool {
    if pattern.starts_with('/') {
        return true;
    }
    let b = pattern.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/'
}

fn glob_to_regex(glob: &str) -> Result<(Regex, bool), String> {
    let (negated, glob) = if let Some(rest) = glob.strip_prefix('!') {
        (true, rest)
    } else {
        (false, glob)
    };

    let mut re = String::from("^");
    let bytes = glob.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                let ch = bytes[i + 1] as char;
                re.push_str(&regex::escape(&ch.to_string()));
                i += 2;
            }
            b'*' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                // Consume trailing slash after `**` so `**/foo` matches `foo` at root too
                if i < bytes.len() && bytes[i] == b'/' {
                    re.push_str("(?:.*/)?");
                    i += 1;
                } else {
                    re.push_str(".*");
                }
            }
            b'*' => {
                re.push_str("[^/]*");
                i += 1;
            }
            b'?' => {
                re.push_str("[^/]");
                i += 1;
            }
            b'[' => {
                let start = i;
                i += 1;
                let mut class = String::from("[");
                if i < bytes.len() && (bytes[i] == b'^' || bytes[i] == b'!') {
                    class.push('^');
                    i += 1;
                }
                // Allow ] as first char in class
                if i < bytes.len() && bytes[i] == b']' {
                    class.push(']');
                    i += 1;
                }
                while i < bytes.len() && bytes[i] != b']' {
                    class.push(bytes[i] as char);
                    i += 1;
                }
                if i < bytes.len() {
                    class.push(']');
                    i += 1;
                    re.push_str(&class);
                } else {
                    // Unterminated bracket — treat as literal
                    re.push_str(&regex::escape(
                        String::from_utf8_lossy(&bytes[start..]).as_ref(),
                    ));
                }
            }
            b'{' => {
                if let Some((alts, next)) = parse_brace_alternation(bytes, i + 1) {
                    re.push_str(&alts);
                    i = next;
                } else {
                    re.push_str("\\{");
                    i += 1;
                }
            }
            ch => {
                let c = ch as char;
                push_regex_literal(&mut re, c);
                i += 1;
            }
        }
    }
    re.push('$');

    let regex = Regex::new(&re).map_err(|e| format!("Invalid glob pattern '{glob}': {e}"))?;
    Ok((regex, negated))
}

fn parse_brace_alternation(bytes: &[u8], mut i: usize) -> Option<(String, usize)> {
    let mut alts = String::from("(?:");
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                push_regex_literal(&mut alts, bytes[i + 1] as char);
                i += 2;
            }
            b'{' => {
                let (nested, next) = parse_brace_alternation(bytes, i + 1)?;
                alts.push_str(&nested);
                i = next;
            }
            b'}' => {
                alts.push(')');
                return Some((alts, i + 1));
            }
            b',' => {
                alts.push('|');
                i += 1;
            }
            ch => {
                push_regex_literal(&mut alts, ch as char);
                i += 1;
            }
        }
    }
    None
}

fn push_regex_literal(re: &mut String, c: char) {
    if ".+^${}()|[]\\".contains(c) {
        re.push('\\');
    }
    re.push(c);
}

#[cfg(test)]
mod tests {
    use super::GlobPattern;

    #[test]
    fn star_matches_file() {
        let g = GlobPattern::new("*.txt").unwrap();
        assert!(g.is_match("hello.txt"));
        assert!(!g.is_match("hello.rs"));
        assert!(!g.is_match("dir/hello.txt")); // * doesn't cross separators
    }

    #[test]
    fn double_star_crosses_separator() {
        let g = GlobPattern::new("**/*.ts").unwrap();
        assert!(g.is_match("src/index.ts"));
        assert!(g.is_match("src/deep/file.ts"));
        assert!(!g.is_match("src/index.rs"));
    }

    #[test]
    fn double_star_slash_preserves_separator_boundary() {
        let g = GlobPattern::new("foo/**/bar").unwrap();
        assert!(g.is_match("foo/bar"));
        assert!(g.is_match("foo/a/bar"));
        assert!(g.is_match("foo/a/b/bar"));
        assert!(!g.is_match("foo/xbar"));
    }

    #[test]
    fn question_mark() {
        let g = GlobPattern::new("file?.txt").unwrap();
        assert!(g.is_match("fileA.txt"));
        assert!(!g.is_match("file.txt"));
        assert!(!g.is_match("fileAB.txt"));
    }

    #[test]
    fn alternation() {
        let g = GlobPattern::new("*.{ts,tsx}").unwrap();
        assert!(g.is_match("app.ts"));
        assert!(g.is_match("app.tsx"));
        assert!(!g.is_match("app.js"));
    }

    #[test]
    fn nested_alternation() {
        let g = GlobPattern::new("{a,{b,c}}.txt").unwrap();
        assert!(g.is_match("a.txt"));
        assert!(g.is_match("b.txt"));
        assert!(g.is_match("c.txt"));
        assert!(!g.is_match("d.txt"));
    }

    #[test]
    fn negation() {
        let g = GlobPattern::new("!index.ts").unwrap();
        assert!(g.is_match("main.ts"));
        assert!(!g.is_match("index.ts"));
    }

    #[test]
    fn classify_target_by_pattern_shape() {
        use super::MatchTarget;
        assert_eq!(
            GlobPattern::new("*.ts").unwrap().target(),
            MatchTarget::Relative
        );
        assert_eq!(
            GlobPattern::new("foo/bar.ts").unwrap().target(),
            MatchTarget::Relative
        );
        assert_eq!(
            GlobPattern::new("./*.ts").unwrap().target(),
            MatchTarget::DotRelative
        );
        assert_eq!(
            GlobPattern::new("/abs/*.ts").unwrap().target(),
            MatchTarget::Absolute
        );
        // Negation classifies by the inner pattern's shape.
        assert_eq!(
            GlobPattern::new("!*.ts").unwrap().target(),
            MatchTarget::Relative
        );
        assert_eq!(
            GlobPattern::new("!./*.ts").unwrap().target(),
            MatchTarget::DotRelative
        );
        assert_eq!(
            GlobPattern::new("!/abs/*.ts").unwrap().target(),
            MatchTarget::Absolute
        );
        // Windows drive-letter prefixed paths are absolute too. The scan
        // walker normalizes backslashes to forward slashes before matching,
        // so we only need the forward-slash form here.
        assert_eq!(
            GlobPattern::new("C:/Users/foo/**/*.txt").unwrap().target(),
            MatchTarget::Absolute
        );
        assert_eq!(
            GlobPattern::new("z:/tmp/file").unwrap().target(),
            MatchTarget::Absolute
        );
        assert_eq!(
            GlobPattern::new("!C:/abs/*.ts").unwrap().target(),
            MatchTarget::Absolute
        );
        // Bare `C:` (no slash) or `:foo` are not absolute.
        assert_eq!(
            GlobPattern::new("C:foo").unwrap().target(),
            MatchTarget::Relative
        );
        assert_eq!(
            GlobPattern::new("1:/foo").unwrap().target(),
            MatchTarget::Relative
        );
    }

    #[test]
    fn match_entry_absolute_handles_windows_drive_letter() {
        let g = GlobPattern::new("C:/scan/**/*.txt").unwrap();
        assert!(g.is_match_entry("file.txt", "C:/scan/file.txt"));
        assert!(g.is_match_entry("sub/file.txt", "C:/scan/sub/file.txt"));
        assert!(!g.is_match_entry("file.rs", "C:/scan/file.rs"));
    }

    #[test]
    fn absolute_unc_patterns_use_portable_slash_form() {
        let g = GlobPattern::new("//server/share/**/*.baml").unwrap();
        assert_eq!(g.target(), super::MatchTarget::Absolute);
        assert!(g.is_match_entry("project/main.baml", "//server/share/project/main.baml"));
        assert!(!g.is_match_entry("project/main.baml", "//other/share/project/main.baml"));
    }

    #[test]
    fn backslash_is_an_escape_not_a_native_separator() {
        let g = GlobPattern::new(r"dir\*.baml").unwrap();
        assert!(g.is_match("dir*.baml"));
        assert!(!g.is_match("dir/file.baml"));
        assert!(!g.is_match(r"dir\file.baml"));
    }
    #[test]
    fn match_entry_picks_relative_form_for_plain_pattern() {
        // Pattern `.*` (literal dot, then `*`) under the OR-against-three-forms
        // strategy would falsely match every file via the `./<rel>` form.
        // With canonical-by-shape, plain patterns match against rel only.
        let g = GlobPattern::new(".*").unwrap();
        assert!(!g.is_match_entry("regular.txt", "/scan/regular.txt"));
        assert!(g.is_match_entry(".hidden", "/scan/.hidden"));
    }

    #[test]
    fn match_entry_dot_relative_uses_dot_form() {
        let g = GlobPattern::new("./*.txt").unwrap();
        // Pattern wants to match `./<name>`, so against rel `foo.txt` it must
        // hit the `./foo.txt` form internally.
        assert!(g.is_match_entry("foo.txt", "/scan/foo.txt"));
        assert!(!g.is_match_entry("foo.rs", "/scan/foo.rs"));
    }

    #[test]
    fn match_entry_absolute_uses_abs_form() {
        let g = GlobPattern::new("/scan/*.txt").unwrap();
        assert!(g.is_match_entry("foo.txt", "/scan/foo.txt"));
        assert!(!g.is_match_entry("foo.txt", "/other/foo.txt"));
    }
}
