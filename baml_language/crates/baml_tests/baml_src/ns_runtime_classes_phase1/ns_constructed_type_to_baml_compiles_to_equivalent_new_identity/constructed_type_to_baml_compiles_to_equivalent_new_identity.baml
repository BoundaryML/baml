client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function RoundTrip<T>() -> T {
    client: TestClient
    prompt: `${ctx.output_format()}`
}

function main() -> bool {
    let scores_t = reflect.map.new(reflect.Type.of<string>(), reflect.Type.of<int>().optional().as_type());
    let original = reflect.class.new(
        "RoundTripRecord",
        {
            "kind": reflect.literal.new("visit").as_type().meta(
                alias = "wire_kind",
                description = "dispatch key",
            ),
            "tags": reflect.Type.of<string>().array().as_type().meta(description = "ordered labels"),
            "scores": scores_t.as_type(),
        },
    );
    let source = original.as_type().to_baml();
    let package = reflect.Package.compile({ "round_trip.baml": source });
    let compiled = package.get_class("root.RoundTripRecord") ?? throw "missing compiled RoundTripRecord";
    let document = `
        {
                        "wire_kind": "visit",
                        "tags": ["urgent", "review"],
                        "scores": {"priority": 7, "followup": null}
                    }
    `;
    type Original = unreflect(original.as_type())
    type Compiled = unreflect(compiled.as_type())
    let original_value = RoundTrip@parse<Original>(document);
    let compiled_value = RoundTrip@parse<Compiled>(document);

    return original.as_type() != compiled.as_type()
        && source == compiled.as_type().to_baml()
        && RoundTrip@render_prompt<Original>().text() == RoundTrip@render_prompt<Compiled>().text()
        && baml.json.to_string(original_value) == baml.json.to_string(compiled_value)
        && reflect.Type.of_value(original_value) == original.as_type()
        && reflect.Type.of_value(compiled_value) == compiled.as_type();
}

test "constructed_type_to_baml_compiles_to_equivalent_new_identity" {
    assert.equal(main(), true)
}
