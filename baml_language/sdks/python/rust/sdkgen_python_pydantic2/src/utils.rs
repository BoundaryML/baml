//! Helpers shared between the `sdkgen_python_pydantic2` emit modules.

/// Format a string as a Python docstring with proper indentation.
/// The first line starts with `"""`, subsequent lines are indented with
/// `indent`, and the closing `"""` sits on the same line as the last
/// content. Backslashes and `"""` sequences in `s` are escaped so the
/// emitted string is always syntactically valid Python.
///
/// Used for method/function bodies — class and enum docstrings go
/// through `format_class_docstring` instead so that field/variant
/// `///` lines fold into the body as an `Attributes:` / `Members:`
/// section.
pub(crate) fn format_docstring(s: &str, indent: &str) -> String {
    if s.is_empty() {
        return "\"\"\"\"\"\"".to_string();
    }

    let escaped = escape_for_docstring(s);

    let lines: Vec<&str> = escaped.lines().collect();
    if lines.len() == 1 {
        return format!("\"\"\"{}\"\"\"", lines[0]);
    }

    let mut result = String::new();
    result.push_str("\"\"\"");
    result.push_str(lines[0]);
    for line in &lines[1..] {
        result.push('\n');
        result.push_str(indent);
        result.push_str(line);
    }
    result.push_str("\"\"\"");
    result
}

/// Render a class- or enum-level docstring that folds per-field /
/// per-variant `///` lines into a single `Attributes:` / `Members:`
/// section, e.g.:
///
/// ```text
/// """
/// Application configuration.
///
/// Attributes:
///     timeout: Timeout in seconds.
///     retries: Number of retry attempts.
///     debug
/// """
/// ```
///
/// `summary` is the class/enum's own `///` text, if any.
/// `members` is `(name, Option<doc>)` pairs for **every** field /
/// variant in declaration order — `Some(doc)` for those that carry a
/// `///`, `None` for those that don't.
/// `section_label` is `"Attributes"` for classes and `"Members"`
/// for enums.
/// `body_indent` is the indentation prepended to every line *after*
/// the opening `"""` — the caller's template emits the leading
/// indent for the first line itself.
///
/// Section visibility rule: the `Attributes:` / `Members:` section
/// appears iff **at least one** member carries a `///`. When it
/// appears, **all** members are listed — undocumented entries render
/// as a bare `name` (no trailing colon), and documented entries as
/// `name: <doc>`. This way the section, when present, faithfully
/// reflects every public child of the type.
///
/// Returns `None` when there's nothing to render (no summary and no
/// member carries a `///`). Returns a single-line `"""…"""` when
/// only a single-line summary is present and no member carries a
/// `///`. Otherwise returns the block form shown above.
pub(crate) fn format_class_docstring(
    summary: Option<&str>,
    members: &[(String, Option<String>)],
    section_label: &str,
    body_indent: &str,
) -> Option<String> {
    let summary = summary.filter(|s| !s.is_empty());
    let any_member_doc = members.iter().any(|(_, d)| d.is_some());

    if summary.is_none() && !any_member_doc {
        return None;
    }

    if let Some(s) = summary {
        if !s.contains('\n') && !any_member_doc {
            return Some(format!("\"\"\"{}\"\"\"", escape_for_docstring(s)));
        }
    }

    let mut out = String::from("\"\"\"\n");
    if let Some(s) = summary {
        for line in s.lines() {
            out.push_str(body_indent);
            out.push_str(&escape_for_docstring(line));
            out.push('\n');
        }
    }
    if any_member_doc {
        if summary.is_some() {
            out.push('\n');
        }
        out.push_str(body_indent);
        out.push_str(section_label);
        out.push_str(":\n");
        for (name, doc) in members {
            match doc {
                Some(d) => {
                    let mut lines = d.lines();
                    if let Some(first) = lines.next() {
                        out.push_str(body_indent);
                        out.push_str("    ");
                        out.push_str(name);
                        out.push_str(": ");
                        out.push_str(&escape_for_docstring(first));
                        out.push('\n');
                        for line in lines {
                            out.push_str(body_indent);
                            out.push_str("        ");
                            out.push_str(&escape_for_docstring(line));
                            out.push('\n');
                        }
                    } else {
                        // Empty docstring — fall through to bare-name form.
                        out.push_str(body_indent);
                        out.push_str("    ");
                        out.push_str(name);
                        out.push('\n');
                    }
                }
                None => {
                    out.push_str(body_indent);
                    out.push_str("    ");
                    out.push_str(name);
                    out.push('\n');
                }
            }
        }
    }
    out.push_str(body_indent);
    out.push_str("\"\"\"");
    Some(out)
}

