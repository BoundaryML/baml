// End-to-end fixture for the host-callable round trip: BAML receives a
// callable typed `(int) -> string` and invokes it from inside another
// BAML function.

function call_with_callback(callback: (int) -> string, x: int) -> string {
    callback(x)
}

// ─── Callback-throws inference (typed-language only) ────────────────────────
//
// A callback with an omitted `throws` gets an inferred effect param `E`. What
// the *outer* function throws then depends on the body — cases python/TS erase
// but Rust surfaces as real, distinct `Error<…>` types. (The callback's return
// type must be concrete — the engine validates the host's returned value
// against it — so these use `() -> int`, not `() -> void`.)

// The callback's error is caught and replaced, so the outer function throws
// nothing (`throws never`): `Error<Infallible>`, with the effect param used
// only in the callback bound.
function callback_error_caught(cb: () -> int) -> int {
    cb() catch (e) {
        _ => 0
    }
}

// The callback's error is caught and a different value is thrown; the outer
// function throws `string | int` — a union that does NOT include the effect
// param.
function callback_error_replaced(cb: () -> int) -> int {
    cb() catch (e) {
        _ => {
            throw "oops"
        }
    }
    throw 1
}

// The callback's error is re-thrown, so the effect param escapes: the outer
// function throws `int | E` — a generic union that DOES carry the effect
// param.
function callback_error_rethrown(cb: () -> int) -> int {
    cb() catch (e) {
        _ => {
            throw e
        }
    }
    throw 1
}

// Callback taking multiple args, exercising arg-tuple decoding on the
// Python dispatch side.
function call_with_two_args(callback: (int, string) -> string, x: int, prefix: string) -> string {
    callback(x, prefix)
}

// Callback returning an int — covers the result-encode path with a
// non-string return type.
function call_int_callback(callback: (int) -> int, x: int) -> int {
    callback(x)
}

// A user-defined class flows through the callback as the argument type;
// exercises class-encoding on the engine→host direction and class-
// decoding on the host→engine return value.
class Person {
    name string
    age int
}

function call_with_class_callback(callback: (Person) -> string, p: Person) -> string {
    callback(p)
}

function call_class_roundtrip_callback(callback: (Person) -> Person, p: Person) -> Person {
    callback(p)
}

enum CallbackMood {
    HAPPY
    SAD
}

function call_enum_roundtrip_callback(callback: (CallbackMood) -> CallbackMood, mood: CallbackMood) -> CallbackMood {
    callback(mood)
}

function call_list_roundtrip_callback(callback: (int[]) -> int[], values: int[]) -> int[] {
    callback(values)
}

function call_map_roundtrip_callback(callback: (map<string, int>) -> map<string, int>, values: map<string, int>) -> map<string, int> {
    callback(values)
}

function call_union_roundtrip_callback(callback: (string | int) -> string | int, value: string | int) -> string | int {
    callback(value)
}

// Closed unions retain their selected-arm metadata across the host-callable
// boundary. These cover unions nested in containers, nullable unions, optional
// named callback arguments, nominal arms, literal arms, and overlapping empty
// container shapes.
function call_nullable_union_roundtrip_callback(
    callback: (string | int | null) -> string | int | null,
    value: string | int | null,
) -> string | int | null {
    callback(value)
}

function call_union_list_roundtrip_callback(
    callback: ((string | int)[]) -> (string | int)[],
    values: (string | int)[],
) -> (string | int)[] {
    callback(values)
}

function call_union_map_roundtrip_callback(
    callback: (map<string, string | int>) -> map<string, string | int>,
    values: map<string, string | int>,
) -> map<string, string | int> {
    callback(values)
}

function call_nominal_union_roundtrip_callback(
    callback: (Person | CallbackMood | string) -> Person | CallbackMood | string,
    value: Person | CallbackMood | string,
) -> Person | CallbackMood | string {
    callback(value)
}

function call_literal_union_roundtrip_callback(
    callback: ("first" | "second" | int) -> "first" | "second" | int,
    value: "first" | "second" | int,
) -> "first" | "second" | int {
    callback(value)
}

function call_overlapping_container_union_callback(
    callback: (int[] | string[]) -> int[] | string[],
    value: int[] | string[],
) -> int[] | string[] {
    callback(value)
}

function call_callback_with_optional_union_states(
    callback: (value?: string | int | null) -> int,
) -> int[] {
    [
        callback(),
        callback(value = null),
        callback(value = "supplied"),
        callback(value = 7),
    ]
}

function call_media_roundtrip_callback(callback: (image) -> image, value: image) -> image {
    callback(value)
}

function call_void_callback(callback: (int) -> void, value: int) -> int {
    callback(value)
    value
}

function call_throwing_void_callback(
    callback: (int) -> void throws baml.errors.HostCallable,
    value: int,
) -> void throws baml.errors.HostCallable {
    callback(value)
}

function propagate_throwing_callback(
    callback: (int) -> string throws baml.errors.HostCallable,
    value: int,
) -> string throws baml.errors.HostCallable {
    callback(value)
}

function ignore_callback(callback: (int) -> string) -> string {
    "ignored"
}

// ─── BAML closures returned to the host ─────────────────────────────────────

