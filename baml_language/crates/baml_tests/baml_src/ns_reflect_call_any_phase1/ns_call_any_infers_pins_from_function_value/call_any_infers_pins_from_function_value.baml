function plain(required: string) -> string throws never {
    required
}

function main() -> string throws never {
    reflect.call_any(plain, { "required": "hello" }) catch (e) {
        _ => "error"
    }
}

test "call_any_infers_pins_from_function_value" {
    assert.equal(main(), "hello")
}
