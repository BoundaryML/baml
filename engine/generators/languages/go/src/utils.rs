pub fn prefix_lines(s: &str, prefix: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    s.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
