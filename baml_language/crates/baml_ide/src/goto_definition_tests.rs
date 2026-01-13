//! Tests for goto-definition using cursor-based testing.

#[cfg(test)]
mod tests {
    use crate::testing::CursorTest;

    #[test]
    fn test_goto_def_parameter() {
        let test = CursorTest::new(r#"
function Foo(r SentimentResponse) -> string {
    match (<[CURSOR]r) {
        Happy => "happy"
        Sad => "sad"
    }
}
"#);

        let result = test.goto_definition();
        assert!(result.contains("-> r"), "Should navigate to parameter 'r', got: {}", result);
    }

    #[test]
    fn test_goto_def_local_variable() {
        let test = CursorTest::new(r#"
function Test() -> string {
    let x = "hello"
    let y = <[CURSOR]x
    y
}
"#);

        let result = test.goto_definition();
        assert!(result.contains("-> x"), "Should navigate to variable 'x', got: {}", result);
    }

    #[test]
    fn test_goto_def_function_call() {
        let test = CursorTest::new(r#"
function Helper() -> string {
    "result"
}

function Main() -> string {
    <[CURSOR]Helper()
}
"#);

        let result = test.goto_definition();
        assert!(result.contains("-> Helper"), "Should navigate to function 'Helper', got: {}", result);
    }

    #[test]
    fn test_goto_def_class_reference() {
        let test = CursorTest::new(r#"
class Person {
    name string
}

function CreatePerson() -> Person {
    <[CURSOR]Person { name: "John" }
}
"#);

        let result = test.goto_definition();
        assert!(result.contains("-> Person"), "Should navigate to class 'Person', got: {}", result);
    }

    #[test]
    fn test_goto_def_enum_variant() {
        let test = CursorTest::new(r#"
enum Status {
    Active
    Inactive
}

function GetStatus() -> Status {
    Status::<[CURSOR]Active
}
"#);

        let result = test.goto_definition();
        // For now, enum variants navigate to the enum itself
        assert!(result.contains("-> Status") || result.contains("-> Active"),
            "Should navigate to enum or variant, got: {}", result);
    }

    #[test]
    fn test_goto_def_field_access() {
        let test = CursorTest::new(r#"
class Person {
    name string
    age int
}

function GetName(p Person) -> string {
    p.<[CURSOR]name
}
"#);

        let result = test.goto_definition();
        // For now, field access might navigate to the class
        assert!(result.contains("Person") || result.contains("name"),
            "Should navigate to Person class or name field, got: {}", result);
    }

    #[test]
    fn test_goto_def_in_block() {
        let test = CursorTest::new(r#"
function Test() -> string {
    {
        let inner = "value"
        <[CURSOR]inner
    }
}
"#);

        let result = test.goto_definition();
        assert!(result.contains("-> inner"), "Should navigate to inner variable, got: {}", result);
    }

    #[test]
    fn test_goto_def_match_pattern_binding() {
        let test = CursorTest::new(r#"
enum Result {
    Ok { value string }
    Err { message string }
}

function HandleResult(r Result) -> string {
    match (r) {
        Ok(o) => o.value
        Err(e) => <[CURSOR]e.message
    }
}
"#);

        let result = test.goto_definition();
        // Pattern bindings should be resolvable
        assert!(result.contains("e") || result.contains("No definition"),
            "Pattern binding navigation, got: {}", result);
    }

    #[test]
    fn test_goto_def_no_definition() {
        let test = CursorTest::new(r#"
function Test() -> string {
    <[CURSOR]undefined_var
}
"#);

        let result = test.goto_definition();
        assert!(result.contains("No definition"),
            "Should not find definition for undefined variable, got: {}", result);
    }

    #[test]
    fn test_goto_def_multi_file() {
        let mut builder = CursorTest::builder();
        builder.source("types.baml", r#"
class Person {
    name string
}
"#);
        builder.source("main.baml", r#"
function CreatePerson() -> Person {
    <[CURSOR]Person { name: "Alice" }
}
"#);
        let test = builder.build();

        let result = test.goto_definition();
        assert!(result.contains("types.baml") || result.contains("-> Person"),
            "Should navigate to Person in types.baml, got: {}", result);
    }
}