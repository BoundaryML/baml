// End-to-end tests: an optional-chained method call (`x?.m<T>()`) seeds the
// callee frame's type args exactly like the plain `x.m<T>()` spelling.
//
// `?.` used to lower a method call as a bound-method *value* invoked
// indirectly, and the indirect call carries no type-arg count — so every
// generic method reached through `?.` started with an empty (or class-args
// only) frame and died on the first `T` use. Each shape below is paired with
// the non-optional spelling it must agree with.
//
// Test bodies live in top-level helper functions: locals holding instances
// misbehave inside `test` blocks (see class_type_args_at_runtime.baml).

class Holder {
    raw string

    function value<T>(self) -> string {
        type.of<T>().to_string()
    }

    /// Type arg inferred from the argument, never written at the call site.
    function ident<T>(self, v: T) -> string {
        type.of<T>().to_string()
    }

    function inner(self) -> Holder? {
        Holder { raw: self.raw }
    }

    function plain(self) -> string {
        self.raw
    }
}

class Boxed<T> {
    v T

    /// Reads only the class-level arg — the bound-method path already curried
    /// these, so this is the "was already working" control.
    function describe(self) -> string {
        type.of<T>().to_string()
    }

    /// Reads class arg then method arg: fails unless the frame is seeded
    /// `[class T, method U]` in that order.
    function both<U>(self) -> string {
        type.of<T>().to_string() + "/" + type.of<U>().to_string()
    }
}

interface Speaker {
    function speak<T>(self) -> string
}

class Dog {
    name string

    implements Speaker {
        function speak<T>(self) -> string {
            type.of<T>().to_string()
        }
    }
}

class Point {
    x int
    label string
}

function holder(present: bool) -> Holder | null {
    if (present) {
        return Holder { raw: "seven" }
    }
    null
}

function boxed(present: bool) -> Boxed<int> | null {
    if (present) {
        return Boxed<int> { v: 1 }
    }
    null
}

function speaker(present: bool) -> Speaker | null {
    if (present) {
        return Dog { name: "rex" }
    }
    null
}

function dog(present: bool) -> Dog | null {
    if (present) {
        return Dog { name: "rex" }
    }
    null
}

// ─── Explicit method type args ────────────────────────────────────────────────

function explicit_type_arg_fn() -> string? {
    holder(true)?.value<int>()
}

function explicit_type_arg_let_else_fn() -> string {
    let h: Holder = holder(true) else { return "none" }
    h.value<int>()
}

test "explicit_type_arg_matches_let_else" {
    assert.equal(explicit_type_arg_fn(), "int")
    assert.equal(explicit_type_arg_let_else_fn(), "int")
}

function explicit_type_arg_null_receiver_fn() -> string? {
    holder(false)?.value<int>()
}

test "explicit_type_arg_null_receiver_short_circuits" {
    assert.equal(explicit_type_arg_null_receiver_fn(), null)
}

// ─── Inferred method type args ────────────────────────────────────────────────

function inferred_type_arg_fn() -> string? {
    holder(true)?.ident("hi")
}

test "inferred_type_arg" {
    assert.equal(inferred_type_arg_fn(), "string")
}

// ─── Class-level args, alone and with method args ─────────────────────────────

function class_type_arg_fn() -> string? {
    boxed(true)?.describe()
}

test "class_type_arg_through_optional_chain" {
    assert.equal(class_type_arg_fn(), "int")
}

function class_and_method_type_args_fn() -> string? {
    boxed(true)?.both<string>()
}

test "class_and_method_type_args_keep_their_slots" {
    assert.equal(class_and_method_type_args_fn(), "int/string")
}

// ─── Interface dispatch through `?.` ──────────────────────────────────────────

function interface_receiver_fn() -> string? {
    speaker(true)?.speak<int>()
}

function concrete_receiver_interface_method_fn() -> string? {
    dog(true)?.speak<string>()
}

test "interface_method_type_args_through_optional_chain" {
    assert.equal(interface_receiver_fn(), "int")
    assert.equal(concrete_receiver_interface_method_fn(), "string")
}

// ─── Chained `a?.b()?.c<T>()` ─────────────────────────────────────────────────

function chained_fn() -> string? {
    holder(true)?.inner()?.value<int>()
}

function chained_null_fn() -> string? {
    holder(false)?.inner()?.value<int>()
}

test "chained_optional_calls_thread_type_args" {
    assert.equal(chained_fn(), "int")
    assert.equal(chained_null_fn(), null)
}

// ─── Negative control: non-generic `?.` calls are unchanged ───────────────────

function non_generic_call_fn() -> string? {
    holder(true)?.plain()
}

function non_generic_call_null_fn() -> string? {
    holder(false)?.plain()
}

test "non_generic_optional_calls_unchanged" {
    assert.equal(non_generic_call_fn(), "seven")
    assert.equal(non_generic_call_null_fn(), null)
}

// ─── The reflection shape the bug was found on ────────────────────────────────

function reflected_field_read_fn() -> int? throws baml.errors.TypeMismatch {
    let value: baml.AnyClass = Point { x: 7, label: "a" } else { return null }
    value.get_field("x")?.read<int>()
}

function reflected_missing_field_fn() -> int? throws baml.errors.TypeMismatch {
    let value: baml.AnyClass = Point { x: 7, label: "a" } else { return null }
    value.get_field("absent")?.read<int>()
}

test "reflected_field_read_through_optional_chain" {
    assert.equal(reflected_field_read_fn(), 7)
    assert.equal(reflected_missing_field_fn(), null)
}

/// A type arg the value does not fit is still a domain error
/// (`baml.errors.TypeMismatch`), not a frame-layout failure.
function reflected_field_wrong_type_arg_fn() -> string {
    let value: baml.AnyClass = Point { x: 7, label: "a" } else { return "no-class" }
    let read = value.get_field("x")?.read<string>() catch (e) {
        baml.errors.TypeMismatch { message } => { return "type-mismatch" }
    }
    read ?? "null"
}

test "reflected_field_wrong_type_arg_is_a_type_mismatch" {
    assert.equal(reflected_field_wrong_type_arg_fn(), "type-mismatch")
}
