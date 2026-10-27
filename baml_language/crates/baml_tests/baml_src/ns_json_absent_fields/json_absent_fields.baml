// A class field may be missing from JSON exactly when `null` is a value of
// its type, and it then decodes to `null`. That is decided by what the type
// means: `T?`, `null`, `unknown`, and `json` (an alias whose union holds
// `null`) all admit it. The decoders, `schema`'s `required` list, and class
// literals agree on it.

class Required {
    id: string,
    payload: json,
}

class AbsentKinds {
    id: string @alias("key"),
    payload: json @alias("body"),
    extra: unknown,
    nothing: null,
    maybe: int?,
}

/// `Required` read three ways: `decode` from text, `to` from the parsed value,
/// and as the member a union tries.
function required_payloads(
    text: string,
) -> string throws baml.json.ParseError | baml.json.DecodeError | baml.json.SerializationError {
    let decoded = baml.json.decode<Required>(text);
    let converted = baml.json.to<Required>(baml.json.parse(text));
    let member = baml.json.decode<Required | int>(text);
    let member_payload = match (member) {
        let r: Required => baml.json.stringify(r.payload),
        _ => "int",
    };
    baml.json.stringify(decoded.payload)
        + ","
        + baml.json.stringify(converted.payload)
        + ","
        + member_payload
}

test "a missing json field decodes to null (B-1913)" {
    assert.equal(required_payloads("{\"id\":\"x\"}"), "null,null,null");
    assert.equal(required_payloads("{\"id\":\"x\",\"payload\":[1]}"), "[1],[1],[1]")
}

function absent_kinds(
    text: string,
    naming: baml.json.Naming,
) -> string throws baml.json.ParseError | baml.json.DecodeError | baml.json.SerializationError {
    let decoded = baml.json.decode<AbsentKinds>(text, naming = naming);
    let converted = baml.json.to<AbsentKinds>(baml.json.parse(text), naming = naming);
    baml.json.stringify(decoded) + "|" + baml.json.stringify(converted)
}

test "missing fields of every type that admits null decode to null" {
    let expected = "{\"id\":\"x\",\"payload\":null,\"extra\":null,\"nothing\":null,\"maybe\":null}";
    assert.equal(
        absent_kinds("{\"id\":\"x\"}", baml.json.Naming.Declared),
        expected + "|" + expected,
    );
    assert.equal(
        absent_kinds("{\"key\":\"x\"}", baml.json.Naming.Aliased),
        expected + "|" + expected,
    )
}

function missing_id(text: string, naming: baml.json.Naming) -> string {
    {
        let _ = baml.json.decode<AbsentKinds>(text, naming = naming);
        "decoded"
    } catch (e) {
        baml.json.DecodeError => e.message,
        baml.json.ParseError => "parse error",
    }
}

test "a field whose type does not admit null is still required" {
    assert.equal(missing_id("{}", baml.json.Naming.Declared), "missing required field `id`");
    assert.equal(
        missing_id("{\"id\":\"x\"}", baml.json.Naming.Aliased),
        "missing required field `key`",
    )
}

class AbsentOverride {
    label: string,
}

implement baml.FromJson for AbsentOverride {
    function from_json(j: json) -> AbsentOverride {
        AbsentOverride { label: "from override" }
    }
}

class AbsentWithOverride {
    payload: json,
    tag: AbsentOverride,
}

test "a missing json field decodes to null when an override is in reach" {
    let decoded = baml.json.decode<AbsentWithOverride>("{\"tag\":{}}");
    assert.equal(baml.json.stringify(decoded.payload), "null");
    assert.equal(decoded.tag.label, "from override")
}

// ─── omit_nulls round trips ──────────────────────────────────────────────────

class OmitNulls {
    name: string,
    payload: json,
    extra: unknown,
    nickname: string?,
    tags: map<string, json>?,
}

function round_trip(
    value: OmitNulls,
    naming: baml.json.Naming,
) -> string throws baml.json.ParseError | baml.json.DecodeError | baml.json.SerializationError {
    let text = baml.json.stringify(value, naming = naming, omit_nulls = true);
    let decoded = baml.json.decode<OmitNulls>(text, naming = naming);
    let converted = baml.json.to<OmitNulls>(
        baml.json.from(value, naming = naming, omit_nulls = true),
        naming = naming,
    );
    text + "|" + baml.json.stringify(decoded) + "|" + baml.json.stringify(converted)
}

test "a value written without its nulls reads back the same" {
    let value = OmitNulls { name: "Ada", payload: null, extra: null, nickname: null, tags: null };
    let full = "{\"name\":\"Ada\",\"payload\":null,\"extra\":null,\"nickname\":null,\"tags\":null}";
    assert.equal(
        round_trip(value, baml.json.Naming.Declared),
        "{\"name\":\"Ada\"}|" + full + "|" + full,
    );
    assert.equal(
        round_trip(value, baml.json.Naming.Aliased),
        "{\"name\":\"Ada\"}|" + full + "|" + full,
    )
}

// ─── schema and class literals agree ─────────────────────────────────────────

class SchemaAbsent {
    id: string,
    payload: json,
    maybe: int?,
}

test "a json field is not required in the schema" {
    let schema = baml.json.schema(SchemaAbsent.type());
    assert.equal(baml.json.path<string[]>(schema, ".required"), ["id"])
}

class LiteralAbsent {
    a: int,
    extra: unknown,
    payload: json,
}

test "a class literal may leave out unknown and json fields" {
    let literal = LiteralAbsent { a: 1 };
    assert.equal(baml.json.stringify(literal), "{\"a\":1,\"extra\":null,\"payload\":null}")
}
