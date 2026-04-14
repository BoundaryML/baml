//! Tests for field-access completions.

#[cfg(test)]
mod tests {
    use crate::{completions::completions_at, testing::CursorTest};

    #[test]
    fn test_field_access_after_dot() {
        // Next line is a string literal — parser can't join across the line.
        let test = CursorTest::new(
            r#"
class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    "dummy"
}

function TestIt() -> string {
    let result = ClassifySentiment("hi")
    result.<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"feeling"),
            "Should contain 'feeling' field, got: {labels:?}"
        );
        assert!(
            labels.contains(&"confidence"),
            "Should contain 'confidence' field, got: {labels:?}"
        );
        assert!(
            labels.contains(&"reasoning"),
            "Should contain 'reasoning' field, got: {labels:?}"
        );
    }

    #[test]
    fn test_field_access_after_dot_with_word_on_next_line() {
        // Next line starts with a WORD — parser may join `result.assert`
        // into a single PATH_EXPR. This is the real-world scenario.
        let test = CursorTest::new(
            r#"
class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    "dummy"
}

function TestIt() -> string {
    let result = ClassifySentiment("hi")
    result.<[CURSOR]
    assert
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"feeling"),
            "Should contain 'feeling' field, got: {labels:?}"
        );
    }

    #[test]
    fn test_field_access_after_dot_with_dotted_next_line() {
        // Matches the real screenshot: `result.` then `assert.` on next line.
        // Parser may form `result.assert` as PATH_EXPR with `assert.` being
        // part of yet another FIELD_ACCESS_EXPR.
        let test = CursorTest::new(
            r#"
class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    "dummy"
}

function TestIt() -> string {
    let result = ClassifySentiment("hi")
    result.<[CURSOR]
    assert.eq(result, result)
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"feeling"),
            "Should contain 'feeling' field, got: {labels:?}"
        );
    }

    #[test]
    fn test_field_access_in_testset() {
        // Real-world scenario: `result.` inside a testset > for > test block.
        let test = CursorTest::new(
            r#"
class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    "dummy"
}

testset "test" {
    test "basic" {
        let result = ClassifySentiment("hi")
        result.<[CURSOR]
        assert.equal(result, result)
    }
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"feeling"),
            "Should contain 'feeling' field, got: {labels:?}"
        );
    }

    #[test]
    fn test_field_access_in_deeply_nested_testset() {
        // Exact reproduction of the user's real code:
        // testset > for > testset > for > test > let result = ... > result.
        let test = CursorTest::new(
            r#"
class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    "dummy"
}

function GenerateTests(n: int, topic: string) -> string[] {
    ["a", "b"]
}

testset "test" {
    let topics = ["happy", "sad"];
    for (let sentiments in topics) {
        testset sentiments {
            let tests = GenerateTests(5, "sad");
            for (let ex in tests) {
                test "sentiment:" + sentiments + ":" + ex {
                    let result = ClassifySentiment("hi");
                    result.<[CURSOR]
                    assert.equal(result, result);
                }
            }
        }
    }
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"feeling"),
            "Should contain 'feeling' field, got: {labels:?}"
        );
        assert!(
            labels.contains(&"confidence"),
            "Should contain 'confidence' field, got: {labels:?}"
        );
        assert!(
            labels.contains(&"reasoning"),
            "Should contain 'reasoning' field, got: {labels:?}"
        );
    }

    #[test]
    fn test_field_access_partial_segment() {
        let test = CursorTest::new(
            r#"
class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    "dummy"
}

function TestIt() -> string {
    let result = ClassifySentiment("hi")
    result.f<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"feeling"),
            "Should contain 'feeling' field, got: {labels:?}"
        );
    }

    #[test]
    fn test_field_access_enum_variants() {
        let test = CursorTest::new(
            r#"
enum Status {
    Active
    Inactive
}

function Test() -> string {
    Status.<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"Active"),
            "Should contain 'Active' variant, got: {labels:?}"
        );
        assert!(
            labels.contains(&"Inactive"),
            "Should contain 'Inactive' variant, got: {labels:?}"
        );
    }
}
