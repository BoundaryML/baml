// Scoped runtime type bindings: `type T = unreflect(t)` is the one position
// that lifts a runtime type into a type, and `T` is a rigid parameter of the
// frame for the rest of its block. These tests pin that the bound name works
// everywhere the removed inline `f<unreflect(t)>()` spelling used to be
// written, and that the programs it produces run.

class Holder<T> { label string }

class Wrapper<T> { inner T }

function wrap<T>(v: T) -> Wrapper<T> throws never { Wrapper<T> { inner: v } }

function ident<T>(v: T) -> T throws never { v }

// A value of a rigid `T` comes from an explicit `match` downcast, never from
// a literal.
function as_t<T>(v: unknown) -> T throws string {
    match (v) {
        let x: T => x,
        _ => throw "not a T",
    }
}

test "the bound name works in every position the inline spelling had" {
    let t = reflect.Type.of<int>()
    type Out = unreflect(t)
    let literal = Holder<Out> { label: "h" }
    let wrapped = wrap<Out>(as_t<Out>(1))
    assert.equal(literal.label, "h")
    assert.is_true(reflect.Type.of_value(wrapped.inner) == t)
    assert.is_true(reflect.Type.of<Out>() == t)
}

// `reflect.Type.of<Wrapper<T>>()` over a scoped `T` loads the frame slot the
// binding filled, so it is the same type as the static spelling.
test "type.of over a scoped binding realizes the bound type" {
    let t = reflect.Type.of<string>()
    type T = unreflect(t)
    assert.is_true(reflect.Type.of<Wrapper<T>>() == reflect.Type.of<Wrapper<string>>())
}

// A static right-hand side binds `T` to its template in the same frame slot
// a runtime one uses; `T` is just as opaque statically.
test "a static right-hand side realizes its template" {
    type T = Wrapper<string>
    assert.is_true(reflect.Type.of<T>() == reflect.Type.of<Wrapper<string>>())
    assert.is_true(reflect.Type.of<T[]>() == reflect.Type.of<Wrapper<string>[]>())
}

// A scoped `T` in a type pattern is a membership test against the bound type,
// through the ordinary `is`/`match` road.
test "a scoped binding in a pattern tests membership against the bound type" {
    let t = reflect.Type.of<string>()
    type T = unreflect(t)
    let value: unknown = Wrapper<string> { inner: "ok" }
    let text: unknown = "plain"
    assert.is_true(value is Wrapper<T>)
    assert.is_true(!(text is Wrapper<T>))
    assert.is_true(text is T)
    assert.is_true(match (value) {
        Wrapper<T> => true,
        _ => false,
    })
}

class Source {
    function pick<T>(self, v: T) -> T throws never { v }
}

// Every construct that transports a call's result, on one scoped runtime
// type, asking each value what class it actually carries: the tag rides on
// the value, so all five agree.
function transports() -> string {
    let t = reflect.Type.of<string>()
    type T = unreflect(t)
    let src = Source {}
    let direct = ident<T>(as_t<T>("a"))
    let awaited = await spawn { ident<T>(as_t<T>("b")) }
    let caught = ident<T>(as_t<T>("c")) catch (e) { _ => 0 }
    let collected = [ident<T>(as_t<T>("d"))]
    let chained = src?.pick<T>(as_t<T>("e"))
    `${reflect.Type.of_value(direct).to_string()} ${reflect.Type.of_value(awaited).to_string()} ${reflect.Type.of_value(caught).to_string()} ${reflect.Type.of_value(collected[0]).to_string()} ${reflect.Type.of_value(chained).to_string()}`
}

test "every transport of a result keeps the runtime type" {
    assert.equal(transports(), "string string string string string")
}

// The error channel: a callee whose `throws` names the parameter compiles and
// runs with a caller that declares no clause of its own.
class Boom<T> { payload T }

function risky<T>(v: T, fail: bool) -> int throws Boom<T> {
    if (fail) { throw Boom<T> { payload: v } }
    0
}

