//! Tests for field-access completions.

#[cfg(test)]
mod tests {
    use crate::{CompletionKind, completions::completions_at, testing::CursorTest};

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

    // Issue #7: Multi-segment chained completion.
    // `foo.bar.<cursor>` should resolve the type of `foo.bar` (not just `bar`),
    // then offer completions based on that type.
    #[test]
    fn test_chained_field_access_completion() {
        let test = CursorTest::new(
            r#"
class Inner {
    name string
    value int
}

class Outer {
    inner Inner
    label string
}

function Test() -> string {
    let o = Outer { inner: Inner { name: "hi", value: 1 }, label: "x" }
    o.inner.<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"name"),
            "Should contain 'name' from Inner, got: {labels:?}"
        );
        assert!(
            labels.contains(&"value"),
            "Should contain 'value' from Inner, got: {labels:?}"
        );
        // Should NOT contain Outer's fields
        assert!(
            !labels.contains(&"label"),
            "Should NOT contain 'label' from Outer, got: {labels:?}"
        );
        assert!(
            !labels.contains(&"inner"),
            "Should NOT contain 'inner' from Outer, got: {labels:?}"
        );
    }

    // Issue #8: Lambda parameter member completion.
    // Inside a lambda, the parameter's type should be resolved for completions.
    #[test]
    fn test_lambda_param_completion() {
        let test = CursorTest::new(
            r#"
class Item {
    name string
    price int
}

function Test() -> string[] {
    let items = [Item { name: "a", price: 1 }]
    items.map((item) -> { item.<[CURSOR] })
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"name"),
            "Should contain 'name' from Item lambda param, got: {labels:?}"
        );
        assert!(
            labels.contains(&"price"),
            "Should contain 'price' from Item lambda param, got: {labels:?}"
        );
    }

