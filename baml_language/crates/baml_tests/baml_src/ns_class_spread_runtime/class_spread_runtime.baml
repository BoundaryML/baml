// Runtime regressions for exact-class spread lowering.
//
// Converted from crates/bex_vm/tests/class_spread_runtime.rs, where each case
// compiled a source string and ran a no-arg `bool` function through the real VM.
// Those entry points survive as top-level helpers, so each assertion is
// `assert.is_true(<helper>())`: a test-block local is boxed (VM quirk), so
// nothing is bound in a test block here.
//
// The corpus is one project, so every class and function carries a `Csr` /
// `csr_` prefix to stay clear of the other namespaces.

// ─── Spread of a generic factory result ─────────────────────────────────────
// The spread must preserve the callable field and the last field of the
// spread-from instance while the explicit `value:` entry overrides it.

class CsrTask<T> {
    value: T,
    transform: (T) -> T throws never,
    events: string[],
}

function csr_defaults<T>(value: T, transform: (T) -> T throws never) -> CsrTask<T> {
    CsrTask<T> { value: value, transform: transform, events: ["created"] }
}

function csr_generic_spread() -> bool {
    let task = CsrTask {
        ...csr_defaults<int>(1, (value: int) -> int { value + 1 }),
        value: 41,
    };
    task.transform(task.value) == 42 && task.events == ["created"]
}

test "generic_spread_preserves_callable_and_last_field" {
    assert.is_true(csr_generic_spread())
}

// ─── Spread of a factory call with omitted defaulted parameters ─────────────
// The factory is called with only its required arguments, so the defaulted
// parameters are filled by the callee; the caller's frame must survive that
// fill and the spread that consumes the result.

class CsrTool {
    name: string,
}

class CsrOptions {
    budget: int,
    tools: CsrTool[],
    registry: CsrTool?,
    hook: CsrTool?,
    observers: CsrTool[],
    dispatch: (int[]) -> int[] throws never,
}

function csr_tool() -> CsrTool {
    CsrTool { name: "knowledge" }
}

function csr_dispatch(values: int[]) -> int[] throws never {
    values
}

function csr_option_defaults(
    tools: CsrTool[],
    callback: (int[]) -> int[] throws never,
    registry: CsrTool? = null,
    hook: CsrTool? = null,
    observers: CsrTool[] = [],
) -> CsrOptions {
    CsrOptions {
        budget: 5,
        tools: tools,
        registry: registry,
        hook: hook,
        observers: observers,
        dispatch: callback,
    }
}

function csr_spread_with_omitted_defaults() -> bool {
    let options = CsrOptions {
        ...csr_option_defaults([csr_tool()], csr_dispatch),
        budget: 1,
    };
    options.budget == 1 &&
        options.dispatch([1]).length() == 1 &&
        options.registry == null &&
        options.hook == null
}

test "spread_factory_call_with_omitted_defaults_preserves_caller_frame" {
    assert.is_true(csr_spread_with_omitted_defaults())
}
