// BEP-062: AnyFunction coercion failures. The derived conformance checks the
// function's return against the Returns pin and its throws against the
// Throws pin (both covariant); non-functions never conform; existential
// widening only goes narrow-to-wide.

class ToolError {
    message string
}

class Doc {
    title string
}

function web_search(q: string) -> Doc[] throws ToolError {
    if q == "boom" {
        throw ToolError { message: "no results" }
    }
    return [Doc { title: q }]
}

function main() -> int throws never {
    let bad_ret: baml.AnyFunction<Returns = string> = web_search
    let bad_throws: baml.AnyFunction<Throws = never> = web_search
    let not_a_fn: baml.AnyFunction = 42
    let narrow: baml.AnyFunction<Returns = Doc[]> = web_search
    let bad_widen: baml.AnyFunction<Returns = string> = narrow
    return 0
}
