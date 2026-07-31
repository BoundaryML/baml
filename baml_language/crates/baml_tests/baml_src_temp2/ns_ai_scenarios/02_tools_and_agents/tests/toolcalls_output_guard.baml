// D6d guard: declaring a task whose output type IS ai.tools.ToolCalls is
// refused up front with a clear error, instead of silently burning the whole
// step limit on unknown-tool errors (audit F12).

function PlanCallsTrap(topic: string) -> ai.tools.ToolCalls {
    provider: fake_tool_provider([])
    prompt: `
        Plan the calls you would make about ${topic}.
        ${ctx.output_format}
    `
}

test "D6d: Task<ai.tools.ToolCalls> is refused with a clear error" {
    let kind = PlanCallsTrap@task("duplicate charges")
        .run(runner = ai.run.Agent<ai.tools.ToolCalls>.new())
        .to_string() catch (e) {
        let invalid: ai.InvalidRequest => {
            assert.is_true(invalid.detail.includes("output type cannot be ai.tools.ToolCalls"));
            "refused"
        },
        _ => "wrong-channel",
    };
    assert.equal(kind, "refused")
}
