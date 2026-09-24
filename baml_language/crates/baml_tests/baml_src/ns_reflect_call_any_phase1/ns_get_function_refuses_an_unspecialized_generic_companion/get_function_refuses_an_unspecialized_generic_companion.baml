client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

type PromptFn = (topic: string) -> ai.Prompt throws unknown;

function GenericList<T>(topic: string) -> T[] {
    client: TestClient
    prompt: `
        Return an empty list of ${topic}.
        ${ctx.output_format()}
    `
}

function main() -> string throws never {
    let package = reflect.Package.current();
    let callable: PromptFn = package.get_function<PromptFn>(
        "root.reflect_call_any_phase1.get_function_refuses_an_unspecialized_generic_companion.GenericList@render_prompt",
    ) catch (e) {
        reflect.errors.CompilationError => {
            return e.diagnostics[0].code + "|" + e.diagnostics[0].message;
        },
        _ => {
            return "wrong error";
        },
    } else {
        return "get_function returned null";
    };
    // Unreachable: the extraction above throws. Calling it directly is
    // what used to die inside the body as an uncatchable internal error.
    callable("items") catch_all (e) {
        _ => {
            return "direct call threw";
        },
    };
    "get_function did not throw"
}

test "get_function_refuses_an_unspecialized_generic_companion" {
    let result = main();
    assert.is_true(result.starts_with("E0165|"));
    assert.contains(result, "GenericList@render_prompt");
    assert.contains(result, "cannot be invoked through reflection: its body needs type arguments")
}
