enum OfOptionChoice {
    First,
    Second,
}

class OfOptionChild {
    value string
}

class OfOptionResult {
    choice OfOptionChoice
    child OfOptionChild
    attributes map<string, int>
    nullable string?
}

function OfAllLegacyOptions() -> OfOptionResult {
    client: "openai/gpt-4o-mini"
    tools: []
    prompt: `${ctx.output_format(
        prefix = null,
        or_splitter = " / ",
        enum_value_prefix = "* ",
        hoisted_class_prefix = "type",
        always_hoist_enums = true,
        quote_class_fields = true,
        hoist_classes = true,
        map_style = "angle",
        render_null_as = "nil",
    )}`
}

test "declarative output format supports every legacy option" {
    let rendered = OfAllLegacyOptions@spec().prompt().text()
    assert.contains(rendered, "output_format_options.OfOptionChoice\n----\n* First\n* Second")
    assert.contains(rendered, "type output_format_options.OfOptionChild {")
    assert.contains(rendered, "type output_format_options.OfOptionResult {")
    assert.contains(rendered, "\"attributes\": map<string, int>,")
    assert.contains(rendered, "\"nullable\": string / nil,")
    assert.is_true(rendered.includes("Answer in JSON") == false)
}

function of_context() -> ai.Context {
    let return_type = reflect.Type.of<OfOptionResult>()
    ai.Context {
        client: ai.ContextClient {
            name: "test",
            provider: "openai",
            default_role: "user",
            allowed_roles: ["user"],
        },
        tags: {},
        _output_format: ai.internal.build_output_format(return_type),
    }
}

function of_render_hoist_classes(
    hoist_classes: bool | "auto" | string[],
) -> string {
    let ctx = of_context()
    ctx.output_format(prefix = null, hoist_classes = hoist_classes)
}

test "output format preserves legacy hoist classes shapes" {
    let all = of_render_hoist_classes(true)
    let disabled = of_render_hoist_classes(false)
    let automatic = of_render_hoist_classes("auto")
    let subset = of_render_hoist_classes(["output_format_options.OfOptionChild"])

    assert.contains(all, "output_format_options.OfOptionChild {")
    assert.contains(all, "output_format_options.OfOptionResult {")
    assert.is_true(disabled.includes("output_format_options.OfOptionChild {") == false)
    assert.equal(disabled, automatic)
    assert.contains(subset, "output_format_options.OfOptionChild {")
    assert.is_true(subset.includes("output_format_options.OfOptionResult {") == false)
}

test "output format distinguishes omitted options from explicit null" {
    let defaults = of_context().output_format(always_hoist_enums = true)
    let without_prefixes = of_context().output_format(
        prefix = null,
        enum_value_prefix = null,
        always_hoist_enums = true,
    )

    assert.contains(defaults, "Answer in JSON")
    assert.contains(defaults, "\n- First")
    assert.is_true(without_prefixes.includes("Answer in JSON") == false)
    assert.contains(without_prefixes, "\nFirst")
    assert.is_true(without_prefixes.includes("\n- First") == false)
}
