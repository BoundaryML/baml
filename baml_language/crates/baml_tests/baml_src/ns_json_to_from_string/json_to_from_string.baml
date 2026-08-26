// Tests for baml.json.to_string / from_string<T>.
//
// Converted from crates/baml_tests/tests/json_to_from_string.rs.
//
// Tests that read struct fields through `let` locals are moved into helper
// functions to avoid the test-block local-boxing VM bug (field access on a
// stored local throws `expected Instance, got Any`).
//
// decode_error_path_points_at_missing_field uses `assert.contains(err.path, ...)`
// (the std `assert.contains` helper) to check the JsonDecodeError path.

// ─── Simple class round-trip ──────────────────────────────────────────────────

class User { name string  age int }

test "to_string_simple_class" {
    assert.equal(
        baml.json.to_string(User { name: "Ada", age: 30 }),
        "{\"name\":\"Ada\",\"age\":30}",
    )
}

function from_string_simple_class_name() -> string {
    let u: User = baml.json.from_string<User>("{\"name\":\"Ada\",\"age\":30}");
    u.name
}

test "from_string_simple_class" {
    assert.equal(from_string_simple_class_name(), "Ada")
}

function from_string_infers_simple_class_name() -> string {
    let u: User = baml.json.from_string("{\"name\":\"Ada\",\"age\":30}");
    u.name
}

test "from_string_infers_simple_class" {
    assert.equal(from_string_infers_simple_class_name(), "Ada")
}

function to_string_infers_simple_class() -> string {
    baml.json.to_string(User { name: "Ada", age: 30 })
}

test "to_string_infers_simple_class" {
    assert.equal(to_string_infers_simple_class(), "{\"name\":\"Ada\",\"age\":30}")
}

function roundtrip_simple_age() -> int {
    let u: User = User { name: "Ada", age: 30 };
    let s: string = baml.json.to_string(u);
    let v: User = baml.json.from_string<User>(s);
    v.age
}

test "roundtrip_simple" {
    assert.equal(roundtrip_simple_age(), 30)
}

// ─── Composite generic ─────────────────────────────────────────────────────────

class Container<T> { item T }

function composite_generic_roundtrip_name() -> string {
    let c: Container<User> = Container<User> { item: User { name: "Ada", age: 30 } };
    let s: string = baml.json.to_string(c);
    let d: Container<User> = baml.json.from_string<Container<User>>(s);
    d.item.name
}

test "composite_generic_roundtrip" {
    assert.equal(composite_generic_roundtrip_name(), "Ada")
}

// ─── Generic forwarding (mirrors reflect_type_of_generic.rs) ─────────────────

function fetch_as<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    baml.json.from_string<T>(s)
}

function generic_forwarding_user_name() -> string {
    let u: User = fetch_as<User>("{\"name\":\"Ada\",\"age\":30}");
    u.name
}

test "generic_forwarding_user" {
    assert.equal(generic_forwarding_user_name(), "Ada")
}

function generic_forwarding_composite_name() -> string {
    let c: Container<User> = fetch_as<Container<User>>("{\"item\":{\"name\":\"Ada\",\"age\":30}}");
    c.item.name
}

test "generic_forwarding_composite" {
    assert.equal(generic_forwarding_composite_name(), "Ada")
}

// ─── Three-level forwarding ───────────────────────────────────────────────────

class UserSimple { name string }

function level1<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    baml.json.from_string<T>(s)
}
function level2<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    level1<T>(s)
}
function level3<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    level2<T>(s)
}

function three_level_name() -> string {
    let u: UserSimple = level3<UserSimple>("{\"name\":\"Ada\"}");
    u.name
}

test "three_level_forwarding" {
    assert.equal(three_level_name(), "Ada")
}

// ─── Closure captures type-arg ────────────────────────────────────────────────

