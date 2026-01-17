#[derive(Debug)]
pub(crate) struct DocString(String);

impl DocString {
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn new(s: impl ToString) -> Self {
        Self(dedent(s.to_string().trim()))
    }

    pub(crate) fn as_comment(&self) -> String {
        self.0
            .lines()
            .map(|s| format!("# {}", s.trim_end()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Escapes the content as a Python docstring.
    pub(crate) fn as_docstring(&self) -> String {
        // Prefer """ as delimiter, fall back to ''' if content contains """
        // If content contains both, escape individual quotes and use """
        let has_triple_double = self.0.contains("\"\"\"");
        let has_triple_single = self.0.contains("'''");

        let (delimiter, escaped) = match (has_triple_double, has_triple_single) {
            (false, _) => ("\"\"\"", self.0.clone()),
            (true, false) => ("'''", self.0.clone()),
            // Contains both: escape """ sequences to \"\"\" to break them up
            (true, true) => ("\"\"\"", self.0.replace("\"\"\"", "\\\"\\\"\\\"")),
        };

        let escaped = escaped.trim();

        if escaped.contains('\n') {
            format!("{delimiter}\n{escaped}\n{delimiter}")
        } else {
            format!("{delimiter}{escaped}{delimiter}")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PyString(String);

impl PyString {
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn new(s: impl ToString) -> Self {
        let s = s.to_string();
        // escape characters that are special in Python strings
        let s = s.replace('\n', "\\n");
        let s = s.replace('\r', "\\r");
        let s = s.replace('\t', "\\t");

        let has_double_quotes = s.contains('"');
        let has_single_quotes = s.contains('\'');

        let s = match (has_double_quotes, has_single_quotes) {
            (true, true) => format!("\"{}\"", s.replace('"', "\\\"")),
            (true, false) => format!("'{s}'"),
            (false, _) => format!("\"{s}\""),
        };

        Self(s)
    }
}

impl std::fmt::Display for PyString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub(crate) fn dedent(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    if min_indent == 0 {
        return s.to_string();
    }

    lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line.trim()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    macro_rules! test_dedent {
        ($($name:ident: $input:expr => $expected:expr),* $(,)?) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!(dedent($input), $expected);
                }
            )*
        };
    }

    macro_rules! test_docstring {
        ($($name:ident: $input:expr => $expected:expr),* $(,)?) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!(DocString::new($input).as_docstring(), $expected);
                }
            )*
        };
    }

    macro_rules! test_comment {
        ($($name:ident: $input:expr => $expected:expr),* $(,)?) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!(DocString::new($input).as_comment(), $expected);
                }
            )*
        };
    }

    macro_rules! test_pystring {
        ($($name:ident: $input:expr => $expected:expr),* $(,)?) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!(PyString::new($input).to_string(), $expected);
                }
            )*
        };
    }

    test_dedent! {
        dedent_0: "    foo\n    bar\n    baz" => "foo\nbar\nbaz",
        dedent_1: "    foo\n\n    bar\n\n    baz" => "foo\n\nbar\n\nbaz",
        dedent_2: "foo\nbar\nbaz" => "foo\nbar\nbaz",
        dedent_3: "    foo\n        bar\n    baz" => "foo\n    bar\nbaz",
    }

    test_docstring! {
        docstring_0: "hello" => r#""""hello""""#,
        docstring_1: "foo\nbar\nbaz" => "\"\"\"\nfoo\nbar\nbaz\n\"\"\"",
        docstring_2: r#"has """triple""" quotes"# => r#"'''has """triple""" quotes'''"#,
        docstring_3: "has '''single''' quotes" => r#""""has '''single''' quotes""""#,
        // When both """ and ''' present, escape """ sequences only
        docstring_4: r#"has """both""" and '''quotes'''"# => r#""""has \"\"\"both\"\"\" and '''quotes'''""""#,
        // Four quotes: first 3 escaped, 4th stays as-is
        docstring_5: r#"""""and'''"# => r#""""\"\"\""and'''""""#,
    }

    test_comment! {
        comment_0: "hello" => "# hello",
        comment_1: "foo\nbar\nbaz" => "# foo\n# bar\n# baz",
        // trim() in new() removes leading indent from first line only
        comment_2: "    indented\n    block" => "# indented\n#     block",
        comment_3: "" => "",
        comment_4: "line one\n\nline three" => "# line one\n# \n# line three",
    }

    test_pystring! {
        pystring_0: "hello" => "\"hello\"",
        pystring_1: "foo\nbar\nbaz" => "\"foo\\nbar\\nbaz\"",
        pystring_2: "foo\"bar\"baz" => "'foo\"bar\"baz'",
        pystring_3: "foo'bar'baz" => "\"foo'bar'baz\"",
        pystring_4: "foo\"bar\"baz'foo'bar" => "\"foo\\\"bar\\\"baz'foo'bar\"",
        pystring_5: "foo'bar'baz\"foo\"bar" => "\"foo'bar'baz\\\"foo\\\"bar\"",
    }
}
