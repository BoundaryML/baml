// `obj.to_string()` sugar: a 0-arg `to_string` call on a value whose type has no
// real `to_string` method lowers to `string.from(obj)`, so any value can be
// rendered with `.to_string()` regardless of whether its type implements
// `baml.ToString`. A real `to_string` (from any interface impl) is dispatched
// normally — never hijacked.
//
// Tests using locals (let bindings, lambdas) go through top-level helper functions
// to work around the test-block local-boxing VM bug. Class names differ from
// expected strings where needed: `PointShow` (ToString impl) renders without a
// class name "(1, 2)", while `Point` (plain structural) renders with its name
// "Point { x: 1, y: 2 }".
//
// Compile-error diagnostics assertions (like receiver_error_is_not_swallowed)
// are not expressible as executing BAML tests and are not included here.

// ─── shared class/interface definitions ───────────────────────────────────────

// Plain class: no ToString impl, renders structurally.
class Point {
    x int
    y int
}

// Override-bearing class: has a custom ToString impl.
class PointShow {
    x int
    y int
    implements baml.ToString {
        function to_string(self) -> string throws never {
            "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
        }
    }
}

class Line {
    start PointShow
    end PointShow
}

// ─── primitives, strings, arrays ──────────────────────────────────────────────

test "primitive_to_string" {
    assert.equal((5).to_string(), "5")
    assert.equal(true.to_string(), "true")
}

test "string_to_string_is_bare" {
    assert.equal("hi".to_string(), "hi")
}

test "array_to_string" {
    assert.equal([1, 2, 3].to_string(), "[1, 2, 3]")
}

// ─── structural rendering and overrides ───────────────────────────────────────

function plain_class_to_string_fn() -> string {
    let p = Point { x: 1, y: 2 };
    p.to_string()
}

test "plain_class_to_string_is_structural" {
    assert.equal(plain_class_to_string_fn(), "Point { x: 1, y: 2 }")
}

function implementor_to_string_fn() -> string {
    let p = PointShow { x: 1, y: 2 };
    p.to_string()
}

test "implementor_to_string_uses_override" {
    assert.equal(implementor_to_string_fn(), "(1, 2)")
}

// `.to_string()` on a container routes through `string.from`, so nested
// overrides are honored.
test "nested_override_honored_via_sugar" {
    assert.equal(
        [PointShow { x: 1, y: 2 }, PointShow { x: 3, y: 4 }].to_string(),
        "[(1, 2), (3, 4)]"
    )
}

// Parity with `string.from`: `.to_string()` on a non-implementor whose FIELDS
// are override-bearing honors those overrides at depth (same walker as
// `string.from`, since the sugar lowers to it).
function nested_override_in_field_fn() -> string {
    let l = Line { start: PointShow { x: 1, y: 2 }, end: PointShow { x: 3, y: 4 } };
    l.to_string()
}

test "nested_override_in_field_honored_via_sugar" {
    assert.equal(nested_override_in_field_fn(), "Line { start: (1, 2), end: (3, 4) }")
}

// `x.to_string()` and `string.from(x)` must produce identical output — the
// sugar respects overrides in exactly the same way.
function sugar_match_line() -> Line {
    Line { start: PointShow { x: 1, y: 2 }, end: PointShow { x: 3, y: 4 } }
}

function sugar_match_via_sugar() -> string {
    let l = sugar_match_line();
    l.to_string()
}

function sugar_match_via_from() -> string {
    let l = sugar_match_line();
    string.from(l)
}

test "sugar_matches_string_from_exactly" {
    assert.equal(sugar_match_via_sugar(), sugar_match_via_from())
}

// ─── receivers: chained calls, generics, optionals, captures ──────────────────

// `expr.to_string()` where the receiver is itself a call expression.
test "chained_method_result_to_string" {
    assert.equal(baml.time.Instant.epoch().to_string(), "1970-01-01T00:00:00Z")
}

// `x.to_string()` on an unbounded type variable falls back to `string.from`,
// which dispatches on the runtime class.
function tostring_show<T>(x: T) -> string { x.to_string() }

function generic_receiver_caller() -> string { tostring_show<int>(42) }

test "generic_receiver_to_string" {
    assert.equal(generic_receiver_caller(), "42")
}

// A `to_string` from a *user* interface (not `baml.ToString`) must dispatch to
// that impl, not be replaced by `string.from`. This is the correctness win of
// the operator-style (resolution-aware) lowering over a blind AST rewrite.
interface MyShow {
    function to_string(self) -> string throws never
}

class Widget {
    id int
    implements MyShow {
        function to_string(self) -> string throws never { "widget!" }
    }
}

function widget_to_string_fn() -> string {
    let w = Widget { id: 1 };
    w.to_string()
}

test "user_interface_to_string_is_not_hijacked" {
    assert.equal(widget_to_string_fn(), "widget!")
}

// Same generic fallback, inside a `throws never` function.
function tostring_show_never<T>(x: T) -> string throws never { x.to_string() }

function zzz_throws_never2_caller() -> string { tostring_show_never<int>(42) }

test "zzz_throws_never2" {
    assert.equal(zzz_throws_never2_caller(), "42")
}

// `.to_string()` works on an optional receiver: a present value renders
// normally, and `null` renders as "null" (string.from is total over null).
function optional_present_to_string_fn() -> string {
    let x: int? = 5;
    x.to_string()
}

function optional_null_to_string_fn() -> string {
    let x: int? = null;
    x.to_string()
}

test "optional_receiver_to_string" {
    assert.equal(optional_present_to_string_fn(), "5")
    assert.equal(optional_null_to_string_fn(), "null")
}

// `.to_string()` on a *captured* (closure) root must lower through the
// fallback too — the MIR hook handles captured roots via Place::Capture,
// matching how TIR/throws treat them (regression: previously ICE'd).
function captured_root_to_string_fn() -> string {
    let p = Point { x: 1, y: 2 };
    let f = () -> string { p.to_string() };
    f()
}

test "captured_root_to_string" {
    assert.equal(captured_root_to_string_fn(), "Point { x: 1, y: 2 }")
}
