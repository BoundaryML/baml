// Tests for baml.json.to_string / from_string<T>.
//
// Converted from crates/baml_tests/tests/json_to_from_string.rs.
//
// Tests that read struct fields through `let` locals are moved into helper
// functions to avoid the test-block local-boxing VM bug (field access on a
// stored local throws `expected Instance, got Any`).
//
// decode_error_path_points_at_missing_field uses `assert.contains(err.path, ...)`
// (the std `assert.contains` helper) to check the baml.json.DecodeError path.

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

class BigintRecord { id bigint  exact 42n }

function bigint_record_json_roundtrip_is_exact() -> bool {
    let expected = 123456789012345678901234567890n;
    let record = baml.json.from_string<BigintRecord>(
        "{\"id\":123456789012345678901234567890,\"exact\":42}",
    );
    record.id == expected && record.exact == 42n
}

function bigint_record_numeric_strings_decode_exactly() -> bool {
    let expected = 123456789012345678901234567890n;
    let record = baml.json.from_string<BigintRecord>(
        "{\"id\":\"123456789012345678901234567890\",\"exact\":\"42\"}",
    );
    record.id == expected && record.exact == 42n
}

test "bigint_and_literal_bigint_json_roundtrip" {
    let large = 123456789012345678901234567890n;
    let encoded = baml.json.to_string(large);
    assert.equal(encoded, "123456789012345678901234567890");
    assert.equal(baml.json.from_string<bigint>(encoded), large);
    assert.equal(baml.json.from_string<42n>("42"), 42n);
    assert.is_true(bigint_record_json_roundtrip_is_exact());
    assert.is_true(bigint_record_numeric_strings_decode_exactly())
}

test "literal_bigint_json_rejects_wrong_value" {
    {
        let wrong: 42n = baml.json.from_string<42n>("43");
        baml.sys.panic("expected literal bigint mismatch")
    } catch (e) {
        baml.json.DecodeError => null
    };
    {
        let wrong: 42n = baml.json.from_string<42n>("\"43\"");
        baml.sys.panic("expected numeric-string literal bigint mismatch")
    } catch (e) {
        baml.json.DecodeError => null
    }
}

test "bigint_json_accepts_numeric_strings" {
    assert.equal(baml.json.from_string<bigint>("\"42\""), 42n);
    assert.equal(baml.json.from_string<bigint>("\"-42\""), -42n);
    assert.equal(baml.json.from_string<bigint>("\"+00042\""), 42n);
    assert.equal(baml.json.from_string<bigint>("\"-0\""), 0n);
    assert.equal(
        baml.json.from_string<bigint>("\"123456789012345678901234567890\""),
        123456789012345678901234567890n,
    );
    assert.equal(baml.json.from_string<42n>("\"42\""), 42n)
}

function bigint_numeric_string_collections_decode_exactly() -> bool {
    let items = baml.json.from_string<bigint[]>("[\"1\",\"-2\"]");
    let by_name = baml.json.from_string<map<string, bigint>>("{\"a\":\"9\"}");
    items.at(0) == 1n && items.at(1) == -2n && by_name.get("a") == 9n
}

test "bigint_numeric_string_collections" {
    assert.is_true(bigint_numeric_string_collections_decode_exactly())
}

function bigint_first_numeric_string_union_selects_bigint() -> bool {
    let value: bigint | string = baml.json.from_string<bigint | string>("\"42\"");
    match (value) {
        let n: bigint => n == 42n
        _ => false
    }
}

function string_first_numeric_string_union_selects_string() -> bool {
    let value: string | bigint = baml.json.from_string<string | bigint>("\"42\"");
    match (value) {
        let s: string => s == "42"
        _ => false
    }
}

test "bigint_numeric_string_union_order" {
    assert.is_true(bigint_first_numeric_string_union_selects_bigint());
    assert.is_true(string_first_numeric_string_union_selects_string())
}

