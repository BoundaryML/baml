class JsonUnionRecord {
    name: string,
}

enum JsonUnionPhase {
    Ready,
}

type JsonUnionPayload = JsonUnionRecord | JsonUnionPhase | image;

type Payload = uint8array | string;

function serialize_json_union_payload(value: JsonUnionPayload) -> string {
    baml.json.stringify(value)
}

test "typed union preserves class enum and media values" {
    let actual = serialize_json_union_payload(JsonUnionRecord { name: "Ada" })
        + "|"
        + serialize_json_union_payload(JsonUnionPhase.Ready)
        + "|"
        + serialize_json_union_payload(image.from_url("https://example.com/a.png", "image/png"));
    assert.equal(
        actual,
        `{"name":"Ada"}|"Ready"|{"kind":"image","source":"url","value":"https://example.com/a.png","mime":"image/png"}`,
    )
}

class JsonNestedDetail {
    label: string,
}

class JsonNestedRecord {
    name: string,
    detail: JsonNestedDetail,
}

type JsonNestedPayload = JsonNestedRecord | string;

test "typed class and union serialization are equivalent" {
    let record = JsonNestedRecord { name: "Ada", detail: JsonNestedDetail { label: "nested" } };
    let payload: JsonNestedPayload = record;
    assert.equal(
        baml.json.stringify(record) + "|" + baml.json.stringify(payload),
        `{"name":"Ada","detail":{"label":"nested"}}|{"name":"Ada","detail":{"label":"nested"}}`,
    )
}

test "typed union preserves uint8array serialization error" {
    let result = {
        let payload: Payload = baml.Uint8Array.from_hex("00ff");
        let _ = baml.json.stringify(payload);
        "unexpected success"
    } catch (e) {
        baml.json.SerializationError => "serialization error",
    };
    assert.equal(result, "serialization error")
}
