// Regression coverage for non-data types at the LLM output-schema boundary.
// The client points at a dead port; cases that invoke an LLM function must fail
// during schema construction, before any provider I/O.
//
// A model's answer is JSON. A union member with no JSON form or no value is
// dropped from the schema, an optional field of such a type is only `null`,
// and a list of `never` is only the empty list. E0164 is for a whole output
// type a model cannot answer with at all.

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
    let result = GenericValue@render_prompt<never>("items") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(result, "E0164|non-data type `never` cannot be rendered as an LLM output schema")
}

test "runtime_never_specialization_uses_the_same_diagnostic" {
    let runtime_type = reflect.Type.of<never>();
    let type RuntimeNever = runtime_type;
    let result = GenericValue@render_prompt<RuntimeNever>("items") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(result, "E0164|non-data type `never` cannot be rendered as an LLM output schema")
}

test "direct_llm_execution_fails_before_network_io" {
    let result = GenericValue<never>("items") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(result, "E0164|non-data type `never` cannot be rendered as an LLM output schema")
}

test "a list of never renders as the empty list" {
    let rendered = GenericList@render_prompt<never>("items").text();
    assert.contains(rendered, "Answer with a JSON Array using this schema:\n[]")
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
    let inner = reflect.Class.new("RuntimeInner", { "payload": reflect.Type.of<unknown>() });
    let outer = reflect.Class.new("RuntimeOuter", { "inner": inner });
    let type RuntimeOuterType = outer;
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

test "a never list in a runtime class field renders as the empty list" {
    let outer = reflect.Class.new("RuntimeOuter", { "items": reflect.Type.of<never>().array() }) catch (e) {
        _ => throw "reflect.Class.new threw",
    };
    let type RuntimeOuterType = outer;
    let rendered = GenericValue@render_prompt<RuntimeOuterType>("runtime").text();
    assert.contains(rendered, "items: [],")
}

test "never_nested_in_a_runtime_class_field_is_rejected_not_panicked" {
    let outer = reflect.Class.new("RuntimeOuter", { "item": reflect.Type.of<never>() }) catch (e) {
        _ => throw "reflect.Class.new threw",
    };
    let type RuntimeOuterType = outer;
    let result = GenericValue@render_prompt<RuntimeOuterType>("runtime") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(
        result,
        "E0164|field `RuntimeOuter.item` has non-data type `never`, which cannot be rendered as an LLM output schema",
    )
}

// ─── Members with no JSON form are dropped ───────────────────────────────────

class Blob {
    bytes: uint8array,
}

class MaybeRaw {
    name: string,
    raw: uint8array?,
}

class Loop {
    next: Loop,
}

class HoldsSkippedType {
    name: string,
    kind: reflect.Type @skip,
}

interface OpenOutput {
    value: string,
}

function render_error<T>() -> string {
    {
        GenericValue@render_prompt<T>("x");
        "rendered"
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    }
}

test "a union member with no JSON form is dropped" {
    assert.equal(
        GenericValue@render_prompt<uint8array | string>("x").text(),
        GenericValue@render_prompt<string>("x").text(),
    )
}

test "a union drops a class that requires a uint8array" {
    assert.equal(
        GenericValue@render_prompt<Blob | string>("x").text(),
        GenericValue@render_prompt<string>("x").text(),
    )
}

test "an optional field with no JSON form renders as null" {
    let rendered = GenericValue@render_prompt<MaybeRaw>("x").text();
    assert.contains(rendered, "name: string,");
    assert.contains(rendered, "raw: null,")
}

test "an output with no JSON form at all is still rejected" {
    assert.equal(
        render_error<uint8array>(),
        "E0164|non-data type `uint8array` cannot be rendered as an LLM output schema",
    );
    assert.equal(
        render_error<Blob>(),
        "E0164|field `output_format_non_data.Blob.bytes` has non-data type `uint8array`, which cannot be rendered as an LLM output schema",
    );
    assert.equal(
        render_error<Loop>(),
        "E0164|every value of `output_format_non_data.Loop` is cyclic, and JSON holds only finite values, so it cannot be rendered as an LLM output schema",
    );
    assert.equal(
        render_error<HoldsSkippedType>(),
        "E0164|the `@skip` field `output_format_non_data.HoldsSkippedType.kind` has type `reflect.Type`, which has no empty value for it to hold, so its class cannot be rendered as an LLM output schema",
    )
}

test "unknown and open interfaces are rejected even beside a member that renders" {
    assert.equal(
        render_error<unknown>(),
        "E0164|non-data type `unknown` cannot be rendered as an LLM output schema",
    );
    assert.equal(
        render_error<unknown | string>(),
        "E0164|non-data type `unknown` cannot be rendered as an LLM output schema",
    );
    assert.equal(
        render_error<OpenOutput>(),
        "E0161|field `output` has open interface type `output_format_non_data.OpenOutput`, which cannot be rendered as an LLM output schema",
    )
}

// A unit output carries no information for a model to produce, however the
// unit type is spelled. `null` stays data inside a larger type.
type NothingAtAll = void

function ReturnsNothing(topic: string) -> void {
    client: OutputFormatTestClient
    prompt: `Describe ${topic}.`
}

function unit_output_error<T>() -> string {
    {
        GenericValue@render_prompt<T>("nothing");
        "no error"
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    }
}

test "unit_output_is_rejected_under_every_spelling" {
    let expected = "E0164|non-data type `null` cannot be rendered as an LLM output schema";
    assert.equal(unit_output_error<void>(), expected);
    assert.equal(unit_output_error<null>(), expected);
    assert.equal(unit_output_error<NothingAtAll>(), expected)
}

test "unit_returning_llm_function_fails_before_network_io" {
    let result = {
        ReturnsNothing("nothing");
        "no error"
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong error",
    };
    assert.equal(result, "E0164|non-data type `null` cannot be rendered as an LLM output schema")
}

test "null_inside_an_output_type_still_renders" {
    let rendered = GenericValue@render_prompt<string?>("something").text();
    assert.contains(rendered, "null")
}
