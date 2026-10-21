// A spec's target is the function it was made from, even when a parameter
// shares the function's name.
function Summarize(Summarize: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Summarize: ${Summarize}`
}

function target_kind(target: unknown) -> string {
    match (target) {
        let s: string => `the argument ${s}`,
        _ => "the function",
    }
}

test "a parameter named like its function does not become the spec's target" {
    assert.equal(target_kind(Summarize@spec("xyz").target()), "the function")
}
