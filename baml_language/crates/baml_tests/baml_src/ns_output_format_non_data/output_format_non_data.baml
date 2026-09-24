// Regression coverage for non-data types at the LLM output-schema boundary.
// The client points at a dead port; cases that invoke an LLM function must fail
// during schema construction, before any provider I/O.

client OutputFormatTestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function GenericList<T>(topic: string) -> T[] {
    client: OutputFormatTestClient
    prompt: `List ${topic}.\n${ctx.output_format()}`
}

function GenericValue<T>(topic: string) -> T {
    client: OutputFormatTestClient
    prompt: `Describe ${topic}.\n${ctx.output_format()}`
}

class Item {
    name: string,
    description: string?,
}

class Payload {
    value: unknown,
}

class Inner {
    payload: reflect.Type,
}

class Envelope {
    inner: Inner,
}

interface HiddenOpen {}

class SkipControl {
    visible: string,
    raw_blob: uint8array? @skip,
    dynamic: unknown @skip,
    open: HiddenOpen? @skip,
}

class Wrapper<T> {
    value: T,
}

type Callback = (int) -> int throws never;

test "static_never_specialization_throws_compilation_error" {
    let result = GenericList@render_prompt<never>("items") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(result, "E0164|non-data type `never` cannot be rendered as an LLM output schema")
}

test "runtime_never_specialization_uses_the_same_diagnostic" {
    let runtime_type = reflect.Type.of<never>();
    type RuntimeNever = unreflect(runtime_type)
    let result = GenericList@render_prompt<RuntimeNever>("items") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(result, "E0164|non-data type `never` cannot be rendered as an LLM output schema")
}

test "direct_llm_execution_fails_before_network_io" {
    let result = GenericList<never>("items") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(result, "E0164|non-data type `never` cannot be rendered as an LLM output schema")
}

test "ordinary_data_specialization_still_renders" {
    let rendered = GenericList@render_prompt<Item>("items").text();
    assert.contains(rendered, "name");
    assert.contains(rendered, "description")
}

test "unknown_class_field_reports_its_path" {
    let result = GenericValue@render_prompt<Payload>("payload") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(
        result,
        "E0164|field `output_format_non_data.Payload.value` has non-data type `unknown`, which cannot be rendered as an LLM output schema",
    )
}

test "nested_non_data_class_field_reports_the_full_path" {
    let result = GenericValue@render_prompt<Envelope>("envelope") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(
        result,
        "E0164|field `output_format_non_data.Envelope.inner.payload` has non-data type `reflect.Type`, which cannot be rendered as an LLM output schema",
    )
}

test "runtime_minted_nested_non_data_field_is_rejected" {
    let inner = reflect.class.new("RuntimeInner", { "payload": reflect.Type.of<unknown>() });
    let outer = reflect.class.new("RuntimeOuter", { "inner": inner.as_type() });
    type RuntimeOuterType = unreflect(outer.as_type())
    let result = GenericValue@render_prompt<RuntimeOuterType>("runtime") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(
        result,
        "E0164|field `RuntimeOuter.inner.payload` has non-data type `unknown`, which cannot be rendered as an LLM output schema",
    )
}

test "skipped_non_data_and_open_interface_fields_do_not_block_rendering" {
    let rendered = GenericValue@render_prompt<SkipControl>("visible data").text();
    assert.contains(rendered, "visible");
    assert.is_true(!rendered.includes("raw_blob"));
    assert.is_true(!rendered.includes("dynamic"));
    assert.is_true(!rendered.includes("open"))
}

test "generic_data_class_output_renders" {
    let rendered = GenericValue@render_prompt<Wrapper<Item>>("wrapped item").text();
    assert.contains(rendered, "name");
    assert.contains(rendered, "value")
}

test "generic_class_output_fails_before_provider_io" {
    let result = GenericValue<Wrapper<Callback>>("wrapped fn") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    if (result is string) {
        assert.is_true(result.starts_with("E0164|"));
        assert.contains(result, "non-data")
    } else {
        throw "call did not throw"
    }
}

test "never_nested_in_a_runtime_class_field_is_rejected_not_panicked" {
    let outer = reflect.class.new("RuntimeOuter", { "items": reflect.Type.of<never>().array().as_type() }) catch (e) {
        _ => throw "class.new threw",
    };
    type RuntimeOuterType = unreflect(outer.as_type())
    let result = GenericValue@render_prompt<RuntimeOuterType>("runtime") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(
        result,
        "E0164|field `RuntimeOuter.items` has non-data type `never`, which cannot be rendered as an LLM output schema",
    )
}
