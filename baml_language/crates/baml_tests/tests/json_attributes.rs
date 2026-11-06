//! The mistakes `@json` attributes make possible are compile errors.

use baml_compiler_diagnostics::Severity;
use baml_tests::stdlib_prefix::{check_user_files, setup_test_db};

fn errors(source: &str) -> Vec<String> {
    check_user_files(&setup_test_db(source))
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect()
}

fn assert_error(source: &str, expected: &str) {
    let messages = errors(source);
    assert!(
        messages.iter().any(|message| message.contains(expected)),
        "{source}: {messages:?}"
    );
}

const TAGGED: &str = r#"
class Known { type: "known", name: string }
class Other { type: "UNKNOWN", raw: map<string, json> @json.fallback }
"#;

#[test]
fn attributes_compile_on_the_declarations_they_describe() {
    assert_eq!(
        errors(
            r#"
class Config {
    id: string @json.rename("config_id"),
    extra: map<string, json> @json.flatten,

    @@json.omit_nulls
}

class Other {
    type: "UNKNOWN",
    raw: map<string, json> @json.fallback,
}

enum Mode {
    Fast @json.rename("fast-mode")
}
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_union_may_hold_members_that_read_no_object_beside_its_fallback() {
    assert_eq!(
        errors(&format!(
            r#"{TAGGED}
type Thing = Known | Other | string | null
class Holder {{ value: (Known | Other)?, items: (string | Known | Other)[] }}
"#
        )),
        Vec::<String>::new()
    );
}

#[test]
fn an_untagged_class_beside_a_fallback_is_an_error() {
    let source =
        format!("{TAGGED}class Plain {{ name: string }}\ntype Thing = Known | Plain | Other");
    assert_error(&source, "`Plain` has no literal field to match on");
}

#[test]
fn an_untagged_field_union_is_an_error() {
    let source =
        format!("{TAGGED}class Plain {{ name: string }}\nclass Holder {{ value: Plain | Other }}");
    assert_error(&source, "`Plain` has no literal field to match on");
}

#[test]
fn a_map_beside_a_fallback_is_an_error() {
    let source = format!("{TAGGED}type Thing = Known | map<string, json> | Other");
    assert_error(&source, "has no literal field to match on");
}

#[test]
fn unknown_beside_a_fallback_is_an_error() {
    let source = format!("{TAGGED}class Holder {{ value: Known | unknown | Other }}");
    assert_error(&source, "`unknown` has no literal field to match on");
}

#[test]
fn a_flatten_alias_cycle_is_an_error_not_a_hang() {
    assert_error(
        "type A = B\ntype B = A\nclass C { x: A @json.flatten }",
        "`@json.flatten` needs a `map<string, json>` field",
    );
}

#[test]
fn a_second_fallback_member_is_an_error() {
    let source = format!(
        "{TAGGED}class Another {{ kind: \"x\", raw: map<string, json> @json.fallback }}\ntype Thing = Other | Another"
    );
    assert_error(&source, "`Other` is this union's `@json.fallback` member");
}

#[test]
fn flatten_and_fallback_need_a_json_object_field() {
    assert_error(
        "class A { x: string @json.flatten }",
        "`@json.flatten` needs a `map<string, json>` field",
    );
    assert_error(
        "class A { x: map<string, int> @json.fallback }",
        "`@json.fallback` needs a `map<string, json>` field",
    );
}

#[test]
fn a_field_holding_the_objects_keys_has_no_key_to_rename() {
    assert_error(
        r#"class A { x: map<string, json> @json.flatten @json.rename("y") }"#,
        "it has no key for `@json.rename` to set",
    );
}

#[test]
fn a_class_has_one_field_for_the_rest_of_the_object() {
    assert_error(
        "class A { x: map<string, json> @json.flatten, y: map<string, json> @json.flatten }",
        "`A` can have only one",
    );
    assert_error(
        "class A { x: map<string, json> @json.flatten, y: map<string, json> @json.fallback }",
        "`A` can have only one",
    );
}

#[test]
fn a_fallback_class_holds_only_literals_beside_the_object() {
    assert_error(
        "class A { name: string, raw: map<string, json> @json.fallback }",
        "its other fields must be literals, and `name` is not one",
    );
}

#[test]
fn attributes_on_a_class_with_an_override_are_an_error() {
    let override_to_json = r#"
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            baml.json.from(self.a)
        }
    }"#;
    assert_error(
        &format!("class A {{ a: string @json.rename(\"b\"), {override_to_json} }}"),
        "`A` implements `baml.ToJson`, so its `@json` attributes would be ignored",
    );
    assert_error(
        &format!("class A {{ a: string, {override_to_json} @@json.omit_nulls }}"),
        "`A` implements `baml.ToJson`, so its `@json` attributes would be ignored",
    );
    assert_error(
        r#"
class A { a: string @json.rename("b") }
implements baml.FromJson for A {
    function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError { A { a: "x" } }
}
"#,
        "`A` implements `baml.FromJson`, so its `@json` attributes would be ignored",
    );
}

#[test]
fn an_impl_that_keeps_the_default_is_not_an_override() {
    assert_eq!(
        errors("class A { a: string @json.rename(\"b\"), implements baml.ToJson { } }"),
        Vec::<String>::new()
    );
}

#[test]
fn two_fields_cannot_share_a_json_key() {
    assert_error(
        r#"class A { a: string @json.rename("k"), b: string @json.rename("k") }"#,
        "duplicate serialized key `k`",
    );
    assert_error(
        r#"class A { a: string @json.rename("b"), b: string }"#,
        "duplicate serialized key `b`",
    );
    assert_error(
        r#"enum E { A @json.rename("k") B @json.rename("k") }"#,
        "duplicate serialized key `k`",
    );
    // Under aliased names `a` and `b` go by one key.
    assert_error(
        r#"class A { a: string @alias("x"), b: string @json.rename("x") }"#,
        "duplicate serialized key `x`",
    );
    assert_error(
        r#"enum E { A @alias("x") B @json.rename("x") }"#,
        "duplicate serialized key `x`",
    );
}

#[test]
fn a_field_holding_the_objects_keys_shares_no_key() {
    assert_eq!(
        errors(
            r#"
class A {
    raw: map<string, json> @json.fallback,
    other: "x" @json.rename("raw"),
}
class B {
    extra: map<string, json> @json.flatten,
    id: string @json.rename("extra"),
}
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn each_attribute_stays_where_it_has_meaning() {
    assert_error(
        "class A { a: string? @json.omit_nulls }",
        "`@json.omit_nulls` is not valid on a class field",
    );
    assert_error(
        "class A { a: map<string, json> @@json.flatten }",
        "not valid",
    );
}