function make_decoder<T>() -> (string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    return (s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError { baml.json.from_string<T>(s) }
}

function closure_captures_type_arg_name() -> string {
    let f = make_decoder<UserSimple>();
    let u: UserSimple = f("{\"name\":\"Ada\"}");
    u.name
}

test "closure_captures_type_arg" {
    assert.equal(closure_captures_type_arg_name(), "Ada")
}

// ─── Enum round-trip ──────────────────────────────────────────────────────────

enum Color { Red Blue }

test "enum_variant_to_string" {
    assert.equal(baml.json.to_string(Color.Red), "\"Red\"")
}

function enum_variant_from_string_name() -> string {
    let c: Color = baml.json.from_string<Color>("\"Blue\"");
    match (c) {
        Color.Blue => "Blue"
        _ => "other"
    }
}

test "enum_variant_from_string" {
    assert.equal(enum_variant_from_string_name(), "Blue")
}

// ─── Optional field round-trip ────────────────────────────────────────────────

class Profile { nickname string? }

test "optional_field_present" {
    assert.equal(
        baml.json.to_string(Profile { nickname: "ada" }),
        "{\"nickname\":\"ada\"}",
    )
}

test "optional_field_null" {
    assert.equal(baml.json.to_string(Profile { nickname: null }), "{\"nickname\":null}")
}

function optional_field_decode_missing_is_null() -> bool {
    let p: Profile = baml.json.from_string<Profile>("{}");
    p.nickname == null
}

test "optional_field_decode_missing" {
    assert.is_true(optional_field_decode_missing_is_null())
}

// ─── Recursive class round-trip ───────────────────────────────────────────────

class Tree { value int  children Tree[] }

function recursive_class_roundtrip_value() -> int {
    let t: Tree = Tree {
        value: 1,
        children: [
            Tree { value: 2, children: [] },
            Tree { value: 3, children: [Tree { value: 4, children: [] }] },
        ]
    };
    let s: string = baml.json.to_string(t);
    let d: Tree = baml.json.from_string<Tree>(s);
    d.children[1].children[0].value
}

test "recursive_class_roundtrip" {
    assert.equal(recursive_class_roundtrip_value(), 4)
}

// ─── Error paths ──────────────────────────────────────────────────────────────

test "parse_throws_on_bad_json" {
    {
        let u: User = baml.json.from_string<User>("{[");
        baml.sys.panic("expected from_string to throw")
    } catch (e) {
        baml.json.JsonParseError => null
    }
}

test "decode_throws_on_missing_field" {
    {
        let u: User = baml.json.from_string<User>("{}");
        baml.sys.panic("expected from_string to throw")
    } catch (e) {
        baml.json.JsonDecodeError => null
    }
}

test "serialize_throws_uint8array" {
    {
        let b: Bad = Bad { x: baml.Uint8Array.from_hex("00ff") };
        baml.json.to_string(b);
        baml.sys.panic("expected to_string to throw")
    } catch (e) {
        baml.json.JsonSerializationError => null
    }
}

// Decode-error path includes the offending field's dotted path.  The check is
// done inside a helper to avoid the test-block local-boxing bug when operating
// on a caught error via a local.  JsonDecodeError carries a dedicated `path`
// field (separate from `message`) that holds the dotted field path.

class Inner { value int }
class Outer { inner Inner }

function check_decode_error_path_contains_field() -> bool {
    {
        let u: Outer = baml.json.from_string<Outer>("{\"inner\":{}}");
        false
    } catch (e) {
        let err: baml.json.JsonDecodeError => {
            assert.contains(err.path, ".inner.value");
            true
        }
    }
}

test "decode_error_path_points_at_missing_field" {
    assert.is_true(check_decode_error_path_contains_field())
}

// ─── Enum round-trip (combined) ──────────────────────────────────────────────

function enum_variant_roundtrip_eq() -> bool {
    let c: Color = Color.Blue;
    let s: string = baml.json.to_string(c);
    let d: Color = baml.json.from_string<Color>(s);
    d == Color.Blue
}

test "enum_variant_roundtrip" {
    assert.is_true(enum_variant_roundtrip_eq())
}

// ─── Media field round-trip ──────────────────────────────────────────────────

class ProfileMedia { avatar image }

test "media_field_to_string" {
    assert.equal(
        baml.json.to_string(ProfileMedia { avatar: image.from_url("https://example.com/a.png", "image/png") }),
        "{\"avatar\":{\"kind\":\"image\",\"source\":\"url\",\"value\":\"https://example.com/a.png\",\"mime\":\"image/png\"}}",
    )
}

function media_field_roundtrip_str() -> string {
    let p: ProfileMedia = ProfileMedia { avatar: image.from_url("https://example.com/a.png", "image/png") };
    let s: string = baml.json.to_string(p);
    let d: ProfileMedia = baml.json.from_string<ProfileMedia>(s);
    baml.json.to_string(d)
}

test "media_field_roundtrip" {
    assert.equal(
        media_field_roundtrip_str(),
        "{\"avatar\":{\"kind\":\"image\",\"source\":\"url\",\"value\":\"https://example.com/a.png\",\"mime\":\"image/png\"}}",
    )
}

// ─── Field-level attributes ──────────────────────────────────────────────────

class ProfileAlias { display_name string @alias("displayName") }

test "alias_field_uses_raw_name_in_json" {
    assert.equal(
        baml.json.to_string(ProfileAlias { display_name: "Ada" }),
        "{\"display_name\":\"Ada\"}",
    )
}

function alias_field_decodes_raw_name_val() -> string {
    let d: ProfileAlias = baml.json.from_string<ProfileAlias>("{\"display_name\":\"Ada\"}");
    d.display_name
}

test "alias_field_decodes_raw_name" {
    assert.equal(alias_field_decodes_raw_name_val(), "Ada")
}

class ProfileSkip {
    name string
    internal_id string @skip
}

class Bad { x uint8array }

test "skip_field_still_emitted_in_json" {
    assert.equal(
        baml.json.to_string(ProfileSkip { name: "Ada", internal_id: "secret" }),
        "{\"name\":\"Ada\",\"internal_id\":\"secret\"}",
    )
}

// ─── json passthrough via TypeAlias ──────────────────────────────────────────

function from_string_json_passthrough_int_match() -> bool {
    let j: json = baml.json.from_string<json>("42");
    match (j) {
        let n: int => true
        _ => false
    }
}

test "from_string_json_passthrough_int" {
    assert.is_true(from_string_json_passthrough_int_match())
}

function from_string_json_passthrough_object_len() -> int {
    let j: json = baml.json.from_string<json>("{\"x\":7}");
    match (j) {
        let m: map<string, json> => m.length()
        _ => -1
    }
}

test "from_string_json_passthrough_object" {
    assert.equal(from_string_json_passthrough_object_len(), 1)
}

function to_string_json_passthrough_str() -> string {
    let j: json = baml.json.parse("[1,2,3]");
    baml.json.to_string(j)
}

test "to_string_json_passthrough" {
    assert.equal(to_string_json_passthrough_str(), "[1,2,3]")
}
