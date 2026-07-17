//! Tests for `document_highlights_at` using cursor-based testing.

#[cfg(test)]
mod tests {
    use text_size::{TextRange, TextSize};

    use crate::testing::CursorTest;

    /// Byte ranges of every occurrence of `needle` in the cursor file's text
    /// (the `<[CURSOR]` marker is already stripped).
    fn occurrences(test: &CursorTest, needle: &str) -> Vec<TextRange> {
        let text = test.cursor.file.text(&test.db);
        text.match_indices(needle)
            .map(|(start, _)| {
                TextRange::new(
                    TextSize::from(u32::try_from(start).unwrap()),
                    TextSize::from(u32::try_from(start + needle.len()).unwrap()),
                )
            })
            .collect()
    }

    #[test]
    fn local_variable_highlights_declaration_and_usages() {
        let test = CursorTest::new(
            r#"
function Demo(items: int[]) -> int {
    let total = items.length()
    let doubled = <[CURSOR]total + total
    doubled
}
"#,
        );

        assert_eq!(
            test.document_highlights(),
            occurrences(&test, "total"),
            "declaration + both usages of `total` should highlight"
        );
    }

    #[test]
    fn cursor_on_declaration_highlights_usages_too() {
        let test = CursorTest::new(
            r#"
function Demo() -> int {
    let <[CURSOR]count = 1
    count + count
}
"#,
        );

        assert_eq!(
            test.document_highlights(),
            occurrences(&test, "count"),
            "cursor on the declaration should still highlight the usages"
        );
    }

    #[test]
    fn parameter_highlights_declaration_and_usages() {
        let test = CursorTest::new(
            r#"
function Echo(<[CURSOR]word: string) -> string {
    word + word
}
"#,
        );

        assert_eq!(
            test.document_highlights(),
            occurrences(&test, "word"),
            "param declaration + both body usages should highlight"
        );
    }

    #[test]
    fn top_level_item_highlights_stay_in_the_request_file() {
        let mut builder = CursorTest::builder();
        builder.source(
            "main.baml",
            r#"
function Greet(name: string) -> string {
    name
}

function Caller() -> string {
    <[CURSOR]Greet("hi")
}
"#,
        );
        builder.source(
            "other.baml",
            r#"
function AlsoUses() -> string {
    Greet("yo")
}
"#,
        );
        let test = builder.build();

        assert_eq!(
            test.document_highlights(),
            occurrences(&test, "Greet"),
            "definition + same-file usages only; other.baml's call must not leak in"
        );
    }

    #[test]
    fn non_identifier_positions_highlight_nothing() {
        let test = CursorTest::new(
            r#"
function Demo() -> int {
    1 <[CURSOR]+ 2
}
"#,
        );

        assert!(
            test.document_highlights().is_empty(),
            "operators should not highlight"
        );
    }
}
