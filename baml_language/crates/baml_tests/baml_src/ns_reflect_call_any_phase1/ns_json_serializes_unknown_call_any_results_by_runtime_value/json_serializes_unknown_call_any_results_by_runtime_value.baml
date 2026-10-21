class Record {
    name: string,
}

class Wrapped {
    value: int,
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            { "overridden": baml.json.from(self.value) }
        }
    }
}

enum State {
    Ready,
}

function make_record() -> Record throws never {
    Record { name: "Ada" }
}

function make_wrapped_list() -> Wrapped[] throws never {
    [Wrapped { value: 1 }]
}

function make_state() -> State throws never {
    State.Ready
}

function make_bigint() -> bigint throws never {
    99999999999999999999n
}

function make_map() -> map<string, int> throws never {
    { "x": 1 }
}

function make_image() -> image throws never {
    image.from_url("https://example.com/a.png", "image/png")
}

function serialize_unknown(f: reflect.AnyFunction) -> string {
    let value: unknown = reflect.call_any(f, {});
    baml.json.to_string(value) + "|" + baml.json.stringify(baml.json.to_json(value))
}

function main() -> string {
    serialize_unknown(make_record)
        + "\n"
        + serialize_unknown(make_wrapped_list)
        + "\n"
        + serialize_unknown(make_state)
        + "\n"
        + serialize_unknown(make_bigint)
        + "\n"
        + serialize_unknown(make_map)
        + "\n"
        + serialize_unknown(make_image)
}

test "json_serializes_unknown_call_any_results_by_runtime_value" {
    assert.equal(
        main(),
        "{\"name\":\"Ada\"}|{\"name\":\"Ada\"}\n[{\"overridden\":1}]|[{\"overridden\":1}]\n\"Ready\"|\"Ready\"\n99999999999999999999|99999999999999999999\n{\"x\":1}|{\"x\":1}\n{\"kind\":\"image\",\"source\":\"url\",\"value\":\"https://example.com/a.png\",\"mime\":\"image/png\"}|{\"kind\":\"image\",\"source\":\"url\",\"value\":\"https://example.com/a.png\",\"mime\":\"image/png\"}",
    )
}