test "bigint_json_rejects_fractions_exponents_and_non_numeric_strings" {
    {
        let wrong: bigint = baml.json.from_string<bigint>("12.5");
        baml.sys.panic("expected fractional JSON number to fail")
    } catch (e) {
        baml.json.DecodeError => null
    };
    {
        let wrong: bigint = baml.json.from_string<bigint>("1e3");
        baml.sys.panic("expected exponential JSON number to fail")
    } catch (e) {
        baml.json.DecodeError => null
    };
    {
        let wrong: bigint = baml.json.from_string<bigint>("\"12.5\"");
        baml.sys.panic("expected fractional numeric string to fail")
    } catch (e) {
        baml.json.DecodeError => null
    };
    {
        let wrong: bigint = baml.json.from_string<bigint>("\"1e3\"");
        baml.sys.panic("expected exponential numeric string to fail")
    } catch (e) {
        baml.json.DecodeError => null
    };
    {
        let wrong: bigint = baml.json.from_string<bigint>("\" 42\"");
        baml.sys.panic("expected whitespace-padded numeric string to fail")
    } catch (e) {
        baml.json.DecodeError => null
    }
}

test "all_bigint_decoders_accept_numeric_strings" {
    let j: json = baml.json.parse("\"+00042\"");
    assert.equal(baml.json.to<bigint>(j), 42n);
    assert.equal(baml.json.from_json<bigint>(j), 42n);
    assert.equal(baml.json.deserialize<bigint>("\"+00042\""), 42n);
    assert.equal(baml.json.to<42n>(j), 42n);
    assert.equal(baml.json.from_json<42n>(j), 42n);
    assert.equal(baml.json.deserialize<42n>("\"+00042\""), 42n);
    assert.equal(
        baml.json.to<bigint>("-123456789012345678901234567890"),
        -123456789012345678901234567890n,
    );
    assert.equal(baml.json.to<bigint>("-0"), 0n);
    assert.equal(baml.json.to_string(j), "\"+00042\"");
    assert.equal(baml.json.to<string>(j), "+00042")
}

function to_bigint_numeric_string_collections_decode_exactly() -> bool {
    let records = baml.json.to<BigintRecord[]>(
        baml.json.parse("[{\"id\":\"123456789012345678901234567890\",\"exact\":\"42\"}]"),
    );
    let record = records.at(0);
    if (record == null) {
        return false;
    }
    let items = baml.json.to<bigint[]>(baml.json.parse("[\"1\",\"-2\"]"));
    let by_name = baml.json.to<map<string, 42n>>(baml.json.parse("{\"a\":\"42\"}"));
    let optional = baml.json.to<bigint?>("42");
    record.id == 123456789012345678901234567890n
        && record.exact == 42n
        && items.at(0) == 1n
        && items.at(1) == -2n
        && by_name.get("a") == 42n
        && optional == 42n
}

test "to_bigint_numeric_string_collections" {
    assert.is_true(to_bigint_numeric_string_collections_decode_exactly());
    assert.equal(baml.json.to<bigint?>(null), null)
}

function to_bigint_first_numeric_string_union_selects_bigint() -> bool {
    let value = baml.json.to<bigint | string>("42");
    match (value) {
        let n: bigint => n == 42n,
        _ => false,
    }
}

function to_string_first_numeric_string_union_selects_string() -> bool {
    let value = baml.json.to<string | bigint>("42");
    match (value) {
        let s: string => s == "42",
        _ => false,
    }
}

test "to_bigint_numeric_string_union_order" {
    assert.is_true(to_bigint_first_numeric_string_union_selects_bigint());
    assert.is_true(to_string_first_numeric_string_union_selects_string())
}

