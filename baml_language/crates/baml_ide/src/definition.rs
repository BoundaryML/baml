//! `definition_at` — go-to-definition at a cursor position.
//!
//! A thin composition over the addressing layer: [`crate::resolve::symbol_at`]
//! identifies the symbol under the cursor, [`crate::resolve::target_definition`]
//! maps it to the declaration's name token. All resolution cases (items,
//! builtins, locals, fields, variants, methods, interface slots) live there.

use baml_base::SourceFile;
use text_size::TextSize;

pub use crate::resolve::Location;
use crate::resolve::{symbol_at, target_definition};

/// Find the definition of the symbol at `offset` in `file`.
///
/// Regular function (not cached); the expensive work (`file_semantic_index`,
/// `resolve_name_at`, inference) is internally Salsa-cached. Returns `None`
/// if the cursor is not on an identifier or the name cannot be resolved.
pub fn definition_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<Location> {
    let target = symbol_at(db, file, offset)?;
    target_definition(db, target)
}

#[cfg(test)]
mod tests {
    use super::{Location, definition_at};
    use crate::test_support::CursorTest;

    /// Feature-side conveniences over the shared cursor harness.
    trait GotoExt {
        fn goto_definition(&self) -> Option<Location>;
        fn format_location_with_name(&self, loc: &Location) -> String;
    }

    impl GotoExt for CursorTest {
        fn goto_definition(&self) -> Option<Location> {
            definition_at(&self.db, self.cursor.file, self.cursor.offset)
        }

        fn format_location_with_name(&self, loc: &Location) -> String {
            self.format_file_range_with_text(loc.file, loc.range)
        }
    }

    #[test]
    fn test_goto_def_parameter() {
        let test = CursorTest::new(
            r#"
function foo(r: SentimentResponse) -> string {
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
function example() -> string {
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
function helper() -> string {
    "result"
}

function main() -> string {
    <[CURSOR]helper()
}
"#,
        );

        let loc = test.goto_definition();
        let desc = loc
            .as_ref()
            .map(|l| test.format_location_with_name(l))
            .unwrap_or_else(|| "No definition found".into());
        assert!(
            desc.contains("-> helper"),
            "Should navigate to function 'helper', got: {desc}"
        );
    }

    #[test]
    fn test_goto_def_class_reference() {
        let test = CursorTest::new(
            r#"
class Person {
    name string
}

function create_person() -> Person {
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

function get_status() -> Status {
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

function get_name(p: Person) -> string {
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
function example() -> string {
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
function example() -> string {
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
function create_person() -> Person {
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
function main() -> int {
  fo<[CURSOR]o(1)
}

function foo(x: int) -> int {
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
            desc.contains("-> foo"),
            "Should navigate to foo function, got: {desc}"
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

function foo(r: Result) -> string {
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

function foo(s: Success) -> string {
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

function foo() -> Success {
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

function foo(s: Success) -> string {
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
  function celebrate(self) -> string {
    "Yay!"
  }
}

function foo(s: Success) -> string {
  s.<[CURSOR]celebrate()
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
            desc.contains("-> celebrate"),
            "Should navigate to method 'celebrate', got: {desc}"
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

function foo(t: TypeValue) -> string {
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

function read_name(p: Person) -> string {
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

function pick_text(d: Data) -> string {
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

    #[test]
    fn member_access_never_resolves_to_a_shadowing_local() {
        // `e.message` names a FIELD; a parameter of the same name is in
        // scope. The member span commits to member resolution — landing on
        // the parameter would be wrong-name navigation.
        let test = CursorTest::new(
            r#"
class IoError {
    message string
}

function respond(message: string, e: IoError) -> string {
    let note = `error: ${e.<[CURSOR]message}`
    message
}
"#,
        );

        let loc = test.goto_definition().expect("field definition found");
        let desc = test.format_location_with_name(&loc);
        assert!(
            desc.contains("test.baml:3"),
            "lands on the field declaration line, got: {desc}"
        );

        // The bare name in the same body still resolves to the parameter.
        let bare = CursorTest::new(
            r#"
class IoError {
    message string
}

function respond(message: string, e: IoError) -> string {
    let note = e.message
    <[CURSOR]message
}
"#,
        );
        let loc = bare.goto_definition().expect("parameter found");
        let desc = bare.format_location_with_name(&loc);
        assert!(
            desc.contains("test.baml:6"),
            "bare name lands on the parameter, got: {desc}"
        );
    }

    #[test]
    fn qualified_names_resolve_in_expressions_and_annotations() {
        // `util.Widget` in a type ANNOTATION — no expression claims the
        // token, so this exercises the CST dot-chain rung.
        let mut builder = CursorTest::builder();
        builder.source(
            "ns_util/helpers.baml",
            r#"
class Widget {
    size int
}
"#,
        );
        builder.source(
            "main.baml",
            r#"
function build() -> int {
    let w: root.util.Wid<[CURSOR]get = root.util.Widget { size: 1 }
    w.size
}
"#,
        );
        let test = builder.build();
        let loc = test.goto_definition().expect("annotation path resolves");
        assert!(
            test.format_location_with_name(&loc)
                .contains("helpers.baml"),
            "lands in the namespace file"
        );
    }

    #[test]
    fn stdlib_call_targets_resolve_to_builtin_sources() {
        // `baml.http.fetch` in an EXPRESSION — the inference ladder has no
        // member record for a namespace-qualified name, so this exercises
        // the resolve_path_at fallback inside the member-position claim.
        let test = CursorTest::new(
            r#"
function get(url: string) -> string {
    let body = baml.http.fet<[CURSOR]ch(url)
    "done"
}
"#,
        );
        let loc = test.goto_definition().expect("stdlib function resolves");
        let path = loc.file.path(&test.db);
        assert!(
            path.to_string_lossy().starts_with("<builtin>/"),
            "definition lives in the stdlib sources, got: {}",
            path.display()
        );
    }
}
