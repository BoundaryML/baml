// BEP-066 R-3 executable oracles: one LLM render context cannot assign two
// non-equivalent runtime definitions to the same displayed qualified name,
// and non-regular recursive generics are rejected before the render walk can
// diverge. Ported from the Rust `runtime_render_identity.rs` suite; the one
// host-boundary test (an uncatchable `baml.errors.RenderPrompt` escaping
// `output_format_with`, which declares `throws never`) remains in Rust.

client IdentityClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function Render<T>() -> T {
    client: IdentityClient
    prompt: `${ctx.output_format}`
}

test "non-equivalent same-name runtime classes fail before render" {
    let left = reflect.class.new("Collision", { "left": type.of<int>() });
    let right = reflect.class.new("Collision", { "right": type.of<string>() });
    let combined = reflect.union.new([left.as_type(), right.as_type()]);
    let rendered = Render$render_prompt<unreflect(combined.as_type())>() catch (e) {
        baml.reflect.errors.CompilationError => {
            `${e.diagnostics[0].code}|${e.diagnostics[0].message}`
        },
        _ => "wrong render error",
    };
    if let message: string = rendered {
        assert.equal(
            message,
            "E0162|type `Collision` has non-equivalent definitions in the same LLM render context",
        )
    } else {
        assert.is_true(false)
    }
}

test "recursive and equivalent same-name runtime classes still render" {
    let node = reflect.class.builder("Node");
    node.field("value", type.of<int>());
    node.field("next", node.type().optional());
    let node_t = node.build();
    let recursive_prompt = Render$render_prompt<unreflect(node_t.as_type())>().text();
    assert.is_true(recursive_prompt.includes("Node"));

    let left = reflect.class.new("Collision", { "value": type.of<int>() });
    let right = reflect.class.new("Collision", { "value": type.of<int>() });
    let equivalent = reflect.union.new([left.as_type(), right.as_type()]);
    let equivalent_prompt = Render$render_prompt<unreflect(equivalent.as_type())>().text();
    assert.is_true(equivalent_prompt.includes("value"))
}

class Chain<T> {
    next: Chain<Chain<T>>,
}

test "non-regular recursive generic fails before render" {
    let rendered = Render$render_prompt<Chain<int>>().text() catch (e) {
        baml.reflect.errors.CompilationError => {
            `${e.diagnostics[0].code}|${e.diagnostics[0].message}`
        },
        _ => "wrong render error",
    };
    assert.equal(
        rendered,
        "E0162|non-regular recursive generic class `runtime_render_identity.Chain` expands from `runtime_render_identity.Chain<int>` to `runtime_render_identity.Chain<runtime_render_identity.Chain<int>>` and cannot be rendered as an LLM output schema",
    )
}

interface Wrapped<T> {
    value: T,
}

class ChainWrapped<T> {
    next: ChainWrapped<Wrapped<T>>,
}

// The bounded specialization analysis must run BEFORE the structural
// interface walk, which keys every realization and would diverge here.
test "interface-wrapped recursive generic fails before interface walk" {
    let rendered = Render$render_prompt<ChainWrapped<int>>() catch (e) {
        baml.reflect.errors.CompilationError => {
            `${e.diagnostics[0].code}|${e.diagnostics[0].message}`
        },
    };
    if let message: string = rendered {
        assert.equal(
            message,
            "E0162|non-regular recursive generic class `runtime_render_identity.ChainWrapped` expands from `runtime_render_identity.ChainWrapped<int>` to `runtime_render_identity.ChainWrapped<runtime_render_identity.Wrapped<int>>` and cannot be rendered as an LLM output schema",
        )
    } else {
        assert.is_true(false)
    }
}

class Box<T> {
    value: T,
    @@alias("Container")
}

class Crate<T> {
    value: T,
    @@alias("Container")
}

class Both {
    boxed: Box<int>,
    crated: Crate<int>,
}

test "colliding non-hoisted generic aliases render inline" {
    let rendered = Render$render_prompt<Both>().text();
    assert.is_true(rendered.includes("boxed:"));
    assert.is_true(rendered.includes("crated:"));
    assert.is_true(!rendered.includes("Container<int>"))
}

class Step<A, B> {
    next: Step<B[], int>,
}

test "finite transformed recursion reaches an exact cycle" {
    let rendered = Render$render_prompt<Step<string, bool>>().text();
    assert.is_true(rendered.includes("Step<int[], int>"))
}

interface OpenValue {
    value: string,
}

class BoxPlain<T> {
    value: T,
}

class Envelope {
    concrete: BoxPlain<int>,
    open: BoxPlain<OpenValue>,
}

test "open interface in second generic specialization is rejected" {
    let rendered = Render$render_prompt<Envelope>() catch (e) {
        baml.reflect.errors.CompilationError => {
            `${e.diagnostics[0].code}|${e.diagnostics[0].message}`
        },
    };
    if let message: string = rendered {
        assert.equal(
            message,
            "E0161|field `runtime_render_identity.Envelope.open.value` has open interface type `runtime_render_identity.OpenValue`, which cannot be rendered as an LLM output schema",
        )
    } else {
        assert.is_true(false)
    }
}

class BoxRec<T> {
    value: T,
    next: BoxRec<T>?,
    @@alias("Sleeve")
}

function RenderPair<T, U>() -> BoxRec<T> | BoxRec<U> {
    client: IdentityClient
    prompt: `${ctx.output_format}`
}

test "runtime type arguments with the same display name remain distinct" {
    let left = reflect.class.new("Arg", { "value": type.of<int>() });
    let right = reflect.class.new("Arg", { "value": type.of<int>() });
    let rendered = RenderPair$render_prompt<unreflect(left.as_type()), unreflect(right.as_type())>() catch (e) {
        baml.reflect.errors.CompilationError => {
            `${e.diagnostics[0].code}|${e.diagnostics[0].message}`
        },
    };
    if let message: string = rendered {
        assert.equal(
            message,
            "E0162|classes `runtime_render_identity.BoxRec<Arg>` and `runtime_render_identity.BoxRec<Arg>` both render as `Sleeve<Arg>` in the same LLM render context",
        )
    } else {
        assert.is_true(false)
    }
}
