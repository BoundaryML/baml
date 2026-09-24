client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function Extract<T>() -> T {
    client: TestClient
    prompt: `${ctx.output_format()}`
}

function main() -> string {
    let t = reflect.class.new("OneField", { "count": reflect.Type.of<int>() });
    type OneField = unreflect(t.as_type())
    let value = Extract@parse<OneField>(`{"count": 4}`);
    let missing = reflect.class.get_field<int>(value, "absent") catch (e) {
        reflect.errors.CompilationError => {
            e.diagnostics[0].code + ":" + e.diagnostics[0].message
        }
    };
    let wrong = reflect.class.get_field<string>(value, "count") catch (e) {
        reflect.errors.CompilationError => {
            e.diagnostics[0].code + ":" + e.diagnostics[0].message
        }
    };
    let missing_text = "missing read did not throw";
    if (missing is string) {
        missing_text = missing
    }
    let wrong_text = "wrong read did not throw";
    if (wrong is string) {
        wrong_text = wrong
    }
    return missing_text + "|" + wrong_text;
}

test "get_field_missing_and_wrong_type_throw_compilation_diagnostics" {
    let result = main();
    assert.is_true(result.starts_with("E0001:class `OneField` has no field `absent`|E0001:"));
    assert.contains(result, "field `OneField.count` has type `int`, expected `string`")
}
