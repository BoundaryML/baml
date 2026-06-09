// End-to-end fixture for the host-callable round trip: BAML receives a
// callable typed `(int) -> string` and invokes it from inside another
// BAML function.

function call_with_callback(callback: (int) -> string, x: int) -> string {
    callback(x)
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