/// Compose a function/method docstring body from an optional `///` summary
/// and the unqualified names of its thrown types (32d). Returns the raw
/// docstring *text* (no `"""` fences — the caller wraps it via
/// [`format_docstring`]).
///
/// - no summary, no raises → `None` (omit the docstring entirely)
/// - summary, no raises     → just the summary (unchanged behavior)
/// - no summary, raises      → `"Raises:\n    E1, E2"`
/// - summary + raises        → `"<summary>\n\nRaises:\n    E1, E2"`
///
/// Names are joined `", "` on one indented line; the `Raises:` label follows
/// Google-style docstring convention so `inspect.getdoc` renders cleanly (the
/// summary line keeps the names' leading indent intact under `cleandoc`).
pub(crate) fn build_function_docstring(summary: Option<&str>, raises: &[String]) -> Option<String> {
    let summary = summary.filter(|s| !s.is_empty());
    if raises.is_empty() {
        return summary.map(std::string::ToString::to_string);
    }
    let raises_block = format!("Raises:\n    {}", raises.join(", "));
    Some(match summary {
        Some(s) => format!("{s}\n\n{raises_block}"),
        None => raises_block,
    })
}

/// Escape `\` and `"""` so they don't break the surrounding
/// `"""…"""` fence. `\` first so a later `\"""` substitution isn't
/// itself doubled.
fn escape_for_docstring(s: &str) -> String {
    s.replace('\\', "\\\\").replace("\"\"\"", "\\\"\"\"")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn format_docstring_empty() {
        assert_eq!(format_docstring("", "    "), "\"\"\"\"\"\"");
    }

    #[test]
    fn format_docstring_single_line() {
        assert_eq!(format_docstring("hello", "    "), "\"\"\"hello\"\"\"");
    }

    #[test]
    fn format_docstring_multi_line() {
        assert_eq!(
            format_docstring("a\nb\nc", "    "),
            "\"\"\"a\n    b\n    c\"\"\""
        );
    }

    #[test]
    fn format_docstring_escapes_triple_quote() {
        assert_eq!(
            format_docstring("hello\"\"\"world", "    "),
            "\"\"\"hello\\\"\"\"world\"\"\""
        );
    }

    #[test]
    fn format_docstring_escapes_trailing_backslash() {
        assert_eq!(format_docstring("text\\", "    "), "\"\"\"text\\\\\"\"\"");
    }

    #[test]
    fn format_class_docstring_none_when_empty() {
        assert_eq!(
            format_class_docstring(None, &[], "Attributes", "    "),
            None
        );
        assert_eq!(
            format_class_docstring(Some(""), &[], "Attributes", "    "),
            None
        );
    }

    #[test]
    fn format_class_docstring_single_line_summary_only() {
        assert_eq!(
            format_class_docstring(Some("Job applicant resume."), &[], "Attributes", "    "),
            Some("\"\"\"Job applicant resume.\"\"\"".to_string()),
        );
    }

    #[test]
    fn format_class_docstring_multi_line_summary_uses_block_form() {
        assert_eq!(
            format_class_docstring(Some("Line one.\nLine two."), &[], "Attributes", "    "),
            Some("\"\"\"\n    Line one.\n    Line two.\n    \"\"\"".to_string()),
        );
    }

    #[test]
    fn format_class_docstring_summary_plus_members_attributes_section() {
        let members = vec![
            (
                "title".to_string(),
                Some("Title shown in lists.".to_string()),
            ),
            ("body".to_string(), Some("Free-form body text.".to_string())),
        ];
        let out = format_class_docstring(
            Some("A document with a title and an optional body."),
            &members,
            "Attributes",
            "    ",
        );
        assert_eq!(
            out,
            Some(
                "\"\"\"\n    A document with a title and an optional body.\n\n    Attributes:\n        title: Title shown in lists.\n        body: Free-form body text.\n    \"\"\""
                    .to_string()
            )
        );
    }

    #[test]
    fn format_class_docstring_members_only_no_leading_summary_blank() {
        let members = vec![("HAPPY".to_string(), Some("Smiling face.".to_string()))];
        let out = format_class_docstring(None, &members, "Members", "    ");
        assert_eq!(
            out,
            Some("\"\"\"\n    Members:\n        HAPPY: Smiling face.\n    \"\"\"".to_string())
        );
    }

    #[test]
    fn format_class_docstring_member_doc_continuation_lines_indent_under_name() {
        let members = vec![("a".to_string(), Some("First.\nContinuation.".to_string()))];
        let out = format_class_docstring(None, &members, "Attributes", "    ");
        assert_eq!(
            out,
            Some(
                "\"\"\"\n    Attributes:\n        a: First.\n            Continuation.\n    \"\"\""
                    .to_string()
            )
        );
    }

    #[test]
    fn format_class_docstring_escapes_triple_quote_in_summary_and_member() {
        let members = vec![("a".to_string(), Some("she said \"\"\" thrice".to_string()))];
        let out = format_class_docstring(
            Some("contains \"\"\" inside"),
            &members,
            "Attributes",
            "    ",
        );
        let expected = "\"\"\"\n    contains \\\"\"\" inside\n\n    Attributes:\n        a: she said \\\"\"\" thrice\n    \"\"\"";
        assert_eq!(out.as_deref(), Some(expected));
    }

    // ── Section visibility: "any-doc" rule ──────────────────────────────────

    /// No summary and zero documented members -> no docstring at all,
    /// even when undocumented members exist.
    #[test]
    fn format_class_docstring_no_section_when_no_member_documented() {
        let members = vec![("a".to_string(), None), ("b".to_string(), None)];
        assert_eq!(
            format_class_docstring(None, &members, "Attributes", "    "),
            None,
        );
    }

    /// Summary present but no member documented -> emit the summary
    /// alone, no `Attributes:` / `Members:` block.
    #[test]
    fn format_class_docstring_summary_only_skips_section_when_all_members_undocumented() {
        let members = vec![("a".to_string(), None), ("b".to_string(), None)];
        assert_eq!(
            format_class_docstring(Some("Just a summary."), &members, "Attributes", "    "),
            Some("\"\"\"Just a summary.\"\"\"".to_string()),
        );
    }

    /// At least one documented member -> the section appears and lists
    /// **every** member: documented as `name: doc`, undocumented as bare
    /// `name`. Order matches the input.
    #[test]
    fn format_class_docstring_section_lists_all_members_when_any_documented() {
        let members = vec![
            ("HAPPY".to_string(), Some("Smiling face.".to_string())),
            ("SAD".to_string(), None),
            ("NEUTRAL".to_string(), None),
        ];
        let out = format_class_docstring(Some("Sentiment scale"), &members, "Members", "    ");
        assert_eq!(
            out,
            Some(
                "\"\"\"\n    Sentiment scale\n\n    Members:\n        HAPPY: Smiling face.\n        SAD\n        NEUTRAL\n    \"\"\""
                    .to_string()
            ),
        );
    }

    /// Same rule with no summary: the section still appears once any
    /// member is documented, and still lists every member.
    #[test]
    fn format_class_docstring_section_appears_without_summary_when_any_documented() {
        let members = vec![
            ("a".to_string(), Some("First.".to_string())),
            ("b".to_string(), None),
        ];
        let out = format_class_docstring(None, &members, "Attributes", "    ");
        assert_eq!(
            out,
            Some("\"\"\"\n    Attributes:\n        a: First.\n        b\n    \"\"\"".to_string()),
        );
    }
}
