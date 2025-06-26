class Example {
    a int
    b string

    // This doesn't work :(
    // @@stream.with_state
}

class Example2 {
    item Example
    element string
    element2 string
}

function Foo(x: int) -> Example2 | Example {
    client "ollama/phi4:latest"
    prompt #"
        Fill out this data model with some examples.

        {{ ctx.output_format }}

        use {{ x }} somewhere in the data model
    "#
}

// use flipped unions to ensure order doesn't matter for code-gen
function Bar(x: int) -> Example | Example2 {
    client "ollama/phi4:latest"
    prompt #"
        Fill out this data model with some examples.

        {{ ctx.output_format }}

        use {{ x }} somewhere in the data model
    "#
}

test FooTest {
    functions [Foo]
    args {
        x 1
    }
}
