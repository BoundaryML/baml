pub fn prefix_lines(s: &str, prefix: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    s.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Escape Rust keywords by prefixing with `r#`. The path keywords `crate`, `self`,
/// `Self` and `super` are rejected by rustc as raw identifiers, so they get a
/// trailing underscore instead.
pub fn escape_keyword(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];

    if matches!(name, "crate" | "self" | "Self" | "super") {
        format!("{}_", name)
    } else if KEYWORDS.contains(&name) {
        format!("r#{}", name)
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::escape_keyword;

    #[test]
    fn raw_escapes_ordinary_keywords() {
        assert_eq!(escape_keyword("type"), "r#type");
        assert_eq!(escape_keyword("match"), "r#match");
        assert_eq!(escape_keyword("name"), "name");
    }

    #[test]
    fn path_keywords_cannot_be_raw_identifiers() {
        for kw in ["crate", "self", "Self", "super"] {
            assert_eq!(escape_keyword(kw), format!("{kw}_"));
        }
    }
}
