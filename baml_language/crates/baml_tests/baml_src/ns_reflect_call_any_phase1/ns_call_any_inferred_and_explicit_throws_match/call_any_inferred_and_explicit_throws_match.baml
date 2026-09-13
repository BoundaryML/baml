class CallError {
    message: string,
}

function fail() -> string throws CallError {
    throw CallError { message: "boom" }
}

function inferred() -> string throws never {
    reflect.call_any(fail, {}) catch (e) {
        CallError => "inferred:" + e.message,
        reflect.InvalidArgumentError => "inferred argument error",
        reflect.errors.CompilationError => "inferred compilation error",
    }
}

function explicit() -> string throws never {
    reflect.call_any<string, CallError>(fail, {}) catch (e) {
        CallError => "explicit:" + e.message,
        reflect.InvalidArgumentError => "explicit argument error",
        reflect.errors.CompilationError => "explicit compilation error",
    }
}

function main() -> string throws never {
    inferred() + "|" + explicit()
}

test "call_any_inferred_and_explicit_throws_match" {
    assert.equal(main(), "inferred:boom|explicit:boom")
}
