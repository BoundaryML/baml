// `baml.json.schema`: what each type form lowers to, which values are left
// out because they have no JSON form, and which types are rejected outright.

function schema_text(t: reflect.Type) -> string {
    baml.json.stringify(baml.json.schema(t))
}

function schema_error(t: reflect.Type) -> string {
    {
        baml.json.schema(t);
        "lowered"
    } catch (e) {
        let err: baml.errors.Unsupported => err.message,
    }
}

// ─── Primitives ───────────────────────────────────────────────────────────────

test "an int schema carries the int range" {
    assert.equal(
        schema_text(int.type()),
        "{\"type\":\"integer\",\"minimum\":-4611686018427387904,\"maximum\":4611686018427387903}",
    )
}

test "a bigint is a decimal string" {
    assert.equal(
        schema_text(bigint.type()),
        "{\"type\":\"string\",\"pattern\":\"^-?(0|[1-9][0-9]*)$\"}",
    )
}

class SchemaBigintLiteral {
    exact: 42n,
}

test "a bigint literal is its decimal string" {
    let schema = baml.json.schema(SchemaBigintLiteral.type());
    assert.equal(
        baml.json.stringify(baml.json.path<json>(schema, ".properties.exact")),
        "{\"type\":\"string\",\"const\":\"42\"}",
    )
}

test "unknown is rejected" {
    assert.contains(schema_error(reflect.Type.of<unknown>()), "`unknown` admits every value")
}

// ─── Types with no JSON form ──────────────────────────────────────────────────

test "a uint8array alone has no schema" {
    assert.contains(schema_error(uint8array.type()), "`uint8array` has no JSON form")
}

test "a union drops a member with no JSON form" {
    assert.equal(
        schema_text(reflect.Type.of<uint8array | int | string>()),
        "{\"anyOf\":[{\"type\":\"integer\",\"minimum\":-4611686018427387904,\"maximum\":4611686018427387903},{\"type\":\"string\"}]}",
    )
}

test "an optional uint8array is only null" {
    assert.equal(schema_text(reflect.Type.of<uint8array?>()), "{\"type\":\"null\"}")
}

test "a list of uint8array has no schema" {
    assert.contains(schema_error(reflect.Type.of<uint8array[]>()), "`uint8array` has no JSON form")
}

test "a map of uint8array has no schema" {
    assert.contains(
        schema_error(reflect.Type.of<map<string, uint8array>>()),
        "`uint8array` has no JSON form",
    )
}

test "a list of never is only the empty list" {
    assert.equal(schema_text(reflect.Type.of<never[]>()), "{\"type\":\"array\",\"maxItems\":0}")
}

test "a map of never is only the empty object" {
    assert.equal(
        schema_text(reflect.Type.of<map<string, never>>()),
        "{\"type\":\"object\",\"additionalProperties\":false}",
    )
}

class SchemaBlob {
    name: string,
    bytes: uint8array,
}

class SchemaMaybeBlob {
    name: string,
    bytes: uint8array?,
}

test "a class that requires a uint8array has no schema" {
    assert.contains(
        schema_error(SchemaBlob.type()),
        "field `json_schema.SchemaBlob.bytes`: `uint8array` has no JSON form",
    )
}

test "a union drops a class that requires a uint8array" {
    assert.equal(schema_text(reflect.Type.of<SchemaBlob | string>()), "{\"type\":\"string\"}")
}

test "a class with an optional uint8array keeps the field as null" {
    let schema = baml.json.schema(SchemaMaybeBlob.type());
    assert.equal(
        baml.json.stringify(baml.json.path<json>(schema, ".properties.bytes")),
        "{\"type\":\"null\"}",
    )
    assert.equal(baml.json.path<string[]>(schema, ".required"), ["name"])
}

// ─── Recursion ────────────────────────────────────────────────────────────────

class SchemaLoop {
    next: SchemaLoop,
}

class SchemaChain {
    value: int,
    next: SchemaChain?,
}

type SchemaNested = SchemaNested[]

test "a class whose every value is cyclic has no schema" {
    assert.contains(schema_error(SchemaLoop.type()), "every value of `json_schema.SchemaLoop` is cyclic")
}

test "a union drops a class whose every value is cyclic" {
    assert.equal(schema_text(reflect.Type.of<SchemaLoop | bool>()), "{\"type\":\"boolean\"}")
}

