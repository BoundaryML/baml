class SimpleClass {
    digits int
    words string  @stream.with_state
}

function MakeSimpleClass() -> SimpleClass {
    client "openai/gpt-4o-mini"
    prompt #"
        {{ ctx.output_format }}
    "#
}

function ConsumeSimpleClass(item: SimpleClass) -> SimpleClass {
    client "openai/gpt-4o-mini"
    prompt #"
        Return back to me verbatim:

        {{ item }}
    "#
}

test MakeSimpleClassTest {
    functions [MakeSimpleClass]
    args {}
}