// A test body's own `throws` clause is inferred, so a `Boom<Out>` it would
// publish is caught inside the block instead.
test "a scoped runtime type in the error channel runs" {
    let t = reflect.Type.of<string>()
    type Out = unreflect(t)
    let outcome = risky<Out>(as_t<Out>("x"), false) catch (e) {
        Boom<Out> => -1,
        string => -2,
    }
    assert.equal(outcome, 0)
}

// A declared `throws unknown` is the nearest relaxation of a thrown `Boom<T>`
// or `T[]` (their arguments are invariant, so nothing closer than `unknown`
// is free of `T`), so the clause is precise, not extraneous. The thrown value
// keeps its runtime type.
function throws_boom(t: reflect.Type) -> int throws unknown {
    type T = unreflect(t)
    risky<T>(as_t<T>("x"), true)
}

function throws_list(t: reflect.Type) -> string throws unknown {
    type T = unreflect(t)
    let v: T = as_t<T>(1)
    let a: T[] = [v]
    throw a
}

test "a declared throws unknown covers a scoped thrown type" {
    let string_t = reflect.Type.of<string>()
    let int_t = reflect.Type.of<int>()
    let boomed = throws_boom(string_t) catch (e) { _ => e }
    let listed = throws_list(int_t) catch (e) { _ => e }
    assert.is_true(reflect.Type.of_value(boomed) == reflect.Type.of<Boom<string>>())
    assert.is_true(reflect.Type.of_value(listed) == reflect.Type.of<int[]>())
}

// What leaves a block. A value leaves only through a type that does not
// mention the binding (`-> unknown`, an `unknown` annotation), and a thrown
// type only through a catch inside the block or a clause that names its
// nearest relaxation.
function leaves_as_unknown(t: reflect.Type) -> unknown {
    type T = unreflect(t)
    as_t<T>(1)
}

function published_through_an_annotation(t: reflect.Type) -> reflect.Type {
    let boxed: unknown = {
        type T = unreflect(t)
        as_t<T>("a")
    }
    reflect.Type.of_value(boxed)
}

function caught_inside(t: reflect.Type) -> int throws never {
    type T = unreflect(t)
    risky<T>(as_t<T>("x"), true) catch (e) {
        Boom<T> => 1,
        string => 2,
    }
}

// A binding's narrowing ends with its block: `v` reads as its declared
// `unknown` again afterwards.
function narrowing_ends_with_the_block(t: reflect.Type, v: unknown) -> bool {
    {
        type T = unreflect(t)
        if (v is T) {
            let narrowed: T = v
            let _ = narrowed
        }
    }
    v is string
}

// A `return` from inside the binding's scope leaves through the frame's
// declared return type, exactly as a tail value leaves through its context.
function returns_through_the_declared_type(t: reflect.Type, early: bool) -> unknown {
    type T = unreflect(t)
    if (early) { return as_t<T>(1) }
    as_t<T>(2)
}

// An annotated `unknown[]` gives the list literal its element type before
// the block closes, so the `T`-typed element is stored as an `unknown`.
function collected_as_unknown_list(t: reflect.Type) -> unknown[] {
    let vs: unknown[] = {
        type T = unreflect(t);
        [ident(as_t<T>(42))]
    }
    vs
}

// A binding's frame slot is keyed by the parameter's identity, never by its
// name, so even the compiler's own effect-parameter spelling is just a name.
function binding_named_like_an_effect_param(t: reflect.Type) -> reflect.Type {
    type __effect_param_0 = unreflect(t)
    reflect.Type.of<__effect_param_0>()
}

// An `unknown` is not a type operand: narrow it to `reflect.Type` with
// `match` or `is` first, and the non-type case is handled where it is
// decided, never erased into the frame.
function bind_narrowed(x: unknown) -> string {
    match (x) {
        let t: reflect.Type => {
            type T = unreflect(t)
            reflect.Type.of<T>().to_string()
        },
        _ => "not a type",
    }
}

