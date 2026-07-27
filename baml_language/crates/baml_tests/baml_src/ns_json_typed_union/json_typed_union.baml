class Record {
    name: string,
}

enum Phase {
    Ready,
}

type Payload = Record | Phase | image;

function serialize_payload(value: Payload) -> string {
    baml.json.to_string<Payload>(value)
}

function typed_union_preserves_class_enum_and_media_values_fn() -> string {
    serialize_payload(Record { name: "Ada" })
        + "|"
        + serialize_payload(Phase.Ready)
        + "|"
        + serialize_payload(image.from_url("https://example.com/a.png", "image/png"))
}

class Detail {
    label: string,
}

class NestedRecord {
    name: string,
    detail: Detail,
}

type NestedPayload = NestedRecord | string;

function typed_class_and_union_serialization_are_equivalent_fn() -> string {
    let record = NestedRecord { name: "Ada", detail: Detail { label: "nested" } };
    baml.json.to_string<NestedRecord>(record) + "|" + baml.json.to_string<NestedPayload>(record)
}

type BytesPayload = uint8array | string;

function typed_union_preserves_uint8array_serialization_error_fn() -> string {
    {
        let _ = baml.json.to_string<BytesPayload>(baml.Uint8Array.from_hex("00ff"));
        "unexpected success"
    } catch (e) {
        baml.json.JsonSerializationError => "serialization error"
    }
}

test "typed_union_preserves_class_enum_and_media_values" {
    assert
        .equal(
            typed_union_preserves_class_enum_and_media_values_fn(),
            #"{"name":"Ada"}|"Ready"|{"kind":"image","source":"url","value":"https://example.com/a.png","mime":"image/png"}"#,
        )
}

test "typed_class_and_union_serialization_are_equivalent" {
    assert
        .equal(
            typed_class_and_union_serialization_are_equivalent_fn(),
            #"{"name":"Ada","detail":{"label":"nested"}}|{"name":"Ada","detail":{"label":"nested"}}"#,
        )
}

test "typed_union_preserves_uint8array_serialization_error" {
    assert.equal(typed_union_preserves_uint8array_serialization_error_fn(), "serialization error")
}
