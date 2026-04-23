//! Extended glob-to-regex converter supporting Bun-compatible glob patterns.
//!
//! Supports: `*` (any non-separator chars), `**` (any chars including separators),
//! `?` (single non-separator char), `[...]` (character classes), `{a,b}` (alternations),
//! `!` prefix (negation), `\` (escape).

use regex::Regex;

pub(crate) struct GlobPattern {
    re: Regex,
}

impl GlobPattern {
    pub(crate) fn new(pattern: &str) -> Result<Self, String> {
        let re = glob_to_regex(pattern)?;
        Ok(Self { re })
    }

    pub(crate) fn is_match(&self, path: &str) -> bool {
        self.re.is_match(path)
    }
}

fn glob_to_regex(glob: &str) -> Result<Regex, String> {
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
                re.push_str(".*");
                i += 2;
                // Consume trailing slash after `**` so `**/foo` matches `foo` at root too
                if i < bytes.len() && bytes[i] == b'/' {
                    i += 1;
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
                let mut depth = 1usize;
                let mut alts = String::from("(?:");
                i += 1;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'{' => {
                            depth += 1;
                            alts.push('{');
                        }
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            alts.push('}');
                        }
                        b',' if depth == 1 => alts.push('|'),
                        ch => {
                            let c = ch as char;
                            if ".+^${}()|[]\\".contains(c) {
                                alts.push('\\');
                            }
                            alts.push(c);
                        }
                    }
                    i += 1;
                }
                alts.push(')');
                re.push_str(&alts);
                if i < bytes.len() {
                    i += 1; // consume closing '}'
                }
            }
            ch => {
                let c = ch as char;
                if ".+^${}()|[]\\".contains(c) {
                    re.push('\\');
                }
                re.push(c);
                i += 1;
            }
        }
    }
    re.push('$');

    let regex = Regex::new(&re).map_err(|e| format!("Invalid glob pattern '{glob}': {e}"))?;
    if negated {
        let inner = &re[1..re.len() - 1]; // strip leading ^ and trailing $
        let neg_re = format!("^(?!{inner}).*$");
        Regex::new(&neg_re)
            .map_err(|e| format!("Invalid negated glob pattern '{glob}': {e}"))
    } else {
        Ok(regex)
    }
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
}