test "to_bigint_rejects_invalid_strings_and_literal_mismatches" {
    for (let text in ["", "12.5", "1e3", " 42", "0x2a", "hello"]) {
        {
            let wrong = baml.json.to<bigint>(text);
            baml.sys.panic("expected invalid bigint string to fail")
        } catch (e) {
            baml.json.DecodeError => null
        }
    }
    ;
    {
        let wrong = baml.json.to<42n>("43");
        baml.sys.panic("expected numeric-string literal bigint mismatch")
    } catch (e) {
        baml.json.DecodeError => null
    }
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

function fetch_as<T>(s: string) -> T throws baml.json.ParseError | baml.json.DecodeError {
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

function level1<T>(s: string) -> T throws baml.json.ParseError | baml.json.DecodeError {
    baml.json.from_string<T>(s)
}
function level2<T>(s: string) -> T throws baml.json.ParseError | baml.json.DecodeError {
    level1<T>(s)
}
function level3<T>(s: string) -> T throws baml.json.ParseError | baml.json.DecodeError {
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

function make_decoder<T>() -> (string) -> T throws baml.json.ParseError | baml.json.DecodeError {
    return (s: string) -> T throws baml.json.ParseError | baml.json.DecodeError { baml.json.from_string<T>(s) }
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
        baml.json.ParseError => null
    }
}

test "decode_throws_on_missing_field" {
    {
        let u: User = baml.json.from_string<User>("{}");
        baml.sys.panic("expected from_string to throw")
    } catch (e) {
        baml.json.DecodeError => null
    }
}

test "serialize_throws_uint8array" {
    {
        let b: Bad = Bad { x: baml.Uint8Array.from_hex("00ff") };
        baml.json.to_string(b);
        baml.sys.panic("expected to_string to throw")
    } catch (e) {
        baml.json.SerializationError => null
    }
}

// Decode-error path includes the offending field's dotted path.  The check is
// done inside a helper to avoid the test-block local-boxing bug when operating
// on a caught error via a local.  baml.json.DecodeError carries a dedicated `path`
// field (separate from `message`) that holds the dotted field path.

class Inner { value int }
class Outer { inner Inner }

function check_decode_error_path_contains_field() -> bool {
    {
        let u: Outer = baml.json.from_string<Outer>("{\"inner\":{}}");
        false
    } catch (e) {
        let err: baml.json.DecodeError => {
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

function from_string_json_passthrough_numeric_string_match() -> bool {
    let j: json = baml.json.from_string<json>("\"42\"");
    match (j) {
        let s: string => s == "42"
        _ => false
    }
}

test "from_string_json_passthrough_numeric_string" {
    assert.is_true(from_string_json_passthrough_numeric_string_match())
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

// ─── deserialize<T> is from_json<T>(parse(s)) ────────────────────────────────
// `deserialize<T>` decodes an override-free `T` straight from the text, in one
// pass. These pin that what it returns and what it throws — class, message and
// `path` — are the composition's, on the documents where a one-pass structural
// walk is known to read differently: a missing field (reported at the class,
// not the field), a nullable payload that does not fit (the union's error, not
// the payload's), a list or map of the wrong shape, trailing text, duplicate
// keys, `-0`, a float that overflows, and a bigint past the limit.

class Pin { name string, age int?, score float }
type PinOrText = Pin | string

function deserialize_outcome<T>(s: string) -> string {
    let out = baml.json.deserialize<T>(s) catch (e) {
        baml.json.ParseError => "ParseError:" + e.message,
        baml.json.DecodeError => "DecodeError:" + e.message + "@" + e.path,
    };
    string.from(out)
}

function composed_outcome<T>(s: string) -> string {
    let out = baml.json.from_json<T>(baml.json.parse(s)) catch (e) {
        baml.json.ParseError => "ParseError:" + e.message,
        baml.json.DecodeError => "DecodeError:" + e.message + "@" + e.path,
    };
    string.from(out)
}

test "deserialize_matches_composition_for_classes" {
    for (let s in [
        "{\"name\":\"Ada\",\"score\":1.5}",
        "{\"name\":\"Ada\",\"age\":null,\"score\":2}",
        "{\"name\":\"Ada\",\"age\":3,\"score\":2.50}",
        "{\"name\":\"Ada\",\"score\":-0}",
        "{\"name\":\"Ada\",\"score\":-0.0}",
        "{\"name\":\"Ada\",\"score\":1E2}",
        "{\"name\":\"Ada\",\"score\":1e400}",
        "{\"name\":\"Ada\",\"name\":\"Bob\",\"score\":1}",
        "{\"score\":1.5}",
        "{\"name\":\"Ada\",\"age\":\"x\",\"score\":1}",
        "{\"name\":\"Ada\",\"score\":\"1\"}",
        "{\"name\":\"Ada\",\"score\":1,\"extra\":[1,2]}",
        "[1, 2]",
        "null",
        "{\"name\":\"Ada\",\"score\":1.5} trailing",
        "{\"name\":\"Ada\",\"score\":1.5",
        "",
    ]) {
        assert.equal(deserialize_outcome<Pin>(s), composed_outcome<Pin>(s));
    }
}

test "deserialize_matches_composition_for_containers" {
    for (let s in [
        "[{\"name\":\"A\",\"score\":1},{\"name\":\"B\",\"score\":2}]",
        "[{\"name\":\"A\",\"score\":1},{\"name\":\"B\"}]",
        "[{\"name\":\"A\",\"score\":1},5]",
        "{}",
        "[]",
    ]) {
        assert.equal(deserialize_outcome<Pin[]>(s), composed_outcome<Pin[]>(s));
    }
    for (let s in [
        "{\"k\":{\"name\":\"A\",\"score\":1},\"j\":{\"name\":\"B\",\"score\":2}}",
        "{\"k\":5}",
        "{\"k\":{\"name\":\"A\",\"score\":1},\"k\":{\"name\":\"B\",\"score\":2}}",
        "[]",
        "{}",
    ]) {
        assert.equal(deserialize_outcome<map<string, Pin>>(s), composed_outcome<map<string, Pin>>(s));
    }
    for (let s in ["{\"a\":1,\"b\":2}", "{\"c\":1}", "{}"]) {
        assert.equal(deserialize_outcome<map<"a" | "b", int>>(s), composed_outcome<map<"a" | "b", int>>(s));
    }
}

test "deserialize_matches_composition_for_scalars_and_unions" {
    for (let s in ["-0", "-0.0", "0", "5", "1e400", "1E2", "2.50", "\"1\"", "null"]) {
        assert.equal(deserialize_outcome<float>(s), composed_outcome<float>(s));
        assert.equal(deserialize_outcome<json>(s), composed_outcome<json>(s));
        assert.equal(deserialize_outcome<int?>(s), composed_outcome<int?>(s));
        assert.equal(deserialize_outcome<int>(s), composed_outcome<int>(s));
    }
    for (let s in ["\"42\"", "12345678901234567890123", "-0", "1.5", "\"1e3\""]) {
        assert.equal(deserialize_outcome<bigint>(s), composed_outcome<bigint>(s));
    }
    for (let s in ["\"s\"", "{\"name\":\"A\",\"score\":1}", "{\"name\":\"A\"}", "5", "null"]) {
        assert.equal(deserialize_outcome<PinOrText>(s), composed_outcome<PinOrText>(s));
        assert.equal(deserialize_outcome<PinOrText?>(s), composed_outcome<PinOrText?>(s));
    }
    for (let s in ["\"a\"", "\"c\"", "1"]) {
        assert.equal(deserialize_outcome<"a" | "b">(s), composed_outcome<"a" | "b">(s));
        assert.equal(deserialize_outcome<42>(s), composed_outcome<42>(s));
    }
    for (let s in ["\"Blue\"", "\"Pink\"", "1"]) {
        assert.equal(deserialize_outcome<Color>(s), composed_outcome<Color>(s));
    }
    assert.equal(deserialize_outcome<json>("[1,2.50,{\"a\":null,\"b\":[true,\"x\"]}]"), composed_outcome<json>("[1,2.50,{\"a\":null,\"b\":[true,\"x\"]}]"));
}

// The composition's own readings, pinned so the comparisons above cannot drift
// together: a missing field is reported at the class; `-0` is the int `0`, so
// a float target reads `0.0`; a float that overflows reads as `null` and does
// not fit `float`; an array where a list was wanted is `expected JSON array`.
test "deserialize_reports_the_decoders_errors" {
    assert.equal(deserialize_outcome<Pin>("{\"score\":1.5}"), "DecodeError:missing required field `name`@");
    assert.equal(deserialize_outcome<Pin[]>("[{\"name\":\"A\",\"score\":1},{\"name\":\"B\"}]"), "DecodeError:missing required field `score`@[1]");
    assert.equal(deserialize_outcome<Pin[]>("{}"), "DecodeError:expected JSON array@");
    assert.equal(deserialize_outcome<map<string, Pin>>("[]"), "DecodeError:expected JSON object@");
    assert.equal(deserialize_outcome<map<string, Pin>>("{\"k\":{\"score\":1}}"), "DecodeError:missing required field `name`@[\"k\"]");
    assert.equal(deserialize_outcome<int?>("\"x\""), "DecodeError:expected integer@");
    assert.equal(deserialize_outcome<float>("-0"), "0.0");
    assert.equal(deserialize_outcome<float>("-0.0"), "-0.0");
    assert.equal(deserialize_outcome<float>("1e400"), "DecodeError:expected number@");
    assert.equal(deserialize_outcome<json>("1e400"), "null");
}
