client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

function Extract<T>() -> T {
    client: TestClient
    prompt: `${ctx.output_format()}`
}

function main() -> string {
    let frozen = reflect.class.builder("FrozenEmployee");
    frozen.field("name", reflect.Type.of<string>());
    frozen.build();
    let frozen_error = frozen.field("age", reflect.Type.of<int>()) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };

    let unresolved = reflect.class.builder("UnbuiltTenant");
    let pending = unresolved.type();
    let pending_error = {
        type Pending = unreflect(pending)
        Extract@render_prompt<Pending>()
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };

    let left = "missing frozen error";
    if (frozen_error is string) {
        left = frozen_error
    }
    let right = "missing pending error";
    if (pending_error is string) {
        right = pending_error
    }
    return left + "|" + right;
}

test "frozen_mutation_and_unresolved_call_name_the_builder" {
    let result = main();
    assert.contains(result, "`FrozenEmployee` is frozen");
    assert.contains(result, "`UnbuiltTenant`");
    assert.contains(result, "call build()")
}
