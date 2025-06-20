class Person {
    name string @assert(valid_name, {{ this|length >= 2 }})
    age int @assert(valid_age, {{ this >= 0 }})

    @@assert(not_minor, {{
        this.age >= 18
    }})
}

function PersonTest() -> Person {
    client "openai/gpt-4o"
    prompt #"
        Fake random information about an adult person.

        {{ ctx.output_format }}
    "#
}

test PersonTest {
    functions [PersonTest]
    args {
        output_format: "json"
    }
}
