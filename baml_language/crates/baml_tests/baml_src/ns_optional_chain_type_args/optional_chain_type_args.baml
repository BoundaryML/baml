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
        reflect.Type.of<T>().to_string()
    }

    /// Type arg inferred from the argument, never written at the call site.
    function ident<T>(self, v: T) -> string {
        reflect.Type.of<T>().to_string()
    }

    function inner(self) -> Holder? {
        Holder { raw: self.raw }
    }

    /// The second stage of `a?.b()?.c<T>()` short-circuiting on its own.
    function inner_missing(self) -> Holder? {
        null
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
        reflect.Type.of<T>().to_string()
    }

    /// Reads class arg then method arg: fails unless the frame is seeded
    /// `[class T, method U]` in that order.
    function both<U>(self) -> string {
        reflect.Type.of<T>().to_string() + "/" + reflect.Type.of<U>().to_string()
    }
}

interface Speaker {
    function speak<T>(self) -> string throws never
}

class Dog {
    name string

    implements Speaker {
        function speak<T>(self) -> string {
            reflect.Type.of<T>().to_string()
        }
    }
}

class Cat {
    name string

    implements Speaker {
        function speak<T>(self) -> string {
            "cat:" + reflect.Type.of<T>().to_string()
        }
    }
}

/// Same class, two specializations: a union receiver whose arms differ only in
/// their class type args, reached through `?.`.
interface Tagged {
    function tag<U>(self) -> string throws never
}

class Pair<T> {
    v T

    implements Tagged {
        function tag<U>(self) -> string {
            reflect.Type.of<T>().to_string() + "/" + reflect.Type.of<U>().to_string()
        }
    }
}

class Point {
    x int
    label string
}

/// Receiver evaluations are observable, so pin them: `?.` lowers its receiver
/// twice (once for the null test, once for the call itself). A regression that
/// re-lowered the original `x?.m` node *underneath* the guard made it three,
/// and the third evaluation reading a member off a freshly-null receiver died
/// with `expected instance, got any`.
class EvalCounter {
    n int
}

class Callables {
    /// A callable-valued *field*, not a method: the member resolves to a field,
    /// so the call declines every direct-dispatch path and lowers the callee as
    /// an opaque callable value — the shape that re-lowered the receiver.
    describe (v: int) -> string throws never

    function method(self, v: int) -> string {
        "method:" + v.to_string()
    }
}

function counted(c: EvalCounter) -> Callables? {
    c.n = c.n + 1
    Callables { describe: (v: int) -> string { "field:" + v.to_string() } }
}

/// Non-null for the first two evaluations, null afterwards.
function counted_null_after_two(c: EvalCounter) -> Callables? {
    c.n = c.n + 1
    if (c.n <= 2) {
        return Callables { describe: (v: int) -> string { "field:" + v.to_string() } }
    }
    null
}

