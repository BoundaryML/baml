function compiled_messages() -> string {
    let pkg = reflect.Package.compile({ "items.baml": `class Item { value string, count int }` });
    let item_type = (pkg.get_class("root.Item") ?? throw "missing Item").as_type();
    type Item = unreflect(item_type)

    let sap = baml.sap.parse<Item>("null") catch (e) {
        baml.errors.ParseError => e.message,
        _ => "not a ParseError",
    };
    let sap_message = if (sap is string) {
        sap
    } else {
        "sap accepted a null"
    };
    let decoded = baml.json.from_string<Item>("[1, 2]") catch (e) {
        baml.json.DecodeError => e.message,
        _ => "not a DecodeError",
    };
    let decode_message = if (decoded is string) {
        decoded
    } else {
        "decode accepted a list"
    };
    `${sap_message}~${decode_message}`
}

function minted_messages() -> string {
    let item_type = reflect.class
        .new("Item", { "value": reflect.Type.of<string>(), "count": reflect.Type.of<int>() })
        .as_type();
    type Item = unreflect(item_type)

    let sap = baml.sap.parse<Item>("null") catch (e) {
        baml.errors.ParseError => e.message,
        _ => "not a ParseError",
    };
    let sap_message = if (sap is string) {
        sap
    } else {
        "sap accepted a null"
    };
    let decoded = baml.json.from_string<Item>("[1, 2]") catch (e) {
        baml.json.DecodeError => e.message,
        _ => "not a DecodeError",
    };
    let decode_message = if (decoded is string) {
        decoded
    } else {
        "decode accepted a list"
    };
    `${sap_message}~${decode_message}`
}

test "a_coercion_error_names_a_runtime_class_as_its_source_spelled_it" {
    let compiled = compiled_messages().split("~");
    let minted = minted_messages().split("~");
    assert.contains(compiled[0], "Expected user.Item");
    assert.contains(minted[0], "Expected Item")
}

test "a_decode_error_names_a_runtime_class_as_its_source_spelled_it" {
    let compiled = compiled_messages().split("~")[1];
    let minted = minted_messages().split("~")[1];
    assert.is_true(!compiled.includes("$dyn") && !minted.includes("$dyn"));
    assert.contains(compiled, "expected JSON object for class `Item`");
    assert.contains(minted, "expected JSON object for class `Item`")
}