test "a class that can end its chain lowers" {
    let schema = baml.json.schema(SchemaChain.type());
    assert.equal(baml.json.path<string>(schema, ".properties.next.anyOf[0].$ref"), "#/$defs/json_schema.SchemaChain")
    assert.equal(baml.json.path<string>(schema, ".$defs[\"json_schema.SchemaChain\"].type"), "object")
}

test "a recursive alias of lists lowers" {
    let schema = baml.json.schema(reflect.Type.of<SchemaNested>());
    assert.equal(baml.json.path<string>(schema, ".$ref"), "#/$defs/json_schema.SchemaNested")
    assert.equal(
        baml.json.path<string>(schema, ".$defs[\"json_schema.SchemaNested\"].items.$ref"),
        "#/$defs/json_schema.SchemaNested",
    )
}

// ─── Maps ─────────────────────────────────────────────────────────────────────

test "string literal map keys become propertyNames" {
    assert.equal(
        schema_text(reflect.Type.of<map<"a" | "b", bool>>()),
        "{\"type\":\"object\",\"additionalProperties\":{\"type\":\"boolean\"},\"propertyNames\":{\"enum\":[\"a\",\"b\"]}}",
    )
}

enum SchemaKey {
    A
    B
}

test "an int map key is rejected" {
    assert.contains(schema_error(reflect.Type.of<map<int, bool>>()), "has no JSON object key form")
}

test "an enum map key is rejected" {
    assert.contains(schema_error(reflect.Type.of<map<SchemaKey, bool>>()), "has no JSON object key form")
}

// ─── Media ────────────────────────────────────────────────────────────────────

test "an image is the tagged media object" {
    let schema = baml.json.schema(image.type());
    assert.equal(baml.json.path<string>(schema, ".properties.kind.const"), "image")
    assert.equal(baml.json.path<string[]>(schema, ".properties.source.enum"), ["url", "base64"])
    assert.equal(baml.json.path<string[]>(schema, ".required"), ["kind", "source", "value", "mime"])
}

// ─── Definition names ─────────────────────────────────────────────────────────

test "two classes with one name get distinct definitions" {
    let left = reflect.Class.new("SchemaTwin", { "left": int.type() });
    let right = reflect.Class.new("SchemaTwin", { "right": string.type() });
    let holder = reflect.Class.new("SchemaTwins", { "first": left, "second": right });
    let type Holder = holder;
    let schema = baml.json.schema(reflect.Type.of<Holder>());
    assert.equal(baml.json.path<string>(schema, ".properties.first.$ref"), "#/$defs/SchemaTwin")
    assert.equal(baml.json.path<string>(schema, ".properties.second.$ref"), "#/$defs/SchemaTwin_2")
    assert.equal(baml.json.path<string>(schema, ".$defs.SchemaTwin.properties.left.type"), "integer")
    assert.equal(baml.json.path<string>(schema, ".$defs.SchemaTwin_2.properties.right.type"), "string")
}

// ─── Naming ───────────────────────────────────────────────────────────────────

enum SchemaTone {
    Calm @alias("calm")
    Loud
    Hidden @skip
}

class SchemaNamed {
    title: string @alias("headline"),
    tone: SchemaTone,
    note: string @skip,
    extra: string? @skip,
}

class SchemaSkipsImage {
    label: string,
    picture: image @skip,
}

test "declared names ignore alias and skip" {
    let schema = baml.json.schema(SchemaNamed.type());
    assert.equal(baml.json.path<map<string, json>>(schema, ".properties").keys(), ["title", "tone", "note", "extra"])
    assert.equal(baml.json.path<string[]>(schema, ".required"), ["title", "tone", "note"])
    assert.equal(baml.json.path<string[]>(schema, ".properties.tone.enum"), ["Calm", "Loud", "Hidden"])
}

test "aliased names follow alias and leave out skip" {
    let schema = baml.json.schema(SchemaNamed.type(), naming = baml.json.Naming.Aliased);
    assert.equal(baml.json.path<map<string, json>>(schema, ".properties").keys(), ["headline", "tone"])
    assert.equal(baml.json.path<string[]>(schema, ".required"), ["headline", "tone"])
    assert.equal(baml.json.path<string[]>(schema, ".properties.tone.enum"), ["calm", "Loud"])
}

test "a skipped field with no empty value has no aliased schema" {
    assert.contains(
        {
            baml.json.schema(SchemaSkipsImage.type(), naming = baml.json.Naming.Aliased);
            "lowered"
        } catch (e) {
            let err: baml.errors.Unsupported => err.message,
        },
        "field `json_schema.SchemaSkipsImage.picture` is `@skip`",
    )
    // Declared names carry the field, so the image is read as JSON.
    assert.equal(
        baml.json.path<string>(baml.json.schema(SchemaSkipsImage.type()), ".properties.picture.properties.kind.const"),
        "image",
    )
}

