//! Tests for `definition_at` using cursor-based testing.

#[cfg(test)]
mod tests {
    use crate::testing::CursorTest;

    #[test]
    fn test_goto_def_parameter() {
        let test = CursorTest::new(
            r#"
function Foo(r: SentimentResponse) -> string {
    match (<[CURSOR]r) {
        Happy => "happy"
        Sad => "sad"
    }
}
"#,
        );

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> r"),
            "Should navigate to parameter 'r', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_local_variable() {
        let test = CursorTest::new(
            r#"
function Test() -> string {
    let x = "hello"
    let y = <[CURSOR]x
    y
}
"#,
        );

        let loc = test.goto_definition();
        assert!(
            loc.is_some(),
            "Should find definition of local variable 'x'"
        );
    }

    #[test]
    fn test_goto_def_function_call() {
        let test = CursorTest::new(
            r#"
function Helper() -> string {
    "result"
}

function Main() -> string {
    <[CURSOR]Helper()
}
"#,
        );

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> Helper"),
            "Should navigate to function 'Helper', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_class_reference() {
        let test = CursorTest::new(
            r#"
class Person {
    name string
}

function CreatePerson() -> Person {
    <[CURSOR]Person { name: "John" }
}
"#,
        );

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> Person"),
            "Should navigate to class 'Person', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_enum_variant() {
        let test = CursorTest::new(
            r#"
enum Status {
    Active
    Inactive
}

function GetStatus() -> Status {
    Status.<[CURSOR]Active
}
"#,
        );

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> Active"),
            "Should navigate to enum variant 'Active', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_field_access() {
        let test = CursorTest::new(
            r#"
class Person {
    name string
    age int
}

function GetName(p: Person) -> string {
    p.<[CURSOR]name
}
"#,
        );

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> name"),
            "Should navigate to field 'name', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_in_block() {
        let test = CursorTest::new(
            r#"
function Test() -> string {
    {
        let inner = "value"
        <[CURSOR]inner
    }
}
"#,
        );

        let loc = test.goto_definition();
        assert!(
            loc.is_some(),
            "Should find definition of block-scoped variable 'inner'"
        );
    }

    #[test]
    fn test_goto_def_no_definition() {
        let test = CursorTest::new(
            r#"
function Test() -> string {
    <[CURSOR]undefined_var
}
"#,
        );

        let loc = test.goto_definition();
        assert!(
            loc.is_none(),
            "Should not find definition for undefined variable"
        );
    }

    #[test]
    fn test_goto_def_multi_file() {
        let mut builder = CursorTest::builder();
        builder.source(
            "types.baml",
            r#"
class Person {
    name string
}
"#,
        );
        builder.source(
            "main.baml",
            r#"
function CreatePerson() -> Person {
    <[CURSOR]Person { name: "Alice" }
}
"#,
        );
        let test = builder.build();

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("types.baml") || desc.contains("-> Person"),
            "Should navigate to Person in types.baml, got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_function_call2() {
        let mut builder = CursorTest::builder();
        builder.source(
            "main.baml",
            r#"
function Main() -> int {
  Fo<[CURSOR]o(1)
}

function Foo(x: int) -> int {
  10
}

"#,
        );
        let test = builder.build();

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> Foo"),
            "Should navigate to Foo function, got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_match_pattern_type_annotation() {
        let mut builder = CursorTest::builder();
        builder.source(
            "main.baml",
            r#"
class Success {
  data string
}

class Failure {
  reason string
}

type Result = Success | Failure

function Foo(r: Result) -> string {
  match (r) {
    s: Success => s.data,
    f: <[CURSOR]Failure => f.reason,
  }
}
"#,
        );
        let test = builder.build();

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> Failure"),
            "Should navigate to Failure class, got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_field_access2() {
        let mut builder = CursorTest::builder();
        builder.source(
            "main.baml",
            r#"
class Success {
  data string
}

function Foo(s: Success) -> string {
  s.d<[CURSOR]ata
}
"#,
        );
        let test = builder.build();

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> data"),
            "Should navigate to field 'data', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_constructor_field() {
        let mut builder = CursorTest::builder();
        builder.source(
            "main.baml",
            r#"
class Success {
  data string
}

function Foo() -> Success {
  Success{ d<[CURSOR]ata: "success!" }
}
"#,
        );
        let test = builder.build();

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> data"),
            "Should navigate to constructor field 'data', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_field_receiver() {
        let mut builder = CursorTest::builder();
        builder.source(
            "main.baml",
            r#"
class Success {
  data string
}

function Foo(s: Success) -> string {
  <[CURSOR]s.data
}
"#,
        );
        let test = builder.build();

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> s"),
            "Should navigate to s parameter in type signature, got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_method() {
        let mut builder = CursorTest::builder();
        builder.source(
            "main.baml",
            r#"
class Success {
  data string
  function Celebrate(self) -> string {
    "Yay!"
  }
}

function Foo(s: Success) -> string {
  s.<[CURSOR]Celebrate()
}
"#,
        );
        let test = builder.build();

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> Celebrate"),
            "Should navigate to method 'Celebrate', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_keyword_named_method() {
        let mut builder = CursorTest::builder();
        builder.source(
            "main.baml",
            r#"
class TypeValue {
  function implements(self) -> string {
    "ok"
  }
}

function Foo(t: TypeValue) -> string {
  t.<[CURSOR]implements()
}
"#,
        );
        let test = builder.build();

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> implements"),
            "Should navigate to keyword-named method 'implements', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_interface_field() {
        let test = CursorTest::new(
            r#"
interface Named {
  fullname: string
}

class Person {
  display_name: string

  implements Named {
    fullname as display_name
  }
}

function ReadName(p: Person) -> string {
  return p.as<Named>.<[CURSOR]fullname
}
"#,
        );

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        // Assert the exact target location (interface field `Named.fullname` at
        // 3:3), so the test can't pass by landing on the `fullname as ...` alias
        // entry or the interface header (both of which also read "-> fullname").
        assert!(
            desc.contains(":3:3 -> fullname"),
            "Should navigate to the interface field declaration Named.fullname (3:3), got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_interface_method() {
        let test = CursorTest::new(
            r#"
interface Serializer {
  function encode(self) -> string
}

class Data {
  implements Serializer {
    function encode(self) -> string { return "json" }
  }
}

function PickText(d: Data) -> string {
  return d.as<Serializer>.<[CURSOR]encode()
}
"#,
        );

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        // Assert the exact target location (interface method `Serializer.encode`
        // at 3:12), so the test can't pass by landing on the class impl method or
        // the interface header (both of which also read "-> encode").
        assert!(
            desc.contains(":3:12 -> encode"),
            "Should navigate to the interface method declaration Serializer.encode (3:12), got: {desc}"
        );
    }
}
