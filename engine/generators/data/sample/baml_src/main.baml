class Example {
    type "example_1" @stream.not_null
    a int @check(a_is_positive, {{this > 0}})
    b string

    // This doesn't work :(
    // @@stream.with_state
}

class Example2 {
    type "example_2" @stream.not_null
    item Example
    element string
    element2 string
}

client<llm> Thinking {
    provider "anthropic"
    options {
        model "claude-sonnet-4-20250514"
        thinking {
            type enabled
            budget_tokens 1024
        }
        max_tokens 2048
    }
}

function Foo(x: int) -> Example2 | Example {
    client Thinking
    prompt #"
        Fill out this data model with some examples.

        {{ ctx.output_format }}

        use {{ x }} somewhere in the data model
    "#
}

// use flipped unions to ensure order doesn't matter for code-gen
function Bar(x: int) -> Example | Example2 {
    client "openai/gpt-4o-mini"
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
