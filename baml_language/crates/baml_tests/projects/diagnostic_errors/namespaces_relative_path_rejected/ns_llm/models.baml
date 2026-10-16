class Response {
    text string
}

function make() -> int {
    1
}

// From `llm`, the child namespace `llm.vendor` is no closer than any other:
// `vendor.Box` is not a path to it.
function child_namespace() -> int {
    vendor.Box<int> { value: 2 }.value + vendor.unbox()
}

// A bare name still reaches this namespace's own items.
function bare() -> int {
    Response { text: "y" }.text.length() + make()
}