    /// Issue #3: Nested lambda parameter completion.
    /// Inside a nested lambda (lambda inside lambda), the inner parameter's type
    /// should be resolved for completions.
    #[test]
    fn test_nested_lambda_param_completion() {
        let test = CursorTest::new(
            r#"
class Tag {
    label string
}

class Item {
    name string
    tags Tag[]
}

function Test() -> string[][] {
    let items = [Item { name: "a", tags: [Tag { label: "x" }] }]
    items.map((item) -> { item.tags.map((tag) -> { tag.<[CURSOR] }) })
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"label"),
            "Should contain 'label' from Tag nested lambda param, got: {labels:?}"
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

    #[test]
    fn test_image_instance_method_completion() {
        let test = CursorTest::new(
            r#"
function Test(img: image) -> string {
    img.<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        for expected in ["url", "file", "base64", "mime_type"] {
            assert!(
                labels.contains(&expected),
                "Should contain image method '{expected}', got: {labels:?}"
            );
        }
        assert!(
            !labels.contains(&"from_url"),
            "Instance image completion should not contain static constructors, got: {labels:?}"
        );
    }

    #[test]
    fn test_image_static_constructor_completion() {
        let test = CursorTest::new(
            r#"
function Test() -> image {
    image.<[CURSOR]
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        for expected in ["from_url", "from_file", "from_base64"] {
            assert!(
                labels.contains(&expected),
                "Should contain image constructor '{expected}', got: {labels:?}"
            );
        }
        assert!(
            !labels.contains(&"base64"),
            "Static image completion should not contain instance methods, got: {labels:?}"
        );
    }

    #[test]
    fn test_string_completion_uses_stdlib_method_names() {
        let test = CursorTest::new(
            r#"
function Test(s: string) -> string {
    s.<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"to_lower_case"),
            "Should contain stdlib string method 'to_lower_case', got: {labels:?}"
        );
        assert!(
            !labels.contains(&"lower"),
            "Should not contain stale hardcoded string method 'lower', got: {labels:?}"
        );
    }

    #[test]
    fn test_all_media_types_have_instance_method_completion() {
        for media_type in ["image", "audio", "video", "pdf"] {
            let test = CursorTest::new(&format!(
                r#"
function Test(value: {media_type}) -> string {{
    value.<[CURSOR]
    "done"
}}
"#
            ));

            let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
            let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

            for expected in ["url", "file", "base64", "mime_type"] {
                assert!(
                    labels.contains(&expected),
                    "Should contain {media_type} method '{expected}', got: {labels:?}"
                );
            }
        }
    }

    #[test]
    fn test_all_media_types_have_static_constructor_completion() {
        for media_type in ["image", "audio", "video", "pdf"] {
            let test = CursorTest::new(&format!(
                r#"
function Test() -> {media_type} {{
    {media_type}.<[CURSOR]
}}
"#
            ));

            let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
            let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

            for expected in ["from_url", "from_file", "from_base64"] {
                assert!(
                    labels.contains(&expected),
                    "Should contain {media_type} constructor '{expected}', got: {labels:?}"
                );
            }
        }
    }

    #[test]
    fn test_uint8array_instance_method_completion() {
        let test = CursorTest::new(
            r#"
function Test(bytes: uint8array) -> string {
    bytes.<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        for expected in ["length", "to_base64", "to_string", "sort"] {
            assert!(
                labels.contains(&expected),
                "Should contain uint8array method '{expected}', got: {labels:?}"
            );
        }
    }

    #[test]
    fn test_package_qualified_builtin_class_method_completion() {
        let test = CursorTest::new(
            r#"
function Test(array: int[]) -> int {
    baml.Array.<[CURSOR]
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        for expected in ["length", "at", "push", "map"] {
            assert!(
                labels.contains(&expected),
                "Should contain baml.Array method '{expected}', got: {labels:?}"
            );
        }
    }

    #[test]
    fn test_package_qualified_media_class_method_completion() {
        let test = CursorTest::new(
            r#"
function Test() -> image {
    baml.media.Image.<[CURSOR]
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        for expected in ["url", "base64", "from_url", "from_base64"] {
            assert!(
                labels.contains(&expected),
                "Should contain baml.media.Image method '{expected}', got: {labels:?}"
            );
        }
    }

    #[test]
    fn test_baml_package_completions() {
        // Test that `baml.` shows completions for the baml package namespace.
        let test = CursorTest::new(
            r#"
function Test() -> string {
    baml.<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        // baml package should have `events` namespace (log is now a top-level package)
        assert!(
            labels.contains(&"events"),
            "Should contain 'events' namespace, got: {labels:?}"
        );
    }

    #[test]
    fn test_bare_builtin_package_completions() {
        let test = CursorTest::new(
            r#"
function Test() -> string {
    b<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"baml"),
            "Should contain 'baml' package root, got: {labels:?}"
        );
        assert!(
            labels.contains(&"reflect"),
            "Should contain the 'reflect' package root, got: {labels:?}"
        );
        assert!(
            labels.contains(&"json"),
            "Should contain the 'json' namespace shorthand, got: {labels:?}"
        );
    }

    #[test]
    fn test_log_package_completions() {
        // Test that `log.` shows completions for log functions.
        let test = CursorTest::new(
            r#"
function Test() -> string {
    log.<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        // log package should have info, debug, warn, error functions
        assert!(
            labels.contains(&"info"),
            "Should contain 'info' function, got: {labels:?}"
        );
        assert!(
            labels.contains(&"debug"),
            "Should contain 'debug' function, got: {labels:?}"
        );
    }

    #[test]
    fn test_function_completion_shows_signature() {
        // Test that function completions show the full signature in the detail.
        let test = CursorTest::new(
            r#"
function MyFunc(param1: string, param2: int) -> bool {
    true
}

function Test() -> string {
    My<[CURSOR]
    "done"
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let my_func = completions.iter().find(|c| c.label == "MyFunc");

        assert!(my_func.is_some(), "Should have MyFunc completion");
        let detail = my_func.unwrap().detail.as_ref();
        assert!(detail.is_some(), "MyFunc should have a detail");
        let detail_str = detail.unwrap();
        assert!(
            detail_str.contains("param1: string"),
            "Detail should contain 'param1: string', got: {detail_str}"
        );
        assert!(
            detail_str.contains("param2: int"),
            "Detail should contain 'param2: int', got: {detail_str}"
        );
        assert!(
            detail_str.contains("-> bool"),
            "Detail should contain '-> bool', got: {detail_str}"
        );
    }

    #[test]
    fn test_value_completion_hides_shadowed_same_scope_local() {
        let test = CursorTest::new(
            r#"
function Test() -> int {
    let x = 1
    let x = 2
    x<[CURSOR]
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let x_count = completions.iter().filter(|c| c.label == "x").count();

        assert_eq!(
            x_count, 1,
            "Should only complete the innermost visible 'x', got: {completions:?}"
        );
    }

    #[test]
    fn test_value_completion_hides_shadowed_parameter() {
        let test = CursorTest::new(
            r#"
function Test(x: int) -> int {
    let x = 2
    x<[CURSOR]
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let x_count = completions.iter().filter(|c| c.label == "x").count();

        assert_eq!(
            x_count, 1,
            "Should only complete the local that shadows parameter 'x', got: {completions:?}"
        );
    }

    #[test]
    fn test_value_completion_after_incomplete_log_call_does_not_reenter_salsa() {
        let test = CursorTest::new(
            r#"
function SimulateHumanGuess(history: string[]) -> string {
  "guess"
}

function GuessGameAgent() -> string {
  let history: string[] = []
  log.info({"famous_person_name":
  let user_input = SimulateHumanGuess(history)
  user_input<[CURSOR]
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        assert!(
            completions.iter().any(|c| c.label == "history"),
            "expected local completion after malformed log call, got: {completions:?}"
        );
    }

    #[test]
    fn test_call_argument_completion_suggests_optional_params() {
        let test = CursorTest::new(
            r#"
function Search(query: string, max_results: int = 10, filter: string? = null) -> int {
    max_results
}

function Test() -> int {
    Search("cats", <[CURSOR])
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let max_results = completions
            .iter()
            .find(|completion| completion.label == "max_results")
            .expect("max_results completion");
        let filter = completions
            .iter()
            .find(|completion| completion.label == "filter")
            .expect("filter completion");

        assert_eq!(max_results.insert_text.as_deref(), Some("max_results = "));
        assert_eq!(max_results.kind, CompletionKind::Parameter);
        assert_eq!(filter.insert_text.as_deref(), Some("filter = "));
        assert_eq!(filter.kind, CompletionKind::Parameter);
        assert!(
            completions
                .iter()
                .all(|completion| completion.label != "query"),
            "Should not suggest parameter already provided positionally, got: {completions:?}"
        );
    }

    #[test]
    fn test_call_argument_completion_hides_already_provided_labels() {
        let test = CursorTest::new(
            r#"
function Search(query: string, max_results: int = 10, filter: string? = null) -> int {
    max_results
}

function Test() -> int {
    Search("cats", max_results = 5, <[CURSOR])
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            !labels.contains(&"max_results"),
            "Should not suggest already provided label, got: {labels:?}"
        );
        assert!(
            labels.contains(&"filter"),
            "Should still suggest remaining optional label, got: {labels:?}"
        );
    }

    #[test]
    fn test_call_argument_completion_keeps_required_named_params_available() {
        let test = CursorTest::new(
            r#"
function Search(query: string, max_results: int = 10, filter: string? = null) -> int {
    max_results
}

function Test() -> int {
    Search(max_results = 5, <[CURSOR])
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"query"),
            "Required named params should remain available, got: {labels:?}"
        );
        assert!(
            labels.contains(&"filter"),
            "Remaining optional params should remain available, got: {labels:?}"
        );
    }

    #[test]
    fn test_call_argument_completion_hides_multiple_positional_params() {
        let test = CursorTest::new(
            r#"
function Search(query: string, limit: int, filter: string? = null) -> int {
    limit
}

function Test() -> int {
    Search("cats", 5, <[CURSOR])
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            !labels.contains(&"query"),
            "Should not suggest first positional parameter, got: {labels:?}"
        );
        assert!(
            !labels.contains(&"limit"),
            "Should not suggest second positional parameter, got: {labels:?}"
        );
        assert!(
            labels.contains(&"filter"),
            "Should still suggest remaining parameter, got: {labels:?}"
        );
    }

    #[test]
    fn test_call_argument_completion_does_not_apply_inside_argument_expression() {
        let test = CursorTest::new(
            r#"
function Search(query: string, max_results: int = 10, filter: string? = null) -> int {
    max_results
}

function Test() -> int {
    let local_value = 2
    Search("cats", local_value + <[CURSOR])
}
"#,
        );

        let completions = completions_at(&test.db, test.cursor.file, test.cursor.offset);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        assert!(
            labels.contains(&"local_value"),
            "Should use value completions inside argument expressions, got: {labels:?}"
        );
        assert!(
            !labels.contains(&"max_results"),
            "Should not suggest outer call labels inside argument expressions, got: {labels:?}"
        );
    }
}
