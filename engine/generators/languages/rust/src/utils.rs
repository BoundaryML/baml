pub fn prefix_lines(s: &str, prefix: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    s.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Escape Rust keywords by prefixing with r#
pub fn escape_keyword(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];

    if KEYWORDS.contains(&name) {
        format!("r#{}", name)
    } else {
        name.to_string()
    }
}