function decode_named_aliased() -> string {
    let named = baml.json.from_string<SchemaNamed>(
        `{"headline": "Hi", "tone": "calm", "note": "ignored", "extra": "ignored"}`,
        naming = baml.json.Naming.Aliased,
    );
    `${named.title}|${named.tone}|[${named.note}]|${named.extra ?? "null"}`
}

test "aliased decoding reads aliases and fills skipped fields with empty values" {
    assert.equal(decode_named_aliased(), "Hi|Calm|[]|null")
}

function decode_named_declared() -> string {
    let named = baml.json.from_string<SchemaNamed>(
        `{"title": "Hi", "tone": "Hidden", "note": "kept", "extra": "kept"}`,
    );
    `${named.title}|${named.tone}|${named.note}|${named.extra ?? "null"}`
}

test "declared decoding reads declared names, skipped members included" {
    assert.equal(decode_named_declared(), "Hi|Hidden|kept|kept")
}

function decode_error(text: string, aliased: bool) -> string {
    let naming = if (aliased) {
        baml.json.Naming.Aliased
    } else {
        baml.json.Naming.Declared
    };
    {
        baml.json.from_string<SchemaNamed>(text, naming = naming);
        "decoded"
    } catch (e) {
        let err: baml.json.DecodeError => err.message,
        let err: baml.json.ParseError => err.message,
    }
}

test "aliased decoding rejects a declared name and a skipped variant" {
    assert.contains(decode_error(`{"title": "Hi", "tone": "Calm"}`, true), "missing required field `headline`")
    assert.contains(decode_error(`{"headline": "Hi", "tone": "Hidden"}`, true), "unknown variant `Hidden`")
    assert.contains(decode_error(`{"headline": "Hi", "tone": "Calm"}`, true), "unknown variant `Calm`")
}

test "declared decoding rejects an alias" {
    assert.contains(decode_error(`{"headline": "Hi", "tone": "Calm", "note": ""}`, false), "missing required field `title`")
}

function decode_skips_image() -> string {
    {
        baml.json.from_string<SchemaSkipsImage>(`{"label": "x"}`, naming = baml.json.Naming.Aliased);
        "decoded"
    } catch (e) {
        let err: baml.json.DecodeError => err.message,
    }
}

test "aliased decoding rejects a skipped field with no empty value" {
    assert.contains(decode_skips_image(), "field `json_schema.SchemaSkipsImage.picture` is `@skip`")
}

// ─── Decoding recursive aliases, functions and null fields ────────────────────

class SchemaLeaf {
    value: int,
}

type SchemaTree = SchemaLeaf | SchemaTree[]

function decode_tree_leaf_kind() -> string {
    let tree = baml.json.from_string<SchemaTree>(`[[{"value": 1}], {"value": 2}]`);
    match (tree) {
        let items: SchemaTree[] => {
            match (items[1]) {
                let leaf: SchemaLeaf => `leaf ${leaf.value}`,
                _ => "not a leaf",
            }
        },
        _ => "not a list",
    }
}

test "a recursive alias decodes as its definition" {
    assert.equal(decode_tree_leaf_kind(), "leaf 2")
}

function decode_tree_mismatch() -> string {
    {
        baml.json.from_string<SchemaTree>(`[{"value": "two"}]`);
        "decoded"
    } catch (e) {
        let err: baml.json.DecodeError => "DecodeError",
    }
}

test "a recursive alias rejects a value its definition does not admit" {
    assert.equal(decode_tree_mismatch(), "DecodeError")
}

type SchemaThunk = () -> int throws never

function decode_thunk_or_string() -> string {
    match (baml.json.from_string<SchemaThunk | string>(`"x"`)) {
        let s: string => `string ${s}`,
        _ => "a function",
    }
}

test "a function member does not take a JSON value" {
    assert.equal(decode_thunk_or_string(), "string x")
}

class SchemaNullField {
    nothing: null,
    name: string,
}

function decode_null_field() -> string {
    let value = baml.json.from_string<SchemaNullField>(`{"name": "n"}`);
    `${value.name}|${value.nothing ?? "absent"}`
}

test "a null field may be absent" {
    assert.equal(decode_null_field(), "n|absent")
}