/// Unique per run, so the sys-op tests stay safe under the parallel runner and
/// safe to re-run (matching `ns_fs`).
function optional_chain_tmp_path(name: string) -> string {
    "/tmp/baml_optional_chain_" + name + "_"
        + baml.time.Instant.now().to_timestamp_milliseconds().to_string()
        + "_" + int.random(0, 1000000000).to_string()
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

/// A union of two *classes*, not an interface value: `?.` reaches the
/// runtime-class dispatcher for the first time.
function animal(which: int) -> Dog | Cat | null {
    if (which == 0) {
        return Dog { name: "rex" }
    }
    if (which == 1) {
        return Cat { name: "tom" }
    }
    null
}

function pair(which: int) -> Pair<int> | Pair<string> | null {
    if (which == 0) {
        return Pair<int> { v: 1 }
    }
    if (which == 1) {
        return Pair<string> { v: "a" }
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

// ─── Union-typed receivers through `?.` ───────────────────────────────────────
//
// A union receiver dispatches on the runtime class. `?.` could not reach that
// dispatcher at all before this fix (the bound-method value swallowed it), so
// these shapes are newly exercised rather than newly fixed.
//
// These are the corpus's first union receivers calling a shared-interface
// method, so they are also the first entries in the `s15_sweep_baml_src`
// hir_ty error channel: hir_ty records the receiver as `Cat | Dog` where the
// method's `self` is `Speaker`. That is pre-existing imprecision, not
// something `?.` introduces — the plain `a.speak<int>()` spelling on a
// `Dog | Cat` parameter records the identical entry. Dispatch, type args and
// runtime results are all correct.

function union_receiver_fn(which: int) -> string? {
    animal(which)?.speak<int>()
}

test "union_receiver_dispatches_and_keeps_its_type_args" {
    assert.equal(union_receiver_fn(0), "int")
    assert.equal(union_receiver_fn(1), "cat:int")
    assert.equal(union_receiver_fn(2), null)
}

/// The union arms differ only in their class type args, so the frame must be
/// seeded from the arm that actually ran.
function union_class_type_args_fn(which: int) -> string? {
    pair(which)?.tag<bool>()
}

test "union_receiver_seeds_class_type_args_per_arm" {
    assert.equal(union_class_type_args_fn(0), "int/bool")
    assert.equal(union_class_type_args_fn(1), "string/bool")
    assert.equal(union_class_type_args_fn(2), null)
}

// ─── Chained `a?.b()?.c<T>()` ─────────────────────────────────────────────────

function chained_fn() -> string? {
    holder(true)?.inner()?.value<int>()
}

function chained_null_fn() -> string? {
    holder(false)?.inner()?.value<int>()
}

/// Null arises at the *second* stage: the first `?.` runs its call, and the
/// second one short-circuits on that call's null result.
function chained_null_second_stage_fn() -> string? {
    holder(true)?.inner_missing()?.value<int>()
}

test "chained_optional_calls_thread_type_args" {
    assert.equal(chained_fn(), "int")
    assert.equal(chained_null_fn(), null)
    assert.equal(chained_null_second_stage_fn(), null)
}

// ─── Sys-ops through `?.` ─────────────────────────────────────────────────────
//
// A `$rust_io_function` method lowers to an inline `sys_op`, not a call, and
// the op is picked by inspecting the callee expression — which `?.` reshapes.
// Reading the optional spelling as an ordinary call left a body-less builtin
// invoked through the generic call path, which also skips the omitted-default
// materialization that only sys-op callees get. The *opcode* is pinned by this
// namespace's bytecode snapshot (`sys_op baml.fs.File.text`); this pair pins
// the behavior.

function optional_file(path: string) -> baml.fs.File? {
    baml.fs.open(path, "r")
}

function sys_op_optional_receiver_fn() -> string {
    let path = optional_chain_tmp_path("sysop_optional")
    baml.fs.write(path, "sys-op text")
    let text = optional_file(path)?.text()
    baml.fs.remove(path)
    text ?? "null"
}

function sys_op_plain_receiver_fn() -> string {
    let path = optional_chain_tmp_path("sysop_plain")
    baml.fs.write(path, "sys-op text")
    let file = baml.fs.open(path, "r")
    let text = file.text()
    baml.fs.remove(path)
    text
}

test "sys_op_method_through_optional_chain_matches_plain_call" {
    assert.equal(sys_op_plain_receiver_fn(), "sys-op text")
    assert.equal(sys_op_optional_receiver_fn(), "sys-op text")
}

// ─── Receiver evaluation count ────────────────────────────────────────────────

function callable_field_eval_count_fn() -> string {
    let c = EvalCounter { n: 0 }
    let out = counted(c)?.describe(1)
    `${out ?? "null"}/${c.n}`
}

function method_eval_count_fn() -> string {
    let c = EvalCounter { n: 0 }
    let out = counted(c)?.method(1)
    `${out ?? "null"}/${c.n}`
}

/// The regression's third receiver evaluation read a member off the null this
/// receiver starts returning, aborting with `expected instance, got any`.
function callable_field_null_after_two_fn() -> string {
    let c = EvalCounter { n: 0 }
    let out = counted_null_after_two(c)?.describe(1)
    `${out ?? "null"}/${c.n}`
}

test "optional_chain_evaluates_its_receiver_exactly_twice" {
    assert.equal(callable_field_eval_count_fn(), "field:1/2")
    assert.equal(method_eval_count_fn(), "method:1/2")
    assert.equal(callable_field_null_after_two_fn(), "field:1/2")
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

function reflected_field_read_fn() -> int? throws reflect.errors.TypeMismatch {
    let value: reflect.AnyClass = Point { x: 7, label: "a" } else { return null }
    value.get_field("x")?.value<int>()
}

function reflected_missing_field_fn() -> int? throws reflect.errors.TypeMismatch {
    let value: reflect.AnyClass = Point { x: 7, label: "a" } else { return null }
    value.get_field("absent")?.value<int>()
}

test "reflected_field_read_through_optional_chain" {
    assert.equal(reflected_field_read_fn(), 7)
    assert.equal(reflected_missing_field_fn(), null)
}

/// A type arg the value does not fit is still a domain error
/// (`reflect.errors.TypeMismatch`), not a frame-layout failure.
function reflected_field_wrong_type_arg_fn() -> string {
    let value: reflect.AnyClass = Point { x: 7, label: "a" } else { return "no-class" }
    let read = value.get_field("x")?.value<string>() catch (e) {
        reflect.errors.TypeMismatch { message } => { return "type-mismatch" }
    }
    read ?? "null"
}

test "reflected_field_wrong_type_arg_is_a_type_mismatch" {
    assert.equal(reflected_field_wrong_type_arg_fn(), "type-mismatch")
}
