client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

type AnyCallable = reflect.AnyFunction<Returns = unknown, Throws = unknown>;

function Plain(topic: string) -> string {
    client: TestClient
    prompt: `
        Say ${topic}.
        ${ctx.output_format()}
    `
}

function main() -> bool {
    let package = reflect.Package.current();
    let callable = package.get_function<AnyCallable>(
        "root.reflect_call_any_phase1.call_any_still_invokes_a_non_generic_companion.Plain@render_prompt",
    )
        ?? throw "expected the companion";
    let rendered = reflect.call_any<unknown, unknown>(callable, { "topic": "hello" });
    rendered is ai.Prompt
}

test "call_any_still_invokes_a_non_generic_companion" {
    assert.equal(main(), true)
}