function make_adder(offset: int) -> (value: int) -> int throws never {
    // Required callable parameter names are type-insignificant. The returned
    // implementation deliberately uses a different name so every SDK exercises
    // positional binding through the handle-dispatch path.
    return (x: int) -> int { offset + x }
}

function make_pair_builder(base: int) -> (delta: int, label: string) -> Person throws never {
    return (delta: int, label: string) -> Person {
        Person {
            name: label,
            age: base + delta,
        }
    }
}

function make_counter(start: int) -> () -> int throws never {
    let current = start;
    return () -> int {
        current += 1;
        current
    }
}

// ─── Optional args × host callables (the combination) ───────────────────────
//
// These exercise the intersection of two surfaces that were each covered
// in isolation but never together: a host callable whose *own* type
// carries optional parameters (`y?: int, z?: string`). Defaults are not
// permitted inside a callable type — only the `?` optional marker is — so the
// host's own language-level default is the only source of a value when BAML
// omits the arg. Every case below uses the same two-optional callback shape so
// the per-bridge `$opts` reshape is exercised with more than one optional in
// every scenario (supplied, int-return, omitted, and second-only).
function call_callback_with_optional_args_all_unset(callback: (x: int, y?: int, z?: int) -> int, x: int) -> int[] {
    [callback(x)]
}

function call_callback_with_optional_args_partially_set(callback: (x: int, y?: int, z?: int) -> int, x: int) -> int[] {
    [callback(x, y = 2), callback(x, z = 3)]
}

function call_callback_with_optional_args_all_set(callback: (x: int, y?: int, z?: int) -> int, x: int) -> int[] {
    [callback(x, y = 2, z = 3)]
}

// An optional parameter whose value is itself nullable has three distinct
// states at the host boundary: omitted, supplied null, and supplied value.
function call_callback_with_nullable_optional_states(callback: (x: int, value?: int?) -> int, x: int) -> int[] {
    [callback(x), callback(x, value = null), callback(x, value = 7)]
}

// Names resembling generator locals and names that normalize to the same Go
// spelling must retain their exact wire identity in the generated adapter.
function call_callback_with_optional_name_edges(
    callback: (
        ctx?: int,
        err?: int,
        result?: int,
        zero?: int,
        bootstrap?: int,
        init?: int,
        main?: int,
        foo_bar?: int,
        fooBar?: int,
    ) -> int,
) -> int {
    callback(
        ctx = 1,
        err = 2,
        result = 3,
        zero = 4,
        bootstrap = 5,
        init = 6,
        main = 7,
        foo_bar = 8,
        fooBar = 9,
    )
}

// Loop that invokes the callback N times — exercises N round-trips
// through SysOp::BamlHostCallHostValue.
function call_repeatedly(callback: (int) -> string, n: int) -> string[] {
    let result: string[] = [];
    for (let i = 0; i < n; i += 1) {
        result.push(callback(i));
    }
    result
}

// Catch a host-callable throw inside BAML code. The callback's declared
// `throws root.errors.HostCallable` tells the type checker that the
// call site can raise that error class, allowing the `catch` clause to
// match it.
function call_with_throwing(callback: (int) -> string throws baml.errors.HostCallable, x: int) -> string {
    callback(x) catch (e) {
        _ => "caught:" + e.class_name
    }
}

function call_with_throwing_propagating(
    callback: (int) -> string throws baml.errors.HostCallable,
    x: int,
) -> string throws baml.errors.HostCallable {
    callback(x)
}

function call_returned_callback(factory: () -> ((int) -> string throws never) throws never, x: int) -> string {
    let callback = factory()
    callback(x)
}

function call_returned_callback_in_list(
    factory: () -> (((int) -> string throws never)[]) throws never,
    x: int,
) -> string {
    let callbacks = factory()
    callbacks[0](x)
}

// User-defined error class returned from a host callable. The Python side
// raises `BamlError(value=ValidationError(...))`; the bridge unwraps the
// inner pydantic value and emits it as its real BAML class so the catch
// matches on the user's declared throws and `e.code` / `e.message` /
// `e.fields` resolve to typed fields (not opaque metadata strings).
class ValidationError {
    code int
    message string
    fields string[]
}

function call_with_typed_throws(
    callback: (int) -> string throws ValidationError,
    x: int,
) -> string {
    callback(x) catch (e) {
        // Typed catch arm: only matches when the thrown value's
        // concrete BAML class is `ValidationError`, proving the
        // host-thrown pydantic model arrived with its real class
        // identity (not as an opaque `HostCallable` metadata blob).
        // Surface `e.message` so the test can verify the typed
        // catch actually fired and read a real BAML class field.
        ValidationError => "caught: " + e.message
    }
}

// Same as `call_with_typed_throws` but never catches — exercises the
// propagation path where the host-thrown codegenned class flows back
// out to the Python caller unhandled, surfacing as a
// `BamlError(value=ValidationError(...))` pydantic instance with its
// typed fields preserved end-to-end.
function call_with_typed_throws_propagating(
    callback: (int) -> string throws ValidationError,
    x: int,
) -> string throws ValidationError {
    callback(x)
}
