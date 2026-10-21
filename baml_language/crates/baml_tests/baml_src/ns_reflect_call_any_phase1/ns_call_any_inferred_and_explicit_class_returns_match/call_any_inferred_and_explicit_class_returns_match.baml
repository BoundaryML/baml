class Result {
    value: string,
}

function make_result() -> Result throws never {
    Result { value: "hello" }
}

function inferred() -> Result throws never {
    reflect.call_any(make_result, {}) catch (e) {
        _ => Result { value: "inferred error" }
    }
}

function explicit() -> Result throws never {
    reflect.call_any<Result, never>(make_result, {}) catch (e) {
        _ => Result { value: "explicit error" }
    }
}

function main() -> string throws never {
    inferred().value + "|" + explicit().value
}

test "call_any_inferred_and_explicit_class_returns_match" {
    assert.equal(main(), "hello|hello")
}
