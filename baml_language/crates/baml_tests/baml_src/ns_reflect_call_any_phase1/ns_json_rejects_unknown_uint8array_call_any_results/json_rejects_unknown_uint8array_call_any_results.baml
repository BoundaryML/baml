function make_bytes() -> uint8array {
    baml.Uint8Array.from_hex("00ff")
}

function to_string_error(value: unknown) -> string {
    {
        let _ = baml.json.to_string(value);
        "unexpected success"
    } catch (e) {
        let err: baml.json.SerializationError => err.message,
    }
}

function to_json_error(value: unknown) -> string {
    {
        let _ = baml.json.to_json(value);
        "unexpected success"
    } catch (e) {
        let err: baml.json.SerializationError => err.message,
    }
}

function main() -> string {
    let f: reflect.AnyFunction = make_bytes;
    let value: unknown = reflect.call_any(f, {});
    to_string_error(value) + "|" + to_json_error(value)
}

test "json_rejects_unknown_uint8array_call_any_results" {
    assert.equal(
        main(),
        "uint8array requires explicit encoding (use to_base64() or to_hex())|uint8array requires explicit encoding (use to_base64() or to_hex())",
    )
}
