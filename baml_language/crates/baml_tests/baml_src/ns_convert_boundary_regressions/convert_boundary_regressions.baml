class NonLiteralError {
    message: string,
}

function make_non_literal_error() -> NonLiteralError {
    NonLiteralError { message: "x" }
}

function throw_call_result() -> int {
    throw make_non_literal_error()
}

function call_throw_call_result() -> int {
    throw_call_result()
}

function throw_binary_result() -> int {
    throw 1 + 2
}

function call_throw_binary_result() -> int {
    throw_binary_result()
}

function throw_array_result() -> int {
    throw [1, 2, 3]
}

function call_throw_array_result() -> int {
    throw_array_result()
}

test "throw of non-literal expressions compiles through bytecode lowering" {
    assert.is_true(true)
}

class GenericBox<T> {
    value: T,
}

client Dummy = openai.ChatClient.new(model = "gpt-4")

function GenericExtract<T>(text: string) -> GenericBox<T> {
    client: Dummy
    prompt: `x`
}

test "generic LLM function with a generic return compiles" {
    assert.is_true(true)
}