test "an operand is narrowed to a type before it binds" {
    let erased: unknown = reflect.Type.of<int>()
    assert.equal(bind_narrowed(erased), "int")
    assert.equal(bind_narrowed(1), "not a type")
}

// A scoped binding is a frame slot keyed by its own identity. It never
// stands in for a same-named associated type, which is not a slot at all and
// reduces as a `Self.X` projection over the implementor.
interface KeyedStore {
    type Key;
    function probe(self) -> string throws never {
        type Key = unreflect(reflect.Type.of<int>())
        reflect.Type.of<Key>().to_string() + "/" + reflect.Type.of<Self.Key>().to_string()
    }
}

class Shelf {
    implements KeyedStore {
        type Key = string;
    }
}

test "a scoped binding does not shadow a same-named associated type" {
    assert.equal(Shelf {}.probe(), "int/string")
}

test "values and effects leave a block only through binding-free types" {
    let int_t = reflect.Type.of<int>()
    let string_t = reflect.Type.of<string>()
    assert.is_true(reflect.Type.of_value(leaves_as_unknown(int_t)) == int_t)
    assert.is_true(published_through_an_annotation(string_t) == string_t)
    assert.equal(caught_inside(string_t), 1)
    assert.is_true(narrowing_ends_with_the_block(string_t, "s"))
    assert.is_true(reflect.Type.of_value(returns_through_the_declared_type(int_t, true)) == int_t)
    assert.is_true(reflect.Type.of_value(returns_through_the_declared_type(int_t, false)) == int_t)
    let collected = collected_as_unknown_list(int_t)
    assert.is_true(reflect.Type.of_value(collected) == reflect.Type.of<unknown[]>())
    assert.is_true(reflect.Type.of_value(collected[0]) == int_t)
    assert.is_true(binding_named_like_an_effect_param(int_t) == int_t)
}

// Sibling blocks reuse one frame slot for their bindings. A descriptor
// computed under the first binding keeps that binding wherever it is
// consumed, even after the second block has rebound the slot and even when
// the consumer sits on a later branch.
function sibling_bindings(a: reflect.Type, b: reflect.Type, both: bool) -> reflect.Type[] {
    let first: reflect.Type = {
        type T = unreflect(a)
        reflect.Type.of<Wrapper<T>>()
    }
    let second: reflect.Type = {
        type U = unreflect(b)
        type V = Wrapper<U>
        reflect.Type.of<V[]>()
    }
    if (both) {
        [first, second]
    } else {
        [second]
    }
}

test "a descriptor keeps its own binding past a sibling rebinding" {
    let int_t = reflect.Type.of<int>()
    let string_t = reflect.Type.of<string>()
    let pair = sibling_bindings(int_t, string_t, true)
    assert.is_true(pair[0] == reflect.Type.of<Wrapper<int>>())
    assert.is_true(pair[1] == reflect.Type.of<Wrapper<string>[]>())
    let single = sibling_bindings(int_t, string_t, false)
    assert.is_true(single[0] == reflect.Type.of<Wrapper<string>[]>())
}

// The Agent scenario: the spec slot takes the bound name, and the program
// dispatches through the inherent `Agent.run<Out>` and parses the reflected
// output type. A scripted `ai.Client` answers with a fixed payload, so the
// loop runs end to end without a network or an API key.
client DefaultClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

class ProbeClient {
    reply: string,

    implements ai.Client {
        function id(self) -> string {
            "probe"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://probe.invalid", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                calls: [],
                content: [ai.content.Text { text: self.reply }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }
}

function DynamicOutput<T>() -> T {
    client: DefaultClient
    prompt: `${ctx.output_format()}`
}

test "an agent spec takes a scoped runtime type" {
    let output_type = reflect.class.new("RuntimeOutput", {
        "name": reflect.Type.of<string>(),
    }).as_type()
    type Out = unreflect(output_type)
    let run = ai.Agent.new(
        client = ProbeClient { reply: `{"name":"Pixel"}` },
    ).run(DynamicOutput@spec<Out>())
    assert.equal(reflect.class.get_field<string>(run.value, "name"), "Pixel")
}
