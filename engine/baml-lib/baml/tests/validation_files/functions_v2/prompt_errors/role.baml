function Foo() -> string {
    client "openai/gpt-4o"
    prompt #"
        You are a helpful assistant.
        {{ ctx.output_format }}

        {{ _.role("assistant", cache_control={"type": "ephemeral"}) }}
        Hi
    "#
}