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

const OVERRIDE_TO_JSON: &str = r#"{
    function to_json(self) -> baml.json.json throws baml.json.SerializationError { "x" }
}"#;

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
    assert_error(&source, "`Plain` has no literal field for its tag");
}

#[test]
fn an_untagged_field_union_is_an_error() {
    let source =
        format!("{TAGGED}class Plain {{ name: string }}\nclass Holder {{ value: Plain | Other }}");
    assert_error(&source, "`Plain` has no literal field for its tag");
}

#[test]
fn a_map_beside_a_fallback_is_an_error() {
    let source = format!("{TAGGED}type Thing = Known | map<string, json> | Other");
    assert_error(&source, "has no literal field for its tag");
}

#[test]
fn unknown_beside_a_fallback_is_an_error() {
    let source = format!("{TAGGED}class Holder {{ value: Known | unknown | Other }}");
    assert_error(&source, "`unknown` has no literal field for its tag");
}

#[test]
fn a_type_parameter_beside_a_fallback_is_an_error() {
    let source = format!("{TAGGED}class Box<T> {{ value: T | Other }}");
    assert_error(&source, "has no literal field for its tag");
}

#[test]
fn every_spelling_of_a_union_is_checked() {
    let plain = format!("{TAGGED}class Plain {{ name: string }}\nclass Box<T> {{ v: T }}\n");
    for declaration in [
        "function f(p: Known | Other | Plain) -> int { 1 }",
        "function f() -> Known | Other | Plain { throw 1 }",
        "class H { b: Box<Other | Plain> }",
        "class H { b: (Other | Plain)[]? }",
        "class H { b: (Other | Plain)[] | string }",
        "type U = map<string, (Other | Plain)[]> | null",
    ] {
        assert_error(
            &format!("{plain}{declaration}"),
            "`Plain` has no literal field for its tag",
        );
    }
}

#[test]
fn members_the_tags_cannot_tell_apart_are_an_error() {
    assert_error(
        r#"
class A { type: "x", a: int }
class B { type: "x", b: int }
class Fb { type: "UNKNOWN", raw: map<string, json> @json.fallback }
type U = A | B | Fb
"#,
        "no literal field tells `A` from `B`",
    );
}

#[test]
fn members_sharing_a_tag_can_be_told_apart_by_a_second_literal() {
    assert_eq!(
        errors(
            r#"
class S3 { type: "storage", service: "s3" }
class Gcs { type: "storage", service: "gcs" }
class Ai { type: "ai" }
class Fb { type: "UNKNOWN", raw: map<string, json> @json.fallback }
type U = S3 | Gcs | Ai | Fb
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_tag_renamed_only_under_aliased_names_is_an_error() {
    assert_error(
        r#"
class A { type: "a" @alias("kind") }
class Fb { type: "UNKNOWN", raw: map<string, json> @json.fallback }
type U = A | Fb
"#,
        "`A` has no literal field for its tag",
    );
}

#[test]
fn a_fallback_class_needs_a_tag() {
    assert_error(
        "class A { raw: map<string, json> @json.fallback }",
        "needs a literal field to name the union's tag",
    );
}

#[test]
fn a_field_json_has_to_read_cannot_be_skipped() {
    assert_error(
        "class A { x: map<string, json> @json.flatten @skip }",
        "`@skip` leaves `x` out of JSON",
    );
    assert_error(
        "class A { type: \"x\" @skip, raw: map<string, json> @json.fallback }",
        "`@skip` leaves `type` out of JSON",
    );
}

#[test]
fn one_field_cannot_be_both_flatten_and_fallback() {
    assert_error(
        "class A { type: \"x\", raw: map<string, json> @json.flatten @json.fallback }",
        "`A` can have only one",
    );
}

#[test]
fn map_of_json_is_recognized_through_aliases() {
    assert_eq!(
        errors(
            r#"
type J = json
type K = string
class A { extra: map<K, J> @json.flatten }
"#
        ),
        Vec::<String>::new()
    );
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
    assert_error(&source, "has no literal field for its tag");
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
fn an_override_of_one_instantiation_counts() {
    assert_error(
        &format!(
            "class Box<T> {{ a: T @json.rename(\"b\") }}\nimplements baml.ToJson for Box<string> {OVERRIDE_TO_JSON}"
        ),
        "`Box` implements `baml.ToJson`",
    );
}

#[test]
fn an_enum_with_an_override_cannot_rename() {
    assert_error(
        &format!(
            "enum Mode {{ Fast @json.rename(\"fast\") }}\nimplements baml.ToJson for Mode {OVERRIDE_TO_JSON}"
        ),
        "`Mode` implements `baml.ToJson`",
    );
}

#[test]
fn an_unrelated_method_named_to_json_is_not_an_override() {
    assert_eq!(
        errors(
            r#"
interface Named { function to_json(self) -> string throws never; }
class A { a: string @json.rename("b") }
implements Named for A {
    function to_json(self) -> string throws never { "x" }
}
"#
        ),
        Vec::<String>::new()
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
