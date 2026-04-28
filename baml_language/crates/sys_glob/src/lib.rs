//! Bun-compatible glob pattern matcher shared by `sys_native` and `bridge_wasm`.
//!
//! Supports: `*` (any non-separator chars), `**` (any chars including separators),
//! `?` (single non-separator char), `[...]` (character classes), `{a,b}` (alternations),
//! `!` prefix (negation), `\` (escape).

use regex::Regex;

pub struct GlobPattern {
    re: Regex,
    negated: bool,
}

impl GlobPattern {
    pub fn new(pattern: &str) -> Result<Self, String> {
        let (re, negated) = glob_to_regex(pattern)?;
        Ok(Self { re, negated })
    }

    pub fn is_match(&self, path: &str) -> bool {
        self.is_match_any([path])
    }

    pub fn is_match_any<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> bool {
        let matched = paths.into_iter().any(|path| self.re.is_match(path));
        if self.negated { !matched } else { matched }
    }
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
}
