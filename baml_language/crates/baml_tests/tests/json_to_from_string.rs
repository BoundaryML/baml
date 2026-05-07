//! Phase 5 runtime tests for `baml.json.to_string<T>` / `from_string<T>`.
//!
//! These tests verify that the runtime type-arg threading reaches the
//! native handlers for typed JSON serialization / deserialization, and
//! that the typed walkers correctly handle each `Ty` shape.
//!
//! Test naming mirrors the spec laid out in
//! `.humanlayer/tasks/runtime-type-reflection-via-reflecttypeof/06-plan-native-json-type.md`,
//! superseded by the inline notes in the iteration prompt.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ─── Simple class round-trip ──────────────────────────────────────────────────

#[tokio::test]
async fn to_string_simple_class() {
    let source = r#"
        class User { name string  age int }
        function main() -> string {
            let u: User = User { name: "Ada", age: 30 };
            baml.json.to_string<User>(u)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"name":"Ada","age":30}"#.to_string()
        ))
    );
}

#[tokio::test]
async fn from_string_simple_class() {
    let source = r#"
        class User { name string  age int }
        function main() -> string {
            let u: User = baml.json.from_string<User>("{\"name\":\"Ada\",\"age\":30}");
            u.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

#[tokio::test]
async fn roundtrip_simple() {
    let source = r#"
        class User { name string  age int }
        function main() -> int {
            let u: User = User { name: "Ada", age: 30 };
            let s: string = baml.json.to_string<User>(u);
            let v: User = baml.json.from_string<User>(s);
            v.age
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// ─── Composite generic ─────────────────────────────────────────────────────────

#[tokio::test]
async fn composite_generic_roundtrip() {
    let source = r#"
        class User { name string  age int }
        class Container<T> { item T }
        function main() -> string {
            let c: Container<User> = Container<User> { item: User { name: "Ada", age: 30 } };
            let s: string = baml.json.to_string<Container<User>>(c);
            let d: Container<User> = baml.json.from_string<Container<User>>(s);
            d.item.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

// ─── Generic forwarding (mirrors reflect_type_of_generic.rs) ─────────────────

#[tokio::test]
async fn generic_forwarding_user() {
    let source = r#"
        class User { name string  age int }
        function fetch_as<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            baml.json.from_string<T>(s)
        }
        function main() -> string {
            let u: User = fetch_as<User>("{\"name\":\"Ada\",\"age\":30}");
            u.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

#[tokio::test]
async fn generic_forwarding_composite() {
    let source = r#"
        class User { name string  age int }
        class Container<T> { item T }
        function fetch_as<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            baml.json.from_string<T>(s)
        }
        function main() -> string {
            let c: Container<User> = fetch_as<Container<User>>("{\"item\":{\"name\":\"Ada\",\"age\":30}}");
            c.item.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

// ─── Three-level forwarding ───────────────────────────────────────────────────

#[tokio::test]
async fn three_level_forwarding() {
    let source = r#"
        class User { name string }
        function level1<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            baml.json.from_string<T>(s)
        }
        function level2<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            level1<T>(s)
        }
        function level3<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            level2<T>(s)
        }
        function main() -> string {
            let u: User = level3<User>("{\"name\":\"Ada\"}");
            u.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

// ─── Closure captures type-arg ────────────────────────────────────────────────

#[tokio::test]
async fn closure_captures_type_arg() {
    let source = r#"
        class User { name string }
        function make_decoder<T>() -> (string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            return (s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError { baml.json.from_string<T>(s) }
        }
        function main() -> string {
            let f = make_decoder<User>();
            let u: User = f("{\"name\":\"Ada\"}");
            u.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

// ─── Enum round-trip ──────────────────────────────────────────────────────────

#[tokio::test]
async fn enum_variant_to_string() {
    let source = r#"
        enum Color { Red Blue }
        function main() -> string {
            baml.json.to_string<Color>(Color.Red)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#""Red""#.to_string()))
    );
}

#[tokio::test]
async fn enum_variant_from_string() {
    let source = r#"
        enum Color { Red Blue }
        function main() -> Color {
            baml.json.from_string<Color>("\"Blue\"")
        }
    "#;
    let output = baml_test!(source);
    assert!(
        matches!(
            &output.result,
            Ok(BexExternalValue::Variant { variant_name, .. }) if variant_name == "Blue"
        ),
        "expected Variant Blue, got {:?}",
        output.result
    );
}

// ─── Optional field round-trip ────────────────────────────────────────────────

#[tokio::test]
async fn optional_field_present() {
    let source = r#"
        class Profile { nickname string? }
        function main() -> string {
            let p: Profile = Profile { nickname: "ada" };
            baml.json.to_string<Profile>(p)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"nickname":"ada"}"#.to_string()
        ))
    );
}

#[tokio::test]
async fn optional_field_null() {
    let source = r#"
        class Profile { nickname string? }
        function main() -> string {
            let p: Profile = Profile { nickname: null };
            baml.json.to_string<Profile>(p)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"nickname":null}"#.to_string()))
    );
}

#[tokio::test]
async fn optional_field_decode_missing() {
    let source = r#"
        class Profile { nickname string? }
        function main() -> bool {
            let p: Profile = baml.json.from_string<Profile>("{}");
            p.nickname == null
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── Recursive class round-trip ───────────────────────────────────────────────

#[tokio::test]
async fn recursive_class_roundtrip() {
    let source = r#"
        class Tree { value int  children Tree[] }
        function main() -> int {
            let t: Tree = Tree {
                value: 1,
                children: [
                    Tree { value: 2, children: [] },
                    Tree { value: 3, children: [Tree { value: 4, children: [] }] },
                ]
            };
            let s: string = baml.json.to_string<Tree>(t);
            let d: Tree = baml.json.from_string<Tree>(s);
            d.children[1].children[0].value
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

// ─── Error paths ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn parse_throws_on_bad_json() {
    let source = r#"
        class User { name string }
        function main() -> User {
            baml.json.from_string<User>("{[")
        }
    "#;
    let output = baml_test!(source);
    let err_str = output.result.unwrap_err().to_string();
    assert!(
        err_str.contains("JsonParseError") || err_str.contains("baml.json"),
        "expected JsonParseError, got {err_str}"
    );
}

#[tokio::test]
async fn decode_throws_on_missing_field() {
    let source = r#"
        class User { name string  age int }
        function main() -> User {
            baml.json.from_string<User>("{}")
        }
    "#;
    let output = baml_test!(source);
    let err_str = output.result.unwrap_err().to_string();
    assert!(
        err_str.contains("JsonDecodeError") || err_str.contains("baml.json"),
        "expected JsonDecodeError, got {err_str}"
    );
}

#[tokio::test]
async fn serialize_throws_uint8array() {
    let source = r#"
        class Bad { x uint8array }
        function main() -> string {
            let b: Bad = Bad { x: baml.Uint8Array.from_hex("00ff") };
            baml.json.to_string<Bad>(b)
        }
    "#;
    let output = baml_test!(source);
    let err_str = output.result.unwrap_err().to_string();
    assert!(
        err_str.contains("JsonSerializationError") || err_str.contains("uint8array"),
        "expected JsonSerializationError, got {err_str}"
    );
}

/// Decode-error path includes the offending field's dotted path so callers
/// can pinpoint what failed inside a nested object.
#[tokio::test]
async fn decode_error_path_points_at_missing_field() {
    let source = r#"
        class Inner { value int }
        class Outer { inner Inner }
        function main() -> Outer {
            baml.json.from_string<Outer>("{\"inner\":{}}")
        }
    "#;
    let output = baml_test!(source);
    let err_str = output.result.unwrap_err().to_string();
    assert!(
        err_str.contains(".inner.value"),
        "expected `.inner.value` in error path, got {err_str}"
    );
}

// ─── Enum round-trip (combined) ──────────────────────────────────────────────

/// Round-trip an enum value through `to_string<Color>` and `from_string<Color>`,
/// then assert the decoded variant equals the original.
#[tokio::test]
async fn enum_variant_roundtrip() {
    let source = r#"
        enum Color { Red Blue }
        function main() -> bool {
            let c: Color = Color.Blue;
            let s: string = baml.json.to_string<Color>(c);
            let d: Color = baml.json.from_string<Color>(s);
            d == Color.Blue
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── Media field round-trip ──────────────────────────────────────────────────

/// `to_string<Profile>` for a class with an `image` field emits the tagged
/// media object form `{"kind":"image","source":"url","value":...,"mime":...}`.
#[tokio::test]
async fn media_field_to_string() {
    let source = r#"
        class Profile { avatar image }
        function main() -> string {
            let p: Profile = Profile { avatar: image.from_url("https://example.com/a.png", "image/png") };
            baml.json.to_string<Profile>(p)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"avatar":{"kind":"image","source":"url","value":"https://example.com/a.png","mime":"image/png"}}"#
                .to_string()
        ))
    );
}

/// `from_string<Profile>` round-trips a media field through the tagged
/// object form.  We confirm by re-serialising and matching the JSON.
#[tokio::test]
async fn media_field_roundtrip() {
    let source = r#"
        class Profile { avatar image }
        function main() -> string {
            let p: Profile = Profile { avatar: image.from_url("https://example.com/a.png", "image/png") };
            let s: string = baml.json.to_string<Profile>(p);
            let d: Profile = baml.json.from_string<Profile>(s);
            baml.json.to_string<Profile>(d)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"avatar":{"kind":"image","source":"url","value":"https://example.com/a.png","mime":"image/png"}}"#
                .to_string()
        ))
    );
}

// ─── Field-level attributes ──────────────────────────────────────────────────

/// Per BEP-038 §"Not retrofitting `@alias` / `@skip`": the JSON interchange
/// path always uses raw field names, regardless of `@alias`.  The aliased
/// key only affects the LLM `ctx.output_format` / `$parse` path.
#[tokio::test]
async fn alias_field_uses_raw_name_in_json() {
    let source = r#"
        class Profile { display_name string @alias("displayName") }
        function main() -> string {
            baml.json.to_string<Profile>(Profile { display_name: "Ada" })
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"display_name":"Ada"}"#.to_string()
        ))
    );
}

/// Round-tripping a JSON document keyed by the raw field name succeeds even
/// when the field has `@alias("...")` attached.
#[tokio::test]
async fn alias_field_decodes_raw_name() {
    let source = r#"
        class Profile { display_name string @alias("displayName") }
        function main() -> string {
            let d: Profile = baml.json.from_string<Profile>("{\"display_name\":\"Ada\"}");
            d.display_name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

/// Per BEP-038: `@skip` is LLM-path-only.  JSON interchange still emits the
/// field — the whole class shape is preserved.
#[tokio::test]
async fn skip_field_still_emitted_in_json() {
    let source = r#"
        class Profile {
            name string
            internal_id string @skip
        }
        function main() -> string {
            baml.json.to_string<Profile>(Profile { name: "Ada", internal_id: "secret" })
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"name":"Ada","internal_id":"secret"}"#.to_string()
        ))
    );
}

// ─── json passthrough via TypeAlias ──────────────────────────────────────────

/// `from_string<json>` should preserve the parsed JSON shape without typing.
#[tokio::test]
async fn from_string_json_passthrough_int() {
    let source = r#"
        function main() -> bool {
            let j: json = baml.json.from_string<json>("42");
            match (j) {
                let n: int => true
                _ => false
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn from_string_json_passthrough_object() {
    let source = r#"
        function main() -> int {
            let j: json = baml.json.from_string<json>("{\"x\":7}");
            match (j) {
                let m: map<string, json> => m.length()
                _ => -1
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

/// `to_string<json>` round-trips a parsed json value back to its source string.
#[tokio::test]
async fn to_string_json_passthrough() {
    let source = r#"
        function main() -> string {
            let j: json = baml.json.parse("[1,2,3]");
            baml.json.to_string<json>(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("[1,2,3]".to_string()))
    );
}
